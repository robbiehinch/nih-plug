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
