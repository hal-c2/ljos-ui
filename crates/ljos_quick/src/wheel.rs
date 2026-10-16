//! Who gets a wheel event.
//!
//! As in Qt (observed on qml6; `features/input/mouse-area-wheel.feature`,
//! `wheel-handler-delivery.feature`) the event is offered to the items
//! under the pointer, topmost first. For each item its WheelHandlers come
//! first, then the item itself: a MouseArea that has a handler for `wheel`,
//! or an item whose type takes the wheel (a Flickable). The first that
//! keeps the event ends the round. An area with no `wheel` handler, a
//! disabled one and a Flickable that cannot move let it go on.

use ljos_engine::{Engine, IntPoint, ObjectId, Value};
use ljos_render::kurbo;

use crate::flickable::wheel::wheel_angle;
use crate::input::{MouseArea, MouseButtons, ScrollPhase, WheelEvent, WheelEventObject};
use crate::item::{is_a, scene_transform};
use crate::wheel_handler::Taken;

/// Offers `ev` to `hits`, the items under the pointer, topmost first,
/// with `buttons` held. Whether one of them used it.
///
/// An item whose WheelHandler only reaches the pointer by its `margin`
/// is asked where it would stand among the hits, as qml6 asks it: what
/// covers it at the pointer comes first, what it covers after. Only its
/// handlers are asked, not the item itself.
pub(crate) fn deliver(
    engine: &Engine,
    hits: &[ObjectId],
    ev: &WheelEvent,
    buttons: MouseButtons,
) -> bool {
    let mut order: Vec<(ObjectId, bool)> = hits.iter().map(|o| (*o, false)).collect();
    for item in crate::wheel_handler::margin_items(engine, ev.pos) {
        if hits.contains(&item) {
            continue;
        }
        let key = crate::wheel_handler::stack_key(engine, item);
        let at = order
            .iter()
            .position(|(o, _)| {
                crate::wheel_handler::stack_key(engine, *o)
                    .partial_cmp(&key)
                    .is_some_and(|c| c == std::cmp::Ordering::Less)
            })
            .unwrap_or(order.len());
        order.insert(at, (item, true));
    }
    let mut used = false;
    for (obj, margin_only) in order {
        if !engine.is_alive(obj) {
            continue;
        }
        match crate::wheel_handler::offer(engine, obj, ev, buttons) {
            Taken::Kept => return true,
            Taken::Shared => used = true,
            Taken::No => {}
        }
        if margin_only || !engine.is_alive(obj) {
            continue;
        }
        let kept = if is_a::<MouseArea>(engine, obj) {
            area_wheel(engine, obj, ev, buttons)
        } else {
            crate::flickable::wheel::offer(engine, obj, ev)
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
    // Qt's deltas are whole numbers, so a fraction a device sent (or one
    // made of pixels, above) is rounded in what a script reads; what
    // scrolls keeps the fraction.
    let point = |v: kurbo::Vec2| IntPoint::rounded(v.x, v.y);
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
    let Ok(event) = engine.create_native_incomplete(native, Some(to)) else {
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
