use ramp::prism::{self, Context, canvas::{Shape, ShapeType, Color}, Assets};

use ramp::prism::drawable::{Component, SizedTree};
use ramp::prism::layout::Stack;
use ramp::prism::event::{Event, OnEvent};
//use ramp::air::{Reactant, Contract, RError, Beaker, Get, Create, from};

#[derive(Debug, Component, Clone)]
pub struct MyComponent(Stack, Shape, Shape);
impl MyComponent {
    pub fn new(ctx: &mut Context, mut shape: Shape, mut shape2: Shape) -> Self {
      //ctx.create(Id::MIN, UpdatingChar("b"));
      //if let Some(Ok(ch)) = ctx.get::<UpdatingChar>(Id::MIN, "/bob_char").map(from) {
      //    shape.color.0 = ch; 
      //    shape2.color.0 = ch; 
      //}
        MyComponent(
            Stack::center(),
            shape,
            shape2,
        )
    }
}

impl OnEvent for MyComponent {
    fn on_event(&mut self, ctx: &mut Context, _sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
      //if let Some(update) = event.downcast_ref::<Update<UpdatingChar>>() {
      //    if let Some(Ok(ch)) = ctx.get::<UpdatingChar>(Id::MIN, "/bob_char").map(from) {
      //        self.1.color.0 = ch; 
      //        self.2.color.0 = ch; 
      //    }
      //}
      //if let Some(KeyboardEvent{key: Key::Character(ch), ..}) = event.downcast_ref::<KeyboardEvent>() {
      //    if ctx.send(Id::MIN, "/", UpdateChar(ch)).is_err() {
      //        println!("{:?}", e);
      //        ctx.send(Haptic);
      //    }
      //}
        vec![event]
    }
}

//  #[derive(Debug)]
//  pub struct Greater(char);
//  impl std::error::Error for Greater {}
//  impl std::fmt::Display for Greater {fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {write!(f, "{self:?}")}}

//  pub struct UpdateChar(char);
//  impl Reactant for UpdateChar {
//      type Error = Greater;    
//      fn apply<B: Beaker>(self, path: &Path, signer: Name, timestamp: u64, beaker: &mut B) -> Result<(), Self::Error>;
//          let old = beaker.get("/bob_char").unwrap();
//          if old > self.0 {Err(Greater(old))?}
//          Ok(beaker.insert("/bob_char", into(self.0).unwrap()))
//      }
//  }

//  #[derive(Serialize, Deserialize)]
//  pub struct UpdatingChar(char);
//  impl Contract for UpdatingChar {
//      fn id() -> Id {Id::hash("UpdatingChar")}

//      fn init(self) -> Value {Value::from([("bob_char", Value::from(self.0))])}

//      fn routes() -> BTreeMap<PathBuf, Reactants> {
//          BTreeMap::from([
//              (PathBuf::from("/"), Reactants::new().add::<UpdateChar>())
//          ]) 
//      }
//  }

ramp::run!{|ctx: &mut Context, _assets: Assets| {
    MyComponent::new(
        ctx,
        Shape{shape: ShapeType::Ellipse(0.0, (150.0, 84.0), 0.0), color: Color(255, 255, 255, 255)},
        Shape{shape: ShapeType::Ellipse(0.0, (84.0, 150.0), 0.0), color: Color(255, 255, 255, 255)},
    )
}}
