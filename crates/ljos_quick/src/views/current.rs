//! The current row: which one it is, the keys and methods that move it,
//! and the highlight that goes to it.
//!
//! There is a highlight item whenever there is a current item, made from
//! `highlight` or, without one, a plain item (qml6 has one too:
//! `highlightItem` is never null while a row is current). With
//! `highlightFollowsCurrentItem` it is as large as the current item and
//! moves to it, at `highlightMoveVelocity` or over
//! `highlightMoveDuration`; the view goes with the highlight, not with
//! the row, which is why a view catches up with a far row only as the
//! highlight gets there.

use ljos_engine::{Engine, ObjectId, Value, WriteKind};

use super::state::{Geo, ViewState, follow_target};
use super::{ItemView, ListView, Part, enrol, pass, put, state_of, warn};
use crate::animation::{Animation, PropertyAnimation, SmoothedAnimation};
use crate::flickable::{self, X, Y};
use crate::input::KeyEvent;
use crate::item::{Item, is_item, is_unset, write_placed};
use crate::keys::{KeyItem, key};
use crate::tree;

/// How near its row the highlight is when it is there. Less than any
/// step an animation takes on its way, so the view follows to the end.
const ARRIVED: f64 = 1e-6;

/// A move of the current row. `Next` and `Previous` go by one row in the
/// order of the model; the four others go by what is on screen in a grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Next,
    Previous,
    Up,
    Down,
    Left,
    Right,
}

/// Settles which row is the current one for this pass, and returns it if
/// there is such a row. Without a word from the author the first row is.
pub(crate) fn normalise(e: &Engine, view: ObjectId, st: &ViewState, rows: usize) -> Option<usize> {
    let p = ItemView::CURRENT_INDEX.index;
    let ci = pass::rd(e, view, p) as i32;
    let first_pass = st.seen_index.get().is_none();
    if st.seen_index.get() != Some(ci) {
        // Somebody's assignment (or, on the first pass, declaration).
        let carried = st.carried.take().filter(|_| first_pass);
        if let Some(c) = carried {
            // The view is where the old one was, which had already gone
            // to its current row or been moved away from it.
            st.cleared.set(c.cleared);
            st.follow.set(false);
            st.auto_index.set(c.auto_index);
        } else {
            let said = !first_pass || !is_unset(e, view, p);
            st.cleared.set(ci < 0 && said);
            st.follow.set(said);
            st.auto_index.set(false);
        }
    }
    let mut want = ci;
    if ci < 0 && !st.cleared.get() && rows > 0 {
        want = 0;
        st.auto_index.set(true);
    } else if rows == 0 && ci >= 0 && st.auto_index.get() {
        want = -1;
    }
    st.seen_index.set(Some(want));
    if want != ci {
        put(e, view, p, Value::Int(want));
    }
    usize::try_from(want).ok().filter(|i| *i < rows)
}

/// Moves the current row; false when it stays where it is.
pub(crate) fn step(e: &Engine, view: ObjectId, step: Step) -> bool {
    let Some(st) = state_of(e, view) else {
        return false;
    };
    let rows = st.delegates().map_or(0, |d| d.count()) as i64;
    if rows == 0 {
        return false;
    }
    let (g, ci, wraps) = e.untrack(|| {
        (
            pass::read_geo(e, view, &st),
            pass::rd(e, view, ItemView::CURRENT_INDEX.index) as i64,
            e.read(view, ItemView::KEY_NAVIGATION_WRAPS.index)
                .to_boolean(),
        )
    });
    let per = g.per_line as i64;
    // One cell on across the lines, and one line on.
    let across = if g.cross_reversed { -1 } else { 1 };
    let along = if g.reversed { -per } else { per };
    let by = match (step, g.main == Y) {
        (Step::Next, _) => 1,
        (Step::Previous, _) => -1,
        (Step::Right, true) | (Step::Down, false) => across,
        (Step::Left, true) | (Step::Up, false) => -across,
        (Step::Down, true) | (Step::Right, false) => along,
        (Step::Up, true) | (Step::Left, false) => -along,
    };
    let mut to = ci + by;
    if ci < 0 && step == Step::Previous && wraps {
        to = rows - 1;
    }
    if !(0..rows).contains(&to) {
        if !wraps || by.abs() != 1 {
            return false;
        }
        to = to.rem_euclid(rows);
    }
    if to == ci {
        return false;
    }
    warn(
        e,
        "currentIndex",
        e.write(
            view,
            ItemView::CURRENT_INDEX.index,
            Value::Int(to as i32),
            WriteKind::Imperative,
        ),
    );
    true
}

fn press(e: &Engine, view: ObjectId, event: &KeyEvent) -> bool {
    let Some(st) = state_of(e, view) else {
        return false;
    };
    let enabled = e.untrack(|| {
        e.read(view, ItemView::KEY_NAVIGATION_ENABLED.index)
            .to_boolean()
    });
    if !enabled {
        return false;
    }
    let g = e.untrack(|| pass::read_geo(e, view, &st));
    let to = if g.grid {
        match event.key {
            key::LEFT => Step::Left,
            key::RIGHT => Step::Right,
            key::UP => Step::Up,
            key::DOWN => Step::Down,
            _ => return false,
        }
    } else {
        // The key that points the way the rows run goes to the next row.
        let (next, previous) = match (g.main == Y, g.reversed) {
            (true, false) => (key::DOWN, key::UP),
            (true, true) => (key::UP, key::DOWN),
            (false, false) => (key::RIGHT, key::LEFT),
            (false, true) => (key::LEFT, key::RIGHT),
        };
        if event.key == next {
            Step::Next
        } else if event.key == previous {
            Step::Previous
        } else {
            return false;
        }
    };
    // A key that moves nothing goes on to whoever is next.
    step(e, view, to)
}

fn ignore(_: &Engine, _: ObjectId, _: &KeyEvent) -> bool {
    false
}

/// What a view does with keys itself: the arrows move the current row.
pub(crate) fn key_hooks() -> KeyItem {
    KeyItem {
        shortcut_override: ignore,
        press,
        release: ignore,
    }
}

fn stop_movers(e: &Engine, st: &ViewState) {
    for mover in &st.movers {
        if let Some(m) = mover.get().filter(|m| e.is_alive(*m)) {
            let _ = e.invoke_by_name(m, "stop", &[]);
        }
    }
    st.mover_to.set([None; 2]);
}

fn drop_highlight(e: &Engine, st: &ViewState) {
    if let Some((_, old)) = st.highlight.take() {
        stop_movers(e, st);
        pass::retire_extra(e, old);
    }
}

/// Makes the highlight item: of `component`, or a plain item.
fn make_highlight(
    e: &Engine,
    view: ObjectId,
    content: ObjectId,
    component: Option<ObjectId>,
) -> Option<ObjectId> {
    let h = match component {
        // Below the rows, which are at 1: `z` is left as it is.
        Some(c) => pass::make_extra(e, view, content, c, &[], None)?,
        None => {
            let h = e
                .create_native_incomplete(Item::default(), Some(view))
                .ok()?;
            warn(e, "highlight", e.complete(h));
            enrol(e, h, view, Part::Extra);
            warn(e, "parent", tree::set_parent_item(e, h, Some(content)));
            h
        }
    };
    // The view goes where the highlight goes.
    for p in [Item::X.index, Item::Y.index] {
        warn(
            e,
            "highlight",
            e.watch(h, p, move |e, _| track(e, view)).map(|_| ()),
        );
    }
    Some(h)
}

/// The animation that moves the highlight along `axis`.
fn mover(e: &Engine, view: ObjectId, st: &ViewState, axis: usize) -> Option<ObjectId> {
    if let Some(m) = st.movers[axis].get().filter(|m| e.is_alive(*m)) {
        return Some(m);
    }
    let m = e
        .create_native_incomplete(SmoothedAnimation::default(), Some(view))
        .ok()?;
    warn(e, "highlight", e.complete(m));
    st.movers[axis].set(Some(m));
    Some(m)
}

/// Sends the highlight to `to` along `axis`: there at once, or on its
/// way at the speed the view says.
#[allow(clippy::too_many_arguments)]
fn send(
    e: &Engine,
    view: ObjectId,
    st: &ViewState,
    h: ObjectId,
    axis: usize,
    to: f64,
    at_once: bool,
    speed: (f64, i32),
) {
    let p = pass::pos_prop(axis);
    let mut going = st.mover_to.get();
    if going[axis] == Some(to) {
        return;
    }
    going[axis] = Some(to);
    st.mover_to.set(going);
    let (velocity, duration) = speed;
    let at_once = at_once || duration == 0 || (velocity <= 0.0 && duration < 0);
    let animation = if at_once {
        None
    } else {
        mover(e, view, st, axis)
    };
    let Some(m) = animation else {
        if let Some(m) = st.movers[axis].get().filter(|m| e.is_alive(*m)) {
            let _ = e.invoke_by_name(m, "stop", &[]);
        }
        warn(e, "highlight", write_placed(e, h, p, Value::Real(to)));
        return;
    };
    let name = if axis == X { "x" } else { "y" };
    let w = |p, v| warn(e, "highlight", e.write(m, p, v, WriteKind::Imperative));
    e.batch(|| {
        w(PropertyAnimation::TARGET.index, Value::Object(h));
        w(PropertyAnimation::PROPERTY.index, Value::string(name));
        w(PropertyAnimation::TO.index, Value::Real(to));
        w(PropertyAnimation::DURATION.index, Value::Int(duration));
        w(SmoothedAnimation::VELOCITY.index, Value::Real(velocity));
    });
    if let Err(err) = e.invoke_by_name(m, "restart", &[]) {
        warn(e, "highlight", Err(err));
    }
    // An animation that could not start leaves the highlight behind.
    let running = e.untrack(|| e.read(m, Animation::RUNNING.index).to_boolean());
    if !running && pass::rd(e, h, p) != to {
        warn(e, "highlight", write_placed(e, h, p, Value::Real(to)));
    }
}

/// A view is a focus scope whose focus item is its current item, as on
/// qml6: the item gets `focus` each time it becomes current, without
/// losing a binding the delegate has for it. No current item leaves the
/// focus where it was.
pub(crate) fn give_focus(e: &Engine, item: ObjectId) {
    warn(
        e,
        "focus",
        e.write_internal(item, Item::FOCUS.index, Value::Bool(true), WriteKind::Host),
    );
}

/// After a layout: the current item, and the highlight on its way to it.
/// `span` is where the current row is, logically, when it has an item.
pub(crate) fn sync(
    e: &Engine,
    view: ObjectId,
    st: &ViewState,
    g: &Geo,
    content: ObjectId,
    span: Option<(f64, f64)>,
) {
    let row = st.seen_index.get().and_then(|i| usize::try_from(i).ok());
    let item = span
        .and(row)
        .and_then(|row| st.delegates()?.object_at(row))
        .filter(|o| is_item(e, *o));
    let was = st.current.replace(item);
    st.current_span.set(item.and(span));
    pass::put_object(e, view, ItemView::CURRENT_ITEM, item);
    if let Some(item) = item.filter(|i| was != Some(*i)) {
        give_focus(e, item);
    }

    let component = e.get(view, ItemView::HIGHLIGHT).ok().flatten();
    let keep = item.is_some()
        && matches!(st.highlight.get(), Some((c, h)) if c == component && e.is_alive(h));
    let mut fresh = false;
    if !keep {
        drop_highlight(e, st);
        if item.is_some()
            && let Some(h) = make_highlight(e, view, content, component)
        {
            st.highlight.set(Some((component, h)));
            fresh = true;
        }
    }
    let highlight = st.highlight.get().map(|x| x.1);
    pass::put_object(e, view, ItemView::HIGHLIGHT_ITEM, highlight);

    let follows = e
        .get(view, ItemView::HIGHLIGHT_FOLLOWS_CURRENT_ITEM)
        .unwrap_or(true);
    let (Some(h), Some(item), true) = (highlight, item, follows) else {
        // A highlight the author moves, or none: the view stays.
        st.follow.set(false);
        if !follows {
            stop_movers(e, st);
        }
        return;
    };
    let duration = pass::rd(e, view, ItemView::HIGHLIGHT_MOVE_DURATION.index) as i32;
    let velocity = if g.grid {
        -1.0
    } else {
        pass::rd(e, view, ListView::HIGHLIGHT_MOVE_VELOCITY.index)
    };
    e.batch(|| {
        for p in [Item::WIDTH.index, Item::HEIGHT.index] {
            let v = Value::Real(pass::rd(e, item, p));
            warn(e, "highlight", write_placed(e, h, p, v));
        }
    });
    for axis in [X, Y] {
        let to = pass::rd(e, item, pass::pos_prop(axis));
        send(e, view, st, h, axis, to, fresh, (velocity, duration));
    }
    track(e, view);
}

/// Brings the view along with the highlight, while the view is following
/// it to a row that was made the current one.
pub(crate) fn track(e: &Engine, view: ObjectId) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    if !st.follow.get() {
        return;
    }
    let (Some((_, h)), Some(current)) = (st.highlight.get(), st.current_span.get()) else {
        return;
    };
    if !e.is_alive(h) {
        return;
    }
    e.untrack(|| {
        let g = pass::read_geo(e, view, &st);
        let dir = flickable::dir(g.main);
        // As long as the row it is going to, which in a grid is the
        // cell: the view shows the whole cell (observed on qml6).
        let size = current.1;
        let at = g.in_logical(pass::rd(e, h, pass::pos_prop(g.main)), size);
        let v0 = g.viewport(pass::rd(e, view, dir.position));
        let to = follow_target((at, size), current, v0, g.view_main);
        if to != v0 {
            let b = flickable::bounds(e, view, g.main);
            let inside = g.content_position(to).clamp(b.min.min(b.max), b.max);
            flickable::scroll_to(e, view, g.main, inside);
        }
        if (at - current.0).abs() < ARRIVED {
            // There: from now on the view is the user's again.
            st.follow.set(false);
        }
    });
}
