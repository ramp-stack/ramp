use ramp::prism::{self, Context, canvas::Font, Assets};
use ramp::prism::drawable::{Component, SizedTree};
use ramp::prism::layout::{Column, Area};
use ramp::prism::event::{OnEvent, Event};
use std::sync::Arc;

mod contract;
mod ui;

use contract::ChatRoom;
use ui::{
    MediaPanels,
    StatusBar,
    Toolbar,
    BtnContent, Btn,
    WHITE_MID,
};

const FA_VIDEO: &str = "\u{f03d}";
const FA_IMAGE: &str = "\u{f03e}";
const FA_SHARE: &str = "\u{f1e0}";
const FA_COPY:  &str = "\u{f0c5}";
const FA_PASTE: &str = "\u{f0ea}";

#[derive(Debug, Clone)]
pub struct ClipboardCopied(pub String);
impl Event for ClipboardCopied {
    fn pass(self: Box<Self>, _ctx: &mut Context, children: &[Area]) -> Vec<Option<Box<dyn Event>>> {
        children.iter().map(|_| Some(self.clone() as Box<dyn Event>)).collect()
    }
}

#[derive(Debug, Clone)]
pub struct ClipboardPasted(pub String);
impl Event for ClipboardPasted {
    fn pass(self: Box<Self>, _ctx: &mut Context, children: &[Area]) -> Vec<Option<Box<dyn Event>>> {
        children.iter().map(|_| Some(self.clone() as Box<dyn Event>)).collect()
    }
}

#[derive(Debug, Clone)]
pub struct CameraStarted;
impl Event for CameraStarted {
    fn pass(self: Box<Self>, _ctx: &mut Context, children: &[Area]) -> Vec<Option<Box<dyn Event>>> {
        children.iter().map(|_| Some(self.clone() as Box<dyn Event>)).collect()
    }
}

#[derive(Debug, Clone)]
pub struct CameraStopped;
impl Event for CameraStopped {
    fn pass(self: Box<Self>, _ctx: &mut Context, children: &[Area]) -> Vec<Option<Box<dyn Event>>> {
        children.iter().map(|_| Some(self.clone() as Box<dyn Event>)).collect()
    }
}

#[derive(Debug, Component, Clone)]
pub struct App(Column, MediaPanels, Toolbar, StatusBar);

impl OnEvent for App {
    fn on_event(&mut self, _ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(ClipboardCopied(t)) = event.downcast_ref::<ClipboardCopied>() {
            self.3.set_message(format!("✓  Copied \"{}\"", t));
        }
        if let Some(ClipboardPasted(t)) = event.downcast_ref::<ClipboardPasted>() {
            self.3.set_message(format!("✓  Pasted \"{}\"", t));
        }
        if event.downcast_ref::<CameraStarted>().is_some() {
            self.3.set_ok("Camera active");
        }
        if event.downcast_ref::<CameraStopped>().is_some() {
            self.3.reset();
        }
        vec![event]
    }
}

impl App {
    pub fn new(panels: MediaPanels, toolbar: Toolbar, status: StatusBar) -> Self {
        App(Column::center(24.0), panels, toolbar, status)
    }
}

ramp::run!{[ChatRoom]; |ctx: &mut Context, assets: Assets| {
    let ui_font = Arc::new(Font::from_bytes(&assets.load_file("font.ttf").unwrap()).unwrap());
    let fa_font = Arc::new(Font::from_bytes(&assets.load_file("fa-solid-900.ttf").unwrap()).unwrap());

    let mk = |glyph: &str, label: &str| {
        BtnContent::new(glyph, label, fa_font.clone(), ui_font.clone(), WHITE_MID)
    };

    let toolbar = Toolbar::new(vec![
        Btn::new(mk(FA_VIDEO, "Camera"), Btn::size(5), true, |ctx, active| {
            if active { ctx.start_camera(); ctx.emit(CameraStarted); }
            else      { ctx.stop_camera();  ctx.emit(CameraStopped); }
        }),
        Btn::new(mk(FA_IMAGE, "Photo"), Btn::size(5), false, |ctx, _| {
            ctx.pick_photo();
        }),
        Btn::new(mk(FA_SHARE, "Share"), Btn::size(5), false, |ctx, _| {
            ctx.share_social("RAMP DEMO CHECK IT!".to_string());
        }),
        Btn::new(mk(FA_COPY, "Copy"), Btn::size(5), false, |ctx, _| {
            let text = "RAMP DEMO CHECK IT!".to_string();
            ctx.set_clipboard(text.clone());
            ctx.emit(ClipboardCopied(text));
        }),
        Btn::new(mk(FA_PASTE, "Paste"), Btn::size(5), false, |ctx, _| {
            if let Some(text) = ctx.get_clipboard() {
                ctx.emit(ClipboardPasted(text));
            }
        }),
    ]);

    App::new(
        MediaPanels::default(),
        toolbar,
        StatusBar::new(ui_font.clone()),
    )
}}