pub use prism;

pub use maverick_os;
pub use prism::drawable::Drawable;
pub use include_dir::Dir;

use wgpu_canvas::{Canvas, Area, Shape, ShapeType, Item, Image};
use prism::{Instance, Handler, Camera, event};
use maverick_os::{air, window, Application, Context};
use crate::maverick_os::hardware::SafeAreaInsets;
use air::{Contracts, Name, Id, Request, Substance, RequestBuilder};
use window::{
    Renderer, Handle, Input, TouchPhase, Touch, MouseScrollDelta, ElementState, Key, NamedKey,
    MouseButton,
};

use std::path::PathBuf;
use std::marker::PhantomData;

pub trait Builder: 'static {
    fn build(ctx: &mut prism::Context) -> Box<dyn Drawable>;
    fn contracts() -> Contracts;
}

pub struct RampContext<'a>(&'a mut Context);
impl Handler for RampContext<'_> {
    fn me(&self) -> Name {self.0.air.name()}
    fn builder(&self) -> &RequestBuilder {self.0.air.builder()}
    fn request(&mut self, request: Request) {self.0.air.request(request);}
    fn list(&self, c_id: Id) -> Vec<Id> {self.0.air.list(&c_id)}
    fn get(&self, c_id: Id, id: Id, path: PathBuf) -> Option<Substance> {
        self.0.air.query(&c_id, &id, path)
    }
    fn start_camera(&mut self) -> Box<dyn Camera> {Box::new(self.0.hardware.camera.start())}
    fn pick_photo(&mut self) { self.0.hardware.photo_picker.open(); }
    fn get_safe_area(&self) -> (f32, f32, f32, f32) { SafeAreaInsets::get() }
    fn share_social(&mut self, data: String) { self.0.hardware.share.share(&data); }
    fn set_clipboard(&mut self, data: String) { self.0.hardware.clipboard.set(data); }
    fn get_clipboard(&self) -> Option<String> { self.0.hardware.clipboard.get() }
    fn trigger_haptic(&self) { self.0.hardware.haptics.vibrate() }
}

use std::future::Future;
use std::sync::Arc;
use std::task::{Context as TContext, Poll, Wake};
use std::thread::{self, Thread};
use core::pin::pin;

struct ThreadWaker(Thread);
impl Wake for ThreadWaker { fn wake(self: Arc<Self>) { self.0.unpark(); } }

pub struct RampRenderer<'surface, B> {
    canvas: Canvas<'surface>,
    _p: PhantomData<B>,
    _surface: PhantomData<&'surface ()>,
}

impl<'surface, B: Builder> Renderer<'surface> for RampRenderer<'surface, B> {
    type Application = Ramp<B>;

    fn new(context: &window::Context, handle: &'surface dyn Handle) -> Self {
        let mut fut = pin!(Canvas::new(handle, context.width, context.height));
        let t = thread::current();
        let waker = Arc::new(ThreadWaker(t)).into();
        let mut cx = TContext::from_waker(&waker);
        let canvas = loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(res) => break res,
                Poll::Pending => thread::park(),
            }
        };
        RampRenderer { canvas, _p: PhantomData::<B>, _surface: PhantomData }
    }

    fn resize(&mut self, context: &window::Context) {
        self.canvas.resize(context.width, context.height);
    }

    fn draw(&mut self, _context: &window::Context, app: &Ramp<B>) {
        self.canvas.draw(app.get_drawables());
    }
}

pub struct Ramp<B> {
    instance:     Instance,
    items:        Vec<(Area, Item)>,
    touching:     bool,
    mouse:        (f32, f32),
    scroll:       Option<(f32, f32)>,
    scale_factor: f64,
    modifiers:    event::Modifiers,
    _p:           PhantomData<fn() -> B>,
}

impl<B: Builder> Ramp<B> {
    pub fn physical(&self, x: f32) -> f32 { (x as f64 * self.scale_factor) as f32 }
    pub fn logical(&self, x: f32) -> f32  { (x as f64 / self.scale_factor) as f32 }

    fn shape(&self, shape: ShapeType) -> ShapeType {
        match shape {
            ShapeType::Ellipse(s, (w, h), a) =>
                ShapeType::Ellipse(self.physical(s), (self.physical(w), self.physical(h)), a),
            ShapeType::Rectangle(s, (w, h), a) =>
                ShapeType::Rectangle(self.physical(s), (self.physical(w), self.physical(h)), a),
            ShapeType::RoundedRectangle(s, (w, h), a, c) =>
                ShapeType::RoundedRectangle(self.physical(s), (self.physical(w), self.physical(h)), a, self.physical(c)),
        }
    }

    fn get_drawables(&self) -> Vec<(Area, Item)> {
        self.items.clone().into_iter().map(|(a, i)| {
            (Area {
                offset: (self.physical(a.offset.0), self.physical(a.offset.1)),
                bounds: a.bounds.map(|b| (self.physical(b.0), self.physical(b.1), self.physical(b.2), self.physical(b.3))),
            }, match i {
                Item::Shape(shape) => Item::Shape(Shape {
                    shape: self.shape(shape.shape),
                    color: shape.color,
                }),
                Item::Image(image) => Item::Image(Image {
                    shape: self.shape(image.shape),
                    image: image.image,
                    color: image.color,
                }),
                Item::Text(mut text) => Item::Text({
                    text.width = text.width.map(|w| self.physical(w));
                    text.spans.iter_mut().for_each(|span| {
                        span.font_size   = self.physical(span.font_size);
                        span.line_height = span.line_height.map(|l| self.physical(l));
                        span.kerning     = self.physical(span.kerning);
                    });
                    text
                }),
            })
        }).collect()
    }

    /// Emit a scroll event with the given dx/dy in logical pixels.
    fn emit_scroll(&mut self, dx: f32, dy: f32) {
        self.instance.emit(event::MouseEvent {
            position: Some(self.mouse),
            state:    event::MouseState::Scroll(dx, dy),
            button:   None,
        });
    }
}

impl<B: Builder> Application for Ramp<B> {
    type Renderer<'surface> = RampRenderer<'surface, B>;

    fn new(ctx: &mut Context) -> Self {
        let scale_factor = ctx.window.scale_factor;
        let screen = (
            (ctx.window.width  as f64 / scale_factor) as f32,
            (ctx.window.height as f64 / scale_factor) as f32,
        );
        let instance = Instance::new(B::build, &mut RampContext(ctx), screen);
        Ramp {
            instance,
            items:        Vec::new(),
            touching:     false,
            mouse:        (0.0, 0.0),
            scroll:       None,
            scale_factor,
            modifiers:    event::Modifiers::default(),
            _p:           PhantomData::<fn() -> B>,
        }
    }

    fn on_input(&mut self, ctx: &mut Context, input: Input) {
        match input {
            Input::Tick => {
                self.items = self.instance.draw(&mut RampContext(ctx));
            }
            Input::Resized => {
                self.scale_factor = ctx.window.scale_factor;
                self.instance.resize((
                    self.logical(ctx.window.width  as f32),
                    self.logical(ctx.window.height as f32),
                ));
            }
            Input::CameraFrame(image) => self.instance.emit(event::CameraFrame(image)),
            Input::Photo(image)       => self.instance.emit(event::PickedPhoto(image)),
            Input::ModifiersChanged(mods) => {
                self.modifiers = event::Modifiers {
                    shift:   mods.state().shift_key(),
                    control: mods.state().control_key(),
                    alt:     mods.state().alt_key(),
                    meta:    mods.state().super_key(),
                };
            }
            Input::Touch(Touch { location, phase, .. }) => {
                let location = (location.x as f32, location.y as f32);
                let position = (self.logical(location.0), self.logical(location.1));
                if let Some(state) = match phase {
                    TouchPhase::Started => {
                        self.scroll  = Some(position);
                        self.touching = true;
                        Some(event::MouseState::Pressed)
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.touching = false;
                        Some(event::MouseState::Released)
                    }
                    TouchPhase::Moved => {
                        self.scroll.and_then(|(prev_x, prev_y)| {
                            self.scroll = Some(position);
                            let dx = position.0 - prev_x;
                            let dy = position.1 - prev_y;
                            let scroll_x = -dx;
                            let scroll_y = -dy;
                            (scroll_x.abs() > 0.01 || scroll_y.abs() > 0.01)
                                .then_some(event::MouseState::Scroll(scroll_x, scroll_y))
                        })
                    }
                } {
                    self.instance.emit(event::MouseEvent {
                        position: Some(position),
                        state,
                        button: None,
                    });
                }
                self.mouse = position;
            }
            Input::CursorMoved { position, .. } => {
                let position = (self.logical(position.0 as f32), self.logical(position.1 as f32));
                if self.mouse != position {
                    self.mouse = position;
                    self.instance.emit(event::MouseEvent {
                        position: Some(position),
                        state:    event::MouseState::Moved,
                        button:   None,
                    });
                }
            }
            Input::Mouse { state, button, .. } => {
                let btn = match button {
                    MouseButton::Left   => event::MouseButton::Left,
                    MouseButton::Right  => event::MouseButton::Right,
                    MouseButton::Middle => event::MouseButton::Middle,
                    _                   => event::MouseButton::Left,
                };
                let ms = match state {
                    ElementState::Pressed  => event::MouseState::Pressed,
                    ElementState::Released => event::MouseState::Released,
                };
                self.instance.emit(event::MouseButtonEvent {
                    position: Some(self.mouse),
                    state:    ms,
                    button:   btn,
                });
                self.instance.emit(event::MouseEvent {
                    position: Some(self.mouse),
                    state:    ms,
                    button:   Some(btn),
                });
            }
            Input::MouseWheel { delta, phase, .. } => {
                let sf = self.scale_factor as f32;

                match delta {
                    // ── Physical mouse wheel (LineDelta) ─────────────────────
                    // Each notch fires immediately — no TouchPhase gating needed.
                    // Positive y = scrolled up (content moves down), so negate.
                    MouseScrollDelta::LineDelta(x, y) => {
                        // One notch ≈ 3 lines ≈ ~60 logical pixels at default
                        const LINE_PX: f32 = 60.0;
                        let dx = -x * LINE_PX;
                        let dy = -y * LINE_PX;
                        if dx.abs() > 0.01 || dy.abs() > 0.01 {
                            self.emit_scroll(dx, dy);
                        }
                    }
                    // ── Trackpad / pixel-precise scroll (PixelDelta) ─────────
                    // Accumulate across TouchPhase::Moved ticks as before.
                    MouseScrollDelta::PixelDelta(p) => {
                        match phase {
                            TouchPhase::Started => {
                                self.scroll = Some((0.0, 0.0));
                            }
                            TouchPhase::Moved => {
                                let prev = self.scroll.unwrap_or((0.0, 0.0));
                                let scroll_x = prev.0 + (-(p.x as f32) * 0.2);
                                let scroll_y = prev.1 + (-(p.y as f32) * 0.2);
                                self.scroll = Some((scroll_x, scroll_y));
                                let dx = scroll_x * sf;
                                let dy = scroll_y * sf;
                                if dx.abs() > 0.01 || dy.abs() > 0.01 {
                                    self.emit_scroll(dx, dy);
                                }
                            }
                            TouchPhase::Ended | TouchPhase::Cancelled => {
                                self.scroll = None;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Input::Keyboard { event, .. } => {
                if let Some(key) = match event.logical_key {
                    Key::Named(named) => match named {
                        NamedKey::Enter      => Some(event::NamedKey::Enter),
                        NamedKey::Tab        => Some(event::NamedKey::Tab),
                        NamedKey::Space      => Some(event::NamedKey::Space),
                        NamedKey::ArrowDown  => Some(event::NamedKey::ArrowDown),
                        NamedKey::ArrowLeft  => Some(event::NamedKey::ArrowLeft),
                        NamedKey::ArrowRight => Some(event::NamedKey::ArrowRight),
                        NamedKey::ArrowUp    => Some(event::NamedKey::ArrowUp),
                        NamedKey::Delete     => Some(event::NamedKey::Delete),
                        NamedKey::Shift      => Some(event::NamedKey::Shift),
                        NamedKey::Control    => Some(event::NamedKey::Control),
                        NamedKey::Alt        => Some(event::NamedKey::Alt),
                        NamedKey::Super | NamedKey::Hyper | NamedKey::Meta
                                             => Some(event::NamedKey::Meta),
                        NamedKey::CapsLock   => Some(event::NamedKey::CapsLock),
                        NamedKey::NumLock    => Some(event::NamedKey::NumLock),
                        NamedKey::Backspace  => Some(event::NamedKey::Backspace),
                        NamedKey::Home       => Some(event::NamedKey::Home),
                        NamedKey::End        => Some(event::NamedKey::End),
                        NamedKey::ScrollLock => Some(event::NamedKey::ScrollLock),
                        NamedKey::Escape     => Some(event::NamedKey::Escape),
                        _                    => None,
                    }.map(event::Key::Named),
                    Key::Character(c)    => Some(event::Key::Character(c.to_string())),
                    Key::Unidentified(_) => None,
                    Key::Dead(_)         => None,
                } {
                    self.instance.emit(event::KeyboardEvent {
                        key,
                        state: match event.state {
                            ElementState::Pressed if event.repeat => event::KeyboardState::Repeated,
                            ElementState::Pressed                 => event::KeyboardState::Pressed,
                            ElementState::Released                => event::KeyboardState::Released,
                        },
                        modifiers: self.modifiers,
                    });
                }
            }
            _ => {}
        }
    }

    fn contracts() -> Contracts { B::contracts() }
}

#[doc(hidden)]
pub mod __private {
    pub use crate::{Builder, Ramp, prism::drawable::Drawable, maverick_os, maverick_os::air::Contracts};
}

#[macro_export]
macro_rules! run {
    ([$($c:ty),* $(,)?]; $($app:tt)*) => {
        pub use $crate::__private::*;
        struct PrismBuilder;
        impl $crate::__private::Builder for PrismBuilder {
            fn build(ctx: &mut prism::Context) -> Box<dyn $crate::__private::Drawable> {
                Box::new(({$($app)*})(ctx))
            }
            fn contracts() -> $crate::__private::Contracts {
                $crate::__private::Contracts::new()$(.add::<$c>())?
            }
        }
        $crate::__private::maverick_os::start!($crate::__private::Ramp<PrismBuilder>);
    };
}