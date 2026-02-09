/// Phase correction algorithms for stereo phase alignment
///
/// This module provides various algorithms to correct phase misalignment between
/// stereo channels. All algorithms implement the PhaseCorrectionAlgorithm trait.

pub mod time_delay;
pub mod allpass_cascade;

use crate::phase_data::NUM_BINS;
use nih_plug::params::enums::Enum;

/// Core trait for phase correction algorithms
pub trait PhaseCorrectionAlgorithm: Send {
    /// Get the name of the algorithm
    fn name(&self) -> &str;

    /// Initialize the algorithm with sample rate and buffer size
    fn initialize(&mut self, sample_rate: f32, max_block_size: usize);

    /// Configure the correction target from phase difference data
    /// phase_differences: Array of phase differences in radians [-π, π] for each bin
    fn set_phase_target(&mut self, phase_differences: &[f32; NUM_BINS]);

    /// Apply correction to stereo audio (modifies in-place)
    /// Corrects the phase relationship between left and right channels
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]);

    /// Reset internal state
    fn reset(&mut self);

    /// Get the latency introduced by this algorithm in samples
    fn latency_samples(&self) -> u32;
}

/// Available correction algorithm types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum AlgorithmType {
    /// Simple time-delay correction for linear phase errors
    #[name = "Time Delay"]
    TimeDelay,
    /// All-pass filter cascade for frequency-dependent correction
    #[name = "All-Pass Cascade"]
    AllPassCascade,
}

impl Default for AlgorithmType {
    fn default() -> Self {
        Self::TimeDelay
    }
}

/// Factory function to create algorithm instances
pub fn create_algorithm(algo_type: AlgorithmType) -> Box<dyn PhaseCorrectionAlgorithm> {
    match algo_type {
        AlgorithmType::TimeDelay => Box::new(time_delay::TimeDelay::new()),
        AlgorithmType::AllPassCascade => Box::new(allpass_cascade::AllPassCascade::new()),
    }
}
