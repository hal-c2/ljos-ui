//! The wheel: who gets a [`WheelEvent`], and what a Flickable does with
//! one.
//!
//! The scene offers the event to the items under the pointer, topmost
//! first, until one says it used it ([`crate::wheel`], which asks the
//! WheelHandlers and MouseAreas as well). A Flickable uses it only when its
//! content moved, so a list that is at its end lets the page around it
//! scroll, as Qt does. An event nothing uses changes nothing and costs no
//! frame.

use std::cell::RefCell;

use ljos_engine::{Engine, MetaRef, ObjectId};
use ljos_render::kurbo;

use super::physics::{Motion, WHEEL_NOTCH_PIXELS, scroll_physics};
use super::{
    Flickable, X, Y, bounds, can_flick, flag, fling, scroll_to, state_of, wheel_enabled,
    wheel_moved, wheel_rested,
};
use crate::input::{ScrollPhase, WheelEvent};
use crate::item::Item;

/// What an item type does with the wheel. The position is in the item's
/// own coordinates.
#[derive(Clone, Copy, Debug)]
pub struct WheelItem {
    /// The wheel turned over the item. True: the item used the event.
    /// False: it goes on to the items beneath.
    pub wheel: fn(&Engine, ObjectId, &WheelEvent, kurbo::Point) -> bool,
}

#[derive(Default)]
struct Registry {
    types: RefCell<Vec<(MetaRef, WheelItem)>>,
}

/// Makes the scene offer wheel events to items of type `meta` (and of the
/// types derived from it). A later registration wins.
pub fn register_wheel_item(engine: &Engine, meta: MetaRef, hooks: WheelItem) {
    engine
        .extension::<Registry>()
        .types
        .borrow_mut()
        .push((meta, hooks));
}

fn wheel_item(engine: &Engine, obj: ObjectId) -> Option<WheelItem> {
    let reg = engine.extension::<Registry>();
    let types = reg.types.borrow();
    let meta = engine.meta_of(obj)?;
    types
        .iter()
        .rev()
        .find(|(m, _)| meta.inherits(m))
        .map(|(_, h)| *h)
}

/// Offers `ev` to one item under the pointer, if its type takes the
/// wheel. Whether the item used it. Who is asked, and in which order, is
/// [`crate::wheel::deliver`]'s to say.
pub(crate) fn offer(engine: &Engine, obj: ObjectId, ev: &WheelEvent) -> bool {
    let Some(h) = wheel_item(engine, obj) else {
        return false;
    };
    let at = engine
        .untrack(|| crate::item::scene_transform(engine, obj))
        .inverse()
        * ev.pos;
    (h.wheel)(engine, obj, ev, at)
}

/// How far the event asks the content to go, towards the end, in each
/// direction. Devices that scroll by the pixel say so themselves; a wheel
/// with notches goes [`WHEEL_NOTCH_PIXELS`] a notch.
pub fn wheel_distance(ev: &WheelEvent) -> [f64; 2] {
    if ev.pixel_delta != kurbo::Vec2::ZERO {
        return [-ev.pixel_delta.x, -ev.pixel_delta.y];
    }
    let per = WHEEL_NOTCH_PIXELS / WheelEvent::NOTCH;
    [-ev.angle_delta.x * per, -ev.angle_delta.y * per]
}

/// How far the wheel turned, in eighths of a degree, sideways and
/// lengthways. A device that scrolls by the pixel and tells no angle is
/// taken for the wheel that would scroll as far.
pub fn wheel_angle(ev: &WheelEvent) -> kurbo::Vec2 {
    if ev.angle_delta != kurbo::Vec2::ZERO {
        return ev.angle_delta;
    }
    ev.pixel_delta * (WheelEvent::NOTCH / WHEEL_NOTCH_PIXELS)
}

fn wheel(e: &Engine, f: ObjectId, ev: &WheelEvent, _at: kurbo::Point) -> bool {
    let takes =
        e.untrack(|| flag(e, f, Flickable::INTERACTIVE.index) && flag(e, f, Item::ENABLED.index));
    if !takes || !wheel_enabled(e, f) {
        return false;
    }
    let physics = scroll_physics(e);
    let distance = wheel_distance(ev);
    let by_pixel = ev.pixel_delta != kurbo::Vec2::ZERO;
    let mut moved = [false; 2];
    for axis in [X, Y] {
        if distance[axis] == 0.0 || !e.untrack(|| can_flick(e, f, axis)) {
            continue;
        }
        let state = e.untrack(|| bounds(e, f, axis));
        match physics.wheel(&state, distance[axis], by_pixel) {
            Motion::Jump(to) => {
                stop_fling(e, f, axis);
                scroll_to(e, f, axis, to);
                moved[axis] = true;
            }
            Motion::Start(seg) => {
                stop_fling(e, f, axis);
                fling::glide(e, f, axis, seg);
                moved[axis] = true;
            }
            _ => {}
        }
    }
    let ended = ev.phase == ScrollPhase::End;
    if moved != [false; 2] {
        // A device that says when the gesture ends needs no timer.
        let rest = (!ended).then(|| physics.wheel_rest().as_millis() as i32);
        wheel_moved(e, f, moved, rest);
        return true;
    }
    if ended {
        wheel_rested(e, f);
    }
    false
}

/// The wheel takes over from a fling in its direction.
fn stop_fling(e: &Engine, f: ObjectId, axis: usize) {
    if let Some(st) = state_of(e, f) {
        let mut axes = [false; 2];
        axes[axis] = true;
        fling::stop(e, f, &st, axes);
    }
}

pub(crate) fn hooks() -> WheelItem {
    WheelItem { wheel }
}
