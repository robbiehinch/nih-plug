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
}
