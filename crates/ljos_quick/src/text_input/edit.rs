//! `TextEdit`: many lines of text the user edits (doc.qt.io,
//! `qml-qtquick-textedit.html`). The text is plain: `textFormat` is kept
//! and rich text is not implemented.

use ljos_engine::{Atom, Cx, Engine, EngineError, ObjectId, QmlObject, Value, qml_impl};

use super::base::{EditKind, SharedState, TextEditor};
use super::ops::{self, How};
use super::text_format;
use crate::item::is_unset;

/// `TextEdit` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "TextEdit", extends = TextEditor)]
pub struct TextEdit {
    #[qml(base)]
    pub editor: TextEditor,
    /// How many lines the text takes as it is laid out, wrapped lines
    /// counted each.
    #[qml(notify, readonly)]
    pub line_count: i32,
    /// Kept; the text is always plain.
    #[qml(notify, enumeration = "TextFormat")]
    pub text_format: i32,
    /// Kept; a tab is as wide as the font makes it.
    #[qml(notify)]
    pub tab_stop_distance: f64,
    /// Space around the text, on every side.
    #[qml(notify)]
    pub text_margin: f64,
    /// Whether the keys move the cursor and select. Until it is assigned
    /// it is what `readOnly` is not.
    #[qml(notify)]
    pub select_by_keyboard: bool,
    /// Always empty: plain text has no links.
    #[qml(notify, readonly)]
    pub hovered_link: Atom,
    /// Kept; nothing is loaded relative to it.
    #[qml(notify)]
    pub base_url: Atom,
}

impl Default for TextEdit {
    fn default() -> Self {
        Self {
            editor: TextEditor::new(true),
            line_count: 1,
            text_format: text_format::PLAIN_TEXT,
            tab_stop_distance: 80.0,
            text_margin: 0.0,
            select_by_keyboard: true,
            hovered_link: Atom::EMPTY,
            base_url: Atom::EMPTY,
        }
    }
}

#[qml_impl]
impl TextEdit {
    /// Declared for programs that connect to it; plain text has no links.
    #[qml(signal, name = "linkActivated")]
    fn link_activated(link: String);

    #[qml(signal, name = "linkHovered")]
    fn link_hovered(link: String);

    /// `append(text)`: a new paragraph at the end.
    #[qml(invokable)]
    fn append(cx: &Cx, text: Value) {
        let Some(st) = ops::ready(cx.engine, cx.this) else {
            return;
        };
        let (len, empty) = {
            let s = st.borrow();
            (s.model.len(), s.model.is_empty())
        };
        let mut added = String::new();
        if !empty {
            added.push('\n');
        }
        added.push_str(text.to_js_string().as_str());
        let how = How {
            kind: EditKind::Other,
            user: false,
            emits: false,
            start: Some(len),
            end: None,
            anywhere: false,
        };
        ops::edit(cx.engine, cx.this, how, |m| m.insert(len, &added));
    }

    /// `remove(start, end)`: positions in either order, cut to the text.
    /// Returns what was removed.
    #[qml(invokable)]
    fn remove(cx: &Cx, start: i32, end: i32) -> String {
        ops::program_remove(cx.engine, cx.this, start, end)
    }

    /// `getFormattedText(start, end)`: plain text has no other form.
    #[qml(invokable, name = "getFormattedText")]
    fn get_formatted_text(cx: &Cx, start: i32, end: i32) -> String {
        let Some(st) = ops::ready(cx.engine, cx.this) else {
            return String::new();
        };
        let s = st.borrow();
        let at = |p: i32| usize::try_from(p).unwrap_or(0);
        s.model.slice(at(start), at(end)).to_owned()
    }
}

impl TextEdit {
    /// What a `TextEdit` adds when an editor completes.
    pub(crate) fn init(
        engine: &Engine,
        obj: ObjectId,
        _st: &SharedState,
    ) -> Result<(), EngineError> {
        if is_unset(engine, obj, TextEdit::SELECT_BY_KEYBOARD.index) {
            engine.bind(obj, TextEdit::SELECT_BY_KEYBOARD.index, move |e| {
                Value::Bool(!e.read(obj, TextEditor::READ_ONLY.index).to_boolean())
            })?;
        }
        Ok(())
    }
}
