use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct KickEvent {
    pub sample_position: usize,
    pub peak_level: f32,
}

pub struct KickDetector {
    // Envelope follower state per channel
    envelope_state: Vec<f32>,
    prev_envelope_state: Vec<f32>,

    // Coefficients computed from attack/release times
    attack_coeff: f32,
    release_coeff: f32,

    // Detection parameters
    threshold: f32,
    last_peak_sample: usize,
    min_kick_interval_samples: usize,

    // Adaptive threshold tracking
    recent_peak_levels: VecDeque<f32>,
    adaptive_threshold_percentile: f32,
}

impl KickDetector {
    pub fn new(num_channels: usize, sample_rate: f32) -> Self {
        let mut detector = Self {
            envelope_state: vec![0.0; num_channels],
            prev_envelope_state: vec![0.0; num_channels],
            attack_coeff: 0.0,
            release_coeff: 0.0,
            threshold: 0.1, // -20dB default
            last_peak_sample: 0,
            min_kick_interval_samples: (sample_rate * 0.1) as usize, // 100ms default
            recent_peak_levels: VecDeque::with_capacity(16),
            adaptive_threshold_percentile: 0.5,
        };

        // Initialize with default attack/release times
        detector.update_coefficients(sample_rate, 3.0, 150.0);
        detector
    }

    pub fn process_sample(
        &mut self,
        sample: f32,
        channel_idx: usize,
        current_sample: usize,
    ) -> Option<KickEvent> {
        // Envelope follower (fast attack, slow release)
        let abs_sample = sample.abs();
        let envelope = &mut self.envelope_state[channel_idx];
        let prev_envelope = self.prev_envelope_state[channel_idx];

        if abs_sample > *envelope {
            *envelope = (*envelope * self.attack_coeff)
                      + (abs_sample * (1.0 - self.attack_coeff));
        } else {
            *envelope = (*envelope * self.release_coeff)
                      + (abs_sample * (1.0 - self.release_coeff));
        }

        // Store envelope value before borrowing self again
        let current_envelope_value = *envelope;

        // Detect peak with hysteresis and timing constraints
        // Use first channel for detection
        if channel_idx == 0 {
            let adaptive_threshold = self.compute_adaptive_threshold();
            let time_since_last = current_sample.saturating_sub(self.last_peak_sample);

            // Rising edge detection: current envelope above threshold and previous was below
            let is_rising_edge = current_envelope_value > adaptive_threshold && prev_envelope <= adaptive_threshold;

            if is_rising_edge && time_since_last > self.min_kick_interval_samples {
                self.last_peak_sample = current_sample;

                // Update peak history
                self.recent_peak_levels.push_back(current_envelope_value);
                if self.recent_peak_levels.len() > 16 {
                    self.recent_peak_levels.pop_front();
                }

                self.prev_envelope_state[channel_idx] = current_envelope_value;

                return Some(KickEvent {
                    sample_position: current_sample,
                    peak_level: current_envelope_value,
                });
            }
        }

        self.prev_envelope_state[channel_idx] = current_envelope_value;
        None
    }

    fn compute_adaptive_threshold(&self) -> f32 {
        if self.recent_peak_levels.is_empty() {
            return self.threshold;
        }

        // Use median of recent peaks (robust to level changes)
        let mut sorted = self.recent_peak_levels.iter().cloned().collect::<Vec<_>>();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = sorted[sorted.len() / 2];

        // Use the higher of fixed threshold or adaptive threshold
        (median * self.adaptive_threshold_percentile).max(self.threshold)
    }

    pub fn update_coefficients(&mut self, sample_rate: f32, attack_ms: f32, release_ms: f32) {
        self.attack_coeff = (-1.0 / (attack_ms / 1000.0 * sample_rate)).exp();
        self.release_coeff = (-1.0 / (release_ms / 1000.0 * sample_rate)).exp();
    }

    pub fn set_threshold(&mut self, threshold_db: f32) {
        self.threshold = 10.0_f32.powf(threshold_db / 20.0);
    }

    pub fn set_min_interval(&mut self, interval_ms: f32, sample_rate: f32) {
        self.min_kick_interval_samples = (sample_rate * interval_ms / 1000.0) as usize;
    }

    pub fn reset(&mut self) {
        for env in &mut self.envelope_state {
            *env = 0.0;
        }
        for env in &mut self.prev_envelope_state {
            *env = 0.0;
        }
        self.recent_peak_levels.clear();
        self.last_peak_sample = 0;
    }
}
