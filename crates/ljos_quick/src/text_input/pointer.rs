//! The pointer in an editor: a press puts the cursor where it lands and
//! takes focus, a drag selects, a double click selects a word. A press
//! released on the link of rich text it landed on activates the link.

use ljos_engine::{Engine, ObjectId, Value};
use ljos_render::kurbo;
use ljos_runtime::Time;

use super::base::{SharedState, TextEditor};
use super::layout::position_at;
use super::ops;
use super::{cursor_position, selection_mode};
use crate::focus::{self, FocusReason};
use crate::input::{Modifiers, MouseButtons, PointerEvent};
use crate::item::Item;
use crate::scene::{DOUBLE_CLICK_INTERVAL, DRAG_THRESHOLD};

/// What an editor remembers of the pointer.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PointerState {
    /// When and where the last press was, to tell a double click.
    last: Option<(Time, kurbo::Point)>,
    /// The press the editor holds selects from here: where it landed, or
    /// the word a double click selected. `None` while it selects nothing
    /// (`selectByMouse: false`).
    anchor: Option<(usize, usize)>,
    /// The drag selects whole words.
    words: bool,
}

fn flag(engine: &Engine, obj: ObjectId, p: ljos_engine::PropIndex) -> bool {
    engine.untrack(|| engine.read(obj, p)).to_boolean()
}

fn at_point(st: &SharedState, at: kurbo::Point) -> usize {
    position_at(st, at, cursor_position::BETWEEN_CHARACTERS)
}

pub(super) fn press(engine: &Engine, obj: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    if ev.button != Some(MouseButtons::LEFT) || !flag(engine, obj, Item::ENABLED.index) {
        return false;
    }
    let Some(st) = ops::ready(engine, obj) else {
        return false;
    };
    if flag(engine, obj, TextEditor::ACTIVE_FOCUS_ON_PRESS.index) {
        focus::force_active_focus(engine, obj, FocusReason::Mouse);
    }
    let selects = flag(engine, obj, TextEditor::SELECT_BY_MOUSE.index);
    let by_words = engine
        .untrack(|| engine.read(obj, TextEditor::MOUSE_SELECTION_MODE.index))
        .to_number() as i32
        == selection_mode::SELECT_WORDS;
    let double = st.borrow().pointer.last.is_some_and(|(t, p)| {
        ev.time.saturating_since(t) <= DOUBLE_CLICK_INTERVAL && (at - p).hypot() <= DRAG_THRESHOLD
    });
    let pos = at_point(&st, at);
    // A link of rich text is activated where the press is released.
    let link = super::rich::link_at(&st, at);
    if let Some(r) = st.borrow_mut().rich.as_mut() {
        r.pressed = link;
    }
    let extends = selects && ev.modifiers.contains(Modifiers::SHIFT);
    ops::act_by_pointer(engine, obj, |st| {
        let mut s = st.borrow_mut();
        let s = &mut *s;
        if extends {
            s.model.move_to(pos, true);
        } else {
            s.model.move_to(pos, false);
        }
        if double && selects {
            let reach = !s.multi;
            s.model.select_word(reach);
        }
        let base = if extends {
            (s.model.anchor(), s.model.anchor())
        } else if double {
            let word = s.model.selection();
            (word.start, word.end)
        } else {
            (pos, pos)
        };
        s.pointer = PointerState {
            // A third press starts over: it is no double click of the
            // second.
            last: (!double).then_some((ev.time, at)),
            anchor: selects.then_some(base),
            words: by_words || double,
        };
    });
    true
}

pub(super) fn moved(engine: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    let Some(st) = ops::ready(engine, obj) else {
        return;
    };
    let pointer = st.borrow().pointer;
    let Some((start, end)) = pointer.anchor else {
        return;
    };
    let pos = at_point(&st, at);
    ops::act_by_pointer(engine, obj, |st| {
        let mut s = st.borrow_mut();
        // From where the press was, each time: a drag that turns round
        // gives back what it had taken.
        if pos > end || (pos >= end && start == end) {
            s.model.select(start, start);
            s.model.move_cursor_selection(pos, pointer.words);
        } else if pos < start {
            s.model.select(end, end);
            s.model.move_cursor_selection(pos, pointer.words);
        } else {
            s.model.select(start, end);
        }
    });
}

pub(super) fn release(engine: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    let pressed = super::base::state_of(engine, obj)
        .and_then(|st| st.borrow_mut().rich.as_mut().and_then(|r| r.pressed.take()));
    cancel(engine, obj);
    let Some(link) = pressed else { return };
    let Some(st) = super::base::state_of(engine, obj) else {
        return;
    };
    if super::rich::link_at(&st, at).as_deref() == Some(&*link) {
        let link = Value::String(ljos_engine::Atom::from(&*link));
        if let Err(err) = engine.emit_by_name(obj, "linkActivated", &[link]) {
            super::base::check(engine, "linkActivated", Err(err));
        }
    }
}

pub(super) fn cancel(engine: &Engine, obj: ObjectId) {
    if let Some(st) = super::base::state_of(engine, obj) {
        let mut s = st.borrow_mut();
        s.pointer.anchor = None;
        s.pointer.words = false;
        if let Some(r) = s.rich.as_mut() {
            r.pressed = None;
        }
    }
}
