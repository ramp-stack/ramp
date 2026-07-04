use ramp::prism;
use prism::Context;
use prism::canvas::{Color, Align, Text, Font, Shape, ShapeType};
use prism::drawable::{Component, SizedTree};
use prism::display::Bin;
use prism::layout::{Column, Stack, Row, Padding, ScrollAnchor, Size, Offset, Area};
use ramp::air;
use prism::event::{OnEvent, Event, KeyboardEvent, Key, KeyboardState, TickEvent};
use crate::prism::emitters::Scrollable;

use air::{Contract, Reactants, Reactant, Instance, Name, Service, Services, Metadata, Secret, Lock, names::Id, Instances};

use serde::{Serialize, Deserialize};
use tokio::task::JoinSet;

use std::collections::HashMap;
use futures::stream::{FuturesUnordered, StreamExt};

pub struct ChatBot(HashMap<Id, Instance<Room>>, Instances<Room>);
impl Service for ChatBot {
    fn id() -> Id {Id::hash("CHATBOT")}
    async fn new(ctx: &mut air::Context, _secret: Secret) -> Self {
        let instances = ctx.instances();
        ChatBot(instances.list(), instances)
    }
    async fn run(&mut self, ctx: &mut air::Context) {
        loop { 
            let mut set = self.0.values_mut().map(|r| async {
                loop {
                    if let Some(index) = r.listen_confirmed().await.downcast::<SendMessage>() {
                        break (index, r)
                    }
                }
            }).collect::<FuturesUnordered<_>>();
            tokio::select!{
                instance = self.1.listen() => {drop(set); self.0.insert(instance.id(), instance);},
                Some((index, room)) = set.next() => {
                    let message = room.load_confirmed().messages.get(index).unwrap().clone();
                    if message.author == ctx.me() && !message.body.contains("ChatBot") {
                        room.apply(SendMessage(format!("ChatBot Replying to \"{:.10}...\": I totally agree", message.body)));
                    }
                }
            }
        }
    }
    async fn shutdown(self, ctx: &mut air::Context) {
        for room in self.1.list().values_mut() {
            room.apply(SendMessage("ChatBot Shutting Down".to_string())).confirmed().await;
        }
        println!("CHATBOT SHUTDOWN");
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Message {
    author: Name,
    timestamp: u64,
    body: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Room {
    author: Name,
    name: String,
    messages: Vec<Message>
}
impl Contract for Room {
    type Init = String;
    fn id() -> Id {Id::hash("Room")}

    fn init(init: Self::Init, metadata: Metadata) -> Self {
        Room {
            author: metadata.signer,
            name: init, 
            messages: Vec::new()
        }
    }

    fn reactants() -> Reactants<Room> {
        Reactants::default().add::<SendMessage>()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SendMessage(String);
impl Reactant<Room> for SendMessage {
    type Output = usize;

    fn id() -> Id {Id::hash("SendMessage")}

    fn apply(self, room: &mut Room, metadata: Metadata) -> Self::Output {
        room.messages.push(Message{author: metadata.signer, timestamp: metadata.timestamp, body: self.0});
        room.messages.len()-1
    }
}

#[derive(Component, Debug, Clone)]
pub struct RoomItem(Stack, Shape, Text);
impl OnEvent for RoomItem {}

#[derive(Component, Debug, Clone)]
pub struct RoomItems(Column, Text, Vec<RoomItem>);
impl OnEvent for RoomItems {}

#[derive(Component, Debug, Clone)]
pub struct Rooms(
    Stack,
    Shape,
    RoomItems,
    #[skip] Vec<Instance<Room>>,
    #[skip] Font,
    #[skip] usize, // currently selected index
);
impl OnEvent for Rooms {
    fn on_event(&mut self, ctx: &mut Context, sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(KeyboardEvent{key, state: KeyboardState::Pressed, ..}) = event.downcast_ref() {
            match key {
                Key::SuperMeta => {
                    let new_name = format!("Room{}", self.3.len());
                    let new = ctx.instances::<Room>().create(new_name);
                    self.3.push(new.clone());
                    self.5 = self.3.len() - 1;
                    let font = self.4.clone();
                    ctx.emit(RoomCreated(new));
                    *self = Self::rebuild(ctx, font, self.3.clone(), self.5);
                }
                Key::Tab => {
                    if !self.3.is_empty() {
                        self.5 = (self.5 + 1) % self.3.len();
                        let selected = self.3[self.5].clone();
                        let font = self.4.clone();
                        ctx.emit(RoomSelected(selected));
                        *self = Self::rebuild(ctx, font, self.3.clone(), self.5);
                    }
                }
                _ => {}
            }
        }
        vec![event]
    }
}
impl Rooms {
    pub fn new(ctx: &mut Context, font: Font) -> Self {
        let instances: Vec<Instance<Room>> = ctx.instances::<Room>().list().into_values().collect();
        Self::rebuild(ctx, font, instances, 0)
    }

    fn rebuild(_ctx: &mut Context, font: Font, instances: Vec<Instance<Room>>, selected: usize) -> Self {
        let items = instances.iter().enumerate().map(|(i, inst)| {
            let name = inst.load_pending().name.clone();
            let color = if i == selected { Color::YELLOW } else { Color::WHITE };
            RoomItem(
                Stack::center(),
                Shape{shape: ShapeType::RoundedRectangle(1.0, (220.0, 20.0), 0.0, 4.0), color: Color::WHITE},
                Text::new(&name, font.clone(), 16.0, color, Align::Left)
            )
        }).collect();

        Rooms(Stack::start(),
            Shape{shape: ShapeType::RoundedRectangle(1.0, (250.0, 500.0), 0.0, 10.0), color: Color::WHITE},
            RoomItems(
                Column::new(10.0, Offset::Start, Size::Fit, Padding::new(8.0), None),
                Text::new("Press 'COMMAND' to create a new room\nPress 'TAB' to go to next room in list", font.clone(), 12.0, Color::WHITE, Align::Left),
                items
            ),
            instances,
            font,
            selected,
        )
    }
}

#[derive(Component, Debug, Clone)]
pub struct TextInput(Row, Text, Shape);
impl OnEvent for TextInput {
    fn on_event(&mut self, ctx: &mut Context, sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(KeyboardEvent{key, state: KeyboardState::Pressed, ..}) = event.downcast_ref() {
            match key {
                Key::Character(c) => {self.1.spans[0].text += &c.to_string();},
                Key::Delete | Key::Backspace => {self.1.spans[0].text.pop();},
                Key::Enter => {self.1.spans[0].text = String::new();}
                _ => {}
            }
        }
        vec![event]
    }
}
impl TextInput {
    pub fn new(font: Font) -> Self {
        TextInput(Row::end(4.0),
            Text::new("Message", font, 16.0, Color::YELLOW, Align::Left),
            Shape{shape: ShapeType::Rectangle(0.0, (2.0, 20.0), 0.0), color: Color::YELLOW},
        )
    }
}

#[derive(Component, Debug, Clone)]
pub struct MessageItem(Stack, Shape, Text);
impl OnEvent for MessageItem {}

#[derive(Component, Debug, Clone)]
pub struct MessageItems(Column, Vec<MessageItem>, TextInput);
impl OnEvent for MessageItems {}

#[derive(Component, Debug, Clone)]
pub struct Messages(
    Stack,
    MessageItems,
    #[skip] Instance<Room>,
    #[skip] Font
);
impl OnEvent for Messages {
    fn on_event(&mut self, ctx: &mut Context, sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(KeyboardEvent{key: Key::Enter, state: KeyboardState::Pressed, ..}) = event.downcast_ref() {
            self.2.apply(SendMessage(self.1.2.1.spans[0].text.clone()));
        }
        if let Some(TickEvent) = event.downcast_ref() {
            if self.2.pending_updated() {
                self.1.1 = self.2.load_pending().messages.iter().map(|message|
                    MessageItem(Stack::start(),
                        Shape{shape: ShapeType::Rectangle(0.0, (500.0, 20.0), 0.0), color: Color::BLACK},
                        Text::new(&format!("{}: {}", &message.author.to_string()[..10], message.body), self.3.clone(), 16.0, Color::WHITE, Align::Left)
                    )
                ).collect();
            }
        }
        vec![event]
    }
}

impl Messages {
    pub fn new(instance: Instance<Room>, font: Font) -> Scrollable<Self> {
        let items = instance.load_pending().messages.iter().map(|message|
            MessageItem(Stack::start(),
                Shape{shape: ShapeType::Rectangle(0.0, (500.0, 20.0), 0.0), color: Color::BLACK},
                Text::new(&format!("{}: {}", &message.author.to_string()[..10], message.body), font.clone(), 16.0, Color::WHITE, Align::Left)
            )
        ).collect();
        Scrollable::end(Messages(
            Stack::new(Offset::Start, Offset::Start, Size::Fit, Size::Fit, Padding::default()),
            MessageItems(Column::start(10.0), items, TextInput::new(font.clone())),
            instance,
            font
        ))
    }
}

#[derive(Component, Debug, Clone)]
pub struct Content(Row, Rooms, Scrollable<Messages>);
impl OnEvent for Content {
    fn on_event(&mut self, ctx: &mut Context, sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(RoomCreated(room)) = event.downcast_ref::<RoomCreated>() {
            let font = self.2.1.3.clone();
            self.2 = Messages::new(room.clone(), font);
        }
        if let Some(RoomSelected(room)) = event.downcast_ref::<RoomSelected>() {
            let font = self.2.1.3.clone();
            self.2 = Messages::new(room.clone(), font);
        }
        vec![event]
    }
}

#[derive(Component, Debug, Clone)]
pub struct App(
    Column,
    Text,
    Content
);

impl OnEvent for App {}

impl App {
    pub fn new(ctx: &mut Context) -> Self {
        let font = Font::from_bytes(include_bytes!("../resources/font.ttf")).unwrap();
        let room = ctx.instances().create("Default".to_string());
        App(Column::center(24.0), 
            Text::new("ChatBot", font.clone(), 32.0, Color::WHITE, Align::Center),
            Content(Row::start(10.0), Rooms::new(ctx, font.clone()), Messages::new(room, font))
        )
    }
}

#[derive(Debug, Clone)]
pub struct RoomCreated(Instance<Room>);
impl Event for RoomCreated {
    fn pass(self: Box<Self>, _ctx: &mut Context, children: &[Area]) -> Vec<Option<Box<dyn Event>>> {
        children.iter().map(|_| Some(self.clone() as Box<dyn Event>)).collect()
    }
}      

#[derive(Debug, Clone)]
pub struct RoomSelected(Instance<Room>);
impl Event for RoomSelected {
    fn pass(self: Box<Self>, _ctx: &mut Context, children: &[Area]) -> Vec<Option<Box<dyn Event>>> {
        children.iter().map(|_| Some(self.clone() as Box<dyn Event>)).collect()
    }
}      