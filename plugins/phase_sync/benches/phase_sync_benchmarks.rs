use criterion::{criterion_group, criterion_main};

mod kick_detector_benches;
mod bass_analyzer_benches;
mod phase_rotator_benches;
mod lookahead_buffer_benches;
mod phase_controller_benches;
mod integration_benches;

criterion_group!(
    benches,
    kick_detector_benches::bench_kick_detector,
    bass_analyzer_benches::bench_bass_analyzer,
    phase_rotator_benches::bench_phase_rotator,
    lookahead_buffer_benches::bench_lookahead_buffer,
    phase_controller_benches::bench_phase_controller,
    integration_benches::bench_full_process,
);

criterion_main!(benches);
