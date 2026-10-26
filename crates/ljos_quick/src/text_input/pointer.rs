//! The pointer in an editor: a press puts the cursor where it lands and
//! takes focus, a drag selects, a double click selects a word and a triple
//! click the paragraph. A press
//! released on the link of rich text it landed on activates the link.

use ljos_engine::{Engine, ObjectId, Value};
use ljos_render::kurbo;
use ljos_runtime::Time;

use super::base::{SharedState, TextEditor};
use super::layout::{character_at, position_at};
use super::ops;
use super::{cursor_position, selection_mode};
use crate::flickable;
use crate::focus::{self, FocusReason};
use crate::input::{Modifiers, MouseButtons, PointerEvent, PointerId};
use crate::item::Item;
use crate::scene::{DOUBLE_CLICK_INTERVAL, DRAG_THRESHOLD};

/// What an editor remembers of the pointer.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PointerState {
    /// When and where the last press was, and how many came quickly one
    /// after the other there, to tell a double and a triple click.
    last: Option<(Time, kurbo::Point, u8)>,
    /// The press the editor holds selects from here: where it landed, or
    /// the word a double click selected. `None` while it selects nothing
    /// (`selectByMouse: false`).
    anchor: Option<(usize, usize)>,
    how: Drag,
}

/// How a drag selects, measured on Qt 6.11.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Drag {
    /// To the gap nearest the pointer.
    #[default]
    Characters,
    /// Out to the words at both ends (`mouseSelectionMode: SelectWords`).
    Words,
    /// From a double click in a multi-line editor: out to the word of the
    /// character under the pointer, and nowhere while it is over no word.
    WordsUnder,
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
    let clicks = match st.borrow().pointer.last {
        Some((t, p, n))
            if ev.time.saturating_since(t) <= DOUBLE_CLICK_INTERVAL
                && (at - p).hypot() <= DRAG_THRESHOLD =>
        {
            n + 1
        }
        _ => 1,
    };
    let double = clicks == 2;
    let triple = clicks == 3;
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
        // Qt 6.11 selects the paragraph with its line break, the whole
        // text of a TextInput, and leaves the cursor at the end.
        if triple && selects {
            s.model.select_paragraph();
        }
        let how = if by_words {
            Drag::Words
        } else if double && s.multi {
            Drag::WordsUnder
        } else {
            Drag::Characters
        };
        let base = if extends {
            (s.model.anchor(), s.model.anchor())
        } else if double && how == Drag::Characters {
            // A TextInput drags by characters from the start of the word
            // a double click selected.
            let start = s.model.selection().start;
            (start, start)
        } else if double || triple {
            let chosen = s.model.selection();
            (chosen.start, chosen.end)
        } else {
            (pos, pos)
        };
        s.pointer = PointerState {
            // A fourth press starts over.
            last: (!triple).then_some((ev.time, at, clicks)),
            anchor: selects.then_some(base),
            how,
        };
    });
    // A TextEdit that selects keeps the mouse's press, so a Flickable
    // round it never takes a drag up or down the lines. A TextInput has
    // one line: a drag across it selects before the Flickable's threshold
    // is passed sideways, and a drag up or down flicks, as in Qt. A touch
    // still flicks.
    if selects && st.borrow().multi && ev.id == PointerId::default() {
        flickable::keep_pointer_grab(engine, obj, true);
    }
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
    let pos = if pointer.how == Drag::WordsUnder {
        let found = character_at(&st, at).and_then(|c| st.borrow().model.word_of_character(c));
        // Over a space, punctuation, past a line's end or above the text
        // the selection stays as it is.
        let Some(word) = found else {
            return;
        };
        ops::act_by_pointer(engine, obj, |st| {
            let mut s = st.borrow_mut();
            if word.start >= end {
                s.model.select(start, word.end);
            } else if word.end <= start {
                s.model.select(end, word.start);
            } else {
                s.model.select(start, end);
            }
        });
        return;
    } else {
        at_point(&st, at)
    };
    let words = pointer.how == Drag::Words;
    let multi = st.borrow().multi;
    let extend = |s: &mut super::base::State| {
        if words && multi {
            s.model.drag_by_words(pos);
        } else {
            s.model.move_cursor_selection(pos, words);
        }
    };
    ops::act_by_pointer(engine, obj, |st| {
        let mut s = st.borrow_mut();
        // From where the press was, each time: a drag that turns round
        // gives back what it had taken.
        if pos > end || (pos >= end && start == end) {
            s.model.select(start, start);
            extend(&mut s);
        } else if pos < start {
            s.model.select(end, end);
            extend(&mut s);
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
    flickable::keep_pointer_grab(engine, obj, false);
    if let Some(st) = super::base::state_of(engine, obj) {
        let mut s = st.borrow_mut();
        s.pointer.anchor = None;
        s.pointer.how = Drag::Characters;
        if let Some(r) = s.rich.as_mut() {
            r.pressed = None;
        }
    }
}
