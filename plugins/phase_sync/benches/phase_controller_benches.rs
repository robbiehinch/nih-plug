use criterion::{black_box, Criterion, BenchmarkId};
use phase_sync::bench_helpers::{AdaptationMode, PhaseController};

pub fn bench_phase_controller(c: &mut Criterion) {
    let mut group = c.benchmark_group("phase_controller");

    // Benchmark phase calculation per sample for different adaptation modes
    let modes = [
        ("Immediate", AdaptationMode::Immediate),
        ("LinearDrift", AdaptationMode::LinearDrift),
        ("ExponentialDrift", AdaptationMode::ExponentialDrift),
        ("LastMoment", AdaptationMode::LastMoment),
    ];

    for (mode_name, mode) in modes {
        group.bench_with_input(
            BenchmarkId::new("calculate_phase", mode_name),
            &mode,
            |b, &mode| {
                let mut controller = PhaseController::new();
                controller.on_kick_detected(0, 1000, 100.0, 48000.0, 0.0); // Set target

                b.iter(|| {
                    controller.get_current_phase(black_box(100), mode, 0.8)
                });
            },
        );
    }

    // Benchmark on_kick_detected (includes interval prediction and median calculation)
    group.bench_function("on_kick_detected", |b| {
        let mut controller = PhaseController::new();

        // Build up kick history
        for i in 0..8 {
            controller.on_kick_detected(i * 48000, (i * 48000) + 1000, 100.0, 48000.0, 0.0);
        }
        let mut kick_sample = 8 * 48000;

        b.iter(|| {
            controller.on_kick_detected(
                black_box(kick_sample),
                black_box(kick_sample + 1000),
                100.0,
                48000.0,
                0.0,
            );
            kick_sample += 48000;
        });
    });

    // Benchmark update_sample_counter (includes timeout logic)
    group.bench_function("update_sample_counter", |b| {
        let mut controller = PhaseController::new();
        controller.on_kick_detected(0, 1000, 100.0, 48000.0, 0.0);

        b.iter(|| {
            controller.update_sample_counter(48000.0);
        });
    });

    group.finish();
}
