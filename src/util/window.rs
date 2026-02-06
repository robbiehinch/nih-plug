//! Windowing functions, useful in conjunction with [`StftHelper`][super::StftHelper].

use std::f32;

/// A Blackman window function with the 'standard' coefficients.
///
/// <https://en.wikipedia.org/wiki/Window_function#Blackman_window>
pub fn blackman(size: usize) -> Vec<f32> {
    let mut window = vec![0.0; size];
    blackman_in_place(&mut window);

    window
}

/// The same as [`blackman()`], but filling an existing slice instead. asfasdf
pub fn blackman_in_place(window: &mut [f32]) {
    let size = window.len();

    let scale_1 = (2.0 * f32::consts::PI) / (size - 1) as f32;
    let scale_2 = scale_1 * 2.0;
    for (i, sample) in window.iter_mut().enumerate() {
        let cos_1 = (scale_1 * i as f32).cos();
        let cos_2 = (scale_2 * i as f32).cos();
        *sample = 0.42 - (0.5 * cos_1) + (0.08 * cos_2);
    }
}

/// A Hann window function.
///
/// <https://en.wikipedia.org/wiki/Hann_function>
pub fn hann(size: usize) -> Vec<f32> {
    let mut window = vec![0.0; size];
    hann_in_place(&mut window);

    window
}

/// The same as [`hann()`], but filling an existing slice instead.
pub fn hann_in_place(window: &mut [f32]) {
    let size = window.len();

    // We want to scale `[0, size - 1]` to `[0, pi]`.
    // XXX: The `sin^2()` version results in weird rounding errors that cause spectral leakage
    let scale = (size as f32 - 1.0).recip() * f32::consts::TAU;
    for (i, sample) in window.iter_mut().enumerate() {
        let cos = (i as f32 * scale).cos();
        *sample = 0.5 - (0.5 * cos)
    }
}

/// Multiply a buffer with a window function.
#[inline]
pub fn multiply_with_window(buffer: &mut [f32], window_function: &[f32]) {
    // TODO: ALso use SIMD here if available
    for (sample, window_sample) in buffer.iter_mut().zip(window_function) {
        *sample *= window_sample;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Blackman window tests
    #[test]
    fn blackman_mathematical_correctness() {
        let size = 512;
        let window = blackman(size);

        // Test formula at key points: 0.42 - 0.5*cos(2π*i/(n-1)) + 0.08*cos(4π*i/(n-1))
        let scale_1 = (2.0 * f32::consts::PI) / (size - 1) as f32;
        let scale_2 = scale_1 * 2.0;

        for i in [0, size / 4, size / 2, 3 * size / 4, size - 1] {
            let expected = 0.42 - (0.5 * (scale_1 * i as f32).cos()) + (0.08 * (scale_2 * i as f32).cos());
            approx::assert_relative_eq!(window[i], expected, epsilon = 1e-6);
        }
    }

    #[test]
    fn blackman_symmetry() {
        let size = 512;
        let window = blackman(size);

        // Window should be symmetric: window[i] ≈ window[size-1-i]
        for i in 0..size / 2 {
            approx::assert_relative_eq!(window[i], window[size - 1 - i], epsilon = 1e-6);
        }
    }

    #[test]
    fn blackman_edge_cases() {
        // Test various sizes (skip size=1 as it produces NaN due to division by zero)
        for size in [2, 4, 64, 512, 1024] {
            let window = blackman(size);
            assert_eq!(window.len(), size);

            // All values should be between 0 and 1 (with small tolerance for floating point errors)
            for &value in &window {
                assert!(value >= -1e-6 && value <= 1.0, "Window value {} out of range [0, 1]", value);
            }
        }
    }

    #[test]
    fn blackman_boundary_values() {
        let size = 512;
        let window = blackman(size);

        // First and last values should be close to 0
        approx::assert_relative_eq!(window[0], 0.0, epsilon = 1e-6);
        approx::assert_relative_eq!(window[size - 1], 0.0, epsilon = 1e-6);

        // Center value should be close to 1.0
        approx::assert_relative_eq!(window[size / 2], 1.0, epsilon = 0.01);
    }

    #[test]
    fn blackman_consistency() {
        let size = 512;
        let window1 = blackman(size);

        let mut window2 = vec![0.0; size];
        blackman_in_place(&mut window2);

        // blackman() and blackman_in_place() should produce identical results
        for i in 0..size {
            approx::assert_relative_eq!(window1[i], window2[i], epsilon = 1e-9);
        }
    }

    // Hann window tests
    #[test]
    fn hann_mathematical_correctness() {
        let size = 512;
        let window = hann(size);

        // Test formula at key points: 0.5 - 0.5*cos(2π*i/(n-1))
        let scale = (size as f32 - 1.0).recip() * f32::consts::TAU;

        for i in [0, size / 4, size / 2, 3 * size / 4, size - 1] {
            let expected = 0.5 - (0.5 * (i as f32 * scale).cos());
            approx::assert_relative_eq!(window[i], expected, epsilon = 1e-6);
        }
    }

    #[test]
    fn hann_symmetry() {
        let size = 512;
        let window = hann(size);

        // Window should be symmetric around center
        for i in 0..size / 2 {
            approx::assert_relative_eq!(window[i], window[size - 1 - i], epsilon = 1e-6);
        }
    }

    #[test]
    fn hann_edge_cases() {
        // Test various sizes (skip size=1 as it produces NaN due to division by zero)
        for size in [2, 4, 64, 512, 1024] {
            let window = hann(size);
            assert_eq!(window.len(), size);

            // All values should be between 0 and 1
            for &value in &window {
                assert!(value >= 0.0 && value <= 1.0, "Window value {} out of range [0, 1]", value);
            }
        }
    }

    #[test]
    fn hann_boundary_values() {
        let size = 512;
        let window = hann(size);

        // First and last values should be 0.0
        approx::assert_relative_eq!(window[0], 0.0, epsilon = 1e-6);
        approx::assert_relative_eq!(window[size - 1], 0.0, epsilon = 1e-6);

        // Center value should be very close to 1.0 (with floating point tolerance)
        approx::assert_relative_eq!(window[size / 2], 1.0, epsilon = 1e-4);
    }

    #[test]
    fn hann_consistency() {
        let size = 512;
        let window1 = hann(size);

        let mut window2 = vec![0.0; size];
        hann_in_place(&mut window2);

        // hann() and hann_in_place() should produce identical results
        for i in 0..size {
            approx::assert_relative_eq!(window1[i], window2[i], epsilon = 1e-9);
        }
    }

    // multiply_with_window tests
    #[test]
    fn multiply_basic() {
        let mut buffer = vec![1.0, 2.0, 3.0, 4.0];
        let window = vec![0.5, 0.5, 0.5, 0.5];

        multiply_with_window(&mut buffer, &window);

        assert_eq!(buffer, vec![0.5, 1.0, 1.5, 2.0]);
    }

    #[test]
    fn multiply_identity() {
        let mut buffer = vec![1.0, 2.0, 3.0, 4.0];
        let original = buffer.clone();
        let window = vec![1.0, 1.0, 1.0, 1.0];

        multiply_with_window(&mut buffer, &window);

        // All-ones window should preserve signal
        assert_eq!(buffer, original);
    }

    #[test]
    fn multiply_zero_suppression() {
        let mut buffer = vec![1.0, 2.0, 3.0, 4.0];
        let window = vec![0.0, 0.0, 0.0, 0.0];

        multiply_with_window(&mut buffer, &window);

        // All-zeros window should zero out signal
        assert_eq!(buffer, vec![0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn multiply_partial() {
        let mut buffer = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let window = vec![0.5, 0.5, 0.5];

        multiply_with_window(&mut buffer, &window);

        // Window shorter than buffer: only first 3 samples affected
        assert_eq!(buffer, vec![0.5, 1.0, 1.5, 4.0, 5.0]);
    }

    #[test]
    fn multiply_empty() {
        let mut buffer: Vec<f32> = vec![];
        let window: Vec<f32> = vec![];

        multiply_with_window(&mut buffer, &window);

        // Empty buffers should not panic
        assert_eq!(buffer.len(), 0);
    }
}
