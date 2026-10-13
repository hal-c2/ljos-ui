//! `PointHandler`: follows one pointer that presses its item, without
//! taking it from anything.
//!
//! What it tells and in which order was read on qml6 (`features/input/
//! point-handler.feature`): at the press it grabs, becomes active, says
//! its translation (zero, said all the same) and then its point; at the
//! release it lets go and empties its point but keeps the translation.

use ljos_engine::{Cx, Engine, EngineError, ObjectId, QmlObject, Vector2d, qml_impl};
use ljos_render::kurbo;

use super::{PointValues, PointerHandler, emit, put, put_point, set_active, vector_value};
use crate::input::{HandlerPoint, PointerEvent};

/// `PointHandler` (QtQuick).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "PointHandler", extends = PointerHandler)]
pub struct PointHandler {
    #[qml(base)]
    pub handler: PointerHandler,
    /// The pointer that presses the handler; empty, `id` -1, without one.
    #[qml(group)]
    pub point: HandlerPoint,
    /// How far the pointer moved since the press. Says so with
    /// `translationChanged`, also when it stays the same.
    #[qml(readonly)]
    pub translation: Vector2d,
}

#[qml_impl]
impl PointHandler {
    #[qml(signal, name = "pointChanged")]
    fn point_changed();
    #[qml(signal, name = "translationChanged")]
    fn translation_changed();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        super::completed(cx)
    }
}

fn translate(engine: &Engine, h: ObjectId, d: kurbo::Vec2) {
    put(engine, h, PointHandler::TRANSLATION.index, vector_value(d));
    emit(engine, h, "translationChanged", &[]);
}

pub(super) fn pressed(engine: &Engine, h: ObjectId, ev: &PointerEvent) {
    set_active(engine, h, true);
    translate(engine, h, kurbo::Vec2::ZERO);
    put_point(engine, h, "point", &PointValues::of(engine, h, ev, ev.pos));
}

pub(super) fn moved(engine: &Engine, h: ObjectId, ev: &PointerEvent, press: kurbo::Point) {
    translate(engine, h, ev.pos - press);
    put_point(engine, h, "point", &PointValues::of(engine, h, ev, press));
}

pub(super) fn released(engine: &Engine, h: ObjectId, ev: &PointerEvent) {
    set_active(engine, h, false);
    let press = super::grabs(engine).press.get();
    translate(engine, h, ev.pos - press);
    put_point(engine, h, "point", &PointValues::empty());
}

pub(super) fn canceled(engine: &Engine, h: ObjectId) {
    set_active(engine, h, false);
    put_point(engine, h, "point", &PointValues::empty());
}
