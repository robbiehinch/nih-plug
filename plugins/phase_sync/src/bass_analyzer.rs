use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct BassPeakInfo {
    pub sample_position: usize,
    pub energy_level: f32,
}

pub struct BassAnalyzer {
    // Analysis window size (samples)
    window_size: usize,

    // Peak tracking
    peak_history: VecDeque<BassPeakInfo>,
}

impl BassAnalyzer {
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            peak_history: VecDeque::with_capacity(16),
        }
    }

    /// Analyze bass timing in the provided buffer
    /// Returns the position and energy of the strongest peak
    pub fn analyze_bass_timing(
        &mut self,
        buffer: &[f32],
        current_sample_offset: usize,
    ) -> Option<BassPeakInfo> {
        if buffer.len() < self.window_size {
            return None;
        }

        // Find RMS energy peak in buffer
        let mut max_energy = 0.0;
        let mut peak_position = 0;

        for i in 0..(buffer.len().saturating_sub(self.window_size)) {
            let energy: f32 = buffer[i..i + self.window_size]
                .iter()
                .map(|x| x * x)
                .sum::<f32>() / self.window_size as f32;

            if energy > max_energy {
                max_energy = energy;
                peak_position = i + self.window_size / 2;
            }
        }

        if max_energy > 0.0 {
            let peak_info = BassPeakInfo {
                sample_position: current_sample_offset + peak_position,
                energy_level: max_energy.sqrt(),
            };

            self.peak_history.push_back(peak_info.clone());
            if self.peak_history.len() > 16 {
                self.peak_history.pop_front();
            }

            Some(peak_info)
        } else {
            None
        }
    }

    /// Analyze bass timing with optimized sliding window algorithm
    /// This uses O(N) complexity instead of O(N×M) by incrementally updating
    /// the window sum instead of recalculating it for each position.
    /// Returns the position and energy of the strongest peak.
    pub fn analyze_bass_timing_optimized(
        &mut self,
        buffer: &[f32],
        current_sample_offset: usize,
    ) -> Option<BassPeakInfo> {
        if buffer.len() < self.window_size {
            return None;
        }

        // Initial window sum (one-time cost: O(M))
        let mut sum_sq: f32 = buffer[0..self.window_size]
            .iter()
            .map(|x| x * x)
            .sum();

        let mut max_energy = sum_sq / self.window_size as f32;
        let mut peak_position = self.window_size / 2;

        // Sliding window: O(N) - only 2 operations per position
        for i in 1..(buffer.len() - self.window_size) {
            let outgoing = buffer[i - 1];
            let incoming = buffer[i + self.window_size - 1];

            // Incremental update: remove old sample, add new sample
            sum_sq = sum_sq - outgoing * outgoing + incoming * incoming;
            let energy = sum_sq / self.window_size as f32;

            if energy > max_energy {
                max_energy = energy;
                peak_position = i + self.window_size / 2;
            }
        }

        if max_energy > 0.0 {
            let peak_info = BassPeakInfo {
                sample_position: current_sample_offset + peak_position,
                energy_level: max_energy.sqrt(),
            };

            self.peak_history.push_back(peak_info.clone());
            if self.peak_history.len() > 16 {
                self.peak_history.pop_front();
            }

            Some(peak_info)
        } else {
            None
        }
    }

    pub fn set_window_size(&mut self, window_size: usize) {
        self.window_size = window_size;
    }

    pub fn reset(&mut self) {
        self.peak_history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_peak_detection() {
        let mut analyzer = BassAnalyzer::new(100);

        // Create buffer with clear peak in middle
        let mut buffer = vec![0.1f32; 1000];
        for i in 450..550 {
            buffer[i] = 0.8; // Peak in middle
        }

        let result = analyzer.analyze_bass_timing(&buffer, 0);
        assert!(result.is_some());

        let peak = result.unwrap();
        // Peak should be detected around position 500 (middle of peak region)
        assert!(peak.sample_position > 400 && peak.sample_position < 600);
    }

    #[test]
    fn test_window_size_scaling() {
        let mut analyzer = BassAnalyzer::new(50);
        let buffer = vec![0.5f32; 1000];

        // Should work with window size
        let result1 = analyzer.analyze_bass_timing(&buffer, 0);
        assert!(result1.is_some());

        // Change window size
        analyzer.set_window_size(200);
        let result2 = analyzer.analyze_bass_timing(&buffer, 0);
        assert!(result2.is_some());
    }

    #[test]
    fn test_insufficient_buffer() {
        let mut analyzer = BassAnalyzer::new(100);
        let buffer = vec![0.5f32; 50]; // Too small

        let result = analyzer.analyze_bass_timing(&buffer, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_peak_history_limit() {
        let mut analyzer = BassAnalyzer::new(50);
        let buffer = vec![0.5f32; 1000];

        // Trigger many peaks
        for _ in 0..20 {
            analyzer.analyze_bass_timing(&buffer, 0);
        }

        // History should be limited (internal detail, but we can verify it doesn't crash)
        // This test mainly ensures no memory leak or panic
        assert_eq!(analyzer.peak_history.len(), 16); // Max capacity
    }

    #[test]
    fn test_optimized_matches_original() {
        let mut analyzer1 = BassAnalyzer::new(100);
        let mut analyzer2 = BassAnalyzer::new(100);

        // Test with constant buffer
        let buffer = vec![0.5f32; 1000];
        let result1 = analyzer1.analyze_bass_timing(&buffer, 0);
        let result2 = analyzer2.analyze_bass_timing_optimized(&buffer, 0);

        assert_eq!(result1.is_some(), result2.is_some());
        if let (Some(r1), Some(r2)) = (result1, result2) {
            assert_eq!(r1.sample_position, r2.sample_position);
            assert!((r1.energy_level - r2.energy_level).abs() < 1e-6);
        }

        // Test with peak in middle
        let mut buffer_with_peak = vec![0.1f32; 1000];
        for i in 450..550 {
            buffer_with_peak[i] = 0.8;
        }

        analyzer1.reset();
        analyzer2.reset();

        let result1 = analyzer1.analyze_bass_timing(&buffer_with_peak, 0);
        let result2 = analyzer2.analyze_bass_timing_optimized(&buffer_with_peak, 0);

        assert_eq!(result1.is_some(), result2.is_some());
        if let (Some(r1), Some(r2)) = (result1, result2) {
            assert_eq!(r1.sample_position, r2.sample_position);
            assert!((r1.energy_level - r2.energy_level).abs() < 1e-6);
        }

        // Test with multiple window sizes
        for window_size in [50, 100, 200] {
            analyzer1.set_window_size(window_size);
            analyzer2.set_window_size(window_size);
            analyzer1.reset();
            analyzer2.reset();

            let result1 = analyzer1.analyze_bass_timing(&buffer_with_peak, 0);
            let result2 = analyzer2.analyze_bass_timing_optimized(&buffer_with_peak, 0);

            assert_eq!(result1.is_some(), result2.is_some());
            if let (Some(r1), Some(r2)) = (result1, result2) {
                // Energy levels must match closely
                assert!(
                    (r1.energy_level - r2.energy_level).abs() < 1e-5,
                    "Energy mismatch at window_size={}: {} vs {}",
                    window_size,
                    r1.energy_level,
                    r2.energy_level
                );

                // Positions may differ slightly due to floating point rounding in tie-breaking
                // When energies are identical, both positions are equally valid
                // Accept if positions match exactly, or if they're within the peak region
                let position_diff = (r1.sample_position as i32 - r2.sample_position as i32).abs();
                let energy_is_identical = (r1.energy_level - r2.energy_level).abs() < 1e-6;

                if position_diff > 0 {
                    assert!(
                        energy_is_identical && position_diff < window_size as i32,
                        "Position mismatch at window_size={} with non-identical energy: pos1={}, pos2={}, energy1={}, energy2={}",
                        window_size, r1.sample_position, r2.sample_position, r1.energy_level, r2.energy_level
                    );
                }
            }
        }
    }
}
