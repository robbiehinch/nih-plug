use atomic_float::AtomicF32;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use std::sync::{Arc, Mutex};

use crate::phase_data::PhaseData;

const BORDER_WIDTH: f32 = 2.0;

pub struct PhaseAnalyzer {
    phase_data: Arc<Mutex<triple_buffer::Output<PhaseData>>>,
    sample_rate: Arc<AtomicF32>,
}

impl PhaseAnalyzer {
    pub fn new<L1, L2>(
        cx: &mut Context,
        phase_data: L1,
        sample_rate: L2,
    ) -> Handle<Self>
    where
        L1: Lens<Target = Arc<Mutex<triple_buffer::Output<PhaseData>>>>,
        L2: Lens<Target = Arc<AtomicF32>>,
    {
        Self {
            phase_data: phase_data.get(cx),
            sample_rate: sample_rate.get(cx),
        }
        .build(cx, |_cx| {})
    }
}

impl View for PhaseAnalyzer {
    fn element(&self) -> Option<&'static str> {
        Some("phase-analyzer")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();

        // Read latest phase data from triple buffer
        let mut phase_data_output = self.phase_data.lock().unwrap();
        let data = phase_data_output.read();

        // Draw the phase spectrum
        draw_phase_spectrum(cx, canvas, data, bounds);

        // Draw border
        draw_border(cx, canvas, bounds);
    }
}

/// Draw the phase spectrum as vertical colored bars
fn draw_phase_spectrum(
    _cx: &mut DrawContext,
    canvas: &mut Canvas,
    data: &PhaseData,
    bounds: BoundingBox,
) {
    let nyquist = data.sample_rate / 2.0;

    // Logarithmic frequency mapping helpers
    let bin_frequency = |bin_idx: f32| (bin_idx / data.num_bins as f32) * nyquist;
    let freq_to_x = |freq: f32| {
        if freq <= 20.0 {
            return bounds.x;
        }
        let t = (freq.ln() - 20.0f32.ln()) / (nyquist.ln() - 20.0f32.ln());
        bounds.x + (bounds.w * t)
    };

    // Calculate bar width based on frequency spacing
    let bar_width = bounds.w / 300.0; // Approximate number of visual bars

    // Draw vertical bars for each frequency bin
    for (bin_idx, &phase_diff) in data
        .phase_differences
        .iter()
        .enumerate()
        .take(data.num_bins)
    {
        let freq = bin_frequency(bin_idx as f32);
        if freq < 20.0 || freq > nyquist {
            continue;
        }

        let x = freq_to_x(freq);
        let color = phase_to_color(phase_diff);

        // Draw full-height bar
        let mut path = vg::Path::new();
        path.rect(x, bounds.y, bar_width, bounds.h);

        let paint = vg::Paint::color(color);
        canvas.fill_path(&path, &paint);
    }

    // Draw frequency labels
    draw_frequency_labels(canvas, bounds, nyquist);
}

/// Map phase in radians [-π, π] to color
/// Blue (-180°) → Green (0°) → Red (+180°)
fn phase_to_color(phase_rad: f32) -> vg::Color {
    use std::f32::consts::PI;

    // Map [-π, π] to [0, 1]
    let t = (phase_rad + PI) / (2.0 * PI);

    if t < 0.5 {
        // Blue (-180°) to Green (0°)
        let local_t = t * 2.0;
        vg::Color::rgbf(0.0, local_t, 1.0 - local_t)
    } else {
        // Green (0°) to Red (+180°)
        let local_t = (t - 0.5) * 2.0;
        vg::Color::rgbf(local_t, 1.0 - local_t, 0.0)
    }
}

/// Draw frequency axis labels
fn draw_frequency_labels(canvas: &mut Canvas, bounds: BoundingBox, nyquist: f32) {
    let freq_to_x = |freq: f32| {
        let t = (freq.ln() - 20.0f32.ln()) / (nyquist.ln() - 20.0f32.ln());
        bounds.x + (bounds.w * t)
    };

    // Common frequency labels
    let frequencies = [
        50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
    ];

    let font_size = 12.0;

    for &freq in &frequencies {
        if freq > nyquist {
            continue;
        }

        let x = freq_to_x(freq);
        let label = if freq >= 1000.0 {
            format!("{}k", freq / 1000.0)
        } else {
            format!("{}", freq)
        };

        // Draw tick mark
        let mut path = vg::Path::new();
        path.move_to(x, bounds.y + bounds.h - 10.0);
        path.line_to(x, bounds.y + bounds.h);
        canvas.stroke_path(&path, &vg::Paint::color(vg::Color::rgbf(0.5, 0.5, 0.5)));

        // Draw label
        let text_paint = vg::Paint::color(vg::Color::rgbf(0.7, 0.7, 0.7))
            .with_font_size(font_size);
        let _ = canvas.fill_text(
            x - 15.0,
            bounds.y + bounds.h - 15.0,
            &label,
            &text_paint,
        );
    }

    // Draw phase labels on the left side
    draw_phase_labels(canvas, bounds);
}

/// Draw phase degree labels on the left side
fn draw_phase_labels(canvas: &mut Canvas, bounds: BoundingBox) {
    let font_size = 12.0;

    let phase_labels = [
        (-180.0, "−180°"),
        (-90.0, "−90°"),
        (0.0, "0°"),
        (90.0, "90°"),
        (180.0, "180°"),
    ];

    for (phase_deg, label) in &phase_labels {
        use std::f32::consts::PI;
        let phase_rad = phase_deg * PI / 180.0;

        // Map phase to y position (inverted: -180 at top, +180 at bottom)
        let t = (phase_rad + PI) / (2.0 * PI);
        let y = bounds.y + bounds.h * (1.0 - t);

        // Draw horizontal line
        let mut path = vg::Path::new();
        path.move_to(bounds.x, y);
        path.line_to(bounds.x + 20.0, y);
        canvas.stroke_path(&path, &vg::Paint::color(vg::Color::rgbf(0.5, 0.5, 0.5)));

        // Draw label
        let text_paint = vg::Paint::color(vg::Color::rgbf(0.7, 0.7, 0.7))
            .with_font_size(font_size);
        let _ = canvas.fill_text(
            bounds.x + 25.0,
            y + 4.0,
            label,
            &text_paint,
        );
    }
}

/// Draw border around the analyzer
fn draw_border(_cx: &mut DrawContext, canvas: &mut Canvas, bounds: BoundingBox) {
    let mut path = vg::Path::new();
    path.rect(bounds.x, bounds.y, bounds.w, bounds.h);

    let border_paint = vg::Paint::color(vg::Color::rgbf(0.4, 0.4, 0.4))
        .with_line_width(BORDER_WIDTH);

    canvas.stroke_path(&path, &border_paint);
}
