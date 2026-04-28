use ramp::prism::{self, Context, canvas::{Image, Shape, Text, ShapeType, Color, Font, Span, Align}, Assets};
use ramp::prism::drawable::{Component, SizedTree};
use ramp::prism::layout::{Stack, Row, Column};
use ramp::prism::event::{OnEvent, Event, CameraFrame, Modifiers, KeyboardState, KeyboardEvent, Key };

use ramp::maverick_os::air::{Contracts, Contract, Substance, Id, Reactants, Reactant, Beaker, Name};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::convert::Infallible;
use crate::maverick_os::window::KeyEvent;
use crate::maverick_os::window::Input;
 use crate::prism::event::PickedPhoto;

use serde::{Serialize, Deserialize};
//use ramp::air::{Reactant, Contract, RError, Beaker, Get, Create, from};

#[derive(Debug, Component, Clone)]
pub struct MyComponent(Row, Option<Image>, Text, Shape);
impl OnEvent for MyComponent {
    fn on_event(&mut self, ctx: &mut Context, sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {

        if let Some(KeyboardEvent{key: Key, state: KeyboardState::Pressed, modifiers}) = event.downcast_ref::<KeyboardEvent>() {
            ctx.pick_photo();
        }

        if let Some(PickedPhoto(image)) = event.downcast_ref::<PickedPhoto>() {
            self.1 = Some(Image{shape: ShapeType::Rectangle(0.0, (48.0, 48.0), 0.0), image: image.clone().into(), color: None});
        }

        
        // if let Some(CameraFrame(image)) = event.downcast_ref::<CameraFrame>() {
        //     self.1 = Some(Image{shape: ShapeType::Rectangle(0.0, (48.0, 48.0), 0.0), image: image.clone().into(), color: None});
        // }

        vec![event]
    }
    
}
impl MyComponent {
    pub fn new(ctx: &Context, shape2: Text, shape3: Shape) -> Self {
        ctx.start_camera();
        MyComponent(
            Row::center(50.0),
            None,
            shape2,
            shape3
        )
    }
}

#[derive(Debug, Component, Clone)]
pub struct MyComponent3(Row, Option<Image>, Text, Shape);
impl OnEvent for MyComponent3 {
    fn on_event(&mut self, ctx: &mut Context, sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {

        // if let Some(KeyboardEvent{key: Key, state: KeyboardState::Pressed, modifiers}) = event.downcast_ref::<KeyboardEvent>() {
        //     ctx.pick_photo();
        // }

        if let Some(CameraFrame(image)) = event.downcast_ref::<CameraFrame>() {
            self.1 = Some(Image{shape: ShapeType::Rectangle(0.0, (48.0, 48.0), 0.0), image: image.clone().into(), color: None});
        }
        vec![event]
    }
    
}
impl MyComponent3 {
    pub fn new(ctx: &Context, shape2: Text, shape3: Shape) -> Self {
        //ctx.start_camera();
        MyComponent3(
            Row::center(50.0),
            None,
            shape2,
            shape3
        )
    }
}


#[derive(Debug, Component, Clone)]
pub struct MyComponent2(Stack, MyComponent3, MyComponent);
impl OnEvent for MyComponent2 {}
impl MyComponent2 {
    pub fn new(one: MyComponent3, two: MyComponent) -> Self {
        MyComponent2(
            Stack::center(), one, two
        )
    }
}


#[derive(Serialize, Deserialize, Hash)]
pub struct ChatRoom;
impl ChatRoom {
    pub fn new(_name: &str) -> Self {ChatRoom}
}
impl Contract for ChatRoom {
    fn id() -> Id {Id::hash("ChatRoom2.5")}

    fn init(self, signer: &Name, _timestamp: u64) -> Substance {Substance::Map(BTreeMap::from([
        ("name".to_string(), Substance::String("myroom".to_string())),
        ("author".to_string(), Substance::String(signer.to_string())),
        ("messages".to_string(), Substance::map())
    ]))}

    fn routes() -> BTreeMap<PathBuf, Reactants> {
        BTreeMap::from([
            (PathBuf::from("/name"), Reactants::new().add::<ChangeName>()),
            (PathBuf::from("/messages"), Reactants::new().add::<SendMessage>())
        ])
    }
}

#[derive(Serialize, Deserialize, Hash)]
pub struct ChangeName(String);
impl Reactant for ChangeName {
    type Error = Infallible;
    type Contract = ChatRoom;

    fn apply<B: Beaker>(self, _path: &Path, signer: &Name, _timestamp: u64, substance: &mut B) -> Result<(), Self::Error> {
        if substance.query("/author") == Ok(Substance::String(signer.to_string())) {
            let _ = substance.insert("/name", Substance::String(self.0));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Hash)]
pub struct SendMessage(String);
impl Reactant for SendMessage {
    type Error = Infallible;
    type Contract = ChatRoom;

    fn apply<B: Beaker>(self, _path: &Path, signer: &Name, timestamp: u64, substance: &mut B) -> Result<(), Self::Error> {
        let _ = substance.insert("/messages/-", Substance::Map(BTreeMap::from([
            ("author".to_string(), Substance::String(signer.to_string())),
            ("timestamp".to_string(), Substance::Integer(timestamp as i64)),
            ("body".to_string(), Substance::String(self.0)),
        ])));
        Ok(())
    }
}


ramp::run!{[ChatRoom]; |ctx: &mut Context, assets: Assets| {
    let font = Font::from_bytes(&assets.load_file("font.ttf").unwrap()).unwrap();
    let text = Text::new(vec![Span::new("View destinations".to_string(), 16.0, Some(16.0*1.25), font.into(), Color(255, 255, 255, 255), 0.0)], None, Align::Center, None);
    MyComponent2::new(MyComponent3::new(
        ctx,
        text.clone(),
        Shape{shape: ShapeType::Ellipse(0.0, (150.0, 84.0), 0.0), color: Color(255, 0, 255, 255)},
        // Shape{shape: ShapeType::Ellipse(0.0, (84.0, 150.0), 0.0), color: Color(255, 255, 255, 255)},
    ), MyComponent::new(
        ctx,
        text.clone(),
        Shape{shape: ShapeType::Ellipse(0.0, (150.0, 84.0), 0.0), color: Color(255, 0, 255, 255)},
        // Shape{shape: ShapeType::Ellipse(0.0, (84.0, 150.0), 0.0), color: Color(255, 255, 255, 255)},
    ), )
}}
