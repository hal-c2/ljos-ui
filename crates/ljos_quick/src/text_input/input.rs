//! `TextInput`: one line of text the user edits (doc.qt.io,
//! `qml-qtquick-textinput.html`).

use ljos_engine::{Atom, Cx, Engine, EngineError, ObjectId, QmlObject, Value, qml_impl};

use super::base::{SharedState, TextEditor, check, publish, state_of};
use super::echo_mode;
use super::layout::LayoutInputs;
use super::ops;
use super::validator::{Validation, validate};
use crate::item::write_computed;

/// What `maximumLength` is until it is assigned, as in Qt.
const MAXIMUM_LENGTH: i32 = 32767;

/// `TextInput` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "TextInput", extends = TextEditor)]
pub struct TextInput {
    #[qml(base)]
    pub editor: TextEditor,
    /// What is shown: the text, its mask, or nothing, with what an input
    /// method is composing.
    #[qml(notify, readonly)]
    pub display_text: Atom,
    #[qml(notify, enumeration = "EchoMode")]
    pub echo_mode: i32,
    #[qml(notify)]
    pub password_character: Atom,
    /// Kept; a typed character is masked at once.
    #[qml(notify)]
    pub password_mask_delay: i32,
    /// In UTF-16 units. A longer text is cut.
    #[qml(notify)]
    pub maximum_length: i32,
    /// An `IntValidator`, `DoubleValidator` or
    /// `RegularExpressionValidator`.
    #[qml(notify)]
    pub validator: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub acceptable_input: bool,
    #[qml(notify)]
    pub auto_scroll: bool,
    /// Kept and not applied (backlog).
    #[qml(notify)]
    pub input_mask: Atom,
}

impl Default for TextInput {
    fn default() -> Self {
        Self {
            editor: TextEditor::new(false),
            display_text: Atom::EMPTY,
            echo_mode: echo_mode::NORMAL,
            password_character: Atom::from("\u{2022}"),
            password_mask_delay: 0,
            maximum_length: MAXIMUM_LENGTH,
            validator: None,
            acceptable_input: true,
            auto_scroll: true,
            input_mask: Atom::EMPTY,
        }
    }
}

fn max_units(v: &Value) -> usize {
    // A negative length is no limit that makes sense; Qt takes it as none.
    usize::try_from(v.to_number() as i64).unwrap_or(usize::MAX)
}

#[qml_impl]
impl TextInput {
    /// Return or Enter was pressed and the text is acceptable.
    #[qml(signal)]
    fn accepted();

    /// The user changed the text; not emitted when a program assigns it.
    #[qml(signal, name = "textEdited")]
    fn text_edited();

    /// `remove(start, end)`: positions in either order, cut to the text.
    #[qml(invokable)]
    fn remove(cx: &Cx, start: i32, end: i32) {
        ops::program_remove(cx.engine, cx.this, start, end);
    }

    /// `ensureVisible(position)`: moves the text so that the position is
    /// inside the field. With `autoScroll` the cursor stays in view too,
    /// and wins where both do not fit.
    #[qml(invokable, name = "ensureVisible")]
    fn ensure_visible(cx: &Cx, position: i32) {
        let Some(st) = ops::ready(cx.engine, cx.this) else {
            return;
        };
        let Some(pos) = super::base::position(&st, position) else {
            return;
        };
        let (engine, obj) = (cx.engine, cx.this);
        let inputs = engine.untrack(|| LayoutInputs::read(engine, obj, false));
        let avail = inputs.available_width();
        {
            let mut s = st.borrow_mut();
            let s = &mut *s;
            let Some(layout) = s.layout.as_ref() else {
                return;
            };
            let x = layout.caret(s.shown.byte_of(pos)).x;
            if x - s.hscroll > avail {
                s.hscroll = x - avail;
            }
            if x - s.hscroll < 0.0 {
                s.hscroll = x;
            }
        }
        check(
            engine,
            "ensureVisible",
            engine.untrack(|| super::layout::relayout(engine, obj, &st, &inputs)),
        );
    }
}

impl TextInput {
    /// What a `TextInput` adds when an editor completes, before the model
    /// takes the declared text.
    pub(crate) fn init(
        engine: &Engine,
        obj: ObjectId,
        st: &SharedState,
    ) -> Result<(), EngineError> {
        let max = engine.untrack(|| engine.read(obj, TextInput::MAXIMUM_LENGTH.index));
        st.borrow_mut().model.set_max_length(max_units(&max));
        engine.watch(obj, TextInput::MAXIMUM_LENGTH.index, move |e, v| {
            if let Some(st) = state_of(e, obj) {
                st.borrow_mut().model.set_max_length(max_units(v));
                check(e, "maximumLength", publish(e, obj, &st));
            }
        })?;
        // A text that stops following its cursor goes back to where it
        // starts (the oracle shows the cursor at the far end of an
        // unscrolled text then).
        engine.watch(obj, TextInput::AUTO_SCROLL.index, move |e, v| {
            if let Some(st) = state_of(e, obj) {
                if !v.to_boolean() {
                    st.borrow_mut().hscroll = 0.0;
                }
                check(e, "autoScroll", publish(e, obj, &st));
            }
        })?;
        // `acceptableInput` follows the text and what the validator
        // allows; a text the program assigned may be one it refuses.
        engine.effect_in(obj, move |e| {
            let text = e.read(obj, TextEditor::TEXT.index).to_js_string();
            let ok = match e.read(obj, TextInput::VALIDATOR.index) {
                Value::Object(v) => validate(e, v, text.as_str()) == Validation::Acceptable,
                _ => true,
            };
            check(
                e,
                "acceptableInput",
                e.untrack(|| {
                    write_computed(e, obj, TextInput::ACCEPTABLE_INPUT.index, Value::Bool(ok))
                }),
            );
        })?;
        Ok(())
    }
}
