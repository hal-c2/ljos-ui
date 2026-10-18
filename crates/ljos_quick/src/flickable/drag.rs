//! Dragging a Flickable's content with the pointer.
//!
//! A press inside a Flickable belongs to whatever it landed on: a button,
//! a `MouseArea`, the Flickable itself. The Flickable watches the moves
//! that follow. The first move that is further from the press than the
//! drag threshold, in a direction the content can go, only arms it; with
//! the move after that it takes the press away from whoever had it (who
//! is told the press was cancelled) and the content follows the pointer
//! from where the pointer then is, so nothing jumps. This is what a
//! running Qt 6.11 does, measured move by move.
//!
//! The scene asks [`steal`] before it delivers a move, and delivers the
//! moves of a press a Flickable holds through [`hooks`].

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use ljos_engine::{Engine, ObjectId};
use ljos_render::kurbo;
use ljos_runtime::Time;

use super::fling::{self, Kind};
use super::physics::{Motion, Segment, VelocityTracker, scroll_physics};
use super::{
    FlickState, Flickable, X, Y, bounds, can_flick, dir, flag, is_flickable, put_flag,
    set_drag_moving, state_of, warn,
};
use crate::input::{MouseArea, MouseButtons, PointerEvent};
use crate::item::{Item, is_a, scene_transform};
use crate::pointer_item::PointerItem;

/// A tree deeper than this is a cycle someone built by hand.
const DEPTH: usize = 4096;

/// What a Flickable knows of the press it is watching.
#[derive(Debug, Default)]
pub(crate) struct DragState {
    /// Which press this is about ([`Presses::serial`]).
    serial: Cell<u64>,
    /// Where the press was, in the Flickable's coordinates.
    press: Cell<kurbo::Point>,
    /// The directions in which the pointer has been past the threshold.
    /// They stay so when it comes back.
    over: Cell<[bool; 2]>,
    /// A move has been past the threshold: the next one takes the press.
    armed: Cell<bool>,
    /// The content follows the pointer.
    taken: Cell<bool>,
    /// Per direction: where the pointer and the content were when the
    /// direction began to follow.
    anchor: Cell<[Option<(f64, f64)>; 2]>,
    velocity: RefCell<[VelocityTracker; 2]>,
    /// Per direction, what `verticalVelocity` and the like show while the
    /// content follows: when the content last moved, where it was asked
    /// to be then, and the speed it moved at.
    shown: Cell<[Option<(Time, f64, f64)>; 2]>,
}

/// The presses of an engine's scenes, counted, so that a Flickable knows
/// a new one without being told about each.
#[derive(Default)]
struct Presses {
    serial: Cell<u64>,
    /// When the press was.
    time: Cell<Time>,
    /// Items that do not give a press up to a Flickable.
    keep: RefCell<HashSet<ObjectId>>,
}

/// The scene says a press began. Flickables forget the last one when they
/// next look.
pub(crate) fn pressed(engine: &Engine, time: Time) {
    let p = engine.extension::<Presses>();
    p.serial.set(p.serial.get().wrapping_add(1));
    p.time.set(time);
}

/// Which press the scene is on: another number for each press. For other
/// views that watch presses as a Flickable does (`PathView`).
pub(crate) fn press_serial(engine: &Engine) -> u64 {
    engine.extension::<Presses>().serial.get()
}

/// Says whether `item` keeps the press it holds when a Flickable around
/// it would take it for a drag. A scroll bar's handle says so while it is
/// held: dragging it must not drag the content.
pub fn keep_pointer_grab(engine: &Engine, item: ObjectId, keep: bool) {
    let p = engine.extension::<Presses>();
    let mut set = p.keep.borrow_mut();
    if keep {
        set.insert(item);
    } else {
        set.remove(&item);
    }
}

/// Whether `obj` is a Flickable that is being dragged: it keeps the
/// pointer from a pointer handler that would take it.
pub(crate) fn is_dragging(engine: &Engine, obj: ObjectId) -> bool {
    is_flickable(engine, obj) && state_of(engine, obj).is_some_and(|st| st.drag.taken.get())
}

/// Whether `item` keeps its press ([`keep_pointer_grab`]).
pub fn keeps_pointer_grab(engine: &Engine, item: ObjectId) -> bool {
    engine.extension::<Presses>().keep.borrow().contains(&item)
}

fn local(engine: &Engine, obj: ObjectId, pos: kurbo::Point) -> kurbo::Point {
    engine.untrack(|| scene_transform(engine, obj)).inverse() * pos
}

fn takes_drags(e: &Engine, f: ObjectId) -> bool {
    e.untrack(|| flag(e, f, Flickable::INTERACTIVE.index) && flag(e, f, Item::ENABLED.index))
}

fn coordinate(p: kurbo::Point, axis: usize) -> f64 {
    if axis == X { p.x } else { p.y }
}

/// Starts watching the current press if `st` still knows an older one.
fn watch(engine: &Engine, f: ObjectId, st: &FlickState, press_scene: kurbo::Point) {
    let presses = engine.extension::<Presses>();
    let serial = presses.serial.get();
    let d = &st.drag;
    if d.serial.get() == serial {
        return;
    }
    d.serial.set(serial);
    let press = local(engine, f, press_scene);
    d.press.set(press);
    d.over.set([false; 2]);
    d.armed.set(false);
    d.taken.set(false);
    d.anchor.set([None; 2]);
    d.shown.set([None; 2]);
    let mut trackers = d.velocity.borrow_mut();
    for axis in [X, Y] {
        trackers[axis].clear();
        trackers[axis].push(presses.time.get(), -coordinate(press, axis));
    }
}

/// Measures the pointer's speed from the press on, before the content
/// follows it too, as qml6 does: a fling thrown soon after a drag began
/// is as fast as the pointer was. The content goes the other way.
fn sample(st: &FlickState, at: kurbo::Point, time: Time) {
    let mut trackers = st.drag.velocity.borrow_mut();
    for axis in [X, Y] {
        trackers[axis].push(time, -coordinate(at, axis));
    }
}

/// Notes in which directions the pointer is now past the threshold.
/// Returns the directions that are new.
fn observe(e: &Engine, f: ObjectId, st: &FlickState, at: kurbo::Point) -> [bool; 2] {
    let d = &st.drag;
    let mut over = d.over.get();
    let mut new = [false; 2];
    for axis in [X, Y] {
        if over[axis] || !e.untrack(|| can_flick(e, f, axis)) {
            continue;
        }
        let moved = coordinate(at, axis) - coordinate(d.press.get(), axis);
        if moved.abs() > crate::system_prefs::start_drag_distance(e) {
            over[axis] = true;
            new[axis] = true;
        }
    }
    d.over.set(over);
    new
}

/// Whether one direction of the content follows the pointer.
pub(crate) fn follows(st: &FlickState, axis: usize) -> bool {
    st.drag.anchor.get()[axis].is_some()
}

/// Makes the directions in `axes` follow the pointer from `at`.
fn anchor(e: &Engine, f: ObjectId, st: &FlickState, axes: [bool; 2], at: kurbo::Point) {
    let d = &st.drag;
    let mut anchors = d.anchor.get();
    let physics = scroll_physics(e);
    let s = fling::settings(e, f);
    for axis in [X, Y] {
        if axes[axis] {
            // Content past a bound (caught in a rebound) gives way as a
            // drag there would: from what that drag asked for.
            let content = physics.undragged(&fling::virtual_position(e, f, axis), &s);
            anchors[axis] = Some((coordinate(at, axis), content));
            // The first speed shown is the way from the press: the content
            // was there then.
            let mut shown = d.shown.get();
            shown[axis] = Some((e.extension::<Presses>().time.get(), content, 0.0));
            d.shown.set(shown);
        }
    }
    d.anchor.set(anchors);
}

/// The Flickable takes the press: its content follows the pointer from
/// here.
fn take(e: &Engine, f: ObjectId, st: &FlickState, at: kurbo::Point) {
    let d = &st.drag;
    fling::stop(e, f, st, [true; 2]);
    d.taken.set(true);
    anchor(e, f, st, d.over.get(), at);
    show_dragging(e, f, st);
}

/// Brings the dragging and moving flags in step with the directions that
/// follow the pointer.
fn show_dragging(e: &Engine, f: ObjectId, st: &FlickState) {
    let follows = st.drag.anchor.get().map(|a| a.is_some());
    let was = e.untrack(|| flag(e, f, Flickable::DRAGGING.index));
    let any = follows[X] || follows[Y];
    e.batch(|| {
        for axis in [X, Y] {
            put_flag(e, f, dir(axis).dragging, follows[axis]);
        }
        put_flag(e, f, Flickable::DRAGGING.index, any);
    });
    if any != was {
        let signal = if any { "dragStarted" } else { "dragEnded" };
        warn(e, signal, e.emit_by_name(f, signal, &[]));
    }
    set_drag_moving(e, f, st, follows);
}

/// The pointer moved to `at` while the content follows it.
fn follow(e: &Engine, f: ObjectId, st: &FlickState, at: kurbo::Point, time: Time) {
    sample(st, at, time);
    let new = observe(e, f, st, at);
    if new != [false; 2] {
        // A direction that joins late starts from this very move.
        anchor(e, f, st, new, at);
        show_dragging(e, f, st);
    }
    let physics = scroll_physics(e);
    let s = fling::settings(e, f);
    let anchors = st.drag.anchor.get();
    for axis in [X, Y] {
        let Some((pointer, content)) = anchors[axis] else {
            continue;
        };
        let state = e.untrack(|| bounds(e, f, axis));
        // The velocity is the finger's, not the content's that gives way
        // past a bound: it is what a fling is thrown with.
        let wanted = content + pointer - coordinate(at, axis);
        let to = physics.dragged(&state, wanted, &s);
        let velocity = shown_velocity(&st.drag, axis, time, wanted);
        fling::place(e, f, axis, to, velocity);
    }
}

/// The speed shown while the content follows: the last move's, as qml6
/// shows, not the smoothed one a release throws. A move that leaves the
/// content where it was shows the last speed and is not counted.
fn shown_velocity(d: &DragState, axis: usize, time: Time, wanted: f64) -> f64 {
    let mut shown = d.shown.get();
    let Some((t0, w0, v0)) = shown[axis] else {
        return 0.0;
    };
    let dt = time.saturating_since(t0).as_secs_f64();
    if wanted == w0 || dt <= 0.0 {
        return v0;
    }
    let v = (wanted - w0) / dt;
    shown[axis] = Some((time, wanted, v));
    d.shown.set(shown);
    v
}

/// The press ended, or was taken away: the content is let go. It flings,
/// or comes back inside its bounds, as the physics says.
fn let_go(e: &Engine, f: ObjectId, st: &FlickState, time: Time, at: Option<kurbo::Point>) {
    let d = &st.drag;
    let thrown = |axis| at.is_some_and(|at| throws(e, f, st, axis, at));
    let was_taken = d.taken.replace(false);
    let anchors = d.anchor.replace([None; 2]);
    d.armed.set(false);
    d.over.set([false; 2]);
    if st.fling.release_caught() {
        warn(e, "flickEnded", e.emit_by_name(f, "flickEnded", &[]));
    }
    let physics = scroll_physics(e);
    let s = fling::settings(e, f);
    let mut segs = [None; 2];
    for axis in [X, Y] {
        if fling::runs(e, f, axis) {
            continue;
        }
        let mut state = fling::virtual_position(e, f, axis);
        if anchors[axis].is_some() || thrown(axis) {
            state.velocity = physics.thrown(&d.velocity.borrow()[axis], time);
        } else if state.clamp(state.position) == state.position {
            // A press caught content that was moving; only content left
            // past a bound has somewhere to go.
            continue;
        }
        match physics.released(&state, &s) {
            Motion::Start(seg) => {
                // `flickStarted` already reads the speed it was thrown with,
                // also when nothing was dragged before.
                if let Segment::Decelerate { velocity, .. } = seg {
                    fling::show_velocity(e, f, axis, velocity);
                }
                segs[axis] = Some(seg);
            }
            _ => fling::show_velocity(e, f, axis, 0.0),
        }
    }
    // The fling is under way before the drag ends, so that `moving` does
    // not drop between them; it is shown after `dragEnded`, as Qt says
    // them.
    fling::begin(e, f, st, segs, Kind::Flick);
    if was_taken {
        show_dragging(e, f, st);
    }
    fling::show(e, f, st);
}

/// Whether a press the Flickable held, and let go at `at` before the
/// content followed it, throws the content along `axis`. qml6 flings it
/// when the pointer ended up twice the drag threshold from the press, even
/// after one move.
fn throws(e: &Engine, f: ObjectId, st: &FlickState, axis: usize, at: kurbo::Point) -> bool {
    let d = &st.drag;
    let moved = coordinate(at, axis) - coordinate(d.press.get(), axis);
    d.serial.get() == press_serial(e)
        && moved.abs() >= 2.0 * crate::system_prefs::start_drag_distance(e)
        && e.untrack(|| can_flick(e, f, axis))
}

/// Asked by the scene before it delivers a move of a held press: whether
/// a Flickable takes the press from `holder`, the item that has it. The
/// scene then cancels the holder's press and gives the press to the
/// Flickable returned.
///
/// The Flickables asked are the holder, if it is one, and those around
/// it, innermost first; the first that was armed by an earlier move wins.
pub(crate) fn steal(
    engine: &Engine,
    holder: ObjectId,
    press_scene: kurbo::Point,
    ev: &PointerEvent,
) -> Option<ObjectId> {
    if !engine.is_alive(holder) || keeps_pointer_grab(engine, holder) {
        return None;
    }
    if is_a::<MouseArea>(engine, holder)
        && engine
            .with(holder, |m: &MouseArea| m.prevent_stealing)
            .unwrap_or(false)
    {
        return None;
    }
    // A pointer handler is not in the item tree: the Flickables asked are
    // those around its item, when the handler lets them take the press.
    let start = if is_a::<crate::handlers::PointerHandler>(engine, holder) {
        crate::handlers::yields_to_flickable(engine, holder)?
    } else {
        holder
    };
    let mut cur = Some(start);
    for _ in 0..DEPTH {
        let f = cur?;
        cur = engine.untrack(|| engine.visual_parent(f));
        if !is_flickable(engine, f) {
            continue;
        }
        let Some(st) = state_of(engine, f) else {
            continue;
        };
        if f == holder && st.drag.taken.get() {
            // It is dragging already; nothing around it gets the press.
            return None;
        }
        if !takes_drags(engine, f) {
            continue;
        }
        watch(engine, f, &st, press_scene);
        let was_armed = st.drag.armed.get();
        let at = local(engine, f, ev.pos);
        sample(&st, at, ev.time);
        observe(engine, f, &st, at);
        if was_armed {
            take(engine, f, &st, at);
            return (f != holder).then_some(f);
        }
        if st.drag.over.get() != [false; 2] {
            st.drag.armed.set(true);
        }
    }
    None
}

fn press(e: &Engine, f: ObjectId, ev: &PointerEvent, _at: kurbo::Point) -> bool {
    // An interactive Flickable is opaque to the pointer, with or without
    // content to drag (checked on qml6): what is behind it gets no click.
    takes_drags(e, f) && ev.button == Some(MouseButtons::LEFT)
}

fn moved(e: &Engine, f: ObjectId, ev: &PointerEvent, at: kurbo::Point) {
    let Some(st) = state_of(e, f) else { return };
    if !st.drag.taken.get() {
        // Still watching; `steal` does that for every Flickable.
        return;
    }
    if !takes_drags(e, f) {
        let_go(e, f, &st, ev.time, None);
        return;
    }
    follow(e, f, &st, at, ev.time);
}

fn release(e: &Engine, f: ObjectId, ev: &PointerEvent, at: kurbo::Point) {
    if let Some(st) = state_of(e, f) {
        let_go(e, f, &st, ev.time, Some(at));
    }
}

fn cancel(e: &Engine, f: ObjectId) {
    if let Some(st) = state_of(e, f) {
        let_go(e, f, &st, e.now(), None);
    }
}

fn hover(_: &Engine, _: ObjectId, _: bool) {}

pub(crate) fn hooks() -> PointerItem {
    PointerItem {
        press,
        moved,
        release,
        cancel,
        hover,
    }
}
