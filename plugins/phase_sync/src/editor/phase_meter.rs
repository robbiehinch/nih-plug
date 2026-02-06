use atomic_float::AtomicF32;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use std::sync::{Arc, Mutex};

use crate::visualization_data::VisualizationData;

pub struct PhaseMeter {
    visualization_data: Arc<Mutex<triple_buffer::Output<VisualizationData>>>,
    sample_rate: Arc<AtomicF32>,
}

impl PhaseMeter {
    pub fn new<L1, L2>(
        cx: &mut Context,
        visualization_data: L1,
        sample_rate: L2,
    ) -> Handle<Self>
    where
        L1: Lens<Target = Arc<Mutex<triple_buffer::Output<VisualizationData>>>>,
        L2: Lens<Target = Arc<AtomicF32>>,
    {
        Self {
            visualization_data: visualization_data.get(cx),
            sample_rate: sample_rate.get(cx),
        }
        .build(cx, |_cx| {})
    }
}

impl View for PhaseMeter {
    fn element(&self) -> Option<&'static str> {
        Some("phase-meter")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();

        // Read latest visualization data from triple buffer
        let mut viz_data_output = self.visualization_data.lock().unwrap();
        let data = viz_data_output.read();

        // Draw phase scale
        draw_phase_scale(canvas, bounds);

        // Draw phase history
        draw_phase_history(canvas, data, bounds);

        // Draw current phase indicator
        draw_current_phase(canvas, data.current_phase_degrees, bounds);

        // Draw target phase indicator
        draw_target_phase(canvas, data.target_phase_degrees, bounds);

        // Draw phase labels
        draw_phase_labels(canvas, bounds);
    }
}

fn draw_phase_scale(canvas: &mut Canvas, bounds: BoundingBox) {
    // Draw vertical lines for phase scale markers
    let phase_range = 360.0; // -180 to +180
    let markers = [-180.0, -90.0, 0.0, 90.0, 180.0];

    for &phase in &markers {
        let y = phase_to_y(phase, bounds);

        let mut path = vg::Path::new();
        path.move_to(bounds.x, y);
        path.line_to(bounds.x + bounds.w, y);

        let alpha = if phase == 0.0 { 150 } else { 50 };
        let paint = vg::Paint::color(vg::Color::rgba(80, 80, 100, alpha));
        canvas.stroke_path(&path, &paint.with_line_width(1.0));
    }
}

fn draw_phase_history(canvas: &mut Canvas, data: &VisualizationData, bounds: BoundingBox) {
    if data.phase_history.len() < 2 {
        return;
    }

    let mut path = vg::Path::new();
    let time_range = 2.0; // Show last 2 seconds

    // Find the first point
    if let Some(first_point) = data.phase_history.front() {
        let x = time_to_x(first_point.time_offset, time_range, bounds);
        let y = phase_to_y(first_point.phase_degrees, bounds);
        path.move_to(x, y);
    }

    // Draw the rest of the points
    for point in data.phase_history.iter().skip(1) {
        let x = time_to_x(point.time_offset, time_range, bounds);
        let y = phase_to_y(point.phase_degrees, bounds);

        if x >= bounds.x && x <= bounds.x + bounds.w {
            path.line_to(x, y);
        }
    }

    // Stroke the phase history
    let paint = vg::Paint::color(vg::Color::rgba(100, 200, 255, 200));
    canvas.stroke_path(&path, &paint.with_line_width(2.0));
}

fn draw_current_phase(canvas: &mut Canvas, current_phase: f32, bounds: BoundingBox) {
    let y = phase_to_y(current_phase, bounds);
    let x = bounds.x + bounds.w - 20.0; // Right side of display

    // Draw circle indicator
    let mut path = vg::Path::new();
    path.circle(x, y, 5.0);

    let paint = vg::Paint::color(vg::Color::rgba(100, 255, 100, 255));
    canvas.fill_path(&path, &paint);

    // Draw outline
    let outline_paint = vg::Paint::color(vg::Color::rgba(50, 150, 50, 255));
    canvas.stroke_path(&path, &outline_paint.with_line_width(2.0));
}

fn draw_target_phase(canvas: &mut Canvas, target_phase: f32, bounds: BoundingBox) {
    let y = phase_to_y(target_phase, bounds);

    // Draw horizontal dashed line across entire width
    let dash_length = 5.0;
    let gap_length = 3.0;
    let mut x = bounds.x;

    while x < bounds.x + bounds.w {
        let mut path = vg::Path::new();
        path.move_to(x, y);
        path.line_to((x + dash_length).min(bounds.x + bounds.w), y);

        let paint = vg::Paint::color(vg::Color::rgba(255, 200, 100, 150));
        canvas.stroke_path(&path, &paint.with_line_width(1.5));

        x += dash_length + gap_length;
    }
}

fn draw_phase_labels(canvas: &mut Canvas, bounds: BoundingBox) {
    let labels = [
        (-180.0, "-180°"),
        (-90.0, "-90°"),
        (0.0, "0°"),
        (90.0, "+90°"),
        (180.0, "+180°"),
    ];

    for (phase, label) in &labels {
        let y = phase_to_y(*phase, bounds);

        // Create text paint
        let mut paint = vg::Paint::color(vg::Color::rgba(180, 180, 200, 255));
        paint.set_font_size(12.0);
        paint.set_text_align(vg::Align::Right);
        paint.set_text_baseline(vg::Baseline::Middle);

        let _ = canvas.fill_text(bounds.x + bounds.w - 30.0, y, label, &paint);
    }
}

// Helper functions

fn phase_to_y(phase_degrees: f32, bounds: BoundingBox) -> f32 {
    // Map phase from [-180, 180] to [bounds.y + bounds.h, bounds.y]
    // (inverted because y increases downward)
    let normalized = (phase_degrees + 180.0) / 360.0; // [0, 1]
    bounds.y + bounds.h - (normalized * bounds.h)
}

fn time_to_x(time_offset: f32, time_range: f32, bounds: BoundingBox) -> f32 {
    // Map time from [-time_range, 0] to [bounds.x, bounds.x + bounds.w]
    let normalized = (time_offset + time_range) / time_range;
    bounds.x + normalized * bounds.w
}
