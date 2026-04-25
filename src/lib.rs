pub use prism;

#[doc(hidden)]
pub mod __private {
    pub use maverick_os;
    pub use prism::drawable::Drawable;
    pub use maverick_os::Assets;

    use wgpu_canvas::{Canvas, Atlas, Area, Shape, ShapeType, Item, Image};
    use prism::{Instance, Hardware, Request};
    use prism::event::{KeyboardState, MouseState, MouseButton, MouseEvent, KeyboardEvent, Modifiers, TickEvent};
    use prism::drawable::SizedTree;
    use maverick_os::{Application, Context, Services};
    use maverick_os::window::{
        Event, Lifetime, Input, TouchPhase, Touch, MouseScrollDelta, ElementState, Key, NamedKey, Modifiers as WinitModifiers,
        MouseButton as WinitMouseButton,
    };

    use std::marker::PhantomData;
    use std::time::Instant;

    pub struct Ramp<B>{
        app: Box<dyn Drawable>,
        atlas: Atlas,
        canvas: Canvas,
        context: prism::Context,
        instance: Instance,
        touching: bool,
        mouse: (f32, f32),
        scroll: Option<(f32, f32)>,
        screen: (f32, f32),
        sized_app: SizedTree,
        scale_factor: f64,
        timer: Instant,
        modifiers: WinitModifiers,
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
    }
    impl<B: Builder> Services for Ramp<B> {}
    impl<B: Builder> Application for Ramp<B> {
        async fn new(ctx: &mut Context, assets: Assets) -> Self {
            let (mut context, receiver) = prism::Context::new();
            let scale_factor = ctx.window.scale_factor;
            let screen = (
                (ctx.window.size.0 as f64 / scale_factor) as f32,
                (ctx.window.size.1 as f64 / scale_factor) as f32,
            );

            let app = B::build(&mut context, assets);
            let size_request = app.request_size();
            let sized_app = app.build(screen, size_request);
            Ramp{
                app,
                atlas: Atlas::default(),
                canvas: Canvas::new(ctx.window.handle.clone(), ctx.window.size.0, ctx.window.size.1).await,
                context,
                instance: Instance::new(receiver),
                touching: false,
                mouse: (0.0, 0.0),
                screen,
                sized_app,
                scroll: None,
                scale_factor,
                timer: Instant::now(),
                modifiers: WinitModifiers::default(),
                _p: PhantomData::<fn() -> B>
            }
        }
        async fn on_event(&mut self, ctx: &mut Context, event: Event) {
            let window = matches!(event, Event::Lifetime(Lifetime::Resumed)).then(|| ctx.window.handle.clone());
            if let Some(event) = match event {
                Event::Lifetime(lifetime) => match lifetime {
                    Lifetime::Resized | Lifetime::Resumed => {
                        self.scale_factor = ctx.window.scale_factor;
                        self.canvas.resize(window, ctx.window.size.0, ctx.window.size.1);
                        self.screen = (self.logical(ctx.window.size.0 as f32), self.logical(ctx.window.size.1 as f32));
                        let size_request = self.app.request_size();
                        self.sized_app = self.app.build(self.screen, size_request);
                        None
                    },
                    Lifetime::Paused => None,
                    Lifetime::Close => None,
                    Lifetime::Draw => {
                        self.instance.tick(&mut self.context);
                        self.app.event(&mut self.context, &self.sized_app, Box::new(TickEvent));
                        self.timer = Instant::now();
                        if let Some(hardware) = self.instance.handle_requests() {
                            match hardware {
                                Hardware::GetCamera => {
                                    if let Some(camera) = ctx.hardware.camera() && let Ok(frame) = camera.frame() {
                                        self.context.send(Request::event(prism::event::HardwareEvent::Camera(frame.into())));
                                    }
                                }
                                Hardware::StopCamera => if let Some(camera) = ctx.hardware.camera_existing() { camera.stop(); },
                                Hardware::SetClipboard(data) => ctx.hardware.clipboard().set(data),
                                Hardware::GetClipboard => {
                                    if let Some(data) = ctx.hardware.clipboard().get() {
                                        self.context.send(Request::event(prism::event::HardwareEvent::Clipboard(data)));
                                    }
                                },
                                Hardware::GetSafeArea => {
                                    let area = ctx.hardware.safe_area_insets();
                                    self.context.send(Request::event(prism::event::HardwareEvent::SafeArea(area.0, area.1, area.2, area.3)));
                                },
                                Hardware::Share(data) => ctx.hardware.share(&data),
                                Hardware::Haptic => ctx.hardware.haptic(),
                                _ => {}
                            }
                        }

                        while let Some(event) = self.instance.events.pop_front() {
                            if let Some(event) = event
                                .pass(&mut self.context, &[prism::layout::Area{offset: (0.0, 0.0), size: self.sized_app.0}])
                                .remove(0)
                            {
                                self.app.event(&mut self.context, &self.sized_app, event);
                            }
                        }

                        let size_request = self.app.request_size();
                        self.sized_app = self.app.build(self.screen, size_request);
                        let drawn = self.app.draw(&self.sized_app, (0.0, 0.0), (0.0, 0.0, self.screen.0, self.screen.1));
                        let scaled: Vec<_> = drawn.into_iter().map(|(a, i)| {
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
                        }).collect();
                        self.canvas.draw(&mut self.atlas, scaled);
                        None
                    },
                    Lifetime::MemoryWarning => None,
                },
                Event::Input(input) => match input {
                    Input::ModifiersChanged(new_modifiers) => {
                        self.modifiers = new_modifiers;
                        None
                    },
                    Input::Touch(Touch { location, phase, .. }) => {
                        let location = (location.x as f32, location.y as f32);
                        let position = (self.logical(location.0), self.logical(location.1));
                        // Touch has no button concept — treat as Left
                        let event = match phase {
                            TouchPhase::Started => {
                                self.scroll = Some(position);
                                self.touching = true;
                                Some(MouseState::Pressed(MouseButton::Left))
                            },
                            TouchPhase::Ended | TouchPhase::Cancelled => {
                                self.touching = false;
                                Some(MouseState::Released(MouseButton::Left))
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
                    Input::Mouse{state, button, ..} => {
                        // Convert winit MouseButton to prism MouseButton
                        let btn = match button {
                            WinitMouseButton::Left   => MouseButton::Left,
                            WinitMouseButton::Right  => MouseButton::Right,
                            WinitMouseButton::Middle => MouseButton::Middle,
                            _ => MouseButton::Left,
                        };
                        Some(Box::new(MouseEvent{position: Some(self.mouse), state: match state {
                            ElementState::Pressed  => MouseState::Pressed(btn),
                            ElementState::Released => MouseState::Released(btn),
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
                        let mods = self.modifiers.state();
                        let modifiers = Modifiers {
                            shift: mods.shift_key(),
                            ctrl:  mods.control_key(),
                            alt:   mods.alt_key(),
                            meta:  mods.super_key(),
                        };
                        Some(event).and_then(|event| Some(Box::new(KeyboardEvent{
                            key: match event.logical_key {
                                Key::Named(named) => Some(prism::event::Key::Named(match named {
                                    NamedKey::Enter => Some(prism::event::NamedKey::Enter),
                                    NamedKey::Tab => Some(prism::event::NamedKey::Tab),
                                    NamedKey::Space => Some(prism::event::NamedKey::Space),
                                    NamedKey::Backspace | NamedKey::Delete => Some(prism::event::NamedKey::Delete),
                                    NamedKey::Escape => Some(prism::event::NamedKey::Escape),
                                    NamedKey::ArrowDown => Some(prism::event::NamedKey::ArrowDown),
                                    NamedKey::ArrowLeft => Some(prism::event::NamedKey::ArrowLeft),
                                    NamedKey::ArrowRight => Some(prism::event::NamedKey::ArrowRight),
                                    NamedKey::ArrowUp => Some(prism::event::NamedKey::ArrowUp),
                                    NamedKey::Insert => Some(prism::event::NamedKey::Insert),
                                    NamedKey::Home => Some(prism::event::NamedKey::Home),
                                    NamedKey::End => Some(prism::event::NamedKey::End),
                                    NamedKey::PageUp => Some(prism::event::NamedKey::PageUp),
                                    NamedKey::PageDown => Some(prism::event::NamedKey::PageDown),
                                    NamedKey::Shift => Some(prism::event::NamedKey::Shift),
                                    NamedKey::Control => Some(prism::event::NamedKey::Control),
                                    NamedKey::Alt => Some(prism::event::NamedKey::Alt),
                                    NamedKey::Meta => Some(prism::event::NamedKey::Meta),
                                    NamedKey::CapsLock => Some(prism::event::NamedKey::CapsLock),
                                    NamedKey::NumLock => Some(prism::event::NamedKey::NumLock),
                                    NamedKey::ScrollLock => Some(prism::event::NamedKey::ScrollLock),
                                    NamedKey::F1 => Some(prism::event::NamedKey::F1),
                                    NamedKey::F2 => Some(prism::event::NamedKey::F2),
                                    NamedKey::F3 => Some(prism::event::NamedKey::F3),
                                    NamedKey::F4 => Some(prism::event::NamedKey::F4),
                                    NamedKey::F5 => Some(prism::event::NamedKey::F5),
                                    NamedKey::F6 => Some(prism::event::NamedKey::F6),
                                    NamedKey::F7 => Some(prism::event::NamedKey::F7),
                                    NamedKey::F8 => Some(prism::event::NamedKey::F8),
                                    NamedKey::F9 => Some(prism::event::NamedKey::F9),
                                    NamedKey::F10 => Some(prism::event::NamedKey::F10),
                                    NamedKey::F11 => Some(prism::event::NamedKey::F11),
                                    NamedKey::F12 => Some(prism::event::NamedKey::F12),
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
                            modifiers,
                        }) as Box<dyn prism::event::Event>))
                    },
                    _ => None
                }
            } {self.instance.events.push_back(event);}
        }
    }

    pub trait Builder {fn build(ctx: &mut prism::Context, assets: Assets) -> Box<dyn Drawable>;}
}

#[macro_export]
macro_rules! run {
    ($($app:tt)*) => {
        pub use $crate::__private::*;
        struct PrismBuilder;
        impl Builder for PrismBuilder {
            fn build(ctx: &mut prism::Context, assets: Assets) -> Box<dyn Drawable> {
                Box::new(({$($app)*})(ctx, assets))
            }
        }

        maverick_os::start!(Ramp<PrismBuilder>);
    };
}