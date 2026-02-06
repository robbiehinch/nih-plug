use nih_plug::prelude::*;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;

mod bass_analyzer;
mod editor;
mod kick_detector;
mod lookahead_buffer;
mod phase_controller;
mod phase_rotator;
mod visualization_data;

use bass_analyzer::{BassAnalyzer, BassPeakInfo};
use kick_detector::{KickDetector, KickEvent};
use lookahead_buffer::LookaheadBuffer;
use phase_controller::{AdaptationMode, PhaseController};
use phase_rotator::{PhaseRotator, f32x2};
use visualization_data::{VisualizationData, KickMarker, PhasePoint};

/// Lookahead buffer size in samples (~85ms at 48kHz)
const LOOKAHEAD_SIZE: usize = 4096;

/// Bass analysis lookback window in samples (~42ms at 48kHz)
const BASS_LOOKBACK_SIZE: usize = 2048;

pub struct PhaseSync {
    params: Arc<PhaseSyncParams>,

    // Audio configuration
    sample_rate: Arc<AtomicF32>,

    // Kick detection (analyzes sidechain input)
    kick_detector: KickDetector,

    // Bass peak detection (analyzes main input)
    bass_analyzer: BassAnalyzer,

    // Phase rotation engine
    phase_rotator: PhaseRotator,

    // Adaptive phase control
    phase_controller: PhaseController,

    // Lookahead buffering for main input
    lookahead_buffer: LookaheadBuffer,

    // GUI communication
    visualization_data_input: triple_buffer::Input<VisualizationData>,
    visualization_data_output: Arc<Mutex<triple_buffer::Output<VisualizationData>>>,

    // Sample counter
    sample_counter: usize,

    // GUI update throttling
    samples_since_last_gui_update: usize,
}

#[derive(Params)]
pub struct PhaseSyncParams {
    // === Kick Detection ===
    #[id = "kick_thresh"]
    pub kick_threshold: FloatParam,

    #[id = "kick_attack"]
    pub kick_attack_ms: FloatParam,

    #[id = "kick_release"]
    pub kick_release_ms: FloatParam,

    #[id = "min_interval"]
    pub min_kick_interval_ms: FloatParam,

    // === Phase Rotation ===
    #[id = "center_freq"]
    pub center_frequency: FloatParam,

    #[id = "phase_amount"]
    pub phase_amount: FloatParam,

    #[id = "freq_spread"]
    pub frequency_spread: FloatParam,

    // === Adaptive Behavior ===
    #[id = "adapt_mode"]
    pub adaptation_mode: EnumParam<AdaptationMode>,

    #[id = "transition"]
    pub transition_threshold: FloatParam,

    // === Mix ===
    #[id = "dry_wet"]
    pub dry_wet: FloatParam,

    // === Advanced ===
    #[id = "bass_window"]
    pub bass_analysis_window_ms: FloatParam,
}

impl Default for PhaseSync {
    fn default() -> Self {
        let (visualization_data_input, visualization_data_output) =
            triple_buffer::TripleBuffer::new(&VisualizationData::default()).split();

        Self {
            params: Arc::new(PhaseSyncParams::default()),
            sample_rate: Arc::new(AtomicF32::new(48000.0)),
            kick_detector: KickDetector::new(2, 48000.0),
            bass_analyzer: BassAnalyzer::new(1024),
            phase_rotator: PhaseRotator::new(),
            phase_controller: PhaseController::new(),
            lookahead_buffer: LookaheadBuffer::new(2, LOOKAHEAD_SIZE),
            visualization_data_input,
            visualization_data_output: Arc::new(Mutex::new(visualization_data_output)),
            sample_counter: 0,
            samples_since_last_gui_update: 0,
        }
    }
}

impl PhaseSync {
    /// Send current state to GUI for visualization
    fn send_visualization_data(&mut self, current_sample: usize, current_phase: f32) {
        let sample_rate = self.sample_rate.load(Ordering::Relaxed);

        // Get current visualization data
        let mut viz_data = VisualizationData::new(sample_rate);

        // Set phase information
        viz_data.current_phase_degrees = current_phase;
        viz_data.target_phase_degrees = self.phase_controller.get_target_phase();

        // Add phase history point
        viz_data.phase_history.push_back(PhasePoint {
            time_offset: 0.0,
            phase_degrees: current_phase,
        });

        // Limit phase history size
        if viz_data.phase_history.len() > visualization_data::MAX_PHASE_HISTORY {
            viz_data.phase_history.pop_front();
        }

        // Update time offsets for existing history (60 FPS update rate)
        let time_delta = 1.0 / 60.0;
        for point in viz_data.phase_history.iter_mut() {
            point.time_offset -= time_delta;
        }

        // Copy recent waveform data from lookahead buffer
        let waveform_samples = viz_data.bass_waveform.len().min(LOOKAHEAD_SIZE);
        for i in 0..waveform_samples {
            // Read from lookahead buffer (most recent samples)
            let sample = self.lookahead_buffer.read_sample(0, waveform_samples - i);
            viz_data.bass_waveform[i] = sample;
        }

        // Write to triple buffer
        self.visualization_data_input.write(viz_data);
    }
}

impl Default for PhaseSyncParams {
    fn default() -> Self {
        Self {
            kick_threshold: FloatParam::new(
                "Kick Threshold",
                -18.0,
                FloatRange::Linear {
                    min: -36.0,
                    max: -6.0,
                },
            )
            .with_unit(" dB")
            .with_step_size(0.1),

            kick_attack_ms: FloatParam::new(
                "Kick Attack",
                3.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 10.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_step_size(0.1),

            kick_release_ms: FloatParam::new(
                "Kick Release",
                150.0,
                FloatRange::Skewed {
                    min: 50.0,
                    max: 500.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_step_size(1.0),

            min_kick_interval_ms: FloatParam::new(
                "Min Kick Interval",
                100.0,
                FloatRange::Linear {
                    min: 50.0,
                    max: 500.0,
                },
            )
            .with_unit(" ms")
            .with_step_size(1.0),

            center_frequency: FloatParam::new(
                "Center Frequency",
                100.0,
                FloatRange::Skewed {
                    min: 40.0,
                    max: 250.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" Hz")
            .with_step_size(0.1),

            phase_amount: FloatParam::new(
                "Phase Amount",
                100.0,
                FloatRange::Linear { min: 0.0, max: 100.0 },
            )
            .with_unit(" %")
            .with_step_size(0.1),

            frequency_spread: FloatParam::new(
                "Frequency Spread",
                0.5,
                FloatRange::Linear { min: 0.1, max: 2.0 },
            )
            .with_unit(" oct")
            .with_step_size(0.01),

            adaptation_mode: EnumParam::new("Adaptation Mode", AdaptationMode::LinearDrift),

            transition_threshold: FloatParam::new(
                "Transition Threshold",
                70.0,
                FloatRange::Linear {
                    min: 10.0,
                    max: 90.0,
                },
            )
            .with_unit(" %")
            .with_step_size(1.0),

            dry_wet: FloatParam::new(
                "Dry/Wet",
                100.0,
                FloatRange::Linear { min: 0.0, max: 100.0 },
            )
            .with_unit(" %")
            .with_step_size(0.1),

            bass_analysis_window_ms: FloatParam::new(
                "Bass Window",
                20.0,
                FloatRange::Linear {
                    min: 10.0,
                    max: 100.0,
                },
            )
            .with_unit(" ms")
            .with_step_size(1.0),
        }
    }
}

impl Plugin for PhaseSync {
    const NAME: &'static str = "Phase Sync";
    const VENDOR: &'static str = "Robson Cozendey";
    const URL: &'static str = "https://github.com/robbert-vdh/nih-plug";
    const EMAIL: &'static str = "robson.cozendey@gmail.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        // Stereo with stereo sidechain
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            ..AudioIOLayout::const_default()
        },
        // Mono with mono sidechain
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[new_nonzero_u32(1)],
            ..AudioIOLayout::const_default()
        },
    ];

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
                visualization_data: self.visualization_data_output.clone(),
                sample_rate: self.sample_rate.clone(),
            },
            editor::default_state(),
        )
    }

    fn initialize(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        let sample_rate = buffer_config.sample_rate;
        self.sample_rate.store(sample_rate, Ordering::Relaxed);

        // Initialize components with correct channel count
        let num_channels = audio_io_layout
            .main_output_channels
            .map(|c| c.get() as usize)
            .unwrap_or(2);

        self.kick_detector = KickDetector::new(num_channels, sample_rate);
        self.lookahead_buffer = LookaheadBuffer::new(num_channels, LOOKAHEAD_SIZE);

        // Update bass analyzer window size
        let window_samples =
            (self.params.bass_analysis_window_ms.value() / 1000.0 * sample_rate) as usize;
        self.bass_analyzer.set_window_size(window_samples);

        // Set latency
        context.set_latency_samples(LOOKAHEAD_SIZE as u32);

        true
    }

    fn reset(&mut self) {
        self.kick_detector.reset();
        self.bass_analyzer.reset();
        self.phase_rotator.reset();
        self.phase_controller.reset();
        self.lookahead_buffer.reset();
        self.sample_counter = 0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let sample_rate = self.sample_rate.load(Ordering::Relaxed);
        let num_channels = buffer.channels();

        // Get sidechain input (kick reference)
        let sidechain = if !aux.inputs.is_empty() {
            Some(&aux.inputs[0])
        } else {
            None
        };

        // Update kick detector parameters if changed
        self.kick_detector.update_coefficients(
            sample_rate,
            self.params.kick_attack_ms.value(),
            self.params.kick_release_ms.value(),
        );
        self.kick_detector
            .set_threshold(self.params.kick_threshold.value());
        self.kick_detector.set_min_interval(
            self.params.min_kick_interval_ms.value(),
            sample_rate,
        );

        // Update bass analyzer window size if changed
        let window_samples =
            (self.params.bass_analysis_window_ms.value() / 1000.0 * sample_rate) as usize;
        self.bass_analyzer.set_window_size(window_samples);

        // Update phase rotator parameters
        self.phase_rotator
            .set_center_frequency(self.params.center_frequency.value());
        self.phase_rotator
            .set_frequency_spread(self.params.frequency_spread.value());

        // Process each sample
        for sample_idx in 0..buffer.samples() {
            let current_sample = self.sample_counter + sample_idx;

            // Step 1: Write bass to lookahead buffer
            for ch in 0..num_channels {
                let input_sample = buffer.as_slice()[ch][sample_idx];
                self.lookahead_buffer.write_sample(ch, input_sample);
            }
            self.lookahead_buffer.advance_write_pos();

            // Step 2: Detect kicks in sidechain
            if let Some(sidechain_buffer) = sidechain {
                if sidechain_buffer.samples() > 0 && sample_idx < sidechain_buffer.samples() {
                    // Access sidechain sample without mutable borrow
                    let sidechain_channels = sidechain_buffer.as_slice_immutable();
                    let kick_sample = if !sidechain_channels.is_empty() {
                        sidechain_channels[0][sample_idx]
                    } else {
                        0.0
                    };

                    if let Some(_kick_event) =
                        self.kick_detector
                            .process_sample(kick_sample, 0, current_sample)
                    {
                        // Step 3: Analyze bass peak timing in recent buffer
                        let bass_buffer =
                            self.lookahead_buffer
                                .get_recent_samples(0, BASS_LOOKBACK_SIZE);

                        if let Some(bass_peak) = self.bass_analyzer.analyze_bass_timing(
                            &bass_buffer,
                            current_sample.saturating_sub(BASS_LOOKBACK_SIZE),
                        ) {
                            // Get current phase before update
                            let current_phase = self.phase_controller.get_current_phase(
                                current_sample,
                                self.params.adaptation_mode.value(),
                                self.params.transition_threshold.value() / 100.0,
                            );

                            // Step 4: Update phase controller
                            self.phase_controller.on_kick_detected(
                                current_sample,
                                bass_peak.sample_position,
                                self.params.center_frequency.value(),
                                sample_rate,
                                current_phase,
                            );
                        }
                    }
                }
            }

            // Step 5: Get current phase rotation
            let current_phase = self.phase_controller.get_current_phase(
                current_sample,
                self.params.adaptation_mode.value(),
                self.params.transition_threshold.value() / 100.0,
            );

            // Apply phase amount scaling
            let scaled_phase = current_phase * (self.params.phase_amount.value() / 100.0);

            // Update phase rotator if needed
            self.phase_rotator.set_target_phase(scaled_phase);
            self.phase_rotator.update_if_needed(sample_rate);

            // Step 6: Read from lookahead buffer and apply phase rotation
            if num_channels == 2 {
                // Stereo processing with SIMD
                let left_sample = self.lookahead_buffer.read_sample(0, LOOKAHEAD_SIZE - 1);
                let right_sample = self.lookahead_buffer.read_sample(1, LOOKAHEAD_SIZE - 1);

                let input_simd = f32x2::from_array([left_sample, right_sample]);
                let output_simd = self.phase_rotator.process(input_simd);
                let output_array = output_simd.as_array();

                // Apply dry/wet mix
                let wet_amount = self.params.dry_wet.value() / 100.0;
                buffer.as_slice()[0][sample_idx] =
                    left_sample * (1.0 - wet_amount) + output_array[0] * wet_amount;
                buffer.as_slice()[1][sample_idx] =
                    right_sample * (1.0 - wet_amount) + output_array[1] * wet_amount;
            } else {
                // Mono processing
                let mono_sample = self.lookahead_buffer.read_sample(0, LOOKAHEAD_SIZE - 1);
                let input_simd = f32x2::from_array([mono_sample, mono_sample]);
                let output_simd = self.phase_rotator.process(input_simd);
                let output_sample = output_simd.as_array()[0];

                // Apply dry/wet mix
                let wet_amount = self.params.dry_wet.value() / 100.0;
                buffer.as_slice()[0][sample_idx] =
                    mono_sample * (1.0 - wet_amount) + output_sample * wet_amount;
            }

            // Update phase controller sample counter
            self.phase_controller.update_sample_counter(sample_rate);

            // Update GUI periodically (every 512 samples ~ 10ms at 48kHz)
            self.samples_since_last_gui_update += 1;
            if self.samples_since_last_gui_update >= 512 {
                self.send_visualization_data(current_sample, current_phase);
                self.samples_since_last_gui_update = 0;
            }
        }

        self.sample_counter += buffer.samples();

        ProcessStatus::Normal
    }
}

impl ClapPlugin for PhaseSync {
    const CLAP_ID: &'static str = "com.robson-cozendey.phase-sync";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Adaptive phase alignment for bass and kick");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Mixing,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for PhaseSync {
    const VST3_CLASS_ID: [u8; 16] = *b"PhaseSyncRCozzey";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Tools,
    ];
}

nih_export_clap!(PhaseSync);
nih_export_vst3!(PhaseSync);
