use atomic_float::AtomicF32;
use nih_plug::prelude::*;
use realfft::{RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex32;
use std::f32::consts::{PI, TAU as TWO_PI};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

mod correction;
mod editor;
mod phase_data;

use correction::{AlgorithmType, PhaseCorrectionAlgorithm};
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

    /// Current phase correction algorithm
    correction_algorithm: Box<dyn PhaseCorrectionAlgorithm>,
    /// Last captured phase snapshot for correction target
    correction_target: Option<[f32; NUM_BINS]>,
}

/// Correction mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum CorrectionMode {
    /// Analysis only - zero latency pass-through
    #[name = "Analyze Only"]
    AnalyzeOnly,
    /// Apply phase correction
    #[name = "Correct"]
    Correct,
}

impl Default for CorrectionMode {
    fn default() -> Self {
        Self::AnalyzeOnly
    }
}

#[derive(Params)]
pub struct PhaseAnalyzerParams {
    /// Momentary button to trigger phase capture
    #[id = "analyze"]
    pub analyze: BoolParam,

    /// Display freeze state (persisted)
    #[persist = "frozen"]
    pub is_frozen: Arc<AtomicBool>,

    // === Correction Parameters ===
    /// Correction mode toggle
    #[id = "correction_mode"]
    pub correction_mode: EnumParam<CorrectionMode>,

    /// Algorithm selection
    #[id = "algorithm"]
    pub algorithm: EnumParam<AlgorithmType>,

    /// Correction amount (wet/dry mix)
    #[id = "correction_amount"]
    pub correction_amount: FloatParam,

    /// Bypass correction
    #[id = "bypass"]
    pub bypass: BoolParam,
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

            correction_algorithm: correction::create_algorithm(AlgorithmType::default()),
            correction_target: None,
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

            // Correction parameters
            correction_mode: EnumParam::new("Correction Mode", CorrectionMode::default()),
            algorithm: EnumParam::new("Algorithm", AlgorithmType::default()),
            correction_amount: FloatParam::new(
                "Correction Amount",
                100.0,
                FloatRange::Linear { min: 0.0, max: 100.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            bypass: BoolParam::new("Bypass", false),
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

        // Initialize correction algorithm
        self.correction_algorithm.initialize(
            buffer_config.sample_rate,
            buffer_config.max_buffer_size as usize,
        );

        true
    }

    fn reset(&mut self) {
        // Reset STFT state
        self.stft.set_block_size(WINDOW_SIZE);

        // Reset correction algorithm
        self.correction_algorithm.reset();
        self.correction_target = None;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Check if we should capture phase data this cycle
        let should_capture = self.should_capture();

        // Check correction mode
        let correction_mode = self.params.correction_mode.value();
        let bypass = self.params.bypass.value();
        let apply_correction = correction_mode == CorrectionMode::Correct && !bypass;

        // Process STFT analysis
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

                    // Store for correction target
                    self.correction_target = Some(phase_data.phase_differences);

                    // Send to GUI via triple buffer (lock-free)
                    self.phase_data_input.write(phase_data);
                }
            });

        // Apply phase correction if enabled
        if apply_correction && self.correction_target.is_some() {
            // Set correction target
            if let Some(ref target) = self.correction_target {
                self.correction_algorithm.set_phase_target(target);
            }

            // Get channel slices and apply correction
            let mut channel_slices = buffer.as_slice();
            if channel_slices.len() >= 2 {
                let (left, right) = channel_slices.split_at_mut(1);
                self.correction_algorithm.process_stereo(left[0], right[0]);
            }
        }

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
