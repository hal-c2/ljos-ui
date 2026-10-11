//! Who gets a wheel event.
//!
//! As in Qt (observed on qml6; `features/input/mouse-area-wheel.feature`,
//! `wheel-handler-delivery.feature`) the event is offered to the items
//! under the pointer, topmost first. For each item its WheelHandlers come
//! first, then the item itself: a MouseArea that has a handler for `wheel`,
//! or an item whose type takes the wheel (a Flickable). The first that
//! keeps the event ends the round. An area with no `wheel` handler, a
//! disabled one and a Flickable that cannot move let it go on.

use ljos_engine::{Engine, ObjectId, Point, Value};
use ljos_render::kurbo;

use crate::flickable::wheel::wheel_angle;
use crate::input::{MouseArea, MouseButtons, ScrollPhase, WheelEvent, WheelEventObject};
use crate::item::{is_a, scene_transform};
use crate::wheel_handler::Taken;

/// Offers `ev` to `hits`, the items under the pointer, topmost first,
/// with `buttons` held. Whether one of them used it.
pub(crate) fn deliver(
    engine: &Engine,
    hits: &[ObjectId],
    ev: &WheelEvent,
    buttons: MouseButtons,
) -> bool {
    let mut used = false;
    for obj in hits {
        if !engine.is_alive(*obj) {
            continue;
        }
        match crate::wheel_handler::offer(engine, *obj, ev, buttons) {
            Taken::Kept => return true,
            Taken::Shared => used = true,
            Taken::No => {}
        }
        if !engine.is_alive(*obj) {
            continue;
        }
        let kept = if is_a::<MouseArea>(engine, *obj) {
            area_wheel(engine, *obj, ev, buttons)
        } else {
            crate::flickable::wheel::offer(engine, *obj, ev)
        };
        if kept {
            return true;
        }
    }
    used
}

/// Tells a MouseArea of the wheel. True: it keeps the event.
fn area_wheel(engine: &Engine, m: ObjectId, ev: &WheelEvent, buttons: MouseButtons) -> bool {
    // An area nobody listens to is not asked: Qt lets the wheel through
    // it, to the Flickable it covers.
    let listens = engine
        .signal_index(m, "wheel")
        .is_ok_and(|s| engine.signal_connected(m, s));
    if !listens || !crate::scene::mouse_area_enabled(engine, m) {
        return false;
    }
    emit_wheel(engine, m, m, ev, buttons)
}

/// Emits `wheel` on `to` with a fresh event object in the coordinates of
/// `item`, and returns its `accepted` after the handlers ran.
pub(crate) fn emit_wheel(
    engine: &Engine,
    to: ObjectId,
    item: ObjectId,
    ev: &WheelEvent,
    buttons: MouseButtons,
) -> bool {
    let at = engine.untrack(|| scene_transform(engine, item)).inverse() * ev.pos;
    let point = |v: kurbo::Vec2| Point { x: v.x, y: v.y };
    let native = WheelEventObject {
        x: at.x,
        y: at.y,
        angle_delta: point(wheel_angle(ev)),
        pixel_delta: point(ev.pixel_delta),
        buttons: buttons.bits() as i32,
        modifiers: ev.modifiers.bits() as i32,
        phase: scroll_phase(ev.phase),
        ..WheelEventObject::default()
    };
    let Ok(event) = engine.create_native(native, Some(to)) else {
        return true;
    };
    let _ = engine.emit_by_name(to, "wheel", &[Value::Object(event)]);
    let accepted = engine
        .read(event, WheelEventObject::ACCEPTED.index)
        .to_boolean();
    engine.destroy_now(event);
    accepted
}

/// The `Qt.ScrollPhase` of a phase.
fn scroll_phase(phase: ScrollPhase) -> i32 {
    match phase {
        ScrollPhase::None => 0,
        ScrollPhase::Begin => 1,
        ScrollPhase::Update => 2,
        ScrollPhase::End => 3,
        ScrollPhase::Momentum => 4,
    }
}
