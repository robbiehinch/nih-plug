# Phase Analyzer

A stereo phase difference analyzer plugin built with NIH-plug. This tool helps identify phase issues introduced by dual-amp processing, stereo effects, or other audio processing.

## Features

- **Snapshot/Freeze Mode**: Press "Analyze" button to capture and freeze the current phase relationship
- **Phase Range**: Displays phase differences from -180° to +180° (wrapped)
- **Color Gradient Visualization**:
  - Blue (-180°) → Green (0°) → Red (+180°)
  - Green indicates in-phase signals
  - Blue/Red indicates out-of-phase signals
- **Logarithmic Frequency Axis**: 20 Hz to Nyquist frequency
- **Analysis-Only**: Audio passes through unchanged (zero latency)

## How It Works

The plugin uses Short-Time Fourier Transform (STFT) to analyze the frequency content of both left and right channels. For each frequency bin, it calculates the phase difference between the channels and visualizes it using a color gradient.

### Technical Details

- **Window Size**: 2048 samples (~43ms at 48kHz)
- **Frequency Resolution**: ~23 Hz per bin at 48kHz
- **Overlap**: 4x overlap for smooth analysis
- **Window Function**: Hann window
- **FFT Bins**: 1025 frequency bins

## Usage

1. Load the plugin on a stereo track
2. Press the "Analyze" button to capture the current phase relationship
3. The display freezes showing the phase difference across the frequency spectrum
4. Press "Analyze" again to capture a new snapshot

## Interpreting the Display

### Colors
- **Green (0°)**: Signals are in phase - typical for mono content or centered elements
- **Blue (-180°) / Red (+180°)**: Signals are out of phase - may indicate phase cancellation issues
- **Mixed Colors**: Varying phase relationships across frequency spectrum

### Common Patterns

**Mono Signal (L=R)**:
- All green across the spectrum
- Indicates perfectly phase-aligned content

**Phase-Inverted (L=-R)**:
- Blue or red across the spectrum
- Strong phase cancellation - will disappear when summed to mono

**Typical Stereo Music**:
- Green in low frequencies (bass typically mono)
- Mixed colors in highs (stereo width from effects)

**Dual-Amp Phase Issues**:
- Frequency-dependent coloring
- May show phase shifts at specific frequencies due to different amp responses

## Use Cases

1. **Dual-Amp Recording**: Verify phase alignment between two microphones/amps
2. **Stereo Width Effects**: Check how stereo widening affects phase relationships
3. **Mix Bus Analysis**: Ensure mono compatibility of stereo masters
4. **Problem Diagnosis**: Identify frequency ranges with phase issues

## Building

```bash
cargo xtask bundle phase_analyzer --release
```

## Future Enhancements

The plugin architecture is designed to support additional features:
- Real-time scrolling heat map (time vs frequency)
- Continuous monitoring mode
- Phase correlation meter
- Mono compatibility warnings

## License

GPL-3.0-or-later
