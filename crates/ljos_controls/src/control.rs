//! `Control` (QtQuick.Templates): what every control is.
//!
//! A control is an item with a `background` behind it and a `contentItem`
//! inside its padding, and it knows how big those two would like to be.
//! Behaviour follows doc.qt.io/qt-6/qml-qtquick-controls-control.html and
//! is pinned by `features/controls/control.feature`.

use std::rc::Rc;

use ljos_engine::{Cx, Engine, EngineError, Font, ObjectId, QmlObject, QmlType, Value, qml_impl};
use ljos_quick::{
    Item, MouseButtons, PointerEvent, PointerItem, is_mirrored, register_pointer_item,
};
use ljos_render::kurbo;

use crate::containers::{Page, Pane, page_bars};
use crate::inherit::{self, InheritState, Kind};
use crate::palette::Palette;
use crate::parts::{PartKind, destroy_replaced_style_parts, manage_part};
use crate::util::{bind_default, flag, is_a, num, object, set_computed};

/// `Control`.
///
/// TODO(focus): `focusPolicy`, `focusReason` and `visualFocus` are stored
/// and do nothing; a control does not take focus on a click or on Tab yet.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "Control", extends = Item)]
pub struct Control {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub padding: f64,
    #[qml(notify)]
    pub top_padding: f64,
    #[qml(notify)]
    pub left_padding: f64,
    #[qml(notify)]
    pub right_padding: f64,
    #[qml(notify)]
    pub bottom_padding: f64,
    #[qml(notify)]
    pub horizontal_padding: f64,
    #[qml(notify)]
    pub vertical_padding: f64,
    #[qml(notify)]
    pub top_inset: f64,
    #[qml(notify)]
    pub left_inset: f64,
    #[qml(notify)]
    pub right_inset: f64,
    #[qml(notify)]
    pub bottom_inset: f64,
    #[qml(notify)]
    pub spacing: f64,
    #[qml(notify, readonly)]
    pub available_width: f64,
    #[qml(notify, readonly)]
    pub available_height: f64,
    #[qml(notify, readonly)]
    pub implicit_background_width: f64,
    #[qml(notify, readonly)]
    pub implicit_background_height: f64,
    #[qml(notify, readonly)]
    pub implicit_content_width: f64,
    #[qml(notify, readonly)]
    pub implicit_content_height: f64,
    #[qml(notify)]
    pub background: Option<ObjectId>,
    #[qml(notify)]
    pub content_item: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub hovered: bool,
    #[qml(notify)]
    pub hover_enabled: bool,
    /// Stored: nothing built so far scrolls.
    #[qml(notify)]
    pub wheel_enabled: bool,
    #[qml(notify, readonly)]
    pub mirrored: bool,
    #[qml(notify)]
    pub font: Font,
    #[qml(group)]
    pub palette: Palette,
    /// Accepted and stored. TODO(i18n): a right-to-left locale should
    /// mirror the control.
    #[qml(notify)]
    pub locale: Value,
    #[qml(notify)]
    pub focus_policy: i32,
    #[qml(notify, readonly)]
    pub focus_reason: i32,
    #[qml(notify, readonly)]
    pub visual_focus: bool,
    #[qml(skip)]
    pub(crate) inherit: Rc<InheritState>,
}

impl Default for Control {
    fn default() -> Self {
        Self {
            item: Item::default(),
            padding: 0.0,
            top_padding: 0.0,
            left_padding: 0.0,
            right_padding: 0.0,
            bottom_padding: 0.0,
            horizontal_padding: 0.0,
            vertical_padding: 0.0,
            top_inset: 0.0,
            left_inset: 0.0,
            right_inset: 0.0,
            bottom_inset: 0.0,
            spacing: 0.0,
            available_width: 0.0,
            available_height: 0.0,
            implicit_background_width: 0.0,
            implicit_background_height: 0.0,
            implicit_content_width: 0.0,
            implicit_content_height: 0.0,
            background: None,
            content_item: None,
            hovered: false,
            hover_enabled: true,
            wheel_enabled: false,
            mirrored: false,
            font: Font::default(),
            palette: Palette::default(),
            locale: Value::Undefined,
            focus_policy: 0,
            focus_reason: 0,
            visual_focus: false,
            inherit: Rc::default(),
        }
    }
}

#[qml_impl]
impl Control {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_control(cx.engine, cx.this)
    }
}

fn init_control(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    // The more specific padding wins: a side, then the axis, then
    // `padding`. Each follows the next until it is given a value.
    let follow = |prop: ljos_engine::Prop<f64>, from: ljos_engine::Prop<f64>| {
        bind_default(engine, obj, prop.index, move |e| {
            Value::Real(num(e, obj, from.index))
        })
    };
    follow(Control::HORIZONTAL_PADDING, Control::PADDING)?;
    follow(Control::VERTICAL_PADDING, Control::PADDING)?;
    follow(Control::LEFT_PADDING, Control::HORIZONTAL_PADDING)?;
    follow(Control::RIGHT_PADDING, Control::HORIZONTAL_PADDING)?;
    follow(Control::TOP_PADDING, Control::VERTICAL_PADDING)?;
    follow(Control::BOTTOM_PADDING, Control::VERTICAL_PADDING)?;

    engine.bind_initial(obj, Control::AVAILABLE_WIDTH.index, move |e| {
        let w = num(e, obj, Item::WIDTH.index)
            - num(e, obj, Control::LEFT_PADDING.index)
            - num(e, obj, Control::RIGHT_PADDING.index);
        Value::Real(w.max(0.0))
    })?;
    engine.bind_initial(obj, Control::AVAILABLE_HEIGHT.index, move |e| {
        let h = num(e, obj, Item::HEIGHT.index)
            - num(e, obj, Control::TOP_PADDING.index)
            - num(e, obj, Control::BOTTOM_PADDING.index);
        Value::Real(h.max(0.0))
    })?;

    let implicit_of = |part: ljos_engine::Prop<Option<ObjectId>>, size: ljos_engine::Prop<f64>| {
        move |e: &Engine| match object(e, obj, part.index) {
            Some(p) if ljos_quick::is_item(e, p) => Value::Real(num(e, p, size.index)),
            _ => Value::Real(0.0),
        }
    };
    engine.bind_initial(
        obj,
        Control::IMPLICIT_BACKGROUND_WIDTH.index,
        implicit_of(Control::BACKGROUND, Item::IMPLICIT_WIDTH),
    )?;
    engine.bind_initial(
        obj,
        Control::IMPLICIT_BACKGROUND_HEIGHT.index,
        implicit_of(Control::BACKGROUND, Item::IMPLICIT_HEIGHT),
    )?;
    engine.bind_initial(
        obj,
        Control::IMPLICIT_CONTENT_WIDTH.index,
        implicit_of(Control::CONTENT_ITEM, Item::IMPLICIT_WIDTH),
    )?;
    engine.bind_initial(
        obj,
        Control::IMPLICIT_CONTENT_HEIGHT.index,
        implicit_of(Control::CONTENT_ITEM, Item::IMPLICIT_HEIGHT),
    )?;
    engine.bind_initial(obj, Control::MIRRORED.index, move |e| {
        Value::Bool(is_mirrored(e, obj))
    })?;

    destroy_replaced_style_parts(engine, obj);
    manage_part(
        engine,
        obj,
        Control::BACKGROUND.index,
        PartKind::Background(Some([
            Control::LEFT_INSET.index,
            Control::TOP_INSET.index,
            Control::RIGHT_INSET.index,
            Control::BOTTOM_INSET.index,
        ])),
    )?;
    manage_part(
        engine,
        obj,
        Control::CONTENT_ITEM.index,
        PartKind::Content(content_rect),
    )?;

    let holds_controls = is_a::<Pane>(engine, obj);
    inherit::attach(engine, obj, Kind::Control, holds_controls)?;

    // A control that stops taking hover is not hovered.
    engine.watch(obj, Control::HOVER_ENABLED.index, move |e, v| {
        if !v.to_boolean() {
            let _ = set_computed(e, obj, Control::HOVERED.index, Value::Bool(false));
        }
    })?;
    Ok(())
}

/// Where a control's content item is: inside the padding, and on a page
/// between the header and the footer (tracked).
fn content_rect(engine: &Engine, obj: ObjectId) -> [f64; 4] {
    let x = num(engine, obj, Control::LEFT_PADDING.index);
    let mut y = num(engine, obj, Control::TOP_PADDING.index);
    let w = num(engine, obj, Control::AVAILABLE_WIDTH.index);
    let mut h = num(engine, obj, Control::AVAILABLE_HEIGHT.index);
    if is_a::<Page>(engine, obj) {
        let (top, bottom) = page_bars(engine, obj);
        y += top;
        h = (h - top - bottom).max(0.0);
    }
    [x, y, w, h]
}

/// Whether the control reacts to the pointer: it is enabled, and so is
/// everything it is inside (in a scene `enabled` reads as that).
pub(crate) fn takes_input(engine: &Engine, obj: ObjectId) -> bool {
    engine.untrack(|| flag(engine, obj, Item::ENABLED.index))
}

pub(crate) fn is_left(ev: &PointerEvent) -> bool {
    ev.button.is_none_or(|b| b == MouseButtons::LEFT)
}

fn press(engine: &Engine, obj: ObjectId, ev: &PointerEvent, _at: kurbo::Point) -> bool {
    // A control is opaque to the pointer: what is behind it does not get
    // the click. A disabled one lets it through (checked on qml6).
    takes_input(engine, obj) && is_left(ev)
}

fn ignore(_: &Engine, _: ObjectId, _: &PointerEvent, _: kurbo::Point) {}

fn ignore_cancel(_: &Engine, _: ObjectId) {}

pub(crate) fn hover(engine: &Engine, obj: ObjectId, over: bool) {
    let on = over && engine.untrack(|| flag(engine, obj, Control::HOVER_ENABLED.index));
    let _ = set_computed(engine, obj, Control::HOVERED.index, Value::Bool(on));
}

pub(crate) fn register(engine: &Engine) {
    register_pointer_item(
        engine,
        ljos_engine::MetaRef::Static(Control::static_meta()),
        PointerItem {
            press,
            moved: ignore,
            release: ignore,
            cancel: ignore_cancel,
            hover,
        },
    );
}
