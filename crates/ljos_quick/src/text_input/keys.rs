//! The keys of an editor: moving the cursor, selecting, deleting, the
//! clipboard, undo and typing.
//!
//! Which key does what comes from the platform's standard keys
//! ([`crate::keys::standard_key_sequences`]), so Ctrl+C copies on Linux and
//! Cmd+C on macOS. What an editor does with a key it has no use for, and
//! which keys it keeps although they change nothing, follows what qml6
//! does (`features/text-input/keys.feature`, `text-edit-keys.feature`): a
//! key the editor does not take goes on to its parents.

use std::cell::RefCell;
use std::collections::HashMap;

use ljos_engine::{Engine, ObjectId, Value};
use ljos_text::reads_right_to_left;

use super::base::{EditKind, SharedState, State, check, flag, publish};
use super::echo_mode;
use super::edit::TextEdit;
use super::input::TextInput;
use super::ops::{self, How};
use super::validator::{Validation, validate};
use crate::input::{KeyEvent, Modifiers};
use crate::keys::{KeyChord, Platform, key, platform, standard_key, standard_key_sequences};

/// The modifiers that tell one key binding from another.
const CHORD_MODIFIERS: Modifiers = Modifiers::SHIFT
    .union(Modifiers::CONTROL)
    .union(Modifiers::ALT)
    .union(Modifiers::META);

/// Where a key moves the cursor. Left and right are what the keys say:
/// in text that reads right to left, Right goes towards the start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motion {
    Right,
    Left,
    WordRight,
    WordLeft,
    Down,
    Up,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    /// With `true` the selection follows the cursor.
    Move(Motion, bool),
    SelectAll,
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
    Backspace,
    Delete,
    DeleteStartOfWord,
    DeleteEndOfWord,
    DeleteEndOfLine,
    DeleteCompleteLine,
    NewLine,
}

impl Action {
    /// It changes the text, so a read-only editor does not do it.
    fn edits(self) -> bool {
        !matches!(self, Action::Move(..) | Action::SelectAll | Action::Copy)
    }
}

/// The standard keys an editor knows and what each does. Where a platform
/// has one key for two of them, the first here has it.
const BINDINGS: &[(&str, Action)] = &[
    ("SelectAll", Action::SelectAll),
    ("Copy", Action::Copy),
    ("Cut", Action::Cut),
    ("Paste", Action::Paste),
    ("Undo", Action::Undo),
    ("Redo", Action::Redo),
    ("Delete", Action::Delete),
    ("DeleteStartOfWord", Action::DeleteStartOfWord),
    ("DeleteEndOfWord", Action::DeleteEndOfWord),
    ("DeleteEndOfLine", Action::DeleteEndOfLine),
    ("DeleteCompleteLine", Action::DeleteCompleteLine),
    ("Backspace", Action::Backspace),
    ("MoveToNextChar", Action::Move(Motion::Right, false)),
    ("MoveToPreviousChar", Action::Move(Motion::Left, false)),
    ("MoveToNextWord", Action::Move(Motion::WordRight, false)),
    ("MoveToPreviousWord", Action::Move(Motion::WordLeft, false)),
    ("MoveToNextLine", Action::Move(Motion::Down, false)),
    ("MoveToPreviousLine", Action::Move(Motion::Up, false)),
    ("MoveToStartOfLine", Action::Move(Motion::LineStart, false)),
    ("MoveToEndOfLine", Action::Move(Motion::LineEnd, false)),
    ("MoveToStartOfBlock", Action::Move(Motion::LineStart, false)),
    ("MoveToEndOfBlock", Action::Move(Motion::LineEnd, false)),
    (
        "MoveToStartOfDocument",
        Action::Move(Motion::DocStart, false),
    ),
    ("MoveToEndOfDocument", Action::Move(Motion::DocEnd, false)),
    ("SelectNextChar", Action::Move(Motion::Right, true)),
    ("SelectPreviousChar", Action::Move(Motion::Left, true)),
    ("SelectNextWord", Action::Move(Motion::WordRight, true)),
    ("SelectPreviousWord", Action::Move(Motion::WordLeft, true)),
    ("SelectNextLine", Action::Move(Motion::Down, true)),
    ("SelectPreviousLine", Action::Move(Motion::Up, true)),
    ("SelectStartOfLine", Action::Move(Motion::LineStart, true)),
    ("SelectEndOfLine", Action::Move(Motion::LineEnd, true)),
    ("SelectStartOfBlock", Action::Move(Motion::LineStart, true)),
    ("SelectEndOfBlock", Action::Move(Motion::LineEnd, true)),
    (
        "SelectStartOfDocument",
        Action::Move(Motion::DocStart, true),
    ),
    ("SelectEndOfDocument", Action::Move(Motion::DocEnd, true)),
    ("InsertParagraphSeparator", Action::NewLine),
    ("InsertLineSeparator", Action::NewLine),
];

/// Whether an editor of one kind has a use for an action at all.
fn knows(action: Action, multi: bool, platform: Platform) -> bool {
    if multi {
        // A TextEdit has no "delete the whole line".
        return action != Action::DeleteCompleteLine;
    }
    match action {
        // One line has no lines to go to, and Return accepts it.
        Action::Move(Motion::Up | Motion::Down, _) | Action::NewLine => false,
        // Ctrl+Home is not a field's key where Home alone is; on macOS
        // the document keys are the only ones a keyboard has for it.
        Action::Move(Motion::DocStart | Motion::DocEnd, _) => platform == Platform::MacOs,
        _ => true,
    }
}

type Table = HashMap<KeyChord, Action>;

/// The key bindings of the engine's platform, made when a key first
/// arrives: one table for fields, one for multi-line editors.
#[derive(Default)]
struct Tables(RefCell<Option<(Platform, [Table; 2])>>);

fn build(platform: Platform, multi: bool) -> Table {
    let mut table = Table::new();
    for (name, action) in BINDINGS {
        if !knows(*action, multi, platform) {
            continue;
        }
        let Some(standard) = standard_key(name) else {
            continue;
        };
        for sequence in standard_key_sequences(standard, platform) {
            if let [chord] = sequence.chords() {
                table.entry(*chord).or_insert(*action);
            }
        }
    }
    table
}

fn action_of(engine: &Engine, ev: &KeyEvent, multi: bool) -> Option<Action> {
    let platform = platform(engine);
    let tables = engine.extension::<Tables>();
    let mut tables = tables.0.borrow_mut();
    if tables.as_ref().is_none_or(|(p, _)| *p != platform) {
        *tables = Some((platform, [build(platform, false), build(platform, true)]));
    }
    let (_, [single, many]) = tables.as_ref()?;
    let modifiers = ev.modifiers & CHORD_MODIFIERS;
    let found = if multi { many } else { single }
        .get(&KeyChord::new(ev.key, modifiers))
        .copied();
    // Backspace is a standard key only where a platform has a second key
    // for it; the key itself always is, with Shift held too.
    found.or_else(|| {
        (ev.key == key::BACKSPACE && (modifiers - Modifiers::SHIFT).is_empty())
            .then_some(Action::Backspace)
    })
}

/// What the key types, if it types.
fn typed(ev: &KeyEvent, multi: bool) -> Option<&str> {
    let m = ev.modifiers;
    // Ctrl and Alt together are AltGr on some keyboards, which types.
    if ev.text.is_empty()
        || m.contains(Modifiers::META)
        || (m.contains(Modifiers::CONTROL) && !m.contains(Modifiers::ALT))
    {
        return None;
    }
    // A key event that carries a tab types it into a multi-line editor;
    // any other control character is a command, not text.
    if ev
        .text
        .chars()
        .any(|c| c.is_control() && !(multi && c == '\t'))
    {
        return None;
    }
    Some(&ev.text)
}

/// What an editor does with a key.
#[derive(Clone, Debug, PartialEq)]
enum Plan {
    /// Nothing: the key goes on to the parents.
    Pass,
    /// Nothing, but the key is the editor's.
    Keep,
    Move {
        pos: usize,
        select: bool,
        /// The column an Up or Down keeps to.
        column: Option<f64>,
    },
    Do(Action),
    Type(String),
    /// Return in a field: it says `accepted` and the key goes on.
    Accept,
}

/// Whether the text at the cursor reads right to left.
fn rtl_at(s: &State) -> bool {
    let Some(layout) = s.layout.as_ref() else {
        return false;
    };
    let byte = s.shown.byte_of(s.model.cursor());
    let line = &layout.lines[layout.line_of(byte).min(layout.lines.len() - 1)];
    line.clusters
        .iter()
        .find(|c| c.range.start == byte)
        .or_else(|| line.clusters.iter().find(|c| c.range.end == byte))
        .map_or(layout.rtl, |c| c.rtl)
}

/// Whether the paragraph the cursor is in reads right to left.
fn paragraph_rtl(s: &State) -> bool {
    let text = s.model.text();
    let at = s.model.cursor_byte().min(text.len());
    let breaks = |c: char| c == '\n' || c == '\u{2029}';
    let start = text[..at]
        .char_indices()
        .rfind(|&(_, c)| breaks(c))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let end = text[at..].find(breaks).map_or(text.len(), |i| at + i);
    reads_right_to_left(&text[start..end])
}

fn motion(s: &State, motion: Motion, select: bool, echo: i32) -> Plan {
    let model = &s.model;
    let (cursor, len) = (model.cursor(), model.len());
    // With a mask the end is where the text would end without its blanks.
    let end = s
        .mask
        .as_ref()
        .map_or(len, |mask| mask.text_end(model.text()));
    // A TextEdit walks the text in the order it is written, the way its
    // paragraph reads; a TextInput walks it the way it shows, character
    // by character (see features/text-input/bidi-edit.feature).
    let rtl = if s.multi { paragraph_rtl(s) } else { rtl_at(s) };
    let to = |pos: usize| Plan::Move {
        pos,
        select,
        column: None,
    };
    match motion {
        Motion::Right | Motion::Left => {
            if echo == echo_mode::NO_ECHO {
                // Nothing shows where the cursor is, so it is not walked
                // through the text; the key is still the field's.
                return Plan::Keep;
            }
            let forward = (motion == Motion::Right) != rtl;
            if select && let Some(step) = super::cells::shift_step(s, forward) {
                return step.map_or(Plan::Keep, to);
            }
            if !select && model.has_selection() {
                // The selection goes and the cursor is left at its end
                // on the side of the key.
                let r = model.selection();
                return to(if forward { r.end } else { r.start });
            }
            let pos = if forward {
                model.next_grapheme(cursor)
            } else {
                model.prev_grapheme(cursor)
            };
            if pos == cursor { Plan::Pass } else { to(pos) }
        }
        Motion::WordRight | Motion::WordLeft => {
            let forward = (motion == Motion::WordRight) != rtl;
            let pos = match (echo == echo_mode::NORMAL, forward) {
                (true, true) => model.next_word(cursor),
                (true, false) => model.prev_word(cursor),
                // A hidden text does not show where its words are.
                (false, true) => len,
                (false, false) => 0,
            };
            if pos == cursor && (select || !model.has_selection()) {
                Plan::Pass
            } else {
                to(pos)
            }
        }
        Motion::DocStart => to(0),
        Motion::DocEnd => to(end),
        Motion::LineStart | Motion::LineEnd => {
            let start = motion == Motion::LineStart;
            let Some(layout) = s.layout.as_ref().filter(|_| s.multi) else {
                return to(if start { 0 } else { end });
            };
            // The line as it is laid out: a wrapped paragraph is several.
            let byte = s.shown.byte_of(cursor);
            let line = &layout.lines[layout.line_of(byte).min(layout.lines.len() - 1)];
            to(s.shown.pos_of(if start {
                line.range.start
            } else {
                line.end.max(line.range.start)
            }))
        }
        Motion::Down | Motion::Up => {
            let Some(layout) = s.layout.as_ref() else {
                return Plan::Pass;
            };
            let caret = layout.caret(s.shown.byte_of(cursor));
            let left = |line: usize| s.geo.line_x.get(line).copied().unwrap_or(0.0);
            // In a table the column is kept from the cell's left edge, so a
            // cursor going between cells of other widths keeps its place in
            // them, as in Qt 6.11.
            let inset = |line: usize| layout.lines.get(line).map_or(0.0, |l| l.left);
            let x = match s.sticky {
                Some((x, at)) if at == cursor => x,
                _ => left(caret.line) + caret.x - inset(caret.line),
            };
            let target = layout.vertical(caret.line, motion == Motion::Down);
            match target {
                Some(line) => Plan::Move {
                    pos: s
                        .shown
                        .pos_of(layout.hit_line(line, x - left(line) + inset(line), false)),
                    select,
                    column: Some(x),
                },
                // Selecting down from the last line takes the rest of it.
                None if motion == Motion::Down && select && cursor != len => to(len),
                None => Plan::Pass,
            }
        }
    }
}

fn plan(engine: &Engine, obj: ObjectId, st: &SharedState, ev: &KeyEvent) -> Plan {
    let multi = st.borrow().multi;
    let read_only = ops::read_only(engine, obj);
    let echo = ops::echo(engine, obj, st);
    if !multi && matches!(ev.key, key::RETURN | key::ENTER) {
        return Plan::Accept;
    }
    let Some(action) = action_of(engine, ev, multi) else {
        return match typed(ev, multi) {
            Some(text) if !read_only => Plan::Type(text.to_owned()),
            _ => Plan::Pass,
        };
    };
    match action {
        Action::Move(m, select) => {
            if multi && !flag(engine, obj, TextEdit::SELECT_BY_KEYBOARD.index) {
                // The keyboard is left to whoever wants it (a view that
                // scrolls with the arrows).
                return Plan::Pass;
            }
            motion(&st.borrow(), m, select, echo)
        }
        // A read-only field keeps the keys that would edit it; a
        // read-only TextEdit lets them go.
        a if a.edits() && read_only => {
            if multi {
                Plan::Pass
            } else {
                Plan::Keep
            }
        }
        a => Plan::Do(a),
    }
}

/// Whether the editor wants the key before a `Shortcut` gets it: only a
/// common editing key it would act on. Any other key it acts on, such as
/// Ctrl+K deleting to the end of the line, is a `Shortcut`'s first and
/// reaches [`press`] only when no `Shortcut` takes it, as in Qt
/// (`features/text-input/shortcut-override.feature`).
pub(super) fn shortcut_override(engine: &Engine, obj: ObjectId, ev: &KeyEvent) -> bool {
    let Some(st) = ops::ready(engine, obj) else {
        return false;
    };
    match plan(engine, obj, &st, ev) {
        Plan::Pass | Plan::Accept => false,
        // A key that types is the editor's whatever its modifiers: AltGr
        // arrives as Ctrl+Alt.
        Plan::Type(_) => true,
        Plan::Keep | Plan::Move { .. } | Plan::Do(_) => common(engine, ev, st.borrow().multi),
    }
}

/// The keys every editor keeps from a `Shortcut`: the clipboard, undo and
/// redo, Select All, the cursor moves and selections, and any key with
/// no modifier but Shift.
fn common(engine: &Engine, ev: &KeyEvent, multi: bool) -> bool {
    ((ev.modifiers & CHORD_MODIFIERS) - Modifiers::SHIFT).is_empty()
        || action_of(engine, ev, multi).is_some_and(|action| {
            matches!(
                action,
                Action::Move(..)
                    | Action::SelectAll
                    | Action::Copy
                    | Action::Cut
                    | Action::Paste
                    | Action::Undo
                    | Action::Redo
            )
        })
}

pub(super) fn press(engine: &Engine, obj: ObjectId, ev: &KeyEvent) -> bool {
    let Some(st) = ops::ready(engine, obj) else {
        return false;
    };
    let mut what = plan(engine, obj, &st, ev);
    match what {
        Plan::Pass => return false,
        Plan::Accept => {
            accept(engine, obj);
            return false;
        }
        _ => {}
    }
    // `PasswordEchoOnEdit`: the first key that does something to a masked
    // text starts it over, and then does what it does to an empty field.
    if what != Plan::Do(Action::Copy) && !st.borrow().editing {
        let before = st.borrow().model.len();
        ops::begin_editing(engine, obj);
        if st.borrow().model.len() != before {
            what = match plan(engine, obj, &st, ev) {
                Plan::Pass | Plan::Accept => Plan::Keep,
                again => again,
            };
        }
    }
    match what {
        Plan::Pass | Plan::Accept | Plan::Keep => {}
        Plan::Move {
            pos,
            select,
            column,
        } => {
            {
                let mut s = st.borrow_mut();
                let pos = match &s.mask {
                    Some(mask) => mask.snap(s.model.cursor(), pos),
                    None => pos,
                };
                s.model.move_to(pos, select);
                super::cells::snap(&mut s);
                s.run = None;
                // Kept where a selection of cells took the cursor.
                let at = s.model.cursor();
                s.sticky = column.map(|x| (x, at));
            }
            check(engine, "cursor", publish(engine, obj, &st));
        }
        Plan::Type(text) => {
            ops::type_text(engine, obj, &text);
        }
        Plan::Do(action) => act(engine, obj, &st, action),
    }
    true
}

/// Return in a field: an acceptable text is finished and accepted.
fn accept(engine: &Engine, obj: ObjectId) {
    let Some(st) = ops::ready(engine, obj) else {
        return;
    };
    if !acceptable(engine, obj, &st) {
        return;
    }
    for signal in ["editingFinished", "accepted"] {
        check(engine, signal, engine.emit_by_name(obj, signal, &[]));
    }
}

/// Whether a field's text is acceptable to its validator, asked of the
/// validator now: `acceptableInput` may not have caught up inside a
/// handler.
pub(super) fn acceptable(engine: &Engine, obj: ObjectId, st: &SharedState) -> bool {
    if st.borrow().multi {
        return true;
    }
    let shown = {
        let s = st.borrow();
        match &s.mask {
            Some(mask) if !mask.acceptable(s.model.text()) => return false,
            Some(mask) => mask.strip(s.model.text()),
            None => s.model.text().to_owned(),
        }
    };
    match engine.untrack(|| engine.read(obj, TextInput::VALIDATOR.index)) {
        Value::Object(v) if engine.is_alive(v) => {
            let text = shown;
            engine.untrack(|| validate(engine, v, &text)) == Validation::Acceptable
        }
        _ => true,
    }
}

/// What deleting does with a mask: places are emptied, and separators stay.
/// Whether `action` was one of those.
fn act_masked(
    engine: &Engine,
    obj: ObjectId,
    mask: &super::mask::Mask,
    action: Action,
    shows: bool,
) -> bool {
    let other = How::user(EditKind::Other);
    match action {
        Action::Backspace => ops::edit(engine, obj, How::user(EditKind::Backspace), |m| {
            mask.backspace(m)
        }),
        Action::Delete => ops::edit(engine, obj, How::user(EditKind::Delete), |m| mask.delete(m)),
        Action::DeleteStartOfWord => ops::edit(engine, obj, other, |m| {
            let target = if shows { m.prev_word(m.cursor()) } else { 0 };
            mask.delete_back_to(m, target)
        }),
        Action::DeleteEndOfWord => ops::edit(engine, obj, other, |m| {
            let to = if shows {
                m.next_word(m.cursor())
            } else {
                m.len()
            };
            let to = mask.snap(m.cursor(), to);
            mask.delete_forward_to(m, to)
        }),
        Action::DeleteEndOfLine => ops::edit(engine, obj, other, |m| {
            let to = m.len();
            mask.delete_forward_to(m, to)
        }),
        Action::DeleteCompleteLine => ops::edit(engine, obj, other, |m| mask.clear(m)),
        _ => return false,
    };
    true
}

fn act(engine: &Engine, obj: ObjectId, st: &SharedState, action: Action) {
    let (multi, shows) = {
        let s = st.borrow();
        (s.multi, ops::echo(engine, obj, st) == echo_mode::NORMAL)
    };
    if let Some(mask) = super::mask::of(st)
        && act_masked(engine, obj, &mask, action, shows)
    {
        return;
    }
    // A word or a line, from the cursor; a selection goes instead.
    let delete_to = |target: fn(&ljos_text::TextModel, usize, bool) -> usize| {
        ops::edit(engine, obj, How::user(EditKind::Other), |m| {
            if m.has_selection() {
                return m.delete_selection();
            }
            let cursor = m.cursor();
            let to = target(m, cursor, shows);
            m.replace(cursor.min(to)..cursor.max(to), "")
        });
    };
    match action {
        Action::Move(..) => {}
        Action::SelectAll => ops::select_all(engine, obj),
        Action::Copy => {
            ops::copy(engine, obj);
        }
        Action::Cut => {
            ops::cut(engine, obj);
        }
        Action::Paste => {
            ops::paste(engine, obj);
        }
        Action::Undo => {
            ops::undo(engine, obj);
        }
        Action::Redo => {
            ops::redo(engine, obj);
        }
        // Over a selection of cells either key empties the cells.
        Action::Backspace | Action::Delete
            if ops::empty_cells(engine, obj, EditKind::Other).is_some() => {}
        Action::Backspace | Action::Delete
            if ops::at_table_edge(engine, obj, action == Action::Delete) => {}
        Action::Backspace if ops::outdent(engine, obj) => {}
        Action::Backspace => {
            ops::edit(engine, obj, How::user(EditKind::Backspace), |m| {
                m.backspace()
            });
        }
        Action::Delete => {
            ops::edit(engine, obj, How::user(EditKind::Delete), |m| {
                m.delete_forward()
            });
        }
        Action::DeleteStartOfWord => {
            delete_to(|m, at, shows| if shows { m.prev_word(at) } else { 0 });
        }
        Action::DeleteEndOfWord => {
            delete_to(|m, at, shows| if shows { m.next_word(at) } else { m.len() });
        }
        Action::DeleteEndOfLine => delete_to(|m, at, _| m.paragraph_end(at)),
        Action::DeleteCompleteLine => {
            ops::edit(engine, obj, How::user(EditKind::Other), |m| {
                m.select_all();
                m.delete_selection()
            });
        }
        Action::NewLine => {
            if multi {
                // Return in a table cell adds a paragraph to the cell, as
                // in Qt 6.11 (features/text-input/rich-text-cell-paragraphs.feature).
                let in_cell = {
                    let mut s = st.borrow_mut();
                    super::rich::sync_doc(&mut s);
                    let at = s.model.selection_bytes().start;
                    s.rich.as_ref().is_some_and(|r| {
                        let blocks = r.doc.blocks();
                        blocks
                            .get(r.doc.block_index(at))
                            .is_some_and(|b| b.table.is_some())
                    })
                };
                let text = if in_cell { "\u{2029}" } else { "\n" };
                ops::edit(engine, obj, How::user(EditKind::Other), |m| {
                    m.replace_selection(text)
                });
            }
        }
    }
}
