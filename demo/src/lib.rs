use ramp::air;

use ramp::prism;
use prism::Context;
use prism::canvas::{Font, Color, Text, Align};
use prism::drawable::Component;
use prism::layout::Column;
use prism::event::OnEvent;

mod hardware;
mod wallet;

#[derive(Component, Debug, Clone)]
pub struct App(
    Column,
    Text,
    hardware::App,
);
impl OnEvent for App {}

ramp::run!([wallet::WalletService], |_ctx: &mut Context| {
    let font = Font::from_bytes(include_bytes!("../resources/font.ttf")).unwrap();
    App(Column::center(24.0), Text::new("Hello", font, 32.0, Color::WHITE, Align::Center), hardware::App::new())
});
