//! The layout pass of a table view: which rows have delegates, how tall
//! each row and how wide each column is, and where each cell goes.
//!
//! The rows are walked from the top. A row in sight gets a delegate for
//! each of its cells in sight and is as tall as its tallest (or as the
//! `rowHeightProvider` says); a row out of sight that was never measured
//! counts as tall as the average of those that were, so `contentHeight`
//! of a long table is a guess that gets better as it moves. Across, only
//! the columns in sight get delegates, and the same goes for
//! `contentWidth`. A column is as wide as its widest delegate in sight,
//! or as the `columnWidthProvider` says. Delegates of cells that leave go
//! to the pool with `reuseItems`, hidden there.

use std::collections::HashSet;

use ljos_engine::{
    Engine, ItemSelectionModel, Made, ObjectId, OverrideContent, PropIndex, RestoreMode, Urgency,
    Value, WriteKind,
};

use super::{TableState, TableView, state_of};
use crate::flickable::Flickable;
use crate::item::{Item, write_computed, write_placed};
use crate::views::{put, warn};

/// Rows that get delegates in one pass at most: rows that are 0 tall
/// would never fill the view.
const MAX_LOADED_ROWS: usize = 1024;

/// Makes the layout effect run again.
pub(crate) fn bump(e: &Engine, st: &TableState) {
    if let Some(n) = st.trigger.get() {
        let g = e.graph();
        let now = g.get_untracked(n).to_number() as i32;
        g.set(n, Value::Int(now.wrapping_add(1)));
    }
}

/// Lays the cells out now, and again when the effect next runs.
pub(crate) fn refresh(e: &Engine, view: ObjectId) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    e.untrack(|| pass(e, view, &st));
    bump(e, &st);
}

fn rd(e: &Engine, obj: ObjectId, p: PropIndex) -> f64 {
    e.read(obj, p).to_number()
}

/// Reads, tracked, what a layout depends on besides the delegates.
pub(crate) fn read_inputs(e: &Engine, view: ObjectId, st: &TableState) {
    for p in [
        Item::WIDTH.index,
        Item::HEIGHT.index,
        Flickable::CONTENT_X.index,
        Flickable::CONTENT_Y.index,
        TableView::ROW_SPACING.index,
        TableView::COLUMN_SPACING.index,
        TableView::ROW_HEIGHT_PROVIDER.index,
        TableView::COLUMN_WIDTH_PROVIDER.index,
        TableView::REUSE_ITEMS.index,
    ] {
        let _ = e.read(view, p);
    }
    // Which cell is current is told to the view, and with which are
    // selected, to the delegates of a tree.
    if let Some(sm) = e.read(view, TableView::SELECTION_MODEL.index).as_object() {
        let _ = e.read(sm, ItemSelectionModel::CURRENT_INDEX.index);
        let _ = e.read(sm, ItemSelectionModel::SELECTED_INDEXES.index);
    }
    if let Some(n) = st.trigger.get() {
        let _ = e.graph().get(n);
    }
}

/// Reads, tracked, the implicit sizes the last pass laid the cells out
/// by: a cell that grows moves the cells after it.
pub(crate) fn read_sizes(e: &Engine, st: &TableState) {
    let Some(d) = st.delegates() else {
        return;
    };
    for (_, o) in d.live() {
        if e.is_alive(o) && !st.hidden.borrow().contains_key(&o) {
            let _ = e.read(o, Item::IMPLICIT_WIDTH.index);
            let _ = e.read(o, Item::IMPLICIT_HEIGHT.index);
        }
    }
}

/// What a size provider gives for `i`; `None` for no provider, or for a
/// size it leaves to the delegates (a negative one, or none).
fn provided(e: &Engine, view: ObjectId, provider: &Value, i: usize) -> Option<f64> {
    if provider.is_nullish() {
        return None;
    }
    let v = e.call_value(view, provider, &[Value::Int(i as i32)])?;
    let n = v.as_number()?;
    (n.is_finite() && n >= 0.0).then_some(n)
}

/// The cells' places: where each row starts and each column, and how
/// tall and wide.
pub(crate) fn pass(e: &Engine, view: ObjectId, st: &TableState) {
    let Some(d) = st.delegates() else {
        return;
    };
    let rows = st.cells.line_count();
    let cols = st.cells.columns();
    let n = |p: PropIndex| rd(e, view, p);
    let (w, h) = (n(Item::WIDTH.index), n(Item::HEIGHT.index));
    let (cx, cy) = (n(Flickable::CONTENT_X.index), n(Flickable::CONTENT_Y.index));
    let (row_gap, col_gap) = (
        n(TableView::ROW_SPACING.index).max(0.0),
        n(TableView::COLUMN_SPACING.index).max(0.0),
    );
    let row_provider = e.read(view, TableView::ROW_HEIGHT_PROVIDER.index);
    let col_provider = e.read(view, TableView::COLUMN_WIDTH_PROVIDER.index);
    let reuse = e.read(view, TableView::REUSE_ITEMS.index).to_boolean();

    let mut heights = std::mem::take(&mut *st.heights.borrow_mut());
    heights.resize(rows, f64::NAN);
    let known: Vec<f64> = heights.iter().copied().filter(|x| !x.is_nan()).collect();
    let mut average = if known.is_empty() {
        0.0
    } else {
        known.iter().sum::<f64>() / known.len() as f64
    };

    let mut widths = std::mem::take(&mut *st.widths.borrow_mut());
    widths.resize(cols, f64::NAN);
    let given: Vec<Option<f64>> = (0..cols)
        .map(|c| provided(e, view, &col_provider, c))
        .collect();
    let sight = (cx, cx + w.max(0.0));

    // Rows in sight, top down, each with its cells in sight; which
    // columns those are is found on the first of them.
    let mut shown: Option<Vec<usize>> = None;
    let mut wanted: HashSet<usize> = HashSet::new();
    let mut loaded_rows: Vec<usize> = Vec::new();
    let mut row_y = Vec::with_capacity(rows);
    let mut y = 0.0;
    for row in 0..rows {
        row_y.push(y);
        let mut tall = heights[row];
        let guess = if tall.is_nan() { average } else { tall };
        let in_sight = cols > 0
            && y + guess >= cy
            && y <= cy + h.max(0.0)
            && loaded_rows.len() < MAX_LOADED_ROWS;
        if in_sight {
            if shown.is_none() {
                let found = columns_in_sight(e, st, &d, row, &given, &mut widths, sight, col_gap);
                shown = Some(found);
            }
            let mut tallest: Option<f64> = None;
            for &c in shown.as_deref().unwrap_or_default() {
                let i = row * cols + c;
                wanted.insert(i);
                if let Some(o) = load(e, st, &d, i) {
                    let t = rd(e, o, Item::IMPLICIT_HEIGHT.index);
                    tallest = Some(tallest.map_or(t, |m: f64| m.max(t)));
                }
            }
            // A row with no column in sight is not measured.
            if let Some(t) = provided(e, view, &row_provider, row).or(tallest) {
                tall = t;
                heights[row] = tall;
            }
            loaded_rows.push(row);
            let measured = heights.iter().filter(|x| !x.is_nan());
            let count = measured.clone().count();
            average = measured.sum::<f64>() / count.max(1) as f64;
        }
        let tall = if tall.is_nan() { average } else { tall };
        y += tall + row_gap;
    }
    let content_h = if rows > 0 { y - row_gap } else { 0.0 };
    let shown = shown.unwrap_or_default();

    // Cells that left: their delegates go.
    for (i, o) in d.live() {
        if !wanted.contains(&i) {
            release(e, st, &d, o, reuse);
        }
    }

    // Columns: as wide as the provider says or the widest delegate in
    // sight; one never measured, as the average of those that were.
    for &c in &shown {
        let widest = loaded_rows
            .iter()
            .filter_map(|row| d.object_at(row * cols + c))
            .map(|o| rd(e, o, Item::IMPLICIT_WIDTH.index))
            .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))));
        if let Some(widest) = widest {
            widths[c] = widest;
        }
    }
    let typical = mean(&widths).unwrap_or(0.0);
    let mut column_x = Vec::with_capacity(cols);
    let mut x = 0.0;
    for c in 0..cols {
        let known = (!widths[c].is_nan()).then_some(widths[c]);
        let wide = given[c].or(known).unwrap_or(typical);
        column_x.push((x, wide));
        x += wide + col_gap;
    }
    let content_w = if cols > 0 { x - col_gap } else { 0.0 };

    for &row in &loaded_rows {
        for &c in &shown {
            let (x, wide) = column_x[c];
            let i = row * cols + c;
            let Some(o) = d.object_at(i) else {
                continue;
            };
            for (p, v) in [
                (Item::X.index, x),
                (Item::Y.index, row_y[row]),
                (Item::WIDTH.index, wide),
                (Item::HEIGHT.index, heights[row]),
            ] {
                warn(e, "place", write_placed(e, o, p, Value::Real(v)));
            }
            decorate(e, view, st, i, o, false);
        }
    }

    *st.heights.borrow_mut() = heights;
    *st.widths.borrow_mut() = widths;
    *st.row_y.borrow_mut() = row_y;

    put(
        e,
        view,
        Flickable::CONTENT_WIDTH.index,
        Value::Real(content_w),
    );
    put(
        e,
        view,
        Flickable::CONTENT_HEIGHT.index,
        Value::Real(content_h),
    );
    put(e, view, TableView::ROWS.index, Value::Int(rows as i32));
    put(e, view, TableView::COLUMNS.index, Value::Int(cols as i32));
    let first = |v: Option<&usize>| v.map_or(-1, |r| *r as i32);
    let left = shown.first();
    let right = shown.last();
    // The rows loaded are the rows in sight.
    let (top, bottom) = (first(loaded_rows.first()), first(loaded_rows.last()));
    put(e, view, TableView::TOP_ROW.index, Value::Int(top));
    put(e, view, TableView::BOTTOM_ROW.index, Value::Int(bottom));
    put(
        e,
        view,
        TableView::LEFT_COLUMN.index,
        Value::Int(first(left)),
    );
    put(
        e,
        view,
        TableView::RIGHT_COLUMN.index,
        Value::Int(first(right)),
    );
    *st.column_x.borrow_mut() = column_x;

    let (row, column) = current_cell(e, view, st).map_or((-1, -1), |(r, c)| (r as i32, c as i32));
    put(e, view, TableView::CURRENT_ROW.index, Value::Int(row));
    put(e, view, TableView::CURRENT_COLUMN.index, Value::Int(column));
}

/// The columns in sight from `sight.0` to `sight.1` across: a column
/// takes room as the provider says, or as wide as it was measured. One
/// never measured is measured on the delegate of its cell in `row` when
/// it may be in sight, and taken as wide as the average of the others
/// when it is before them. A column 0 wide is never in sight.
#[allow(clippy::too_many_arguments)]
fn columns_in_sight(
    e: &Engine,
    st: &TableState,
    d: &ljos_engine::ViewDelegates,
    row: usize,
    given: &[Option<f64>],
    widths: &mut [f64],
    sight: (f64, f64),
    gap: f64,
) -> Vec<usize> {
    let cols = given.len();
    let mut shown = Vec::new();
    let mut x = 0.0;
    for c in 0..cols {
        if x >= sight.1 {
            break;
        }
        let wide = match (given[c], widths[c]) {
            (Some(g), _) => g,
            (None, m) if !m.is_nan() => m,
            (None, _) => match mean(widths) {
                Some(guess) if x + guess <= sight.0 => guess,
                _ => {
                    let m = load(e, st, d, row * cols + c)
                        .map_or(0.0, |o| rd(e, o, Item::IMPLICIT_WIDTH.index));
                    widths[c] = m;
                    m
                }
            },
        };
        if x + wide > sight.0 && x < sight.1 {
            shown.push(c);
        }
        x += wide + gap;
    }
    shown
}

/// The delegate of cell `i`, made or taken from the pool.
fn load(e: &Engine, st: &TableState, d: &ljos_engine::ViewDelegates, i: usize) -> Option<ObjectId> {
    let (o, made) = d.request(e, i, Urgency::Now)?;
    if made == Made::Reused {
        revive(e, st, o);
    }
    Some(o)
}

/// The average of the widths that were measured.
fn mean(widths: &[f64]) -> Option<f64> {
    let known: Vec<f64> = widths.iter().copied().filter(|x| !x.is_nan()).collect();
    (!known.is_empty()).then(|| known.iter().sum::<f64>() / known.len() as f64)
}

/// The place of the model index of cell `index` of the view, as the
/// engine compares places.
fn place_of_cell(st: &TableState, index: usize) -> Option<Vec<(i32, i32)>> {
    let cols = st.cells.columns();
    let line = st.cells.line(index / cols.max(1))?;
    let mut place: Vec<(i32, i32)> = line.path.iter().map(|r| (*r as i32, 0)).collect();
    place.last_mut()?.1 = (index % cols.max(1)) as i32;
    Some(place)
}

/// The view's cell the selection model has as current.
pub(crate) fn current_cell(e: &Engine, view: ObjectId, st: &TableState) -> Option<(usize, usize)> {
    let sm = e.read(view, TableView::SELECTION_MODEL.index).as_object()?;
    let index = e.read(sm, ItemSelectionModel::CURRENT_INDEX.index);
    super::cell_of_index(e, st, &index)
}

/// Whether the cell's model index is among `indexes`.
fn among(e: &Engine, st: &TableState, place: &[(i32, i32)], indexes: &Value) -> bool {
    let Value::List(list) = e.plain(indexes) else {
        return false;
    };
    list.iter()
        .any(|i| e.model_index_place(i, st.model.get()).as_deref() == Some(place))
}

/// Tells the delegate of a tree's cell where its row is in the tree.
/// `first`: the delegate was just made, and its bindings did not run
/// yet; a required property is given here or not at all.
pub(crate) fn decorate(
    e: &Engine,
    view: ObjectId,
    st: &TableState,
    index: usize,
    obj: ObjectId,
    first: bool,
) {
    if !st.tree.get() || !e.is_alive(obj) {
        return;
    }
    let cols = st.cells.columns().max(1);
    let Some(line) = st.cells.line(index / cols) else {
        return;
    };
    let place = place_of_cell(st, index).unwrap_or_default();
    let sm = e.read(view, TableView::SELECTION_MODEL.index).as_object();
    let (current, selected) = match sm {
        Some(sm) => {
            let cur = e.read(sm, ItemSelectionModel::CURRENT_INDEX.index);
            let cur = e.model_index_place(&cur, st.model.get()).as_deref() == Some(&place[..]);
            let sel = e.read(sm, ItemSelectionModel::SELECTED_INDEXES.index);
            (cur, among(e, st, &place, &sel))
        }
        None => (false, false),
    };
    let values = [
        ("treeView", Value::Object(view)),
        ("isTreeNode", Value::Bool(index.is_multiple_of(cols))),
        ("expanded", Value::Bool(st.cells.is_expanded(&line.path))),
        ("hasChildren", Value::Bool(line.has_children)),
        ("depth", Value::Int(line.depth as i32)),
        ("current", Value::Bool(current)),
        ("selected", Value::Bool(selected)),
        ("editing", Value::Bool(false)),
    ];
    for (name, v) in values {
        let Ok(p) = e.property_index(obj, name) else {
            continue;
        };
        let r = if first {
            e.write_internal(obj, p, v, WriteKind::Imperative)
        } else {
            write_computed(e, obj, p, v)
        };
        warn(e, name, r);
    }
}

/// A delegate was tapped: its cell is the current one.
pub(crate) fn tapped(e: &Engine, view: ObjectId, obj: ObjectId, double: bool) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let Some(i) = st.delegates().and_then(|d| d.index_of(obj)) else {
        return;
    };
    let cols = st.cells.columns().max(1);
    if double {
        super::nav::double_tapped(e, view, i / cols);
    } else {
        super::nav::tapped(e, view, &st, (i / cols, i % cols));
    }
}

/// The cell at a point of the content, as last laid out.
pub(crate) fn cell_at(st: &TableState, x: f64, y: f64) -> Option<(usize, usize)> {
    let heights = st.heights.borrow();
    let row = st
        .row_y
        .borrow()
        .iter()
        .enumerate()
        .position(|(r, top)| y >= *top && y < top + heights.get(r).copied().unwrap_or(0.0))?;
    let column = st
        .column_x
        .borrow()
        .iter()
        .position(|(left, wide)| x >= *left && x < left + wide)?;
    Some((row, column))
}

/// Moves the content so row `row` is in sight.
pub(crate) fn show_row(e: &Engine, view: ObjectId, row: i32) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let Some(top) = usize::try_from(row)
        .ok()
        .and_then(|r| st.row_y.borrow().get(r).copied())
    else {
        return;
    };
    show_y(e, view, top);
}

/// Moves the content to `y`, as far as there is content.
pub(crate) fn show_y(e: &Engine, view: ObjectId, y: f64) {
    let h = rd(e, view, Item::HEIGHT.index);
    let content = rd(e, view, Flickable::CONTENT_HEIGHT.index);
    let y = y.min((content - h).max(0.0)).max(0.0);
    put(e, view, Flickable::CONTENT_Y.index, Value::Real(y));
}

/// Gives a delegate back: to the pool with `reuse`, hidden there.
fn release(
    e: &Engine,
    st: &TableState,
    d: &ljos_engine::ViewDelegates,
    obj: ObjectId,
    reuse: bool,
) {
    if !d.release(e, obj, reuse) {
        return;
    }
    if let Some(att) = super::attached_of(e, obj) {
        warn(e, "pooled", e.emit_by_name(att, "pooled", &[]));
    }
    if !e.is_alive(obj) || st.hidden.borrow().contains_key(&obj) {
        return;
    }
    // Not an assignment: what the delegate says about `visible` is there
    // again when it shows another cell.
    match e.push_override(
        obj,
        Item::VISIBLE.index,
        OverrideContent::Value(Value::Bool(false)),
    ) {
        Ok(id) => {
            st.hidden.borrow_mut().insert(obj, id);
        }
        Err(err) => warn(e, "hide", Err(err)),
    }
}

/// A delegate came out of the pool: it is shown again and told.
fn revive(e: &Engine, st: &TableState, obj: ObjectId) {
    let hidden = st.hidden.borrow_mut().remove(&obj);
    if let Some(id) = hidden {
        warn(
            e,
            "show",
            e.remove_override(obj, Item::VISIBLE.index, id, RestoreMode::BindingOrValue),
        );
    }
    if let Some(att) = super::attached_of(e, obj) {
        warn(e, "reused", e.emit_by_name(att, "reused", &[]));
    }
}
