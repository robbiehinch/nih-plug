# Phase Sync Performance Benchmarks

This document describes the benchmarking suite for the Phase Sync plugin, which provides comprehensive performance testing for all critical components.

## Overview

The benchmark suite measures:
- **Individual component efficiency** (microbenchmarks)
- **Full processing path performance** (integration benchmarks)
- **Scalability** with varying track counts (1-8)
- **Memory allocation patterns**

## Running Benchmarks

### Run All Benchmarks

```bash
# From the nih-plug root directory
cargo bench --package phase_sync
```

### Run Specific Benchmark Groups

```bash
# Kick detector benchmarks
cargo bench --package phase_sync --bench phase_sync_benchmarks -- kick_detector

# Bass analyzer benchmarks
cargo bench --package phase_sync --bench phase_sync_benchmarks -- bass_analyzer

# Phase rotator benchmarks
cargo bench --package phase_sync --bench phase_sync_benchmarks -- phase_rotator

# Lookahead buffer benchmarks
cargo bench --package phase_sync --bench phase_sync_benchmarks -- lookahead_buffer

# Phase controller benchmarks
cargo bench --package phase_sync --bench phase_sync_benchmarks -- phase_controller

# Integration benchmarks (full processing)
cargo bench --package phase_sync --bench phase_sync_benchmarks -- integration
```

### Baseline Comparison

Criterion supports saving baselines and comparing against them:

```bash
# Save initial baseline
cargo bench --package phase_sync -- --save-baseline initial

# Make changes to code...

# Compare against baseline
cargo bench --package phase_sync -- --baseline initial
```

This will show percentage improvements or regressions for each benchmark.

## Viewing Results

### HTML Reports

Criterion generates detailed HTML reports with graphs:

```
target/criterion/report/index.html
```

Open this file in a browser to see:
- Time series plots showing performance over multiple runs
- Statistical distribution plots
- Comparison charts (when using baselines)

### Console Output

Terminal output shows:
- **time**: Mean execution time per iteration
- **thrpt**: Throughput (samples/second for relevant benchmarks)
- **R²**: Statistical goodness of fit (>0.99 is excellent)
- **mean**: Average time with confidence interval
- **std dev**: Standard deviation of measurements

## Profiling with Flamegraphs

For detailed CPU profiling, use flamegraphs to visualize where time is spent:

### Install Flamegraph Tool

```bash
cargo install flamegraph
```

### Generate Flamegraph

```bash
# Profile all benchmarks
cargo flamegraph --release --bench phase_sync_benchmarks

# Profile specific benchmark group
cargo flamegraph --release --bench phase_sync_benchmarks -- kick_detector

# Output: flamegraph.svg
```

Open the generated `flamegraph.svg` in a browser to see an interactive call stack visualization.

### On Windows

Flamegraph requires Linux/macOS. On Windows:
1. Use WSL (Windows Subsystem for Linux)
2. Install perf: `sudo apt-get install linux-tools-generic`
3. Run flamegraph commands in WSL

## Memory Profiling

To profile memory allocations and identify allocation hotspots:

### Using Valgrind (Linux/WSL)

```bash
# Build benchmarks in release mode
cargo build --release --package phase_sync --benches

# Run with massif (heap profiler)
valgrind --tool=massif --massif-out-file=massif.out \
  ./target/release/deps/phase_sync_benchmarks-*

# Generate report
ms_print massif.out > memory_profile.txt
```

### Using DHAT (Rust-based)

Add to `Cargo.toml`:
```toml
[dev-dependencies]
dhat = "0.3"
```

Instrument code with `#[global_allocator]` to track allocations.

## Benchmark Groups

### 1. Kick Detector (`kick_detector`)

Measures kick detection and envelope follower performance:

- **process_single_sample**: Per-sample envelope processing cost
- **adaptive_threshold/N**: Threshold computation with N peaks in history
- **process_buffer_512**: Full 512-sample buffer processing

**Key Metrics**: Should process <500ns per sample for real-time performance.

### 2. Bass Analyzer (`bass_analyzer`)

Measures RMS peak detection performance:

- **find_peak/Nms**: Peak detection with N millisecond window size (10-100ms)
- **buffer_size/N**: Peak detection with varying buffer sizes (512-4096)

**Key Metrics**: O(N×M) complexity - larger windows and buffers scale quadratically.

### 3. Phase Rotator (`phase_rotator`)

Measures biquad all-pass filter cascade performance:

- **process_stereo_sample/N**: Process single sample with N filters active (1-16)
- **process_buffer_512**: Full 512-sample stereo buffer processing
- **update_coefficients**: Cost of recalculating filter coefficients

**Key Metrics**: Should process <5μs per sample with 16 filters. SIMD provides ~2x speedup.

### 4. Lookahead Buffer (`lookahead_buffer`)

Measures circular buffer operations:

- **write_single_sample**: Write stereo sample and advance pointer
- **read_single_sample**: Read with delay offset
- **get_recent_samples/N**: Allocate and copy N recent samples
- **write_8_tracks**: Write to 8 buffers simultaneously

**Key Metrics**: `get_recent_samples` allocates Vec - potential optimization target.

### 5. Phase Controller (`phase_controller`)

Measures phase calculation and adaptation:

- **calculate_phase/Mode**: Phase interpolation for each adaptation mode
- **on_kick_detected**: Kick timing update (includes median calculation)
- **update_sample_counter**: Per-sample timeout logic

**Key Metrics**: Different modes have different computational costs.

### 6. Integration (`integration`)

Measures full realistic processing:

- **process_block/N_tracks**: Full 512-sample block with 1-8 bass tracks

**Key Metrics**: Should scale linearly with track count. Target <20μs per sample for 8 tracks.

## Performance Targets

At 48kHz sample rate, for real-time audio processing:

| Component | Target | Notes |
|-----------|--------|-------|
| **Kick Detector** | <500 ns/sample | Envelope follower + detection |
| **Bass Analyzer** | <100 μs per kick | Triggered only on kick (not per-sample) |
| **Phase Rotator (16 filters)** | <5 μs/sample | SIMD-optimized biquad cascade |
| **Lookahead Buffer** | <100 ns/sample | Simple circular buffer ops |
| **Phase Controller** | <200 ns/sample | Phase interpolation |
| **Full Process (8 tracks)** | <20 μs/sample | 50% CPU headroom at 512 buffer |

**Real-time constraint**: At 48kHz with 512-sample buffer, we have 10.67ms to process each block.
- Per-sample budget: 21.8 μs (if processing serially)
- With 8 tracks: 2.7 μs per track per sample

## Expected Insights

Based on architectural analysis, benchmarks typically reveal:

1. **Phase rotator dominates** per-sample cost (16 biquads × SIMD ops)
2. **Bass analyzer is O(N×M)** - window size has quadratic impact on performance
3. **Lookahead `get_recent_samples`** allocates Vec on every kick (potential optimization)
4. **Multi-track scales linearly** when implemented correctly
5. **Kick detector sorting** (16 elements median) is negligible vs. per-sample envelope
6. **SIMD provides ~2x speedup** for stereo processing vs. scalar operations

## Optimization Workflow

1. **Establish baseline**: Run benchmarks and save baseline
2. **Identify hotspots**: Use flamegraphs to find bottlenecks
3. **Make targeted changes**: Optimize specific functions
4. **Compare against baseline**: Verify improvements with criterion
5. **Test in DAW**: Confirm real-world performance improvement

## Common Optimization Strategies

- **Reduce allocations**: Pool buffers, use `&mut` instead of returning `Vec`
- **SIMD**: Vectorize stereo processing with `f32x2`
- **Cache coefficients**: Avoid recomputing biquad coefficients unnecessarily
- **Algorithmic improvements**: Reduce O(N²) to O(N log N) or O(N)
- **Lazy updates**: Only update when parameters change significantly

## Interpreting Statistical Results

Criterion provides robust statistical analysis:

- **Mean**: Average execution time (most important metric)
- **Std Dev**: Variability in measurements (lower is more stable)
- **R²**: How well measurements fit linear model (>0.99 is excellent)
- **Outliers**: Samples that deviate significantly (investigate if many outliers)
- **Change**: Percentage improvement/regression vs. baseline

**Green** = improvement, **Red** = regression, **Gray** = no significant change

## Continuous Performance Testing

To prevent performance regressions:

1. Run benchmarks before major refactors
2. Save baselines for each release
3. Compare new changes against baselines
4. Investigate any regression >5%
5. Document performance changes in commit messages

## Contact

For questions about benchmarking or performance optimization, see the main repository documentation.
