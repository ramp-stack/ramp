use ramp::prism::{self, Context, canvas::{Shape, ShapeType, Color}, Assets};

use ramp::prism::drawable::Component;
use ramp::prism::layout::Stack;
use ramp::prism::event::OnEvent;
//use ramp::air::{Reactant, Contract, RError, Beaker, Get, Create, from};

#[derive(Debug, Component, Clone)]
pub struct MyComponent(Stack, Shape, Shape);
impl OnEvent for MyComponent {}
impl MyComponent {
    pub fn new(_ctx: &mut Context, shape: Shape, shape2: Shape) -> Self {
        MyComponent(
            Stack::center(),
            shape,
            shape2,
        )
    }
}

ramp::run!{|ctx: &mut Context, _assets: Assets| {
    MyComponent::new(
        ctx,
        Shape{shape: ShapeType::Ellipse(0.0, (150.0, 84.0), 0.0), color: Color(255, 255, 255, 255)},
        Shape{shape: ShapeType::Ellipse(0.0, (84.0, 150.0), 0.0), color: Color(255, 255, 255, 255)},
    )
}}
