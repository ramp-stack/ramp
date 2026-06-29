use ramp::prism;
use prism::{Context, canvas::{Image, Shape, Text, ShapeType, Color, Font, Align}};
use prism::drawable::{Component, SizedTree};
use prism::layout::{Stack, Row, Column, Offset, Size, Padding};
use prism::event::{OnEvent, Event, CameraFrame, PickedPhoto, MouseEvent, MouseState, MouseButton};
use std::sync::{Arc, Mutex};
use std::rc::Rc;
use std::ops::Deref;
use std::fmt;
use crate::prism::Camera;

// === COLORS & CONSTANTS ===
pub const SURFACE:    Color = Color(22,  22,  32,  255);
pub const ACCENT_DIM: Color = Color(99,  102, 241, 60 );
pub const ACCENT_ACT: Color = Color(99,  102, 241, 200);
pub const WHITE_HI:   Color = Color(255, 255, 255, 230);
pub const WHITE_DIM:  Color = Color(255, 255, 255, 80 );
pub const WHITE_MID:  Color = Color(255, 255, 255, 160);
pub const BTN_BG:     Color = Color(30,  30,  46,  255);

pub const PANEL_W: f32 = 186.0;
pub const PANEL_H: f32 = 280.0;
pub const PANEL_R: f32 = 20.0;
pub const TOOLBAR_W: f32 = 360.0;
pub const BTN_GAP: f32 = 10.0;
pub const ROW_H: f32 = 52.0;
pub const ROW_R: f32 = 14.0;

const FA_COPY:  &str = "\u{f0c5}";
const FA_PASTE: &str = "\u{f0ea}";
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
    pub fn new() -> Self {
        let ui_font = Font::from_bytes(include_bytes!("../resources/font.ttf")).unwrap();
        let fa_font = Font::from_bytes(include_bytes!("../resources/fa-solid-900.ttf")).unwrap();

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

        App(
            Column::center(24.0),
            MediaPanels::default(),
            toolbar,
            copy_paste,
        )
    }
}

fn rounded_rect(w: f32, h: f32, r: f32, color: Color) -> Shape {
    Shape { 
        shape: ShapeType::RoundedRectangle(0.0, (w, h), 0.0, r), 
        color 
    }
}

pub struct Cb<F: ?Sized>(Arc<F>);

impl<F: ?Sized> Clone for Cb<F> {
    fn clone(&self) -> Self {
        Cb(Arc::clone(&self.0))
    }
}

impl<F: ?Sized> Deref for Cb<F> {
    type Target = F;
    fn deref(&self) -> &F { &self.0 }
}

impl<F: ?Sized> fmt::Debug for Cb<F> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { 
        write!(f, "Cb") 
    }
}

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
    pub fn new(kind: MediaPanelKind) -> Self {
        let (border_color, has_placeholder) = match kind {
            MediaPanelKind::Camera => (ACCENT_DIM, true),
            MediaPanelKind::Photo => (Color(255, 255, 255, 14), true),
        };
        
        MediaPanel(
            Stack::center(),
            rounded_rect(PANEL_W + 4.0, PANEL_H + 4.0, PANEL_R + 2.0, border_color),
            has_placeholder.then(|| rounded_rect(PANEL_W, PANEL_H, PANEL_R, SURFACE)),
            None,
            kind,
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct MediaPanels(Row, MediaPanel, MediaPanel);

impl OnEvent for MediaPanels {}

impl Default for MediaPanels {
    fn default() -> Self {
        MediaPanels(
            Row::center(20.0), 
            MediaPanel::new(MediaPanelKind::Camera), 
            MediaPanel::new(MediaPanelKind::Photo)
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct BtnContent(Column, Text, Text);

impl OnEvent for BtnContent {}

impl BtnContent {
    pub fn new(glyph: &str, label: &str, fa: Font, ui: Font, color: Color) -> Self {
        BtnContent(
            Column::center(4.0),
            Text::new(glyph, fa, 22.0, color, Align::Center),
            Text::new(label, ui, 11.0, color, Align::Center)
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
enum BtnMode { 
    Impulse,
    Camera,
}

type Cam = Option<Box<dyn Camera>>;
type Callback = Cb<dyn Fn(&mut Context, bool) + Send + Sync>;
#[derive(Debug, Component, Clone)]
pub struct Btn(
    Stack,
    Shape,
    BtnContent,
    #[skip] bool, 
    #[skip] BtnMode, 
    #[skip] Callback,
    #[skip] Option<Rc<Mutex<Cam>>>,
);

impl OnEvent for Btn {
    fn on_event(&mut self, ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(MouseEvent { state: MouseState::Released(MouseButton::Left), position: Some(_) }) = event.downcast_ref::<MouseEvent>() {
            match self.4 {
                BtnMode::Camera => {
                    self.3 = !self.3;
                    if self.3 {
                        if let Some(cam_handle) = &self.6 {
                            *cam_handle.lock().unwrap() = Some(ctx.start_camera());
                        }
                    } else {
                        if let Some(cam_handle) = &self.6 {
                            *cam_handle.lock().unwrap() = None;
                        }
                    }
                    self.1 = Self::bg(self.1.shape.size().0, self.3);
                    (self.5)(ctx, self.3);
                }
                BtnMode::Impulse => {
                    (self.5)(ctx, false);
                }
            }
            ctx.trigger_haptic();
        }
        vec![event]
    }
}

impl Btn {
    fn bg(size: f32, active: bool) -> Shape {
        rounded_rect(size, size, 18.0, if active { ACCENT_ACT } else { BTN_BG })
    }

    pub fn size(n: u8) -> f32 {
        (TOOLBAR_W - BTN_GAP * (n as f32 + 1.0)) / n as f32
    }

    fn new_internal(
        content: BtnContent,
        size: f32,
        mode: BtnMode,
        on_press: impl Fn(&mut Context, bool) + Send + Sync + 'static,
    ) -> Self {
        let camera_handle = if matches!(mode, BtnMode::Camera) {
            Some(Rc::new(Mutex::new(None)))
        } else {
            None
        };

        Btn(
            Stack::new(Offset::Center, Offset::Center, Size::Static(size), Size::Static(size), Padding::default()),
            Self::bg(size, false),
            content,
            false,
            mode,
            Cb(Arc::new(on_press)),
            camera_handle,
        )
    }

    pub fn camera(content: BtnContent, size: f32) -> Self {
        Self::new_internal(content, size, BtnMode::Camera, |_, _| {})
    }

    pub fn impulse(
        content: BtnContent,
        size: f32,
        on_press: impl Fn(&mut Context, bool) + Send + Sync + 'static,
    ) -> Self {
        Self::new_internal(content, size, BtnMode::Impulse, on_press)
    }
}

#[derive(Debug, Component, Clone)]
pub struct Toolbar(Row, Btn, Vec<Btn>);

impl OnEvent for Toolbar {}

impl Toolbar {
    pub fn new(camera: Btn, buttons: Vec<Btn>) -> Self {
        Toolbar(Row::center(BTN_GAP), camera, buttons)
    }
}

#[derive(Debug, Component, Clone)]
pub struct ActionRowContent(Row, Text, Text, Text);

impl OnEvent for ActionRowContent {}

type ActionCallback = Cb<dyn Fn(&mut Context) -> Option<(String, String)> + Send + Sync>;

#[derive(Debug, Component, Clone)]
pub struct ActionRow(
    Stack,
    Shape,
    ActionRowContent,
    #[skip] ActionCallback,
);

impl OnEvent for ActionRow {
    fn on_event(
        &mut self,
        ctx: &mut Context,
        _sized: &SizedTree,
        event: Box<dyn Event>,
    ) -> Vec<Box<dyn Event>> {
        if let Some(MouseEvent { state: MouseState::Released(MouseButton::Left), position: Some(_) }) = event.downcast_ref::<MouseEvent>() {
            if let Some((new_label, new_value)) = (self.3)(ctx) {
                let label_font = self.2.2.spans[0].font.clone();
                let value_font = self.2.3.spans[0].font.clone();
                
            
                self.2.2 = Text::new(&new_label, label_font, 13.0, WHITE_MID, Align::Center);
                self.2.3 = Text::new(&new_value, value_font, 13.0, WHITE_HI, Align::Center);
            }
            ctx.trigger_haptic();
        }
        vec![event]
    }
}

impl ActionRow {
    pub fn new(
        fa: Font,
        ui: Font,
        icon: &str,
        label: &str,
        value: &str,
        on_press: impl Fn(&mut Context) -> Option<(String, String)> + Send + Sync + 'static,
    ) -> Self {
        let content = ActionRowContent(
            Row::start(6.0),
            Text::new(icon, fa, 16.0, WHITE_MID, Align::Center),
            Text::new(label, ui.clone(), 13.0, WHITE_MID, Align::Center),
            Text::new(value, ui, 13.0, if value == "—" { WHITE_DIM } else { WHITE_HI }, Align::Center),
        );

        ActionRow(
            Stack::new(
                Offset::Center,
                Offset::Center,
                Size::Static(TOOLBAR_W),
                Size::Static(ROW_H),
                Padding::new(14.0),
            ),
            rounded_rect(TOOLBAR_W, ROW_H, ROW_R, BTN_BG),
            content,
            Cb(Arc::new(on_press)),
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct CopyPastePanel(Column, ActionRow, ActionRow);

impl OnEvent for CopyPastePanel {}

impl CopyPastePanel {
    pub fn new(
        fa: Font,
        ui: Font,
        copy_text: &str,
        on_copy: impl Fn(&mut Context, &str) + Send + Sync + 'static,
        on_paste: impl Fn(&mut Context, &dyn Fn(&str)) + Send + Sync + 'static,
    ) -> Self {
        let copy_text_owned = copy_text.to_string();
        
        let copy_row = ActionRow::new(
            fa.clone(),
            ui.clone(),
            FA_COPY,
            "Copy: ",
            copy_text,
            move |ctx| {
                on_copy(ctx, &copy_text_owned);
                None
            },
        );
        
        let paste_row = ActionRow::new(
            fa,
            ui,
            FA_PASTE,
            "Paste: ",
            "—",
            move |ctx| {
                let result: Mutex<Option<String>> = Mutex::new(None);
                on_paste(ctx, &|text: &str| {
                    *result.lock().unwrap() = Some(text.to_string());
                });
                result.into_inner().unwrap().map(|text| {
                    ("Pasted: ".to_string(), text)
                })
            },
        );
        
        CopyPastePanel(Column::center(8.0), copy_row, paste_row)
    }
}
