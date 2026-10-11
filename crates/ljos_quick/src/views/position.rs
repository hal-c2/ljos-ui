//! Bringing a view to a row, and asking which row is at a point.
//!
//! A row the view never laid out has no known place. The view then lays
//! rows out around where rows of the average length would put it, which
//! measures the row, and goes there: the row is where the method said,
//! and the estimate only shows in `contentY` and the length of the
//! content, as it does in Qt.

use ljos_engine::{Engine, ObjectId};

use super::state::{position_mode, position_target, snap_target};
use super::{GridView, ListView, pass, state_of};
use crate::flickable::{self, Y};
use crate::item::Item;

/// How often `positionViewAtEnd` looks again: the end of a list whose
/// rows were never measured moves as they are.
const END_ROUNDS: usize = 3;

pub(crate) fn at_index(e: &Engine, view: ObjectId, index: i32, mode: i32) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let Some(d) = st.delegates() else {
        return;
    };
    let Some(row) = usize::try_from(index).ok().filter(|r| *r < d.count()) else {
        return;
    };
    e.untrack(|| {
        let g = pass::read_geo(e, view, &st);
        let dir = flickable::dir(g.main);
        let line = g.line_of(row);
        let known = |measured: bool| {
            let slots = st.slots.borrow();
            let found = slots.iter().find(|s| s.line == line)?;
            (found.measured || !measured).then_some((found.pos, found.size))
        };
        let span = if g.grid {
            Some((line as f64 * g.cell_main, g.cell_main))
        } else {
            known(true)
        };
        let span = span.or_else(|| {
            let guess = st.place_of(line, g.spacing);
            st.anchor.set((line, guess));
            pass::pass(e, view, &st, Some(guess));
            known(false)
        });
        let Some((pos, size)) = span else {
            return;
        };
        let v0 = g.viewport(pass::rd(e, view, dir.position));
        let Some(to) = position_target(mode, pos, size, v0, g.view_main) else {
            return;
        };
        st.positioned.set(true);
        // Where it was told to go is where it stays.
        st.follow.set(false);
        let b = flickable::bounds(e, view, g.main);
        let inside = g.content_position(to).clamp(b.min.min(b.max), b.max);
        flickable::scroll_to(e, view, g.main, inside);
    });
    // Whatever was laid out to measure the row, what is shown now has
    // its delegates when this returns.
    pass::refresh(e, view);
}

/// `positionViewAtEnd` and `positionViewAtBeginning`: past the last row
/// and before the first, so that a footer or a header shows.
pub(crate) fn at_end(e: &Engine, view: ObjectId, end: bool) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let rows = st.delegates().map_or(0, |d| d.count());
    if rows > 0 {
        if end {
            at_index(e, view, rows as i32 - 1, position_mode::END);
        } else {
            at_index(e, view, 0, position_mode::BEGINNING);
        }
    }
    for _ in 0..END_ROUNDS {
        let moved = e.untrack(|| {
            let g = pass::read_geo(e, view, &st);
            let b = flickable::bounds(e, view, g.main);
            // Rows that run against the coordinate begin at its far end.
            let bound = if end != g.reversed { b.max } else { b.min };
            if b.position == bound {
                return false;
            }
            st.positioned.set(true);
            flickable::scroll_to(e, view, g.main, bound);
            true
        });
        if !moved {
            break;
        }
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
