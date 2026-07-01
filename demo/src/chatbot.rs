use ramp::prism;
use prism::Context;
use prism::canvas::{Color, Align, Text, Font, Shape, ShapeType};
use prism::drawable::{Component, SizedTree};
use prism::display::Bin;
use prism::layout::{Column, Stack, Row, Padding, ScrollAnchor, Size, Offset};
use ramp::air;
use prism::event::{OnEvent, Event, KeyboardEvent, Key, KeyboardState, TickEvent};

use air::{Contract, Reactants, Reactant, Instance, Name, Service, Services, Metadata, Secret, Lock, names::Id};

use serde::{Serialize, Deserialize};

#[derive(Default)]
pub struct ChatBot;
impl Service for ChatBot {
    fn id() -> Id {Id::hash("CHATBOT")}
    async fn new(_ctx: &mut air::Context, _secret: Secret) -> Self {ChatBot}
    async fn run(&mut self, ctx: &mut air::Context) {
        let mut join_set = tokio::task::JoinSet::new();
        loop { tokio::select!{
            mut room = ctx.listen::<Room>() => {
                join_set.spawn(async {(room.listen_confirmed::<SendMessage>().await, room)});
            },
            Some(Ok((index, mut room))) = join_set.join_next() => {
                let message = room.confirmed().unwrap().messages.get(index).unwrap().clone();
                if message.author == ctx.me() && !message.body.contains("ChatBot") {
                    room.apply(SendMessage(format!("ChatBot Replying to \"{:.10}...\": I totally agree", message.body)));
                }
                join_set.spawn(async {(room.listen_confirmed::<SendMessage>().await, room)});
            }
        }}
    }
    async fn shutdown(self, ctx: &mut air::Context) {
        for mut room in ctx.list::<Room>() {
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
    type Result = usize;

    fn id() -> Id {Id::hash("SendMessage")}

    fn apply(self, room: &mut Room, metadata: Metadata) -> Self::Result {
        room.messages.push(Message{author: metadata.signer, timestamp: metadata.timestamp, body: self.0});
        room.messages.len()-1
    }
}

#[derive(Component, Debug, Clone)]
pub struct RoomItem(Stack, Shape, Text);
impl OnEvent for RoomItem {}

#[derive(Component, Debug, Clone)]
pub struct RoomItems(Column, Vec<RoomItem>);
impl OnEvent for RoomItems {}

#[derive(Component, Debug, Clone)]
pub struct Rooms(Stack, Shape, RoomItems);
impl OnEvent for Rooms {}
impl Rooms {
    pub fn new(font: Font, room_names: Vec<String>) -> Self {
        Rooms(Stack::start(),
            Shape{shape: ShapeType::Rectangle(0.0, (200.0, 500.0), 0.0), color: Color::BLUE},
            RoomItems(
                Column::new(10.0, Offset::Start, Size::Static(200.0), Padding::default(), Some(ScrollAnchor::Start)),
                room_names.into_iter().map(|name| RoomItem(
                    Stack::center(),
                    Shape{shape: ShapeType::Rectangle(0.0, (200.0, 20.0), 0.0), color: Color::RED},
                    Text::new(&name, font.clone(), 16.0, Color::GREEN, Align::Left)
                )).collect()
            )
        )
    }
}

#[derive(Component, Debug, Clone)]
pub struct TextInput(Stack, Shape, Text);
impl OnEvent for TextInput {
    fn on_event(&mut self, ctx: &mut Context, sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(KeyboardEvent{key, state: KeyboardState::Pressed, ..}) = event.downcast_ref() {
            match key {
                Key::Character(c) => {self.2.spans[0].text += &c.to_string();},
                Key::Delete | Key::Backspace => {self.2.spans[0].text.pop();},
                Key::Enter => {self.2.spans[0].text = String::new();}
                _ => {}
            }
        }
        vec![event]
    }
}
impl TextInput {
    pub fn new(font: Font) -> Self {
        TextInput(Stack::center(),
            Shape{shape: ShapeType::Rectangle(0.0, (100.0, 20.0), 0.0), color: Color::YELLOW},
            Text::new("placeholder", font, 16.0, Color::GREEN, Align::Left)
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
    Shape,
    MessageItems,
    #[skip] Instance<Room>,
    #[skip] Font
);
impl OnEvent for Messages {
    fn on_event(&mut self, ctx: &mut Context, sized: &SizedTree, event: Box<dyn Event>) -> Vec<Box<dyn Event>> {
        if let Some(KeyboardEvent{key: Key::Enter, state: KeyboardState::Pressed, ..}) = event.downcast_ref() {
            println!("Sending");
            self.3.apply(SendMessage(self.2.2.2.spans[0].text.clone()));
        }
        if let Some(TickEvent) = event.downcast_ref() {
            if self.3.pending_updated() {
                self.2.1 = self.3.pending().messages.iter().map(|message|
                    MessageItem(
                        Stack::start(),
                        Shape{shape: ShapeType::Rectangle(0.0, (500.0, 20.0), 0.0), color: Color::BLACK},
                        Text::new(&format!("{}: {}", &message.author.to_string()[..10], message.body), self.4.clone(), 16.0, Color::WHITE, Align::Left)
                    )
                ).collect();
            }
        }
        vec![event]
    }
}
impl Messages {
    pub fn new(instance: Instance<Room>, font: Font) -> Self {
        Messages(
            Stack::new(Offset::Start, Offset::Start, Size::Static(500.0), Size::Static(500.0), Padding::default()),
            Shape{shape: ShapeType::Rectangle(0.0, (500.0, 500.0), 0.0), color: Color::WHITE},
            MessageItems(
                Column::new(10.0, Offset::Start, Size::Static(500.0), Padding::default(), Some(ScrollAnchor::Start)),
                vec![],
                TextInput::new(font.clone())
            ),
            instance,
            font
        )
    }
}

#[derive(Component, Debug, Clone)]
pub struct Content(Row, Rooms, Messages);
impl OnEvent for Content {}

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
        let room = ctx.create("Default".to_string());
        App(Column::center(24.0),
            Text::new("ChatBot", font.clone(), 32.0, Color::WHITE, Align::Center),
            Content(Row::start(10.0), Rooms::new(font.clone(), vec!["Default".to_string()]), Messages::new(room, font))
        )
    }
}
