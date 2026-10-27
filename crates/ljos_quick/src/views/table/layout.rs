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
//! or as the `columnWidthProvider` or `setColumnWidth` says. Delegates of cells that leave go
//! to the pool with `reuseItems`, hidden there.
//!
//! A view with a `syncView` lays out by it in the directions
//! `syncDirection` gives: the other's columns, rows, spacing and content
//! size stand for its own, and the two share a content position.

use std::collections::HashSet;
use std::rc::Rc;

use ljos_engine::{
    Engine, ItemModel, ItemSelectionModel, Made, ObjectId, PropIndex, PropertyFlags, Urgency,
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
        crate::item::bump_node(e, n);
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
    e.read_number(obj, p)
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
    // selected, to the delegates.
    if let Some(sm) = e.read(view, TableView::SELECTION_MODEL.index).as_object() {
        let _ = e.read(sm, ItemSelectionModel::CURRENT_INDEX.index);
        let _ = e.read(sm, ItemSelectionModel::SELECTED_INDEXES.index);
    }
    if let Some(n) = st.trigger.get() {
        let _ = e.graph().get(n);
    }
    let _ = e.read(view, TableView::SYNC_DIRECTION.index);
    if let Some((sync, other)) = synced_to(e, view) {
        for p in [
            Flickable::CONTENT_X.index,
            Flickable::CONTENT_Y.index,
            TableView::ROW_SPACING.index,
            TableView::COLUMN_SPACING.index,
            // Its first pass may come after this view's: it has no lines
            // to tell of until then.
            Flickable::CONTENT_WIDTH.index,
            Flickable::CONTENT_HEIGHT.index,
        ] {
            let _ = e.read(sync, p);
        }
        if let Some(n) = other.laid.get() {
            let _ = e.graph().get(n);
        }
    }
}

/// `Qt.Horizontal` and `Qt.Vertical`, as `syncDirection` has them.
const ACROSS: i32 = 1;
const DOWN: i32 = 2;

/// The view `view` is synced to, and its state.
fn synced_to(e: &Engine, view: ObjectId) -> Option<(ObjectId, std::rc::Rc<TableState>)> {
    let sync = e.read(view, TableView::SYNC_VIEW.index).as_object()?;
    if sync == view || !e.is_alive(sync) {
        return None;
    }
    Some((sync, state_of(e, sync)?))
}

/// What a view takes from the view it is synced to.
struct Sync {
    across: Option<(Vec<f64>, f64, f64)>,
    down: Option<(Vec<f64>, f64, f64)>,
}

/// The other view's column widths, column spacing and content width
/// across, and row heights, row spacing and content height down, as it
/// last laid them out.
fn sync_of(e: &Engine, view: ObjectId) -> Option<Sync> {
    let (sync, other) = synced_to(e, view)?;
    let dir = rd(e, view, TableView::SYNC_DIRECTION.index) as i32;
    let n = |p: PropIndex| rd(e, sync, p);
    let across = (dir & ACROSS != 0).then(|| {
        let widths = other.column_x.borrow().iter().map(|c| c.1).collect();
        (
            widths,
            n(TableView::COLUMN_SPACING.index).max(0.0),
            n(Flickable::CONTENT_WIDTH.index),
        )
    });
    let down = (dir & DOWN != 0).then(|| {
        let gap = n(TableView::ROW_SPACING.index).max(0.0);
        let content = n(Flickable::CONTENT_HEIGHT.index);
        let ys = other.row_y.borrow();
        let heights = ys
            .iter()
            .enumerate()
            .map(|(r, y)| ys.get(r + 1).map_or(content, |next| next - gap) - y)
            .collect();
        (heights, gap, content)
    });
    Some(Sync { across, down })
}

/// Shares the content position with the view `view` is synced to, in
/// the synced directions: whichever of the two moved since they last
/// shared it moves the other, and the spacing is the other's.
pub(crate) fn follow(e: &Engine, view: ObjectId, st: &TableState) {
    let Some((sync, _)) = synced_to(e, view) else {
        st.synced_at.set((f64::NAN, f64::NAN));
        return;
    };
    let dir = rd(e, view, TableView::SYNC_DIRECTION.index) as i32;
    let mut at = st.synced_at.get();
    for (bit, content, spacing, last) in [
        (
            ACROSS,
            Flickable::CONTENT_X.index,
            TableView::COLUMN_SPACING.index,
            &mut at.0,
        ),
        (
            DOWN,
            Flickable::CONTENT_Y.index,
            TableView::ROW_SPACING.index,
            &mut at.1,
        ),
    ] {
        if dir & bit == 0 {
            *last = f64::NAN;
            continue;
        }
        let gap = rd(e, sync, spacing);
        if rd(e, view, spacing) != gap {
            warn(
                e,
                "spacing",
                write_computed(e, view, spacing, Value::Real(gap)),
            );
        }
        let (mine, theirs) = (rd(e, view, content), rd(e, sync, content));
        if mine != theirs {
            // Followed until this one moves on its own.
            let (to, moved) = if theirs != *last || last.is_nan() {
                (view, theirs)
            } else {
                (sync, mine)
            };
            put(e, to, content, Value::Real(moved));
            *last = moved;
        } else {
            *last = mine;
        }
    }
    st.synced_at.set(at);
}

/// Reads, tracked, the implicit sizes the last pass laid the cells out
/// by: a cell that grows moves the cells after it.
pub(crate) fn read_sizes(e: &Engine, st: &TableState) {
    let Some(d) = st.delegates() else {
        return;
    };
    for (_, o) in d.live() {
        if e.is_alive(o) && !st.hidden.borrow().contains(&o) {
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

/// How wide and tall qml6 takes a cell whose delegate has no implicit
/// size to be, and a view that laid out no cell.
const UNSIZED: f64 = 50.0;

/// A delegate's implicit width or height, as a column or row takes it.
pub(crate) fn implicit(e: &Engine, o: ObjectId, p: PropIndex) -> f64 {
    let x = rd(e, o, p);
    if x > 0.0 { x } else { UNSIZED }
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
    let sync = sync_of(e, view);
    let across = sync.as_ref().and_then(|s| s.across.as_ref());
    let down = sync.as_ref().and_then(|s| s.down.as_ref());
    let (row_gap, col_gap) = (
        down.map_or_else(|| n(TableView::ROW_SPACING.index).max(0.0), |d| d.1),
        across.map_or_else(|| n(TableView::COLUMN_SPACING.index).max(0.0), |a| a.1),
    );
    let synced_height = |row: usize| down.and_then(|d| d.0.get(row).copied());
    let row_provider = e.read(view, TableView::ROW_HEIGHT_PROVIDER.index);
    let col_provider = e.read(view, TableView::COLUMN_WIDTH_PROVIDER.index);
    let reuse = e.read(view, TableView::REUSE_ITEMS.index).to_boolean();

    let mut heights = std::mem::take(&mut *st.heights.borrow_mut());
    heights.resize(rows, f64::NAN);
    for (row, h) in heights.iter_mut().enumerate() {
        if let Some(synced) = synced_height(row) {
            *h = synced;
        }
    }
    let known: Vec<f64> = heights.iter().copied().filter(|x| !x.is_nan()).collect();
    let mut average = if known.is_empty() {
        0.0
    } else {
        known.iter().sum::<f64>() / known.len() as f64
    };

    let mut widths = std::mem::take(&mut *st.widths.borrow_mut());
    widths.resize(cols, f64::NAN);
    let given: Vec<Option<f64>> = (0..cols)
        .map(|c| {
            across
                .and_then(|a| a.0.get(c).copied())
                .or_else(|| provided(e, view, &col_provider, c))
                .or_else(|| super::resize::own(st, super::resize::COLUMNS, c))
        })
        .collect();
    let sight = (cx, cx + w.max(0.0));

    // Rows the last pass placed wholly out of sight go before any row is
    // loaded, so a row that scrolls in takes the delegate of the one that
    // scrolled out (qml6) rather than a new one. A row whose height
    // changed and is in sight after all takes one back from the pool.
    if reuse && cols > 0 {
        let last_y = st.row_y.borrow();
        let gone: Vec<ObjectId> = d
            .live()
            .into_iter()
            .filter(|&(i, o)| {
                let row = i / cols;
                let (Some(&top), Some(&tall)) = (last_y.get(row), heights.get(row)) else {
                    return false;
                };
                !tall.is_nan()
                    && (top + tall <= cy || top > cy + h.max(0.0))
                    && !st.editing.get().is_some_and(|ed| ed.delegate == o)
            })
            .map(|(_, o)| o)
            .collect();
        drop(last_y);
        for o in gone {
            release(e, st, &d, o, reuse);
        }
    }

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
        // A row that ends where the content in sight begins is not in
        // sight (qml6); one of no known height may be.
        let reaches = if guess > 0.0 { y + guess > cy } else { y >= cy };
        let in_sight =
            cols > 0 && reaches && y <= cy + h.max(0.0) && loaded_rows.len() < MAX_LOADED_ROWS;
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
                    let t = implicit(e, o, Item::IMPLICIT_HEIGHT.index);
                    tallest = Some(tallest.map_or(t, |m: f64| m.max(t)));
                }
            }
            // A row with no column in sight is not measured.
            let fixed = synced_height(row)
                .or_else(|| provided(e, view, &row_provider, row))
                .or_else(|| super::resize::own(st, super::resize::ROWS, row));
            if let Some(t) = fixed.or(tallest) {
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
    let content_h = match down {
        Some(d) => d.2,
        None if rows > 0 => y - row_gap,
        None => 0.0,
    };
    let shown = shown.unwrap_or_default();

    // Cells that left: their delegates go, but for the edited cell's,
    // which is kept out of the way.
    for (i, o) in d.live() {
        if wanted.contains(&i) {
            continue;
        }
        if st.editing.get().is_some_and(|ed| ed.delegate == o) {
            let wide = rd(e, o, Item::WIDTH.index);
            warn(
                e,
                "away",
                write_placed(e, o, Item::X.index, Value::Real(-(AWAY + wide))),
            );
        } else {
            release(e, st, &d, o, reuse);
        }
    }
    let edited_in_sight = st
        .editing
        .get()
        .and_then(|ed| d.index_of(ed.delegate))
        .is_some_and(|i| wanted.contains(&i));
    super::edit::set_away(st, st.editing.get().is_some() && !edited_in_sight);

    // Columns: as wide as the provider says or the widest delegate in
    // sight; one never measured, as the average of those that were.
    for &c in &shown {
        let widest = loaded_rows
            .iter()
            .filter_map(|row| d.object_at(row * cols + c))
            .map(|o| implicit(e, o, Item::IMPLICIT_WIDTH.index))
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
    let content_w = match across {
        Some(a) => a.2,
        None if cols > 0 => x - col_gap,
        None => 0.0,
    };
    // A view with cells of which none was ever in sight (it has no room)
    // is as large as qml6 says one is then: what lets a view sized by its
    // content, a vertical header, find room for its first cells.
    let never = |sizes: &[f64]| sizes.iter().all(|x| x.is_nan());
    let unlaid = rows > 0 && cols > 0 && shown.is_empty() && never(&widths) && never(&heights);
    let content_w = if unlaid && across.is_none() {
        UNSIZED
    } else {
        content_w
    };
    let content_h = if unlaid && down.is_none() {
        UNSIZED
    } else {
        content_h
    };

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
    // The views synced to this one lay out again when its lines moved.
    let moved = *st.row_y.borrow() != row_y || *st.column_x.borrow() != column_x;
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
    // The rows loaded are the rows in sight, when a column is.
    let (top, bottom) = if shown.is_empty() {
        (-1, -1)
    } else {
        (first(loaded_rows.first()), first(loaded_rows.last()))
    };
    if rows == 0 || loaded_rows.last() == Some(&(rows - 1)) {
        let cells: Rc<dyn ItemModel> = st.cells.clone();
        let current = |e: &Engine, view| -> Option<Rc<dyn ItemModel>> {
            Some(state_of(e, view)?.cells.clone())
        };
        crate::views::fetch::soon(e, view, cells, current);
    }
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
    if moved && let Some(n) = st.laid.get() {
        crate::item::bump_node(e, n);
    }

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
                        .map_or(0.0, |o| implicit(e, o, Item::IMPLICIT_WIDTH.index));
                    widths[c] = m;
                    m
                }
            },
        };
        if wide > 0.0 && x + wide > sight.0 && x < sight.1 {
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
    place.last_mut()?.1 = st.cells.logical_column(index % cols.max(1)) as i32;
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

/// Tells the delegate of a cell whether it is current, selected and
/// edited, and a tree's delegate where its row is in the tree. A table's
/// delegate is told only what it declares `required`, as on qml6.
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
    if !e.is_alive(obj) {
        return;
    }
    let tree = st.tree.get();
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
    let editing = st
        .editing
        .get()
        .is_some_and(|ed| ed.cell == (index / cols, index % cols));
    let in_tree = [
        ("treeView", Value::Object(view)),
        ("isTreeNode", Value::Bool(index.is_multiple_of(cols))),
        ("expanded", Value::Bool(st.cells.is_expanded(&line.path))),
        ("hasChildren", Value::Bool(line.has_children)),
        ("depth", Value::Int(line.depth as i32)),
    ];
    let in_any = [
        ("current", Value::Bool(current)),
        ("selected", Value::Bool(selected)),
        ("editing", Value::Bool(editing)),
    ];
    // A `TableViewDelegate`'s and a header's: given only where required.
    let views = [
        ("tableView", Value::Object(view)),
        ("headerView", Value::Object(view)),
    ];
    let required = |p: ljos_engine::PropIndex| {
        e.meta_of(obj).is_some_and(|meta| {
            meta.properties
                .get(p.0 as usize)
                .is_some_and(|d| d.flags.contains(PropertyFlags::REQUIRED))
        })
    };
    let tree_part = if tree { &in_tree[..] } else { &[] };
    let given = tree_part.iter().chain(&in_any).map(|g| (g, tree));
    for ((name, v), always) in given.chain(views.iter().map(|g| (g, false))) {
        let (name, v) = (*name, v.clone());
        let Ok(p) = e.property_index(obj, name) else {
            continue;
        };
        if !always && !required(p) {
            continue;
        }
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
    super::nav::tapped_cell(e, view, &st, (i / cols, i % cols), double);
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

/// How far left of the content the delegate of an edited cell out of
/// sight is kept.
const AWAY: f64 = 10_000.0;

/// Destroys the delegate `obj`, which is not pooled.
pub(crate) fn let_go(e: &Engine, st: &TableState, obj: ObjectId) {
    if let Some(d) = st.delegates() {
        release(e, st, &d, obj, false);
    }
}

/// Gives a delegate back: to the pool with `reuse`, not shown there.
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
    if e.is_alive(obj) && st.hidden.borrow_mut().insert(obj) {
        crate::views::cull::pool(e, obj, true);
    }
}

/// A delegate came out of the pool: it is shown again and told.
fn revive(e: &Engine, st: &TableState, obj: ObjectId) {
    if st.hidden.borrow_mut().remove(&obj) {
        crate::views::cull::pool(e, obj, false);
    }
    if let Some(att) = super::attached_of(e, obj) {
        warn(e, "reused", e.emit_by_name(att, "reused", &[]));
    }
}
