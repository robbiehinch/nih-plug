/// Data structure for visualization information sent from DSP to GUI thread

use std::collections::VecDeque;

/// Size of waveform buffer for display (about 100ms at 48kHz / 50 update rate)
pub const WAVEFORM_SIZE: usize = 2048;

/// Maximum number of kick markers to display
pub const MAX_KICK_MARKERS: usize = 8;

/// Maximum phase history points
pub const MAX_PHASE_HISTORY: usize = 100;

#[derive(Debug, Clone)]
pub struct VisualizationData {
    // Kick detection display
    pub recent_kicks: VecDeque<KickMarker>,
    pub next_kick_predicted: Option<f32>, // Time in seconds from now

    // Bass waveform (ringbuffer for scrolling display)
    pub bass_waveform: [f32; WAVEFORM_SIZE],
    pub waveform_write_pos: usize,

    // Phase rotation over time
    pub phase_history: VecDeque<PhasePoint>,

    // Current state
    pub current_phase_degrees: f32,
    pub target_phase_degrees: f32,
    pub kick_detected_flash: bool,

    // Sample rate for time calculations
    pub sample_rate: f32,
}

#[derive(Clone, Debug)]
pub struct KickMarker {
    pub time_offset: f32, // Seconds from current time (negative = past)
    pub level: f32,
}

#[derive(Clone, Debug)]
pub struct PhasePoint {
    pub time_offset: f32, // Seconds from current time (negative = past)
    pub phase_degrees: f32,
}

impl Default for VisualizationData {
    fn default() -> Self {
        Self {
            recent_kicks: VecDeque::with_capacity(MAX_KICK_MARKERS),
            next_kick_predicted: None,
            bass_waveform: [0.0; WAVEFORM_SIZE],
            waveform_write_pos: 0,
            phase_history: VecDeque::with_capacity(MAX_PHASE_HISTORY),
            current_phase_degrees: 0.0,
            target_phase_degrees: 0.0,
            kick_detected_flash: false,
            sample_rate: 48000.0,
        }
    }
}

impl VisualizationData {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            ..Default::default()
        }
    }
}
