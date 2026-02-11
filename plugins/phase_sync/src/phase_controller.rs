use nih_plug::prelude::*;
use std::collections::VecDeque;

#[derive(Enum, Clone, Copy, Debug, PartialEq)]
pub enum AdaptationMode {
    #[id = "immediate"]
    #[name = "Immediate"]
    Immediate,

    #[id = "linear"]
    #[name = "Linear Drift"]
    LinearDrift,

    #[id = "exponential"]
    #[name = "Exponential"]
    ExponentialDrift,

    #[id = "last_moment"]
    #[name = "Last Moment"]
    LastMoment,
}

pub struct PhaseController {
    // Current state
    current_rotation_degrees: f32,
    target_rotation_degrees: f32,

    // Timing information
    last_kick_sample: Option<usize>,
    next_kick_sample_predicted: Option<usize>,
    inter_kick_interval: f32,

    // Kick interval history for prediction
    recent_intervals: VecDeque<usize>,

    // Time since last update (for gradual reset)
    samples_since_last_kick: usize,
}

impl PhaseController {
    pub fn new() -> Self {
        Self {
            current_rotation_degrees: 0.0,
            target_rotation_degrees: 0.0,
            last_kick_sample: None,
            next_kick_sample_predicted: None,
            inter_kick_interval: 48000.0, // Default 1 second at 48kHz
            recent_intervals: VecDeque::with_capacity(8),
            samples_since_last_kick: 0,
        }
    }

    pub fn get_current_phase(
        &self,
        current_sample: usize,
        mode: AdaptationMode,
        transition_threshold: f32,
    ) -> f32 {
        let Some(last_kick) = self.last_kick_sample else {
            return 0.0; // No kick detected yet
        };

        let Some(_next_kick_predicted) = self.next_kick_sample_predicted else {
            return self.current_rotation_degrees; // No prediction yet
        };

        let samples_elapsed = current_sample.saturating_sub(last_kick) as f32;
        let progress = (samples_elapsed / self.inter_kick_interval).min(1.0);
        let delta = self.target_rotation_degrees - self.current_rotation_degrees;

        match mode {
            AdaptationMode::Immediate => {
                // Snap to target immediately
                self.target_rotation_degrees
            }

            AdaptationMode::LinearDrift => {
                // Linear interpolation over entire interval
                self.current_rotation_degrees + delta * progress
            }

            AdaptationMode::ExponentialDrift => {
                // Exponential curve: slow start, faster end
                let exp_progress = 1.0 - (-3.0 * progress).exp();
                let normalized = exp_progress / (1.0 - (-3.0_f32).exp());
                self.current_rotation_degrees + delta * normalized
            }

            AdaptationMode::LastMoment => {
                // Hold until threshold, then transition smoothly
                if progress < transition_threshold {
                    self.current_rotation_degrees
                } else {
                    let local_progress =
                        (progress - transition_threshold) / (1.0 - transition_threshold);
                    self.current_rotation_degrees + delta * local_progress
                }
            }
        }
    }

    pub fn on_kick_detected(
        &mut self,
        kick_sample: usize,
        bass_peak_sample: usize,
        center_freq: f32,
        sample_rate: f32,
        current_phase: f32,
    ) {
        // Calculate required phase shift
        let time_offset = (kick_sample as f32 - bass_peak_sample as f32) / sample_rate;
        let phase_radians = 2.0 * std::f32::consts::PI * center_freq * time_offset;
        let mut phase_degrees = phase_radians.to_degrees();

        // Wrap to [-180, 180]
        while phase_degrees > 180.0 {
            phase_degrees -= 360.0;
        }
        while phase_degrees < -180.0 {
            phase_degrees += 360.0;
        }

        // Update state
        self.current_rotation_degrees = current_phase;
        self.target_rotation_degrees = phase_degrees;

        // Update prediction
        if let Some(last_kick) = self.last_kick_sample {
            let interval = kick_sample.saturating_sub(last_kick);
            self.recent_intervals.push_back(interval);
            if self.recent_intervals.len() > 8 {
                self.recent_intervals.pop_front();
            }

            // Use median interval for prediction (robust to tempo changes)
            if !self.recent_intervals.is_empty() {
                let mut sorted = self.recent_intervals.iter().cloned().collect::<Vec<_>>();
                sorted.sort();
                self.inter_kick_interval = sorted[sorted.len() / 2] as f32;
                self.next_kick_sample_predicted =
                    Some(kick_sample + self.inter_kick_interval as usize);
            }
        }

        self.last_kick_sample = Some(kick_sample);
        self.samples_since_last_kick = 0;
    }

    pub fn update_sample_counter(&mut self, sample_rate: f32) {
        self.samples_since_last_kick += 1;

        // Gradually reset to 0 degrees if no kick detected for 2 seconds
        let timeout_samples = (sample_rate * 2.0) as usize;
        if self.samples_since_last_kick > timeout_samples {
            let fade_samples = (sample_rate * 0.5) as f32; // 500ms fade
            let fade_progress =
                ((self.samples_since_last_kick - timeout_samples) as f32 / fade_samples).min(1.0);

            self.current_rotation_degrees *= 1.0 - fade_progress;
            self.target_rotation_degrees *= 1.0 - fade_progress;

            // Clear prediction
            if fade_progress >= 1.0 {
                self.next_kick_sample_predicted = None;
            }
        }
    }

    pub fn get_target_phase(&self) -> f32 {
        self.target_rotation_degrees
    }

    pub fn reset(&mut self) {
        self.current_rotation_degrees = 0.0;
        self.target_rotation_degrees = 0.0;
        self.last_kick_sample = None;
        self.next_kick_sample_predicted = None;
        self.recent_intervals.clear();
        self.samples_since_last_kick = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_immediate_mode() {
        let mut controller = PhaseController::new();

        // Need two kicks to establish prediction
        controller.on_kick_detected(0, 1000, 100.0, 48000.0, 0.0);
        controller.on_kick_detected(48000, 49000, 100.0, 48000.0, 0.0);

        // In immediate mode, should snap to target instantly
        let phase = controller.get_current_phase(48100, AdaptationMode::Immediate, 0.8);
        assert!((phase - controller.get_target_phase()).abs() < 0.1);
    }

    #[test]
    fn test_linear_drift_progression() {
        let mut controller = PhaseController::new();

        // Need two kicks to establish prediction
        controller.on_kick_detected(0, 1000, 100.0, 48000.0, 0.0);
        controller.on_kick_detected(48000, 49000, 100.0, 48000.0, 45.0); // Different phase

        // Linear mode should progress gradually
        let phase_25 = controller.get_current_phase(48000 + 12000, AdaptationMode::LinearDrift, 0.8);
        let phase_50 = controller.get_current_phase(48000 + 24000, AdaptationMode::LinearDrift, 0.8);
        let phase_75 = controller.get_current_phase(48000 + 36000, AdaptationMode::LinearDrift, 0.8);

        // Should be monotonically increasing (or decreasing depending on target)
        // Just verify they're different and ordered
        assert_ne!(phase_25, phase_50);
        assert_ne!(phase_50, phase_75);
    }

    #[test]
    fn test_last_moment_delay() {
        let mut controller = PhaseController::new();
        controller.on_kick_detected(0, 1000, 100.0, 48000.0, 0.0);

        // Last moment mode should hold at current until threshold
        let phase_early = controller.get_current_phase(1000, AdaptationMode::LastMoment, 0.8);
        let phase_mid = controller.get_current_phase(24000, AdaptationMode::LastMoment, 0.8);

        // Early phase should match current (not yet transitioning)
        assert!((phase_early - 0.0).abs() < 0.1);

        // Mid phase should still be holding (below 80% threshold)
        assert!((phase_mid - 0.0).abs() < 1.0);
    }

    #[test]
    fn test_kick_interval_prediction() {
        let mut controller = PhaseController::new();

        // Build consistent interval history
        for i in 0..8 {
            controller.on_kick_detected(i * 48000, (i * 48000) + 1000, 100.0, 48000.0, 0.0);
        }

        // Should have prediction
        assert!(controller.next_kick_sample_predicted.is_some());

        // Interval should be around 48000 samples (1 second at 48kHz)
        assert!((controller.inter_kick_interval - 48000.0).abs() < 100.0);
    }

    #[test]
    fn test_phase_wrapping() {
        let mut controller = PhaseController::new();

        // Test with large time offset that creates phase >180 degrees
        controller.on_kick_detected(0, 10000, 100.0, 48000.0, 0.0);

        let target = controller.get_target_phase();

        // Should wrap to [-180, 180] range
        assert!(target >= -180.0 && target <= 180.0);
    }

    #[test]
    fn test_reset_clears_state() {
        let mut controller = PhaseController::new();
        controller.on_kick_detected(0, 1000, 100.0, 48000.0, 0.0);

        controller.reset();

        // Should return to initial state
        assert_eq!(controller.current_rotation_degrees, 0.0);
        assert_eq!(controller.target_rotation_degrees, 0.0);
        assert!(controller.last_kick_sample.is_none());
        assert!(controller.next_kick_sample_predicted.is_none());
    }
}
