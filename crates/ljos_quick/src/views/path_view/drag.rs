//! The user moving the rows along the path.
//!
//! A press is taken on a delegate, or within `dragMargin` of the path. The
//! pointer is followed by the point of the path nearest it: a move across
//! the path does nothing. The first move more than 10 pixels along the
//! path from where it was pressed begins the drag there, without moving
//! the rows; from then on each move moves them as far as the pointer went
//! along the path (all of this observed on qml6, move by move: 10 pixels
//! is not enough, 11 is, whatever the moves before).
//!
//! The speed at the release is not a `Flickable`'s. qml6 throws the mean
//! speed of the last three moves after the one that began the drag, less
//! a fiftieth for each millisecond the pointer rested before it let go:
//! still for 50 ms, it throws nothing. Slower than a `Flickable` would
//! fling, it throws nothing either (measured with qmltestrunner at fixed
//! event times). What happens after it is the
//! engine's scroll physics ([`super::motion::release`]).

use ljos_engine::{Engine, ObjectId};
use ljos_render::kurbo;
use std::time::Duration;

use ljos_runtime::Time;

use super::layout::{contains, frame_of};
use super::motion::{self, interrupt, show_dragging, show_moving, wrap, write_offset};
use super::{PathView, State, rd, state_of};
use crate::flickable::physics::scroll_physics;
use crate::input::{MouseArea, PointerEvent};
use crate::item::{is_a, scene_transform};
use crate::pointer_item::PointerItem;

/// A tree deeper than this is a cycle someone built by hand.
const DEPTH: usize = 4096;

/// How far along the path the pointer goes before the drag begins: 10 in
/// qml6, as the default drag distance, but kept when a host changes that.
const TAKE: f64 = 10.0;

/// How long a pointer rests before it lets go for a release to throw
/// nothing; a shorter rest takes its share off the speed.
const REST_FADE: Duration = Duration::from_millis(50);
/// How many of the last moves' speeds a release averages.
const KEPT: usize = 3;

/// A press the view holds.
#[derive(Debug)]
pub(crate) struct Press {
    /// The length along the path the pointer was last followed from.
    anchor: f64,
    /// The drag began.
    stolen: bool,
    /// When, and at which length along the path, the last event was.
    last: (Time, f64),
    /// The speeds along the path of the last moves of the drag, newest at
    /// `next - 1`, in pixels a second.
    speeds: [f64; KEPT],
    kept: usize,
    next: usize,
}

impl Press {
    fn new(time: Time, len: f64) -> Self {
        Press {
            anchor: len,
            stolen: false,
            last: (time, len),
            speeds: [0.0; KEPT],
            kept: 0,
            next: 0,
        }
    }

    fn keep(&mut self, speed: f64) {
        self.speeds[self.next] = speed;
        self.next = (self.next + 1) % KEPT;
        self.kept = (self.kept + 1).min(KEPT);
    }

    /// The speed a release at `now` throws with.
    fn thrown(&self, now: Time) -> f64 {
        if self.kept == 0 {
            return 0.0;
        }
        let rest = now.saturating_since(self.last.0).as_secs_f64();
        let left = (1.0 - rest / REST_FADE.as_secs_f64()).max(0.0);
        self.speeds[..self.kept].iter().sum::<f64>() / self.kept as f64 * left
    }
}

pub(crate) fn hooks() -> PointerItem {
    PointerItem {
        press,
        moved,
        release,
        cancel,
        hover: |_, _, _| {},
    }
}

fn press(e: &Engine, view: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    let Some(st) = state_of(e, view) else {
        return false;
    };
    begin(e, view, &st, ev, at)
}

/// Asked by the scene before it delivers a move of a press that `holder`
/// holds (a `MouseArea` in a delegate): whether a `PathView` around it
/// takes the press to drag the rows. The view follows the press from its
/// first move as if it held it; the move that begins to move the rows
/// takes it, after `dragStarted`, and the scene then cancels the holder's
/// press (that order measured on qml6). A press on a `MouseArea` that
/// prevents stealing, or on an item that keeps its grab, stays.
pub(crate) fn steal(
    e: &Engine,
    holder: ObjectId,
    press_scene: kurbo::Point,
    ev: &PointerEvent,
) -> Option<ObjectId> {
    if !e.is_alive(holder)
        || crate::flickable::drag::keeps_pointer_grab(e, holder)
        || crate::flickable::drag::is_dragging(e, holder)
        || is_a::<PathView>(e, holder)
    {
        return None;
    }
    if is_a::<MouseArea>(e, holder)
        && e.with(holder, |m: &MouseArea| m.prevent_stealing)
            .unwrap_or(false)
    {
        return None;
    }
    let start = if is_a::<crate::handlers::PointerHandler>(e, holder) {
        crate::handlers::yields_to_flickable(e, holder)?
    } else {
        holder
    };
    let mut cur = e.untrack(|| e.visual_parent(start));
    let mut view = None;
    for _ in 0..DEPTH {
        let Some(o) = cur else { break };
        if is_a::<PathView>(e, o) {
            view = Some(o);
            break;
        }
        cur = e.untrack(|| e.visual_parent(o));
    }
    let view = view?;
    let st = state_of(e, view)?;
    let serial = crate::flickable::drag::press_serial(e);
    if st.watching.get() != Some(serial) {
        st.watching.set(Some(serial));
        *st.press.borrow_mut() = None;
        let at = local(e, view, press_scene);
        if !begin(e, view, &st, ev, at) {
            return None;
        }
    }
    if st.press.borrow().is_none() {
        return None;
    }
    moved(e, view, ev, local(e, view, ev.pos));
    e.untrack(|| e.read(view, PathView::DRAGGING.index))
        .to_boolean()
        .then_some(view)
}

fn local(e: &Engine, obj: ObjectId, pos: kurbo::Point) -> kurbo::Point {
    e.untrack(|| scene_transform(e, obj)).inverse() * pos
}

/// Starts following a press at `at` (in the view's coordinates). False:
/// the view does not take it.
fn begin(e: &Engine, view: ObjectId, st: &State, ev: &PointerEvent, at: kurbo::Point) -> bool {
    if !e
        .untrack(|| e.read(view, PathView::INTERACTIVE.index))
        .to_boolean()
    {
        return false;
    }
    let n = st.delegates().map_or(0, |d| d.count());
    let Some(f) = e.untrack(|| frame_of(e, view, st, n)) else {
        return false;
    };
    let Some((len, distance)) = f.track.nearest(at) else {
        return false;
    };
    let margin = e.untrack(|| rd(e, view, PathView::DRAG_MARGIN.index));
    let on_delegate = st.delegates().is_some_and(|d| {
        d.live()
            .iter()
            .any(|(_, o)| e.untrack(|| contains(e, *o, at.x, at.y)))
    });
    if !on_delegate && !(margin > 0.0 && distance <= margin) {
        return false;
    }
    // A press stops the rows where they are.
    if st.motion.get().is_some() {
        interrupt(e, view, st);
    }
    *st.press.borrow_mut() = Some(Press::new(ev.time, len));
    true
}

fn moved(e: &Engine, view: ObjectId, ev: &PointerEvent, at: kurbo::Point) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let n = st.delegates().map_or(0, |d| d.count());
    let Some(f) = e.untrack(|| frame_of(e, view, &st, n)) else {
        return;
    };
    let Some((len, _)) = f.track.nearest(at) else {
        return;
    };
    let mut slot = st.press.borrow_mut();
    let Some(p) = slot.as_mut() else {
        return;
    };
    // Round a closed path the short way.
    let short = |d: f64| {
        if f.track.closed && f.track.total > 0.0 {
            let t = f.track.total;
            (d + t / 2.0).rem_euclid(t) - t / 2.0
        } else {
            d
        }
    };
    let along = short(len - p.anchor);
    let (t0, len0) = p.last;
    let secs = ev.time.saturating_since(t0).as_secs_f64();
    p.last = (ev.time, len);
    let speed = (secs > 0.0).then(|| short(len - len0) / secs);
    if !p.stolen {
        if along.abs() > TAKE {
            p.stolen = true;
            p.anchor = len;
        }
        return;
    }
    let mut share = f.track.percent_of_len(len) - f.track.percent_of_len(p.anchor);
    if f.track.closed {
        share = (share + 0.5).rem_euclid(1.0) - 0.5;
    }
    p.anchor = len;
    if let Some(speed) = speed {
        p.keep(speed);
    }
    drop(slot);
    if share == 0.0 {
        return;
    }
    let o = e.untrack(|| rd(e, view, PathView::OFFSET.index));
    write_offset(e, view, &st, wrap(o + share * f.k as f64, n));
    show_moving(e, view, true);
    show_dragging(e, view, true);
}

/// The speed along the path a release at `now` throws with, in pixels a
/// second; a pointer that crept throws nothing.
fn velocity(e: &Engine, p: &Press, now: Time) -> f64 {
    let Some(k) = scroll_physics(e).kinetic().copied() else {
        return 0.0;
    };
    let v = p.thrown(now);
    if v.abs() < k.min_flick { 0.0 } else { v }
}

fn release(e: &Engine, view: ObjectId, ev: &PointerEvent, _: kurbo::Point) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let Some(p) = st.press.borrow_mut().take() else {
        return;
    };
    let dragging = e
        .untrack(|| e.read(view, PathView::DRAGGING.index))
        .to_boolean();
    if !dragging {
        return;
    }
    show_dragging(e, view, false);
    let v = velocity(e, &p, ev.time);
    motion::release(e, view, &st, v);
}

fn cancel(e: &Engine, view: ObjectId) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    if st.press.borrow_mut().take().is_none() {
        return;
    }
    if e.untrack(|| e.read(view, PathView::DRAGGING.index))
        .to_boolean()
    {
        show_dragging(e, view, false);
        motion::release(e, view, &st, 0.0);
    } else {
        show_moving(e, view, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64) -> Time {
        Time::from_millis(ms)
    }

    #[test]
    fn a_release_throws_the_mean_of_the_last_three_speeds() {
        let mut p = Press::new(at(0), 0.0);
        assert_eq!(p.thrown(at(0)), 0.0);
        for speed in [9000.0, 100.0, 200.0, 600.0] {
            p.keep(speed);
        }
        p.last = (at(100), 0.0);
        assert_eq!(p.thrown(at(100)), 300.0);
    }

    #[test]
    fn a_rest_takes_a_fiftieth_a_millisecond_off_the_speed() {
        let mut p = Press::new(at(0), 0.0);
        p.keep(600.0);
        p.last = (at(100), 0.0);
        assert!((p.thrown(at(125)) - 300.0).abs() < 1e-9);
        assert!((p.thrown(at(140)) - 120.0).abs() < 1e-9);
        assert_eq!(p.thrown(at(150)), 0.0);
        assert_eq!(p.thrown(at(400)), 0.0);
    }
}
