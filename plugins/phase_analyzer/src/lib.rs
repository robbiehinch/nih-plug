use atomic_float::AtomicF32;
use nih_plug::prelude::*;
use realfft::{RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex32;
use std::f32::consts::{PI, TAU as TWO_PI};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

mod editor;
mod phase_data;

use phase_data::{PhaseData, NUM_BINS, WINDOW_SIZE};

/// Good balance between frequency/time resolution (~43ms at 48kHz, ~23Hz resolution)
const OVERLAP_TIMES: usize = 4;

pub struct PhaseAnalyzer {
    params: Arc<PhaseAnalyzerParams>,

    /// STFT processing helper
    stft: util::StftHelper<1>,
    /// Current sample rate
    sample_rate: Arc<AtomicF32>,

    /// FFT plan for processing
    r2c_plan: Arc<dyn RealToComplex<f32>>,
    /// Pre-computed Hann window function
    window_function: Vec<f32>,
    /// FFT buffer for left channel
    left_fft_buffer: Vec<Complex32>,
    /// FFT buffer for right channel
    right_fft_buffer: Vec<Complex32>,

    /// Flag indicating analyze button was pressed
    capture_requested: Arc<AtomicBool>,

    /// Thread-safe communication to GUI (DSP writes here)
    phase_data_input: triple_buffer::Input<PhaseData>,
    /// Thread-safe communication to GUI (GUI reads here)
    phase_data_output: Arc<Mutex<triple_buffer::Output<PhaseData>>>,
}

#[derive(Params)]
pub struct PhaseAnalyzerParams {
    /// Momentary button to trigger phase capture
    #[id = "analyze"]
    pub analyze: BoolParam,

    /// Display freeze state (persisted)
    #[persist = "frozen"]
    pub is_frozen: Arc<AtomicBool>,
}

impl Default for PhaseAnalyzer {
    fn default() -> Self {
        // Set up triple buffer for lock-free DSP->GUI communication
        let (phase_data_input, phase_data_output) =
            triple_buffer::TripleBuffer::new(&PhaseData::default()).split();

        let sample_rate = Arc::new(AtomicF32::new(44100.0));
        let capture_requested = Arc::new(AtomicBool::new(false));
        let is_frozen = Arc::new(AtomicBool::new(false));

        // Create FFT plan
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c_plan = planner.plan_fft_forward(WINDOW_SIZE);

        // Pre-compute Hann window
        let window_function = util::window::hann(WINDOW_SIZE);

        // Create FFT output buffers
        let left_fft_buffer = r2c_plan.make_output_vec();
        let right_fft_buffer = r2c_plan.make_output_vec();

        Self {
            params: Arc::new(PhaseAnalyzerParams::new(
                capture_requested.clone(),
                is_frozen.clone(),
            )),

            stft: util::StftHelper::new(2, WINDOW_SIZE, 0),
            sample_rate: sample_rate.clone(),

            r2c_plan,
            window_function,
            left_fft_buffer,
            right_fft_buffer,

            capture_requested,

            phase_data_input,
            phase_data_output: Arc::new(Mutex::new(phase_data_output)),
        }
    }
}

impl PhaseAnalyzerParams {
    fn new(capture_requested: Arc<AtomicBool>, is_frozen: Arc<AtomicBool>) -> Self {
        Self {
            analyze: BoolParam::new("Analyze", false).with_callback(Arc::new({
                let capture_flag = capture_requested.clone();
                let frozen_flag = is_frozen.clone();
                move |value| {
                    if value {
                        capture_flag.store(true, Ordering::Relaxed);
                        frozen_flag.store(true, Ordering::Relaxed);
                    }
                }
            })),
            is_frozen,
        }
    }
}

impl Plugin for PhaseAnalyzer {
    const NAME: &'static str = "Phase Analyzer";
    const VENDOR: &'static str = "Moist Plugins GmbH";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "info@example.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(
            editor::Data {
                params: self.params.clone(),
                phase_data: self.phase_data_output.clone(),
                sample_rate: self.sample_rate.clone(),
            },
            editor::default_state(),
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate
            .store(buffer_config.sample_rate, Ordering::Relaxed);
        true
    }

    fn reset(&mut self) {
        // Reset STFT state
        self.stft.set_block_size(WINDOW_SIZE);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Check if we should capture phase data this cycle
        let should_capture = self.should_capture();

        // Process STFT analysis (audio passes through unchanged)
        self.stft
            .process_analyze_only(buffer, OVERLAP_TIMES, |channel_idx, real_fft_buffer| {
                // Apply Hann window
                util::window::multiply_with_window(real_fft_buffer, &self.window_function);

                // Perform FFT on appropriate buffer
                let target_buffer = if channel_idx == 0 {
                    &mut self.left_fft_buffer
                } else {
                    &mut self.right_fft_buffer
                };

                self.r2c_plan
                    .process(real_fft_buffer, target_buffer)
                    .unwrap();

                // After both channels are processed, compute phase differences
                if channel_idx == 1 && should_capture {
                    let mut phase_data = PhaseData::new(self.sample_rate.load(Ordering::Relaxed));
                    phase_data.is_frozen = true;

                    for (bin_idx, (left_bin, right_bin)) in self
                        .left_fft_buffer
                        .iter()
                        .zip(&self.right_fft_buffer)
                        .enumerate()
                        .take(NUM_BINS)
                    {
                        // Extract phase from complex numbers
                        let left_phase = left_bin.arg();
                        let right_phase = right_bin.arg();

                        // Compute difference and wrap to [-π, π]
                        let mut phase_diff = left_phase - right_phase;
                        while phase_diff > PI {
                            phase_diff -= TWO_PI;
                        }
                        while phase_diff < -PI {
                            phase_diff += TWO_PI;
                        }

                        phase_data.phase_differences[bin_idx] = phase_diff;
                    }

                    // Send to GUI via triple buffer (lock-free)
                    self.phase_data_input.write(phase_data);
                }
            });

        ProcessStatus::Normal
    }
}

impl PhaseAnalyzer {
    /// Check if we should capture phase data this processing cycle
    fn should_capture(&self) -> bool {
        // Use compare_exchange to atomically check and clear the flag
        self.capture_requested
            .compare_exchange(true, false, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }
}

impl ClapPlugin for PhaseAnalyzer {
    const CLAP_ID: &'static str = "com.moist-plugins-gmbh.phase-analyzer";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Stereo phase difference analyzer");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Analyzer,
        ClapFeature::Stereo,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for PhaseAnalyzer {
    const VST3_CLASS_ID: [u8; 16] = *b"PhaseAnalyzer123";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Analyzer];
}

nih_export_clap!(PhaseAnalyzer);
nih_export_vst3!(PhaseAnalyzer);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_initialization() {
        let plugin = PhaseAnalyzer::default();
        assert_eq!(plugin.params.analyze.value(), false);
        assert_eq!(plugin.params.is_frozen.load(Ordering::Relaxed), false);
    }

    #[test]
    fn test_capture_mechanism() {
        let plugin = PhaseAnalyzer::default();

        // Initially should not capture
        assert!(!plugin.should_capture());

        // Set capture requested
        plugin.capture_requested.store(true, Ordering::Relaxed);

        // Should capture once
        assert!(plugin.should_capture());

        // Should not capture again (flag was cleared)
        assert!(!plugin.should_capture());
    }

    #[test]
    fn test_phase_data_default() {
        let data = PhaseData::default();
        assert_eq!(data.num_bins, NUM_BINS);
        assert_eq!(data.is_frozen, false);
        assert_eq!(data.sample_rate, 44100.0);

        // All phase differences should be initialized to zero
        for &phase in &data.phase_differences {
            assert_eq!(phase, 0.0);
        }
    }
}
