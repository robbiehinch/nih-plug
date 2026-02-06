use atomic_float::AtomicF32;
use nih_plug::prelude::*;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::widgets::*;
use nih_plug_vizia::{assets, create_vizia_editor, ViziaState, ViziaTheming};
use std::sync::{Arc, Mutex};

use crate::visualization_data::VisualizationData;
use crate::PhaseSyncParams;

mod waveform_view;
mod phase_meter;

const EDITOR_WIDTH: u32 = 900;
const EDITOR_HEIGHT: u32 = 600;

#[derive(Lens, Clone)]
pub struct Data {
    pub params: Arc<PhaseSyncParams>,
    pub visualization_data: Arc<Mutex<triple_buffer::Output<VisualizationData>>>,
    pub sample_rate: Arc<AtomicF32>,
}

impl Model for Data {}

pub(crate) fn default_state() -> Arc<ViziaState> {
    ViziaState::new(|| (EDITOR_WIDTH, EDITOR_HEIGHT))
}

pub(crate) fn create(
    editor_data: Data,
    editor_state: Arc<ViziaState>,
) -> Option<Box<dyn Editor>> {
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        assets::register_noto_sans_light(cx);
        assets::register_noto_sans_thin(cx);

        editor_data.clone().build(cx);

        VStack::new(cx, |cx| {
            // Title bar
            HStack::new(cx, |cx| {
                Label::new(cx, "Phase Sync")
                    .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
                    .font_weight(FontWeightKeyword::Thin)
                    .font_size(30.0)
                    .height(Pixels(50.0))
                    .child_top(Stretch(1.0))
                    .child_bottom(Pixels(0.0))
                    .left(Pixels(20.0));
            })
            .height(Pixels(60.0))
            .child_left(Stretch(1.0))
            .child_right(Stretch(1.0));

            // Main visualization area
            HStack::new(cx, |cx| {
                // Left: Bass waveform with kick indicators
                VStack::new(cx, |cx| {
                    Label::new(cx, "Bass Waveform & Kick Detection")
                        .font_size(14.0)
                        .height(Pixels(20.0))
                        .left(Pixels(10.0));

                    waveform_view::WaveformView::new(
                        cx,
                        Data::visualization_data,
                        Data::sample_rate,
                    )
                    .background_color(Color::rgb(20, 20, 30))
                    .border_color(Color::rgb(60, 60, 80))
                    .border_width(Pixels(1.0));
                })
                .width(Stretch(2.0))
                .height(Pixels(280.0))
                .row_between(Pixels(5.0));

                // Right: Phase rotation meter
                VStack::new(cx, |cx| {
                    Label::new(cx, "Phase Rotation")
                        .font_size(14.0)
                        .height(Pixels(20.0))
                        .left(Pixels(10.0));

                    phase_meter::PhaseMeter::new(
                        cx,
                        Data::visualization_data,
                        Data::sample_rate,
                    )
                    .background_color(Color::rgb(20, 20, 30))
                    .border_color(Color::rgb(60, 60, 80))
                    .border_width(Pixels(1.0));
                })
                .width(Stretch(1.0))
                .height(Pixels(280.0))
                .row_between(Pixels(5.0));
            })
            .col_between(Pixels(10.0))
            .left(Pixels(10.0))
            .right(Pixels(10.0));

            // Parameter controls
            HStack::new(cx, |cx| {
                // Kick Detection parameters
                VStack::new(cx, |cx| {
                    Label::new(cx, "Kick Detection")
                        .font_size(16.0)
                        .font_weight(FontWeightKeyword::Bold)
                        .height(Pixels(25.0));

                    ParamSlider::new(cx, Data::params, |p| &p.kick_threshold);
                    ParamSlider::new(cx, Data::params, |p| &p.kick_attack_ms);
                    ParamSlider::new(cx, Data::params, |p| &p.kick_release_ms);
                    ParamSlider::new(cx, Data::params, |p| &p.min_kick_interval_ms);
                })
                .width(Stretch(1.0))
                .row_between(Pixels(5.0))
                .child_left(Pixels(5.0))
                .child_right(Pixels(5.0));

                // Phase Rotation parameters
                VStack::new(cx, |cx| {
                    Label::new(cx, "Phase Rotation")
                        .font_size(16.0)
                        .font_weight(FontWeightKeyword::Bold)
                        .height(Pixels(25.0));

                    ParamSlider::new(cx, Data::params, |p| &p.center_frequency);
                    ParamSlider::new(cx, Data::params, |p| &p.phase_amount);
                    ParamSlider::new(cx, Data::params, |p| &p.frequency_spread);
                    ParamSlider::new(cx, Data::params, |p| &p.bass_analysis_window_ms);
                })
                .width(Stretch(1.0))
                .row_between(Pixels(5.0))
                .child_left(Pixels(5.0))
                .child_right(Pixels(5.0));

                // Adaptive Behavior parameters
                VStack::new(cx, |cx| {
                    Label::new(cx, "Adaptation")
                        .font_size(16.0)
                        .font_weight(FontWeightKeyword::Bold)
                        .height(Pixels(25.0));

                    ParamSlider::new(cx, Data::params, |p| &p.adaptation_mode);
                    ParamSlider::new(cx, Data::params, |p| &p.transition_threshold);
                    ParamSlider::new(cx, Data::params, |p| &p.dry_wet);
                })
                .width(Stretch(1.0))
                .row_between(Pixels(5.0))
                .child_left(Pixels(5.0))
                .child_right(Pixels(5.0));
            })
            .col_between(Pixels(10.0))
            .height(Pixels(200.0))
            .left(Pixels(10.0))
            .right(Pixels(10.0))
            .top(Pixels(10.0));
        })
        .row_between(Pixels(10.0))
        .background_color(Color::rgb(30, 30, 40));
    })
}
