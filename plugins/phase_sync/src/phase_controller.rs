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
