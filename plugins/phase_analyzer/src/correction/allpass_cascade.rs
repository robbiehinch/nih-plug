/// All-pass filter cascade for frequency-dependent phase correction
///
/// Groups 1025 FFT bins into 12 logarithmically-spaced frequency bands and applies
/// all-pass biquad filters to each band for smooth phase correction.

use super::PhaseCorrectionAlgorithm;
use crate::phase_data::NUM_BINS;
use std::f32::consts::TAU;

/// Number of frequency bands for all-pass cascade
const NUM_BANDS: usize = 12;

/// Q factor for Butterworth response (smooth, no resonance)
const BUTTERWORTH_Q: f32 = std::f32::consts::FRAC_1_SQRT_2; // 0.707

/// Biquad filter implementation
#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    s1: f32,
    s2: f32,
}

impl Biquad {
    fn new() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// Set all-pass filter coefficients
    /// Based on Audio EQ Cookbook: http://shepazu.github.io/Audio-EQ-Cookbook/audio-eq-cookbook.html
    fn set_allpass(&mut self, sample_rate: f32, frequency: f32, q: f32) {
        let omega0 = TAU * (frequency / sample_rate);
        let cos_omega0 = omega0.cos();
        let alpha = omega0.sin() / (2.0 * q);

        // Prenormalize by a0
        let a0 = 1.0 + alpha;
        self.b0 = (1.0 - alpha) / a0;
        self.b1 = (-2.0 * cos_omega0) / a0;
        self.b2 = (1.0 + alpha) / a0;
        self.a1 = (-2.0 * cos_omega0) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    /// Process a single sample using transposed direct form II
    #[inline]
    fn process(&mut self, sample: f32) -> f32 {
        let result = self.b0 * sample + self.s1;
        self.s1 = self.b1 * sample - self.a1 * result + self.s2;
        self.s2 = self.b2 * sample - self.a2 * result;
        result
    }

    fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }
}

/// Frequency band definition
#[derive(Clone, Copy, Debug)]
struct Band {
    start_bin: usize,
    end_bin: usize,
    center_freq: f32,
}

pub struct AllPassCascade {
    sample_rate: f32,
    bands: [Band; NUM_BANDS],
    filters: [Biquad; NUM_BANDS],
    target_phases: [f32; NUM_BANDS],
}

impl AllPassCascade {
    pub fn new() -> Self {
        Self {
            sample_rate: 44100.0,
            bands: [Band {
                start_bin: 0,
                end_bin: 0,
                center_freq: 0.0,
            }; NUM_BANDS],
            filters: [Biquad::new(); NUM_BANDS],
            target_phases: [0.0; NUM_BANDS],
        }
    }

    /// Initialize frequency bands (logarithmically spaced)
    fn initialize_bands(&mut self) {
        let nyquist = self.sample_rate / 2.0;
        let bin_spacing = nyquist / (NUM_BINS - 1) as f32;

        // Logarithmic frequency spacing from 20 Hz to Nyquist
        let log_min = 20.0f32.ln();
        let log_max = nyquist.ln();
        let log_step = (log_max - log_min) / NUM_BANDS as f32;

        for i in 0..NUM_BANDS {
            let freq_start = (log_min + i as f32 * log_step).exp();
            let freq_end = (log_min + (i + 1) as f32 * log_step).exp();
            let center_freq = (freq_start * freq_end).sqrt(); // Geometric mean

            let start_bin = (freq_start / bin_spacing).round() as usize;
            let end_bin = (freq_end / bin_spacing).round().min(NUM_BINS as f32 - 1.0) as usize;

            self.bands[i] = Band {
                start_bin,
                end_bin,
                center_freq,
            };
        }
    }

    /// Group phase differences by frequency band (average phase per band)
    fn group_phase_by_bands(&mut self, phase_diff: &[f32; NUM_BINS]) {
        for (i, band) in self.bands.iter().enumerate() {
            if band.end_bin <= band.start_bin {
                self.target_phases[i] = 0.0;
                continue;
            }

            let mut sum = 0.0;
            let mut count = 0;

            for bin in band.start_bin..=band.end_bin {
                if bin < NUM_BINS {
                    sum += phase_diff[bin];
                    count += 1;
                }
            }

            self.target_phases[i] = if count > 0 {
                sum / count as f32
            } else {
                0.0
            };
        }
    }

    /// Update filter coefficients based on target phases
    fn update_filters(&mut self) {
        for (i, &target_phase) in self.target_phases.iter().enumerate() {
            if target_phase.abs() < 0.01 {
                // Near-zero phase, use identity filter
                self.filters[i].b0 = 1.0;
                self.filters[i].b1 = 0.0;
                self.filters[i].b2 = 0.0;
                self.filters[i].a1 = 0.0;
                self.filters[i].a2 = 0.0;
            } else {
                // Set all-pass filter for this band
                // Note: All-pass filters provide 180° phase shift at center frequency
                // Multiple cascaded filters may be needed for larger phase corrections
                self.filters[i].set_allpass(
                    self.sample_rate,
                    self.bands[i].center_freq,
                    BUTTERWORTH_Q,
                );
            }
        }
    }
}

impl PhaseCorrectionAlgorithm for AllPassCascade {
    fn name(&self) -> &str {
        "All-Pass Cascade"
    }

    fn initialize(&mut self, sample_rate: f32, _max_block_size: usize) {
        self.sample_rate = sample_rate;
        self.initialize_bands();

        // Reset all filters
        for filter in &mut self.filters {
            filter.reset();
        }
    }

    fn set_phase_target(&mut self, phase_differences: &[f32; NUM_BINS]) {
        self.group_phase_by_bands(phase_differences);
        self.update_filters();
    }

    fn process_stereo(&mut self, _left: &mut [f32], right: &mut [f32]) {
        // Apply cascade of all-pass filters to right channel
        for sample in right.iter_mut() {
            let mut output = *sample;
            for filter in &mut self.filters {
                output = filter.process(output);
            }
            *sample = output;
        }
    }

    fn reset(&mut self) {
        for filter in &mut self.filters {
            filter.reset();
        }
        self.target_phases.fill(0.0);
    }

    fn latency_samples(&self) -> u32 {
        // Group delay of cascaded biquads (approximate)
        // Each all-pass contributes ~1-2 samples at center frequency
        (NUM_BANDS * 2) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allpass_cascade_initialization() {
        let mut algo = AllPassCascade::new();
        algo.initialize(48000.0, 512);

        assert_eq!(algo.sample_rate, 48000.0);

        // Verify bands are initialized
        for (i, band) in algo.bands.iter().enumerate() {
            assert!(band.center_freq > 0.0, "Band {} has zero center freq", i);
            assert!(band.start_bin <= band.end_bin, "Band {} has invalid range", i);
        }
    }

    #[test]
    fn test_band_coverage() {
        let mut algo = AllPassCascade::new();
        algo.initialize(48000.0, 512);

        // First band should start near bin 0
        assert!(algo.bands[0].start_bin < 10);

        // Last band should end near NUM_BINS
        assert!(algo.bands[NUM_BANDS - 1].end_bin > NUM_BINS - 100);
    }

    #[test]
    fn test_phase_grouping_zero() {
        let mut algo = AllPassCascade::new();
        algo.initialize(48000.0, 512);

        let phase_diff = [0.0; NUM_BINS];
        algo.group_phase_by_bands(&phase_diff);

        for &phase in &algo.target_phases {
            assert_eq!(phase, 0.0);
        }
    }

    #[test]
    fn test_phase_grouping_constant() {
        let mut algo = AllPassCascade::new();
        algo.initialize(48000.0, 512);

        let phase_diff = [1.5; NUM_BINS];
        algo.group_phase_by_bands(&phase_diff);

        for &phase in &algo.target_phases {
            assert!((phase - 1.5).abs() < 0.01);
        }
    }

    #[test]
    fn test_biquad_allpass_identity() {
        let mut filter = Biquad::new();

        let input = [1.0, 0.5, -0.5, 0.0, 1.0];
        let mut output = [0.0; 5];

        // Identity filter (b0=1, others=0) should pass through
        for (i, &sample) in input.iter().enumerate() {
            output[i] = filter.process(sample);
        }

        // First few samples may differ due to filter warmup
        // but steady state should match
        assert!((output[4] - input[4]).abs() < 0.01);
    }

    #[test]
    fn test_stereo_processing_preserves_left() {
        let mut algo = AllPassCascade::new();
        algo.initialize(48000.0, 512);

        let mut left = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let mut right = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let left_copy = left.clone();

        algo.process_stereo(&mut left, &mut right);

        // Left channel should be unchanged
        assert_eq!(left, left_copy);
    }

    #[test]
    fn test_latency_reasonable() {
        let algo = AllPassCascade::new();
        let latency = algo.latency_samples();

        // Should be low latency (< 50 samples)
        assert!(latency < 50);
        assert!(latency > 0);
    }
}
