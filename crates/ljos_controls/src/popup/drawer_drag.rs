//! Dragging a drawer in from the edge of its window, and out again.
//!
//! The overlay hears a press before anything under it does, and every move
//! while a button is held (`ljos_quick::set_layer_take`). A press within
//! `dragMargin` of the edge a closed drawer comes in from, or one on an
//! open drawer, makes that drawer a candidate. The press goes on to what is
//! under it as usual; the drawer takes the pointer from it once the pointer
//! has gone more than 20 pixels along the drawer's axis, in for a closed
//! drawer and out for an open one, and follows the pointer from there.
//!
//! What a drawer does when it is let go, the distance it is taken at, and
//! the way the speed of the pointer is reckoned were read off Qt 6.11.2
//! with qmltestrunner (offscreen), as a black box. The speed is a model that
//! fits every measured drag, not a known formula: each move is a sample of
//! the speed, smoothed as `0.8 × before + 0.2 × sample`; the speed dies
//! away over 100 ms while the pointer is still, and a drawer that was
//! closed opens if it is at least 0.3 of the way in or the pointer was
//! still going in faster than 150 px/s. One that was open stays open only
//! if more than 0.7 of it is in, however fast it was moved.
//! `features/controls/drawer-drag.feature` and
//! `drawer-drag-release.feature` pin it.

use std::cell::Cell;

use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::{Item, MouseButtons, PointerEvent, child_items, scene_transform};
use ljos_render::kurbo;
use ljos_runtime::Time;

use super::drawer::{Drawer, edge, is_drawer};
use super::overlay::{is_modal, point_in, stack};
use super::{Popup, item_of, popup_of, put};
use crate::util::{check, flag, num};

/// How far along its axis the pointer goes before the drawer takes it:
/// more than this.
const GRAB_DISTANCE: f64 = 20.0;
/// How far in a drawer that was closed is let go to open.
const OPEN_FROM_CLOSED: f64 = 0.3;
/// How far in a drawer that was open has to stay to stay open: more.
const STAY_OPEN: f64 = 0.7;
/// The speed in (pixels a second) past which a closed drawer let go opens
/// wherever it is.
const FLICK_SPEED: f64 = 150.0;
/// How much of the speed so far a new sample keeps.
const SMOOTHING: f64 = 0.8;
/// Over how many milliseconds the speed dies away by a factor of e while
/// the pointer is still.
const STILL_DECAY_MS: f64 = 100.0;

/// The drag of the press that is down, if it is a drawer's.
#[derive(Default)]
struct Drags(Cell<Option<Drag>>);

#[derive(Clone, Copy, Debug)]
struct Drag {
    drawer: ObjectId,
    item: ObjectId,
    /// Along x (the left and right edges) or along y.
    horizontal: bool,
    /// +1 when in is towards larger coordinates (the left and top edges).
    inward: f64,
    /// Where the press was along the axis, in the window.
    press: f64,
    was_open: bool,
    /// The drawer's position when the press came.
    start: f64,
    /// Where along the axis the drawer took the pointer.
    grab: Option<f64>,
    speed: Speed,
}

impl Drag {
    fn along(&self, p: kurbo::Point) -> f64 {
        if self.horizontal { p.x } else { p.y }
    }

    /// The position the drawer is at with the pointer at `at`.
    fn position_at(&self, at: f64, size: f64) -> f64 {
        let Some(grab) = self.grab else {
            return self.start;
        };
        if size <= 0.0 {
            return self.start;
        }
        (self.start + self.inward * (at - grab) / size).clamp(0.0, 1.0)
    }

    /// Whether the pointer at `at` has gone far enough the way this drawer
    /// can be dragged for it to take the pointer.
    fn wants(&self, at: f64) -> bool {
        let gone = self.inward * (at - self.press);
        if self.was_open {
            gone < -GRAB_DISTANCE
        } else {
            gone > GRAB_DISTANCE
        }
    }
}

/// How fast the pointer goes in, in pixels a second.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Speed {
    value: f64,
    /// Where the pointer was, and when, at the last sample.
    at: f64,
    time: Time,
}

impl Speed {
    fn new(at: f64, time: Time) -> Self {
        Self {
            value: 0.0,
            at,
            time,
        }
    }

    /// A move to `at` at `time`. Moves that come at the same instant tell
    /// nothing about speed; how far they went counts with the next one.
    fn moved(&mut self, at: f64, time: Time, inward: f64) {
        let dt = time.saturating_since(self.time).as_secs_f64();
        if dt <= 0.0 {
            return;
        }
        let sample = inward * (at - self.at) / dt;
        self.value = SMOOTHING * self.value + (1.0 - SMOOTHING) * sample;
        self.at = at;
        self.time = time;
    }

    /// The speed at `time`, which is less the longer the pointer was still.
    fn at(&self, time: Time) -> f64 {
        let still_ms = time.saturating_since(self.time).as_secs_f64() * 1000.0;
        self.value * (-still_ms / STILL_DECAY_MS).exp()
    }
}

/// Whether a drawer let go at `position` with the pointer going in at
/// `speed` opens.
fn goes_open(was_open: bool, position: f64, speed: f64) -> bool {
    if was_open {
        position > STAY_OPEN
    } else {
        position >= OPEN_FROM_CLOSED || speed > FLICK_SPEED
    }
}

fn slot(engine: &Engine) -> std::rc::Rc<Drags> {
    engine.extension::<Drags>()
}

/// A press in the window of `overlay`: it may be the start of a drag of a
/// drawer.
pub(super) fn pressed(engine: &Engine, overlay: ObjectId, ev: &PointerEvent) {
    let found = if ev.button.unwrap_or(MouseButtons::LEFT) == MouseButtons::LEFT {
        engine.untrack(|| candidate(engine, overlay, ev))
    } else {
        None
    };
    slot(engine).0.set(found);
}

fn drag_of(engine: &Engine, drawer: ObjectId, item: ObjectId, ev: &PointerEvent) -> Drag {
    let side = num(engine, drawer, Drawer::EDGE.index) as i32;
    let horizontal = matches!(side, edge::LEFT | edge::RIGHT);
    let inward = if matches!(side, edge::RIGHT | edge::BOTTOM) {
        -1.0
    } else {
        1.0
    };
    let at = if horizontal { ev.pos.x } else { ev.pos.y };
    let was_open = flag(engine, drawer, Popup::VISIBLE.index);
    Drag {
        drawer,
        item,
        horizontal,
        inward,
        press: at,
        was_open,
        start: num(engine, drawer, Drawer::POSITION.index),
        grab: None,
        speed: Speed::new(at, ev.time),
    }
}

/// The drawer a press can drag: the open one it is on, or a closed one at
/// whose edge it is.
fn candidate(engine: &Engine, overlay: ObjectId, ev: &PointerEvent) -> Option<Drag> {
    for (item, popup) in stack(engine, overlay).into_iter().rev() {
        if point_in(engine, item, ev.pos) {
            let draggable = is_drawer(engine, popup)
                && flag(engine, popup, Drawer::INTERACTIVE.index)
                && !super::is_going(engine, popup);
            return draggable.then(|| drag_of(engine, popup, item, ev));
        }
        if is_modal(engine, popup) {
            return None;
        }
    }
    let local = scene_transform(engine, overlay).inverse() * ev.pos;
    let (w, h) = (
        num(engine, overlay, Item::WIDTH.index),
        num(engine, overlay, Item::HEIGHT.index),
    );
    child_items(engine, overlay)
        .into_iter()
        .rev()
        .find_map(|item| {
            let popup = popup_of(engine, item)?;
            if !is_drawer(engine, popup)
                || flag(engine, popup, Popup::VISIBLE.index)
                || !flag(engine, popup, Drawer::INTERACTIVE.index)
            {
                return None;
            }
            let margin = num(engine, popup, Drawer::DRAG_MARGIN.index);
            let at_edge = margin > 0.0
                && match num(engine, popup, Drawer::EDGE.index) as i32 {
                    edge::LEFT => local.x <= margin,
                    edge::RIGHT => local.x >= w - margin,
                    edge::TOP => local.y <= margin,
                    edge::BOTTOM => local.y >= h - margin,
                    _ => false,
                };
            at_edge.then(|| drag_of(engine, popup, item, ev))
        })
}

fn alive(engine: &Engine, d: &Drag) -> bool {
    engine.is_alive(d.drawer) && item_of(engine, d.drawer) == Some(d.item)
}

fn size_along(engine: &Engine, d: &Drag) -> f64 {
    let size = if d.horizontal {
        Popup::WIDTH
    } else {
        Popup::HEIGHT
    };
    num(engine, d.drawer, size.index)
}

fn follow(engine: &Engine, d: &Drag, at: f64) {
    let p = d.position_at(at, size_along(engine, d));
    check(
        engine,
        "position",
        put(engine, d.drawer, Drawer::POSITION.index, Value::Real(p)),
    );
}

/// A move with a button held, before the scene delivers it (the overlay's
/// `LayerTake`). Returns the drawer's item once the drawer has the pointer.
pub(super) fn take(
    engine: &Engine,
    _overlay: ObjectId,
    _press: kurbo::Point,
    ev: &PointerEvent,
) -> Option<ObjectId> {
    let drags = slot(engine);
    let mut d = drags.0.get()?;
    if !alive(engine, &d) {
        drags.0.set(None);
        return None;
    }
    let at = d.along(ev.pos);
    d.speed.moved(at, ev.time, d.inward);
    if d.grab.is_none() {
        if !d.wants(at) {
            drags.0.set(Some(d));
            return None;
        }
        d.grab = Some(at);
        // Before anything is said: a handler that runs then may press.
        drags.0.set(Some(d));
        engine.untrack(|| {
            if d.was_open {
                super::hold(engine, d.drawer);
            } else {
                check(engine, "drawer", super::show_still(engine, d.drawer));
            }
        });
        return engine.is_alive(d.item).then_some(d.item);
    }
    drags.0.set(Some(d));
    engine.untrack(|| follow(engine, &d, at));
    Some(d.item)
}

/// The pointer was let go and the drawer did not have it: what was pressed
/// had it, and there is no drag to finish.
pub(super) fn let_go(engine: &Engine) {
    let drags = slot(engine);
    if drags.0.get().is_some_and(|d| d.grab.is_none()) {
        drags.0.set(None);
    }
}

/// The drawer's item, which had the pointer, was let go of.
pub(super) fn released(engine: &Engine, item: ObjectId, ev: &PointerEvent) {
    let Some(d) = slot(engine).0.take() else {
        return;
    };
    if d.item != item || d.grab.is_none() || !alive(engine, &d) {
        return;
    }
    engine.untrack(|| {
        // Where it is let go is where it was dragged to last; the speed
        // goes by the moves alone.
        follow(engine, &d, d.along(ev.pos));
        let position = num(engine, d.drawer, Drawer::POSITION.index);
        let open = goes_open(d.was_open, position, d.speed.at(ev.time));
        finish(engine, &d, open, position);
    });
}

/// Something took the pointer from the drawer, or the platform did: it
/// goes back to how it was.
pub(super) fn canceled(engine: &Engine, item: ObjectId) {
    let Some(d) = slot(engine).0.take() else {
        return;
    };
    if d.item != item || d.grab.is_none() || !alive(engine, &d) {
        return;
    }
    engine.untrack(|| {
        let position = num(engine, d.drawer, Drawer::POSITION.index);
        finish(engine, &d, d.was_open, position);
    });
}

fn finish(engine: &Engine, d: &Drag, open: bool, position: f64) {
    let done = if open {
        // One that was open says again that it is about to show; one the
        // drag brought in said so when it came.
        super::settle_open(engine, d.drawer, d.was_open)
    } else if position <= 0.0 {
        // Nothing of it is in: there is nothing for the exit to move.
        super::close_at_once(engine, d.drawer)
    } else {
        super::close(engine, d.drawer)
    };
    check(engine, "drawer", done);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Speed, goes_open};
    use ljos_runtime::Time;

    fn ms(n: u64) -> Time {
        Time::default() + Duration::from_millis(n)
    }

    /// Moves along the axis every `gap` ms from a press at `points[0]`;
    /// the speed at a release `after` ms after the last move.
    fn speed(points: &[f64], gap: u64, after: u64) -> f64 {
        let mut s = Speed::new(points[0], ms(0));
        let mut t = 0;
        for p in &points[1..] {
            t += gap;
            s.moved(*p, ms(t), 1.0);
        }
        s.at(ms(t + after))
    }

    #[test]
    fn a_closed_drawer_opens_from_0_3_of_the_way_or_when_flicked() {
        assert!(!goes_open(false, 0.29, 0.0));
        assert!(goes_open(false, 0.3, 0.0));
        assert!(goes_open(false, 0.0, 151.0));
        assert!(!goes_open(false, 0.2, 150.0));
    }

    #[test]
    fn an_open_drawer_stays_open_only_above_0_7_however_fast() {
        assert!(goes_open(true, 0.71, 0.0));
        assert!(!goes_open(true, 0.7, 0.0));
        assert!(!goes_open(true, 0.5, 10_000.0));
    }

    #[test]
    fn a_quick_flick_is_fast_and_a_slow_drag_is_not() {
        // Measured on qml6: these opened a drawer left 0.2 of the way in.
        assert!(speed(&[4.0, 30.0, 40.0, 50.0, 54.0], 10, 0) > 150.0);
        assert!(speed(&[4.0, 30.0, 38.0, 46.0, 54.0], 10, 100) > 150.0);
        // And these did not.
        assert!(speed(&[4.0, 30.0, 38.0, 46.0, 54.0], 10, 200) < 150.0);
        assert!(
            speed(
                &[4.0, 30.0, 34.0, 38.0, 42.0, 46.0, 50.0, 54.0, 58.0],
                100,
                0
            ) < 150.0
        );
    }

    #[test]
    fn moves_at_one_instant_tell_no_speed() {
        let mut s = Speed::new(4.0, ms(0));
        for p in [12.0, 20.0, 28.0, 100.0] {
            s.moved(p, ms(0), 1.0);
        }
        assert_eq!(s.at(ms(0)), 0.0);
    }
}
