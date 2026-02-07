use atomic_float::AtomicF32;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use std::sync::{Arc, Mutex};

use crate::phase_data::PhaseData;

const BORDER_WIDTH: f32 = 2.0;
const STRIP_HEIGHT: f32 = 60.0;
const LABEL_FONT_SIZE: f32 = 12.0;
const SECTION_FONT_SIZE: f32 = 13.0;
const STRIP_MARGIN: f32 = 15.0;
const SECTION_GAP: f32 = 20.0;

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

        let mut phase_data_output = self.phase_data.lock().unwrap();
        let data = phase_data_output.read();
        let nyquist = data.sample_rate / 2.0;

        // Horizontal area for the strips (with margins)
        let strip_x = bounds.x + STRIP_MARGIN;
        let strip_w = bounds.w - 2.0 * STRIP_MARGIN;

        // Calculate total content height and center it vertically
        let section_label_h = 18.0;
        let freq_label_h = 20.0;
        let legend_h = 28.0;
        let total_content_h = section_label_h
            + STRIP_HEIGHT
            + SECTION_GAP
            + section_label_h
            + STRIP_HEIGHT
            + 12.0
            + freq_label_h
            + 10.0
            + legend_h;
        let start_y = bounds.y + (bounds.h - total_content_h) / 2.0;
        let mut y = start_y;

        // --- Discrete section ---
        draw_section_label(canvas, strip_x, y, "Phase (discrete)");
        y += section_label_h;
        draw_strip_background(canvas, strip_x, y, strip_w, STRIP_HEIGHT);
        draw_discrete_strip(canvas, data, strip_x, y, strip_w, STRIP_HEIGHT, nyquist);
        draw_strip_border(canvas, strip_x, y, strip_w, STRIP_HEIGHT);
        y += STRIP_HEIGHT + SECTION_GAP;

        // --- Gradient section ---
        draw_section_label(canvas, strip_x, y, "Phase (gradient)");
        y += section_label_h;
        draw_strip_background(canvas, strip_x, y, strip_w, STRIP_HEIGHT);
        draw_gradient_strip(canvas, data, strip_x, y, strip_w, STRIP_HEIGHT, nyquist);
        draw_strip_border(canvas, strip_x, y, strip_w, STRIP_HEIGHT);
        y += STRIP_HEIGHT + 12.0;

        // --- Frequency labels ---
        draw_frequency_labels(canvas, strip_x, y, strip_w, nyquist);
        y += freq_label_h + 10.0;

        // --- Color legend ---
        draw_color_legend(canvas, strip_x, y, strip_w);

        // Outer border
        draw_border(cx, canvas, bounds);
    }
}

/// Draw a section label above a strip
fn draw_section_label(canvas: &mut Canvas, x: f32, y: f32, text: &str) {
    let text_paint =
        vg::Paint::color(vg::Color::rgbf(0.65, 0.65, 0.65)).with_font_size(SECTION_FONT_SIZE);
    let _ = canvas.fill_text(x + 2.0, y + SECTION_FONT_SIZE, text, &text_paint);
}

/// Fill strip background with a dark color
fn draw_strip_background(canvas: &mut Canvas, x: f32, y: f32, w: f32, h: f32) {
    let mut path = vg::Path::new();
    path.rect(x, y, w, h);
    canvas.fill_path(
        &path,
        &vg::Paint::color(vg::Color::rgbf(0.08, 0.08, 0.08)),
    );
}

/// Draw discrete colored bars (one solid color per frequency bin, extending to
/// the next bin's position so there are no gaps)
fn draw_discrete_strip(
    canvas: &mut Canvas,
    data: &PhaseData,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    nyquist: f32,
) {
    let bin_frequency = |bin_idx: f32| (bin_idx / data.num_bins as f32) * nyquist;
    let freq_to_x_pos = |freq: f32| -> f32 {
        if freq <= 20.0 {
            return x;
        }
        let t = (freq.ln() - 20.0f32.ln()) / (nyquist.ln() - 20.0f32.ln());
        x + w * t
    };

    // Collect visible bins with their x positions and phase values
    let mut bins: Vec<(f32, f32)> = Vec::new();
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
        bins.push((freq_to_x_pos(freq), phase_diff));
    }

    // Draw each bin as a bar extending to the next bin's position
    for i in 0..bins.len() {
        let (bx, phase_diff) = bins[i];
        let next_x = if i + 1 < bins.len() {
            bins[i + 1].0
        } else {
            x + w
        };
        let bar_w = (next_x - bx).max(0.5);

        let color = phase_to_color(phase_diff);
        let mut path = vg::Path::new();
        path.rect(bx, y, bar_w, h);
        canvas.fill_path(&path, &vg::Paint::color(color));
    }
}

/// Draw gradient-interpolated strip: colors blend smoothly between adjacent
/// frequency bins so the rate of phase change is clearly visible
fn draw_gradient_strip(
    canvas: &mut Canvas,
    data: &PhaseData,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    nyquist: f32,
) {
    let bin_frequency = |bin_idx: f32| (bin_idx / data.num_bins as f32) * nyquist;
    let freq_to_x_pos = |freq: f32| -> f32 {
        if freq <= 20.0 {
            return x;
        }
        let t = (freq.ln() - 20.0f32.ln()) / (nyquist.ln() - 20.0f32.ln());
        x + w * t
    };

    // Collect visible bins with their x positions and colors
    let mut bins: Vec<(f32, vg::Color)> = Vec::new();
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
        bins.push((freq_to_x_pos(freq), phase_to_color(phase_diff)));
    }

    // Draw a horizontal linear gradient between each consecutive pair of bins
    for pair in bins.windows(2) {
        let (x1, color1) = pair[0];
        let (x2, color2) = pair[1];
        let rect_w = x2 - x1;
        if rect_w <= 0.0 {
            continue;
        }

        let mut path = vg::Path::new();
        path.rect(x1, y, rect_w, h);
        let paint = vg::Paint::linear_gradient(x1, y, x2, y, color1, color2);
        canvas.fill_path(&path, &paint);
    }
}

/// Draw frequency axis labels below both strips
fn draw_frequency_labels(canvas: &mut Canvas, strip_x: f32, y: f32, strip_w: f32, nyquist: f32) {
    let freq_to_x = |freq: f32| -> f32 {
        let t = (freq.ln() - 20.0f32.ln()) / (nyquist.ln() - 20.0f32.ln());
        strip_x + strip_w * t
    };

    let frequencies = [
        50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
    ];

    for &freq in &frequencies {
        if freq > nyquist {
            continue;
        }

        let fx = freq_to_x(freq);
        let label = if freq >= 1000.0 {
            format!("{}k", freq / 1000.0)
        } else {
            format!("{}", freq)
        };

        // Tick mark
        let mut path = vg::Path::new();
        path.move_to(fx, y);
        path.line_to(fx, y + 6.0);
        canvas.stroke_path(
            &path,
            &vg::Paint::color(vg::Color::rgbf(0.5, 0.5, 0.5)),
        );

        // Label
        let text_paint =
            vg::Paint::color(vg::Color::rgbf(0.7, 0.7, 0.7)).with_font_size(LABEL_FONT_SIZE);
        let _ = canvas.fill_text(fx - 10.0, y + 18.0, &label, &text_paint);
    }
}

/// Draw a color legend showing the phase-to-color mapping
fn draw_color_legend(canvas: &mut Canvas, strip_x: f32, y: f32, strip_w: f32) {
    use std::f32::consts::PI;

    let legend_w = strip_w * 0.4;
    let legend_h = 10.0;
    let legend_x = strip_x + (strip_w - legend_w) / 2.0;

    // Draw the gradient bar in small steps
    let steps = 80;
    let step_w = legend_w / steps as f32;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let phase = (t * 2.0 - 1.0) * PI;
        let color = phase_to_color(phase);
        let sx = legend_x + i as f32 * step_w;

        let mut path = vg::Path::new();
        path.rect(sx, y, step_w + 1.0, legend_h);
        canvas.fill_path(&path, &vg::Paint::color(color));
    }

    // Labels below the legend bar
    let label_y = y + legend_h + 12.0;
    let text_paint =
        vg::Paint::color(vg::Color::rgbf(0.7, 0.7, 0.7)).with_font_size(11.0);
    let _ = canvas.fill_text(legend_x - 5.0, label_y, "\u{2212}180\u{b0}", &text_paint);
    let _ = canvas.fill_text(
        legend_x + legend_w / 2.0 - 5.0,
        label_y,
        "0\u{b0}",
        &text_paint,
    );
    let _ = canvas.fill_text(
        legend_x + legend_w - 10.0,
        label_y,
        "+180\u{b0}",
        &text_paint,
    );
}

/// Map phase in radians [-\u{03c0}, \u{03c0}] to color
/// Blue (-180\u{b0}) \u{2192} Green (0\u{b0}) \u{2192} Red (+180\u{b0})
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

/// Draw a thin border around a strip
fn draw_strip_border(canvas: &mut Canvas, x: f32, y: f32, w: f32, h: f32) {
    let mut path = vg::Path::new();
    path.rect(x, y, w, h);
    let paint =
        vg::Paint::color(vg::Color::rgbf(0.3, 0.3, 0.3)).with_line_width(1.0);
    canvas.stroke_path(&path, &paint);
}

/// Draw border around the analyzer
fn draw_border(_cx: &mut DrawContext, canvas: &mut Canvas, bounds: BoundingBox) {
    let mut path = vg::Path::new();
    path.rect(bounds.x, bounds.y, bounds.w, bounds.h);

    let border_paint =
        vg::Paint::color(vg::Color::rgbf(0.4, 0.4, 0.4)).with_line_width(BORDER_WIDTH);

    canvas.stroke_path(&path, &border_paint);
}
