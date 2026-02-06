use std::f32::consts;
use std::ops::{Add, Mul, Sub};

#[cfg(feature = "simd")]
use std::simd::f32x2;

#[cfg(not(feature = "simd"))]
#[derive(Clone, Copy, Debug)]
pub struct f32x2 {
    data: [f32; 2],
}

#[cfg(not(feature = "simd"))]
impl f32x2 {
    pub fn splat(value: f32) -> Self {
        Self { data: [value, value] }
    }

    pub fn from_array(array: [f32; 2]) -> Self {
        Self { data: array }
    }

    pub fn as_array(&self) -> &[f32; 2] {
        &self.data
    }
}

#[cfg(not(feature = "simd"))]
impl std::ops::Mul for f32x2 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self {
            data: [self.data[0] * rhs.data[0], self.data[1] * rhs.data[1]],
        }
    }
}

#[cfg(not(feature = "simd"))]
impl std::ops::Add for f32x2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            data: [self.data[0] + rhs.data[0], self.data[1] + rhs.data[1]],
        }
    }
}

#[cfg(not(feature = "simd"))]
impl std::ops::Sub for f32x2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            data: [self.data[0] - rhs.data[0], self.data[1] - rhs.data[1]],
        }
    }
}

/// A simple biquad filter implementation for all-pass filtering
#[derive(Clone, Copy, Debug)]
pub struct Biquad<T> {
    pub coefficients: BiquadCoefficients<T>,
    s1: T,
    s2: T,
}

/// The coefficients `[b0, b1, b2, a1, a2]` for [`Biquad`]. These coefficients are all
/// prenormalized, i.e. they have been divided by `a0`.
#[derive(Clone, Copy, Debug)]
pub struct BiquadCoefficients<T> {
    b0: T,
    b1: T,
    b2: T,
    a1: T,
    a2: T,
}

/// Either an `f32` or some SIMD vector type of `f32`s that can be used with our biquads.
pub trait SimdType:
    Mul<Output = Self> + Sub<Output = Self> + Add<Output = Self> + Copy + Sized
{
    fn from_f32(value: f32) -> Self;
}

impl<T: SimdType> Default for Biquad<T> {
    fn default() -> Self {
        Self {
            coefficients: BiquadCoefficients::identity(),
            s1: T::from_f32(0.0),
            s2: T::from_f32(0.0),
        }
    }
}

impl<T: SimdType> Biquad<T> {
    /// Process a single sample.
    pub fn process(&mut self, sample: T) -> T {
        let result = self.coefficients.b0 * sample + self.s1;

        self.s1 = self.coefficients.b1 * sample - self.coefficients.a1 * result + self.s2;
        self.s2 = self.coefficients.b2 * sample - self.coefficients.a2 * result;

        result
    }

    /// Reset the state to zero
    pub fn reset(&mut self) {
        self.s1 = T::from_f32(0.0);
        self.s2 = T::from_f32(0.0);
    }
}

impl<T: SimdType> BiquadCoefficients<T> {
    /// Convert scalar coefficients into the correct vector type.
    pub fn from_f32s(scalar: BiquadCoefficients<f32>) -> Self {
        Self {
            b0: T::from_f32(scalar.b0),
            b1: T::from_f32(scalar.b1),
            b2: T::from_f32(scalar.b2),
            a1: T::from_f32(scalar.a1),
            a2: T::from_f32(scalar.a2),
        }
    }

    /// Filter coefficients for identity (pass-through).
    pub fn identity() -> Self {
        Self::from_f32s(BiquadCoefficients {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        })
    }

    /// Compute the coefficients for an all-pass filter.
    pub fn allpass(sample_rate: f32, frequency: f32, q: f32) -> Self {
        let omega0 = consts::TAU * (frequency / sample_rate);
        let cos_omega0 = omega0.cos();
        let alpha = omega0.sin() / (2.0 * q);

        // Prenormalize everything with a0
        let a0 = 1.0 + alpha;
        let b0 = (1.0 - alpha) / a0;
        let b1 = (-2.0 * cos_omega0) / a0;
        let b2 = (1.0 + alpha) / a0;
        let a1 = (-2.0 * cos_omega0) / a0;
        let a2 = (1.0 - alpha) / a0;

        Self::from_f32s(BiquadCoefficients { b0, b1, b2, a1, a2 })
    }
}

impl SimdType for f32 {
    #[inline(always)]
    fn from_f32(value: f32) -> Self {
        value
    }
}

#[cfg(feature = "simd")]
impl SimdType for f32x2 {
    #[inline(always)]
    fn from_f32(value: f32) -> Self {
        f32x2::splat(value)
    }
}

#[cfg(not(feature = "simd"))]
impl SimdType for f32x2 {
    #[inline(always)]
    fn from_f32(value: f32) -> Self {
        f32x2::splat(value)
    }
}

/// Phase rotator using a chain of all-pass biquad filters
pub struct PhaseRotator {
    // Biquad all-pass filters (SIMD vectorized for stereo)
    filters: Vec<Biquad<f32x2>>,

    // Number of active filters (determines phase range)
    num_active_filters: usize,

    // Center frequency and bandwidth for bass focus
    center_frequency: f32,
    frequency_spread_octaves: f32,

    // Current phase rotation amount (degrees)
    current_phase_degrees: f32,

    // Track if we need to update coefficients
    needs_update: bool,
}

impl PhaseRotator {
    pub fn new() -> Self {
        Self {
            filters: vec![Biquad::default(); 16], // Up to 16 stages for wider phase range
            num_active_filters: 0,
            center_frequency: 100.0,
            frequency_spread_octaves: 0.5,
            current_phase_degrees: 0.0,
            needs_update: false,
        }
    }

    pub fn process(&mut self, sample: f32x2) -> f32x2 {
        let mut output = sample;
        for filter in self.filters.iter_mut().take(self.num_active_filters) {
            output = filter.process(output);
        }
        output
    }

    pub fn set_target_phase(&mut self, target_phase_degrees: f32) {
        if (self.current_phase_degrees - target_phase_degrees).abs() > 0.1 {
            self.current_phase_degrees = target_phase_degrees;
            self.needs_update = true;
        }
    }

    pub fn set_center_frequency(&mut self, frequency: f32) {
        if (self.center_frequency - frequency).abs() > 0.1 {
            self.center_frequency = frequency;
            self.needs_update = true;
        }
    }

    pub fn set_frequency_spread(&mut self, spread_octaves: f32) {
        if (self.frequency_spread_octaves - spread_octaves).abs() > 0.01 {
            self.frequency_spread_octaves = spread_octaves;
            self.needs_update = true;
        }
    }

    pub fn update_if_needed(&mut self, sample_rate: f32) {
        if !self.needs_update {
            return;
        }

        self.update_coefficients(sample_rate);
        self.needs_update = false;
    }

    fn update_coefficients(&mut self, sample_rate: f32) {
        // Calculate number of filter stages needed
        // Each stage can provide up to ~90 degrees of phase shift
        let num_filters = (self.current_phase_degrees.abs() / 90.0).ceil() as usize;
        let new_num_active = num_filters.min(16);

        // If number of active filters changed significantly, reset filter states
        if (self.num_active_filters as i32 - new_num_active as i32).abs() > 2 {
            for filter in &mut self.filters {
                filter.reset();
            }
        }

        self.num_active_filters = new_num_active;

        if self.num_active_filters == 0 {
            return;
        }

        // Spread filters around center frequency for smooth response
        for filter_idx in 0..self.num_active_filters {
            let spread_factor = if self.num_active_filters > 1 {
                (filter_idx as f32 / (self.num_active_filters - 1) as f32) * 2.0 - 1.0
            } else {
                0.0
            };

            let filter_freq = self.center_frequency
                * 2.0f32.powf(self.frequency_spread_octaves * spread_factor);
            let filter_freq = filter_freq.clamp(40.0, 250.0); // Bass range only
            let q = 0.707; // Butterworth Q

            self.filters[filter_idx].coefficients =
                BiquadCoefficients::allpass(sample_rate, filter_freq, q);
        }
    }

    pub fn reset(&mut self) {
        for filter in &mut self.filters {
            filter.reset();
        }
        self.current_phase_degrees = 0.0;
        self.num_active_filters = 0;
        self.needs_update = false;
    }
}
