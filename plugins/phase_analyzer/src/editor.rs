use atomic_float::AtomicF32;
use nih_plug::prelude::*;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::widgets::*;
use nih_plug_vizia::{assets, create_vizia_editor, ViziaState, ViziaTheming};
use std::sync::{Arc, Mutex};

use crate::phase_data::PhaseData;
use crate::PhaseAnalyzerParams;

mod analyzer;

const EDITOR_WIDTH: u32 = 800;
const EDITOR_HEIGHT: u32 = 500;

#[derive(Lens, Clone)]
pub struct Data {
    pub params: Arc<PhaseAnalyzerParams>,
    pub phase_data: Arc<Mutex<triple_buffer::Output<PhaseData>>>,
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

        ResizeHandle::new(cx);

        VStack::new(cx, |cx| {
            // Title bar with analyze button
            HStack::new(cx, |cx| {
                Label::new(cx, "Phase Analyzer")
                    .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
                    .font_weight(FontWeightKeyword::Thin)
                    .font_size(30.0)
                    .height(Pixels(50.0))
                    .child_top(Stretch(1.0))
                    .child_bottom(Pixels(0.0))
                    .left(Pixels(20.0));

                // Analyze button - mapped to the analyze parameter
                ParamButton::new(cx, Data::params, |params| &params.analyze);

            })
            .height(Pixels(60.0))
            .child_left(Stretch(1.0))
            .child_right(Stretch(1.0))
            .col_between(Pixels(20.0));

            // Main visualization widget
            analyzer::PhaseAnalyzer::new(cx, Data::phase_data, Data::sample_rate)
                .height(Stretch(1.0))
                .width(Stretch(1.0));
        })
        .row_between(Pixels(0.0))
        .child_left(Stretch(1.0))
        .child_right(Stretch(1.0));
    })
}
