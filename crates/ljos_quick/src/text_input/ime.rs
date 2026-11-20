//! The editors and the platform's input method (ARCHITECTURE §11).
//!
//! Two directions. Inwards, the input method composes and commits text:
//! the host hands that to [`QuickScene::ime_preedit`] and
//! [`QuickScene::ime_commit`], which reach the editor with active focus
//! through its [`TextInputClient`]. Outwards, the platform must know
//! whether an editor has focus and where its cursor is, to open a keyboard
//! or place a candidate window: [`ImeState::sync`] tells a
//! [`PlatformTextInput`] what changed since it was last told.
//!
//! Nothing here knows a toolkit: the desktop host implements
//! [`PlatformTextInput`] over its windowing layer, a phone over its
//! keyboard service.

use std::ops::Range;

use ljos_engine::{Engine, ObjectId};
use ljos_render::kurbo;

use super::base::{EditKind, TextEditor, check, publish, state_of};
use super::echo_mode;
use super::input_method_hint as hint;
use super::layout::position_rect;
use super::ops::{self, How};
use crate::item::scene_transform;
use crate::keys::{Surrounding, TextInputClient};
use crate::scene::QuickScene;

/// The client of every `TextInput` and `TextEdit`.
pub(super) struct Client;

fn int(engine: &Engine, obj: ObjectId, p: ljos_engine::PropIndex) -> i32 {
    engine.untrack(|| engine.read(obj, p)).to_number() as i32
}

impl TextInputClient for Client {
    fn surrounding(&self, engine: &Engine, item: ObjectId) -> Surrounding {
        let Some(st) = ops::ready(engine, item) else {
            return Surrounding::default();
        };
        // An input method learns from what it is shown and may keep it: a
        // field that hides its text shows it nothing.
        if ops::echo(engine, item, &st) != echo_mode::NORMAL {
            return Surrounding::default();
        }
        let s = st.borrow();
        Surrounding {
            text: s.model.text().to_owned(),
            anchor: s.model.anchor(),
            cursor: s.model.cursor(),
        }
    }

    fn replace(&self, engine: &Engine, item: ObjectId, range: Range<usize>, text: &str) {
        if ops::read_only(engine, item) {
            return;
        }
        ops::begin_editing(engine, item);
        let mask = ops::ready(engine, item).and_then(|st| super::mask::of(&st));
        ops::edit(engine, item, How::user(EditKind::Other), |m| {
            let len = m.len();
            let range = range.start.min(len)..range.end.min(len);
            match mask {
                Some(mask) => mask.replace(m, range, text),
                None => m.replace(range, text),
            }
        });
    }

    fn delete_before_cursor(&self, engine: &Engine, item: ObjectId, units: usize) {
        let Some(cursor) = ops::ready(engine, item).map(|st| st.borrow().model.cursor()) else {
            return;
        };
        self.replace(engine, item, cursor.saturating_sub(units)..cursor, "");
    }

    fn digest(&self, engine: &Engine, item: ObjectId) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        if let Some(st) = ops::ready(engine, item) {
            let s = st.borrow();
            (s.model.text(), s.model.anchor(), s.model.cursor()).hash(&mut h);
        }
        h.finish()
    }

    fn set_selection(&self, engine: &Engine, item: ObjectId, anchor: usize, cursor: usize) {
        ops::act(engine, item, |st| {
            let mut s = st.borrow_mut();
            let len = s.model.len();
            s.model.select(anchor.min(len), cursor.min(len));
        });
    }

    fn cursor_rect(&self, engine: &Engine, item: ObjectId) -> kurbo::Rect {
        let Some(st) = ops::ready(engine, item) else {
            return kurbo::Rect::ZERO;
        };
        let r = st.borrow().cursor_rect;
        kurbo::Rect::new(r.x, r.y, r.x + r.width, r.y + r.height)
    }

    fn anchor_rect(&self, engine: &Engine, item: ObjectId) -> kurbo::Rect {
        let cursor = self.cursor_rect(engine, item);
        let Some(st) = ops::ready(engine, item) else {
            return cursor;
        };
        let (at_anchor, at_cursor) = {
            let s = st.borrow();
            (s.model.anchor() as i32, s.model.cursor() as i32)
        };
        // `position_rect` measures from elsewhere than the cursor rectangle
        // does for some editors; the way from one position to the other is
        // the same.
        let (a, c) = (position_rect(&st, at_anchor), position_rect(&st, at_cursor));
        cursor + kurbo::Vec2::new(a.x - c.x, a.y - c.y)
    }

    fn hints(&self, engine: &Engine, item: ObjectId) -> i32 {
        let mut hints = int(engine, item, TextEditor::INPUT_METHOD_HINTS.index);
        let Some(st) = state_of(engine, item) else {
            return hints;
        };
        if st.borrow().multi {
            hints |= hint::MULTI_LINE;
        } else if ops::echo(engine, item, &st) != echo_mode::NORMAL {
            // What is typed into a field that hides its text is not to be
            // learnt, completed or capitalised.
            hints |= hint::HIDDEN_TEXT
                | hint::SENSITIVE_DATA
                | hint::NO_PREDICTIVE_TEXT
                | hint::NO_AUTO_UPPERCASE;
        }
        hints
    }

    fn enabled(&self, engine: &Engine, item: ObjectId) -> bool {
        !ops::read_only(engine, item)
    }

    fn set_preedit(
        &self,
        engine: &Engine,
        item: ObjectId,
        text: &str,
        cursor: Option<usize>,
    ) -> bool {
        if ops::read_only(engine, item) {
            return false;
        }
        let Some(st) = ops::ready(engine, item) else {
            return false;
        };
        if text.is_empty() && st.borrow().preedit.is_empty() {
            return true;
        }
        if !text.is_empty() {
            // Composing is editing: a text shown masked until then starts
            // over.
            ops::begin_editing(engine, item);
        }
        {
            let mut s = st.borrow_mut();
            text.clone_into(&mut s.preedit);
            s.preedit_cursor = cursor.filter(|_| !text.is_empty());
            s.sticky = None;
        }
        check(engine, "preedit", publish(engine, item, &st));
        true
    }

    fn preedit(&self, engine: &Engine, item: ObjectId) -> String {
        ops::ready(engine, item).map_or_else(String::new, |st| st.borrow().preedit.clone())
    }

    fn commit(&self, engine: &Engine, item: ObjectId, text: &str) -> bool {
        if ops::read_only(engine, item) {
            return false;
        }
        let Some(st) = ops::ready(engine, item) else {
            return false;
        };
        let composing = {
            let mut s = st.borrow_mut();
            s.preedit_cursor = None;
            !std::mem::take(&mut s.preedit).is_empty()
        };
        // Typed as the user types: a validator may refuse it and
        // `maximumLength` cuts it. That also shows the text without the
        // composition.
        if !ops::accept_text(engine, item, text) && composing {
            check(engine, "commit", publish(engine, item, &st));
        }
        true
    }
}

impl QuickScene {
    /// An input method shows `text` as what it is composing, in place of
    /// what it showed before; an empty `text` ends the composition
    /// without typing anything. `cursor` is its own cursor in `text`, in
    /// UTF-16 units. Returns whether an editor took it: `false` when no
    /// editor has active focus or it is read-only.
    pub fn ime_preedit(&self, engine: &Engine, text: &str, cursor: Option<usize>) -> bool {
        self.note_input();
        let _acting = crate::input::Delivering::begin(engine);
        self.text_input_target(engine)
            .is_some_and(|(item, client)| client.set_preedit(engine, item, text, cursor))
    }

    /// An input method types `text`, ending its composition. Returns
    /// whether an editor took it.
    pub fn ime_commit(&self, engine: &Engine, text: &str) -> bool {
        self.note_input();
        let _acting = crate::input::Delivering::begin(engine);
        self.text_input_target(engine)
            .is_some_and(|(item, client)| client.commit(engine, item, text))
    }

    /// What the editor with active focus holds, as an input method that
    /// is told the whole of it sees it: the composition in the text at
    /// the cursor. `None` without an editor that takes text.
    pub fn ime_text(&self, engine: &Engine) -> Option<ImeText> {
        let (item, client) = self.text_input_target(engine)?;
        if !client.enabled(engine, item) {
            return None;
        }
        let around = client.surrounding(engine, item);
        let preedit = client.preedit(engine, item);
        if preedit.is_empty() && hides(engine, item, &*client) {
            // What the method typed, which it holds still.
            let text = typed_hidden(engine, item, &*client);
            let end = units_of(&text);
            return Some(ImeText {
                text,
                anchor: end,
                cursor: end,
                compose: None,
            });
        }
        if preedit.is_empty() {
            return Some(ImeText {
                text: around.text,
                anchor: around.anchor,
                cursor: around.cursor,
                compose: None,
            });
        }
        let at = byte_at(&around.text, around.cursor);
        let mut text = around.text[..at].to_owned();
        text.push_str(&preedit);
        text.push_str(&around.text[at..]);
        let end = around.cursor + units_of(&preedit);
        Some(ImeText {
            text,
            anchor: end,
            cursor: end,
            compose: Some(around.cursor..end),
        })
    }

    /// An input method that tells the whole of what it holds now: the
    /// editor with active focus takes what changed. Text that changed is
    /// typed, the composing region becomes the composition (a word
    /// already typed and composed again leaves the text while it is), and
    /// the method's selection becomes the editor's. Returns whether an
    /// editor took it.
    ///
    /// A field that hides its text shows the method nothing
    /// ([`TextInputClient::surrounding`]), so the method only knows what
    /// it typed there: that is taken to be just before the cursor, and
    /// what changed of it is typed or taken back there. It is kept for as
    /// long as the field is as the method left it ([`QuickScene::
    /// ime_text`] tells it), so the method is not told to forget it after
    /// every key, which it would do only after it typed the next.
    pub fn ime_set_text(&self, engine: &Engine, now: &ImeText) -> bool {
        self.note_input();
        let _acting = crate::input::Delivering::begin(engine);
        let Some((item, client)) = self.text_input_target(engine) else {
            return false;
        };
        if !client.enabled(engine, item) {
            return false;
        }
        let hidden = hides(engine, item, &*client);
        let around = client.surrounding(engine, item);
        let composing = !client.preedit(engine, item).is_empty();
        let (text, compose) = now.split();
        let (range, typed) = changed(&around.text, &text);
        let edits = !range.is_empty() || !typed.is_empty();
        if composing && (edits || compose.is_none()) {
            // The method ended or moved its composition: what it composed
            // is in `text` if it was kept.
            client.set_preedit(engine, item, "", None);
        }
        if hidden {
            let before = typed_hidden(engine, item, &*client);
            let (range, typed) = changed(&before, &text);
            let digest = client.digest(engine, item);
            let back = units_of(&before) - range.start;
            if back > 0 {
                client.delete_before_cursor(engine, item, back);
            }
            // What followed the change in what it held was taken back
            // with it.
            let again = typed + &before[byte_at(&before, range.end)..];
            if !again.is_empty() {
                client.commit(engine, item, &again);
            }
            let after = client.digest(engine, item);
            // A field that refused what was typed is not as the method
            // holds it: the method is told to start anew.
            let took = after != digest || (back == 0 && again.is_empty());
            *hidden_held(engine).0.borrow_mut() = took.then(|| HiddenHeld {
                item,
                text: text.clone(),
                digest: after,
            });
        } else if edits {
            let at_cursor =
                around.anchor == around.cursor && range == (around.cursor..around.cursor);
            if at_cursor {
                // Typed as the user types: a validator and `maximumLength`
                // have their say.
                client.commit(engine, item, &typed);
            } else {
                client.replace(engine, item, range, &typed);
            }
        }
        match compose {
            Some((at, composed)) => {
                let after = client.surrounding(engine, item);
                if !hidden && (after.anchor, after.cursor) != (at, at) {
                    client.set_selection(engine, item, at, at);
                }
                let cursor = now.cursor.clamp(at, at + units_of(&composed)) - at;
                client.set_preedit(engine, item, &composed, Some(cursor));
            }
            None if !hidden => {
                let len = units_of(&text);
                let (anchor, cursor) = (now.anchor.min(len), now.cursor.min(len));
                let after = client.surrounding(engine, item);
                if (after.anchor, after.cursor) != (anchor, cursor) {
                    client.set_selection(engine, item, anchor, cursor);
                }
            }
            None => {}
        }
        true
    }
}

/// Whether `item` hides its text from an input method.
fn hides(engine: &Engine, item: ObjectId, client: &dyn TextInputClient) -> bool {
    client.hints(engine, item) & hint::HIDDEN_TEXT != 0
}

/// What an input method that tells the whole of what it holds typed into
/// a field that hides its text.
struct HiddenHeld {
    item: ObjectId,
    text: String,
    /// The field as the method left it ([`TextInputClient::digest`]).
    digest: u64,
}

#[derive(Default)]
struct Hidden(std::cell::RefCell<Option<HiddenHeld>>);

fn hidden_held(engine: &Engine) -> std::rc::Rc<Hidden> {
    engine.extension::<Hidden>()
}

/// What the method typed into the hidden field `item`, while the field is
/// as it left it; else nothing.
fn typed_hidden(engine: &Engine, item: ObjectId, client: &dyn TextInputClient) -> String {
    let held = hidden_held(engine);
    let mut held = held.0.borrow_mut();
    match held.as_ref() {
        Some(h) if h.item == item && h.digest == client.digest(engine, item) => h.text.clone(),
        _ => {
            *held = None;
            String::new()
        }
    }
}

/// The whole of what an input method holds: some platforms tell this
/// after every change rather than the change (Android's GameTextInput).
/// Positions count UTF-16 code units; `compose` is the part of `text`
/// still being composed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImeText {
    pub text: String,
    /// Where the selection starts; equal to `cursor` without one.
    pub anchor: usize,
    pub cursor: usize,
    pub compose: Option<Range<usize>>,
}

/// The byte where the UTF-16 unit `unit` of `text` is; a unit inside a
/// character, or past the end, is taken to the start of that character
/// or the end.
fn byte_at(text: &str, unit: usize) -> usize {
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        if units + c.len_utf16() > unit {
            return byte;
        }
        units += c.len_utf16();
    }
    text.len()
}

fn units_of(text: &str) -> usize {
    text.encode_utf16().count()
}

/// What changed from `old` to `new`: the range of `old`, in UTF-16 units,
/// that `new` has the returned text in place of. Taken from the ends, so
/// a character typed or deleted is that character.
fn changed(old: &str, new: &str) -> (Range<usize>, String) {
    let prefix: usize = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let (old_rest, new_rest) = (&old[prefix..], &new[prefix..]);
    let suffix: usize = old_rest
        .chars()
        .rev()
        .zip(new_rest.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let start = units_of(&old[..prefix]);
    let end = start + units_of(&old_rest[..old_rest.len() - suffix]);
    (start..end, new_rest[..new_rest.len() - suffix].to_owned())
}

impl ImeText {
    /// What is text, without what is being composed, and the composition.
    fn split(&self) -> (String, Option<(usize, String)>) {
        let len = units_of(&self.text);
        match &self.compose {
            Some(c) if c.start.min(len) < c.end.min(len) => {
                let (a, b) = (byte_at(&self.text, c.start), byte_at(&self.text, c.end));
                let mut text = self.text[..a].to_owned();
                text.push_str(&self.text[b..]);
                let at = units_of(&self.text[..a]);
                (text, Some((at, self.text[a..b].to_owned())))
            }
            _ => (self.text.clone(), None),
        }
    }
}

/// What a platform's text input service is told: the software keyboard of
/// a phone, the input method framework of a desktop.
pub trait PlatformTextInput {
    /// An editor took focus: text input is wanted. `hints` are
    /// `Qt.InputMethodHint` flags (digits only, hidden text, many lines).
    fn enable(&mut self, hints: i32);
    /// No editor has focus any more.
    fn disable(&mut self);
    /// Where the cursor is, in window coordinates: the place for a
    /// candidate window, and what a keyboard must not cover.
    fn set_cursor_area(&mut self, rect: kurbo::Rect);
    /// The text around the cursor changed. A platform without use for it
    /// ignores it.
    fn update(&mut self, _surrounding: &Surrounding) {}
    /// The application ended the composition (`Qt.inputMethod.commit()`,
    /// `reset()`): what the platform's method holds of it is dropped.
    fn interrupt(&mut self) {}
    /// The application asks for the on-screen keyboard
    /// (`Qt.inputMethod.show()`, `hide()`). A platform without one
    /// ignores it.
    fn set_keyboard_wanted(&mut self, _wanted: bool) {}
}

/// What the platform was last told, so that it is told only what changed.
#[derive(Debug, Default)]
pub struct ImeState {
    target: Option<(ObjectId, i32)>,
    area: Option<kurbo::Rect>,
    surrounding: Option<Surrounding>,
}

impl ImeState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The editor the platform was told about.
    pub fn target(&self) -> Option<ObjectId> {
        self.target.map(|(item, _)| item)
    }

    /// Tells `platform` what changed since the last call. The host calls
    /// it after the scene handled input or made a frame; it costs a few
    /// reads when nothing changed.
    pub fn sync(
        &mut self,
        scene: &QuickScene,
        engine: &Engine,
        platform: &mut dyn PlatformTextInput,
    ) {
        // Things move without telling the object: this is the one place
        // every host passes through after a frame.
        crate::input_method::refresh(engine);
        let asked = crate::input_method::take_requests(engine);
        if asked.interrupt {
            platform.interrupt();
        }
        if let Some(wanted) = asked.keyboard {
            platform.set_keyboard_wanted(wanted);
        }
        let target = scene
            .text_input_target(engine)
            .filter(|(item, client)| client.enabled(engine, *item));
        let Some((item, client)) = target else {
            if self.target.take().is_some() {
                self.area = None;
                self.surrounding = None;
                platform.disable();
            }
            return;
        };
        let now = (item, client.hints(engine, item));
        if self.target != Some(now) {
            self.target = Some(now);
            self.area = None;
            self.surrounding = None;
            platform.enable(now.1);
        }
        let area =
            scene_transform(engine, item).transform_rect_bbox(client.cursor_rect(engine, item));
        if self.area != Some(area) {
            self.area = Some(area);
            platform.set_cursor_area(area);
        }
        let surrounding = client.surrounding(engine, item);
        if self.surrounding.as_ref() != Some(&surrounding) {
            platform.update(&surrounding);
            self.surrounding = Some(surrounding);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_inside_a_character_is_its_start() {
        assert_eq!(byte_at("a𝄞b", 0), 0);
        assert_eq!(byte_at("a𝄞b", 1), 1);
        assert_eq!(byte_at("a𝄞b", 2), 1);
        assert_eq!(byte_at("a𝄞b", 3), 5);
        assert_eq!(byte_at("a𝄞b", 9), 6);
    }

    #[test]
    fn what_changed_is_counted_in_utf16() {
        assert_eq!(changed("ab", "abc"), (2..2, "c".into()));
        assert_eq!(changed("abc", "ac"), (1..2, String::new()));
        assert_eq!(changed("a𝄞b", "a𝄞xb"), (3..3, "x".into()));
        assert_eq!(changed("a𝄞b", "ab"), (1..3, String::new()));
        assert_eq!(changed("aa", "aaa"), (2..2, "a".into()));
        assert_eq!(changed("same", "same"), (4..4, String::new()));
    }

    #[test]
    fn a_composing_region_is_split_from_the_text() {
        let held = ImeText {
            text: "ab𝄞c".into(),
            anchor: 4,
            cursor: 4,
            compose: Some(2..4),
        };
        assert_eq!(held.split(), ("abc".into(), Some((2, "𝄞".into()))));
        let empty = ImeText {
            compose: Some(1..1),
            ..held.clone()
        };
        assert_eq!(empty.split(), ("ab𝄞c".into(), None));
        let past = ImeText {
            compose: Some(9..12),
            ..held
        };
        assert_eq!(past.split().1, None);
    }
}
