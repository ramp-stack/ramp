use ramp::prism::{self, Context, canvas::{Shape, ShapeType, Color}};

ramp::run!{|_ctx: &mut Context, _assets: Assets| {
    Shape{shape: ShapeType::Ellipse(0.0, (150.0, 84.0), 0.0), color: Color(255, 255, 255, 255)}
}}