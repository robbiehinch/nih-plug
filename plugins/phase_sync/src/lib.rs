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

/// Maximum number of bass tracks that can be aligned simultaneously
const MAX_BASS_TRACKS: usize = 8;

pub struct PhaseSync {
    params: Arc<PhaseSyncParams>,

    // Audio configuration
    sample_rate: Arc<AtomicF32>,

    // Kick detection (reads main input - the kick reference)
    kick_detector: KickDetector,

    // Per-track components (vectorized for multiple bass tracks)
    num_bass_tracks: usize,
    bass_analyzers: Vec<BassAnalyzer>,
    lookahead_buffers: Vec<LookaheadBuffer>,
    phase_rotators: Vec<PhaseRotator>,
    phase_controllers: Vec<PhaseController>,

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

        // Start with 1 bass track (will be resized in initialize())
        let num_bass_tracks = 1;

        Self {
            params: Arc::new(PhaseSyncParams::default()),
            sample_rate: Arc::new(AtomicF32::new(48000.0)),

            kick_detector: KickDetector::new(1, 48000.0),  // Mono kick

            num_bass_tracks,
            bass_analyzers: vec![BassAnalyzer::new(1024)],
            lookahead_buffers: vec![LookaheadBuffer::new(2, LOOKAHEAD_SIZE)],
            phase_rotators: vec![PhaseRotator::new()],
            phase_controllers: vec![PhaseController::new()],

            visualization_data_input,
            visualization_data_output: Arc::new(Mutex::new(visualization_data_output)),
            sample_counter: 0,
            samples_since_last_gui_update: 0,
        }
    }
}

impl PhaseSync {
    /// Send current state to GUI for visualization (shows track 0)
    fn send_visualization_data(&mut self, current_sample: usize, current_phase: f32) {
        let sample_rate = self.sample_rate.load(Ordering::Relaxed);

        // Get current visualization data
        let mut viz_data = VisualizationData::new(sample_rate);

        // Set phase information (from track 0)
        viz_data.current_phase_degrees = current_phase;
        viz_data.target_phase_degrees = if !self.phase_controllers.is_empty() {
            self.phase_controllers[0].get_target_phase()
        } else {
            0.0
        };

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

        // Copy recent waveform data from lookahead buffer (track 0)
        if !self.lookahead_buffers.is_empty() {
            let waveform_samples = viz_data.bass_waveform.len().min(LOOKAHEAD_SIZE);
            for i in 0..waveform_samples {
                // Read from lookahead buffer (most recent samples)
                let sample = self.lookahead_buffers[0].read_sample(0, waveform_samples - i);
                viz_data.bass_waveform[i] = sample;
            }
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
        // Mono kick reference + 1 stereo bass track
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),        // Mono kick reference
            main_output_channels: NonZeroU32::new(1),       // Pass-through kick output
            aux_input_ports: &[new_nonzero_u32(2)],         // 1 stereo bass input
            aux_output_ports: &[new_nonzero_u32(2)],        // 1 stereo bass output (aligned)
            ..AudioIOLayout::const_default()
        },
        // Mono kick reference + 2 stereo bass tracks
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[new_nonzero_u32(2), new_nonzero_u32(2)],
            aux_output_ports: &[new_nonzero_u32(2), new_nonzero_u32(2)],
            ..AudioIOLayout::const_default()
        },
        // Mono kick reference + 4 stereo bass tracks
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[new_nonzero_u32(2); 4],
            aux_output_ports: &[new_nonzero_u32(2); 4],
            ..AudioIOLayout::const_default()
        },
        // Mono kick reference + 8 stereo bass tracks
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[new_nonzero_u32(2); 8],
            aux_output_ports: &[new_nonzero_u32(2); 8],
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

        // Determine number of bass tracks from aux input count
        self.num_bass_tracks = audio_io_layout.aux_input_ports.len();

        // Initialize kick detector (mono, reads main input channel 0)
        self.kick_detector = KickDetector::new(1, sample_rate);

        // Resize and initialize per-track components
        self.bass_analyzers.clear();
        self.lookahead_buffers.clear();
        self.phase_rotators.clear();
        self.phase_controllers.clear();

        for _ in 0..self.num_bass_tracks {
            // One bass analyzer per track
            let mut analyzer = BassAnalyzer::new(1024);
            let window_samples =
                (self.params.bass_analysis_window_ms.value() / 1000.0 * sample_rate) as usize;
            analyzer.set_window_size(window_samples);
            self.bass_analyzers.push(analyzer);

            // One lookahead buffer per track (stereo)
            self.lookahead_buffers.push(LookaheadBuffer::new(2, LOOKAHEAD_SIZE));

            // One phase rotator per track
            self.phase_rotators.push(PhaseRotator::new());

            // One phase controller per track
            self.phase_controllers.push(PhaseController::new());
        }

        // Set latency (same for all tracks)
        context.set_latency_samples(LOOKAHEAD_SIZE as u32);

        true
    }

    fn reset(&mut self) {
        self.kick_detector.reset();

        // Reset all per-track components
        for analyzer in &mut self.bass_analyzers {
            analyzer.reset();
        }
        for buffer in &mut self.lookahead_buffers {
            buffer.reset();
        }
        for rotator in &mut self.phase_rotators {
            rotator.reset();
        }
        for controller in &mut self.phase_controllers {
            controller.reset();
        }

        self.sample_counter = 0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let sample_rate = self.sample_rate.load(Ordering::Relaxed);

        // Get kick reference from main input
        let kick_buffer = buffer;

        // Get bass tracks from aux inputs
        if aux.inputs.len() != self.num_bass_tracks {
            return ProcessStatus::Normal;  // Configuration mismatch
        }

        // Get bass output buffers
        if aux.outputs.len() != self.num_bass_tracks {
            return ProcessStatus::Normal;
        }

        // Update shared parameters
        self.kick_detector.update_coefficients(
            sample_rate,
            self.params.kick_attack_ms.value(),
            self.params.kick_release_ms.value(),
        );
        self.kick_detector.set_threshold(self.params.kick_threshold.value());
        self.kick_detector.set_min_interval(
            self.params.min_kick_interval_ms.value(),
            sample_rate,
        );

        let window_samples =
            (self.params.bass_analysis_window_ms.value() / 1000.0 * sample_rate) as usize;

        // Update all bass analyzers
        for analyzer in &mut self.bass_analyzers {
            analyzer.set_window_size(window_samples);
        }

        // Update all phase rotators
        for rotator in &mut self.phase_rotators {
            rotator.set_center_frequency(self.params.center_frequency.value());
            rotator.set_frequency_spread(self.params.frequency_spread.value());
        }

        // Process each sample
        for sample_idx in 0..kick_buffer.samples() {
            let current_sample = self.sample_counter + sample_idx;

            // === STEP 1: Write all bass tracks to lookahead buffers ===
            for (track_idx, input_buffer) in aux.inputs.iter().enumerate() {
                let input_channels = input_buffer.as_slice_immutable();
                for ch in 0..2 {  // Stereo
                    if ch < input_channels.len() && sample_idx < input_channels[ch].len() {
                        let sample = input_channels[ch][sample_idx];
                        self.lookahead_buffers[track_idx].write_sample(ch, sample);
                    }
                }
                self.lookahead_buffers[track_idx].advance_write_pos();
            }

            // === STEP 2: Detect kicks in main input and pass through ===
            let kick_sample = if kick_buffer.channels() > 0 {
                kick_buffer.as_slice()[0][sample_idx]
            } else {
                0.0
            };

            // Pass through kick to main output
            if kick_buffer.channels() > 0 {
                kick_buffer.as_slice()[0][sample_idx] = kick_sample;
            }

            let kick_detected = self.kick_detector
                .process_sample(kick_sample, 0, current_sample)
                .is_some();

            // === STEP 3: On kick detection, analyze all bass peaks ===
            if kick_detected {
                for track_idx in 0..self.num_bass_tracks {
                    // Get recent bass samples for this track
                    let bass_buffer = self.lookahead_buffers[track_idx]
                        .get_recent_samples(0, BASS_LOOKBACK_SIZE);

                    // Analyze bass peak timing
                    if let Some(bass_peak) = self.bass_analyzers[track_idx].analyze_bass_timing(
                        &bass_buffer,
                        current_sample.saturating_sub(BASS_LOOKBACK_SIZE),
                    ) {
                        // Get current phase before update
                        let current_phase = self.phase_controllers[track_idx].get_current_phase(
                            current_sample,
                            self.params.adaptation_mode.value(),
                            self.params.transition_threshold.value() / 100.0,
                        );

                        // Update this track's phase controller
                        self.phase_controllers[track_idx].on_kick_detected(
                            current_sample,
                            bass_peak.sample_position,
                            self.params.center_frequency.value(),
                            sample_rate,
                            current_phase,
                        );
                    }
                }
            }

            // === STEP 4: Process each bass track independently ===
            for track_idx in 0..self.num_bass_tracks {
                // Get current phase rotation for this track
                let current_phase = self.phase_controllers[track_idx].get_current_phase(
                    current_sample,
                    self.params.adaptation_mode.value(),
                    self.params.transition_threshold.value() / 100.0,
                );

                // Apply phase amount scaling
                let scaled_phase = current_phase * (self.params.phase_amount.value() / 100.0);

                // Update this track's phase rotator
                self.phase_rotators[track_idx].set_target_phase(scaled_phase);
                self.phase_rotators[track_idx].update_if_needed(sample_rate);

                // Read from lookahead buffer (stereo)
                let left_sample = self.lookahead_buffers[track_idx]
                    .read_sample(0, LOOKAHEAD_SIZE - 1);
                let right_sample = self.lookahead_buffers[track_idx]
                    .read_sample(1, LOOKAHEAD_SIZE - 1);

                // Apply phase rotation
                let input_simd = f32x2::from_array([left_sample, right_sample]);
                let output_simd = self.phase_rotators[track_idx].process(input_simd);
                let output_array = output_simd.as_array();

                // Apply dry/wet mix
                let wet_amount = self.params.dry_wet.value() / 100.0;
                let output_left = left_sample * (1.0 - wet_amount) + output_array[0] * wet_amount;
                let output_right = right_sample * (1.0 - wet_amount) + output_array[1] * wet_amount;

                // Write to aux output
                if let Some(output_buffer) = aux.outputs.get_mut(track_idx) {
                    let output_channels = output_buffer.as_slice();
                    if output_channels.len() >= 2 && sample_idx < output_channels[0].len() {
                        output_channels[0][sample_idx] = output_left;
                        output_channels[1][sample_idx] = output_right;
                    }
                }

                // Update phase controller sample counter
                self.phase_controllers[track_idx].update_sample_counter(sample_rate);
            }

            // Update GUI periodically (track 0 only)
            self.samples_since_last_gui_update += 1;
            if self.samples_since_last_gui_update >= 512 {
                let current_phase = if !self.phase_controllers.is_empty() {
                    self.phase_controllers[0].get_current_phase(
                        current_sample,
                        self.params.adaptation_mode.value(),
                        self.params.transition_threshold.value() / 100.0,
                    )
                } else {
                    0.0
                };
                self.send_visualization_data(current_sample, current_phase);
                self.samples_since_last_gui_update = 0;
            }
        }

        self.sample_counter += kick_buffer.samples();

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
