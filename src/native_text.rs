//! Native text mirrors follow Prism's drawing snapshots. Native callbacks queue owned actions;
//! Prism remains the text owner and renders the glyphs. Editing and selection UI are native.

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
    SetText {edit: SetText, revision: u64},
    Selection {id: Uuid, text: String, range: Range<usize>, revision: u64},
    Keyboard(KeyboardEvent),
    Mouse(MouseEvent),
}

#[derive(Clone, Debug)]
pub struct SetText {
    pub id: Uuid,
    pub text: String,
    /// UTF-8 byte offsets, converted by the native adapter before queuing the event.
    pub range: Range<usize>,
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
        let value: String = self.spans.iter().map(|span| span.text.as_str()).collect();
        if value != event.text {
            let start: usize = value.chars().zip(event.text.chars())
                .take_while(|(old, new)| old == new).map(|(c, _)| c.len_utf8()).sum();
            let suffix: usize = value[start..].chars().rev().zip(event.text[start..].chars().rev())
                .take_while(|(old, new)| old == new).map(|(c, _)| c.len_utf8()).sum();
            let range = start..value.len() - suffix;
            let mut offset = 0;
            for span in &mut self.spans {
                let end = offset + span.text.len();
                let from = range.start.max(offset);
                let to = range.end.min(end);
                if from < to {span.text.replace_range(from - offset..to - offset, "");}
                offset = end;
            }
            let mut offset = start;
            let span = self.spans.iter_mut().find(|span| {
                if offset <= span.text.len() {true} else {offset -= span.text.len(); false}
            }).unwrap();
            span.text.insert_str(offset, &event.text[start..event.text.len() - suffix]);
        }
        self.cursor = Some(event.range.end);
        self.anchor = (event.range.start != event.range.end).then_some(event.range.start);
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
