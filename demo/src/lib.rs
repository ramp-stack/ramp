use ramp::prism::{self, Context, canvas::Font};
use ramp::prism::drawable::Component;
use ramp::prism::layout::Column;
use ramp::prism::event::OnEvent;
use std::sync::Arc;

mod ui;

use ui::{
    MediaPanels,
    Toolbar,
    BtnContent, Btn,
    CopyPastePanel,
    WHITE_MID,
};

const FA_VIDEO: &str = "\u{f03d}";
const FA_IMAGE: &str = "\u{f03e}";
const FA_SHARE: &str = "\u{f1e0}";
const COPY_TEXT: &str = "RAMP DEMO CHECK IT!";

#[derive(Debug, Component, Clone)]
pub struct App(
    Column,
    MediaPanels,
    Toolbar,
    CopyPastePanel,
);

impl OnEvent for App {}

impl App {
    pub fn new(
        panels: MediaPanels,
        toolbar: Toolbar,
        copy_paste: CopyPastePanel,
    ) -> Self {
        App(Column::center(24.0), panels, toolbar, copy_paste)
    }
}

ramp::run!{[]; |_ctx: &mut Context| {
    let ui_font = Arc::new(Font::from_bytes(include_bytes!("../resources/font.ttf")).unwrap());
    let fa_font = Arc::new(Font::from_bytes(include_bytes!("../resources/fa-solid-900.ttf")).unwrap());

    let mk = |glyph: &str, label: &str| {
        BtnContent::new(glyph, label, fa_font.clone(), ui_font.clone(), WHITE_MID)
    };

    let size = Btn::size(3);

    let toolbar = Toolbar::new(
        Btn::camera(mk(FA_VIDEO, "Camera"), size),
        vec![
            Btn::impulse(mk(FA_IMAGE, "Photo"), size, |ctx, _| {
                ctx.pick_photo();
            }),
            Btn::impulse(mk(FA_SHARE, "Share"), size, |ctx, _| {
                ctx.share_social(COPY_TEXT.to_string());
            }),
        ],
    );

    let copy_paste = CopyPastePanel::new(
        fa_font.clone(),
        ui_font.clone(),
        COPY_TEXT,
        |ctx, text| {
            ctx.set_clipboard(text.to_string());
        },
        |ctx, on_pasted| {
            if let Some(text) = ctx.get_clipboard() {
                on_pasted(&text);
            }
        },
    );

    App::new(
        MediaPanels::default(),
        toolbar,
        copy_paste,
    )
}}