//! The user moving the rows along the path.
//!
//! A press is taken on a delegate, or within `dragMargin` of the path. The
//! pointer is followed by the point of the path nearest it: a move across
//! the path does nothing. Once the pointer is more than 8 pixels along the
//! path from where it was pressed the drag begins there, without moving;
//! from then on each move moves the rows as far as the pointer went along
//! the path (all of this observed on qml6).
//!
//! The speed at the release is measured as a `Flickable`'s is, and what
//! happens after it is the engine's scroll physics
//! ([`super::motion::release`]).

use ljos_engine::{Engine, ObjectId};
use ljos_render::kurbo;
use ljos_runtime::Time;

use super::layout::{contains, frame_of};
use super::motion::{self, interrupt, show_dragging, show_moving, wrap, write_offset};
use super::{PathView, State, rd, state_of};
use crate::flickable::physics::{VelocityTracker, scroll_physics};
use crate::input::{MouseArea, PointerEvent};
use crate::item::{is_a, scene_transform};
use crate::pointer_item::PointerItem;

/// A tree deeper than this is a cycle someone built by hand.
const DEPTH: usize = 4096;

/// A press the view holds.
#[derive(Debug)]
pub(crate) struct Press {
    /// The length along the path the pointer was last followed from.
    anchor: f64,
    /// Past the threshold.
    stolen: bool,
    /// How far the pointer has gone along the path since the press, and
    /// when: for the speed at the release.
    travel: f64,
    samples: VelocityTracker,
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
    let mut samples = VelocityTracker::default();
    samples.push(ev.time, 0.0);
    *st.press.borrow_mut() = Some(Press {
        anchor: len,
        stolen: false,
        travel: 0.0,
        samples,
    });
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
    let mut along = len - p.anchor;
    if f.track.closed && f.track.total > 0.0 {
        // Round a closed path the short way.
        let t = f.track.total;
        along = (along + t / 2.0).rem_euclid(t) - t / 2.0;
    }
    if !p.stolen {
        // The pointer went far enough along the path to be a drag.
        if along.abs() > crate::system_prefs::start_drag_distance(e) {
            p.stolen = true;
            p.anchor = len;
            p.samples.clear();
            p.samples.push(ev.time, p.travel);
        }
        return;
    }
    let mut share = f.track.percent_of_len(len) - f.track.percent_of_len(p.anchor);
    if f.track.closed {
        share = (share + 0.5).rem_euclid(1.0) - 0.5;
    }
    p.anchor = len;
    p.travel += along;
    p.samples.push(ev.time, p.travel);
    drop(slot);
    if share == 0.0 {
        return;
    }
    let o = e.untrack(|| rd(e, view, PathView::OFFSET.index));
    write_offset(e, view, &st, wrap(o + share * f.k as f64, n));
    show_moving(e, view, true);
    show_dragging(e, view, true);
}

/// The speed along the path over the last moves, in pixels a second; a
/// pointer that rested before it let go throws nothing.
fn velocity(e: &Engine, p: &Press, now: Time) -> f64 {
    let rested = p
        .samples
        .last_time()
        .is_none_or(|t| now.saturating_since(t) > scroll_physics(e).release_rest());
    if rested { 0.0 } else { p.samples.velocity(now) }
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
