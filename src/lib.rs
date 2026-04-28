pub use prism;

pub use maverick_os;
pub use prism::drawable::Drawable;
pub use include_dir::Dir;

use wgpu_canvas::{Canvas, Area, Shape, ShapeType, Item, Image};
use prism::Handler;
use prism::event::{KeyboardState, MouseState, MouseEvent, KeyboardEvent, TickEvent};
use prism::drawable::SizedTree;
use air::{Name, Id, Request, Substance, RequestBuilder};

use maverick_os::{hardware, air, window, Application, Context};
use window::{
    Renderer, Handle, Input, TouchPhase, Touch, MouseScrollDelta, ElementState, Key, NamedKey
};
use air::Contracts;

use std::path::PathBuf;
use std::marker::PhantomData;
use std::time::Instant;

pub use include_dir;

pub trait Builder: 'static {
    fn build(ctx: &mut prism::Context) -> Box<dyn Drawable>;
    fn contracts() -> Contracts;
}

pub struct RampHandler(hardware::Context, air::Context);
impl RampHandler {
    pub fn new(ctx: &Context) -> Self {
        RampHandler(ctx.hardware.clone(), ctx.air.clone())
    }
}
impl Handler for RampHandler {
    fn me(&self) -> Name {self.1.name()}

    fn builder(&self) -> &RequestBuilder {self.1.builder()}
    fn request(&self, request: Request) {self.1.request(request);}
    fn list(&self, c_id: Id) -> Vec<Id> {self.1.list(&c_id)}
    fn get(&self, c_id: Id, id: Id, path: PathBuf) -> Option<Substance> {
        self.1.query(&c_id, &id, path)
    }

    fn start_camera(&self) {
        todo!()
    }
    fn stop_camera(&self) {
        if let Some(camera) = self.0.camera_existing() { camera.stop(); }
    }
    fn pick_photo(&self) {
        self.0.photo_picker();
    }

    fn get_safe_area(&self) -> (f32, f32, f32, f32) {self.0.safe_area_insets()}
    fn share_social(&self, data: String) {self.0.share(&data)}
    fn set_clipboard(&self, data: String) {self.0.clipboard().set(data)}
    fn get_clipboard(&self) -> Option<String> {self.0.clipboard().get()}
    fn trigger_haptic(&self) {self.0.haptic()}
}

use std::future::Future;
use std::sync::Arc;
use std::task::{Context as TContext, Poll, Wake};
use std::thread::{self, Thread};
use core::pin::pin;

struct ThreadWaker(Thread);
impl Wake for ThreadWaker {fn wake(self: Arc<Self>) {self.0.unpark();}}

pub struct RampRenderer<'surface, B> {
    canvas: Canvas<'surface>,
    _p: PhantomData<B>
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

        RampRenderer {canvas, _p: PhantomData::<B>}
    }

    fn resize(&mut self, context: &window::Context) {
        self.canvas.resize(context.width, context.height);
    }

    fn draw(&mut self, _context: &window::Context, app: &Ramp<B>) {
        self.canvas.draw(app.get_drawables());
    }
}


pub struct Ramp<B>{
    app: Box<dyn Drawable>,
    context: prism::Context,
    touching: bool,
    mouse: (f32, f32),
    scroll: Option<(f32, f32)>,
    screen: (f32, f32),
    sized_app: SizedTree,
    scale_factor: f64,
    timer: Instant,
    modifiers: prism::event::Modifiers,
    _p: PhantomData::<fn() -> B>
}
impl<B: Builder> Ramp<B> {
    pub fn physical(&self, x: f32) -> f32 {(x as f64 * self.scale_factor) as f32}
    pub fn logical(&self, x: f32) -> f32 {(x as f64 / self.scale_factor) as f32}

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
        let drawn = self.app.draw(&self.sized_app, (0.0, 0.0), (0.0, 0.0, self.screen.0, self.screen.1));
        drawn.into_iter().map(|(a, i)| {
            (Area{
                offset: (self.physical(a.offset.0), self.physical(a.offset.1)),
                bounds: a.bounds.map(|b| (self.physical(b.0), self.physical(b.1), self.physical(b.2), self.physical(b.3)))
            }, match i {
                Item::Shape(shape) => Item::Shape(Shape{
                    shape: self.shape(shape.shape),
                    color: shape.color
                }),
                Item::Image(image) => Item::Image(Image{
                    shape: self.shape(image.shape),
                    image: image.image,
                    color: image.color
                }),
                Item::Text(mut text) => Item::Text({
                    text.width = text.width.map(|w| self.physical(w));
                    text.spans.iter_mut().for_each(|span| {
                        span.font_size = self.physical(span.font_size);
                        span.line_height = span.line_height.map(|l| self.physical(l));
                        span.kerning = self.physical(span.kerning);
                    });
                    text
                })
            })
        }).collect()
    }
}
impl<B: Builder> Application for Ramp<B> {
    type Renderer<'surface> = RampRenderer<'surface, B>;

    fn new(ctx: &Context) -> Self {
        let handler = RampHandler::new(ctx);
        let mut context = prism::Context::new(handler);
        let scale_factor = ctx.window.scale_factor;
        let screen = (
            (ctx.window.width as f64 / scale_factor) as f32,
            (ctx.window.height as f64 / scale_factor) as f32,
        );

        let app = B::build(&mut context);
        let size_request = app.request_size();
        let sized_app = app.build(screen, size_request);
        Ramp{
            app,
            context,
            touching: false,
            mouse: (0.0, 0.0),
            screen,
            sized_app,
            scroll: None,
            scale_factor,
            timer: Instant::now(),
            modifiers: prism::event::Modifiers::default(),
            _p: PhantomData::<fn() -> B>
        }
    }

    fn on_input(&mut self, ctx: &Context, input: Input) {
        if let Some(event) = match input {
            Input::Tick => {
                self.app.event(&mut self.context, &self.sized_app, Box::new(TickEvent));
                self.timer = Instant::now();
                for event in self.context.1.drain(..).rev().collect::<Vec<_>>() {
                    if let Some(event) = event
                        .pass(&mut self.context, &[prism::layout::Area{offset: (0.0, 0.0), size: self.sized_app.0}])
                        .remove(0)
                    {
                        self.app.event(&mut self.context, &self.sized_app, event);
                    }
                }

                let size_request = self.app.request_size();
                self.sized_app = self.app.build(self.screen, size_request);
                None
            },
            Input::Resized => {
                self.scale_factor = ctx.window.scale_factor;
                self.screen = (self.logical(ctx.window.width as f32), self.logical(ctx.window.height as f32));
                let size_request = self.app.request_size();
                self.sized_app = self.app.build(self.screen, size_request);
                None
            },
            Input::CameraFrame(image) => {
                self.app.event(
                    &mut self.context,
                    &self.sized_app,
                    Box::new(prism::event::CameraFrame(image))
                );
                None
            },
            Input::PickedPhoto(image, success) => {
                if success {
                    self.app.event(
                        &mut self.context,
                        &self.sized_app,
                        Box::new(prism::event::PickedPhoto(image))
                    );
                }
                None
            },
            Input::ModifiersChanged(mods) => {
                self.modifiers = prism::event::Modifiers {
                    shift: mods.state().shift_key(),
                    control: mods.state().control_key(),
                    alt: mods.state().alt_key(),
                    meta: mods.state().super_key(),
                };
                None
            },
            Input::Touch(Touch { location, phase, .. }) => {
                let location = (location.x as f32, location.y as f32);
                let position = (self.logical(location.0), self.logical(location.1));
                let event = match phase {
                    TouchPhase::Started => {
                        self.scroll = Some(position);
                        self.touching = true;
                        Some(MouseState::Pressed)
                    },
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.touching = false;
                        Some(MouseState::Released)
                    },
                    TouchPhase::Moved => {
                        self.scroll.and_then(|(prev_x, prev_y)| {
                            self.scroll = Some(position);
                            let dx = position.0 - prev_x;
                            let dy = position.1 - prev_y;
                            let scroll_x = -dx * 1.0;
                            let scroll_y = -dy * 1.0;

                            (scroll_x.abs() > 0.01 || scroll_y.abs() > 0.01).then_some(
                                MouseState::Scroll(scroll_x, scroll_y)
                            )
                        })
                    }
                }.map(|state| Box::new(MouseEvent{position: Some(position), state}) as Box<dyn prism::event::Event>);
                self.mouse = position;
                event
            },
            Input::CursorMoved{position, ..} => {
                let position = (self.logical(position.0 as f32), self.logical(position.1 as f32));
                (self.mouse != position).then_some({
                    self.mouse = position;
                    Box::new(MouseEvent{position: Some(position), state: MouseState::Moved}) as Box<dyn prism::event::Event>
                })
            },
            Input::Mouse{state, ..} => {
                Some(Box::new(MouseEvent{position: Some(self.mouse), state: match state {
                    ElementState::Pressed => MouseState::Pressed,
                    ElementState::Released => MouseState::Released,
                }}) as Box<dyn prism::event::Event>)
            },
            Input::MouseWheel{delta, phase, ..} => {
                match phase {
                    TouchPhase::Started => {
                        self.scroll = Some((0.0, 0.0));
                        None
                    }
                    TouchPhase::Moved => {
                        self.scroll.map(|(prev_x, prev_y)| {
                            let pos = match delta {
                                MouseScrollDelta::LineDelta(x, y) => (x.signum(), y.signum()),
                                MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                            };

                            let scroll_x = prev_x + (-pos.0 * 0.2);
                            let scroll_y = prev_y + (-pos.1 * 0.2);

                            let sf = ctx.window.scale_factor as f32;
                            let state = MouseState::Scroll(scroll_x * sf, scroll_y * sf);

                            Box::new(MouseEvent{ position: Some(self.mouse), state }) as Box<dyn prism::event::Event>
                        })
                    },
                    _ => None
                }
            },
            Input::Keyboard{event, ..} => {
                Some(event).and_then(|event| Some(Box::new(KeyboardEvent{
                    key: match event.logical_key {
                        Key::Named(named) => Some(prism::event::Key::Named(match named {
                            NamedKey::Enter => Some(prism::event::NamedKey::Enter),
                            NamedKey::Tab => Some(prism::event::NamedKey::Tab),
                            NamedKey::Space => Some(prism::event::NamedKey::Space),
                            NamedKey::ArrowDown => Some(prism::event::NamedKey::ArrowDown),
                            NamedKey::ArrowLeft => Some(prism::event::NamedKey::ArrowLeft),
                            NamedKey::ArrowRight => Some(prism::event::NamedKey::ArrowRight),
                            NamedKey::ArrowUp => Some(prism::event::NamedKey::ArrowUp),
                            NamedKey::Delete | NamedKey::Backspace => Some(prism::event::NamedKey::Delete),
                            NamedKey::Shift => Some(prism::event::NamedKey::Shift),
                            NamedKey::Control => Some(prism::event::NamedKey::Control),
                            NamedKey::Alt => Some(prism::event::NamedKey::Alt),
                            NamedKey::Super | NamedKey::Hyper | NamedKey::Meta => Some(prism::event::NamedKey::Meta),
                            NamedKey::CapsLock => Some(prism::event::NamedKey::CapsLock),
                            NamedKey::NumLock => Some(prism::event::NamedKey::NumLock),
                            NamedKey::Backspace => Some(prism::event::NamedKey::Backspace),
                            NamedKey::Home => Some(prism::event::NamedKey::Home),
                            NamedKey::End => Some(prism::event::NamedKey::End),
                            NamedKey::ScrollLock => Some(prism::event::NamedKey::ScrollLock),
                            _ => None
                        }?)),
                        Key::Character(c) => Some(prism::event::Key::Character(c.to_string())),
                        Key::Unidentified(_) => None,
                        Key::Dead(_) => None,
                    }?,
                    state: match event.state {
                        ElementState::Pressed if event.repeat => KeyboardState::Repeated,
                        ElementState::Pressed => KeyboardState::Pressed,
                        ElementState::Released => KeyboardState::Released,
                    },
                    modifiers: self.modifiers,
                }) as Box<dyn prism::event::Event>))
            },
            _ => None
        } {self.context.1.push(event);}
    }

    fn contracts() -> Contracts {B::contracts()}
}


#[doc(hidden)]
pub mod __private {
    pub use crate::{Builder, Ramp, prism::drawable::Drawable, maverick_os, include_dir, maverick_os::air::Contracts};
}

#[macro_export]
macro_rules! run {
    ([$($c:ty)?]; $($app:tt)*) => {
        pub use $crate::__private::*;
        struct PrismBuilder;
        impl Builder for PrismBuilder {
            fn build(ctx: &mut prism::Context) -> Box<dyn Drawable> {
                let resources = include_dir::include_dir!("$CARGO_MANIFEST_DIR/resources");
                Box::new(({$($app)*})(ctx, Assets(resources)))
            }

            fn contracts() -> Contracts {
                Contracts::new()$(.add::<$c>())?
            }
        }

        maverick_os::start!(Ramp<PrismBuilder>);
    };
}