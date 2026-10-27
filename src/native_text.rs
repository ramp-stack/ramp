//! Native text mirrors follow Prism's drawing snapshots. Native callbacks queue owned actions;
//! Prism remains the text owner and renders the glyphs. Selection and clipboard UI are native.

use prism::Context;
use prism::drawable::Text;
use prism::events::{ChildEvent, DynEvent, Event, KeyboardEvent, MouseEvent, OnEvent, on_event};
use uuid::Uuid;

use std::ops::Range;

#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "ios")]
pub use ios::NativeText;
#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
pub use android::NativeText;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::NativeText;
#[cfg(not(any(target_os = "ios", target_os = "android", target_os = "macos")))]
mod desktop;
#[cfg(not(any(target_os = "ios", target_os = "android", target_os = "macos")))]
pub use desktop::NativeText;

pub enum Action {
    SetText(SetText),
    Selection {id: Uuid, text: String, range: Range<usize>},
    Keyboard(KeyboardEvent),
    Mouse(MouseEvent),
}

#[derive(Clone, Debug)]
pub struct SetText {
    pub id: Uuid,
    pub text: String,
    /// UTF-8 byte offset, converted by the native adapter before queuing the event.
    pub cursor: Option<usize>,
}
impl Event for SetText {
    fn pass(&self, _ctx: &mut Context, children: &mut [ChildEvent<'_, Self>]) {
        for child in children {*child.event = Some(self.clone());}
    }
}

#[derive(Clone, Debug)]
pub struct Selection {
    pub id: Uuid,
    pub text: String,
    pub range: Range<usize>,
}
impl Event for Selection {
    fn pass(&self, _ctx: &mut Context, children: &mut [ChildEvent<'_, Self>]) {
        for child in children {*child.event = Some(self.clone());}
    }
}

#[on_event]
impl OnEvent<SetText> for Text {
    fn on(&mut self, _ctx: &mut Context, event: SetText) -> Vec<Box<dyn DynEvent>> {
        if self.id != event.id {return vec![Box::new(event)]}
        if !self.spans.iter().flat_map(|span| span.text.chars()).eq(event.text.chars()) {
            self.spans.truncate(1);
            self.spans.first_mut().unwrap().text = event.text;
        }
        self.cursor = event.cursor;
        self.anchor = None;
        vec![]
    }
}

#[on_event]
impl OnEvent<Selection> for Text {
    fn on(&mut self, _ctx: &mut Context, event: Selection) -> Vec<Box<dyn DynEvent>> {
        if self.id != event.id {return vec![Box::new(event)]}
        if self.spans.iter().flat_map(|span| span.text.chars()).eq(event.text.chars()) {
            self.cursor = Some(event.range.end);
            self.anchor = (event.range.start != event.range.end).then_some(event.range.start);
        }
        vec![]
    }
}
