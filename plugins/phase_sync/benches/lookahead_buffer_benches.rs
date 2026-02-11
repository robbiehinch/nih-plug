use criterion::{black_box, Criterion, BenchmarkId};
use phase_sync::bench_helpers::LookaheadBuffer;

pub fn bench_lookahead_buffer(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookahead_buffer");

    // Benchmark write operations
    group.bench_function("write_single_sample", |b| {
        let mut buffer = LookaheadBuffer::new(2, 4096);
        b.iter(|| {
            buffer.write_sample(0, black_box(0.5));
            buffer.write_sample(1, black_box(-0.3));
            buffer.advance_write_pos();
        });
    });

    // Benchmark read operations
    group.bench_function("read_single_sample", |b| {
        let mut buffer = LookaheadBuffer::new(2, 4096);

        // Prime buffer with data
        for _ in 0..4096 {
            buffer.write_sample(0, 0.5);
            buffer.write_sample(1, -0.3);
            buffer.advance_write_pos();
        }

        b.iter(|| {
            let _ = buffer.read_sample(0, black_box(1000));
            let _ = buffer.read_sample(1, black_box(1000));
        });
    });

    // Benchmark get_recent_samples (allocates Vec)
    for sample_count in [512, 1024, 2048, 4096] {
        group.bench_with_input(
            BenchmarkId::new("get_recent_samples", sample_count),
            &sample_count,
            |b, &count| {
                let mut buffer = LookaheadBuffer::new(2, 4096);
                for _ in 0..4096 {
                    buffer.write_sample(0, 0.5);
                    buffer.write_sample(1, -0.3);
                    buffer.advance_write_pos();
                }

                b.iter(|| {
                    let _ = buffer.get_recent_samples(0, black_box(count));
                });
            },
        );
    }

    // Benchmark multi-track scenario (8 buffers)
    group.bench_function("write_8_tracks", |b| {
        let mut buffers: Vec<LookaheadBuffer> = (0..8)
            .map(|_| LookaheadBuffer::new(2, 4096))
            .collect();

        b.iter(|| {
            for buffer in &mut buffers {
                buffer.write_sample(0, black_box(0.5));
                buffer.write_sample(1, black_box(-0.3));
                buffer.advance_write_pos();
            }
        });
    });

    group.finish();
}
