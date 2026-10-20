use ::air::Secret;
use maverick_os::inputs as native;
use maverick_os::{Data, Hardware, LogicalWindow};
use prism::events as event;

pub use maverick_os;
pub use prism;
pub use renderer::Renderer;

pub mod air;
pub mod hardware;
mod renderer;

pub struct Ramp<A: prism::Application>(prism::Prism<A>);
impl<A: prism::Application> maverick_os::Application for Ramp<A> {
    type Renderer = Renderer;

    fn name() -> String {A::name()}

    fn new(hardware: Hardware, window: LogicalWindow) -> Self {
        let key = "ramp.air.identity";
        let secret = match hardware.get_cache(key) {
            Some(Data::Text(saved)) => serde_json::from_str(&saved).expect("Invalid saved Air identity"),
            Some(_) => panic!("The saved Air identity must be text"),
            None => {
                let secret = Secret::new();
                let saved = serde_json::to_string(&secret).expect("Could not encode the Air identity");
                hardware.set_cache(key, &Data::Text(saved));
                secret
            }
        };
        let window = prism::Window {size: window.size, insets: window.insets};
        Self(prism::Prism::new(hardware::Hardware(hardware), air::Air::new(secret), window))
    }

    fn on_input(&mut self, input: native::Input) {
        match input {
            native::Input::Media(media) => self.0.on_event(match media {
                native::Media::Play => event::MediaEvent::Play,
                native::Media::Pause => event::MediaEvent::Pause,
                native::Media::VolumeUp => event::MediaEvent::VolumeUp,
                native::Media::VolumeDown => event::MediaEvent::VolumeDown,
            }),
            native::Input::Resized(window) => {
                self.0.resize(prism::Window {size: window.size, insets: window.insets});
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

    fn on_tick(&mut self) -> Vec<prism::drawable::Instruction> {self.0.draw()}
}

#[macro_export]
macro_rules! run {
    ($app:ty) => {$crate::maverick_os::start!($crate::Ramp<$app>);};
}
