//! An editor and an assistive technology (ARCHITECTURE §14): what the
//! editor says it holds, and what it does when asked to select or to
//! change its text.
//!
//! A field that hides its text says what it shows: the mask, never the
//! text. That holds while `PasswordEchoOnEdit` shows the eye what is being
//! typed, because a screen reader reads it out loud.

use ljos_a11y::accesskit::Role as AkRole;
use ljos_a11y::{NodeData, NodeKey, Role, TextValue};
use ljos_engine::{Engine, EngineError, MetaRef, ObjectId, QmlType, Value};
use ljos_text::{byte_to_utf16, utf16_to_byte};

use super::base::{EditKind, TextEditor, state_of};
use super::echo_mode;
use super::input::TextInput;
use super::ops::{self, How};
use crate::accessible::{
    Accessible, AccessibleAction, AccessibleAttached, AccessibleItem, register_accessible_item,
};
use crate::item::is_unset;
use crate::scene::QuickScene;

/// What an assistive technology asks of a node that holds text. Places
/// are bytes of the node's [`TextValue::value`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessibleTextAction {
    /// Select from `anchor` to `cursor`, which may be before it; the same
    /// place twice only moves the cursor.
    Select { anchor: usize, cursor: usize },
    /// Type this over the selection.
    ReplaceSelection(String),
    /// Type this over the whole text.
    SetValue(String),
}

/// An editor is editable text unless its author says what else it is.
pub(super) fn default_role(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let att = engine.attached_object(obj, &MetaRef::Static(Accessible::static_meta()))?;
    let role = AccessibleAttached::ROLE.index;
    if is_unset(engine, att, role) {
        // Derived: the author's later `Accessible.role` is still the
        // first assignment.
        engine.write_derived(att, role, Value::Int(Role::EditableText.qt_value()))?;
    }
    Ok(())
}

/// Whether the editor shows its text, and the mask it shows otherwise.
/// Tracked: the node follows `echoMode`.
fn mask(e: &Engine, obj: ObjectId, multi: bool) -> Option<String> {
    if multi {
        return None;
    }
    let echo = e.read(obj, TextInput::ECHO_MODE.index).to_number() as i32;
    match echo {
        echo_mode::NORMAL => None,
        echo_mode::NO_ECHO => Some(String::new()),
        _ => Some(
            e.read(obj, TextInput::PASSWORD_CHARACTER.index)
                .to_js_string()
                .chars()
                .next()
                .map(String::from)
                .unwrap_or_default(),
        ),
    }
}

fn describe(e: &Engine, obj: ObjectId, data: &mut NodeData) {
    // An editor its author gave another role is that, and says no more.
    if data.qt_role != Role::EditableText {
        return;
    }
    let Some(st) = state_of(e, obj) else {
        return;
    };
    let multi = st.borrow().multi;
    let position = |p| usize::try_from(e.read(obj, p).to_number() as i64).unwrap_or(0);
    let text = e.read(obj, TextEditor::TEXT.index).to_js_string();
    let start = position(TextEditor::SELECTION_START.index);
    let end = position(TextEditor::SELECTION_END.index);
    let cursor = position(TextEditor::CURSOR_POSITION.index);
    let anchor = if cursor == start { end } else { start };
    let read_only = e.read(obj, TextEditor::READ_ONLY.index).to_boolean();
    let mask = mask(e, obj, multi);
    data.role = match (&mask, multi) {
        (Some(_), _) => AkRole::PasswordInput,
        (None, true) => AkRole::MultilineTextInput,
        // What the author said of a field that shows its text stands.
        (None, false) if data.role == AkRole::SearchInput => AkRole::SearchInput,
        (None, false) => AkRole::TextInput,
    };
    data.read_only |= read_only;
    data.text = Some(match mask {
        // One mark for every unit of the text, as on the screen.
        Some(mark) => {
            let units = ljos_text::utf16_len(text.as_str());
            let at = |p: usize| p.min(units) * mark.len();
            TextValue {
                anchor: at(anchor),
                cursor: at(cursor),
                value: mark.repeat(units),
                read_only,
            }
        }
        None => TextValue {
            anchor: utf16_to_byte(text.as_str(), anchor),
            cursor: utf16_to_byte(text.as_str(), cursor),
            value: text.to_string(),
            read_only,
        },
    });
}

/// The position in the text of a byte of what the node says it holds.
fn position(engine: &Engine, obj: ObjectId, value: &TextValue, byte: usize) -> usize {
    let mut byte = byte.min(value.value.len());
    while !value.value.is_char_boundary(byte) {
        byte -= 1;
    }
    let masked =
        state_of(engine, obj).is_some_and(|st| ops::echo(engine, obj, &st) != echo_mode::NORMAL);
    if masked {
        // A mark stands for a unit of the text.
        value.value[..byte].chars().count()
    } else {
        byte_to_utf16(&value.value, byte)
    }
}

/// The editor does what an assistive technology asked. Returns whether it
/// took it.
fn act(engine: &Engine, obj: ObjectId, value: &TextValue, action: AccessibleTextAction) -> bool {
    match action {
        AccessibleTextAction::Select { anchor, cursor } => {
            let anchor = position(engine, obj, value, anchor);
            let cursor = position(engine, obj, value, cursor);
            ops::act(engine, obj, |st| {
                let mut s = st.borrow_mut();
                let len = s.model.len();
                s.model.select(anchor.min(len), cursor.min(len));
            });
            true
        }
        _ if value.read_only || ops::read_only(engine, obj) => false,
        // Both are the user's typing: a validator judges them, they can
        // be undone, and a `TextInput` says `textEdited`.
        AccessibleTextAction::ReplaceSelection(text) => {
            ops::begin_editing(engine, obj);
            ops::edit(engine, obj, How::user(EditKind::Other), |m| {
                m.replace_selection(&text)
            });
            true
        }
        AccessibleTextAction::SetValue(text) => {
            ops::edit(engine, obj, How::user(EditKind::Other), |m| {
                m.select_all();
                m.replace_selection(&text)
            });
            true
        }
    }
}

pub(super) fn register(engine: &Engine) {
    register_accessible_item(
        engine,
        MetaRef::Static(TextEditor::static_meta()),
        AccessibleItem {
            describe,
            act: |_: &Engine, _: ObjectId, _: AccessibleAction| false,
        },
    );
}

impl QuickScene {
    /// A request of an assistive technology to the text of a node: the
    /// editor behind it selects or types. Returns whether it took it; a
    /// disabled node takes nothing, a read-only one only a selection.
    pub fn accessible_text_action(
        &mut self,
        engine: &Engine,
        key: NodeKey,
        action: AccessibleTextAction,
    ) -> bool {
        let Some(value) = self
            .accessibility_tree()
            .get(key)
            .filter(|data| !data.disabled)
            .and_then(|data| data.text.clone())
        else {
            return false;
        };
        let Some(item) = self.accessible_item(key).filter(|i| engine.is_alive(*i)) else {
            return false;
        };
        state_of(engine, item).is_some() && act(engine, item, &value, action)
    }
}
