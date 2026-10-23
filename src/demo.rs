use air::{Contract, Id, Metadata, Name};
use ramp2::prism;
use prism::{Application, Context};
use prism::air::{Instance, Instances};
use prism::drawable::{Area, Color, Component, Drawable, Instruction, Item, Layout, LayoutTree};
use prism::drawable::{Offset, RangeTree, Rect, Size, SizeRange, shape::{Shape, Variant}};
use prism::drawable::text::{Align, Font, Text};
use prism::events::*;
use prism::hardware::Data;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Demo {
    ui: Panel,
    font: Font,
    me: Name,
    rooms: Instances<ChatRoom>,
    chats: BTreeMap<Id, Instance<ChatRoom>>,
    selected: Id,
    fields: [String; 2],
    focus: usize,
    select_all: bool,
    scroll: [f32; 2],
    error: Option<String>,
}
impl Component for Demo {
    fn children(&self) -> Vec<&dyn Drawable> {self.ui.children()}
    fn children_mut(&mut self) -> Vec<&mut dyn Drawable> {self.ui.children_mut()}
    fn layout(&self) -> &dyn Layout {&self.ui.flow}
    fn changed(&mut self) -> ChangeFuture<'_> {
        Box::pin(Change::select(vec![
            Box::pin(async {
                let room = self.rooms.next().await;
                Change::new(move |drawable, _| {
                    let demo = drawable.downcast_mut::<Self>().unwrap();
                    demo.chats.entry(room.id()).or_insert(room);
                    demo.ui = demo.view();
                })
            }),
            Box::pin(async {
                ChangeSource::changed(&mut self.chats).await;
                Change::new(|drawable, _| {
                    let demo = drawable.downcast_mut::<Self>().unwrap();
                    demo.ui = demo.view();
                })
            }),
            Box::pin(async {
                Drawable::changed(&mut self.ui).await.map::<Self>(|demo| &mut demo.ui)
            }),
        ]))
    }
}
impl Application for Demo {
    fn name() -> String {"Ramp demo".into()}
    fn new(ctx: &mut Context) -> impl Drawable {
        let (draft, error) = match ctx.hardware.get_cache("draft") {
            Ok(Some(Data::Text(text))) => (text, None),
            Ok(_) => (String::new(), None),
            Err(error) => (String::new(), Some(format!("Could not restore draft: {error}"))),
        };
        let rooms = ctx.air.instances::<ChatRoom>();
        let selected = Id::hash(&("RampDemo.default", ctx.air.me()));
        rooms.create(selected);
        let mut demo = Self {
            ui: Panel::new(true, (f32::INFINITY, f32::INFINITY), vec![]),
            font: Font::from_bytes(include_bytes!("../resources/font.ttf")).unwrap(),
            me: ctx.air.me(), rooms, chats: BTreeMap::new(), selected, fields: [String::new(), draft], focus: 1,
            select_all: false, scroll: [0.0; 2], error,
        };
        demo.refresh();
        demo
    }
}
impl Demo {
    fn refresh(&mut self) {
        for (id, room) in self.rooms.list() {self.chats.entry(id).or_insert(room);}
        self.ui = self.view();
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
    fn view(&self) -> Panel {
        let fill = f32::INFINITY;
        let list = Panel::new(false, (fill, fill), self.chats.iter().map(|(_, room)| {
            let room = room.pending();
            let name = format!("Chat {}", &room.id.to_string()[..6]);
            self.control(&name, Some(Action::Select(room.id)))
        }).collect());
        let list = Scroll {panel: list, index: 0, offset: self.scroll[0], maximum: 0.0, bottom: false};
        let mut name = Text::new(&self.me.to_string(), self.font.clone(), 14.0, Color::white(), Align::Left);
        name.width = Some(184.0);
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
        let history = Scroll {
            panel: Panel::new(false, (fill, fill), messages), index: 1,
            offset: self.scroll[1], maximum: 0.0, bottom: true,
        };
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
        Panel::new(true, (fill, fill), vec![Box::new(sidebar), Box::new(content)])
    }
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
            Action::Send => if let Some(mut room) = self.room() {
                if !self.fields[1].is_empty() {
                    room.send(ChatAction::SendMessage(std::mem::take(&mut self.fields[1])));
                    self.scroll[1] = 0.0;
                    self.error = ctx.hardware.set_cache("draft", &Data::Text(String::new()))
                        .err().map(|error| format!("Saved draft was not cleared: {error}"));
                }
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
struct Message {
    author: Name,
    body: String,
    timestamp: u64,
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
}
impl Layout for Flow {
    fn size_range(&self, _: Vec<SizeRange>) -> SizeRange {self.range}
    fn layout(&self, size: Size, children: Vec<SizeRange>) -> Vec<Area> {
        let axis = usize::from(!self.row);
        let cross = 1 - axis;
        let available = [(size.0 - 8.0).max(0.0), (size.1 - 8.0).max(0.0)];
        let ranges: Vec<_> = children.iter().map(|r| [
            [r.min_width, r.min_height], [r.max_width, r.max_height],
        ]).collect();
        let gaps = children.len().saturating_sub(1) as f32 * 8.0;
        let minimum = ranges.iter().map(|r| r[0][axis]).sum::<f32>() + gaps;
        let flexible = ranges.iter().filter(|r| r[1][axis].is_infinite()).count().max(1);
        let extra = (available[axis] - minimum).max(0.0) / flexible as f32;
        let sizes: Vec<_> = ranges.iter().map(|r| {
            let mut size = r[0];
            if r[1][axis].is_infinite() {size[axis] += extra;}
            size[cross] = available[cross].clamp(r[0][cross], r[1][cross]);
            size
        }).collect();
        let mut cursor = 4.0;
        sizes.into_iter().map(|size| {
            let mut offset = [4.0, 4.0];
            offset[axis] = cursor;
            cursor += size[axis] + 8.0;
            Area {offset: (offset[0], offset[1]), size: (size[0], size[1])}
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
        Self {flow: Flow {row, range}, children}
    }
}

#[derive(Clone, Debug)]
struct Scroll {
    panel: Panel,
    index: usize,
    offset: f32,
    maximum: f32,
    bottom: bool,
}
impl Drawable for Scroll {
    fn changed(&mut self) -> ChangeFuture<'_> {
        Box::pin(async move {
            Drawable::changed(&mut self.panel).await.map::<Self>(|scroll| &mut scroll.panel)
        })
    }

    fn size_range(&self) -> RangeTree {self.panel.size_range()}
    fn layout(&mut self, size: Size, range: &RangeTree) -> LayoutTree {
        let mut tree = Drawable::layout(&mut self.panel, size, range);
        let end = tree.1.iter().map(|(offset, child)| offset.1 + child.0.1 + 4.0)
            .fold(0.0, f32::max);
        self.maximum = (end - tree.0.1).max(0.0);
        let offset = self.offset.clamp(0.0, self.maximum);
        let shift = if self.bottom {self.maximum - offset} else {offset};
        for (offset, _) in &mut tree.1 {offset.1 -= shift;}
        tree
    }
    fn draw(&self, sized: &LayoutTree, offset: Offset, bound: Rect) -> Vec<Instruction> {
        self.panel.draw(sized, offset, bound)
    }
    fn event(&mut self, ctx: &mut Context, sized: &LayoutTree, event: Box<dyn DynEvent>) {
        if matches!(event.downcast_ref::<WindowEvent>(), Some(WindowEvent::Resized(_))) {
            ctx.emit(Action::Scroll(self.index, 0.0, self.maximum));
        }
        if let Some(MouseEvent {position: Some(_), state: MouseState::Scroll(_, y)}) =
            event.downcast_ref::<MouseEvent>() {
            let delta = y * if self.bottom {32.0} else {-32.0};
            ctx.emit(Action::Scroll(self.index, delta, self.maximum));
        } else {
            self.panel.event(ctx, sized, event);
        }
    }
}

#[derive(Clone, Debug)]
struct Control {
    text: Text, action: Option<Action>, color: Option<Color>, width: f32,
}
impl Drawable for Control {
    fn size_range(&self) -> RangeTree {
        RangeTree(SizeRange {
            min_width: if self.width.is_finite() {self.width} else {0.0}, max_width: self.width,
            min_height: 32.0, max_height: 32.0,
        }, vec![])
    }
    fn draw(&self, sized: &LayoutTree, offset: Offset, bound: Rect) -> Vec<Instruction> {
        let mut instructions = vec![];
        if let Some(color) = self.color {
            let shape = Shape::new(sized.0.0, sized.0.1, color, Variant::RoundedRectangle(6.0));
            instructions.push(Instruction {bound, offset, item: Item::Shape(shape)});
        }
        let mut text = self.text.clone();
        text.width = Some((sized.0.0 - 12.0).max(0.0)); text.max_lines = Some(1);
        let offset = (offset.0 + 6.0, offset.1 + 6.0);
        instructions.push(Instruction {bound, offset, item: Item::Text(text)});
        instructions
    }
    fn event(&mut self, ctx: &mut Context, _: &LayoutTree, event: Box<dyn DynEvent>) {
        if let Some(MouseEvent {position: Some(_), state: MouseState::Pressed(MouseButton::Left)}) =
            event.downcast_ref::<MouseEvent>() {
            if let Some(action) = &self.action {ctx.emit(action.clone());}
        }
    }
}
