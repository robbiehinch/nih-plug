use criterion::{black_box, Criterion, BenchmarkId};
use phase_sync::bench_helpers::{f32x2, PhaseRotator};

pub fn bench_phase_rotator(c: &mut Criterion) {
    let mut group = c.benchmark_group("phase_rotator");

    // Benchmark single stereo sample processing with varying filter counts
    for num_filters in [1, 4, 8, 12, 16] {
        group.bench_with_input(
            BenchmarkId::new("process_stereo_sample", num_filters),
            &num_filters,
            |b, &count| {
                let mut rotator = PhaseRotator::new();
                let target_phase = (count as f32) * 90.0 / 16.0; // Scale to use filters
                rotator.set_target_phase(target_phase);
                rotator.set_center_frequency(100.0);
                rotator.set_frequency_spread(0.5);
                rotator.update_if_needed(48000.0);

                let input = f32x2::from_array([0.5, -0.3]);

                b.iter(|| {
                    rotator.process(black_box(input))
                });
            },
        );
    }

    // Benchmark buffer processing (512 stereo samples)
    group.bench_function("process_buffer_512", |b| {
        let mut rotator = PhaseRotator::new();
        rotator.set_target_phase(45.0);
        rotator.set_center_frequency(100.0);
        rotator.set_frequency_spread(0.5);
        rotator.update_if_needed(48000.0);

        let mut left = vec![0.5f32; 512];
        let mut right = vec![-0.3f32; 512];

        b.iter(|| {
            for i in 0..512 {
                let input = f32x2::from_array([left[i], right[i]]);
                let output = rotator.process(black_box(input));
                let output_array = output.as_array();
                left[i] = output_array[0];
                right[i] = output_array[1];
            }
        });
    });

    // Benchmark coefficient update cost
    group.bench_function("update_coefficients", |b| {
        let mut rotator = PhaseRotator::new();
        let mut angle = 0.0;

        b.iter(|| {
            angle = (angle + 1.0) % 90.0;
            rotator.set_target_phase(black_box(angle));
            rotator.set_center_frequency(100.0);
            rotator.set_frequency_spread(0.5);
            rotator.update_if_needed(48000.0);
            // Force coefficient update by processing sample
            let input = f32x2::from_array([0.0, 0.0]);
            rotator.process(input);
        });
    });

    group.finish();
}
