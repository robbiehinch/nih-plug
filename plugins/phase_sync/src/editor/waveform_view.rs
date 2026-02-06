use atomic_float::AtomicF32;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use std::sync::{Arc, Mutex};

use crate::visualization_data::VisualizationData;

pub struct WaveformView {
    visualization_data: Arc<Mutex<triple_buffer::Output<VisualizationData>>>,
    sample_rate: Arc<AtomicF32>,
}

impl WaveformView {
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

impl View for WaveformView {
    fn element(&self) -> Option<&'static str> {
        Some("waveform-view")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();

        // Read latest visualization data from triple buffer
        let mut viz_data_output = self.visualization_data.lock().unwrap();
        let data = viz_data_output.read();

        // Draw waveform
        draw_waveform(canvas, data, bounds);

        // Draw kick markers
        draw_kick_markers(canvas, data, bounds);

        // Draw predicted next kick
        if let Some(next_kick_time) = data.next_kick_predicted {
            draw_predicted_kick(canvas, next_kick_time, bounds);
        }

        // Draw center line
        draw_center_line(canvas, bounds);
    }
}

fn draw_waveform(canvas: &mut Canvas, data: &VisualizationData, bounds: BoundingBox) {
    if data.bass_waveform.len() < 2 {
        return;
    }

    let mut path = vg::Path::new();
    let center_y = bounds.y + bounds.h / 2.0;
    let amplitude_scale = bounds.h / 2.0 * 0.8; // Use 80% of available height

    // Start path
    let first_sample = data.bass_waveform[0];
    path.move_to(bounds.x, center_y - first_sample * amplitude_scale);

    // Draw waveform from left to right
    for (i, &sample) in data.bass_waveform.iter().enumerate() {
        let x = bounds.x + (i as f32 / data.bass_waveform.len() as f32) * bounds.w;
        let y = center_y - sample * amplitude_scale;
        path.line_to(x, y);
    }

    // Stroke the waveform
    let paint = vg::Paint::color(vg::Color::rgba(100, 200, 255, 200));
    canvas.stroke_path(&path, &paint.with_line_width(1.5));
}

fn draw_kick_markers(canvas: &mut Canvas, data: &VisualizationData, bounds: BoundingBox) {
    for kick in &data.recent_kicks {
        // Calculate x position based on time offset
        // Assume we're showing last 2 seconds of data
        let time_range = 2.0; // seconds
        let x_offset = (kick.time_offset + time_range) / time_range;

        if x_offset < 0.0 || x_offset > 1.0 {
            continue;
        }

        let x = bounds.x + x_offset * bounds.w;

        // Draw vertical line
        let mut path = vg::Path::new();
        path.move_to(x, bounds.y);
        path.line_to(x, bounds.y + bounds.h);

        // Color based on kick level (brighter = louder)
        let alpha = (kick.level * 255.0).min(255.0) as u8;
        let paint = vg::Paint::color(vg::Color::rgba(255, 100, 100, alpha));
        canvas.stroke_path(&path, &paint.with_line_width(2.0));
    }

    // Flash effect when kick detected
    if data.kick_detected_flash {
        let mut path = vg::Path::new();
        path.rect(bounds.x, bounds.y, bounds.w, bounds.h);
        let paint = vg::Paint::color(vg::Color::rgba(255, 255, 255, 30));
        canvas.fill_path(&path, &paint);
    }
}

fn draw_predicted_kick(canvas: &mut Canvas, next_kick_time: f32, bounds: BoundingBox) {
    // Calculate x position for predicted kick
    let time_range = 2.0; // seconds
    let x_offset = (next_kick_time + time_range) / time_range;

    if x_offset < 0.0 || x_offset > 1.0 {
        return;
    }

    let x = bounds.x + x_offset * bounds.w;

    // Draw dashed vertical line
    let dash_length = 5.0;
    let gap_length = 3.0;
    let mut y = bounds.y;

    while y < bounds.y + bounds.h {
        let mut path = vg::Path::new();
        path.move_to(x, y);
        path.line_to(x, (y + dash_length).min(bounds.y + bounds.h));

        let paint = vg::Paint::color(vg::Color::rgba(255, 200, 100, 150));
        canvas.stroke_path(&path, &paint.with_line_width(1.5));

        y += dash_length + gap_length;
    }
}

fn draw_center_line(canvas: &mut Canvas, bounds: BoundingBox) {
    let center_y = bounds.y + bounds.h / 2.0;

    let mut path = vg::Path::new();
    path.move_to(bounds.x, center_y);
    path.line_to(bounds.x + bounds.w, center_y);

    let paint = vg::Paint::color(vg::Color::rgba(80, 80, 100, 100));
    canvas.stroke_path(&path, &paint.with_line_width(1.0));
}
