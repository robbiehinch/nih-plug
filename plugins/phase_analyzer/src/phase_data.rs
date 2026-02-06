/// Data structure for phase difference information sent from DSP to GUI thread

/// Number of FFT bins (half of window size + 1)
pub const WINDOW_SIZE: usize = 2048;
pub const NUM_BINS: usize = WINDOW_SIZE / 2 + 1; // 1025 bins

#[derive(Debug, Clone)]
pub struct PhaseData {
    /// Phase differences in radians [-π, π] for each frequency bin
    pub phase_differences: [f32; NUM_BINS],
    /// Number of valid bins (typically NUM_BINS)
    pub num_bins: usize,
    /// Whether the display is frozen in snapshot mode
    pub is_frozen: bool,
    /// Current sample rate
    pub sample_rate: f32,
}

impl Default for PhaseData {
    fn default() -> Self {
        Self {
            phase_differences: [0.0; NUM_BINS],
            num_bins: NUM_BINS,
            is_frozen: false,
            sample_rate: 44100.0,
        }
    }
}

impl PhaseData {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            ..Default::default()
        }
    }
}
