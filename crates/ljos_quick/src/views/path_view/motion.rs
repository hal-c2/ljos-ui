//! `offset` and `currentIndex`, and `offset` moving by itself.
//!
//! With `StrictlyEnforceRange` the two say the same thing: the current row
//! is the one at the highlight, `round(n - offset) mod n`. A change of
//! `offset` (a drag, a flick, an assignment) makes the row there current;
//! a change of `currentIndex` moves `offset` there over
//! `highlightMoveDuration`, the current row changing at once (qml6). In
//! the other modes the two are independent.
//!
//! A run of `offset` is worked out from the engine's clock each frame; the
//! view asks for frames only while one runs.
//!
//! A flick is the engine's scroll physics with a PathView's numbers, all
//! measured on qml6 frame by frame. The rows slow down evenly by
//! `flickDeceleration / 10` places a second each second (100 gives 1000
//! pixels a second each second on a path of 100 pixels a place, 2000 on
//! one of 200, with `pathItemCount` or without); the release velocity and
//! `maximumFlickVelocity` are pixels along the path; and a flick goes at
//! most `count - 1` places, slowing down harder to stop there. With a
//! strict range or a `snapMode` the rows rest on the row nearest a quarter
//! of a place beyond where they would have stopped, and get there by
//! slowing down evenly from the release velocity; `SnapOneItem` goes to
//! its row at `maximumFlickVelocity`.

use std::cell::RefCell;
use std::time::Duration;

use ljos_engine::{Engine, ObjectId, PropIndex, Value};
use ljos_runtime::Time;

use super::{
    PathView, State, highlight_range, movement_direction, put, range_mode, rd, snap_mode, state_of,
};
use crate::views::position_mode;
use crate::views::warn;

const FRAME: Duration = Duration::from_nanos(16_666_667);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// To the current row: `moving` stays as it is.
    Glide,
    /// After a release that had speed: `flicking` while it runs.
    Flick,
    /// After a release without speed, to a whole row.
    Settle,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Run {
    pub kind: Kind,
    pub from: f64,
    pub delta: f64,
    pub start: Time,
    pub micros: u64,
}

impl Run {
    fn eased(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self.kind {
            // `Easing.InOutQuad`.
            Kind::Glide => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            // `Easing.OutQuad`: a speed that falls evenly to nothing.
            Kind::Flick | Kind::Settle => 1.0 - (1.0 - t) * (1.0 - t),
        }
    }
}

/// The views whose `offset` runs.
#[derive(Default)]
struct Active(RefCell<Vec<ObjectId>>);

/// `v` brought into `0..n`, a hair under a whole number taken as it.
pub(crate) fn wrap(v: f64, n: usize) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let n = n as f64;
    let mut w = v.rem_euclid(n);
    if (w - w.round()).abs() < 1e-9 {
        w = w.round();
    }
    if w >= n { 0.0 } else { w }
}

fn rows(st: &State) -> usize {
    st.delegates().map_or(0, |d| d.count())
}

fn strict(e: &Engine, view: ObjectId) -> bool {
    range_mode(e, view) == highlight_range::STRICT
}

fn current(e: &Engine, view: ObjectId) -> i64 {
    rd(e, view, PathView::CURRENT_INDEX.index) as i64
}

fn offset(e: &Engine, view: ObjectId) -> f64 {
    rd(e, view, PathView::OFFSET.index)
}

/// The row at the highlight when `offset` is `o`.
fn row_at(o: f64, n: usize) -> i64 {
    ((n as f64 - o).round() as i64).rem_euclid(n as i64)
}

/// The `offset` that brings row `i` to the highlight.
fn offset_of(i: i64, n: usize) -> f64 {
    wrap(n as f64 - i as f64, n)
}

pub(crate) fn write_offset(e: &Engine, view: ObjectId, st: &State, v: f64) {
    st.writing_offset.set(true);
    put(e, view, PathView::OFFSET.index, Value::Real(v));
    st.writing_offset.set(false);
}

fn write_current(e: &Engine, view: ObjectId, st: &State, i: i64) {
    st.writing_current.set(true);
    warn(
        e,
        "currentIndex",
        e.set_internal(view, PathView::CURRENT_INDEX, i as i32),
    );
    st.writing_current.set(false);
}

/// Shows a flag of the view and says so with its signal.
pub(crate) fn show(e: &Engine, view: ObjectId, p: PropIndex, on: bool, started: &str, ended: &str) {
    let was = e.untrack(|| e.read(view, p)).to_boolean();
    if was == on {
        return;
    }
    put(e, view, p, Value::Bool(on));
    warn(
        e,
        "signal",
        e.emit_by_name(view, if on { started } else { ended }, &[]),
    );
}

pub(crate) fn show_moving(e: &Engine, view: ObjectId, on: bool) {
    show(
        e,
        view,
        PathView::MOVING.index,
        on,
        "movementStarted",
        "movementEnded",
    );
}

pub(crate) fn show_flicking(e: &Engine, view: ObjectId, on: bool) {
    show(
        e,
        view,
        PathView::FLICKING.index,
        on,
        "flickStarted",
        "flickEnded",
    );
}

pub(crate) fn show_dragging(e: &Engine, view: ObjectId, on: bool) {
    show(
        e,
        view,
        PathView::DRAGGING.index,
        on,
        "dragStarted",
        "dragEnded",
    );
}

/// Starts `run`, in place of one there was.
pub(crate) fn begin(e: &Engine, view: ObjectId, st: &State, run: Run) {
    st.motion.set(Some(run));
    let a = e.extension::<Active>();
    if !a.0.borrow().contains(&view) {
        a.0.borrow_mut().push(view);
    }
    e.request_wake();
}

/// Ends the run there is where it is, as an interruption: a flick still
/// says that it ended.
pub(crate) fn interrupt(e: &Engine, view: ObjectId, st: &State) {
    if let Some(run) = st.motion.take() {
        if run.kind == Kind::Flick {
            show_flicking(e, view, false);
        }
        if run.kind != Kind::Glide {
            show_moving(e, view, false);
        }
    }
}

/// A model was given: one that replaces a model with rows starts from
/// its first row; the first one takes what was given before it.
pub(crate) fn sourced(e: &Engine, view: ObjectId, st: &State) {
    let n = rows(st);
    if n == 0 {
        return;
    }
    if st.sourced.replace(true) {
        interrupt(e, view, st);
        write_offset(e, view, st, 0.0);
        write_current(e, view, st, 0);
        return;
    }
    settle_now(e, view, st, n);
}

/// Rows came or went.
pub(crate) fn rows_changed(e: &Engine, view: ObjectId, st: &State, n: usize) {
    if n == 0 {
        return;
    }
    if !st.sourced.replace(true) {
        settle_now(e, view, st, n);
        return;
    }
    let c = current(e, view);
    if c >= n as i64 {
        write_current(e, view, st, n as i64 - 1);
    }
    settle_now(e, view, st, n);
}

/// Brings `offset` into range and, in strict mode, in step with
/// `currentIndex` at once.
fn settle_now(e: &Engine, view: ObjectId, st: &State, n: usize) {
    let c = current(e, view);
    if !(0..n as i64).contains(&c) {
        write_current(e, view, st, c.rem_euclid(n as i64));
    }
    let o = offset(e, view);
    let w = wrap(o, n);
    if w != o {
        write_offset(e, view, st, w);
    }
    if st.motion.get().is_some() || !strict(e, view) {
        return;
    }
    let c = current(e, view);
    if row_at(w, n) == c {
        return;
    }
    if c != 0 || w == 0.0 {
        write_offset(e, view, st, offset_of(c, n));
    } else {
        write_current(e, view, st, row_at(w, n));
    }
}

/// `currentIndex` changed.
pub(crate) fn current_changed(e: &Engine, view: ObjectId) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    if st.writing_current.get() {
        return;
    }
    let n = rows(&st);
    if n == 0 {
        return;
    }
    let c = current(e, view);
    if !(0..n as i64).contains(&c) {
        // Comes round: 7 of 3 rows is row 1, and -1 the last (qml6).
        warn(
            e,
            "currentIndex",
            e.set_internal(view, PathView::CURRENT_INDEX, c.rem_euclid(n as i64) as i32),
        );
        return;
    }
    match range_mode(e, view) {
        highlight_range::STRICT => {
            let o = offset(e, view);
            let gliding = st.motion.get().is_some_and(|r| r.kind == Kind::Glide);
            if !gliding && row_at(o, n) == c {
                return;
            }
            glide(e, view, &st, n, offset_of(c, n));
        }
        highlight_range::APPLY => keep_in_range(e, view, &st, n, c),
        _ => {}
    }
}

/// `ApplyRange`: row `c`, counted forward from the beginning of the range,
/// is brought back to its end when it is past it. Counted so, a row is
/// never before the beginning; one just before it on the path is brought
/// all the way round (qml6).
fn keep_in_range(e: &Engine, view: ObjectId, st: &State, n: usize, c: i64) {
    let Some(f) = super::layout::frame_of(e, view, st, n) else {
        return;
    };
    let k = f.k as f64;
    let begin = rd(e, view, PathView::PREFERRED_HIGHLIGHT_BEGIN.index);
    let end = rd(e, view, PathView::PREFERRED_HIGHLIGHT_END.index);
    let o = offset(e, view);
    let nf = n as f64;
    let mut at = (c as f64 + o).rem_euclid(nf);
    if nf - at < 1e-9 {
        at = 0.0;
    }
    let past = at + (begin - end) * k;
    if past > 1e-9 {
        glide(e, view, st, n, wrap(o - past, n));
    }
}

/// Moves `offset` to `target`.
fn glide(e: &Engine, view: ObjectId, st: &State, n: usize, target: f64) {
    let o = offset(e, view);
    let nf = n as f64;
    let ahead = (target - o).rem_euclid(nf);
    let delta = match rd(e, view, PathView::MOVEMENT_DIRECTION.index) as i32 {
        // `offset` goes up.
        movement_direction::NEGATIVE => ahead,
        // `offset` goes down.
        movement_direction::POSITIVE => {
            if ahead == 0.0 {
                0.0
            } else {
                ahead - nf
            }
        }
        _ => {
            if ahead > nf / 2.0 {
                ahead - nf
            } else {
                ahead
            }
        }
    };
    let ms = rd(e, view, PathView::HIGHLIGHT_MOVE_DURATION.index);
    if let Some(run) = st.motion.get()
        && run.kind != Kind::Glide
    {
        interrupt(e, view, st);
    }
    if ms <= 0.0 || delta.abs() < 1e-12 {
        st.motion.set(None);
        write_offset(e, view, st, target);
        return;
    }
    begin(
        e,
        view,
        st,
        Run {
            kind: Kind::Glide,
            from: o,
            delta,
            start: e.now(),
            micros: (ms * 1000.0) as u64,
        },
    );
}

/// `offset` changed.
pub(crate) fn offset_changed(e: &Engine, view: ObjectId) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let n = rows(&st);
    if !st.writing_offset.get() {
        // Someone else moved it: whatever ran stops.
        interrupt(e, view, &st);
        let o = offset(e, view);
        let w = wrap(o, n);
        if n > 0 && w != o {
            write_offset(e, view, &st, w);
        }
    } else if st.motion.get().is_some_and(|r| r.kind == Kind::Glide) {
        return;
    }
    if n > 0 && strict(e, view) {
        let r = row_at(offset(e, view), n);
        if r != current(e, view) {
            write_current(e, view, &st, r);
        }
    }
}

/// `incrementCurrentIndex()` and `decrementCurrentIndex()`.
pub(crate) fn step(e: &Engine, view: ObjectId, by: i64) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let n = rows(&st) as i64;
    if n == 0 {
        return;
    }
    let next = (current(e, view) + by).rem_euclid(n);
    warn(
        e,
        "currentIndex",
        e.set_internal(view, PathView::CURRENT_INDEX, next as i32),
    );
}

/// `positionViewAtIndex(index, mode)`.
pub(crate) fn position_at(e: &Engine, view: ObjectId, index: i32, mode: i32) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let n = rows(&st);
    let Some(i) = usize::try_from(index).ok().filter(|i| *i < n) else {
        return;
    };
    interrupt(e, view, &st);
    let Some(f) = super::layout::frame_of(e, view, &st, n) else {
        return;
    };
    let ranged = range_mode(e, view) != highlight_range::NONE;
    let k = f.k as f64;
    let rel = f.rel(i);
    let place = match mode {
        position_mode::CENTER => k / 2.0,
        // With a range, the last place; without one, the end of the path,
        // which is where the first place is (qml6).
        position_mode::END if ranged => k - 1.0,
        position_mode::END => k,
        // Where the highlight begins.
        position_mode::SNAP_POSITION if ranged => f.shift - rd(e, view, PathView::OFFSET.index),
        position_mode::CONTAIN | position_mode::VISIBLE => {
            if rel < k {
                return;
            }
            // To whichever end of the path is nearer.
            if rel - (k - 1.0) <= n as f64 - rel {
                k - 1.0
            } else {
                0.0
            }
        }
        _ => 0.0,
    };
    let o = offset(e, view);
    write_offset(e, view, &st, wrap(o + place - rel, n));
}

/// What the engine's [`ScrollPhysics`](crate::flickable::physics::ScrollPhysics)
/// does with a release of `v` pixels a second along a path of
/// `px_per_place` pixels a place: how fast the rows go on and how far,
/// in pixels. A path has no bounds, so the physics only slows them down.
fn thrown(e: &Engine, view: ObjectId, v: f64, o: f64, px_per_place: f64) -> (f64, f64, f64) {
    use crate::flickable::physics::{AxisState, FlickSettings, Motion, Segment, scroll_physics};
    // qml6 slows the rows down by a tenth of `flickDeceleration` places a
    // second each second, whatever a place's length; its velocities are
    // pixels along the path (measured, see the module comment).
    let s = FlickSettings {
        deceleration: rd(e, view, PathView::FLICK_DECELERATION.index).max(1.0) / 10.0
            * px_per_place,
        maximum_velocity: rd(e, view, PathView::MAXIMUM_FLICK_VELOCITY.index).max(0.0),
        drag_over: false,
        overshoot: false,
    };
    let axis = AxisState {
        position: o * px_per_place,
        min: f64::NEG_INFINITY,
        max: f64::INFINITY,
        velocity: v,
    };
    match scroll_physics(e).released(&axis, &s) {
        Motion::Start(Segment::Decelerate { velocity, rate, .. }) if rate > 0.0 => (
            velocity,
            velocity * velocity.abs() / (2.0 * rate),
            s.deceleration,
        ),
        _ => (0.0, 0.0, s.deceleration),
    }
}

/// A release: the rows go on at the speed they had, `v` pixels a second
/// along the path, slowing down evenly, and come to rest where the snap
/// mode says; a rest that is moved to a whole row is reached by slowing
/// down evenly to it (qml6).
pub(crate) fn release(e: &Engine, view: ObjectId, st: &State, v: f64) {
    let n = rows(st);
    let Some(f) = super::layout::frame_of(e, view, st, n) else {
        show_moving(e, view, false);
        return;
    };
    let px_per_place = if f.k > 0 {
        f.track.total / f.k as f64
    } else {
        0.0
    };
    if px_per_place <= 0.0 || !px_per_place.is_finite() {
        show_moving(e, view, false);
        return;
    }
    let o = offset(e, view);
    let (velocity, reach, rate) = thrown(e, view, v, o, px_per_place);
    // A flick goes at most one row short of all the way round (qml6).
    let lap = (n as f64 - 1.0).max(0.0) * px_per_place;
    let reach = if reach.is_finite() {
        reach.clamp(-lap, lap)
    } else {
        0.0
    };
    let mut end = o + reach / px_per_place;
    let snap = rd(e, view, PathView::SNAP_MODE.index) as i32;
    if strict(e, view) || snap != snap_mode::NO_SNAP {
        // The row nearest a quarter of a place beyond where the rows would
        // stop: 3.24 places rests on 3, 3.26 on 4, going up; going down
        // 6.76 rests on 7 and 6.74 on 6 (qml6).
        // Still, a rest halfway between two rows goes to the lower offset,
        // whichever way the rows were dragged (qml6).
        end = if velocity == 0.0 {
            (end - 0.5).ceil()
        } else {
            (end + 0.25 * velocity.signum()).round()
        };
    }
    let one = snap == snap_mode::SNAP_ONE_ITEM;
    if one {
        // One row on from the row nearest where it was let go.
        let from = o.round();
        end = end.clamp(from - 1.0, from + 1.0);
    }
    let delta = end - o;
    if delta.abs() < 1e-9 {
        show_moving(e, view, false);
        return;
    }
    let distance = (delta * px_per_place).abs();
    let flick = velocity.abs() > 1.0;
    let speed = if flick && velocity.signum() == delta.signum() {
        if one {
            // To the next row at the fastest a flick goes (qml6).
            rd(e, view, PathView::MAXIMUM_FLICK_VELOCITY.index).max(velocity.abs())
        } else {
            velocity.abs()
        }
    } else {
        // Settling without speed, or back against it: from the speed that
        // the deceleration brings to rest there.
        (2.0 * rate * distance).sqrt()
    };
    // `Easing.OutQuad` over `2 d / v` is even slowing down from `v` to
    // rest in `d`.
    let micros = (2.0 * distance / speed * 1e6) as u64;
    if flick {
        show_flicking(e, view, true);
    }
    begin(
        e,
        view,
        st,
        Run {
            kind: if flick { Kind::Flick } else { Kind::Settle },
            from: o,
            delta,
            start: e.now(),
            micros: micros.max(1),
        },
    );
}

/// Frame step: every view whose `offset` runs is moved to where it is at
/// this time.
pub(crate) fn tick(e: &Engine, _now: Time) {
    let a = e.extension::<Active>();
    if a.0.borrow().is_empty() {
        return;
    }
    let views: Vec<ObjectId> = a.0.borrow().clone();
    let at = e.now();
    for view in views {
        let Some(st) = state_of(e, view) else {
            continue;
        };
        let Some(run) = st.motion.get() else {
            continue;
        };
        let n = rows(&st);
        let t = at.saturating_since(run.start).as_micros() as f64 / run.micros.max(1) as f64;
        let done = t >= 1.0 || n == 0;
        let v = run.from + run.delta * run.eased(t);
        write_offset(e, view, &st, wrap(v, n));
        // A handler of the change may have started another run.
        let same = st
            .motion
            .get()
            .is_some_and(|r| r.start == run.start && r.kind == run.kind);
        if done && same {
            st.motion.set(None);
            match run.kind {
                Kind::Glide => {}
                Kind::Flick => {
                    show_flicking(e, view, false);
                    show_moving(e, view, false);
                }
                Kind::Settle => show_moving(e, view, false),
            }
        }
    }
    a.0.borrow_mut()
        .retain(|v| e.is_alive(*v) && state_of(e, *v).is_some_and(|st| st.motion.get().is_some()));
}

/// The next frame a running `offset` needs; `None` once none runs.
pub(crate) fn next_wake(e: &Engine, now: Time) -> Option<Time> {
    let a = e.extension::<Active>();
    let any =
        a.0.borrow()
            .iter()
            .any(|v| state_of(e, *v).is_some_and(|st| st.motion.get().is_some()));
    any.then(|| now + FRAME)
}
