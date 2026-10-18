//! `TextInput` and `TextEdit`: text the user edits (ARCHITECTURE §7.5, §11).
//!
//! Both are a [`TextEditor`], which keeps the text in a
//! [`ljos_text::TextModel`] (cursor, selection, undo history) and lays it
//! out with [`ljos_text::EditLayout`], where every cursor position has a
//! place. `TextInput` is one line and adds echo modes, a validator and
//! `accepted`; `TextEdit` is many lines and adds wrapping and movement by
//! line.
//!
//! Nothing here runs unless something happens: a key, the pointer, an input
//! method, a property change, or the tick of the cursor's blink timer,
//! which only runs while a field with a visible cursor has active focus.
//!
//! Behaviour follows doc.qt.io (`qml-qtquick-textinput.html`,
//! `qml-qtquick-textedit.html`) and what `features/text-input/` pins down.
//!
//! A program colours stretches of an editor's text without owning its
//! layout: [`set_highlights`], [`set_highlighter`] and, from QML, the
//! `textDocument` of a `TextEdit`.
//!
//! A `TextEdit` with `textFormat: RichText` or `MarkdownText` edits text
//! with formats ([`rich`]): typing takes the format of the text before it,
//! undo takes formats back, `cursorSelection` ([`TextSelection`]) formats
//! the selection, and its `textDocument` reads and saves files ([`files`]).
//!
//! Not implemented: `inputMask`, rich text in a `TextInput`, wrapping in a
//! `TextInput`, and the `Qt.inputMethod` object.

mod a11y;
mod base;
mod clipboard;
mod edit;
mod files;
mod highlight;
mod ime;
mod input;
mod keys;
mod layout;
mod ops;
mod paint;
mod pointer;
mod rich;
mod selection;
mod validator;

use std::rc::Rc;

use ljos_engine::{Engine, MetaRef, QmlType, Version};

use crate::enums::{EnumTable, align, wrap_mode};
use crate::keys::{KeyItem, register_key_item, register_text_input_client};
use crate::pointer_item::{PointerItem, register_hover_move, register_pointer_item};

pub use a11y::AccessibleTextAction;
pub use base::TextEditor;
pub use clipboard::Clipboard;
pub use edit::TextEdit;
pub use highlight::{
    HighlightSpan, Highlighter, TextDocument, highlights, set_highlighter, set_highlights,
};
pub use ime::{ImeState, PlatformTextInput};
pub use input::TextInput;
pub use ops::accept_text;
pub use ops::last_edit_typed;
pub use paint::EditorPaint;
pub use selection::TextSelection;
pub use validator::{
    DoubleValidator, IntValidator, RegularExpressionValidator, Validation, validate,
};

pub(crate) use base::{init_editor as init, refresh};

/// `TextInput.echoMode`.
pub mod echo_mode {
    pub const NORMAL: i32 = 0;
    pub const NO_ECHO: i32 = 1;
    pub const PASSWORD: i32 = 2;
    pub const PASSWORD_ECHO_ON_EDIT: i32 = 3;
}

/// `mouseSelectionMode`.
pub mod selection_mode {
    pub const SELECT_CHARACTERS: i32 = 0;
    pub const SELECT_WORDS: i32 = 1;
}

/// The `position` argument of `TextInput.positionAt`.
pub mod cursor_position {
    pub const BETWEEN_CHARACTERS: i32 = 0;
    pub const ON_CHARACTER: i32 = 1;
}

/// `TextEdit.textFormat`.
pub mod text_format {
    pub const PLAIN_TEXT: i32 = 0;
    pub const RICH_TEXT: i32 = 1;
    pub const AUTO_TEXT: i32 = 2;
    pub const MARKDOWN_TEXT: i32 = 3;
    /// `Text` only: an editor has no styled text.
    pub const STYLED_TEXT: i32 = 4;
}

/// `Qt.InputMethodHint` flags an editor adds to `inputMethodHints` itself.
pub mod input_method_hint {
    pub const HIDDEN_TEXT: i32 = 0x1;
    pub const SENSITIVE_DATA: i32 = 0x2;
    pub const NO_AUTO_UPPERCASE: i32 = 0x4;
    pub const NO_PREDICTIVE_TEXT: i32 = 0x40;
    pub const MULTI_LINE: i32 = 0x400;
    pub const DIGITS_ONLY: i32 = 0x10000;
    pub const FORMATTED_NUMBERS_ONLY: i32 = 0x20000;
}

fn keys(list: &[(&'static str, i32)]) -> Vec<(&'static str, i64)> {
    list.iter().map(|(k, v)| (*k, i64::from(*v))).collect()
}

/// The enums of `TextInput` and `TextEdit` (`TextInput.Password`,
/// `TextEdit.Wrap`).
pub(crate) fn editor_enums() -> EnumTable {
    vec![
        (
            "HAlignment",
            keys(&[
                ("AlignLeft", align::LEFT),
                ("AlignRight", align::RIGHT),
                ("AlignHCenter", align::H_CENTER),
                ("AlignJustify", align::JUSTIFY),
            ]),
        ),
        (
            "VAlignment",
            keys(&[
                ("AlignTop", align::TOP),
                ("AlignBottom", align::BOTTOM),
                ("AlignVCenter", align::V_CENTER),
            ]),
        ),
        (
            "WrapMode",
            keys(&[
                ("NoWrap", wrap_mode::NO_WRAP),
                ("WordWrap", wrap_mode::WORD_WRAP),
                ("WrapAnywhere", wrap_mode::WRAP_ANYWHERE),
                ("WrapAtWordBoundaryOrAnywhere", wrap_mode::WRAP),
                ("Wrap", wrap_mode::WRAP),
            ]),
        ),
        (
            "EchoMode",
            keys(&[
                ("Normal", echo_mode::NORMAL),
                ("NoEcho", echo_mode::NO_ECHO),
                ("Password", echo_mode::PASSWORD),
                ("PasswordEchoOnEdit", echo_mode::PASSWORD_ECHO_ON_EDIT),
            ]),
        ),
        (
            "SelectionMode",
            keys(&[
                ("SelectCharacters", selection_mode::SELECT_CHARACTERS),
                ("SelectWords", selection_mode::SELECT_WORDS),
            ]),
        ),
        (
            "CursorPosition",
            keys(&[
                (
                    "CursorBetweenCharacters",
                    cursor_position::BETWEEN_CHARACTERS,
                ),
                ("CursorOnCharacter", cursor_position::ON_CHARACTER),
            ]),
        ),
        (
            "TextFormat",
            keys(&[
                ("PlainText", text_format::PLAIN_TEXT),
                ("RichText", text_format::RICH_TEXT),
                ("AutoText", text_format::AUTO_TEXT),
                ("MarkdownText", text_format::MARKDOWN_TEXT),
            ]),
        ),
        (
            "RenderType",
            keys(&[
                ("QtRendering", 0),
                ("NativeRendering", 1),
                ("CurveRendering", 2),
            ]),
        ),
    ]
}

/// The editor's own key handling, for a type derived from it that
/// registers hooks of its own and wants these underneath.
pub fn key_hooks() -> KeyItem {
    KeyItem {
        shortcut_override: keys::shortcut_override,
        press: keys::press,
        release: |_, _, _| false,
    }
}

/// The editor's own pointer handling, for a type derived from it that
/// registers hooks of its own (a control that also tracks hovering) and
/// wants these underneath.
pub fn pointer_hooks() -> PointerItem {
    PointerItem {
        press: pointer::press,
        moved: pointer::moved,
        release: pointer::release,
        cancel: pointer::cancel,
        // Over rich text, `hoveredLink` follows the pointer.
        hover: rich::hover,
    }
}

/// The editor types that say themselves how large they would like to be.
#[derive(Default)]
struct OwnImplicitSize {
    types: std::cell::RefCell<Vec<MetaRef>>,
}

/// Makes editors of type `meta` (and of the types derived from it) leave
/// `implicitWidth` and `implicitHeight` alone: a control whose style
/// computes them from the editor's `contentWidth`, its padding and its
/// background.
pub fn register_own_implicit_size(engine: &Engine, meta: MetaRef) {
    engine
        .extension::<OwnImplicitSize>()
        .types
        .borrow_mut()
        .push(meta);
}

/// What the `width` of an editor that follows its implicit size is. A
/// one-line editor is as wide as its text to the fraction, though its
/// implicit width is rounded up, as Qt's is while nothing binds to it.
pub(crate) fn followed_width(engine: &Engine, obj: ljos_engine::ObjectId) -> ljos_engine::Value {
    let implicit = engine.read(obj, crate::item::Item::IMPLICIT_WIDTH.index);
    // Read for the dependency: a new text may round up to the same width.
    engine.read(obj, TextEditor::CONTENT_WIDTH.index);
    let natural = base::state_of(engine, obj).and_then(|st| st.borrow().natural_width);
    match natural {
        Some((given, exact)) if given == implicit.to_number() => ljos_engine::Value::Real(exact),
        _ => implicit,
    }
}

/// Whether `obj`'s type was given to [`register_own_implicit_size`].
pub(crate) fn has_own_implicit_size(engine: &Engine, obj: ljos_engine::ObjectId) -> bool {
    let reg = engine.extension::<OwnImplicitSize>();
    let types = reg.types.borrow();
    if types.is_empty() {
        return false;
    }
    engine
        .meta_of(obj)
        .is_some_and(|meta| types.iter().any(|m| meta.inherits(m)))
}

#[derive(Default)]
struct ClipToPadding {
    types: std::cell::RefCell<Vec<MetaRef>>,
}

/// Makes editors of type `meta` (and of the types derived from it) draw
/// their text only inside their padding: a control whose background
/// shows around the text.
pub fn register_clip_to_padding(engine: &Engine, meta: MetaRef) {
    engine
        .extension::<ClipToPadding>()
        .types
        .borrow_mut()
        .push(meta);
}

/// Whether `obj`'s type was given to [`register_clip_to_padding`].
pub(crate) fn clips_to_padding(engine: &Engine, obj: ljos_engine::ObjectId) -> bool {
    let reg = engine.extension::<ClipToPadding>();
    let types = reg.types.borrow();
    !types.is_empty()
        && engine
            .meta_of(obj)
            .is_some_and(|meta| types.iter().any(|m| meta.inherits(m)))
}

pub(crate) fn register(engine: &Engine) {
    for ty in [
        TextInput::static_meta(),
        TextEdit::static_meta(),
        // Not created from QML, and known by name all the same: what
        // `textDocument` gives has members a file is checked against.
        TextDocument::static_meta(),
        TextSelection::static_meta(),
        IntValidator::static_meta(),
        DoubleValidator::static_meta(),
        RegularExpressionValidator::static_meta(),
    ] {
        engine.register_type("QtQuick", Version::new(2, 15), MetaRef::Static(ty));
    }
    let meta = || MetaRef::Static(TextEditor::static_meta());
    register_key_item(engine, meta(), key_hooks());
    register_pointer_item(engine, meta(), pointer_hooks());
    register_hover_move(
        engine,
        MetaRef::Static(TextEdit::static_meta()),
        rich::hover_moved,
    );
    register_text_input_client(engine, meta(), Rc::new(ime::Client));
    a11y::register(engine);
}
