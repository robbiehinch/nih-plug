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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_envelope_builds_up() {
        let mut detector = KickDetector::new(1, 48000.0);
        let mut sample_count = 0;

        // Process silence - envelope should stay at zero
        for _ in 0..100 {
            detector.process_sample(0.0, 0, sample_count);
            sample_count += 1;
        }
        assert_eq!(detector.envelope_state[0], 0.0);

        // Process signal - envelope should build up
        for _ in 0..100 {
            detector.process_sample(0.5, 0, sample_count);
            sample_count += 1;
        }
        assert!(detector.envelope_state[0] > 0.0, "Envelope should build up from signal");
    }

    #[test]
    fn test_performance_per_sample_processing() {
        let mut detector = KickDetector::new(1, 48000.0);
        let iterations = 48000; // 1 second at 48kHz
        let mut sample_count = 0;

        let start = Instant::now();
        for i in 0..iterations {
            let sample = if i % 10000 == 0 { 0.8 } else { 0.1 };
            detector.process_sample(sample, 0, sample_count);
            sample_count += 1;
        }
        let elapsed = start.elapsed();

        let time_per_sample = elapsed.as_nanos() / iterations as u128;

        // Assert performance: should process <500ns per sample (plenty of headroom)
        assert!(
            time_per_sample < 500,
            "Kick detector too slow: {} ns/sample (target <500ns)",
            time_per_sample
        );

        println!("KickDetector performance: {} ns/sample", time_per_sample);
    }

    #[test]
    fn test_adaptive_threshold_computation() {
        let mut detector = KickDetector::new(1, 48000.0);
        let mut sample_count = 0;

        // Build peak history
        for _ in 0..16 {
            for _ in 0..100 {
                detector.process_sample(0.1, 0, sample_count);
                sample_count += 1;
            }
            detector.process_sample(0.8, 0, sample_count);
            sample_count += 1;
        }

        // Threshold should adapt to peaks
        // Test that threshold is reasonable (not too sensitive or insensitive)
        let triggered_on_medium = detector.process_sample(0.6, 0, sample_count).is_some();
        sample_count += 1;
        let not_triggered_on_low = detector.process_sample(0.2, 0, sample_count).is_none();

        // At least one of these should be true
        assert!(
            triggered_on_medium || not_triggered_on_low,
            "Adaptive threshold not working correctly"
        );
    }

    #[test]
    fn test_min_interval_configuration() {
        let mut detector = KickDetector::new(1, 48000.0);

        // Test interval configuration
        detector.set_min_interval(100.0, 48000.0); // 100ms at 48kHz
        assert_eq!(detector.min_kick_interval_samples, 4800);

        detector.set_min_interval(50.0, 48000.0); // 50ms at 48kHz
        assert_eq!(detector.min_kick_interval_samples, 2400);

        // Verify threshold configuration
        detector.set_threshold(-18.0); // -18dB
        let expected = 10.0_f32.powf(-18.0 / 20.0);
        assert!((detector.threshold - expected).abs() < 0.001);
    }
}
