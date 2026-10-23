pub use air_notifications as notifications;
pub use maverick_os;
pub use prism;
use ::air::Secret;
use maverick_os::inputs as native;
use maverick_os::{Data, Hardware, LogicalWindow};
use prism::events as event;

pub mod air;
pub mod hardware;
mod shared;
mod renderer;
pub use renderer::Renderer;

pub struct Ramp<A: prism::Application>(prism::Prism, std::marker::PhantomData<A>);
impl<A: prism::Application> maverick_os::Application for Ramp<A> {
    type Renderer = Renderer;

    fn name() -> String {A::name()}

    fn new(hardware: Hardware, window: LogicalWindow) -> Self {
        let key = "ramp.air.identity";
        let secret = match hardware.get_cache(key).expect("Could not read the Air identity") {
            Some(Data::Text(saved)) => serde_json::from_str(&saved).expect("Invalid saved Air identity"),
            Some(_) => panic!("The saved Air identity must be text"),
            None => {
                let secret = Secret::new();
                let saved = serde_json::to_string(&secret).expect("Could not encode the Air identity");
                hardware.set_cache(key, &Data::Text(saved)).expect("Could not save the Air identity");
                secret
            }
        };
        let window = prism::Window {
            size: window.size, insets: window.insets, keyboard_visible: window.keyboard_visible,
            touch_controls: window.touch_controls,
        };
        let air = prism::air::Air::new(air::Air::new(secret, &hardware));
        let prism = prism::Prism::new::<A, _>(hardware::Hardware(hardware), air, window);
        Self(prism, std::marker::PhantomData)
    }

    fn on_input(&mut self, input: native::Input) {
        match input {
            native::Input::Notification(header, body) => {
                self.0.on_event(event::Notification {header, body});
            }
            native::Input::Media(media) => self.0.on_event(match media {
                native::Media::Play => event::MediaEvent::Play,
                native::Media::Pause => event::MediaEvent::Pause,
                native::Media::VolumeUp => event::MediaEvent::VolumeUp,
                native::Media::VolumeDown => event::MediaEvent::VolumeDown,
            }),
            native::Input::Resized(window) => {
                self.0.resize(prism::Window {
                    size: window.size, insets: window.insets, keyboard_visible: window.keyboard_visible,
                    touch_controls: window.touch_controls,
                });
            }
            native::Input::WindowFocused(focused) => {
                self.0.on_event(event::WindowEvent::Focused(focused));
                if !focused {
                    self.0.on_event(event::MouseEvent {position: None, state: event::MouseState::Moved});
                }
            }
            native::Input::WindowMoved(x, y) => self.0.on_event(event::WindowEvent::Moved(x, y)),
            native::Input::DroppedFile(path) => self.0.on_event(event::FileEvent::Dropped(path)),
            native::Input::HoveredFile(path) => self.0.on_event(event::FileEvent::Hovered(path)),
            native::Input::HoveredFileCancelled => self.0.on_event(event::FileEvent::Cancelled),
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
                    self.0.on_event(event::KeyboardEvent {key, state, modifiers});
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
                    self.0.on_event(event::MouseEvent {position: Some(position), state});
                }
                native::DeviceInput::CursorEntered => {
                    self.0.on_event(event::CursorEvent {device, entered: true});
                }
                native::DeviceInput::CursorLeft => {
                    self.0.on_event(event::CursorEvent {device, entered: false});
                    self.0.on_event(event::MouseEvent {position: None, state: event::MouseState::Moved});
                }
            },
        }
    }

    fn draw(&mut self) -> Vec<prism::drawable::Instruction> {self.0.draw()}
    async fn changed(&mut self) {self.0.changed().await;}
}

#[macro_export]
macro_rules! run {
    ($app:ty) => {$crate::maverick_os::start!($crate::Ramp<$app>);};
}
