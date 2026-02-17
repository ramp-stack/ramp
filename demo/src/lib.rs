use ramp::{prism};
use prism::canvas::{Shape, ShapeType, Color, Text, Align, Span, Font, Image, RgbaImage};
use prism::drawable::{Component, SizedTree};
use prism::layout::Row;
use prism::event::{OnEvent, MouseEvent, Event};
use prism::Context;

use std::sync::Arc;

#[derive(Debug, Component)]
pub struct TwoShape(Row, Shape, Image, Shape, Shape, #[skip] f32, Text);
impl TwoShape {
    fn rotate_shape(shape: &mut ShapeType, add: f32) {
        let r = match shape {
            ShapeType::Rectangle(_, _, r) | ShapeType::RoundedRectangle(_, _, r, _) | ShapeType::Ellipse(_, _, r) => r
        };
        *r = (*r+add) % 180.0;
    }
}

impl OnEvent for TwoShape {
    fn on_event(&mut self, _ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        // if let Some(MouseEvent{position: Some((x, _)), ..}) = event.downcast_ref() {
        //     let n = if *x > self.7 {-1.0} else {1.0};
        //     self.7 = *x;
        //     Self::rotate_shape(&mut self.1.shape, n);
        //     Self::rotate_shape(&mut self.2.shape, n*2.0);
        //     Self::rotate_shape(&mut self.3.shape, n*5.0);
        //     Self::rotate_shape(&mut self.4.shape, n*10.0);
        //     Self::rotate_shape(&mut self.5.shape, n*20.0);
        //     Self::rotate_shape(&mut self.6.shape, n*25.0);
        // }
        vec![event]
    }
}

ramp::run!{|_ctx: &mut Context, assets: Assets| {
    let font = Font::from_bytes(&assets.get_font("font.ttf").unwrap()).unwrap();
    let image = assets.get_image("dog.png").unwrap();
    // let image: Arc<RgbaImage> = Arc::new(image::open("./dog.png").unwrap().into());
    TwoShape(
        Row::center(0.0),
        Shape{shape: ShapeType::Rectangle(1.0, (150.0, 123.0), 0.0), color: Color(0, 255, 255, 255)},
        Image{shape: ShapeType::Rectangle(0.0, (150.0, 93.0), 0.0), image: image.clone(), color: None},
        Shape{shape: ShapeType::Ellipse(0.0, (150.0, 84.0), 0.0), color: Color(255, 255, 255, 255)},
        // Image{shape: ShapeType::Ellipse(0.0, (150.0, 28.0), 0.0), image: image.clone(), color: None},
        Shape{shape: ShapeType::RoundedRectangle(1.0, (150.0, 84.0), 0.0, 20.0), color: Color(255, 0, 255, 255)},
        // Image{shape: ShapeType::RoundedRectangle(0.0, (150.0, 28.0), 0.0, 20.0), image, color: None},
        0.0,
        Text::new(vec![
            Span::new("Hello World I am now having fun".to_string(), 16.0, None, Arc::new(font), Color(0, 255, 0, 255), 0.0)
        ], None, Align::Center, None),
    )

}}