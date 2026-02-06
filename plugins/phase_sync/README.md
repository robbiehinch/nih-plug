# Phase Sync

An adaptive phase alignment plugin that automatically aligns bass peaks with kick drum attacks using phase rotation.

## Features

- **Automatic Kick Detection**: Time-domain envelope follower detects kick drum peaks from sidechain input
- **Bass Peak Analysis**: RMS energy analysis identifies bass guitar/synth peaks in the main signal
- **Adaptive Phase Rotation**: All-pass filter chain rotates bass phase to align with kicks
- **Multiple Adaptation Modes**:
  - **Immediate**: Snap to target phase instantly at each kick
  - **Linear Drift**: Gradually transition over the entire kick interval
  - **Exponential**: Slow start, faster transition toward next kick
  - **Last Moment**: Hold current phase, then transition near the end
- **Bass Frequency Focus**: Phase rotation concentrated in 40-250 Hz range
- **Low Latency**: ~85ms lookahead buffer at 48kHz for predictive phase adjustment

## Usage

1. Route your kick drum to the sidechain input
2. Route your bass to the main input
3. Adjust detection parameters until kicks are reliably detected
4. Fine-tune phase rotation amount and frequency range
5. Choose adaptation mode based on musical preference
6. Mix to taste with dry/wet control

## Parameters

### Kick Detection
- **Kick Threshold**: -36 to -6 dB (detection sensitivity)
- **Kick Attack**: 1-10 ms (envelope follower attack time)
- **Kick Release**: 50-500 ms (envelope follower release time)
- **Min Kick Interval**: 50-500 ms (prevents double-triggering)

### Phase Rotation
- **Center Frequency**: 40-250 Hz (bass fundamental frequency)
- **Phase Amount**: 0-100% (rotation intensity)
- **Frequency Spread**: 0.1-2.0 octaves (filter bandwidth)

### Adaptive Behavior
- **Adaptation Mode**: Immediate / Linear / Exponential / Last Moment
- **Transition Threshold**: 10-90% (for Last Moment mode)

### Mix
- **Dry/Wet**: 0-100% (processed signal mix)

### Advanced
- **Bass Window**: 10-100 ms (peak detection window size)

## Tips

- Start with **Kick Threshold** at -18dB and adjust until kicks are reliably detected
- Set **Center Frequency** to match your bass fundamental (typically 60-100 Hz for bass guitar)
- Use **Linear Drift** mode for smooth, natural-sounding alignment
- Try **Last Moment** mode for more aggressive, punchy alignment
- Increase **Frequency Spread** for wider bass instruments or synths
- Lower **Phase Amount** if the effect is too pronounced

## Technical Details

- **Latency**: 4096 samples (~85ms at 48kHz)
- **CPU Usage**: <5% on modern processors
- **Detection Method**: Time-domain envelope follower (near-zero latency)
- **Phase Rotation**: Biquad all-pass filter chain (up to 16 stages)
- **Analysis Window**: 1024-2048 samples for bass peak detection
- **Prediction**: Median-based kick interval prediction (robust to tempo changes)

## License

GPL-3.0-or-later
