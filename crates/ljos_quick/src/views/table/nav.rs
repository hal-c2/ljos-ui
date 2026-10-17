//! Moving the current cell of a table view and selecting with the keys
//! and the pointer, through the view's `selectionModel`, as qml6 does.
//!
//! Up and Down move the current row, Page Up and Page Down by the rows in
//! sight less one, Home and End to the first and last column (Control
//! changes nothing), and Tab and Backtab to the next and the previous
//! cell, from the end of a row to the next and from the last cell to the
//! first; with nothing current they start at the first row in sight. In
//! a `TreeView`, Right expands the current row and Left collapses it, and
//! neither moves; in a table they move the current column. With Shift
//! the selection is the cells from where the Shift moves began to the
//! current one (only the first with `SingleSelection`); a move without
//! Shift, or a tap, takes the selection away. Backtab never selects:
//! Shift is how it is typed. A tap makes its cell
//! current; a double tap expands or collapses a tree's row. Keys and taps
//! open editors as the view's `editTriggers` say ([`edit`]).

use ljos_engine::{Engine, ItemSelectionModel, ObjectId, Value};
use ljos_render::kurbo;
use ljos_runtime::Time;

use super::{
    TableState, TableView, edit, edit_trigger, index_of_cell, layout, selection_behavior,
    selection_mode, state_of,
};
use crate::flickable::{Flickable, drag};
use crate::input::{KeyEvent, Modifiers, MouseButtons, PointerEvent};
use crate::item::Item;
use crate::keys::{KeyItem, key};
use crate::pointer_item::PointerItem;
use crate::{DOUBLE_CLICK_INTERVAL, DRAG_THRESHOLD};

/// `ItemSelectionModel`'s flags a view selects with.
const CLEAR_AND_SELECT: i32 = 0x3;
const ROWS: i32 = 0x20;
const COLUMNS: i32 = 0x40;

/// What the view does with keys itself.
pub(crate) fn key_hooks() -> KeyItem {
    KeyItem {
        shortcut_override: |_, _, _| false,
        press,
        release: |_, _, _| false,
    }
}

/// The Flickable's handling of the pointer, with taps on cells besides.
pub(crate) fn pointer_hooks() -> PointerItem {
    let flick = drag::hooks();
    PointerItem {
        press: pointer_press,
        moved: flick.moved,
        release: pointer_release,
        cancel: |e, view| {
            if let Some(st) = state_of(e, view) {
                st.press.set(None);
            }
            (drag::hooks().cancel)(e, view);
        },
        hover: flick.hover,
    }
}

fn flag(e: &Engine, view: ObjectId, p: ljos_engine::PropIndex) -> bool {
    e.untrack(|| e.read(view, p).to_boolean())
}

fn int(e: &Engine, view: ObjectId, p: ljos_engine::PropIndex) -> i32 {
    e.untrack(|| e.read(view, p).to_number() as i32)
}

fn real(e: &Engine, view: ObjectId, p: ljos_engine::PropIndex) -> f64 {
    e.untrack(|| e.read(view, p).to_number())
}

fn selection_model(e: &Engine, view: ObjectId) -> Option<ObjectId> {
    e.untrack(|| e.read(view, TableView::SELECTION_MODEL.index).as_object())
}

fn press(e: &Engine, view: ObjectId, ev: &KeyEvent) -> bool {
    let (Some(st), Some(_)) = (state_of(e, view), selection_model(e, view)) else {
        return false;
    };
    if let Some(current) = e.untrack(|| layout::current_cell(e, view, &st))
        && edit::key_opens(e, view, &st, current, ev)
    {
        return true;
    }
    if !flag(e, view, TableView::KEY_NAVIGATION_ENABLED.index) {
        return false;
    }
    let rows = st.cells.line_count();
    if rows == 0 {
        return false;
    }
    let current = e.untrack(|| layout::current_cell(e, view, &st));
    let last = rows - 1;
    let page = rows_in_sight(e, view, &st).saturating_sub(1).max(1);
    let last_column = st.cells.columns().saturating_sub(1);
    let to = match (ev.key, current) {
        (key::END, None) => (first_in_sight(e, view, &st), last_column),
        (
            key::UP
            | key::DOWN
            | key::PAGE_UP
            | key::PAGE_DOWN
            | key::HOME
            | key::TAB
            | key::BACKTAB,
            None,
        ) => (first_in_sight(e, view, &st), 0),
        (key::HOME, Some((r, _))) => (r, 0),
        (key::END, Some((r, _))) => (r, last_column),
        (key::TAB | key::BACKTAB, Some(cell)) => next_cell(&st, cell, ev.key == key::TAB),
        (key::UP, Some((r, c))) => (r.saturating_sub(1), c),
        (key::DOWN, Some((r, c))) => ((r + 1).min(last), c),
        (key::PAGE_UP, Some((r, c))) => (r.saturating_sub(page), c),
        (key::PAGE_DOWN, Some((r, c))) => ((r + page).min(last), c),
        (key::LEFT | key::RIGHT, None) => return false,
        (key::LEFT | key::RIGHT, Some((r, _))) if st.tree.get() => {
            let expanded = st
                .cells
                .line(r)
                .is_some_and(|l| st.cells.is_expanded(&l.path));
            let has_children = st.cells.line(r).is_some_and(|l| l.has_children);
            let method = match ev.key {
                key::RIGHT if has_children && !expanded => "expand",
                key::LEFT if expanded => "collapse",
                _ => return true,
            };
            let r = e.invoke_by_name(view, method, &[Value::Int(r as i32)]);
            super::warn(e, method, r.map(drop));
            return true;
        }
        (key::LEFT, Some((r, c))) => (r, c.saturating_sub(1)),
        (key::RIGHT, Some((r, c))) => (r, (c + 1).min(st.cells.columns().saturating_sub(1))),
        _ => return false,
    };
    let shift = ev.modifiers.contains(Modifiers::SHIFT) && ev.key != key::BACKTAB;
    move_to(e, view, &st, current, to, shift);
    reveal(e, view, &st, to.0);
    true
}

/// Makes `to` the current cell; with `shift`, the selection reaches from
/// where the Shift moves began to it, and without, it goes.
fn move_to(
    e: &Engine,
    view: ObjectId,
    st: &TableState,
    from: Option<(usize, usize)>,
    to: (usize, usize),
    shift: bool,
) {
    let Some(sm) = selection_model(e, view) else {
        return;
    };
    let behaviour = int(e, view, TableView::SELECTION_BEHAVIOR.index);
    if shift && from.is_some() && behaviour != selection_behavior::SELECTION_DISABLED {
        let anchor = st.anchor.get().or(from).unwrap_or(to);
        st.anchor.set(Some(anchor));
        let end = if int(e, view, TableView::SELECTION_MODE.index) == selection_mode::SINGLE {
            anchor
        } else {
            to
        };
        let command = CLEAR_AND_SELECT
            | match behaviour {
                selection_behavior::SELECT_ROWS => ROWS,
                selection_behavior::SELECT_COLUMNS => COLUMNS,
                _ => 0,
            };
        let ranges = block(e, st, anchor, end);
        let r = e.invoke_by_name(sm, "select", &[ranges, Value::Int(command)]);
        super::warn(e, "select", r.map(drop));
    } else {
        forget_selection(e, st, sm);
    }
    let index = index_of_cell(e, st, to.0, to.1);
    let r = e.invoke_by_name(sm, "setCurrentIndex", &[index, Value::Int(0)]);
    super::warn(e, "setCurrentIndex", r.map(drop));
}

/// The cell after `cell` (`forward`) or before it, from the end of a row
/// to the next and from the last cell to the first.
pub(crate) fn next_cell(st: &TableState, (r, c): (usize, usize), forward: bool) -> (usize, usize) {
    let last = st.cells.line_count().saturating_sub(1);
    let last_column = st.cells.columns().saturating_sub(1);
    match forward {
        true if c < last_column => (r, c + 1),
        true if r < last => (r + 1, 0),
        true => (0, 0),
        false if c > 0 => (r, c - 1),
        false if r > 0 => (r - 1, last_column),
        false => (last, last_column),
    }
}

/// Makes `cell` current, keeping the selection.
pub(crate) fn make_current(e: &Engine, view: ObjectId, st: &TableState, cell: (usize, usize)) {
    let Some(sm) = selection_model(e, view) else {
        return;
    };
    let index = index_of_cell(e, st, cell.0, cell.1);
    let r = e.invoke_by_name(sm, "setCurrentIndex", &[index, Value::Int(0)]);
    super::warn(e, "setCurrentIndex", r.map(drop));
}

/// Takes the selection away.
pub(crate) fn forget(e: &Engine, view: ObjectId, st: &TableState) {
    if let Some(sm) = selection_model(e, view) {
        forget_selection(e, st, sm);
    }
}

fn forget_selection(e: &Engine, st: &TableState, sm: ObjectId) {
    st.anchor.set(None);
    let selected = e.untrack(|| {
        e.read(sm, ItemSelectionModel::HAS_SELECTION.index)
            .to_boolean()
    });
    if selected {
        let r = e.invoke_by_name(sm, "clearSelection", &[]);
        super::warn(e, "clearSelection", r.map(drop));
    }
}

/// The cells from `a` to `b` as ranges: one per run of rows under the
/// same parent, since a range does not reach across parents.
fn block(e: &Engine, st: &TableState, a: (usize, usize), b: (usize, usize)) -> Value {
    let (top, bottom) = (a.0.min(b.0), a.0.max(b.0));
    let (left, right) = (a.1.min(b.1), a.1.max(b.1));
    let parent = |r: usize| {
        st.cells
            .line(r)
            .map(|l| l.path[..l.path.len().saturating_sub(1)].to_vec())
    };
    let mut ranges = Vec::new();
    let mut start = top;
    for r in top..=bottom {
        let next = r + 1;
        let breaks = next > bottom || parent(next) != parent(r) || {
            let row = |r: usize| st.cells.line(r).and_then(|l| l.path.last().copied());
            row(next) != row(r).map(|x| x + 1)
        };
        if breaks {
            ranges.push(Value::map([
                ("topLeft", index_of_cell(e, st, start, left)),
                ("bottomRight", index_of_cell(e, st, r, right)),
            ]));
            start = next;
        }
    }
    Value::list(ranges)
}

/// The rows wholly in sight.
fn rows_in_sight(e: &Engine, view: ObjectId, st: &TableState) -> usize {
    let (top, height) = (
        real(e, view, Flickable::CONTENT_Y.index),
        real(e, view, Item::HEIGHT.index),
    );
    let heights = st.heights.borrow();
    let ys = st.row_y.borrow();
    ys.iter()
        .zip(heights.iter())
        .filter(|(y, h)| **y >= top - 0.5 && **y + **h <= top + height + 0.5)
        .count()
}

fn first_in_sight(e: &Engine, view: ObjectId, st: &TableState) -> usize {
    let top = real(e, view, Flickable::CONTENT_Y.index);
    st.row_y
        .borrow()
        .iter()
        .position(|y| *y >= top - 0.5)
        .unwrap_or(0)
}

/// Moves the content as little as it takes to show row `row` whole.
fn reveal(e: &Engine, view: ObjectId, st: &TableState, row: usize) {
    let (Some(top), Some(h)) = (
        st.row_y.borrow().get(row).copied(),
        st.heights.borrow().get(row).copied(),
    ) else {
        return;
    };
    let y = real(e, view, Flickable::CONTENT_Y.index);
    let height = real(e, view, Item::HEIGHT.index);
    let to = if top < y {
        top
    } else if top + h > y + height {
        top + h - height
    } else {
        return;
    };
    layout::show_y(e, view, to);
}

fn pointer_press(e: &Engine, view: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    let takes = (drag::hooks().press)(e, view, ev, at);
    let left = ev.button == Some(MouseButtons::LEFT);
    if let Some(st) = state_of(e, view)
        && left
    {
        let content = (
            real(e, view, Flickable::CONTENT_X.index),
            real(e, view, Flickable::CONTENT_Y.index),
        );
        st.press.set(Some((at, content)));
    }
    takes || left
}

fn pointer_release(e: &Engine, view: ObjectId, ev: &PointerEvent, at: kurbo::Point) {
    (drag::hooks().release)(e, view, ev, at);
    let Some(st) = state_of(e, view) else {
        return;
    };
    let Some((pressed, content)) = st.press.take() else {
        return;
    };
    let now = (
        real(e, view, Flickable::CONTENT_X.index),
        real(e, view, Flickable::CONTENT_Y.index),
    );
    // A press that moved the content, or went far, was a drag.
    if now != content || (at - pressed).hypot() > DRAG_THRESHOLD {
        return;
    }
    let (x, y) = (at.x + now.0, at.y + now.1);
    if edit::on_editor(e, &st, x, y) {
        return;
    }
    // A tap anywhere else commits the open editor, a tap on no cell too.
    edit::close(e, view, &st, true);
    let Some(cell) = layout::cell_at(&st, x, y) else {
        return;
    };
    tap(e, view, &st, cell, ev.time);
}

/// A tap on cell `cell` at `time`: the second on it soon after the first
/// is a double tap.
fn tap(e: &Engine, view: ObjectId, st: &TableState, cell: (usize, usize), time: Time) {
    let double = st
        .last_tap
        .take()
        .is_some_and(|(t, c)| c == cell && time.saturating_since(t) <= DOUBLE_CLICK_INTERVAL);
    if !double {
        st.last_tap.set(Some((time, cell)));
    }
    tapped_cell(e, view, st, cell, double);
}

/// A tap, or the second of a double tap, on `cell`, and the editor it
/// opens as the edit triggers say.
pub(crate) fn tapped_cell(
    e: &Engine,
    view: ObjectId,
    st: &TableState,
    cell: (usize, usize),
    double: bool,
) {
    edit::close(e, view, st, true);
    // Whether it was selected is asked before the tap takes the
    // selection away.
    let was_selected = !double
        && selection_model(e, view).is_some_and(|sm| {
            let index = index_of_cell(e, st, cell.0, cell.1);
            e.invoke_by_name(sm, "isSelected", &[index])
                .is_ok_and(|v| v.to_boolean())
        });
    if double {
        double_tapped(e, view, cell.0);
    } else {
        tapped(e, view, st, cell);
    }
    edit::tap_opens(e, view, st, cell, double, was_selected);
}

/// A tap makes its cell current and takes the selection away.
fn tapped(e: &Engine, view: ObjectId, st: &TableState, cell: (usize, usize)) {
    if !flag(e, view, TableView::POINTER_NAVIGATION_ENABLED.index) {
        return;
    }
    let Some(sm) = selection_model(e, view) else {
        return;
    };
    forget_selection(e, st, sm);
    let index = index_of_cell(e, st, cell.0, cell.1);
    let r = e.invoke_by_name(sm, "setCurrentIndex", &[index, Value::Int(0)]);
    super::warn(e, "setCurrentIndex", r.map(drop));
}

/// A double tap expands a tree's row, or collapses it, unless a double
/// tap edits.
fn double_tapped(e: &Engine, view: ObjectId, row: usize) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let edits = int(e, view, TableView::EDIT_TRIGGERS.index) & edit_trigger::DOUBLE_TAPPED != 0;
    if !st.tree.get() || edits {
        return;
    }
    let r = e.invoke_by_name(view, "toggleExpanded", &[Value::Int(row as i32)]);
    super::warn(e, "toggleExpanded", r.map(drop));
}
