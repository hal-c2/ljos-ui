//! `HoverHandler`: says whether the pointer is over its item.
//!
//! It never takes the pointer, so it never keeps a press or a hover from
//! anything; a `blocking` one keeps the hover from the handlers beneath.

use std::cell::Cell;

use ljos_engine::{Cx, Engine, EngineError, ObjectId, QmlObject, Value, qml_impl};
use ljos_render::kurbo;

use super::{PointValues, PointerHandler, flag, handler_item, num, put, put_point, reaches};
use crate::input::{HandlerPoint, PointerEvent};
use crate::tree::resources;

/// `HoverHandler` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "HoverHandler", extends = PointerHandler)]
pub struct HoverHandler {
    #[qml(base)]
    pub handler: PointerHandler,
    /// The pointer over the item; empty, `id` -1, when it is not over it.
    #[qml(group)]
    pub point: HandlerPoint,
    #[qml(notify, readonly)]
    pub hovered: bool,
    /// The handlers of the items beneath are not hovered while this one
    /// is.
    #[qml(notify)]
    pub blocking: bool,
}

impl Default for HoverHandler {
    fn default() -> Self {
        Self {
            handler: PointerHandler {
                // Hover is not a button: Qt's HoverHandler reads 0 here.
                accepted_buttons: 0,
                ..PointerHandler::default()
            },
            point: HandlerPoint::default(),
            hovered: false,
            blocking: false,
        }
    }
}

#[qml_impl]
impl HoverHandler {
    #[qml(signal, name = "pointChanged")]
    fn point_changed();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let count = cx.engine.extension::<Count>();
        count.0.set(count.0.get() + 1);
        super::completed(cx)
    }
}

/// How many HoverHandlers were ever made: none, and the hover pass is
/// skipped at no cost.
#[derive(Default)]
struct Count(Cell<usize>);

pub(super) fn any(engine: &Engine) -> bool {
    engine.extension::<Count>().0.get() > 0
}

/// The pointer is at `ev` over `hits`, topmost first; `hovered` is the
/// list of hovered handlers, brought up to date.
pub(super) fn update(
    engine: &Engine,
    ev: &PointerEvent,
    hits: &[ObjectId],
    hovered: &mut Vec<ObjectId>,
) {
    let mut now = Vec::new();
    let mut blocked = false;
    for &item in hits {
        if blocked {
            break;
        }
        for h in resources(engine, item) {
            if !crate::item::is_a::<HoverHandler>(engine, h) || !super::usable(engine, h) {
                continue;
            }
            if !super::modifiers_fit(engine, h, ev) {
                continue;
            }
            let margin = num(engine, h, PointerHandler::MARGIN.index);
            if !handler_item(engine, h).is_some_and(|i| reaches(engine, i, ev.pos, margin)) {
                continue;
            }
            now.push(h);
            if flag(engine, h, HoverHandler::BLOCKING.index) {
                blocked = true;
            }
        }
    }
    for &h in hovered.iter() {
        if !now.contains(&h) && engine.is_alive(h) {
            put_point(engine, h, "point", &PointValues::empty());
            put(engine, h, HoverHandler::HOVERED.index, Value::Bool(false));
        }
    }
    for &h in &now {
        let press = ev.pos;
        let mut v = PointValues::of(engine, h, ev, press);
        v.press_position = kurbo::Point::ZERO;
        v.scene_press_position = kurbo::Point::ZERO;
        put_point(engine, h, "point", &v);
        put(engine, h, HoverHandler::HOVERED.index, Value::Bool(true));
    }
    *hovered = now;
}
