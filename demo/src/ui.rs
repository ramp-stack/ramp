use ramp::prism;
use ramp::prism::{Context, canvas::{Image, Shape, Text, ShapeType, Color, Font, Span, Align}};
use ramp::prism::drawable::{Component, SizedTree};
use ramp::prism::layout::{Stack, Row, Column, Offset, Size, Padding};
use ramp::prism::layout::Area;
use ramp::prism::event::{OnEvent, Event, CameraFrame, PickedPhoto, MouseEvent, MouseState};
use std::sync::Arc;

pub const BG:         Color = Color(12,  12,  18,  255);
pub const SURFACE:    Color = Color(22,  22,  32,  255);
pub const ACCENT_DIM: Color = Color(99,  102, 241, 60 );
pub const WHITE_HI:   Color = Color(255, 255, 255, 230);
pub const WHITE_DIM:  Color = Color(255, 255, 255, 80 );
pub const WHITE_MID:  Color = Color(255, 255, 255, 160);

pub const PANEL_W: f32 = 186.0;
pub const PANEL_H: f32 = 280.0;
pub const PANEL_R: f32 = 20.0;

pub const TOOLBAR_W: f32 = 360.0;
pub const BTN_GAP:   f32 = 10.0;

#[derive(Debug, Clone, PartialEq)]
pub enum MediaPanelKind { Camera, Photo }

#[derive(Debug, Component, Clone)]
pub struct MediaPanel(
    Stack,
    Shape,
    Option<Shape>,
    Option<Image>,
    #[skip] MediaPanelKind,
);

impl OnEvent for MediaPanel {
    fn on_event(
        &mut self,
        _ctx: &mut Context,
        _sized: &SizedTree,
        event: Box<dyn Event>,
    ) -> Vec<Box<dyn Event>> {
        if self.4 == MediaPanelKind::Camera {
            if let Some(CameraFrame(img)) = event.downcast_ref::<CameraFrame>() {
                self.2 = None;
                self.3 = Some(Image {
                    shape: ShapeType::RoundedRectangle(0.0, (PANEL_W, PANEL_H), 0.0, PANEL_R),
                    image: img.clone().into(),
                    color: None,
                });
            }
        } else if let Some(PickedPhoto(img)) = event.downcast_ref::<PickedPhoto>() {
            self.2 = None;
            self.3 = Some(Image {
                shape: ShapeType::RoundedRectangle(0.0, (PANEL_W, PANEL_H), 0.0, PANEL_R),
                image: img.clone().into(),
                color: None,
            });
        }
        vec![event]
    }
}

impl MediaPanel {
    fn rect(w: f32, h: f32, radius: f32, color: Color) -> Shape {
        Shape { shape: ShapeType::RoundedRectangle(0.0, (w, h), 0.0, radius), color }
    }

    pub fn camera() -> Self {
        MediaPanel(
            Stack::center(),
            Self::rect(PANEL_W + 4.0, PANEL_H + 4.0, PANEL_R + 2.0, ACCENT_DIM),
            Some(Self::rect(PANEL_W, PANEL_H, PANEL_R, SURFACE)),
            None,
            MediaPanelKind::Camera,
        )
    }

    pub fn photo() -> Self {
        MediaPanel(
            Stack::center(),
            Self::rect(PANEL_W + 4.0, PANEL_H + 4.0, PANEL_R + 2.0, Color(255, 255, 255, 14)),
            Some(Self::rect(PANEL_W, PANEL_H, PANEL_R, SURFACE)),
            None,
            MediaPanelKind::Photo,
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct MediaPanels(Row, MediaPanel, MediaPanel);

impl OnEvent for MediaPanels {}

impl Default for MediaPanels {
    fn default() -> Self {
        MediaPanels(Row::center(20.0), MediaPanel::camera(), MediaPanel::photo())
    }
}

#[derive(Debug, Component, Clone)]
pub struct StatusBar(Stack, Shape, Text);

impl OnEvent for StatusBar {}

impl StatusBar {
    fn rect(w: f32, h: f32, radius: f32, color: Color) -> Shape {
        Shape { shape: ShapeType::RoundedRectangle(0.0, (w, h), 0.0, radius), color }
    }

    fn text(s: impl Into<String>, font: Arc<Font>, size: f32, color: Color) -> Text {
        Text::new(
            vec![Span::new(s.into(), size, Some(size * 1.35), font, color, 0.0)],
            None,
            Align::Center,
            None,
        )
    }

    pub fn new(font: Arc<Font>) -> Self {
        StatusBar(
            Stack::center(),
            Self::rect(280.0, 36.0, 18.0, Color(30, 30, 46, 200)),
            Self::text("Ready", font, 13.0, WHITE_DIM),
        )
    }

    pub fn set_message(&mut self, msg: impl Into<String>) {
        let font = self.2.spans[0].font.clone();
        self.2 = Self::text(msg, font, 13.0, WHITE_HI);
    }

    pub fn set_ok(&mut self, msg: impl Into<String>) {
        let font = self.2.spans[0].font.clone();
        self.2 = Self::text(msg, font, 13.0, Color(134, 239, 172, 230));
    }

    pub fn set_warn(&mut self, msg: impl Into<String>) {
        let font = self.2.spans[0].font.clone();
        self.2 = Self::text(msg, font, 13.0, Color(251, 191, 36, 230));
    }

    pub fn reset(&mut self) {
        let font = self.2.spans[0].font.clone();
        self.2 = Self::text("Ready", font, 13.0, WHITE_DIM);
    }
}

#[derive(Debug, Component, Clone)]
pub struct BtnContent(Column, Text, Text);

impl OnEvent for BtnContent {}

impl BtnContent {
    fn text(s: impl Into<String>, font: Arc<Font>, size: f32, color: Color) -> Text {
        Text::new(
            vec![Span::new(s.into(), size, Some(size * 1.35), font, color, 0.0)],
            None,
            Align::Center,
            None,
        )
    }

    fn icon(glyph: &str, fa: Arc<Font>, color: Color) -> Text {
        Text::new(
            vec![Span::new(glyph.to_string(), 22.0, Some(26.0), fa, color, 0.0)],
            None,
            Align::Center,
            None,
        )
    }

    pub fn new(glyph: &str, label: &str, fa: Arc<Font>, ui: Arc<Font>, color: Color) -> Self {
        BtnContent(
            Column::center(4.0),
            Self::icon(glyph, fa, color),
            Self::text(label, ui, 11.0, color),
        )
    }
}

#[derive(Clone)]
pub struct Callback(Arc<dyn Fn(&mut Context, bool) + Send + Sync>);

impl std::fmt::Debug for Callback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Callback")
    }
}

impl Callback {
    pub fn new(f: impl Fn(&mut Context, bool) + Send + Sync + 'static) -> Self {
        Callback(Arc::new(f))
    }
}

impl std::ops::Deref for Callback {
    type Target = dyn Fn(&mut Context, bool) + Send + Sync;
    fn deref(&self) -> &Self::Target { &*self.0 }
}

#[derive(Debug, Component, Clone)]
pub struct Btn(
    Stack,
    Shape,
    BtnContent,
    #[skip] bool,
    #[skip] bool,
    #[skip] Callback,
);

impl OnEvent for Btn {
    fn on_event(&mut self, ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(MouseEvent { state: MouseState::Released, position: Some(_) }) = event.downcast_ref::<MouseEvent>() {
            if self.4 {
                self.3 = !self.3;
                self.1 = Self::bg(self.1.shape.size().0, self.3);
            }
            (self.5)(ctx, self.3);
            ctx.trigger_haptic();
        }
        vec![event]
    }
}

impl Btn {
    fn bg(size: f32, active: bool) -> Shape {
        Shape {
            shape: ShapeType::RoundedRectangle(0.0, (size, size), 0.0, 18.0),
            color: if active { Color(99, 102, 241, 200) } else { Color(30, 30, 46, 255) },
        }
    }

    pub fn size(n: u8) -> f32 {
        (TOOLBAR_W - BTN_GAP * (n as f32 + 1.0)) / n as f32
    }

    pub fn new(
        content: BtnContent,
        size: f32,
        toggle: bool,
        on_press: impl Fn(&mut Context, bool) + Send + Sync + 'static,
    ) -> Self {
        Btn(
            Stack::new(Offset::Center, Offset::Center, Size::Static(size), Size::Static(size), Padding::default()),
            Self::bg(size, false),
            content,
            false,
            toggle,
            Callback::new(on_press),
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct Toolbar(Row, Vec<Btn>);

impl OnEvent for Toolbar {}

impl Toolbar {
    pub fn new(buttons: Vec<Btn>) -> Self {
        Toolbar(Row::center(24.0), buttons)
    }
}
