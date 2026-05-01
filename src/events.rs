use prism::{Instance, event};
use maverick_os::Context;

pub struct EventHandler(Instance);
impl EventHandler {
    fn on_input(&mut self, ctx: &mut Context, input: Input) {
        if let Some(event) = match input {
            Input::Tick => {
                Context::tick(ctx, &mut self.app, &self.sized_app, events)
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
            Input::CameraFrame(image) => Some(event::CameraFrame(image)),
            Input::PickedPhoto(image) => Some(event::PickedPhoto(image),
            Input::ModifiersChanged(mods) => {
                self.modifiers = event::Modifiers {
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
                            NamedKey::Delete => Some(prism::event::NamedKey::Delete),
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
