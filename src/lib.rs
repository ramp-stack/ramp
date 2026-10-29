pub use maverick_os;
pub use prism;
use ::air::Secret;
use futures::{FutureExt, StreamExt};
use futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use maverick_os::inputs as native;
use maverick_os::{Data, Hardware, LogicalWindow};
use prism::events as event;

pub mod air;
pub mod notifications;
pub mod hardware;
mod native_text;
use native_text::{Action, Selection};
mod shared;
mod player;
pub use player::Player;
mod renderer;
pub use renderer::{Frame, Renderer};

pub struct Ramp<A: prism::Application> {
    prism: prism::Prism,
    sender: UnboundedSender<Action>,
    receiver: UnboundedReceiver<Action>,
    revision: u64,
    application: std::marker::PhantomData<A>,
}
impl<A: prism::Application> maverick_os::Application for Ramp<A> {
    type Renderer = Renderer;

    fn name() -> String {A::name()}

    fn new(hardware: Hardware, window: LogicalWindow) -> Self {
        let key = "ramp.air.identity";
        let secret = hardware.get_cache(key).and_then(|data| match data {
            Data::Text(saved) => serde_json::from_str::<Secret>(&saved).ok(),
            _ => None,
        }).filter(|secret| secret.path().is_empty()
            && secret.name().to_string() == secret.public(None).public_key().to_string())
            .unwrap_or_else(|| {
                let secret = Secret::new();
                let saved = serde_json::to_string(&secret).unwrap();
                if let Err(error) = hardware.set_cache(key, &Data::Text(saved)) {
                    log::warn!("Could not save AIR identity: {error}");
                }
                secret
            });
        let window = prism::Window {
            size: window.size, insets: window.insets, is_mobile: window.is_mobile,
        };
        let air = prism::air::Air::new(air::Air::new(secret, &hardware));
        let prism = prism::Prism::new::<A, _>(hardware::Hardware(hardware), air, window);
        let (sender, receiver) = mpsc::unbounded();
        Self {prism, sender, receiver, revision: 0, application: std::marker::PhantomData}
    }

    fn on_input(&mut self, input: native::Input) {
        self.drain_native();
        match input {
            native::Input::Notification(header, body) => {
                self.prism.on_event(event::Notification {header, body});
            }
            native::Input::Media(media) => self.prism.on_event(match media {
                native::Media::Play => event::MediaEvent::Play,
                native::Media::Pause => event::MediaEvent::Pause,
                native::Media::Toggle => event::MediaEvent::Toggle,
                native::Media::VolumeUp => event::MediaEvent::VolumeUp,
                native::Media::VolumeDown => event::MediaEvent::VolumeDown,
            }),
            native::Input::Resized(window) => {
                self.prism.on_event(event::WindowEvent::Resized(prism::Window {
                    size: window.size, insets: window.insets, is_mobile: window.is_mobile,
                }));
            }
            native::Input::WindowFocused(focused) => {
                self.prism.on_event(event::WindowEvent::Focused(focused));
                if !focused {
                    self.prism.on_event(event::MouseEvent {position: None, state: event::MouseState::Moved});
                }
            }
            native::Input::WindowMoved(x, y) => self.prism.on_event(event::WindowEvent::Moved(x, y)),
            native::Input::DroppedFile(path) => self.prism.on_event(event::FileEvent::Dropped(path)),
            native::Input::HoveredFile(path) => self.prism.on_event(event::FileEvent::Hovered(path)),
            native::Input::HoveredFileCancelled => self.prism.on_event(event::FileEvent::Cancelled),
            native::Input::Device(device, input) => match input {
                native::DeviceInput::Keyboard(key, state, modifiers) => {
                    let key = match key {
                        native::Key::Escape => event::Key::Escape,
                        native::Key::Enter => event::Key::Enter,
                        native::Key::Tab => event::Key::Tab,
                        native::Key::Space => event::Key::Space,
                        native::Key::Up => event::Key::Up,
                        native::Key::Down => event::Key::Down,
                        native::Key::Left => event::Key::Left,
                        native::Key::Right => event::Key::Right,
                        native::Key::Delete => event::Key::Delete,
                        native::Key::Backspace => event::Key::Backspace,
                        native::Key::Home => event::Key::Home,
                        native::Key::End => event::Key::End,
                        native::Key::Shift => event::Key::Shift,
                        native::Key::Control => event::Key::Control,
                        native::Key::Alt => event::Key::Alt,
                        native::Key::SuperMeta => event::Key::SuperMeta,
                        native::Key::CapsLock => event::Key::CapsLock,
                        native::Key::NumLock => event::Key::NumLock,
                        native::Key::ScrollLock => event::Key::ScrollLock,
                        native::Key::Character(character) => event::Key::Character(character.to_string()),
                        key => {log::debug!("Unsupported keyboard key: {key:?}"); return}
                    };
                    let state = match state {
                        native::KeyboardState::Pressed => event::KeyboardState::Pressed,
                        native::KeyboardState::Repeated => event::KeyboardState::Repeated,
                        native::KeyboardState::Released => event::KeyboardState::Released,
                    };
                    let modifiers = event::Modifiers {
                        shift: modifiers.shift, control: modifiers.control,
                        alt: modifiers.alt, supermeta: modifiers.supermeta,
                    };
                    self.action(Action::Keyboard(event::KeyboardEvent {key, state, modifiers}));
                }
                native::DeviceInput::Mouse(position, state) => {
                    let button = |button| match button {
                        native::MouseButton::Left => event::MouseButton::Left,
                        native::MouseButton::Middle => event::MouseButton::Middle,
                        native::MouseButton::Right => event::MouseButton::Right,
                    };
                    let state = match state {
                        native::MouseState::Pressed(b) => event::MouseState::Pressed(button(b)),
                        native::MouseState::Released(b) => event::MouseState::Released(button(b)),
                        native::MouseState::Moved => event::MouseState::Moved,
                        native::MouseState::Scroll(x, y) => event::MouseState::Scroll(x, y),
                    };
                    self.action(Action::Mouse(event::MouseEvent {position: Some(position), state}));
                }
                native::DeviceInput::CursorEntered => {
                    self.prism.on_event(event::CursorEvent {device, entered: true});
                }
                native::DeviceInput::CursorLeft => {
                    self.prism.on_event(event::CursorEvent {device, entered: false});
                    self.prism.on_event(event::MouseEvent {position: None, state: event::MouseState::Moved});
                }
            },
        }
    }

    fn draw(&mut self) -> Frame {
        self.drain_native();
        Frame::new(self.prism.draw(), self.sender.clone(), self.revision)
    }
    async fn changed(&mut self) {
        futures::select_biased! {
            action = self.receiver.next().fuse() => self.action(action.unwrap()),
            _ = self.prism.changed().fuse() => {},
        }
        self.drain_native();
    }
}

impl<A: prism::Application> Ramp<A> {
    fn drain_native(&mut self) {
        while let Ok(action) = self.receiver.try_recv() {self.action(action);}
    }

    fn action(&mut self, action: Action) {
        match action {
            Action::SetText {edit, revision} => {
                assert!(edit.text.is_char_boundary(edit.range.start)
                    && edit.text.is_char_boundary(edit.range.end));
                self.prism.on_event(edit);
                self.revision = self.revision.max(revision);
            }
            Action::Selection {id, text, range, revision} => {
                assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
                self.prism.on_event(Selection {id, text, range});
                self.revision = self.revision.max(revision);
            }
            Action::Keyboard(event) => self.prism.on_event(event),
            Action::Mouse(event) => self.prism.on_event(event),
        }
    }
}

#[macro_export]
macro_rules! run {
    ($app:ty) => {$crate::maverick_os::start!($crate::Ramp<$app>);};
}
