use futures::channel::mpsc::UnboundedSender;
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM, NativeMethod};
use prism::drawable::{Instruction, Item};
use prism::events::{Key, KeyboardEvent, KeyboardState, Modifiers, MouseButton, MouseEvent, MouseState};
use serde_json::json;
use uuid::Uuid;

use std::collections::{HashMap, HashSet};

use super::{Action, SetText};

pub struct NativeText {
    window: maverick_os::Window,
    vm: JavaVM,
    class: GlobalRef,
    activity: GlobalRef,
    sender: UnboundedSender<Action>,
    views: HashMap<Uuid, (GlobalRef, String, bool)>,
    fonts: HashMap<usize, GlobalRef>,
}
impl NativeText {
    pub fn new(window: &maverick_os::Window, sender: UnboundedSender<Action>, revision: u64) -> Self {
        let context = ndk_context::android_context();
        let vm = unsafe {JavaVM::from_raw(context.vm().cast()).unwrap()};
        let (class, activity) = {
            let mut env = vm.attach_current_thread().unwrap();
            env.with_local_frame(16, |env| -> jni::errors::Result<_> {
                let application = unsafe {JObject::from_raw(context.context().cast())};
                let loader = env.call_method(application, "getClassLoader",
                    "()Ljava/lang/ClassLoader;", &[])?.l()?;
                let name = env.new_string("me.ramp.maverick.MaverickActivity")?;
                let activity = env.call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(&name)])?.l()?;
                let activity = env.call_static_method(JClass::from(activity), "current",
                    "()Lme/ramp/maverick/MaverickActivity;", &[])?.l()?;
                env.call_method(&activity, "initializeText", "(J)V", &[JValue::Long(revision as i64)])?;
                let name = env.new_string("me.ramp.maverick.NativeText")?;
                let class = JClass::from(env.call_method(loader, "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&name)])?.l()?);
                env.register_native_methods(&class, &[
                    NativeMethod {
                        name: "selectionChanged".into(),
                        sig: "(Ljava/lang/String;Ljava/lang/String;IIJ)V".into(),
                        fn_ptr: Self::selection as *mut std::ffi::c_void,
                    },
                    NativeMethod {
                        name: "textReplaced".into(),
                        sig: "(Ljava/lang/String;Ljava/lang/String;IIJ)V".into(),
                        fn_ptr: Self::replace as *mut std::ffi::c_void,
                    },
                    NativeMethod {
                        name: "command".into(), sig: "(II)V".into(),
                        fn_ptr: Self::command as *mut std::ffi::c_void,
                    },
                    NativeMethod {
                        name: "pointer".into(), sig: "(FFIFF)V".into(),
                        fn_ptr: Self::pointer as *mut std::ffi::c_void,
                    },
                ])?;
                Ok((env.new_global_ref(class)?, env.new_global_ref(activity)?))
            }).unwrap()
        };
        Self {
            window: window.clone(), vm, class, activity, sender,
            views: HashMap::new(), fonts: HashMap::new(),
        }
    }

    pub fn update(&mut self, instructions: &[Instruction], revision: u64) {
        let scale = self.window.physical().scale_factor as f32;
        let mut seen = HashSet::new();
        let mut retained = HashSet::new();
        let mut env = self.vm.attach_current_thread().unwrap();
        for instruction in instructions {
            let Item::Text(text) = &instruction.item else {continue};
            if !seen.insert(text.id) || text.spans.is_empty() {continue}
            let size = text.size();
            let width = text.width.unwrap_or(size.0);
            let height = size.1.max(text.spans[0].line_height.unwrap_or(text.spans[0].font_size));
            let (x, y) = instruction.offset;
            let (left, top, bound_width, bound_height) = instruction.bound;
            let right = (left + bound_width).min(x + width);
            let bottom = (top + bound_height).min(y + height);
            let left = left.max(x);
            let top = top.max(y);
            let visible = right > left && bottom > top;
            if !visible && !text.focused {continue}
            retained.insert(text.id);
            let value: String = text.spans.iter().map(|span| span.text.as_str()).collect();
            let cursor = text.cursor.map(|cursor| Self::utf16(&value, cursor));
            let anchor = text.anchor.map(|anchor| Self::utf16(&value, anchor));
            let editable = cursor.is_some()
                || self.views.get(&text.id).is_some_and(|(_, _, editable)| *editable);
            let mut end = 0;
            let spans: Vec<_> = text.spans.iter().map(|span| {
                let start = end;
                end += span.text.encode_utf16().count();
                json!({
                    "start": start, "end": end, "size": span.font_size * scale,
                    "height": span.line_height.unwrap_or_else(|| {
                        span.font.horizontal_line_metrics(span.font_size).unwrap().new_line_size
                    }) * scale,
                    "kerning": span.kerning * scale, "font": span.font.file_hash().to_string(),
                })
            }).collect();
            let snapshot = json!({
                "text": value, "cursor": cursor, "anchor": anchor,
                "focused": text.focused, "visible": visible,
                "revision": if editable {revision} else {0},
                "x": x * scale, "y": y * scale, "width": width * scale,
                "height": height * scale, "left": left * scale, "top": top * scale,
                "right": right * scale, "bottom": bottom * scale, "scale": scale,
                "align": match text.align {
                    prism::drawable::text::Align::Left => 0,
                    prism::drawable::text::Align::Center => 1,
                    prism::drawable::text::Align::Right => 2,
                },
                "spans": spans,
            }).to_string();
            if text.cursor.is_none()
                && self.views.get(&text.id).is_some_and(|(_, previous, _)| *previous == snapshot)
            {continue}
            env.with_local_frame(32, |env| -> jni::errors::Result<()> {
                let class = <&JClass>::from(self.class.as_obj());
                let mut typefaces = Vec::new();
                for span in &text.spans {
                    let key = span.font.file_hash();
                    if let std::collections::hash_map::Entry::Vacant(entry) = self.fonts.entry(key) {
                        let bytes = env.byte_array_from_slice(span.font.bytes())?;
                        let font = env.call_static_method(class, "font", "([B)Landroid/graphics/Typeface;",
                            &[JValue::Object(&bytes)])?.l()?;
                        entry.insert(env.new_global_ref(font)?);
                    }
                    typefaces.push(self.fonts[&key].clone());
                }
                if let std::collections::hash_map::Entry::Vacant(entry) = self.views.entry(text.id) {
                    let id = env.new_string(text.id.to_string())?;
                    let view = env.call_static_method(class, "create",
                        "(Lme/ramp/maverick/MaverickActivity;Ljava/lang/String;)Lme/ramp/maverick/NativeText;",
                        &[JValue::Object(self.activity.as_obj()), JValue::Object(&id)])?.l()?;
                    let view = env.new_global_ref(view)?;
                    unsafe {env.set_rust_field(view.as_obj(), "events", self.sender.clone())?;}
                    entry.insert((view, String::new(), editable));
                }
                let typeface_class = env.find_class("android/graphics/Typeface")?;
                let fonts = env.new_object_array(typefaces.len() as i32, typeface_class, JObject::null())?;
                for (index, font) in typefaces.iter().enumerate() {
                    env.set_object_array_element(&fonts, index as i32, font.as_obj())?;
                }
                let (view, previous, was_editable) = self.views.get_mut(&text.id).unwrap();
                let data = env.new_string(&snapshot)?;
                env.call_method(view.as_obj(), "update", "(Ljava/lang/String;[Landroid/graphics/Typeface;)V",
                    &[JValue::Object(&data), JValue::Object(&fonts)])?;
                *previous = snapshot;
                *was_editable = editable;
                Ok(())
            }).unwrap();
        }
        self.views.retain(|id, (view, _, _)| {
            if retained.contains(id) {return true}
            Self::close(&mut env, view);
            false
        });
    }

    fn close(env: &mut JNIEnv, view: &GlobalRef) {
        let _monitor = env.lock_obj(view.as_obj()).unwrap();
        env.call_method(view.as_obj(), "close", "()V", &[]).unwrap();
        unsafe {env.take_rust_field::<_, _, UnboundedSender<Action>>(view.as_obj(), "events").unwrap();}
    }

    fn utf8(text: &str, offset: i32) -> usize {
        let mut units = 0;
        for (byte, character) in text.char_indices() {
            units += character.len_utf16();
            if units > offset as usize {return byte}
        }
        text.len()
    }

    fn utf16(text: &str, offset: usize) -> usize {
        let mut offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {offset -= 1;}
        text[..offset].encode_utf16().count()
    }

    extern "system" fn selection(mut env: JNIEnv, view: JObject, id: JString, text: JString, start: i32, end: i32, revision: i64) {
        let id = String::from(env.get_string(&id).unwrap()).parse().unwrap();
        let text = String::from(env.get_string(&text).unwrap());
        let range = Self::utf8(&text, start)..Self::utf8(&text, end);
        let sender = unsafe {
            env.get_rust_field::<_, _, UnboundedSender<Action>>(view, "events").unwrap().clone()
        };
        let _ = sender.unbounded_send(Action::Selection {id, text, range, revision: revision as u64});
    }

    extern "system" fn replace(mut env: JNIEnv, view: JObject, id: JString, text: JString, start: i32, end: i32, revision: i64) {
        let id = String::from(env.get_string(&id).unwrap()).parse().unwrap();
        let text = String::from(env.get_string(&text).unwrap());
        let range = Self::utf8(&text, start)..Self::utf8(&text, end);
        let sender = unsafe {
            env.get_rust_field::<_, _, UnboundedSender<Action>>(view, "events").unwrap().clone()
        };
        let _ = sender.unbounded_send(Action::SetText {
            edit: SetText {id, text, range}, revision: revision as u64,
        });
    }

    extern "system" fn command(mut env: JNIEnv, view: JObject, code: i32, meta: i32) {
        let sender = unsafe {
            env.get_rust_field::<_, _, UnboundedSender<Action>>(view, "events").unwrap().clone()
        };
        let key = match code {66 => Key::Enter, 61 => Key::Tab, _ => unreachable!()};
        for state in [KeyboardState::Pressed, KeyboardState::Released] {
            let _ = sender.unbounded_send(Action::Keyboard(KeyboardEvent {
                key: key.clone(), state, modifiers: Modifiers {
                    shift: meta & 1 != 0, control: meta & 0x1000 != 0,
                    alt: meta & 2 != 0, supermeta: meta & 0x10000 != 0,
                },
            }));
        }
    }

    extern "system" fn pointer(mut env: JNIEnv, view: JObject, x: f32, y: f32, state: i32, dx: f32, dy: f32) {
        let sender = unsafe {
            env.get_rust_field::<_, _, UnboundedSender<Action>>(view, "events").unwrap().clone()
        };
        let state = match state {
            0 => MouseState::Pressed(MouseButton::Left),
            1 => MouseState::Released(MouseButton::Left),
            3 => MouseState::Scroll(dx, dy),
            _ => unreachable!(),
        };
        let _ = sender.unbounded_send(Action::Mouse(MouseEvent {position: Some((x, y)), state}));
    }
}
impl Drop for NativeText {
    fn drop(&mut self) {
        let mut env = self.vm.attach_current_thread().unwrap();
        for (view, _, _) in self.views.values() {Self::close(&mut env, view);}
    }
}
