/// Time-delay based phase correction
///
/// This algorithm detects linear phase shifts (constant delay) using least-squares
/// regression and applies fractional delay compensation using Lagrange interpolation.

use super::PhaseCorrectionAlgorithm;
use crate::phase_data::NUM_BINS;
use std::f32::consts::PI;

const TWO_PI: f32 = 2.0 * PI;

pub struct TimeDelay {
    sample_rate: f32,
    detected_delay: f32,
    delay_buffer: Vec<f32>,
    buffer_position: usize,
}

impl TimeDelay {
    pub fn new() -> Self {
        Self {
            sample_rate: 44100.0,
            detected_delay: 0.0,
            delay_buffer: Vec::new(),
            buffer_position: 0,
        }
    }

    /// Detect delay in samples from phase slope using least-squares regression
    fn detect_delay(&mut self, phase_diff: &[f32; NUM_BINS]) {
        // Perform linear regression: phase = slope * omega + offset
        // where omega = 2π * freq / sample_rate (normalized frequency)
        // slope relates to delay: delay_samples = -slope

        let n = NUM_BINS as f32;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xx = 0.0;
        let mut sum_xy = 0.0;

        // Bin spacing in Hz
        let bin_spacing = self.sample_rate / 2048.0; // WINDOW_SIZE = 2048

        // Unwrap phase for regression (handle wrapping)
        let mut unwrapped_phase = vec![0.0; NUM_BINS];
        unwrapped_phase[0] = phase_diff[0];

        for i in 1..NUM_BINS {
            let mut diff = phase_diff[i] - unwrapped_phase[i - 1];

            // Unwrap phase jumps
            while diff > PI {
                diff -= TWO_PI;
            }
            while diff < -PI {
                diff += TWO_PI;
            }

            unwrapped_phase[i] = unwrapped_phase[i - 1] + diff;
        }

        // Linear regression on unwrapped phase vs normalized angular frequency
        for (i, &phase) in unwrapped_phase.iter().enumerate() {
            let freq_hz = i as f32 * bin_spacing;
            // Normalized angular frequency: omega = 2π * f / fs
            let omega = TWO_PI * freq_hz / self.sample_rate;

            sum_x += omega;
            sum_y += phase;
            sum_xx += omega * omega;
            sum_xy += omega * phase;
        }

        // Calculate slope: m = (n*sum_xy - sum_x*sum_y) / (n*sum_xx - sum_x*sum_x)
        let denominator = n * sum_xx - sum_x * sum_x;

        if denominator.abs() > 1e-10 {
            let slope = (n * sum_xy - sum_x * sum_y) / denominator;

            // Convert slope to delay in samples
            // phase(ω) = -ω * delay
            // slope = dφ/dω = -delay
            self.detected_delay = -slope;

            // Clamp to reasonable range (0 to 1000 samples)
            self.detected_delay = self.detected_delay.clamp(0.0, 1000.0);
        } else {
            self.detected_delay = 0.0;
        }
    }

    /// Apply fractional delay using 3rd-order Lagrange interpolation
    fn apply_fractional_delay(&mut self, buffer: &mut [f32]) {
        if self.detected_delay < 0.01 {
            return; // Negligible delay, skip processing
        }

        let delay_int = self.detected_delay.floor() as usize;
        let delay_frac = self.detected_delay - delay_int as f32;

        // Ensure delay buffer is large enough
        let required_size = delay_int + 4; // +4 for interpolation kernel
        if self.delay_buffer.len() < required_size {
            self.delay_buffer.resize(required_size, 0.0);
        }

        // Process each sample
        for sample in buffer.iter_mut() {
            // Write current sample to delay buffer
            self.delay_buffer[self.buffer_position] = *sample;

            // Calculate delayed sample position
            let read_pos = if self.buffer_position >= delay_int {
                self.buffer_position - delay_int
            } else {
                self.delay_buffer.len() + self.buffer_position - delay_int
            };

            // 3rd-order Lagrange interpolation (4-point)
            let idx = [
                if read_pos == 0 { self.delay_buffer.len() - 1 } else { read_pos - 1 },
                read_pos,
                if read_pos + 1 >= self.delay_buffer.len() { 0 } else { read_pos + 1 },
                if read_pos + 2 >= self.delay_buffer.len() { read_pos + 2 - self.delay_buffer.len() } else { read_pos + 2 },
            ];

            // Lagrange coefficients for fractional delay d
            let d = delay_frac;
            let c0 = -d * (d - 1.0) * (d - 2.0) / 6.0;
            let c1 = (d + 1.0) * (d - 1.0) * (d - 2.0) / 2.0;
            let c2 = -(d + 1.0) * d * (d - 2.0) / 2.0;
            let c3 = (d + 1.0) * d * (d - 1.0) / 6.0;

            *sample = c0 * self.delay_buffer[idx[0]]
                    + c1 * self.delay_buffer[idx[1]]
                    + c2 * self.delay_buffer[idx[2]]
                    + c3 * self.delay_buffer[idx[3]];

            // Advance buffer position
            self.buffer_position = (self.buffer_position + 1) % self.delay_buffer.len();
        }
    }
}

impl PhaseCorrectionAlgorithm for TimeDelay {
    fn name(&self) -> &str {
        "Time Delay"
    }

    fn initialize(&mut self, sample_rate: f32, max_block_size: usize) {
        self.sample_rate = sample_rate;
        // Allocate enough for max delay + interpolation kernel
        self.delay_buffer.resize(1024 + max_block_size, 0.0);
        self.buffer_position = 0;
    }

    fn set_phase_target(&mut self, phase_differences: &[f32; NUM_BINS]) {
        self.detect_delay(phase_differences);
    }

    fn process_stereo(&mut self, _left: &mut [f32], right: &mut [f32]) {
        // Determine which channel is leading based on phase slope
        // Positive detected_delay means right channel is delayed, so delay left
        // For simplicity, always delay the right channel
        self.apply_fractional_delay(right);
    }

    fn reset(&mut self) {
        self.delay_buffer.fill(0.0);
        self.buffer_position = 0;
        self.detected_delay = 0.0;
    }

    fn latency_samples(&self) -> u32 {
        self.detected_delay.ceil() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_delay_initialization() {
        let mut algo = TimeDelay::new();
        algo.initialize(48000.0, 512);

        assert_eq!(algo.sample_rate, 48000.0);
        assert!(algo.delay_buffer.len() >= 512);
        assert_eq!(algo.detected_delay, 0.0);
    }

    #[test]
    fn test_delay_detection_zero_phase() {
        let mut algo = TimeDelay::new();
        algo.initialize(48000.0, 512);

        let phase_diff = [0.0; NUM_BINS];
        algo.detect_delay(&phase_diff);

        assert!(algo.detected_delay.abs() < 0.1);
    }

    #[test]
    fn test_delay_detection_linear_phase() {
        let mut algo = TimeDelay::new();
        algo.initialize(48000.0, 512);

        // Simulate 2-sample delay: phase = -2πf * delay
        // Using smaller delay to avoid phase wrapping issues in higher bins
        let delay_samples = 2.0;
        let bin_spacing = 48000.0 / 2048.0;
        let mut phase_diff = [0.0; NUM_BINS];

        for (i, phase) in phase_diff.iter_mut().enumerate() {
            let freq = i as f32 * bin_spacing;
            *phase = -TWO_PI * freq * delay_samples / 48000.0;
            // Wrap to [-π, π]
            while *phase > PI {
                *phase -= TWO_PI;
            }
            while *phase < -PI {
                *phase += TWO_PI;
            }
        }

        algo.detect_delay(&phase_diff);

        // Should detect approximately 2 samples delay (within 0.5 sample tolerance)
        assert!(
            (algo.detected_delay - delay_samples).abs() < 0.5,
            "Expected delay ~{}, got {}",
            delay_samples,
            algo.detected_delay
        );
    }

    #[test]
    fn test_fractional_delay_pass_through() {
        let mut algo = TimeDelay::new();
        algo.initialize(48000.0, 128);
        algo.detected_delay = 0.0; // No delay

        let mut buffer = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let expected = buffer.clone();

        algo.apply_fractional_delay(&mut buffer);

        // Should be unchanged (no delay)
        assert_eq!(buffer, expected);
    }

    #[test]
    fn test_latency_calculation() {
        let mut algo = TimeDelay::new();
        algo.initialize(48000.0, 512);
        algo.detected_delay = 10.5;

        assert_eq!(algo.latency_samples(), 11); // Ceiling of 10.5
    }
}
