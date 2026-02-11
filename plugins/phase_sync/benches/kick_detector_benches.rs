use criterion::{black_box, Criterion, BenchmarkId};
use phase_sync::bench_helpers::KickDetector;

pub fn bench_kick_detector(c: &mut Criterion) {
    let mut group = c.benchmark_group("kick_detector");

    // Benchmark per-sample processing
    group.bench_function("process_single_sample", |b| {
        let mut detector = KickDetector::new(1, 48000.0);
        let mut sample_count = 0;
        b.iter(|| {
            detector.process_sample(black_box(0.5), 0, sample_count);
            sample_count += 1;
        });
    });

    // Benchmark adaptive threshold computation (with varying peak history sizes)
    for history_size in [1, 4, 8, 16] {
        group.bench_with_input(
            BenchmarkId::new("adaptive_threshold", history_size),
            &history_size,
            |b, &size| {
                let mut detector = KickDetector::new(1, 48000.0);
                let mut sample_count = 0;

                // Prime with samples to build history
                for _ in 0..size {
                    detector.process_sample(0.8, 0, sample_count); // Trigger peaks
                    sample_count += 1;
                    for _ in 0..100 {
                        detector.process_sample(0.1, 0, sample_count);
                        sample_count += 1;
                    }
                }

                b.iter(|| {
                    // This will trigger threshold computation
                    detector.process_sample(black_box(0.9), 0, sample_count);
                    sample_count += 1;
                });
            },
        );
    }

    // Benchmark full buffer processing (512 samples)
    group.bench_function("process_buffer_512", |b| {
        let mut detector = KickDetector::new(1, 48000.0);
        let samples = vec![0.5f32; 512];
        let mut sample_count = 0;

        b.iter(|| {
            for &sample in &samples {
                detector.process_sample(black_box(sample), 0, sample_count);
                sample_count += 1;
            }
        });
    });

    group.finish();
}
