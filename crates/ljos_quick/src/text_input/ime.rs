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
        ops::edit(engine, item, How::user(EditKind::Other), |m| {
            let len = m.len();
            m.replace(range.start.min(len)..range.end.min(len), text)
        });
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
        self.text_input_target(engine)
            .is_some_and(|(item, client)| client.set_preedit(engine, item, text, cursor))
    }

    /// An input method types `text`, ending its composition. Returns
    /// whether an editor took it.
    pub fn ime_commit(&self, engine: &Engine, text: &str) -> bool {
        self.note_input();
        self.text_input_target(engine)
            .is_some_and(|(item, client)| client.commit(engine, item, text))
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
