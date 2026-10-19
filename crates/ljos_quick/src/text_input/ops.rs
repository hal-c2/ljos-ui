//! What can be done to an editor's text, by the user or by a program:
//! every way in (keys, the pointer, an input method, the methods QML
//! calls) goes through here, so that the model, the properties, the undo
//! history and the signals stay in step.

use ljos_engine::{Engine, ObjectId, Value};
use ljos_text::{TextModel, utf16_len};

use super::base::{
    EditKind, Run, SharedState, TextEditor, check, init_editor, publish, publish_unshown,
    reconcile, state_of,
};
use super::clipboard::Clipboard;
use super::echo_mode;
use super::input::TextInput;
use super::validator::{Validation, validate};

/// The state of an editor, brought up to date with what a program wrote
/// to its properties.
pub(crate) fn ready(engine: &Engine, obj: ObjectId) -> Option<SharedState> {
    let st = state_of(engine, obj)?;
    if !st.borrow().initialised {
        // Called before the item completed (from a handler that ran
        // early): completing now is what it would have found later.
        check(engine, "init", init_editor(engine, obj));
    }
    reconcile(engine, obj, &st);
    Some(st)
}

fn flag(engine: &Engine, obj: ObjectId, p: ljos_engine::PropIndex) -> bool {
    engine.untrack(|| engine.read(obj, p)).to_boolean()
}

pub(crate) fn read_only(engine: &Engine, obj: ObjectId) -> bool {
    flag(engine, obj, TextEditor::READ_ONLY.index)
}

/// `echoMode` of a `TextInput`; a `TextEdit` always shows its text.
pub(crate) fn echo(engine: &Engine, obj: ObjectId, st: &SharedState) -> i32 {
    if st.borrow().multi {
        return echo_mode::NORMAL;
    }
    engine
        .untrack(|| engine.read(obj, TextInput::ECHO_MODE.index))
        .to_number() as i32
}

/// Moves the cursor or changes the selection: `f` does it to the model.
/// That ends a run of typing, so the next edit is an undo step of its own.
pub(crate) fn act(engine: &Engine, obj: ObjectId, f: impl FnOnce(&SharedState)) {
    if let Some(st) = acted(engine, obj, f) {
        check(engine, "selection", publish(engine, obj, &st));
    }
}

/// `act` for the pointer, which leaves the blink of the cursor as it is.
pub(crate) fn act_by_pointer(engine: &Engine, obj: ObjectId, f: impl FnOnce(&SharedState)) {
    if let Some(st) = acted(engine, obj, f) {
        check(engine, "selection", publish_unshown(engine, obj, &st));
    }
}

fn acted(engine: &Engine, obj: ObjectId, f: impl FnOnce(&SharedState)) -> Option<SharedState> {
    let st = ready(engine, obj)?;
    f(&st);
    {
        let mut s = st.borrow_mut();
        s.run = None;
        s.sticky = None;
    }
    Some(st)
}

/// How an edit is made.
#[derive(Clone, Copy, Debug)]
pub(crate) struct How {
    pub kind: EditKind,
    /// The user's edit: a validator may refuse it.
    pub user: bool,
    /// A `TextInput` says `textEdited`.
    pub emits: bool,
    /// Where the edit starts; `None`: at the cursor. It joins the run
    /// before it when that ended here.
    pub start: Option<usize>,
    /// Where it ends; `None`: at the cursor afterwards.
    pub end: Option<usize>,
    /// It joins a run of its kind wherever that ended.
    pub anywhere: bool,
}

impl How {
    /// What the user does.
    pub fn user(kind: EditKind) -> How {
        How {
            kind,
            user: true,
            emits: true,
            start: None,
            end: None,
            anywhere: false,
        }
    }
}

/// Changes the text: `f` does it to the model and says whether the text
/// changed. Returns that.
pub(crate) fn edit(
    engine: &Engine,
    obj: ObjectId,
    how: How,
    f: impl FnOnce(&mut TextModel) -> bool,
) -> bool {
    let Some(st) = ready(engine, obj) else {
        return false;
    };
    let single = !st.borrow().multi;
    let validator = if how.user && single {
        match engine.untrack(|| engine.read(obj, TextInput::VALIDATOR.index)) {
            Value::Object(v) if engine.is_alive(v) => Some(v),
            _ => None,
        }
    } else {
        None
    };
    // The verdict needs the engine, which must not be called with the
    // state borrowed: the edit is made, judged, and taken back if refused.
    let (changed, undo_point) = {
        let mut s = st.borrow_mut();
        let s = &mut *s;
        let start = how.start.unwrap_or_else(|| s.model.cursor());
        let joins = how.kind != EditKind::Other
            && !s.model.has_selection()
            && s.run
                .is_some_and(|run| run.kind == how.kind && (how.anywhere || run.end == start));
        let point = validator.map(|_| s.model.checkpoint());
        if !joins {
            s.model.break_undo();
        }
        super::rich::before_edit(s, how.start, !joins);
        (f(&mut s.model), point)
    };
    let changed = match (validator, undo_point) {
        (Some(v), Some(point)) if changed => {
            let text = st.borrow().model.text().to_owned();
            if validate(engine, v, &text) == Validation::Invalid {
                st.borrow_mut().model.rollback(point);
                false
            } else {
                true
            }
        }
        _ => changed,
    };
    {
        let mut s = st.borrow_mut();
        super::rich::after_edit(&mut s);
        if changed {
            let end = how.end.unwrap_or_else(|| s.model.cursor());
            s.run = Some(Run {
                kind: how.kind,
                end,
            });
        }
        s.sticky = None;
    }
    check(engine, "edit", publish(engine, obj, &st));
    if changed && how.emits && single {
        check(
            engine,
            "textEdited",
            engine.emit_by_name(obj, "textEdited", &[]),
        );
    }
    changed
}

/// `PasswordEchoOnEdit`: the first thing the user does to a text that is
/// shown masked starts it over.
pub(crate) fn begin_editing(engine: &Engine, obj: ObjectId) {
    let Some(st) = ready(engine, obj) else {
        return;
    };
    if echo(engine, obj, &st) != echo_mode::PASSWORD_ECHO_ON_EDIT
        || read_only(engine, obj)
        || std::mem::replace(&mut st.borrow_mut().editing, true)
    {
        return;
    }
    edit(engine, obj, How::user(EditKind::Other), |m| {
        m.select_all();
        m.delete_selection()
    });
    // What comes next is typed into the empty field and is a step of its
    // own.
    st.borrow_mut().run = None;
}

/// Types `text` at the cursor as the user would: over the selection, or
/// over what follows the cursor in `overwriteMode`.
pub(crate) fn type_text(engine: &Engine, obj: ObjectId, text: &str) -> bool {
    let overwrite = flag(engine, obj, TextEditor::OVERWRITE_MODE.index);
    edit(engine, obj, How::user(EditKind::Typed), |m| {
        if overwrite {
            m.overwrite(text)
        } else {
            m.replace_selection(text)
        }
    })
}

/// Types `text` into an editor as the user would, for a host that has
/// text without keys and for a control built on an editor. A read-only
/// editor takes none. Returns whether the text changed.
pub fn accept_text(engine: &Engine, item: ObjectId, text: &str) -> bool {
    if state_of(engine, item).is_none() || read_only(engine, item) || text.is_empty() {
        return false;
    }
    begin_editing(engine, item);
    type_text(engine, item, text)
}

/// Whether the last thing that changed the text of an editor was typing,
/// and not a key that takes characters away, a paste or a program: what
/// a control asks that completes a text while it is typed.
pub fn last_edit_typed(engine: &Engine, item: ObjectId) -> bool {
    state_of(engine, item).is_some_and(|st| {
        st.borrow()
            .run
            .is_some_and(|run| matches!(run.kind, EditKind::Typed))
    })
}

/// `insert(position, text)` from a program. It works on a read-only
/// editor too.
pub(crate) fn program_insert(engine: &Engine, obj: ObjectId, pos: i32, text: &str) {
    let Some(st) = ready(engine, obj) else {
        return;
    };
    let Some(pos) = super::base::position(&st, pos) else {
        return;
    };
    let single = !st.borrow().multi;
    // Rich text is inserted as the editor reads its text: `insert(0,
    // "<b>x</b>")` inserts a bold "x".
    let fragment = super::rich::fragment(&st, text);
    let text = fragment.as_ref().map_or(text, |f| f.text()).to_owned();
    let how = How {
        kind: EditKind::Program,
        user: false,
        emits: true,
        start: Some(pos),
        end: Some(pos + utf16_len(&text)),
        // A field's program edits in a row are one step; in a TextEdit
        // only insertions that follow each other are.
        anywhere: single,
    };
    if let (Some(fragment), Some(r)) = (fragment.clone(), st.borrow_mut().rich.as_mut()) {
        r.pending = Some(fragment);
    }
    edit(engine, obj, how, |m| m.insert(pos, &text));
}

/// `remove(start, end)` from a program, positions in either order and cut
/// to the text.
pub(crate) fn program_remove(engine: &Engine, obj: ObjectId, start: i32, end: i32) {
    let Some(st) = ready(engine, obj) else {
        return;
    };
    let (single, len) = {
        let s = st.borrow();
        (!s.multi, s.model.len())
    };
    let at = |p: i32| usize::try_from(p).unwrap_or(0).min(len);
    let (a, b) = (at(start.min(end)), at(start.max(end)));
    let how = How {
        kind: if single {
            EditKind::Program
        } else {
            EditKind::Other
        },
        user: false,
        emits: true,
        start: Some(a),
        end: Some(a),
        anywhere: single,
    };
    edit(engine, obj, how, |m| m.remove(a, b));
}

/// `clear()`: unlike assigning an empty text it can be undone, and unlike
/// an edit it is not the user's.
pub(crate) fn clear(engine: &Engine, obj: ObjectId) {
    let how = How {
        kind: EditKind::Other,
        user: false,
        emits: false,
        start: None,
        end: None,
        anywhere: false,
    };
    edit(engine, obj, how, |m| {
        m.select_all();
        m.delete_selection()
    });
}

/// Whether the selection may leave the editor: a field that hides its
/// text does not hand it to the clipboard (doc.qt.io, `TextInput.copy`).
fn may_copy(engine: &Engine, obj: ObjectId, st: &SharedState) -> bool {
    echo(engine, obj, st) == echo_mode::NORMAL
}

pub(crate) fn copy(engine: &Engine, obj: ObjectId) -> bool {
    let Some(st) = ready(engine, obj) else {
        return false;
    };
    if !may_copy(engine, obj, &st) {
        return false;
    }
    let text = st.borrow().model.selected_text().to_owned();
    if text.is_empty() {
        return false;
    }
    let html = super::rich::copied_html(&mut st.borrow_mut());
    Clipboard::of(engine).copy(engine, &text);
    // The HTML goes beside the characters, to the service only: a host
    // that hands copies on is handed the characters.
    if let Some(html) = html {
        let _ = ljos_platform::Platform::of(engine)
            .clipboard()
            .set_html(&html, &text);
    }
    true
}

pub(crate) fn cut(engine: &Engine, obj: ObjectId) -> bool {
    if read_only(engine, obj) || !copy(engine, obj) {
        return false;
    }
    edit(engine, obj, How::user(EditKind::Other), |m| {
        m.delete_selection()
    })
}

pub(crate) fn paste(engine: &Engine, obj: ObjectId) -> bool {
    if read_only(engine, obj) {
        return false;
    }
    let Some(st) = ready(engine, obj) else {
        return false;
    };
    // Rich text pastes the HTML on the clipboard with its formats; what
    // has only characters takes the format where it goes.
    let rich = st.borrow().rich.is_some();
    let fragment = rich
        .then(|| ljos_platform::Platform::of(engine).clipboard().html())
        .flatten()
        .and_then(|html| super::rich::pasted(&st, &html));
    let text = match &fragment {
        Some(f) => f.text().to_owned(),
        None => Clipboard::of(engine).text(),
    };
    if text.is_empty() {
        return false;
    }
    begin_editing(engine, obj);
    if let (Some(fragment), Some(r)) = (fragment, st.borrow_mut().rich.as_mut()) {
        r.pending = Some(fragment);
    }
    edit(engine, obj, How::user(EditKind::Other), |m| {
        m.replace_selection(&text)
    })
}

fn history(
    engine: &Engine,
    obj: ObjectId,
    undo: bool,
    f: impl FnOnce(&mut TextModel) -> bool,
) -> bool {
    if read_only(engine, obj) {
        return false;
    }
    let Some(st) = ready(engine, obj) else {
        return false;
    };
    let changed = {
        let mut s = st.borrow_mut();
        s.run = None;
        s.sticky = None;
        let changed = f(&mut s.model);
        if changed {
            super::rich::history_swap(&mut s, undo);
        }
        changed
    };
    check(engine, "undo", publish(engine, obj, &st));
    if changed && !st.borrow().multi {
        check(
            engine,
            "textEdited",
            engine.emit_by_name(obj, "textEdited", &[]),
        );
    }
    changed
}

pub(crate) fn undo(engine: &Engine, obj: ObjectId) -> bool {
    history(engine, obj, true, TextModel::undo)
}

pub(crate) fn redo(engine: &Engine, obj: ObjectId) -> bool {
    history(engine, obj, false, TextModel::redo)
}
