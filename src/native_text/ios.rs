use futures::channel::mpsc::UnboundedSender;
use maverick_os::Window;
use objc2::{define_class, msg_send, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_core_foundation::{CGAffineTransform, CFData, CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGDataProvider, CGFont, CGRectContainsPoint};
use objc2_core_text::CTFont;
use objc2_foundation::{
    NSArray, NSMutableAttributedString, NSNumber, NSObjectProtocol, NSRange, NSSet, NSString,
};
use objc2_ui_kit::{
    NSFontAttributeName, NSForegroundColorAttributeName, NSKernAttributeName,
    NSParagraphStyleAttributeName, NSLineBreakMode, NSMutableParagraphStyle, NSTextAlignment,
    NSWritingDirection, NSTextContainer, UIColor, UIEdgeInsets, UIEvent, UIFont,
    UIEditMenuConfiguration, UIEditMenuInteraction, UIGestureRecognizer, UIGestureRecognizerDelegate,
    UIGestureRecognizerState, UILongPressGestureRecognizer, UIMenuAutoFill, UIMenuBuilder,
    UIScrollViewContentInsetAdjustmentBehavior, UIScrollViewDelegate, UITextAutocorrectionType,
    UITextInput, UITextInteraction, UITextRange, UITextSelectionDisplayInteraction, UITextSelectionRect,
    UITextSpellCheckingType, UITextView, UITextViewDelegate, UITouch, UIView,
};
use prism::drawable::{Instruction, Item};
use prism::drawable::text::{Align, Text};
use prism::events::{Key, KeyboardEvent, KeyboardState, Modifiers, MouseButton, MouseEvent, MouseState};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use uuid::Uuid;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

use super::{Action, SetText};

pub struct NativeText {
    parent: Retained<UIView>,
    views: HashMap<Uuid, Retained<TextView>>,
    fonts: HashMap<usize, CFRetained<CGFont>>,
    sender: UnboundedSender<Action>,
    sequence: Rc<Cell<u64>>,
}
impl NativeText {
    pub fn new(window: &Window, sender: UnboundedSender<Action>, revision: u64) -> Self {
        MainThreadMarker::new().unwrap();
        let RawWindowHandle::UiKit(handle) = window.window_handle().unwrap().as_raw() else {
            unreachable!()
        };
        let parent = unsafe {handle.ui_view.cast::<UIView>().as_ref()}.retain();
        Self {
            parent, views: HashMap::new(), fonts: HashMap::new(), sender,
            sequence: Rc::new(Cell::new(revision)),
        }
    }

    pub fn update(&mut self, instructions: &[Instruction], revision: u64) {
        let mut seen = HashSet::new();
        let focused = instructions.iter().find_map(|instruction| match &instruction.item {
            Item::Text(text) if text.focused => Some(text.id),
            _ => None,
        });
        for instruction in instructions {
            let Item::Text(text) = &instruction.item else {continue};
            if !seen.insert(text.id) {continue}
            let view = self.views.entry(text.id).or_insert_with(|| {
                TextView::new(&self.parent, text, self.sender.clone(), self.sequence.clone())
            });
            view.ivars().applying.set(true);
            let container = view.superview().unwrap().downcast::<ContainerView>().unwrap();
            let (x, y, width, height) = instruction.bound;
            let size = text.size();
            let text_width = text.width.unwrap_or(size.0).max(1.0);
            let text_height = size.1.max(1.0);
            // Selected newlines must fit compact labels without changing native text wrapping.
            view.ivars().selection_width.set(if text.cursor.is_some() {
                text_width as f64
            } else {size.0 as f64});
            let left = x.max(instruction.offset.0);
            let top = y.max(instruction.offset.1);
            let width = (x + width).min(instruction.offset.0 + text_width) - left;
            let height = (y + height).min(instruction.offset.1 + text_height) - top;
            let bounds = self.parent.bounds();
            // Only actual clipping edges constrain selection decorations; leaf edges do not.
            let clip_left = if x <= instruction.offset.0 {bounds.min().x} else {left as f64};
            let clip_top = if y <= instruction.offset.1 {bounds.min().y} else {top as f64};
            let clip_right = if x + instruction.bound.2 >= instruction.offset.0 + size.0 {
                bounds.max().x
            } else {(left + width) as f64};
            let clip_bottom = if y + instruction.bound.3 >= instruction.offset.1 + size.1 {
                bounds.max().y
            } else {(top + height) as f64};
            container.setFrame(CGRect::new(
                CGPoint::new(clip_left, clip_top),
                CGSize::new((clip_right - clip_left).max(0.0), (clip_bottom - clip_top).max(0.0)),
            ));
            container.ivars().set(CGRect::new(
                CGPoint::new(left as f64 - clip_left, top as f64 - clip_top),
                CGSize::new(width.max(0.0) as f64, height.max(0.0) as f64),
            ));
            container.setHidden(width <= 0.0 || height <= 0.0);
            view.setFrame(CGRect::new(
                CGPoint::new(
                    instruction.offset.0 as f64 - clip_left, instruction.offset.1 as f64 - clip_top,
                ),
                CGSize::new(text_width as f64, text_height as f64),
            ));
            view.synchronize(text, &mut self.fonts, revision);
        }
        self.views.retain(|id, view| {
            if seen.contains(id) {return true}
            if view.ivars().snapshot.borrow().as_ref().is_some_and(|text| text.focused)
                && focused.is_none() && view.isFirstResponder() {
                view.superview().unwrap().setHidden(true);
                return true
            }
            view.detach();
            false
        });
    }
}
impl Drop for NativeText {
    fn drop(&mut self) {
        for view in self.views.values() {view.detach();}
    }
}

define_class!(
    #[unsafe(super(UITextSelectionRect))]
    #[thread_kind = MainThreadOnly]
    #[ivars = (Retained<UITextSelectionRect>, CGRect)]
    struct SelectionRect;

    unsafe impl NSObjectProtocol for SelectionRect {}

    impl SelectionRect {
        #[unsafe(method(rect))]
        fn rect(&self) -> CGRect {self.ivars().1}

        #[unsafe(method(writingDirection))]
        fn writing_direction(&self) -> NSWritingDirection {self.ivars().0.writingDirection()}

        #[unsafe(method(containsStart))]
        fn contains_start(&self) -> bool {self.ivars().0.containsStart()}

        #[unsafe(method(containsEnd))]
        fn contains_end(&self) -> bool {self.ivars().0.containsEnd()}

        #[unsafe(method(isVertical))]
        fn is_vertical(&self) -> bool {self.ivars().0.isVertical()}

        #[unsafe(method(transform))]
        fn transform(&self) -> CGAffineTransform {self.ivars().0.transform()}
    }
);

impl SelectionRect {
    fn new(rectangle: Retained<UITextSelectionRect>, width: f64) -> Retained<Self> {
        let mut rect = rectangle.rect();
        let right = rect.max().x.clamp(0.0, width);
        rect.origin.x = rect.origin.x.clamp(0.0, width);
        rect.size.width = (right - rect.origin.x).max(0.0);
        let rectangle = Self::alloc(rectangle.mtm()).set_ivars((rectangle, rect));
        unsafe {msg_send![super(rectangle), init]}
    }
}

struct TextState {
    id: Uuid,
    sequence: Rc<Cell<u64>>,
    revision: Cell<u64>,
    snapshot: RefCell<Option<Text>>,
    selection_width: Cell<f64>,
    sender: UnboundedSender<Action>,
    active: Cell<bool>,
    applying: Cell<bool>,
    composing: Cell<bool>,
    native_action: Cell<bool>,
    touch: Cell<Option<(f32, f32)>>,
}

define_class!(
    #[unsafe(super(UITextView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TextState]
    struct TextView;

    unsafe impl NSObjectProtocol for TextView {}
    unsafe impl UIScrollViewDelegate for TextView {}
    unsafe impl UIGestureRecognizerDelegate for TextView {
        #[unsafe(method(gestureRecognizer:shouldReceiveTouch:))]
        fn receive_blank_touch(&self, _gesture: &UIGestureRecognizer, touch: &UITouch) -> bool {
            let point = touch.locationInView(Some(self));
            self.ready() && !self.isEditable()
                && !self.selection_handle(point) && self.blank_line(point).is_some()
        }
    }
    unsafe impl UITextViewDelegate for TextView {
        #[unsafe(method(textViewDidChange:))]
        fn changed(&self, _view: &UITextView) {self.send_text();}

        #[unsafe(method(textViewDidChangeSelection:))]
        fn selection_changed(&self, _view: &UITextView) {
            if self.ready() && self.markedTextRange().is_none() {self.send_selection();}
        }

        #[unsafe(method(textViewDidBeginEditing:))]
        fn began_editing(&self, _view: &UITextView) {
            if self.ready() {self.send_selection();}
        }
    }

    impl TextView {
        #[unsafe(method(buildMenuWithBuilder:))]
        fn build_menu(&self, builder: &ProtocolObject<dyn UIMenuBuilder>) {
            let _: () = unsafe {msg_send![super(self), buildMenuWithBuilder: builder]};
            if !self.isEditable() && objc2::available!(ios = 17.0) {
                unsafe {builder.removeMenuForIdentifier(UIMenuAutoFill);}
            }
        }

        #[unsafe(method(blankLinePress:))]
        fn blank_press(&self, gesture: &UILongPressGestureRecognizer) {
            if !self.ready() || self.isEditable() {return}
            let state = gesture.state();
            if !matches!(state, UIGestureRecognizerState::Began | UIGestureRecognizerState::Ended) {
                return
            }
            let point = gesture.locationInView(Some(self));
            let Some(range) = self.blank_line(point) else {return};
            self.ivars().touch.set(None);
            self.becomeFirstResponder();
            self.select(range);
            if state != UIGestureRecognizerState::Ended {return}
            // UIKit's text-owned menu only accepts its internal configurations.
            let menu = self.interactions().into_iter().find_map(|interaction| {
                interaction.downcast::<UIEditMenuInteraction>().ok()
                    .filter(|menu| menu.delegate().is_none())
            }).unwrap_or_else(|| {
                let menu = UIEditMenuInteraction::initWithDelegate(
                    UIEditMenuInteraction::alloc(self.mtm()), None,
                );
                self.addInteraction(ProtocolObject::from_ref(&*menu));
                menu
            });
            let configuration = unsafe {
                UIEditMenuConfiguration::configurationWithIdentifier_sourcePoint(
                    None, point, self.mtm(),
                )
            };
            menu.presentEditMenuWithConfiguration(&configuration);
        }

        #[unsafe(method_id(selectionRectsForRange:))]
        fn selection_rects(&self, range: &UITextRange) -> Retained<NSArray<UITextSelectionRect>> {
            let rectangles: Retained<NSArray<UITextSelectionRect>> = unsafe {
                msg_send![super(self), selectionRectsForRange: range]
            };
            rectangles.into_iter().map(|rectangle| {
                SelectionRect::new(rectangle, self.ivars().selection_width.get()).into_super()
            }).collect()
        }

        #[unsafe(method(autocorrectionType))]
        fn autocorrection_type(&self) -> UITextAutocorrectionType {UITextAutocorrectionType::Yes}

        #[unsafe(method(spellCheckingType))]
        fn spell_checking_type(&self) -> UITextSpellCheckingType {UITextSpellCheckingType::Yes}

        #[unsafe(method(insertText:))]
        fn insert(&self, text: &NSString) {
            if self.ivars().native_action.get() || self.ivars().applying.get() {
                let _: () = unsafe {msg_send![super(self), insertText: text]};
                return
            }
            let composing = self.ivars().composing.get() || self.markedTextRange().is_some();
            let command = match text.to_string().as_str() {
                "\n" | "\r" | "\r\n" => Some(Key::Enter), "\t" => Some(Key::Tab), _ => None,
            };
            if !composing && let Some(command) = command {
                self.send_selection();
                self.send_key(command);
                return
            }
            self.ivars().applying.set(true);
            let _: () = unsafe {msg_send![super(self), insertText: text]};
            self.ivars().applying.set(false);
            self.ivars().composing.set(self.markedTextRange().is_some());
            self.send_text();
        }

        #[unsafe(method(deleteBackward))]
        fn delete(&self) {
            if self.ivars().native_action.get() || self.ivars().applying.get() {
                let _: () = unsafe {msg_send![super(self), deleteBackward]};
                return
            }
            self.ivars().applying.set(true);
            let _: () = unsafe {msg_send![super(self), deleteBackward]};
            self.ivars().applying.set(false);
            self.ivars().composing.set(self.markedTextRange().is_some());
            self.send_text();
        }

        #[unsafe(method(setMarkedText:selectedRange:))]
        fn mark(&self, text: Option<&NSString>, range: NSRange) {
            self.ivars().composing.set(true);
            let applying = self.ivars().applying.replace(true);
            let _: () = unsafe {msg_send![super(self), setMarkedText: text, selectedRange: range]};
            self.ivars().applying.set(applying);
            self.send_text();
        }

        #[unsafe(method(unmarkText))]
        fn unmark(&self) {
            let applying = self.ivars().applying.replace(true);
            let _: () = unsafe {msg_send![super(self), unmarkText]};
            self.ivars().applying.set(applying);
            if self.ivars().composing.replace(false) && self.ready() {self.send_text();}
        }

        #[unsafe(method(cut:))]
        fn cut(&self, sender: Option<&AnyObject>) {
            self.ivars().native_action.set(true);
            self.ivars().applying.set(true);
            let _: () = unsafe {msg_send![super(self), cut: sender]};
            self.ivars().applying.set(false);
            self.ivars().native_action.set(false);
            self.send_text();
        }

        #[unsafe(method(paste:))]
        fn paste(&self, sender: Option<&AnyObject>) {
            self.ivars().native_action.set(true);
            self.ivars().applying.set(true);
            let _: () = unsafe {msg_send![super(self), paste: sender]};
            self.ivars().applying.set(false);
            self.ivars().native_action.set(false);
            self.send_text();
        }

        #[unsafe(method(touchesBegan:withEvent:))]
        fn touches_began(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            let point = touches.anyObject().map(|touch| touch.locationInView(Some(self)));
            let handle = point.is_some_and(|point| self.selection_handle(point));
            self.ivars().touch.set(None);
            if !handle && let Some(position) = self.position(touches) {
                self.ivars().touch.set(Some(position));
                self.send_mouse(position, MouseState::Pressed(MouseButton::Left));
                self.send_mouse(position, MouseState::Released(MouseButton::Left));
                if !self.isEditable() && let Some(point) = point
                    && let Some(range) = self.blank_line(point) {
                    self.becomeFirstResponder();
                    self.select(range);
                }
            }
            let _: () = unsafe {msg_send![super(self), touchesBegan: touches, withEvent: event]};
        }

        #[unsafe(method(touchesMoved:withEvent:))]
        fn touches_moved(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            if let Some(position) = self.position(touches)
                && let Some(previous) = self.ivars().touch.get() {
                self.ivars().touch.set(Some(position));
                self.send_mouse(position, MouseState::Scroll(
                    previous.0 - position.0, previous.1 - position.1,
                ));
            }
            let _: () = unsafe {msg_send![super(self), touchesMoved: touches, withEvent: event]};
        }

        #[unsafe(method(touchesEnded:withEvent:))]
        fn touches_ended(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            self.ivars().touch.set(None);
            let _: () = unsafe {msg_send![super(self), touchesEnded: touches, withEvent: event]};
        }

        #[unsafe(method(touchesCancelled:withEvent:))]
        fn touches_cancelled(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            self.ivars().touch.set(None);
            let _: () = unsafe {msg_send![super(self), touchesCancelled: touches, withEvent: event]};
        }
    }
);

impl TextView {
    fn new(parent: &UIView, text: &Text, sender: UnboundedSender<Action>, sequence: Rc<Cell<u64>>) -> Retained<Self> {
        let mtm = MainThreadMarker::new().unwrap();
        let container = ContainerView::new(mtm);
        container.setClipsToBounds(true);
        container.setOpaque(false);
        parent.addSubview(&container);
        let view = Self::alloc(mtm).set_ivars(TextState {
            id: text.id, sequence, revision: Cell::new(0), snapshot: RefCell::new(None),
            selection_width: Cell::new(0.0), sender,
            active: Cell::new(true), applying: Cell::new(false), composing: Cell::new(false),
            native_action: Cell::new(false), touch: Cell::new(None),
        });
        let view: Retained<Self> = unsafe {
            msg_send![super(view), initWithFrame: CGRect::ZERO, textContainer: None::<&NSTextContainer>]
        };
        view.setOpaque(false);
        view.setBackgroundColor(Some(&UIColor::clearColor()));
        view.setTextColor(Some(&UIColor::clearColor()));
        view.setScrollEnabled(false);
        view.setDelaysContentTouches(false);
        view.setShowsVerticalScrollIndicator(false);
        view.setShowsHorizontalScrollIndicator(false);
        view.setContentInsetAdjustmentBehavior(UIScrollViewContentInsetAdjustmentBehavior::Never);
        view.setTextContainerInset(UIEdgeInsets {top: 0.0, left: 0.0, bottom: 0.0, right: 0.0});
        view.textContainer().setLineFragmentPadding(0.0);
        view.setSelectable(true);
        view.setEditable(text.cursor.is_some());
        unsafe {view.setDelegate(Some(ProtocolObject::from_ref(&*view)));}
        container.addSubview(&view);
        // UIKit's word-selection press replaces blank-paragraph selections.
        let press = unsafe {
            UILongPressGestureRecognizer::initWithTarget_action(
                UILongPressGestureRecognizer::alloc(mtm), Some(&*view), Some(objc2::sel!(blankLinePress:)),
            )
        };
        press.setCancelsTouchesInView(false);
        press.setDelegate(Some(ProtocolObject::from_ref(&*view)));
        if let Some(gestures) = view.gestureRecognizers() {
            for gesture in gestures {
                if gesture.downcast_ref::<UILongPressGestureRecognizer>().is_some() {
                    gesture.requireGestureRecognizerToFail(&press);
                }
            }
        }
        for interaction in view.interactions() {
            if let Ok(interaction) = interaction.downcast::<UITextInteraction>() {
                for gesture in interaction.gesturesForFailureRequirements() {
                    gesture.requireGestureRecognizerToFail(&press);
                }
            }
        }
        view.addGestureRecognizer(&press);
        view
    }

    fn synchronize(&self, text: &Text, fonts: &mut HashMap<usize, CFRetained<CGFont>>, revision: u64) {
        self.ivars().applying.set(true);
        if self.ivars().revision.get() > revision {
            self.setContentOffset(CGPoint::ZERO);
            self.ivars().applying.set(false);
            return
        }
        // Editable mirrors still reconcile native edits that the app may have rejected.
        if text.cursor.is_none() && self.ivars().snapshot.borrow().as_ref().is_some_and(|previous| {
            previous == text && previous.focused == text.focused && previous.anchor == text.anchor
        }) {
            self.setContentOffset(CGPoint::ZERO);
            self.ivars().applying.set(false);
            return
        }
        let was_focused = self.ivars().snapshot.borrow().as_ref().is_some_and(|text| text.focused);
        self.setEditable(text.cursor.is_some());
        self.textContainer().setMaximumNumberOfLines(text.max_lines.unwrap_or(0) as usize);
        {
            let value: String = text.spans.iter().map(|span| span.text.as_str()).collect();
            let current = self.text().to_string();
            let previous = self.ivars().snapshot.borrow();
            let attributes_changed = previous.as_ref().is_none_or(|previous| {
                previous.spans != text.spans || previous.align != text.align
            });
            let style_changed = previous.as_ref().is_none_or(|previous| {
                previous.align != text.align || previous.spans.len() != text.spans.len()
                    || previous.spans.iter().zip(&text.spans).any(|(old, new)| {
                        old.font != new.font || old.font_size != new.font_size
                            || old.line_height != new.line_height || old.kerning != new.kerning
                    })
            });
            if current != value || attributes_changed {
                let attributed = NSMutableAttributedString::initWithString(
                    NSMutableAttributedString::alloc(), &NSString::from_str(&value),
                );
                let mut start = 0;
                let mut first_font = None;
                for span in &text.spans {
                    let font = fonts.entry(span.font.file_hash()).or_insert_with(|| {
                        let data = CFData::from_bytes(span.font.bytes());
                        let provider = CGDataProvider::with_cf_data(Some(&data)).unwrap();
                        CGFont::with_data_provider(&provider).unwrap()
                    });
                    let font = unsafe {
                        CTFont::with_graphics_font(font, span.font_size as f64, std::ptr::null(), None)
                    };
                    // CTFont and UIFont are toll-free bridged; UIKit retains each assigned font.
                    let font = unsafe {CFRetained::as_ptr(&font).cast::<UIFont>().as_ref()};
                    let paragraph = NSMutableParagraphStyle::new();
                    paragraph.setAlignment(match text.align {
                        Align::Left => NSTextAlignment::Left,
                        Align::Center => NSTextAlignment::Center,
                        Align::Right => NSTextAlignment::Right,
                    });
                    paragraph.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
                    let height = span.line_height.unwrap_or_else(|| {
                        span.font.horizontal_line_metrics(span.font_size).unwrap().new_line_size
                    });
                    paragraph.setMinimumLineHeight(height as f64);
                    paragraph.setMaximumLineHeight(height as f64);
                    let range = NSRange::new(start, span.text.encode_utf16().count());
                    unsafe {
                        attributed.addAttribute_value_range(NSFontAttributeName, font, range);
                        attributed.addAttribute_value_range(
                            NSForegroundColorAttributeName, &UIColor::clearColor(), range,
                        );
                        attributed.addAttribute_value_range(NSParagraphStyleAttributeName, &paragraph, range);
                        attributed.addAttribute_value_range(
                            NSKernAttributeName, &NSNumber::new_f32(span.kerning), range,
                        );
                    }
                    if first_font.is_none() {first_font = Some(font.retain());}
                    start += range.length;
                }
                if current != value {
                    if self.markedTextRange().is_some() {self.unmarkText();}
                    self.setFont(first_font.as_deref());
                    self.setAttributedText(Some(&attributed));
                    self.setTextColor(Some(&UIColor::clearColor()));
                    if let Some(undo) = self.undoManager() {undo.removeAllActions();}
                } else if attributed.length() == 0 && style_changed {
                    self.setFont(first_font.as_deref());
                } else {
                    let native = self.textStorage();
                    for key in unsafe {[
                        NSFontAttributeName, NSForegroundColorAttributeName,
                        NSParagraphStyleAttributeName, NSKernAttributeName,
                    ]} {
                        let mut index = 0;
                        while index < attributed.length() {
                            let mut actual_range = NSRange::new(0, 0);
                            let mut desired_range = NSRange::new(0, 0);
                            let actual = unsafe {
                                native.attribute_atIndex_effectiveRange(key, index, &mut actual_range)
                            };
                            let desired = unsafe {
                                attributed.attribute_atIndex_effectiveRange(key, index, &mut desired_range)
                            }.unwrap();
                            let end = (actual_range.location + actual_range.length)
                                .min(desired_range.location + desired_range.length);
                            let equal = actual.is_some_and(|actual| unsafe {
                                msg_send![&*actual, isEqual: &*desired]
                            });
                            if !equal {
                                unsafe {
                                    native.addAttribute_value_range(
                                        key, &desired, NSRange::new(index, end - index),
                                    );
                                }
                            }
                            index = end;
                        }
                    }
                }
            }
            if self.markedTextRange().is_none() && let Some(cursor) = text.cursor {
                let anchor = Self::utf16(&value, text.anchor.unwrap_or(cursor));
                let cursor = Self::utf16(&value, cursor);
                let desired = NSRange::new(cursor.min(anchor), cursor.abs_diff(anchor));
                let selection = self.selection();
                if selection != desired {self.select(desired);}
            }
        }
        *self.ivars().snapshot.borrow_mut() = Some(text.clone());
        if text.focused && !was_focused && !self.isFirstResponder() {self.becomeFirstResponder();}
        if !text.focused && self.isEditable() && self.isFirstResponder() {self.resignFirstResponder();}
        self.setContentOffset(CGPoint::ZERO);
        self.ivars().applying.set(false);
    }

    fn detach(&self) {
        self.ivars().active.set(false);
        self.resignFirstResponder();
        unsafe {self.setDelegate(None);}
        if let Some(container) = self.superview() {container.removeFromSuperview();}
        self.removeFromSuperview();
    }

    fn ready(&self) -> bool {self.ivars().active.get() && !self.ivars().applying.get()}

    fn selection_handle(&self, point: CGPoint) -> bool {
        if !objc2::available!(ios = 17.0) {return false}
        self.interactions().iter().any(|interaction| {
            let Ok(interaction) = interaction.downcast::<UITextSelectionDisplayInteraction>() else {
                return false
            };
            interaction.isActivated() && interaction.handleViews().iter().any(|handle| {
                ContainerView::contains_handle(&handle, self.convertPoint_toView(point, Some(&handle)))
            })
        })
    }

    fn blank_line(&self, point: CGPoint) -> Option<NSRange> {
        let caret = self.closestPositionToPoint(point)?;
        let offset = self.offsetFromPosition_toPosition(&self.beginningOfDocument(), &caret) as usize;
        let text = self.text().to_string();
        let byte = Self::utf8(&text, offset);
        if text.as_bytes().get(byte) == Some(&b'\n')
            && (byte == 0 || text[..byte].ends_with('\n')) {
            Some(NSRange::new(offset, 1))
        } else if byte == text.len() && text.ends_with('\n') {
            Some(NSRange::new(offset - 1, 1))
        } else {None}
    }

    fn send_text(&self) {
        if !self.ready() || !self.isEditable() {return}
        let text = self.text().to_string();
        let selection = self.selection();
        let range = Self::utf8(&text, selection.location)
            ..Self::utf8(&text, selection.location + selection.length);
        let _ = self.ivars().sender.unbounded_send(Action::SetText {
            edit: SetText {id: self.ivars().id, text, range}, revision: self.next_revision(),
        });
    }

    fn send_selection(&self) {
        if !self.ready() || !self.isEditable() || !self.isFirstResponder() {return}
        let text = self.text().to_string();
        let selection = self.selection();
        let range = Range {
            start: Self::utf8(&text, selection.location),
            end: Self::utf8(&text, selection.location + selection.length),
        };
        let _ = self.ivars().sender.unbounded_send(Action::Selection {
            id: self.ivars().id, text, range, revision: self.next_revision(),
        });
    }

    fn next_revision(&self) -> u64 {
        let revision = self.ivars().sequence.get() + 1;
        self.ivars().sequence.set(revision);
        self.ivars().revision.set(revision);
        revision
    }

    fn selection(&self) -> NSRange {
        let Some(range) = self.selectedTextRange() else {return NSRange::new(0, 0)};
        let beginning = self.beginningOfDocument();
        let start = self.offsetFromPosition_toPosition(&beginning, &range.start()) as usize;
        let end = self.offsetFromPosition_toPosition(&beginning, &range.end()) as usize;
        NSRange::new(start, end - start)
    }

    fn select(&self, range: NSRange) {
        let beginning = self.beginningOfDocument();
        let start = self.positionFromPosition_offset(&beginning, range.location as isize).unwrap();
        let end = self.positionFromPosition_offset(&start, range.length as isize).unwrap();
        let range = self.textRangeFromPosition_toPosition(&start, &end).unwrap();
        self.setSelectedTextRange(Some(&range));
    }

    fn send_key(&self, key: Key) {
        if !self.ready() {return}
        for state in [KeyboardState::Pressed, KeyboardState::Released] {
            let _ = self.ivars().sender.unbounded_send(Action::Keyboard(KeyboardEvent {
                key: key.clone(), state, modifiers: Modifiers::default(),
            }));
        }
    }

    fn position(&self, touches: &NSSet<UITouch>) -> Option<(f32, f32)> {
        let parent = self.superview()?.superview()?;
        let point = touches.anyObject()?.locationInView(Some(&parent));
        Some((point.x as f32, point.y as f32))
    }

    fn send_mouse(&self, position: (f32, f32), state: MouseState) {
        if !self.ready() {return}
        let _ = self.ivars().sender.unbounded_send(Action::Mouse(MouseEvent {
            position: Some(position), state,
        }));
    }

    fn utf8(text: &str, offset: usize) -> usize {
        let mut units = 0;
        for (index, character) in text.char_indices() {
            if units + character.len_utf16() > offset {return index}
            units += character.len_utf16();
        }
        text.len()
    }

    fn utf16(text: &str, offset: usize) -> usize {
        text.char_indices().take_while(|(index, _)| *index < offset)
            .map(|(_, character)| character.len_utf16()).sum()
    }
}

define_class!(
    #[unsafe(super(UIView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Cell<CGRect>]
    struct ContainerView;

    unsafe impl NSObjectProtocol for ContainerView {}

    impl ContainerView {
        #[unsafe(method_id(hitTest:withEvent:))]
        fn hit_test(&self, point: CGPoint, event: Option<&UIEvent>) -> Option<Retained<UIView>> {
            if self.isHidden() || !CGRectContainsPoint(self.bounds(), point) {None} else {
                self.subviews().firstObject().and_then(|view| {
                    let view = view.downcast::<TextView>().unwrap();
                    let local = self.convertPoint_toView(point, Some(&view));
                    if view.selection_handle(local) {
                        let view: &UIView = &view;
                        Some(view.retain())
                    } else if CGRectContainsPoint(self.ivars().get(), point) {
                        view.hitTest_withEvent(local, event)
                    } else {None}
                })
            }
        }
    }
);

impl ContainerView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let view = Self::alloc(mtm).set_ivars(Cell::new(CGRect::ZERO));
        unsafe {msg_send![super(view), init]}
    }

    fn contains_handle(view: &UIView, point: CGPoint) -> bool {
        if view.isHidden() || view.alpha() <= 0.01 {return false}
        CGRectContainsPoint(view.bounds(), point) || view.subviews().iter().any(|child| {
            Self::contains_handle(&child, view.convertPoint_toView(point, Some(&child)))
        })
    }
}
