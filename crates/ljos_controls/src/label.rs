//! `Label` (QtQuick.Templates): a `Text` that takes its font and its
//! colours from the controls it is inside, and may have a background.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-label.html; pinned
//! by `features/controls/label.feature`.

use std::rc::Rc;

use ljos_engine::{Cx, Engine, EngineError, ObjectId, QmlObject, QmlType, Value, qml_impl};
use ljos_quick::focus::{FocusReason, force_active_focus};
use ljos_quick::{
    Item, PointerEvent, PointerItem, Text, is_item, register_pointer_condition,
    register_pointer_item,
};
use ljos_render::kurbo;

use crate::control::{focus_policy, is_left, takes_input};
use crate::inherit::{self, InheritState, Kind};
use crate::palette::Palette;
use crate::parts::{PartKind, manage_part, settle_parts};
use crate::util::{num, object};

/// `Label`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "Label", extends = Text)]
pub struct Label {
    #[qml(base)]
    pub text: Text,
    #[qml(notify, holds = "Item")]
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
    /// How the label takes the keyboard (`Qt.FocusPolicy`): not at all,
    /// unless it is told to.
    #[qml(notify, enumeration = "Qt.FocusPolicy")]
    pub focus_policy: i32,
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
        settle_parts(e, obj);
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
        crate::focus::link_policy(e, obj, Label::FOCUS_POLICY.index, false)?;
        crate::a11y::label_defaults(e, obj)
    }
}

/// Whether a click brings the keyboard to the label.
fn takes_clicks(engine: &Engine, obj: ObjectId) -> bool {
    let policy = engine.untrack(|| num(engine, obj, Label::FOCUS_POLICY.index)) as i32;
    policy & focus_policy::CLICK != 0
}

/// Measured on qml6: a label whose policy has `Qt.ClickFocus` takes the
/// press and the keyboard, and what is behind it gets no click.
fn press(engine: &Engine, obj: ObjectId, ev: &PointerEvent, _at: kurbo::Point) -> bool {
    if !(takes_input(engine, obj) && is_left(ev)) {
        return false;
    }
    force_active_focus(engine, obj, FocusReason::Mouse);
    true
}

fn ignore(_: &Engine, _: ObjectId, _: &PointerEvent, _: kurbo::Point) {}

fn ignore_cancel(_: &Engine, _: ObjectId) {}

fn ignore_hover(_: &Engine, _: ObjectId, _: bool) {}

/// A label is text that the pointer goes through, until its policy says a
/// click brings the keyboard to it.
pub(crate) fn register(engine: &Engine) {
    let meta = || ljos_engine::MetaRef::Static(Label::static_meta());
    register_pointer_item(
        engine,
        meta(),
        PointerItem {
            press,
            moved: ignore,
            release: ignore,
            cancel: ignore_cancel,
            hover: ignore_hover,
        },
    );
    register_pointer_condition(engine, meta(), takes_clicks);
}
