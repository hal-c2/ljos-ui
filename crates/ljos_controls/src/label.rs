//! `Label` (QtQuick.Templates): a `Text` that takes its font and its
//! colours from the controls it is inside, and may have a background.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-label.html; pinned
//! by `features/controls/label.feature`.

use std::rc::Rc;

use ljos_engine::{Cx, Engine, EngineError, ObjectId, QmlObject, Value, qml_impl};
use ljos_quick::{Item, Text, is_item};

use crate::inherit::{self, InheritState, Kind};
use crate::palette::Palette;
use crate::parts::{PartKind, destroy_replaced_style_parts, manage_part};
use crate::util::{num, object};

/// `Label`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "Label", extends = Text)]
pub struct Label {
    #[qml(base)]
    pub text: Text,
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
    #[qml(group)]
    pub palette: Palette,
    #[qml(skip)]
    pub(crate) inherit: Rc<InheritState>,
}

#[qml_impl]
impl Label {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        let implicit_of = |size: ljos_engine::Prop<f64>| {
            move |e: &Engine| match object(e, obj, Label::BACKGROUND.index) {
                Some(p) if is_item(e, p) => Value::Real(num(e, p, size.index)),
                _ => Value::Real(0.0),
            }
        };
        e.bind_initial(
            obj,
            Label::IMPLICIT_BACKGROUND_WIDTH.index,
            implicit_of(Item::IMPLICIT_WIDTH),
        )?;
        e.bind_initial(
            obj,
            Label::IMPLICIT_BACKGROUND_HEIGHT.index,
            implicit_of(Item::IMPLICIT_HEIGHT),
        )?;
        destroy_replaced_style_parts(e, obj);
        manage_part(
            e,
            obj,
            Label::BACKGROUND.index,
            PartKind::Background(Some([
                Label::LEFT_INSET.index,
                Label::TOP_INSET.index,
                Label::RIGHT_INSET.index,
                Label::BOTTOM_INSET.index,
            ])),
        )?;
        inherit::attach(e, obj, Kind::Label, false)?;
        crate::a11y::label_defaults(e, obj)
    }
}
