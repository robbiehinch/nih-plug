//! Abstractions for the parameterized threshold curve.
//!
//! This was previously computed directly inside of the `CompressorBank` but this makes it easier to
//! reuse it when drawing the GUI.

/// Parameters for a curve, similar to the fields found in `ThresholdParams` but using plain floats
/// instead of parameters.
#[derive(Debug, Default, Clone, Copy)]
pub struct CurveParams {
    /// The compressor threshold at the center frequency. When sidechaining is enabled, the input
    /// signal is gained by the inverse of this value. This replaces the input gain in the original
    /// Spectral Compressor. In the polynomial below, this is the intercept.
    pub intercept: f32,
    /// The center frqeuency for the target curve when sidechaining is not enabled. The curve is a
    /// polynomial `threshold_db + curve_slope*x + curve_curve*(x^2)` that evaluates to a decibel
    /// value, where `x = ln(center_frequency) - ln(bin_frequency)`. In other words, this is
    /// evaluated in the log/log domain for decibels and octaves.
    pub center_frequency: f32,
    /// The slope for the curve, in the log/log domain. See the polynomial above.
    pub slope: f32,
    /// The, uh, 'curve' for the curve, in the logarithmic domain. This is the third coefficient in
    /// the quadratic polynomial and controls the parabolic behavior. Positive values turn the curve
    /// into a v-shaped curve, while negative values attenuate everything outside of the center
    /// frequency. See the polynomial above.
    pub curve: f32,
}

/// Evaluates the quadratic threshold curve. This used to be calculated directly inside of the
/// compressor bank since it's so simple, but the editor also needs to compute this so it makes
/// sense to deduplicate it a bit.
///
/// The curve is evaluated in log-log space (so with octaves being the independent variable and gain
/// in decibels being the output of the equation).
pub struct Curve<'a> {
    params: &'a CurveParams,
    /// The natural logarithm of [`CurveParams::cemter_frequency`].
    ln_center_frequency: f32,
}

impl<'a> Curve<'a> {
    pub fn new(params: &'a CurveParams) -> Self {
        Self {
            params,
            ln_center_frequency: params.center_frequency.ln(),
        }
    }

    /// Evaluate the curve for the natural logarithm of the frequency value. This can be used as an
    /// optimization to avoid computing these logarithms all the time.
    #[inline]
    pub fn evaluate_ln(&self, ln_freq: f32) -> f32 {
        let offset = ln_freq - self.ln_center_frequency;
        self.params.intercept + (self.params.slope * offset) + (self.params.curve * offset * offset)
    }

    /// Evaluate the curve for a value in Hertz.
    #[inline]
    #[allow(unused)]
    pub fn evaluate_linear(&self, freq: f32) -> f32 {
        self.evaluate_ln(freq.ln())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx;

    fn make_params(intercept: f32, center_frequency: f32, slope: f32, curve: f32) -> CurveParams {
        CurveParams {
            intercept,
            center_frequency,
            slope,
            curve,
        }
    }

    #[test]
    fn evaluate_ln_at_center_returns_intercept() {
        let params = make_params(-20.0, 1000.0, 5.0, 2.0);
        let curve = Curve::new(&params);

        // At center frequency, offset = 0, so result should equal intercept
        let ln_center = 1000.0_f32.ln();
        let result = curve.evaluate_ln(ln_center);

        approx::assert_relative_eq!(result, -20.0, epsilon = 1e-6);
    }

    #[test]
    fn evaluate_ln_one_octave_above_center() {
        // One octave above means frequency * 2, so ln(2*f) - ln(f) = ln(2)
        let params = make_params(-20.0, 1000.0, 5.0, 0.0); // curve = 0 to isolate slope
        let curve = Curve::new(&params);

        let ln_double = 2000.0_f32.ln();
        let result = curve.evaluate_ln(ln_double);

        // offset = ln(2000) - ln(1000) = ln(2)
        // result = intercept + slope * ln(2) = -20 + 5 * ln(2)
        let expected = -20.0 + 5.0 * 2.0_f32.ln();
        approx::assert_relative_eq!(result, expected, epsilon = 1e-6);
    }

    #[test]
    fn evaluate_ln_quadratic_term_behavior() {
        // Test that the curve coefficient adds parabolic behavior
        let params = make_params(0.0, 1000.0, 0.0, 2.0); // Only curve term active
        let curve = Curve::new(&params);

        // One octave above: offset = ln(2)
        let ln_double = 2000.0_f32.ln();
        let result_above = curve.evaluate_ln(ln_double);
        let expected_above = 2.0 * 2.0_f32.ln() * 2.0_f32.ln();
        approx::assert_relative_eq!(result_above, expected_above, epsilon = 1e-6);

        // One octave below: offset = -ln(2), but squared so same magnitude
        let ln_half = 500.0_f32.ln();
        let result_below = curve.evaluate_ln(ln_half);
        // offset = ln(500) - ln(1000) = -ln(2)
        // result = 2 * (-ln(2))^2 = 2 * ln(2)^2 (same as above)
        approx::assert_relative_eq!(result_below, expected_above, epsilon = 1e-6);
    }

    #[test]
    fn evaluate_linear_matches_evaluate_ln() {
        let params = make_params(-15.0, 500.0, 3.0, 1.5);
        let curve = Curve::new(&params);

        let freq = 750.0_f32;
        let result_linear = curve.evaluate_linear(freq);
        let result_ln = curve.evaluate_ln(freq.ln());

        approx::assert_relative_eq!(result_linear, result_ln, epsilon = 1e-6);
    }
}
