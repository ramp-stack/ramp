use futures::channel::mpsc::UnboundedSender;
use objc2::{define_class, msg_send, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::{
    NSBackgroundColorAttributeName, NSClipView, NSColor, NSEvent, NSEventModifierFlags,
    NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSKernAttributeName,
    NSLineBreakMode, NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSSelectionGranularity,
    NSTextAlignment, NSTextDelegate, NSTextInputClient, NSTextView, NSTextViewDelegate, NSView,
};
use objc2_core_foundation::{CFData, CFRetained};
use objc2_core_text::{CTFont, CTFontManagerCreateFontDescriptorFromData};
use objc2_foundation::{
    NSDictionary, NSMutableAttributedString, NSNotification, NSNumber, NSObjectProtocol,
    NSPoint, NSRange, NSRect, NSSize, NSString, NSUndoManager,
};
use prism::drawable::{Instruction, Item};
use prism::drawable::text::{Align, Text};
use prism::events::{MouseButton, MouseEvent, MouseState};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use uuid::Uuid;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::{Action, SetText};

pub struct NativeText {
    parent: Retained<NSView>,
    sender: UnboundedSender<Action>,
    sequence: Rc<Cell<u64>>,
    views: HashMap<Uuid, (Retained<NSClipView>, Retained<TextView>)>,
    fonts: HashMap<(usize, u32), Retained<NSFont>>,
}
impl NativeText {
    pub fn new(window: &maverick_os::Window, sender: UnboundedSender<Action>, revision: u64) -> Self {
        let RawWindowHandle::AppKit(handle) = window.window_handle().unwrap().as_raw()
            else {unreachable!()};
        MainThreadMarker::new().unwrap();
        let parent = unsafe {Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()).unwrap()};
        Self {
            parent, sender, sequence: Rc::new(Cell::new(revision)),
            views: HashMap::new(), fonts: HashMap::new(),
        }
    }

    pub fn update(&mut self, instructions: &[Instruction], revision: u64) {
        let mtm = MainThreadMarker::new().unwrap();
        let mut seen = HashSet::new();
        let mut visible = HashSet::new();
        let focused = instructions.iter().find_map(|instruction| match &instruction.item {
            Item::Text(text) if text.focused => Some(text.id), _ => None,
        });
        for instruction in instructions {
            let Item::Text(text) = &instruction.item else {continue};
            if !seen.insert(text.id) {continue}
            let (width, height) = text.size();
            let width = text.width.unwrap_or(width);
            let (left, top, clip_width, clip_height) = instruction.bound;
            let (right, bottom) = (left + clip_width, top + clip_height);
            let left = left.max(instruction.offset.0);
            let top = top.max(instruction.offset.1);
            let right = right.min(instruction.offset.0 + width);
            let bottom = bottom.min(instruction.offset.1 + height);
            if right <= left || bottom <= top {
                if let Some((_, view)) = self.views.get(&text.id) && view.ivars().revision.get() <= revision {
                    *view.ivars().text.borrow_mut() = Some(text.clone());
                }
                continue;
            }
            visible.insert(text.id);

            let (clip, view) = self.views.entry(text.id).or_insert_with(|| {
                let clip = NSClipView::initWithFrame(NSClipView::alloc(mtm), NSRect::ZERO);
                clip.setDrawsBackground(false);
                let view = TextView::new(&self.parent, self.sender.clone(), self.sequence.clone(), mtm);
                clip.setDocumentView(Some(&view));
                self.parent.addSubview(&clip);
                (clip, view)
            });
            view.ivars().updating.set(true);
            let container = unsafe {view.textContainer().unwrap()};
            container.setContainerSize(NSSize::new(width as f64, f64::MAX));
            container.setMaximumNumberOfLines(text.max_lines.unwrap_or(0) as usize);
            container.setLineBreakMode(if text.max_lines.is_some() {NSLineBreakMode::ByTruncatingTail}
                else {NSLineBreakMode::ByWordWrapping});
            view.setFrame(NSRect::new(NSPoint::ZERO, NSSize::new(width as f64, height as f64)));
            let y = if self.parent.isFlipped() {top as f64}
                else {self.parent.bounds().size.height - bottom as f64};
            clip.setFrame(NSRect::new(
                NSPoint::new(left as f64, y), NSSize::new((right - left) as f64, (bottom - top) as f64),
            ));
            clip.setBoundsOrigin(NSPoint::new(
                (left - instruction.offset.0) as f64, (top - instruction.offset.1) as f64,
            ));
            let hidden = clip.isHidden();
            clip.setHidden(false);

            if view.ivars().revision.get() > revision {
                view.ivars().updating.set(false);
                continue;
            }
            let previous = view.ivars().text.borrow();
            let string = text.spans.iter().map(|span| span.text.as_str()).collect::<String>();
            let preserve = view.string().to_string() == string;
            let changed = !preserve || previous.as_ref().is_none_or(|old| {
                old.spans != text.spans || old.align != text.align
            });
            let selected = view.selectedRange();
            if changed {
                let attributed = NSMutableAttributedString::initWithString(
                    NSMutableAttributedString::alloc(), &NSString::from_str(&string),
                );
                let mut location = 0;
                let mut first = None;
                for span in &text.spans {
                    let key = (span.font.file_hash(), span.font_size.to_bits());
                    let font = self.fonts.entry(key).or_insert_with(|| unsafe {
                        let data = CFData::from_bytes(span.font.bytes());
                        let descriptor = CTFontManagerCreateFontDescriptorFromData(&data).unwrap();
                        let font = CTFont::with_font_descriptor(
                            &descriptor, span.font_size as f64, std::ptr::null(),
                        );
                        // CTFont and NSFont are toll-free bridged; the descriptor owns the font data.
                        Retained::retain(CFRetained::as_ptr(&font).as_ptr().cast::<NSFont>()).unwrap()
                    });
                    let paragraph = NSMutableParagraphStyle::new();
                    paragraph.setAlignment(match text.align {
                        Align::Left => NSTextAlignment::Left, Align::Center => NSTextAlignment::Center,
                        Align::Right => NSTextAlignment::Right,
                    });
                    paragraph.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
                    let height = span.line_height.unwrap_or_else(|| {
                        span.font.horizontal_line_metrics(span.font_size).unwrap().new_line_size
                    });
                    paragraph.setMinimumLineHeight(height as f64);
                    paragraph.setMaximumLineHeight(height as f64);
                    let range = NSRange::new(location, span.text.encode_utf16().count());
                    unsafe {
                        attributed.addAttribute_value_range(NSFontAttributeName, font, range);
                        attributed.addAttribute_value_range(
                            NSForegroundColorAttributeName, &NSColor::clearColor(), range,
                        );
                        attributed.addAttribute_value_range(
                            NSKernAttributeName, &NSNumber::new_f32(span.kerning), range,
                        );
                        attributed.addAttribute_value_range(NSParagraphStyleAttributeName, &paragraph, range);
                    }
                    if first.is_none() {first = Some((font.clone(), paragraph));}
                    location += range.length;
                }
                let native = unsafe {view.textStorage().unwrap()};
                if let Some((font, paragraph)) = first && (!preserve || attributed.length() == 0) {
                    view.setFont(Some(&font));
                    view.setDefaultParagraphStyle(Some(&paragraph));
                }
                if preserve {
                    native.beginEditing();
                    let mut index = 0;
                    while index < attributed.length() {
                        let mut range = NSRange::new(0, 0);
                        unsafe {
                            let attributes = attributed.attributesAtIndex_effectiveRange(index, &mut range);
                            native.addAttributes_range(&attributes, range);
                        }
                        index = range.location + range.length;
                    }
                    native.endEditing();
                } else {
                    if view.hasMarkedText() {view.unmarkText();}
                    native.setAttributedString(&attributed);
                    view.breakUndoCoalescing();
                    view.ivars().undo.removeAllActions();
                }
            }
            view.setEditable(text.cursor.is_some());
            if text.focused && (hidden || previous.as_ref().is_none_or(|old| !old.focused)) {
                view.window().unwrap().makeFirstResponder(Some(view));
            } else if !text.focused && previous.as_ref().is_some_and(|old| old.focused)
                && view.is_first_responder()
            {
                view.window().unwrap().makeFirstResponder(Some(&self.parent));
            }
            if let Some(cursor) = text.cursor && !view.hasMarkedText() {
                let anchor = TextView::utf16_offset(&string, text.anchor.unwrap_or(cursor));
                let cursor = TextView::utf16_offset(&string, cursor);
                let range = NSRange::new(cursor.min(anchor), cursor.abs_diff(anchor));
                if view.selectedRange() != range {view.setSelectedRange(range);}
            } else if text.cursor.is_none() && preserve && view.selectedRange() != selected {
                view.setSelectedRange(selected);
            }
            drop(previous);
            *view.ivars().text.borrow_mut() = Some(text.clone());
            view.ivars().updating.set(false);
        }
        self.views.retain(|id, (clip, view)| {
            if seen.contains(id) {
                clip.setHidden(!visible.contains(id));
                return true;
            }
            let editing = view.ivars().text.borrow().as_ref().is_some_and(|text| text.focused);
            if focused.is_none_or(|focused| focused == *id) && editing {
                clip.setHidden(true);
                return true;
            }
            view.detach();
            clip.removeFromSuperview();
            false
        });
    }
}
impl Drop for NativeText {
    fn drop(&mut self) {
        for (clip, view) in self.views.values() {
            view.detach();
            clip.removeFromSuperview();
        }
    }
}

struct TextState {
    parent: Retained<NSView>,
    sender: RefCell<Option<UnboundedSender<Action>>>,
    text: RefCell<Option<Text>>,
    undo: Retained<NSUndoManager>,
    sequence: Rc<Cell<u64>>,
    revision: Cell<u64>,
    updating: Cell<bool>,
    anchor: Cell<NSRange>,
}
define_class!(
    #[unsafe(super(NSTextView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TextState]
    struct TextView;

    unsafe impl NSObjectProtocol for TextView {}
    unsafe impl NSTextDelegate for TextView {
        #[unsafe(method(textDidChange:))]
        fn text_did_change(&self, _notification: &NSNotification) {
            if self.ivars().updating.get() || !self.isEditable() {return}
            let id = self.ivars().text.borrow().as_ref().unwrap().id;
            let text = self.string().to_string();
            let selected = self.selectedRange();
            let range = Self::byte_offset(&text, selected.location)
                ..Self::byte_offset(&text, selected.location + selected.length);
            self.send(Action::SetText {edit: SetText {id, text, range}, revision: self.revision()});
        }
    }
    unsafe impl NSTextViewDelegate for TextView {
        #[unsafe(method(textViewDidChangeSelection:))]
        fn text_view_did_change_selection(&self, _notification: &NSNotification) {self.selection();}

        #[unsafe(method_id(undoManagerForTextView:))]
        fn undo_manager_for_text_view(&self, _view: &NSTextView) -> Option<Retained<NSUndoManager>> {
            Some(self.ivars().undo.clone())
        }
    }
    impl TextView {
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if self.editing_shortcut(event) {return}
            if self.forward_key(event) {self.ivars().parent.keyDown(event);}
            else {unsafe {let _: () = msg_send![super(self), keyDown: event];}}
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            if self.forward_key(event) {self.ivars().parent.keyUp(event);}
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {self.ivars().parent.flagsChanged(event);}

        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {self.editing_shortcut(event)}

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            // AppKit's blocking text tracking loop would postpone Winit's press dispatch.
            self.mouse(event, MouseState::Pressed(MouseButton::Left));
            self.mouse(event, MouseState::Released(MouseButton::Left));
            self.window().unwrap().makeFirstResponder(Some(self));
            self.setSelectionGranularity(match event.clickCount() {
                1 => NSSelectionGranularity::SelectByCharacter,
                2 => NSSelectionGranularity::SelectByWord,
                _ => NSSelectionGranularity::SelectByParagraph,
            });
            let range = self.range_at(event);
            if event.modifierFlags().contains(NSEventModifierFlags::Shift) {
                self.ivars().anchor.set(self.selectedRange());
                self.mouseDragged(event);
            } else {
                self.ivars().anchor.set(range);
                self.setSelectedRange(range);
                self.selection();
            }
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let anchor = self.ivars().anchor.get();
            let range = self.range_at(event);
            let start = anchor.location.min(range.location);
            let end = (anchor.location + anchor.length).max(range.location + range.length);
            self.setSelectedRange(NSRange::new(start, end - start));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {self.selection();}

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {self.ivars().parent.scrollWheel(event);}

        #[unsafe(method(autoscroll:))]
        fn autoscroll(&self, _event: &NSEvent) -> bool {false}

        #[unsafe(method(scrollRangeToVisible:))]
        fn scroll_range_to_visible(&self, _range: NSRange) {}
    }
);
impl TextView {
    fn new(parent: &NSView, sender: UnboundedSender<Action>, sequence: Rc<Cell<u64>>, mtm: MainThreadMarker) -> Retained<Self> {
        let view = Self::alloc(mtm).set_ivars(TextState {
            parent: parent.retain(), sender: RefCell::new(Some(sender)),
            text: RefCell::new(None), undo: NSUndoManager::new(mtm), sequence, revision: Cell::new(0),
            updating: Cell::new(true), anchor: Cell::new(NSRange::new(0, 0)),
        });
        let view: Retained<Self> = unsafe {msg_send![super(view), initWithFrame: NSRect::ZERO]};
        view.setDelegate(Some(ProtocolObject::from_ref(&*view)));
        view.setDrawsBackground(false);
        // AppKit's active and inactive selection fills sit above Prism's glyphs.
        view.setAlphaValue(0.3);
        view.setRichText(false);
        view.setImportsGraphics(false);
        view.setSelectable(true);
        view.setAllowsUndo(true);
        view.setContinuousSpellCheckingEnabled(true);
        view.setAutomaticSpellingCorrectionEnabled(true);
        view.setUsesFontPanel(false);
        view.setUsesRuler(false);
        view.setHorizontallyResizable(false);
        view.setVerticallyResizable(false);
        view.setTextContainerInset(NSSize::ZERO);
        view.setTextColor(Some(&NSColor::clearColor()));
        view.setInsertionPointColor(Some(&NSColor::clearColor()));
        let container = unsafe {view.textContainer().unwrap()};
        container.setLineFragmentPadding(0.0);
        container.setWidthTracksTextView(false);
        container.setHeightTracksTextView(false);
        let colors = [NSColor::clearColor(), NSColor::selectedTextBackgroundColor()];
        let attributes = unsafe {NSDictionary::from_slices(
            &[NSForegroundColorAttributeName, NSBackgroundColorAttributeName],
            &[&*colors[0] as &AnyObject, &*colors[1] as &AnyObject],
        )};
        unsafe {view.setSelectedTextAttributes(&attributes);}
        view
    }

    fn send(&self, action: Action) {
        if let Some(sender) = &*self.ivars().sender.borrow() {let _ = sender.unbounded_send(action);}
    }

    fn revision(&self) -> u64 {
        let revision = self.ivars().sequence.get() + 1;
        self.ivars().sequence.set(revision);
        self.ivars().revision.set(revision);
        revision
    }

    fn selection(&self) {
        if self.ivars().updating.get() || !self.isEditable() {return}
        let id = self.ivars().text.borrow().as_ref().unwrap().id;
        let text = self.string().to_string();
        let selected = self.selectedRange();
        let range = Self::byte_offset(&text, selected.location)
            ..Self::byte_offset(&text, selected.location + selected.length);
        self.send(Action::Selection {id, text, range, revision: self.revision()});
    }

    fn mouse(&self, event: &NSEvent, state: MouseState) {
        let parent = &self.ivars().parent;
        let point = parent.convertPoint_fromView(event.locationInWindow(), None);
        let y = if parent.isFlipped() {point.y} else {parent.bounds().size.height - point.y};
        self.send(Action::Mouse(MouseEvent {position: Some((point.x as f32, y as f32)), state}));
    }

    fn range_at(&self, event: &NSEvent) -> NSRange {
        let point = self.convertPoint_fromView(event.locationInWindow(), None);
        let index = self.characterIndexForInsertionAtPoint(point);
        self.selectionRangeForProposedRange_granularity(NSRange::new(index, 0), self.selectionGranularity())
    }

    fn forward_key(&self, event: &NSEvent) -> bool {
        !self.isEditable()
            || event.modifierFlags().intersects(NSEventModifierFlags::Command | NSEventModifierFlags::Control)
            || !self.hasMarkedText() && matches!(event.keyCode(), 36 | 48 | 53 | 76)
    }

    fn editing_shortcut(&self, event: &NSEvent) -> bool {
        if !self.is_first_responder() || !event.modifierFlags().contains(NSEventModifierFlags::Command) {
            return false;
        }
        let characters = event.charactersIgnoringModifiers().map(|value| value.to_string());
        unsafe {
            match characters.as_deref() {
                Some("c") => self.copy(None), Some("a") => self.selectAll(None),
                Some("x") if self.isEditable() => self.cut(None),
                Some("v") if self.isEditable() => self.pasteAsPlainText(None),
                Some("z" | "Z") if self.isEditable() => {
                    let undo = &self.ivars().undo;
                    if event.modifierFlags().contains(NSEventModifierFlags::Shift) {undo.redo();}
                    else {undo.undo();}
                }
                _ => return false,
            }
        }
        true
    }

    fn is_first_responder(&self) -> bool {
        self.window().and_then(|window| window.firstResponder())
            .is_some_and(|responder| std::ptr::eq(Retained::as_ptr(&responder).cast::<Self>(), self))
    }

    fn byte_offset(text: &str, utf16: usize) -> usize {
        let mut position = 0;
        for (offset, character) in text.char_indices() {
            if position >= utf16 {return offset}
            position += character.len_utf16();
        }
        text.len()
    }

    fn utf16_offset(text: &str, byte: usize) -> usize {
        text.char_indices().take_while(|(index, character)| index + character.len_utf8() <= byte)
            .map(|(_, character)| character.len_utf16()).sum()
    }

    fn detach(&self) {
        self.ivars().updating.set(true);
        self.ivars().sender.borrow_mut().take();
        self.ivars().undo.removeAllActions();
        self.setDelegate(None);
        if self.is_first_responder() {
            self.window().unwrap().makeFirstResponder(Some(&self.ivars().parent));
        }
    }
}
