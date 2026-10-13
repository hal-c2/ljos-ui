//! Content that moves by itself, frame by frame: flings, overshoot,
//! rebound and a wheel that glides.
//!
//! What the content does is asked of the [`ScrollPhysics`](super::ScrollPhysics) as a curve
//! ([`Segment`]); this module only follows curves. Each frame the scene
//! calls [`tick`], which puts every moving Flickable where its curve says
//! at the frame's time, and [`next_wake`], which asks for the next frame
//! while anything moves and for none once everything rests: nothing runs
//! after a fling has settled.
//!
//! A Flickable with a `rebound` transition comes back through it: a
//! hidden item stands in for the content, the transition moves the item's
//! `x` and `y`, and the content follows them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use ljos_engine::{Engine, ObjectId, PropIndex, Value};
use ljos_runtime::Time;

use super::physics::{AxisState, FlickSettings, Motion, Segment, scroll_physics};
use super::{
    FlickState, Flickable, X, Y, bounds, bounds_behavior, bounds_movement, flag, is_flickable, num,
    put, put_flag, scroll_to, state_of, sync_moving, warn,
};
use crate::animation::driver;
use crate::animation::timeline::{Change, ChangeKind, Changes, Timed};
use crate::animation::transition::animations;
use crate::item::Item;

/// The frame period asked for while content moves.
const FRAME: Duration = Duration::from_nanos(16_666_667);

/// A tree deeper than this is a cycle someone built by hand.
const DEPTH: usize = 4096;

/// Why the content moves by itself, which decides what it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A fling, or the rebound after one or after a drag: `flicking` and
    /// `moving`, with `flickStarted` and `flickEnded`.
    Flick,
    /// `returnToBounds()`: neither flicking nor moving (measured on qml6).
    Quiet,
    /// A wheel that glides: moving, not flicking.
    Wheel,
}

#[derive(Clone, Copy, Debug)]
enum Start {
    /// Asked for at this time of the engine's clock; the first frame works
    /// out what that is in its own.
    Called(Time),
    /// The frame time the segment started at.
    At(Time),
}

#[derive(Clone, Copy, Debug)]
struct AxisRun {
    seg: Segment,
    start: Start,
    kind: Kind,
    /// The bounds `seg` was worked out in: other bounds work it out anew.
    min: f64,
    max: f64,
    /// A `rebound` transition moves it, not `seg`.
    driven: bool,
}

/// What a Flickable has moving by itself.
#[derive(Debug, Default)]
pub(crate) struct FlingState {
    runs: Cell<[Option<AxisRun>; 2]>,
    /// The stand-in the `rebound` transition moves.
    proxy: Cell<Option<ObjectId>>,
    /// Counts rebound transitions, so that one that was replaced does not
    /// end what replaced it.
    serial: Cell<u64>,
    /// The directions a press caught moving. They still count as moving
    /// until the press ends, and only then is `flickEnded` said (measured
    /// on qml6).
    caught: Cell<[bool; 2]>,
    /// What was caught was a fling, which has `flickEnded` to say.
    caught_flick: Cell<bool>,
}

impl FlingState {
    /// The directions moving by themselves, of any kind.
    pub(crate) fn axes(&self) -> [bool; 2] {
        self.runs.get().map(|r| r.is_some())
    }

    /// The directions that show as moving.
    pub(crate) fn moving(&self) -> [bool; 2] {
        let caught = self.caught.get();
        let runs = self.runs.get();
        [X, Y].map(|a| caught[a] || runs[a].is_some_and(|r| r.kind != Kind::Quiet))
    }

    /// The press that caught the content ended: what it caught ends now.
    /// Whether it caught a fling.
    pub(crate) fn release_caught(&self) -> bool {
        self.caught.set([false; 2]);
        self.caught_flick.replace(false)
    }

    fn flicking(&self) -> [bool; 2] {
        self.runs
            .get()
            .map(|r| r.is_some_and(|r| r.kind == Kind::Flick))
    }
}

/// The Flickables of an engine that have something moving.
#[derive(Default)]
struct Active(RefCell<Vec<ObjectId>>);

fn active(engine: &Engine) -> Rc<Active> {
    engine.extension::<Active>()
}

/// The `boundsBehavior`, deceleration and top speed of `f`.
pub(crate) fn settings(e: &Engine, f: ObjectId) -> FlickSettings {
    e.untrack(|| {
        let behavior = num(e, f, Flickable::BOUNDS_BEHAVIOR.index) as i32;
        FlickSettings {
            deceleration: num(e, f, Flickable::FLICK_DECELERATION.index),
            maximum_velocity: num(e, f, Flickable::MAXIMUM_FLICK_VELOCITY.index),
            drag_over: behavior & bounds_behavior::DRAG_OVER_BOUNDS != 0,
            overshoot: behavior & bounds_behavior::OVERSHOOT_BOUNDS != 0,
        }
    })
}

fn velocity_prop(axis: usize) -> PropIndex {
    if axis == X {
        Flickable::HORIZONTAL_VELOCITY.index
    } else {
        Flickable::VERTICAL_VELOCITY.index
    }
}

fn overshoot_prop(axis: usize) -> PropIndex {
    if axis == X {
        Flickable::HORIZONTAL_OVERSHOOT.index
    } else {
        Flickable::VERTICAL_OVERSHOOT.index
    }
}

fn flicking_prop(axis: usize) -> PropIndex {
    if axis == X {
        Flickable::FLICKING_HORIZONTALLY.index
    } else {
        Flickable::FLICKING_VERTICALLY.index
    }
}

/// Whether the content stops at its bounds while the overshoot says how
/// far it would have gone (`boundsMovement: StopAtBounds`).
fn stops_at_bounds(e: &Engine, f: ObjectId) -> bool {
    e.untrack(|| {
        num(e, f, Flickable::BOUNDS_MOVEMENT.index) as i32 == bounds_movement::STOP_AT_BOUNDS
    })
}

/// Where the content would be in one direction, past its bounds too: the
/// position plus what `boundsMovement` kept it from.
pub(crate) fn virtual_position(e: &Engine, f: ObjectId, axis: usize) -> AxisState {
    let mut state = e.untrack(|| bounds(e, f, axis));
    if stops_at_bounds(e, f) {
        state.position += e.untrack(|| num(e, f, overshoot_prop(axis)));
    }
    state
}

/// Puts the content at `at` in one direction, which may be past its
/// bounds, with the overshoot and velocity that go with it.
pub(crate) fn place(e: &Engine, f: ObjectId, axis: usize, at: f64, velocity: f64) {
    let state = e.untrack(|| bounds(e, f, axis));
    let inside = state.clamp(at);
    let shown = if stops_at_bounds(e, f) { inside } else { at };
    e.batch(|| {
        scroll_to(e, f, axis, shown);
        put(e, f, overshoot_prop(axis), Value::Real(at - inside));
        put(e, f, velocity_prop(axis), Value::Real(velocity));
    });
}

/// Shows the velocity of a drag.
pub(crate) fn show_velocity(e: &Engine, f: ObjectId, axis: usize, velocity: f64) {
    put(e, f, velocity_prop(axis), Value::Real(velocity));
}

/// Brings `flicking` in step with the runs, and says when flicking starts
/// and ends.
fn show_flicking(e: &Engine, f: ObjectId, st: &FlickState) {
    let now = st.fling.flicking();
    let any = now[X] || now[Y];
    let was = e.untrack(|| flag(e, f, Flickable::FLICKING.index));
    e.batch(|| {
        for axis in [X, Y] {
            put_flag(e, f, flicking_prop(axis), now[axis]);
        }
        put_flag(e, f, Flickable::FLICKING.index, any);
    });
    if any != was {
        let signal = if any { "flickStarted" } else { "flickEnded" };
        warn(e, signal, e.emit_by_name(f, signal, &[]));
    }
}

/// Starts the curves `segs` for the directions that have one, replacing
/// what those directions did. The flags change after the caller has said
/// what it has to (a drag that ends says so before the flick starts):
/// call [`show`] then.
pub(crate) fn begin(
    e: &Engine,
    f: ObjectId,
    st: &FlickState,
    segs: [Option<Segment>; 2],
    kind: Kind,
) {
    let mut runs = st.fling.runs.get();
    let mut any = false;
    for axis in [X, Y] {
        let Some(seg) = segs[axis] else { continue };
        let b = e.untrack(|| bounds(e, f, axis));
        runs[axis] = Some(AxisRun {
            seg,
            start: Start::Called(e.now()),
            kind,
            min: b.min,
            max: b.max,
            driven: false,
        });
        any = true;
    }
    if !any {
        return;
    }
    st.fling.runs.set(runs);
    let list = active(e);
    if !list.0.borrow().contains(&f) {
        list.0.borrow_mut().push(f);
    }
    e.request_wake();
    if let Some(rebounds) = rebound_axes(&segs, kind) {
        rebound_through_transition(e, f, st, rebounds);
    }
}

/// Shows what [`begin`] started: `moving` first, then `flicking`, as Qt
/// says them.
pub(crate) fn show(e: &Engine, f: ObjectId, st: &FlickState) {
    sync_moving(e, f, st);
    show_flicking(e, f, st);
}

/// The directions of `segs` that are rebounds a transition could run.
fn rebound_axes(segs: &[Option<Segment>; 2], kind: Kind) -> Option<[bool; 2]> {
    if kind == Kind::Wheel {
        return None;
    }
    let axes = segs.map(|s| matches!(s, Some(Segment::Rebound { .. })));
    (axes[X] || axes[Y]).then_some(axes)
}

/// Stops the content where it is in the directions in `axes`. What stops
/// is said: `flickEnded`, `movementEnded`.
pub(crate) fn stop(e: &Engine, f: ObjectId, st: &FlickState, axes: [bool; 2]) {
    let mut runs = st.fling.runs.get();
    let mut stopped = false;
    for axis in [X, Y] {
        if axes[axis] && runs[axis].take().is_some() {
            stopped = true;
            show_velocity(e, f, axis, 0.0);
        }
    }
    if !stopped {
        return;
    }
    st.fling.runs.set(runs);
    if st.fling.axes() == [false; 2] {
        forget_rebound(e, st);
    }
    show_flicking(e, f, st);
    sync_moving(e, f, st);
}

/// Stops whatever moves `f` by itself.
pub(crate) fn stop_all(e: &Engine, f: ObjectId) {
    if let Some(st) = state_of(e, f) {
        stop(e, f, &st, [true; 2]);
    }
}

/// Starts a fling of `f` at `velocity` per direction (pixels per second,
/// positive towards the end), as `flick()` does.
pub(crate) fn flick(e: &Engine, f: ObjectId, velocity: [f64; 2]) {
    let Some(st) = state_of(e, f) else { return };
    let physics = scroll_physics(e);
    let s = settings(e, f);
    let mut segs = [None; 2];
    for axis in [X, Y] {
        if velocity[axis] == 0.0 || !e.untrack(|| super::can_flick(e, f, axis)) {
            continue;
        }
        let mut state = virtual_position(e, f, axis);
        state.velocity = velocity[axis];
        if let Motion::Start(seg) = physics.released(&state, &s) {
            segs[axis] = Some(seg);
        }
    }
    if segs == [None; 2] {
        return;
    }
    begin(e, f, &st, segs, Kind::Flick);
    show(e, f, &st);
}

/// Brings content that is past its bounds back, as `returnToBounds()`
/// does: with the rebound, or at once if the physics has none.
pub(crate) fn return_to_bounds(e: &Engine, f: ObjectId) {
    let Some(st) = state_of(e, f) else { return };
    let physics = scroll_physics(e);
    let mut segs = [None; 2];
    for axis in [X, Y] {
        let state = virtual_position(e, f, axis);
        match physics.return_to_bounds(&state) {
            Motion::Start(seg) => segs[axis] = Some(seg),
            Motion::Jump(to) => place(e, f, axis, to, 0.0),
            _ => {}
        }
    }
    begin(e, f, &st, segs, Kind::Quiet);
}

/// Starts the wheel's glide in one direction. A glide under way goes on
/// from where it is.
pub(crate) fn glide(e: &Engine, f: ObjectId, axis: usize, seg: Segment) {
    let Some(st) = state_of(e, f) else { return };
    let mut segs = [None; 2];
    segs[axis] = Some(seg);
    begin(e, f, &st, segs, Kind::Wheel);
}

/// Whether one direction of `f` is moving by itself.
pub(crate) fn runs(e: &Engine, f: ObjectId, axis: usize) -> bool {
    state_of(e, f).is_some_and(|st| st.fling.axes()[axis])
}

/// A press came down on `hits` (topmost first): the innermost Flickable
/// around it that is flicking stops, and takes the press, so that what
/// was flicked past is not clicked.
pub(crate) fn catch(engine: &Engine, hits: &[ObjectId]) -> Option<ObjectId> {
    let mut cur = hits.first().copied();
    for _ in 0..DEPTH {
        let f = cur?;
        cur = engine.untrack(|| engine.visual_parent(f));
        let takes = || {
            engine.untrack(|| {
                flag(engine, f, Flickable::INTERACTIVE.index)
                    && flag(engine, f, Item::ENABLED.index)
            })
        };
        if !is_flickable(engine, f) || !takes() {
            continue;
        }
        let Some(st) = state_of(engine, f) else {
            continue;
        };
        let moving = st.fling.moving();
        if moving == [false; 2] {
            continue;
        }
        let flicking = st.fling.flicking();
        let runs = st.fling.runs.replace([None; 2]);
        for axis in [X, Y] {
            if runs[axis].is_some() {
                show_velocity(engine, f, axis, 0.0);
            }
        }
        forget_rebound(engine, &st);
        st.fling.caught.set(moving);
        st.fling
            .caught_flick
            .set(st.fling.caught_flick.get() || flicking[X] || flicking[Y]);
        // `flicking` drops at once; `flickEnded` and `movementEnded` wait
        // for the press to end.
        engine.batch(|| {
            for axis in [X, Y] {
                put_flag(engine, f, flicking_prop(axis), false);
            }
            put_flag(engine, f, Flickable::FLICKING.index, false);
        });
        return Some(f);
    }
    None
}

/// Frame step: every Flickable that moves by itself is put where its
/// curves say at `now`.
pub(crate) fn tick(engine: &Engine, now: Time) {
    let list = active(engine);
    if list.0.borrow().is_empty() {
        return;
    }
    let flickables: Vec<ObjectId> = list.0.borrow().clone();
    let mut ended = Vec::new();
    engine.batch(|| {
        for f in &flickables {
            if let Some(st) = state_of(engine, *f) {
                let done = advance(engine, *f, &st, now);
                if done != [false; 2] {
                    ended.push((*f, st, done));
                }
            }
        }
    });
    // Said after the batch: what a handler of `flickEnded` reads is where
    // the content came to rest.
    for (f, st, done) in ended {
        let mut runs = st.fling.runs.get();
        for axis in [X, Y] {
            if done[axis] {
                runs[axis] = None;
            }
        }
        st.fling.runs.set(runs);
        show_flicking(engine, f, &st);
        sync_moving(engine, f, &st);
    }
    list.0.borrow_mut().retain(|f| {
        engine.is_alive(*f) && state_of(engine, *f).is_some_and(|st| st.fling.axes() != [false; 2])
    });
}

/// The next frame the moving content needs; `None` once nothing moves.
pub(crate) fn next_wake(engine: &Engine, now: Time) -> Option<Time> {
    let list = active(engine);
    let any = list.0.borrow().iter().any(|f| {
        state_of(engine, *f)
            .is_some_and(|st| st.fling.runs.get().iter().flatten().any(|r| !r.driven))
    });
    any.then(|| now + FRAME)
}

/// Moves the curves of `f` to `now`. Returns the directions that came to
/// rest; they are still in the runs, for the caller to end.
fn advance(e: &Engine, f: ObjectId, st: &FlickState, now: Time) -> [bool; 2] {
    let physics = scroll_physics(e);
    let Some(k) = physics.kinetic().copied() else {
        return st.fling.axes();
    };
    let s = settings(e, f);
    let mut runs = st.fling.runs.get();
    let mut done = [false; 2];
    for axis in [X, Y] {
        let Some(mut run) = runs[axis] else { continue };
        if run.driven {
            continue;
        }
        let mut t0 = match run.start {
            // The time between the call and this frame has passed for it.
            Start::Called(at) => now.saturating_sub(e.now().saturating_since(at)),
            Start::At(t) => t,
        };
        let b = e.untrack(|| bounds(e, f, axis));
        if (b.min, b.max) != (run.min, run.max) && !matches!(run.seg, Segment::Glide { .. }) {
            // The content changed size under it (a view that made rows):
            // the rest of the way is worked out in the bounds there are.
            let (at, v) = run.seg.at(&k, now.saturating_since(t0));
            let state = AxisState {
                position: at,
                velocity: v,
                ..b
            };
            match physics.next(&state, &s) {
                Some(seg) => {
                    run.seg = seg;
                    t0 = now;
                }
                None => {
                    place(e, f, axis, at, 0.0);
                    done[axis] = true;
                    continue;
                }
            }
            (run.min, run.max) = (b.min, b.max);
        }
        // Segments that end before `now` hand on to the next; a bounded
        // number, since each is worked out from where the last one ended.
        let mut ended = false;
        for _ in 0..8 {
            let length = run.seg.duration(&k);
            let elapsed = now.saturating_since(t0);
            if elapsed < length {
                break;
            }
            let (at, v) = run.seg.at(&k, length);
            let state = AxisState {
                position: at,
                velocity: v,
                ..b
            };
            let next = match run.seg {
                // A glide ends where the wheel sent it.
                Segment::Glide { .. } => None,
                _ => physics.next(&state, &s),
            };
            match next {
                Some(seg @ Segment::Snap { .. }) => {
                    // As on qml6 the content is seen at the farthest it went
                    // for one frame before it jumps back. Clamped: the
                    // braking ends a rounding past the limit.
                    let far = at.clamp(b.min - k.max_overshoot, b.max.max(b.min) + k.max_overshoot);
                    place(e, f, axis, far, v);
                    t0 = now;
                    run.seg = seg;
                    break;
                }
                Some(seg) => {
                    t0 = t0.saturating_add(length);
                    run.seg = seg;
                    if matches!(seg, Segment::Rebound { .. })
                        && run.kind != Kind::Wheel
                        && rebound_transition(e, f).is_some()
                    {
                        place(e, f, axis, at, 0.0);
                        run.driven = true;
                        runs[axis] = Some(run);
                        st.fling.runs.set(runs);
                        let mut axes = [false; 2];
                        axes[axis] = true;
                        rebound_through_transition(e, f, st, axes);
                        runs = st.fling.runs.get();
                        break;
                    }
                }
                None => {
                    place(e, f, axis, at, 0.0);
                    ended = true;
                    break;
                }
            }
        }
        if ended {
            done[axis] = true;
            continue;
        }
        if run.driven {
            continue;
        }
        // A snap that starts now waits for the next frame (above).
        if !matches!(run.seg, Segment::Snap { .. }) {
            let (at, v) = run.seg.at(&k, now.saturating_since(t0));
            place(e, f, axis, at, v);
        }
        run.start = Start::At(t0);
        runs[axis] = Some(run);
    }
    st.fling.runs.set(runs);
    done
}

/// The `rebound` transition of `f`, if it has one to run.
fn rebound_transition(e: &Engine, f: ObjectId) -> Option<ObjectId> {
    e.untrack(|| match e.read(f, Flickable::REBOUND.index) {
        Value::Object(t) if e.is_alive(t) => Some(t),
        _ => None,
    })
    .filter(|t| !animations(e, *t).is_empty())
}

/// The hidden item the `rebound` transition moves; the content follows
/// its `x` and `y` while a rebound is driven.
fn proxy(e: &Engine, f: ObjectId, st: &FlickState) -> Option<ObjectId> {
    if let Some(p) = st.fling.proxy.get().filter(|p| e.is_alive(*p)) {
        return Some(p);
    }
    let made = e.create_native(Item::default(), Some(f)).and_then(|p| {
        e.complete(p)?;
        for (axis, prop) in [(X, Item::X.index), (Y, Item::Y.index)] {
            e.watch(p, prop, move |e, _| {
                let Some(st) = state_of(e, f) else { return };
                if st.fling.runs.get()[axis].is_some_and(|r| r.driven) {
                    let at = e.untrack(|| num(e, p, prop));
                    place(e, f, axis, at, 0.0);
                }
            })?;
        }
        Ok(p)
    });
    match made {
        Ok(p) => {
            st.fling.proxy.set(Some(p));
            Some(p)
        }
        Err(err) => {
            warn(e, "rebound", Err(err));
            None
        }
    }
}

/// Hands the rebound of the directions in `axes` to the `rebound`
/// transition, if `f` has one. The runs of those directions are marked
/// driven; the transition's end ends them.
fn rebound_through_transition(e: &Engine, f: ObjectId, st: &FlickState, axes: [bool; 2]) {
    let Some(transition) = rebound_transition(e, f) else {
        return;
    };
    let Some(p) = proxy(e, f, st) else { return };
    let mut runs = st.fling.runs.get();
    let mut changes = Changes::default();
    for (axis, prop) in [(X, Item::X.index), (Y, Item::Y.index)] {
        let Some(run) = runs[axis].as_mut().filter(|_| axes[axis]) else {
            continue;
        };
        let Segment::Rebound { from, to } = run.seg else {
            continue;
        };
        run.driven = true;
        // The stand-in starts where the content is; nothing follows it
        // until the run is marked driven, which it is by now.
        let _ = e.write_internal(
            p,
            prop,
            Value::Real(from),
            ljos_engine::WriteKind::Imperative,
        );
        changes.list.push(Change {
            obj: p,
            prop,
            to: Value::Real(to),
            kind: ChangeKind::Property,
        });
    }
    st.fling.runs.set(runs);
    if changes.list.is_empty() {
        return;
    }
    let serial = st.fling.serial.get() + 1;
    st.fling.serial.set(serial);
    let timeline = Timed::of_transition(e, &animations(e, transition), &changes);
    let ends: Vec<(usize, f64)> = changes
        .list
        .iter()
        .map(|c| {
            (
                if c.prop == Item::X.index { X } else { Y },
                c.to.to_number(),
            )
        })
        .collect();
    let done: driver::Done = Rc::new(move |e| {
        let Some(st) = state_of(e, f) else { return };
        if st.fling.serial.get() != serial {
            return;
        }
        let mut runs = st.fling.runs.get();
        for (axis, to) in &ends {
            if runs[*axis].is_some_and(|r| r.driven) {
                place(e, f, *axis, *to, 0.0);
                runs[*axis] = None;
            }
        }
        st.fling.runs.set(runs);
        if st.fling.axes() == [false; 2] {
            active(e).0.borrow_mut().retain(|o| *o != f);
        }
        show_flicking(e, f, &st);
        sync_moving(e, f, &st);
    });
    driver::start_timeline(e, p, timeline, Some(done));
}

/// Ends a rebound transition that is under way, leaving the content where
/// it is.
fn forget_rebound(e: &Engine, st: &FlickState) {
    st.fling.serial.set(st.fling.serial.get() + 1);
    if let Some(p) = st.fling.proxy.get().filter(|p| e.is_alive(*p)) {
        driver::forget_driven(e, p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebounds_are_found_for_a_transition() {
        let r = Segment::Rebound { from: 1.0, to: 0.0 };
        let d = Segment::Decelerate {
            from: 0.0,
            velocity: 1.0,
            rate: 1.0,
            duration: Duration::ZERO,
        };
        assert_eq!(
            rebound_axes(&[None, Some(r)], Kind::Quiet),
            Some([false, true])
        );
        assert_eq!(rebound_axes(&[Some(d), None], Kind::Flick), None);
        assert_eq!(rebound_axes(&[Some(r), None], Kind::Wheel), None);
    }
}
