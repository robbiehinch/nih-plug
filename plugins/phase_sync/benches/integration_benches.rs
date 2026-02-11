use criterion::{black_box, Criterion, BenchmarkId};
use phase_sync::bench_helpers::{
    AdaptationMode, BassAnalyzer, KickDetector, LookaheadBuffer, PhaseController, PhaseRotator,
    f32x2,
};

pub fn bench_full_process(c: &mut Criterion) {
    let mut group = c.benchmark_group("integration");

    // Benchmark full process loop with varying bass track counts
    for num_tracks in [1, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("process_block", format!("{}_tracks", num_tracks)),
            &num_tracks,
            |b, &tracks| {
                // Setup realistic plugin state
                let buffer_size = 512;
                let sample_rate = 48000.0;

                let mut kick_detector = KickDetector::new(1, sample_rate);
                let mut bass_analyzers: Vec<BassAnalyzer> =
                    (0..tracks).map(|_| BassAnalyzer::new(1024)).collect();
                let mut lookahead_buffers: Vec<LookaheadBuffer> =
                    (0..tracks).map(|_| LookaheadBuffer::new(2, 4096)).collect();
                let mut phase_rotators: Vec<PhaseRotator> =
                    (0..tracks).map(|_| PhaseRotator::new()).collect();
                let mut phase_controllers: Vec<PhaseController> =
                    (0..tracks).map(|_| PhaseController::new()).collect();

                // Prime buffers with data
                for buffer in &mut lookahead_buffers {
                    for _ in 0..4096 {
                        buffer.write_sample(0, 0.3);
                        buffer.write_sample(1, -0.3);
                        buffer.advance_write_pos();
                    }
                }

                let main_input = vec![0.5f32; buffer_size];
                let bass_inputs: Vec<Vec<f32>> = (0..tracks)
                    .map(|_| vec![0.3f32; buffer_size * 2])
                    .collect();

                let mut sample_count = 0;

                b.iter(|| {
                    // Simulate main process loop
                    for sample_idx in 0..buffer_size {
                        let current_sample = sample_count + sample_idx;

                        // Step 1: Write bass tracks to lookahead buffers
                        for (track_idx, input_buffer) in bass_inputs.iter().enumerate() {
                            lookahead_buffers[track_idx]
                                .write_sample(0, black_box(input_buffer[sample_idx * 2]));
                            lookahead_buffers[track_idx]
                                .write_sample(1, black_box(input_buffer[sample_idx * 2 + 1]));
                            lookahead_buffers[track_idx].advance_write_pos();
                        }

                        // Step 2: Detect kicks
                        let kick_sample = main_input[sample_idx];
                        let kick_detected = kick_detector
                            .process_sample(black_box(kick_sample), 0, current_sample)
                            .is_some();

                        // Step 3: On kick, analyze bass peaks
                        if kick_detected {
                            for track_idx in 0..tracks {
                                let bass_buffer =
                                    lookahead_buffers[track_idx].get_recent_samples(0, 2048);

                                if let Some(_bass_peak) =
                                    bass_analyzers[track_idx].analyze_bass_timing(
                                        black_box(&bass_buffer),
                                        current_sample.saturating_sub(2048),
                                    )
                                {
                                    let current_phase = phase_controllers[track_idx]
                                        .get_current_phase(
                                            current_sample,
                                            AdaptationMode::LinearDrift,
                                            0.8,
                                        );

                                    phase_controllers[track_idx].on_kick_detected(
                                        current_sample,
                                        _bass_peak.sample_position,
                                        100.0,
                                        sample_rate,
                                        current_phase,
                                    );
                                }
                            }
                        }

                        // Step 4: Process each bass track
                        for track_idx in 0..tracks {
                            let current_phase = phase_controllers[track_idx].get_current_phase(
                                current_sample,
                                AdaptationMode::LinearDrift,
                                0.8,
                            );

                            phase_rotators[track_idx].set_target_phase(current_phase);
                            phase_rotators[track_idx].set_center_frequency(100.0);
                            phase_rotators[track_idx].set_frequency_spread(0.5);
                            phase_rotators[track_idx].update_if_needed(sample_rate);

                            let left_sample =
                                lookahead_buffers[track_idx].read_sample(0, 4095);
                            let right_sample =
                                lookahead_buffers[track_idx].read_sample(1, 4095);

                            let input_simd = f32x2::from_array([left_sample, right_sample]);
                            let _output_simd = phase_rotators[track_idx].process(input_simd);

                            phase_controllers[track_idx].update_sample_counter(sample_rate);
                        }
                    }
                    sample_count += buffer_size;
                });
            },
        );
    }

    group.finish();
}
