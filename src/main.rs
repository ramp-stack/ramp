use air::{Contract, Id, Metadata, Name};
use futures::{FutureExt, StreamExt, stream::FuturesUnordered};
use ramp2::prism;
use prism::{Application, Context};
use prism::air::{Instance, Instances};
use prism::drawable::{Area, Color, Component, Drawable, Instruction, Item, Layout, LayoutTree};
use prism::drawable::{Offset, Rect, Size, SizeRange, Text as DrawableText, shape::{Shape, Variant}};
use prism::drawable::text::{Align, Font, Text};
use prism::events::*;
use prism::hardware::Data;
use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

ramp2::run!(Demo);

#[derive(Component, Clone, Debug)]
pub struct Demo {
    flow: Flow,
    children: Vec<Box<dyn Drawable>>,
    #[skip] font: Font,
    #[skip] me: Name,
    #[skip] rooms: Instances<ChatRoom>,
    #[skip] chats: BTreeMap<Id, Instance<ChatRoom>>,
    #[skip] selected: Id,
    #[skip] fields: [String; 2],
    #[skip] focus: usize,
    #[skip] select_all: bool,
    #[skip] scroll: [f32; 2],
    #[skip] error: Option<String>,
}
impl Application for Demo {
    fn name() -> String {"Ramp demo".into()}
    fn new(ctx: &mut Context) -> impl Drawable {
        let draft = match ctx.hardware.get_cache("draft") {
            Some(Data::Text(text)) => text,
            _ => String::new(),
        };
        let rooms = ctx.air.instances::<ChatRoom>();
        let selected = Id::hash(&("RampDemo.default", ctx.air.me()));
        rooms.create(selected);
        let Panel {flow, children} = Panel::new(true, (f32::INFINITY, f32::INFINITY), vec![]);
        let mut demo = Self {
            flow, children,
            font: Font::from_bytes(include_bytes!("../resources/font.ttf")).unwrap(),
            me: ctx.air.me(), rooms, chats: BTreeMap::new(), selected,
            fields: [String::new(), draft], focus: 1,
            select_all: false, scroll: [0.0; 2], error: None,
        };
        demo.refresh();
        demo
    }
}
impl Demo {
    fn refresh(&mut self) {
        for (id, room) in self.rooms.list() {self.chats.entry(id).or_insert(room);}
        self.rebuild();
    }
    fn control(&self, label: &str, action: Option<Action>) -> Box<dyn Drawable> {
        let text = Text::new(label, self.font.clone(), 14.0, Color::white(), Align::Left);
        let width = match action {
            Some(Action::Copy | Action::Share | Action::New | Action::Add | Action::Send) => {
                text.size().0 + 20.0
            }
            _ => f32::INFINITY,
        };
        let color = action.as_ref().map(|action| {
            let selected = match action {
                Action::Select(id) => *id == self.selected,
                Action::Focus(field) => *field == self.focus,
                _ => false,
            };
            if selected {Color(48, 79, 100, 255)} else {Color(38, 42, 50, 255)}
        });
        Box::new(Control {text, action, color, width})
    }
    fn rebuild(&mut self) {
        let fill = f32::INFINITY;
        let list = Panel::new(false, (fill, fill), self.chats.values().map(|room| {
            let room = room.pending();
            let name = format!("Chat {}", &room.id.to_string()[..6]);
            self.control(&name, Some(Action::Select(room.id)))
        }).collect());
        let list = Scroll::new(list, 0, self.scroll[0], false);
        let name = DrawableText::new(
            &self.me.to_string(), self.font.clone(), 14.0, Color::white(), Align::Left,
        );
        let sidebar = Panel::new(false, (200.0, fill), vec![
            self.control("New chat", Some(Action::New)), Box::new(list), Box::new(name),
            Box::new(Panel::new(true, (fill, 40.0), vec![
                self.control("Copy", Some(Action::Copy)), self.control("Share", Some(Action::Share)),
            ])),
        ]);
        let (title, messages) = self.room().map(|room| {
            let room = room.pending();
            let title = room.members.iter().map(|name| {
                let name = name.to_string();
                name[name.len() - 5..].to_string()
            }).collect::<Vec<_>>().join(", ");
            let messages = room.messages.iter().filter(|message| {
                message.body.strip_prefix("air-file:").and_then(|id| id.parse::<Id>().ok()).is_none()
            }).map(|message| {
                let author = if message.author == self.me {"Me".into()} else {
                    let name = message.author.to_string();
                    name[name.len() - 5..].to_string()
                };
                self.control(&format!("{author}: {}", message.body), None)
            }).collect();
            (title, messages)
        }).unwrap_or(("Loading chat…".into(), vec![]));
        let history = Scroll::new(
            Panel::new(false, (fill, fill), messages), 1, self.scroll[1], true,
        );
        let person = if self.fields[0].is_empty() {"Air name to add"} else {&self.fields[0]};
        let draft = if self.fields[1].is_empty() {"Message…"} else {&self.fields[1]};
        let content = Panel::new(false, (fill, fill), vec![
            self.control(self.error.as_deref().unwrap_or(&title), None),
            Box::new(Panel::new(true, (fill, 40.0), vec![
                self.control(person, Some(Action::Focus(0))), self.control("Add person", Some(Action::Add)),
            ])),
            Box::new(history),
            Box::new(Panel::new(true, (fill, 40.0), vec![
                self.control(draft, Some(Action::Focus(1))), self.control("Send", Some(Action::Send)),
            ])),
        ]);
        let Panel {flow, children} = Panel::new(
            true, (fill, fill), vec![Box::new(sidebar), Box::new(content)],
        );
        self.flow = flow;
        self.children = children;
    }
}
#[on_change(self.rooms.next())]
impl OnChange<Instance<ChatRoom>> for Demo {
    fn on_change(&mut self, _ctx: &mut Context, room: Instance<ChatRoom>) {
        self.chats.entry(room.id()).or_insert(room);
        self.rebuild();
    }
}

#[on_change(async {
    let mut waits: FuturesUnordered<_> = self.chats.values_mut().map(|chat| chat.next()).collect();
    if waits.next().await.is_none() {std::future::pending::<()>().await;}
    while let Some(Some(_)) = waits.next().now_or_never() {}
})]
impl OnChange<()> for Demo {
    fn on_change(&mut self, _ctx: &mut Context, _: ()) {self.rebuild();}
}

#[on_event]
impl OnEvent<KeyboardEvent> for Demo {
    fn on(&mut self, ctx: &mut Context, key: KeyboardEvent) -> Vec<Box<dyn DynEvent>> {
        if key.state != KeyboardState::Released {ctx.emit(Action::Key(key));}
        vec![]
    }
}
#[on_event]
impl OnEvent<Action> for Demo {
    fn on(&mut self, ctx: &mut Context, action: Action) -> Vec<Box<dyn DynEvent>> {
        self.action(ctx, action);
        self.refresh();
        vec![]
    }
}
impl Demo {
    fn room(&self) -> Option<Instance<ChatRoom>> {
        self.chats.iter().find(|(_, room)| room.pending().id == self.selected)
            .map(|(_, room)| room.clone())
    }
    fn insert(&mut self, ctx: &Context, text: &str) {
        if self.select_all {self.fields[self.focus].clear(); self.select_all = false;}
        self.fields[self.focus].push_str(text);
        self.error = ctx.hardware.set_cache("draft", &Data::Text(self.fields[1].clone()))
            .err().map(|error| format!("Draft was not saved: {error}"));
    }
    fn action(&mut self, ctx: &mut Context, action: Action) {
        match action {
            Action::Copy => ctx.hardware.set_clipboard(&Data::Text(self.me.to_string())),
            Action::Share => ctx.hardware.share(&Data::Text(self.me.to_string())),
            Action::Paste => if let Some(Data::Text(text)) = ctx.hardware.get_clipboard() {
                self.insert(ctx, &text);
            }
            Action::New => {
                self.selected = Id::random();
                self.rooms.create(self.selected);
                self.chats = self.rooms.list().into_iter().collect();
                self.fields[0].clear(); self.focus = 1; self.select_all = false;
                self.scroll[1] = 0.0;
            }
            Action::Select(id) => {
                self.selected = id; self.fields[0].clear(); self.select_all = false;
                self.scroll[1] = 0.0;
            }
            Action::Focus(field) => {self.focus = field; self.select_all = false;}
            Action::Add => {
                let name = match self.fields[0].trim().parse::<Name>() {
                    Ok(name) => name,
                    Err(error) => {log::warn!("Invalid Air name: {error}"); return}
                };
                let Some(mut room) = self.room() else {return};
                self.fields[0].clear(); self.focus = 1; self.select_all = false;
                std::thread::spawn(move || {
                    if futures::executor::block_on(room.send(ChatAction::AddMember(name)).confirmed()) {
                        room.share(name);
                    }
                });
            }
            Action::Send => if let Some(mut room) = self.room()
                && !self.fields[1].is_empty()
            {
                room.send(ChatAction::SendMessage(std::mem::take(&mut self.fields[1])));
                self.scroll[1] = 0.0;
                self.error = ctx.hardware.set_cache("draft", &Data::Text(String::new()))
                    .err().map(|error| format!("Saved draft was not cleared: {error}"));
            },
            Action::Scroll(index, delta, maximum) => {
                self.scroll[index] = (self.scroll[index].clamp(0.0, maximum) + delta)
                    .clamp(0.0, maximum);
            }
            Action::Key(key) => {
                let command = key.modifiers.control || key.modifiers.supermeta;
                match &key.key {
                    Key::Character(text) if command => match text.to_lowercase().as_str() {
                        "v" => self.action(ctx, Action::Paste), "a" => self.select_all = true,
                        "c" => ctx.hardware.set_clipboard(&Data::Text(self.fields[self.focus].clone())),
                        _ => {}
                    },
                    Key::Character(text) => self.insert(ctx, text),
                    Key::Space => self.insert(ctx, " "),
                    Key::Backspace => {
                        let field = &mut self.fields[self.focus];
                        if self.select_all {field.clear();} else {field.pop();}
                        self.select_all = false; self.insert(ctx, "");
                    }
                    Key::Tab => {self.focus = 1 - self.focus; self.select_all = false;}
                    Key::Enter => self.action(ctx, if self.focus == 0 {Action::Add} else {Action::Send}),
                    _ => {}
                }
            }
        }
    }
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
struct Message {
    author: Name,
    body: String,
    timestamp: u64,
}

#[derive(Clone, Debug)]
enum Action {
    Copy, Share, Paste, New, Add, Send, Select(Id), Focus(usize), Key(KeyboardEvent),
    Scroll(usize, f32, f32),
}
impl Event for Action {
    fn pass(&self, _: &mut Context, children: &mut [ChildEvent<'_, Self>]) {
        for child in children {*child.event = Some(self.clone());}
    }
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
enum ChatAction {SendMessage(String), AddMember(Name)}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
struct ChatRoom {
    members: Vec<Name>,
    messages: Vec<Message>,
    id: Id,
}
impl Contract for ChatRoom {
    type Init = Id;
    type Message = ChatAction;
    type Result = bool;

    fn id() -> Id {Id::hash("ChatRoom2.9")}
    fn init(id: Id, metadata: Metadata) -> Self {
        Self {members: vec![metadata.signer], messages: vec![], id}
    }
    fn apply(&mut self, message: ChatAction, metadata: Metadata) -> bool {
        match message {
            ChatAction::SendMessage(body) => self.messages.push(Message {
                author: metadata.signer, body, timestamp: metadata.timestamp,
            }),
            ChatAction::AddMember(name) => {
                if self.members.contains(&name) {return false}
                self.members.push(name);
            }
        }
        true
    }
}

#[derive(Clone, Debug)]
struct Flow {
    row: bool,
    range: SizeRange,
    children: Vec<SizeRange>,
}
impl Layout for Flow {
    fn request_width(&mut self, children: Vec<(f32, f32)>) -> (f32, f32) {
        self.children = children.into_iter().map(|(min_width, max_width)| SizeRange {
            min_width, max_width, min_height: 0.0, max_height: 0.0,
        }).collect();
        (self.range.min_width, self.range.max_width)
    }

    fn assign_width(&mut self, width: f32) -> Vec<f32> {
        let available = (width - 8.0).max(0.0);
        let widths: Vec<_> = if self.row {
            let gaps = self.children.len().saturating_sub(1) as f32 * 8.0;
            let minimum = self.children.iter().map(|r| r.min_width).sum::<f32>() + gaps;
            let flexible = self.children.iter().filter(|r| r.max_width.is_infinite()).count().max(1);
            let extra = (available - minimum).max(0.0) / flexible as f32;
            self.children.iter().map(|r| {
                r.min_width + if r.max_width.is_infinite() {extra} else {0.0}
            }).collect()
        } else {
            self.children.iter().map(|r| available.clamp(r.min_width, r.max_width)).collect()
        };
        for (child, &width) in self.children.iter_mut().zip(&widths) {
            child.min_width = width;
            child.max_width = width;
        }
        widths
    }

    fn request_height(&mut self, _width: f32, children: Vec<(f32, f32)>) -> (f32, f32) {
        for (child, (min, max)) in self.children.iter_mut().zip(children) {
            child.min_height = min;
            child.max_height = max;
        }
        (self.range.min_height, self.range.max_height)
    }

    fn layout(&mut self, size: Size) -> Vec<Area> {
        let available = (size.1 - 8.0).max(0.0);
        let heights: Vec<_> = if self.row {
            self.children.iter().map(|r| available.clamp(r.min_height, r.max_height)).collect()
        } else {
            let gaps = self.children.len().saturating_sub(1) as f32 * 8.0;
            let minimum = self.children.iter().map(|r| r.min_height).sum::<f32>() + gaps;
            let flexible = self.children.iter().filter(|r| r.max_height.is_infinite()).count().max(1);
            let extra = (available - minimum).max(0.0) / flexible as f32;
            self.children.iter().map(|r| {
                r.min_height + if r.max_height.is_infinite() {extra} else {0.0}
            }).collect()
        };
        let mut cursor = 4.0;
        self.children.iter().zip(heights).map(|(child, height)| {
            let size = (child.min_width, height);
            let offset = if self.row {(cursor, 4.0)} else {(4.0, cursor)};
            cursor += if self.row {size.0 + 8.0} else {size.1 + 8.0};
            Area {offset, size}
        }).collect()
    }
}

#[derive(Clone, Debug, Component)]
struct Panel {
    flow: Flow,
    children: Vec<Box<dyn Drawable>>,
}
impl Panel {
    fn new(row: bool, size: Size, children: Vec<Box<dyn Drawable>>) -> Self {
        let range = SizeRange {
            min_width: if size.0.is_finite() {size.0} else {0.0}, max_width: size.0,
            min_height: if size.1.is_finite() {size.1} else {0.0}, max_height: size.1,
        };
        Self {flow: Flow {row, range, children: vec![]}, children}
    }
}

#[derive(Clone, Debug)]
struct ScrollLayout {
    flow: Flow,
    offset: f32,
    maximum: f32,
    bottom: bool,
}
impl Layout for ScrollLayout {
    fn request_width(&mut self, children: Vec<(f32, f32)>) -> (f32, f32) {
        self.flow.request_width(children)
    }
    fn assign_width(&mut self, width: f32) -> Vec<f32> {self.flow.assign_width(width)}
    fn request_height(&mut self, width: f32, children: Vec<(f32, f32)>) -> (f32, f32) {
        self.flow.request_height(width, children)
    }
    fn layout(&mut self, size: Size) -> Vec<Area> {
        let mut areas = self.flow.layout(size);
        let end = areas.iter().map(|area| area.offset.1 + area.size.1 + 4.0)
            .fold(0.0, f32::max);
        self.maximum = (end - size.1).max(0.0);
        let offset = self.offset.clamp(0.0, self.maximum);
        let shift = if self.bottom {self.maximum - offset} else {offset};
        for area in &mut areas {area.offset.1 -= shift;}
        areas
    }
}

#[derive(Component, Clone, Debug)]
struct Scroll {
    layout: ScrollLayout,
    children: Vec<Box<dyn Drawable>>,
    #[skip] index: usize,
}
impl Scroll {
    fn new(panel: Panel, index: usize, offset: f32, bottom: bool) -> Self {
        let Panel {flow, children} = panel;
        Self {layout: ScrollLayout {flow, offset, maximum: 0.0, bottom}, children, index}
    }
}
#[on_event]
impl OnEvent<WindowEvent> for Scroll {
    fn on(&mut self, ctx: &mut Context, event: WindowEvent) -> Vec<Box<dyn DynEvent>> {
        if matches!(event, WindowEvent::Resized(_)) {
            ctx.emit(Action::Scroll(self.index, 0.0, self.layout.maximum));
        }
        prism::events![event]
    }
}
#[on_event]
impl OnEvent<MouseEvent> for Scroll {
    fn on(&mut self, ctx: &mut Context, event: MouseEvent) -> Vec<Box<dyn DynEvent>> {
        if let MouseEvent {position: Some(_), state: MouseState::Scroll(_, y)} = event {
            let delta = y * if self.layout.bottom {32.0} else {-32.0};
            ctx.emit(Action::Scroll(self.index, delta, self.layout.maximum));
            vec![]
        } else {prism::events![event]}
    }
}

#[derive(Clone, Debug)]
struct Control {
    text: Text,
    action: Option<Action>,
    color: Option<Color>,
    width: f32,
}
impl Drawable for Control {
    fn request_width(&mut self) -> (f32, f32) {
        (if self.width.is_finite() {self.width} else {0.0}, self.width)
    }
    fn request_height(&mut self, width: f32) -> (f32, f32) {
        self.text.width = Some((width - 12.0).max(0.0));
        self.text.max_lines = Some(1);
        (32.0, 32.0)
    }
    fn draw(&self, sized: &LayoutTree, offset: Offset, bound: Rect) -> Vec<Instruction> {
        let mut instructions = vec![];
        if let Some(color) = self.color {
            let shape = Shape::new(sized.0.0, sized.0.1, color, Variant::RoundedRectangle(6.0));
            instructions.push(Instruction {bound, offset, item: Item::Shape(shape)});
        }
        let offset = (offset.0 + 6.0, offset.1 + 6.0);
        instructions.push(Instruction {bound, offset, item: Item::Text(self.text.clone())});
        instructions
    }
    fn event(&mut self, ctx: &mut Context, _: &LayoutTree, event: Box<dyn DynEvent>) {
        EventHandler::dispatch(self, ctx, event);
    }
}
#[on_event]
impl OnEvent<MouseEvent> for Control {
    fn on(&mut self, ctx: &mut Context, event: MouseEvent) -> Vec<Box<dyn DynEvent>> {
        if event.position.is_some() && event.state == MouseState::Pressed(MouseButton::Left)
            && let Some(action) = &self.action {ctx.emit(action.clone());}
        vec![]
    }
}
