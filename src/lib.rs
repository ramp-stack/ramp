pub use air;
use air::Services;

pub use maverick_os;
pub use prism::drawable::Drawable;

pub use prism;

use wgpu_canvas::{Canvas, Instruction};
use maverick_os::{window, Application, Context, hardware::SafeAreaInsets};
use window::{Renderer, Handle, Input, DeviceInput, MouseState, MouseButton, KeyboardState, Key};
use prism::{Instance, Handler, Camera, event};

use std::marker::PhantomData;
use std::future::Future;
use std::sync::Arc;
use std::task::{Context as TContext, Poll, Wake};
use std::thread::{self, Thread};
use core::pin::pin;

pub trait Builder: 'static {
    fn services() -> Services;
    fn build(ctx: &mut prism::Context) -> Box<dyn Drawable>;
}

pub struct RampContext(Context);
impl Handler for RampContext {
    fn air(&self) -> &air::Context {&self.0.air}
    fn start_camera(&self) -> Box<dyn Camera> {
        Box::new(self.0.hardware.camera.start())
    }
    fn pick_photo(&self) {
        self.0.hardware.photo_picker.open();
    }
    fn get_safe_area(&self) -> (f32, f32, f32, f32) {
        SafeAreaInsets::get() 
    }
    fn share_social(&self, data: String) {
        self.0.hardware.share.share(&data);
    }
    fn set_clipboard(&self, data: String) {
        self.0.hardware.clipboard.set(data);
    }
    fn get_clipboard(&self) -> Option<String> {
        self.0.hardware.clipboard.get()
    }
    fn trigger_haptic(&self) {
        self.0.hardware.haptics.vibrate()
    }
}

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
        self.canvas.draw(app.instructions.clone().into_iter().map(|mut i| {
            i.scale(app.scale_factor); i
        }).collect());
    }
}

pub struct Ramp<B> {
    instance: Instance,
    instructions: Vec<Instruction>,
    scale_factor: f32,
    last_tick: std::time::Instant,
    _p: PhantomData<fn() -> B>,
}
impl<B: Builder> Application for Ramp<B> {
    type Renderer<'surface> = RampRenderer<'surface, B>;

    fn services() -> Services {B::services()}

    fn new(ctx: &Context) -> Self {
        let scale_factor = ctx.window.scale_factor as f32;
        let screen = (ctx.window.width as f32 / scale_factor, ctx.window.height as f32 / scale_factor);
        let instance = Instance::new(B::build, &mut RampContext(ctx.clone()), screen);
        Ramp {
            instance,
            instructions: Vec::new(),
            scale_factor,
            last_tick: std::time::Instant::now(),
            _p: PhantomData::<fn() -> B>,
        }
    }

    fn on_input(&mut self, ctx: &Context, input: Input) {
        match input {
            Input::Tick => {
                let now = std::time::Instant::now();
                let _dt = now.duration_since(self.last_tick).as_secs_f32().min(0.1);
                self.last_tick = now;
                self.instructions = self.instance.draw(&mut RampContext(ctx.clone()));
            }
            Input::Resized => {
                self.scale_factor = ctx.window.scale_factor as f32;
                self.instance.resize((
                    ctx.window.width as f32 / self.scale_factor,
                    ctx.window.height as f32 / self.scale_factor
                ));
            }
            Input::CameraFrame(image) => self.instance.emit(event::CameraFrame(image)),
            Input::SelectedPhoto(image) => self.instance.emit(event::PickedPhoto(image)),
            Input::Device(_, event) => match event.scale(1.0 / self.scale_factor) {
                DeviceInput::Keyboard(key, state, modifiers) => self.instance.emit(event::KeyboardEvent{key: match key {
                    Key::Escape => event::Key::Escape,
                    Key::Enter => event::Key::Enter,
                    Key::Tab => event::Key::Tab,
                    Key::Space => event::Key::Space,
                    Key::Up => event::Key::Up,
                    Key::Down => event::Key::Down,
                    Key::Left => event::Key::Left,
                    Key::Right => event::Key::Right,
                    Key::Delete => event::Key::Delete,
                    Key::Backspace => event::Key::Backspace,
                    Key::Home => event::Key::Home,
                    Key::End => event::Key::End,
                    Key::Shift => event::Key::Shift,
                    Key::Control => event::Key::Control,
                    Key::Alt => event::Key::Alt,
                    Key::SuperMeta => event::Key::SuperMeta,
                    Key::CapsLock => event::Key::CapsLock,
                    Key::NumLock => event::Key::NumLock,
                    Key::ScrollLock => event::Key::ScrollLock,
                    Key::Character(char) => event::Key::Character(char)
                }, state: match state {
                    KeyboardState::Pressed => event::KeyboardState::Pressed,
                    KeyboardState::Repeated => event::KeyboardState::Repeated,
                    KeyboardState::Released => event::KeyboardState::Released,
                }, modifiers: event::Modifiers{
                    shift: modifiers.shift,
                    control: modifiers.control,
                    alt: modifiers.alt,
                    supermeta: modifiers.supermeta,
                }}),
                DeviceInput::Mouse(position, state) => self.instance.emit(event::MouseEvent{
                    position: Some(position),
                    state: match state {
                        MouseState::Pressed(button) => event::MouseState::Pressed(match button {
                            MouseButton::Left => event::MouseButton::Left,
                            MouseButton::Middle => event::MouseButton::Middle,
                            MouseButton::Right => event::MouseButton::Right,
                        }),
                        MouseState::Released(button) => event::MouseState::Released(match button {
                            MouseButton::Left => event::MouseButton::Left,
                            MouseButton::Middle => event::MouseButton::Middle,
                            MouseButton::Right => event::MouseButton::Right,
                        }),
                        MouseState::Moved => event::MouseState::Moved,
                        MouseState::Scroll(dx, dy) => event::MouseState::Scroll(dx, dy),
                    }
                }),
                e => log::debug!("Ignored DeviceInputs: {e:?}")
            },
            _ => {}
        }
    }
}

#[doc(hidden)]
pub mod __private {
    pub use crate::{Builder, Ramp, prism::drawable::Drawable, maverick_os};
}

#[macro_export]
macro_rules! run {
    ([$($service:ident),*], $($app:tt)*) => {
        pub use $crate::__private::*;
        struct PrismBuilder;
        impl $crate::__private::Builder for PrismBuilder {
            fn services() -> $crate::air::Services {$crate::air::Services::default()$( .add::<$service>() )*}

            fn build(ctx: &mut prism::Context) -> Box<dyn $crate::__private::Drawable> {
                Box::new(({$($app)*})(ctx))
            }
        }
        $crate::__private::maverick_os::start!($crate::__private::Ramp<PrismBuilder>);
    };
}
