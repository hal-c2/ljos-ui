//! `TextEdit`: many lines of text the user edits (doc.qt.io,
//! `qml-qtquick-textedit.html`). With `textFormat: TextEdit.RichText` or
//! `MarkdownText` the text has formats ([`super::rich`]): `text` reads back
//! as HTML or Markdown, `cursorSelection` formats the selection, links are
//! hovered and activated, and images are shown. A program colours stretches
//! of the text through `textDocument` (see [`super::highlight`]), which also
//! reads and saves files.

use ljos_engine::{Atom, Cx, Engine, EngineError, ObjectId, QmlObject, Url, Value, qml_impl};
use ljos_text::utf16_to_byte;

use super::base::{EditKind, SharedState, TextEditor, check, publish, state_of};
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
    /// The size the text takes, past the item where wrapping cannot keep
    /// it inside: what `contentWidth` and `contentHeight` are.
    #[qml(notify, readonly)]
    pub painted_width: f64,
    #[qml(notify, readonly)]
    pub painted_height: f64,
    /// `PlainText`, `RichText`, `MarkdownText`, or `AutoText`, which reads
    /// a text that looks like markup as rich text.
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
    /// The link of rich text under the pointer; empty over plain text.
    #[qml(notify, readonly)]
    pub hovered_link: Atom,
    /// What the images of rich text are found relative to.
    #[qml(notify, reset = crate::text::BASE_URL_RESET)]
    pub base_url: Url,
    /// The [`super::highlight::TextDocument`], one for as long as the
    /// editor lives. It is made when the editor completes.
    #[qml(notify, readonly)]
    pub text_document: Option<ObjectId>,
    /// The [`super::selection::TextSelection`]: the selection's format.
    #[qml(notify, readonly, object = "TextSelection")]
    pub cursor_selection: Option<ObjectId>,
}

impl Default for TextEdit {
    fn default() -> Self {
        Self {
            editor: TextEditor::new(true),
            line_count: 1,
            painted_width: 0.0,
            painted_height: 0.0,
            text_format: text_format::PLAIN_TEXT,
            tab_stop_distance: 80.0,
            text_margin: 0.0,
            select_by_keyboard: true,
            hovered_link: Atom::EMPTY,
            base_url: Url::default(),
            text_document: None,
            cursor_selection: None,
        }
    }
}

#[qml_impl]
impl TextEdit {
    /// A link of rich text was clicked.
    #[qml(signal, name = "linkActivated")]
    fn link_activated(link: String);

    #[qml(signal, name = "linkHovered")]
    fn link_hovered(link: String);

    /// `linkAt(x, y)`: the address of the link of the rich text at a
    /// point of the item, or an empty string.
    #[qml(invokable, name = "linkAt")]
    fn link_at(cx: &Cx, x: f64, y: f64) -> String {
        ops::ready(cx.engine, cx.this)
            .and_then(|st| super::rich::link_at(&st, ljos_render::kurbo::Point::new(x, y)))
            .map(|href| href.to_string())
            .unwrap_or_default()
    }

    /// `append(text)`: a new paragraph at the end. Rich text is read as
    /// the editor reads its text.
    #[qml(invokable)]
    fn append(cx: &Cx, text: Value) {
        let Some(st) = ops::ready(cx.engine, cx.this) else {
            return;
        };
        let (len, empty) = {
            let s = st.borrow();
            (s.model.len(), s.model.is_empty())
        };
        let source = text.to_js_string();
        let mut added = String::new();
        let fragment = super::rich::fragment(&st, source.as_str());
        match &fragment {
            Some(f) if !empty => {
                let f = f.as_new_paragraphs();
                added.push_str(f.text());
                if let Some(r) = st.borrow_mut().rich.as_mut() {
                    r.pending = Some(f);
                }
            }
            Some(f) => {
                added.push_str(f.text());
                if let Some(r) = st.borrow_mut().rich.as_mut() {
                    r.pending = Some(f.clone());
                }
            }
            None => {
                if !empty {
                    added.push('\n');
                }
                added.push_str(source.as_str());
            }
        }
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
    /// Measured: it gives nothing back, though the documentation has it
    /// return a string.
    #[qml(invokable)]
    fn remove(cx: &Cx, start: i32, end: i32) {
        ops::program_remove(cx.engine, cx.this, start, end);
    }

    /// `getFormattedText(start, end)`: that part of rich text as HTML or
    /// Markdown; of plain text, the text.
    #[qml(invokable, name = "getFormattedText")]
    fn get_formatted_text(cx: &Cx, start: i32, end: i32) -> String {
        let Some(st) = ops::ready(cx.engine, cx.this) else {
            return String::new();
        };
        super::rich::refresh(cx.engine, cx.this, &st);
        let s = st.borrow();
        let at = |p: i32| usize::try_from(p).unwrap_or(0).min(s.model.len());
        let (a, b) = (at(start.min(end)), at(start.max(end)));
        let Some(r) = s.rich.as_ref() else {
            return s.model.slice(a, b).to_owned();
        };
        let text = s.model.text();
        let range = utf16_to_byte(text, a)..utf16_to_byte(text, b);
        match r.format {
            ljos_text::rich::TextFormat::Markdown => r.doc.markdown_fragment(range),
            _ => r.doc.html_fragment(range),
        }
    }
}

impl TextEdit {
    /// What a `TextEdit` adds when an editor completes.
    pub(crate) fn init(
        engine: &Engine,
        obj: ObjectId,
        st: &SharedState,
    ) -> Result<(), EngineError> {
        super::highlight::init_document(engine, obj, st)?;
        super::selection::init(engine, obj, st)?;
        if is_unset(engine, obj, TextEdit::SELECT_BY_KEYBOARD.index) {
            engine.bind(obj, TextEdit::SELECT_BY_KEYBOARD.index, move |e| {
                Value::Bool(!e.read(obj, TextEditor::READ_ONLY.index).to_boolean())
            })?;
        }
        // The text is read again in the new format.
        engine.watch(obj, TextEdit::TEXT_FORMAT.index, move |e, _| {
            let Some(st) = ops::ready(e, obj) else { return };
            let text = e
                .untrack(|| e.read(obj, TextEditor::TEXT.index))
                .to_js_string();
            let (format, font) = super::rich::inputs(e, obj, true, text.as_str());
            {
                let mut s = st.borrow_mut();
                let s = &mut *s;
                super::rich::assign(s, format, text.as_str(), &font);
                s.model.move_to(0, false);
                s.run = None;
                s.sticky = None;
            }
            check(e, "textFormat", publish(e, obj, &st));
        })?;
        // The HTML names the font, and the selection reads as it.
        for p in [TextEditor::FONT.index, TextEditor::COLOR.index] {
            engine.watch(obj, p, move |e, _| {
                let Some(st) = state_of(e, obj) else { return };
                if !st.borrow().initialised {
                    return;
                }
                super::rich::font_changed(&st);
                check(e, "font", publish(e, obj, &st));
            })?;
        }
        Ok(())
    }
}
