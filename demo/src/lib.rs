use ramp::air;

use ramp::prism;
use prism::Context;
use prism::canvas::{Font, Color, Text, Align};
use prism::drawable::{Component, SizedTree};
use prism::layout::Column;
use prism::event::{OnEvent, Event, KeyboardEvent, Key, KeyboardState};
use prism::display::Enum;

mod hardware;
mod chatbot;
//mod wallet;

#[derive(Component, Debug, Clone)]
pub struct App(
    Column,
    Text,
    Enum<Box<dyn Drawable>>,
    #[skip] Vec<String>,
    #[skip] usize
);
impl OnEvent for App {
    fn on_event(&mut self, ctx: &mut Context, sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(KeyboardEvent{key: Key::Left | Key::Right, state: KeyboardState::Pressed, ..}) = event.downcast_ref() {
            self.4 = (self.4 + 1) % self.3.len();
            self.2.display(&self.3[self.4]);
        }
        vec![event]
    }

}

ramp::run!([chatbot::ChatBot], |ctx: &mut Context| {
    let font = Font::from_bytes(include_bytes!("../resources/font.ttf")).unwrap();
    App(Column::center(24.0),
        Text::new("Hello use arrows to switch apps!", font, 32.0, Color::WHITE, Align::Center),
        Enum::new(vec![("hardware".to_string(), Box::new(hardware::App::new())), ("chat".to_string(), Box::new(chatbot::App::new(ctx)))], "hardware".to_string()),
        vec!["hardware".to_string(), "chat".to_string()],
        0
    )
});
