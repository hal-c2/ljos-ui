//! `TextArea` (QtQuick.Templates): a `TextEdit` with a background and a
//! placeholder, which takes its font and its colours from the controls
//! it is inside.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-textarea.html;
//! pinned by `features/controls/text-area.feature`. What it shares with
//! `TextField` is in [`crate::text_control`].

use std::rc::Rc;
use std::sync::OnceLock;

use ljos_engine::{Atom, Color, Cx, EngineError, ObjectId, QmlObject, qml_impl};
use ljos_quick::TextEdit;
use ljos_quick::focus::FocusReason;

use crate::inherit::{InheritState, Kind};
use crate::palette::Palette;
use crate::text_control::{self, FieldState, Props};

/// `TextArea`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "TextArea", extends = TextEdit)]
pub struct TextArea {
    #[qml(base)]
    pub edit: TextEdit,
    #[qml(notify)]
    pub background: Option<ObjectId>,
    #[qml(notify, reset)]
    pub top_inset: f64,
    #[qml(notify, reset)]
    pub left_inset: f64,
    #[qml(notify, reset)]
    pub right_inset: f64,
    #[qml(notify, reset)]
    pub bottom_inset: f64,
    #[qml(notify, readonly)]
    pub implicit_background_width: f64,
    #[qml(notify, readonly)]
    pub implicit_background_height: f64,
    /// Shown by the style while the area is empty.
    #[qml(notify)]
    pub placeholder_text: Atom,
    #[qml(notify)]
    pub placeholder_text_color: Color,
    #[qml(notify, readonly)]
    pub hovered: bool,
    #[qml(notify, reset)]
    pub hover_enabled: bool,
    /// Why the area last got or lost the keyboard (`Qt.FocusReason`).
    #[qml(notify, readonly, enumeration = "Qt.FocusReason")]
    pub focus_reason: i32,
    #[qml(group)]
    pub palette: Palette,
    #[qml(skip)]
    pub(crate) inherit: Rc<InheritState>,
    #[qml(skip)]
    pub(crate) state: Rc<FieldState>,
}

impl Default for TextArea {
    fn default() -> Self {
        let mut edit = TextEdit::default();
        // An area is a stop of the Tab key; a bare `TextEdit` is not.
        edit.editor.item.active_focus_on_tab = true;
        Self {
            edit,
            background: None,
            top_inset: 0.0,
            left_inset: 0.0,
            right_inset: 0.0,
            bottom_inset: 0.0,
            implicit_background_width: 0.0,
            implicit_background_height: 0.0,
            placeholder_text: Atom::EMPTY,
            placeholder_text_color: Color::TRANSPARENT,
            hovered: false,
            hover_enabled: true,
            focus_reason: FocusReason::Other as i32,
            palette: Palette::default(),
            inherit: Rc::default(),
            state: Rc::default(),
        }
    }
}

impl TextArea {
    pub(crate) fn props() -> &'static Props {
        static PROPS: OnceLock<Props> = OnceLock::new();
        PROPS.get_or_init(|| Props {
            kind: Kind::TextArea,
            background: Self::BACKGROUND.index,
            insets: [
                Self::LEFT_INSET.index,
                Self::TOP_INSET.index,
                Self::RIGHT_INSET.index,
                Self::BOTTOM_INSET.index,
            ],
            implicit_background: [
                Self::IMPLICIT_BACKGROUND_WIDTH.index,
                Self::IMPLICIT_BACKGROUND_HEIGHT.index,
            ],
            placeholder_text: Self::PLACEHOLDER_TEXT.index,
            hovered: Self::HOVERED.index,
            hover_enabled: Self::HOVER_ENABLED.index,
            focus_reason: Self::FOCUS_REASON.index,
        })
    }
}

#[qml_impl]
impl TextArea {
    /// A press landed on the area; `event` is a `MouseEvent`.
    #[qml(signal, name = "pressed")]
    fn press_signal(event: Option<ObjectId>);

    #[qml(signal)]
    fn released(event: Option<ObjectId>);

    /// The press stayed where it was for the platform's hold time.
    #[qml(signal, name = "pressAndHold")]
    fn press_and_hold(event: Option<ObjectId>);

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        text_control::completed(cx.engine, cx.this, Self::props())
    }
}
