use criterion::{black_box, Criterion, BenchmarkId};
use phase_sync::bench_helpers::BassAnalyzer;

pub fn bench_bass_analyzer(c: &mut Criterion) {
    let mut group = c.benchmark_group("bass_analyzer");

    // Benchmark peak detection with varying window sizes (in samples at 48kHz)
    // 10ms = 480, 25ms = 1200, 50ms = 2400, 100ms = 4800
    for window_ms in [10.0, 25.0, 50.0, 100.0] {
        group.bench_with_input(
            BenchmarkId::new("find_peak", format!("{}ms", window_ms)),
            &window_ms,
            |b, &window_ms| {
                let mut analyzer = BassAnalyzer::new(1024);
                let window_samples = (window_ms * 48.0) as usize; // 48 samples per ms
                analyzer.set_window_size(window_samples);
                let buffer_size = 4096; // Full lookahead buffer
                let buffer = vec![0.3f32; buffer_size];

                b.iter(|| {
                    analyzer.analyze_bass_timing(black_box(&buffer), black_box(0))
                });
            },
        );
    }

    // Benchmark with varying buffer sizes (stress test O(N×M) complexity)
    for buffer_size in [512, 1024, 2048, 4096] {
        group.bench_with_input(
            BenchmarkId::new("buffer_size", buffer_size),
            &buffer_size,
            |b, &size| {
                let mut analyzer = BassAnalyzer::new(1024);
                let window_samples = 1200; // 25ms window
                analyzer.set_window_size(window_samples);
                let buffer = vec![0.3f32; size];

                b.iter(|| {
                    analyzer.analyze_bass_timing(black_box(&buffer), black_box(0))
                });
            },
        );
    }

    // === OPTIMIZED VERSION BENCHMARKS ===
    // Benchmark optimized sliding window version with varying window sizes
    for window_ms in [10.0, 25.0, 50.0, 100.0] {
        group.bench_with_input(
            BenchmarkId::new("find_peak_optimized", format!("{}ms", window_ms)),
            &window_ms,
            |b, &window_ms| {
                let mut analyzer = BassAnalyzer::new(1024);
                let window_samples = (window_ms * 48.0) as usize;
                analyzer.set_window_size(window_samples);
                let buffer_size = 4096;
                let buffer = vec![0.3f32; buffer_size];

                b.iter(|| {
                    analyzer.analyze_bass_timing_optimized(black_box(&buffer), black_box(0))
                });
            },
        );
    }

    // Benchmark optimized version with varying buffer sizes
    for buffer_size in [512, 1024, 2048, 4096] {
        group.bench_with_input(
            BenchmarkId::new("buffer_size_optimized", buffer_size),
            &buffer_size,
            |b, &size| {
                let mut analyzer = BassAnalyzer::new(1024);
                let window_samples = 1200; // 25ms window
                analyzer.set_window_size(window_samples);
                let buffer = vec![0.3f32; size];

                b.iter(|| {
                    analyzer.analyze_bass_timing_optimized(black_box(&buffer), black_box(0))
                });
            },
        );
    }

    group.finish();
}
