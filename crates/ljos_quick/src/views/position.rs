//! Bringing a view to a row, and asking which row is at a point.
//!
//! A row the view never laid out has no known place. The view then lays
//! rows out around where rows of the average length would put it, which
//! measures the row, and goes there: the row is where the method said,
//! and the estimate only shows in `contentY` and the length of the
//! content, as it does in Qt.

use ljos_engine::{Engine, ObjectId, Value};

use super::state::{Geo, position_mode, position_target, snap_target};
use super::{GridView, ListView, pass, put, state_of};
use crate::flickable::{self, Y};
use crate::item::Item;

pub(crate) fn at_index(e: &Engine, view: ObjectId, index: i32, mode: i32) {
    go(e, view, index, mode, None);
}

/// `positionViewAtEnd` and `positionViewAtBeginning`: past the last row
/// and before the first, so that a footer or a header shows.
pub(crate) fn at_end(e: &Engine, view: ObjectId, end: bool) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let rows = st.delegates().map_or(0, |d| d.count());
    if rows > 0 {
        let (row, mode) = if end {
            (rows - 1, position_mode::END)
        } else {
            (0, position_mode::BEGINNING)
        };
        go(e, view, row as i32, mode, Some(end));
        return;
    }
    e.untrack(|| {
        let g = pass::read_geo(e, view, &st);
        let b = flickable::bounds(e, view, g.main);
        st.positioned.set(true);
        flickable::scroll_to(e, view, g.main, edge_of(&g, (b.min, b.max), end));
    });
    pass::refresh(e, view);
}

/// The end of the content, or its beginning: rows that run against the
/// coordinate begin at its far end.
fn edge_of(g: &Geo, (min, max): (f64, f64), end: bool) -> f64 {
    if end != g.reversed { max } else { min }
}

/// Brings row `index` where `mode` says, or with `edge` the end or the
/// beginning of the content. Where that is is worked out first: the rows
/// are laid out there, and the view is given its place and the length of
/// the content those rows make in one go, so that it moves once and makes
/// delegates only for the rows it shows and the one it measured.
fn go(e: &Engine, view: ObjectId, index: i32, mode: i32, edge: Option<bool>) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let Some(d) = st.delegates() else {
        return;
    };
    let Some(row) = usize::try_from(index).ok().filter(|r| *r < d.count()) else {
        return;
    };
    let mut moved = st.passes.get();
    // Whether the rows were laid out where the view already was, as the
    // layout effect last laid them out.
    let mut stayed = false;
    e.untrack(|| {
        let g = pass::read_geo(e, view, &st);
        let dir = flickable::dir(g.main);
        let line = g.line_of(row);
        let p0 = pass::rd(e, view, dir.position);
        let v0 = g.viewport(p0);
        let known = |measured: bool| {
            let slots = st.slots.borrow();
            let found = slots.iter().find(|s| s.line == line)?;
            (found.measured || !measured).then_some((found.pos, found.size))
        };
        let mut guessed = false;
        let span = if g.grid {
            Some((line as f64 * g.cell_main, g.cell_main))
        } else {
            known(true)
        };
        let span = span.or_else(|| {
            guessed = true;
            // The first row starts the content, wherever the view was;
            // another is where rows of the average length put it.
            let guess = if line == 0 {
                0.0
            } else {
                st.place_of(line, g.spacing)
            };
            st.anchor.set((line, guess));
            let around = position_target(mode, guess, st.avg.get(), v0, g.view_main);
            pass::pass(e, view, &st, Some(around.unwrap_or(guess)));
            known(false)
        });
        let Some((pos, size)) = span else {
            return;
        };
        let to = position_target(mode, pos, size, v0, g.view_main);
        let Some(to) = to.or(edge.map(|_| v0)) else {
            return;
        };
        // Where the view goes if the content keeps its length: the rows
        // are laid out for what it will show, not for a place past the
        // ends of the content it cannot go to.
        let place = |b: &flickable::AxisState| match edge {
            Some(end) => edge_of(&g, (b.min, b.max), end),
            None => g.content_position(to).clamp(b.min.min(b.max), b.max),
        };
        let b0 = flickable::bounds(e, view, g.main);
        let lay_at = g.viewport(place(&b0));
        st.positioned.set(true);
        // Where it was told to go is where it stays.
        st.follow.set(false);
        st.laid.set(None);
        pass::pass(e, view, &st, Some(lay_at));
        moved = st.passes.get();
        let laid = st.laid.take();
        e.batch(|| {
            if let Some((origin, length)) = laid {
                put(e, view, dir.origin, Value::Real(origin));
                put(e, view, dir.content, Value::Real(length));
            }
            let b = flickable::bounds(e, view, g.main);
            let at = place(&b);
            stayed = !guessed && at == p0 && g.viewport(at) == lay_at;
            st.bounds.set((b.min, b.max, 0.0));
            flickable::scroll_to(e, view, g.main, at);
        });
    });
    // What is shown now has its delegates when this returns. The layout
    // effect has laid them out again if the view moved. If it did not,
    // it is run for the delegates the pass made, unless the pass laid
    // out the same ones where the effect last did: it would do the same
    // again.
    let Some(st) = state_of(e, view) else {
        return;
    };
    if st.dirty.replace(false) {
        pass::refresh(e, view);
        return;
    }
    if st.passes.get() != moved {
        return;
    }
    let same = stayed
        && st.pending.borrow().is_none()
        && *st.sized.borrow() == pass::sized_items(&st, st.grid.get());
    if !same {
        pass::refresh(e, view);
    }
}

/// The row at a point of the content, if one is shown there.
pub(crate) fn index_at(e: &Engine, view: ObjectId, x: f64, y: f64) -> Option<usize> {
    let st = state_of(e, view)?;
    let rows = st.delegates()?.count();
    e.untrack(|| {
        let g = pass::read_geo(e, view, &st);
        if !g.grid {
            // By what the delegates cover: not a heading, not the space
            // between two rows (observed on qml6).
            let placed = st.placed.borrow().clone();
            return placed.into_iter().find_map(|(row, o, ..)| {
                if !e.is_alive(o) {
                    return None;
                }
                let n = |p| pass::rd(e, o, p);
                let (ix, iy) = (n(Item::X.index), n(Item::Y.index));
                let inside = x >= ix
                    && x < ix + n(Item::WIDTH.index)
                    && y >= iy
                    && y < iy + n(Item::HEIGHT.index);
                inside.then_some(row)
            });
        }
        // A grid by its cells, whatever the delegates cover.
        if g.cell_main <= 0.0 || g.cell_cross <= 0.0 {
            return None;
        }
        let (along, across) = if g.main == Y { (y, x) } else { (x, y) };
        let along = if g.reversed { -along } else { along };
        let line = if g.reversed {
            // The first line ends at 0, which is not in it.
            (along / g.cell_main).ceil() - 1.0
        } else {
            (along / g.cell_main).floor()
        };
        let cell = (across / g.cell_cross).floor();
        if line < 0.0 || cell < 0.0 || cell >= g.per_line as f64 {
            return None;
        }
        let cell = cell as usize;
        let column = if g.cross_reversed {
            g.per_line - 1 - cell
        } else {
            cell
        };
        let row = (line as usize)
            .checked_mul(g.per_line)?
            .checked_add(column)?;
        (row < rows).then_some(row)
    })
}

/// `snapMode`: a view that came to rest starts where a row starts.
pub(crate) fn snap(e: &Engine, view: ObjectId) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    e.untrack(|| {
        let mode = if st.grid.get() {
            GridView::SNAP_MODE.index
        } else {
            ListView::SNAP_MODE.index
        };
        if pass::rd(e, view, mode) as i32 == 0 {
            return;
        }
        let g = pass::read_geo(e, view, &st);
        let b = flickable::bounds(e, view, g.main);
        let v0 = g.viewport(b.position);
        let to = snap_target(&st.slots.borrow(), v0);
        let Some(to) = to else {
            return;
        };
        let inside = g.content_position(to).clamp(b.min.min(b.max), b.max);
        if inside != b.position {
            flickable::scroll_to(e, view, g.main, inside);
        }
    });
}
