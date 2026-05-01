use ramp::prism::{self, Context, canvas::{Image, Shape, Text, ShapeType, Color, Font, Span, Align}};
use ramp::prism::drawable::{Component, SizedTree};
use ramp::prism::layout::{Stack, Row, Column};
use ramp::prism::layout::Area;
use ramp::prism::event::{OnEvent, Event, CameraFrame, MouseEvent, MouseState, PickedPhoto};
use ramp::prism::Camera;

use ramp::maverick_os::air::{Contracts, Contract, Substance, Id, Reactants, Reactant, Beaker, Name};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::convert::Infallible;
use std::sync::Arc;

use serde::{Serialize, Deserialize};

const BG:          Color = Color(12,  12,  18,  255);
const SURFACE:     Color = Color(22,  22,  32,  255);
const ACCENT:      Color = Color(99,  102, 241, 255);
const ACCENT_DIM:  Color = Color(99,  102, 241, 60 );
const BTN_BG:      Color = Color(30,  30,  46,  255);
const BTN_BORDER:  Color = Color(255, 255, 255, 18 );
const WHITE_HI:    Color = Color(255, 255, 255, 230);
const WHITE_MID:   Color = Color(255, 255, 255, 160);
const WHITE_DIM:   Color = Color(255, 255, 255, 80 );

const FA_VIDEO:      &str = "\u{f03d}";
const FA_IMAGE:      &str = "\u{f03e}";
const FA_SHARE:      &str = "\u{f1e0}";
const FA_COPY:       &str = "\u{f0c5}";
const FA_PASTE:      &str = "\u{f0ea}";

fn icon_text(glyph: &str, fa: Arc<Font>, color: Color) -> Text {
    Text::new(
        vec![Span::new(glyph.to_string(), 22.0, Some(26.0), fa, color, 0.0)],
        None,
        Align::Center,
        None,
    )
}

#[derive(Debug, Component, Clone)]
pub struct BtnContent(Column, Text, Text);
impl OnEvent for BtnContent {}
impl BtnContent {
    pub fn new(glyph: &str, label: &str, fa: Arc<Font>, ui: Arc<Font>, color: Color) -> Self {
        BtnContent(
            Column::center(4.0),
            icon_text(glyph, fa, color),
            label_text(label, ui, 11.0, color),
        )
    }
}

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
pub struct PhotoPicked;
impl Event for PhotoPicked {
    fn pass(self: Box<Self>, _ctx: &mut Context, children: &[Area]) -> Vec<Option<Box<dyn Event>>> {
        children.iter().map(|_| Some(self.clone() as Box<dyn Event>)).collect()
    }
}

fn label_text(s: impl Into<String>, font: Arc<Font>, size: f32, color: Color) -> Text {
    Text::new(
        vec![Span::new(s.into(), size, Some(size * 1.35), font, color, 0.0)],
        None,
        Align::Center,
        None,
    )
}

fn pill(w: f32, h: f32, radius: f32, color: Color) -> Shape {
    Shape { shape: ShapeType::RoundedRectangle(0.0, (w, h), 0.0, radius), color }
}

fn rect(w: f32, h: f32, color: Color) -> Shape {
    Shape { shape: ShapeType::Rectangle(0.0, (w, h), 0.0), color }
}

#[derive(Debug, Component, Clone)]
pub struct StatusLabel(Stack, Shape, Text);

impl OnEvent for StatusLabel {
    fn on_event(&mut self, _ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        let font = self.2.spans[0].font.clone();
        if let Some(ClipboardCopied(t)) = event.downcast_ref::<ClipboardCopied>() {
            self.2 = label_text(format!("✓  Copied \"{}\"", t), font.clone(), 13.0, WHITE_HI);
        }
        if let Some(ClipboardPasted(t)) = event.downcast_ref::<ClipboardPasted>() {
            self.2 = label_text(format!("✓  Pasted \"{}\"", t), font.clone(), 13.0, WHITE_HI);
        }
        if let Some(CameraStarted) = event.downcast_ref::<CameraStarted>() {
            self.2 = label_text("Camera active", font.clone(), 13.0, Color(134, 239, 172, 230));
        }
        if let Some(PhotoPicked) = event.downcast_ref::<PhotoPicked>() {
            self.2 = label_text("Photo loaded", font.clone(), 13.0, Color(134, 239, 172, 230));
        }
        vec![event]
    }
}

impl StatusLabel {
    pub fn new(font: Font) -> Self {
        StatusLabel(
            Stack::center(),
            pill(280.0, 36.0, 18.0, Color(30, 30, 46, 200)),
            label_text("Ready", Arc::new(font), 13.0, WHITE_DIM),
        )
    }
}

const CAM_W: f32 = 186.0;
const CAM_H: f32 = 280.0;
const PHO_W: f32 = 186.0;
const PHO_H: f32 = 280.0;
const PANEL_R: f32 = 20.0;

#[derive(Debug, Component, Clone)]
pub struct Viewfinder(Stack, Shape, Option<Shape>, Option<Image>);

impl OnEvent for Viewfinder {
    fn on_event(&mut self, _ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(CameraFrame(img)) = event.downcast_ref::<CameraFrame>() {
            self.2 = None;
            self.3 = Some(Image {
                shape: ShapeType::RoundedRectangle(0.0, (CAM_W, CAM_H), 0.0, PANEL_R),
                image: img.clone().into(),
                color: None,
            });
        }
        vec![event]
    }
}

impl Viewfinder {
    pub fn new() -> Self {
        Viewfinder(
            Stack::center(),
            pill(CAM_W + 4.0, CAM_H + 4.0, PANEL_R + 2.0, ACCENT_DIM),
            Some(Shape { shape: ShapeType::RoundedRectangle(0.0, (CAM_W, CAM_H), 0.0, PANEL_R), color: SURFACE }),
            None,
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct PhotoDisplay(Stack, Shape, Option<Shape>, Option<Image>);

impl OnEvent for PhotoDisplay {
    fn on_event(&mut self, ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(PickedPhoto(img)) = event.downcast_ref::<PickedPhoto>() {
            self.2 = None;
            self.3 = Some(Image {
                shape: ShapeType::RoundedRectangle(0.0, (PHO_W, PHO_H), 0.0, PANEL_R),
                image: img.clone().into(),
                color: None,
            });
            ctx.emit(PhotoPicked);
        }
        vec![event]
    }
}

impl PhotoDisplay {
    pub fn new() -> Self {
        PhotoDisplay(
            Stack::center(),
            pill(PHO_W + 4.0, PHO_H + 4.0, PANEL_R + 2.0, Color(255, 255, 255, 14)),
            Some(Shape { shape: ShapeType::RoundedRectangle(0.0, (PHO_W, PHO_H), 0.0, PANEL_R), color: SURFACE }),
            None,
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct Viewers(Row, Viewfinder, PhotoDisplay);

impl OnEvent for Viewers {}

impl Viewers {
    pub fn new(viewfinder: Viewfinder, photo: PhotoDisplay) -> Self {
        Viewers(Row::center(20.0), viewfinder, photo)
    }
}

const TOOLBAR_MAX_W: f32 = 360.0;
const BTN_GAP:       f32 = 10.0;
const BTN_W: f32 = (TOOLBAR_MAX_W - BTN_GAP * 4.0) / 5.0;
const BTN_H: f32 = BTN_W;

fn btn_bg(active: bool) -> Shape {
    Shape {
        shape: ShapeType::RoundedRectangle(0.0, (BTN_W, BTN_H), 0.0, 18.0),
        color: if active { Color(99, 102, 241, 200) } else { BTN_BG },
    }
}

#[derive(Debug, Component, Clone)]
pub struct StartCameraButton(Stack, Shape, BtnContent, #[skip] bool, #[skip] Option<Box<dyn Camera>>);

impl OnEvent for StartCameraButton {
    fn on_event(&mut self, ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(MouseEvent { state: MouseState::Released, position: Some(_) }) = event.downcast_ref::<MouseEvent>() {
            self.3 = !self.3;
            if self.3 {
                self.4 = Some(ctx.start_camera());
                ctx.emit(CameraStarted);
                self.1 = btn_bg(true);
            } else {
                self.4 = None;
                self.1 = btn_bg(false);
            }
            ctx.trigger_haptic();
        }
        vec![event]
    }
}

impl StartCameraButton {
    pub fn new(fa: Font, ui: Font) -> Self {
        StartCameraButton(
            Stack::center(),
            btn_bg(false),
            BtnContent::new(FA_VIDEO, "Camera", Arc::new(fa), Arc::new(ui), WHITE_MID),
            false,
            None
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct PhotoButton(Stack, Shape, BtnContent);

impl OnEvent for PhotoButton {
    fn on_event(&mut self, ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(MouseEvent { state: MouseState::Released, position: Some(_) }) = event.downcast_ref::<MouseEvent>() {
            ctx.pick_photo();
            ctx.trigger_haptic();
        }
        vec![event]
    }
}

impl PhotoButton {
    pub fn new(fa: Font, ui: Font) -> Self {
        PhotoButton(
            Stack::center(),
            btn_bg(false),
            BtnContent::new(FA_IMAGE, "Photo", Arc::new(fa), Arc::new(ui), WHITE_MID),
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct ShareButton(Stack, Shape, BtnContent);

impl OnEvent for ShareButton {
    fn on_event(&mut self, ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(MouseEvent { state: MouseState::Released, position: Some(_) }) = event.downcast_ref::<MouseEvent>() {
            ctx.share_social("RAMP DEMO CHECK IT!".to_string());
            ctx.trigger_haptic();
        }
        vec![event]
    }
}

impl ShareButton {
    pub fn new(fa: Font, ui: Font) -> Self {
        ShareButton(
            Stack::center(),
            btn_bg(false),
            BtnContent::new(FA_SHARE, "Share", Arc::new(fa), Arc::new(ui), WHITE_MID),
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct SetClipboardButton(Stack, Shape, BtnContent);

impl OnEvent for SetClipboardButton {
    fn on_event(&mut self, ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(MouseEvent { state: MouseState::Released, position: Some(_) }) = event.downcast_ref::<MouseEvent>() {
            let text = "RAMP DEMO CHECK IT!".to_string();
            ctx.set_clipboard(text.clone());
            ctx.emit(ClipboardCopied(text));
            ctx.trigger_haptic();
        }
        vec![event]
    }
}

impl SetClipboardButton {
    pub fn new(fa: Font, ui: Font) -> Self {
        SetClipboardButton(
            Stack::center(),
            btn_bg(false),
            BtnContent::new(FA_COPY, "Copy", Arc::new(fa), Arc::new(ui), WHITE_MID),
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct GetClipboardButton(Stack, Shape, BtnContent);

impl OnEvent for GetClipboardButton {
    fn on_event(&mut self, ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(MouseEvent { state: MouseState::Released, position: Some(_) }) = event.downcast_ref::<MouseEvent>() {
            if let Some(text) = ctx.get_clipboard() {
                ctx.emit(ClipboardPasted(text));
            }
            ctx.trigger_haptic();
        }
        vec![event]
    }
}

impl GetClipboardButton {
    pub fn new(fa: Font, ui: Font) -> Self {
        GetClipboardButton(
            Stack::center(),
            btn_bg(false),
            BtnContent::new(FA_PASTE, "Paste", Arc::new(fa), Arc::new(ui), WHITE_MID),
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct ToolRow(
    Row,
    StartCameraButton,
    PhotoButton,
    ShareButton,
    SetClipboardButton,
    GetClipboardButton,
);
impl OnEvent for ToolRow {}
impl ToolRow {
    fn new(s: StartCameraButton, p: PhotoButton, sh: ShareButton, sc: SetClipboardButton, gc: GetClipboardButton) -> Self {
        ToolRow(Row::center(BTN_GAP), s, p, sh, sc, gc)
    }
}

#[derive(Debug, Component, Clone)]
pub struct Toolbar(Stack, Shape, ToolRow);
impl OnEvent for Toolbar {}
impl Toolbar {
    pub fn new(row: ToolRow) -> Self {
        Toolbar(
            Stack::center(),
            pill(TOOLBAR_MAX_W, BTN_H + 20.0, 24.0, Color(18, 18, 28, 200)),
            row,
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct App(Column, Viewers, Toolbar, StatusLabel);

impl OnEvent for App {}

impl App {
    pub fn new(viewers: Viewers, toolbar: Toolbar, label: StatusLabel) -> Self {
        App(Column::center(24.0), viewers, toolbar, label)
    }
}

#[derive(Serialize, Deserialize, Hash)]
pub struct ChatRoom;
impl ChatRoom {
    pub fn new(_name: &str) -> Self {ChatRoom}
}
impl Contract for ChatRoom {
    fn id() -> Id {Id::hash("ChatRoom2.5")}

    fn init(self, signer: &Name, _timestamp: u64) -> Substance {Substance::Map(BTreeMap::from([
        ("name".to_string(), Substance::String("myroom".to_string())),
        ("author".to_string(), Substance::String(signer.to_string())),
        ("messages".to_string(), Substance::map())
    ]))}

    fn routes() -> BTreeMap<PathBuf, Reactants> {
        BTreeMap::from([
            (PathBuf::from("/name"), Reactants::new().add::<ChangeName>()),
            (PathBuf::from("/messages"), Reactants::new().add::<SendMessage>())
        ])
    }
}

#[derive(Serialize, Deserialize, Hash)]
pub struct ChangeName(String);
impl Reactant for ChangeName {
    type Error = Infallible;
    type Contract = ChatRoom;

    fn apply<B: Beaker>(self, _path: &Path, signer: &Name, _timestamp: u64, substance: &mut B) -> Result<(), Self::Error> {
        if substance.query("/author") == Ok(Substance::String(signer.to_string())) {
            let _ = substance.insert("/name", Substance::String(self.0));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Hash)]
pub struct SendMessage(String);
impl Reactant for SendMessage {
    type Error = Infallible;
    type Contract = ChatRoom;

    fn apply<B: Beaker>(self, _path: &Path, signer: &Name, timestamp: u64, substance: &mut B) -> Result<(), Self::Error> {
        let _ = substance.insert("/messages/-", Substance::Map(BTreeMap::from([
            ("author".to_string(), Substance::String(signer.to_string())),
            ("timestamp".to_string(), Substance::Integer(timestamp as i64)),
            ("body".to_string(), Substance::String(self.0)),
        ])));
        Ok(())
    }
}

ramp::run!{[ChatRoom]; |ctx: &mut Context| {
    let ui_font = Font::from_bytes(include_bytes!("../resources/font.ttf")).unwrap();
    let fa_font = Font::from_bytes(include_bytes!("../resources/fa-solid-900.ttf")).unwrap();

    App::new(
        Viewers::new(
            Viewfinder::new(),
            PhotoDisplay::new(),
        ),
        Toolbar::new(ToolRow::new(
            StartCameraButton::new(fa_font.clone(), ui_font.clone()),
            PhotoButton::new(fa_font.clone(), ui_font.clone()),
            ShareButton::new(fa_font.clone(), ui_font.clone()),
            SetClipboardButton::new(fa_font.clone(), ui_font.clone()),
            GetClipboardButton::new(fa_font.clone(), ui_font.clone()),
        )),
        StatusLabel::new(ui_font.clone()),
    )
}}