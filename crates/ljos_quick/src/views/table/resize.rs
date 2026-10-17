//! Sizes of a table view's columns and rows given by the program
//! (`setColumnWidth`, `setRowHeight`) or by the user dragging the end of
//! one, as qml6 does.
//!
//! An explicit size is the one the next layout takes for its column or
//! row, unless a size provider or the `syncView` gives one; a view synced
//! to another in a direction keeps its explicit sizes there, so a header
//! and the view it heads share them. With `resizableColumns`
//! (`resizableRows`) a press within [`REACH`] of the end of a column
//! (row), or in the spacing after it, is the view's before its cells'.
//! Once the pointer went past the drag threshold along it, the size
//! follows the pointer from there. Over such an end the pointer shows
//! `Qt.SplitHCursor` (a column's) or `Qt.SplitVCursor` (a row's), as on
//! qml6.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use ljos_engine::{Engine, ObjectId};
use ljos_render::kurbo;

use super::{TableState, TableView, layout, state_of};
use crate::DRAG_THRESHOLD;
use crate::flickable::Flickable;
use crate::item::{Item, is_a};

/// Columns, then rows.
pub(crate) const COLUMNS: usize = 0;
pub(crate) const ROWS: usize = 1;

/// How far from the end of a column or row a press still resizes it.
const REACH: f64 = 4.0;

/// The narrowest a drag makes a column or a row: one dragged to nothing
/// stays in the layout (qml6).
const LEAST: f64 = 0.001;

/// What a table view keeps of its explicit sizes and of a resize.
#[derive(Debug, Default)]
pub(crate) struct ResizeState {
    /// The explicit column widths, then row heights.
    explicit: [RefCell<HashMap<usize, f64>>; 2],
    /// The column, then the row, a press resizes.
    grab: Cell<[Option<Line>; 2]>,
}

/// A column or row a press resizes.
#[derive(Clone, Copy, Debug)]
struct Line {
    index: usize,
    /// Its size when pressed.
    size: f64,
    /// Where the press was, along it.
    pressed: f64,
    /// Where the drag began, once it did.
    from: Option<f64>,
}

/// `syncDirection`'s flag for each axis.
fn direction(axis: usize) -> i32 {
    if axis == COLUMNS { 1 } else { 2 }
}

/// The view that keeps `view`'s explicit sizes along `axis`: the one it
/// is synced to in that direction, as far as that goes.
fn owner(e: &Engine, view: ObjectId, axis: usize) -> ObjectId {
    let mut at = view;
    for _ in 0..64 {
        let synced = e.untrack(|| {
            let dir = e.read(at, TableView::SYNC_DIRECTION.index).to_number() as i32;
            let sync = e.read(at, TableView::SYNC_VIEW.index).as_object();
            sync.filter(|s| {
                dir & direction(axis) != 0 && *s != at && e.is_alive(*s) && is_a::<TableView>(e, *s)
            })
        });
        match synced {
            Some(s) if s != view => at = s,
            _ => break,
        }
    }
    at
}

/// Gives line `i` along `axis` the explicit size `size`; a negative
/// size takes it away.
pub(crate) fn set(e: &Engine, view: ObjectId, axis: usize, i: usize, size: f64) {
    let Some(st) = state_of(e, owner(e, view, axis)) else {
        return;
    };
    {
        let mut sizes = st.resize.explicit[axis].borrow_mut();
        if size >= 0.0 {
            sizes.insert(i, size);
        } else {
            sizes.remove(&i);
        }
    }
    layout::bump(e, &st);
}

/// The explicit size of line `i` along `axis`, -1 for none.
pub(crate) fn explicit(e: &Engine, view: ObjectId, axis: usize, i: usize) -> f64 {
    state_of(e, owner(e, view, axis))
        .and_then(|st| st.resize.explicit[axis].borrow().get(&i).copied())
        .unwrap_or(-1.0)
}

/// The explicit size `st`'s own layout takes for line `i` along `axis`.
pub(crate) fn own(st: &TableState, axis: usize, i: usize) -> Option<f64> {
    st.resize.explicit[axis].borrow().get(&i).copied()
}

/// Takes away the explicit sizes along `axis`.
pub(crate) fn clear(e: &Engine, view: ObjectId, axis: usize) {
    let Some(st) = state_of(e, owner(e, view, axis)) else {
        return;
    };
    st.resize.explicit[axis].borrow_mut().clear();
    layout::bump(e, &st);
}

/// The view `hits` holds whose column or row end is under the pointer
/// at `local(view)`, which takes the press to resize it.
pub(crate) fn catch(
    e: &Engine,
    hits: &[ObjectId],
    local: impl Fn(ObjectId) -> kurbo::Point,
) -> Option<ObjectId> {
    hits.iter()
        .copied()
        .filter(|o| is_a::<TableView>(e, *o))
        .find(|&view| press(e, view, local(view)))
}

/// A press at `at` on `view`: true when it is on the end of a column or
/// a row the user may resize.
fn press(e: &Engine, view: ObjectId, at: kurbo::Point) -> bool {
    let Some(st) = state_of(e, view) else {
        return false;
    };
    let grab = ends(e, view, &st, at);
    st.resize.grab.set(grab);
    grab.iter().any(Option::is_some)
}

/// `Qt.SplitHCursor` and `Qt.SplitVCursor`.
const SPLIT_H: i32 = 12;
const SPLIT_V: i32 = 11;

/// The cursor over the end of a column or row a view among `hits` lets
/// the user resize: a column's when it is on both.
pub(crate) fn cursor(
    e: &Engine,
    hits: &[ObjectId],
    local: impl Fn(ObjectId) -> kurbo::Point,
) -> Option<i32> {
    hits.iter()
        .copied()
        .filter(|o| is_a::<TableView>(e, *o))
        .find_map(|view| {
            let st = state_of(e, view)?;
            match ends(e, view, &st, local(view)) {
                [Some(_), _] => Some(SPLIT_H),
                [None, Some(_)] => Some(SPLIT_V),
                [None, None] => None,
            }
        })
}

/// The column and the row of `view` whose ends are at `at`, as a press
/// there would resize them.
fn ends(e: &Engine, view: ObjectId, st: &TableState, at: kurbo::Point) -> [Option<Line>; 2] {
    let read = |p| e.untrack(|| e.read(view, p));
    let (x, y) = (
        at.x + read(Flickable::CONTENT_X.index).to_number(),
        at.y + read(Flickable::CONTENT_Y.index).to_number(),
    );
    let column = read(TableView::RESIZABLE_COLUMNS.index)
        .to_boolean()
        .then(|| {
            let lines: Vec<(f64, f64)> = st.column_x.borrow().clone();
            let gap = read(TableView::COLUMN_SPACING.index).to_number().max(0.0);
            end_at(&lines, gap, x)
        })
        .flatten()
        .map(|(index, size)| Line {
            index,
            size,
            pressed: at.x,
            from: None,
        });
    let row = read(TableView::RESIZABLE_ROWS.index)
        .to_boolean()
        .then(|| {
            let ys = st.row_y.borrow();
            let heights = st.heights.borrow();
            let lines: Vec<(f64, f64)> = ys
                .iter()
                .zip(heights.iter())
                .map(|(y, h)| (*y, *h))
                .filter(|(_, h)| !h.is_nan())
                .collect();
            let gap = read(TableView::ROW_SPACING.index).to_number().max(0.0);
            end_at(&lines, gap, y)
        })
        .flatten()
        .map(|(index, size)| Line {
            index,
            size,
            pressed: at.y,
            from: None,
        });
    [column, row]
}

/// The line among `lines` (start and size each) whose end `at` is on,
/// with its size: within [`REACH`] of the end, or in the gap after it.
fn end_at(lines: &[(f64, f64)], gap: f64, at: f64) -> Option<(usize, f64)> {
    lines.iter().enumerate().find_map(|(i, &(start, size))| {
        let end = start + size;
        (size > 0.0 && at >= end - REACH && at <= end + gap + REACH).then_some((i, size))
    })
}

/// Whether `view` holds a press that resizes.
pub(crate) fn holds(st: &TableState) -> bool {
    st.resize.grab.get().iter().any(Option::is_some)
}

/// The pointer moved to `at` while `view` holds a press that resizes.
pub(crate) fn moved(e: &Engine, view: ObjectId, st: &TableState, at: kurbo::Point) {
    let mut grab = st.resize.grab.get();
    for (axis, line) in grab.iter_mut().enumerate() {
        let Some(line) = line else {
            continue;
        };
        let now = if axis == COLUMNS { at.x } else { at.y };
        if line.from.is_none() && (now - line.pressed).abs() > DRAG_THRESHOLD {
            line.from = Some(now);
        }
        if let Some(from) = line.from {
            set(
                e,
                view,
                axis,
                line.index,
                (line.size + now - from).max(LEAST),
            );
        }
    }
    st.resize.grab.set(grab);
}

/// The press that resized ended.
pub(crate) fn release(st: &TableState) {
    st.resize.grab.set([None; 2]);
}

/// The implicit size of line `i` along `axis`: the largest of its
/// delegates' while it has any, -1 when it has none.
pub(crate) fn implicit(e: &Engine, view: ObjectId, axis: usize, i: usize) -> f64 {
    let Some(st) = state_of(e, view) else {
        return -1.0;
    };
    let (Some(d), cols) = (st.delegates(), st.cells.columns()) else {
        return -1.0;
    };
    if cols == 0 {
        return -1.0;
    }
    let p = if axis == COLUMNS {
        Item::IMPLICIT_WIDTH.index
    } else {
        Item::IMPLICIT_HEIGHT.index
    };
    d.live()
        .into_iter()
        .filter(|(cell, o)| {
            let line = if axis == COLUMNS {
                cell % cols
            } else {
                cell / cols
            };
            line == i && !st.hidden.borrow().contains_key(o)
        })
        .map(|(_, o)| layout::implicit(e, o, p))
        .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))))
        .unwrap_or(-1.0)
}
