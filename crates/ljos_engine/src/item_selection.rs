//! `ItemSelectionModel` (QtQml.Models): which items of a model are
//! selected and which one is current, as a view that selects is given
//! it in `selectionModel`. And the model indexes that name the items.
//!
//! An index is a frozen script object with Qt's members (`row`,
//! `column`, `parent`, `valid`, `model`, `internalId`): a model makes
//! one with `index(row, column)`, and the selection model reads any
//! object with those members, so a view of a tree can hand it indexes
//! whose `parent` is another index. Two indexes of one item are equal
//! in what they hold, not as objects: compare `row` and `column`.
//!
//! In a tree model ([`crate::ItemTree`]) the rows of each level are
//! known, so whole rows and columns are selected and read at every
//! level; in other models only at the top.
//!
//! The selection is kept as Qt keeps it: ranges that do not overlap, in
//! the order they were made, and a selection being made (a command with
//! `Current` replaces it) with its command, which the next command
//! without `Current` merges into the ranges. Rows the model gains, loses
//! or moves take the ranges and the current item along, as qml6 does.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::builtins::QtObject;
use crate::cx::Cx;
use crate::engine::Engine;
use crate::ids::ObjectId;
use crate::model::{ItemModel, ModelChange};
use crate::tree::{ItemTree, TreeChange};
use crate::value::Value;
use crate::{QmlObject, qml_impl};

/// The keys of `ItemSelectionModel.SelectionFlag`.
mod flag {
    pub const CLEAR: i64 = 0x1;
    pub const SELECT: i64 = 0x2;
    pub const DESELECT: i64 = 0x4;
    pub const TOGGLE: i64 = 0x8;
    pub const CURRENT: i64 = 0x10;
    pub const ROWS: i64 = 0x20;
    pub const COLUMNS: i64 = 0x40;
}

fn selection_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    let keys = vec![
        ("NoUpdate", 0x0),
        ("Clear", flag::CLEAR),
        ("Select", flag::SELECT),
        ("Deselect", flag::DESELECT),
        ("Toggle", flag::TOGGLE),
        ("Current", flag::CURRENT),
        ("Rows", flag::ROWS),
        ("Columns", flag::COLUMNS),
        ("SelectCurrent", 0x12),
        ("ToggleCurrent", 0x18),
        ("ClearAndSelect", 0x3),
    ];
    vec![("SelectionFlag", keys.clone()), ("SelectionFlags", keys)]
}

/// Where an item is: its row and column under each of its ancestors,
/// the top level first. A list's items are one step deep.
type Place = Vec<(i32, i32)>;

/// How deep an index's parents are followed before it is taken to loop.
const DEPTH: usize = 256;

/// `ItemSelectionModel`.
#[derive(QmlObject)]
#[qml(
    module = "QtQml.Models",
    name = "ItemSelectionModel",
    extends = QtObject,
    enums = selection_enums
)]
pub struct ItemSelectionModel {
    #[qml(base)]
    pub base: QtObject,
    #[qml(notify)]
    pub model: Value,
    #[qml(notify, readonly, rename = "hasSelection")]
    pub has_selection: bool,
    #[qml(notify, readonly, rename = "currentIndex")]
    pub current_index: Value,
    #[qml(notify, readonly, rename = "selectedIndexes")]
    pub selected_indexes: Vec<Value>,
    /// One range per selected item. Its change is told by the declared
    /// `selectionChanged(selected, deselected)`, as Qt's is: a notify of its
    /// own would declare a second `selectionChanged` without the arguments.
    #[qml(readonly)]
    pub selection: Vec<Value>,
    #[qml(skip)]
    state: Rc<RefCell<Selection>>,
}

impl Default for ItemSelectionModel {
    fn default() -> Self {
        Self {
            base: QtObject::default(),
            model: Value::Undefined,
            has_selection: false,
            current_index: Value::Undefined,
            selected_indexes: Vec::new(),
            selection: Vec::new(),
            state: Rc::default(),
        }
    }
}

impl std::fmt::Debug for ItemSelectionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemSelectionModel")
            .field("selection", &self.state.borrow())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct Selection {
    /// The model the places are in; another model starts afresh.
    model: Option<ObjectId>,
    /// The ranges made part of the selection.
    ranges: Vec<Range>,
    /// The selection being made, and what its command does with it.
    pending: Vec<Range>,
    pending_command: i64,
    /// `ranges` with `pending` merged in: the selection scripts see.
    shown: Vec<Range>,
    current: Option<Place>,
    /// The subscription to the model's changes.
    following: Option<Following>,
}

/// A subscription to the changes of the model a selection is in.
enum Following {
    List(Rc<dyn ItemModel>, u64),
    Tree(Rc<dyn ItemTree>, u64),
}

impl std::fmt::Debug for Following {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Following::List(..) => "List",
            Following::Tree(..) => "Tree",
        })
    }
}

impl Following {
    fn stop(self) {
        match self {
            Following::List(model, id) => model.unsubscribe(id),
            Following::Tree(tree, id) => tree.unsubscribe(id),
        }
    }
}

/// Items from `top` to `bottom` and `left` to `right` under one parent.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Range {
    parent: Place,
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
}

impl Range {
    fn rows(&self, top: i32, bottom: i32) -> Range {
        Range {
            top,
            bottom,
            ..self.clone()
        }
    }

    fn contains(&self, place: &[(i32, i32)]) -> bool {
        place.split_last().is_some_and(|(last, parent)| {
            parent == self.parent.as_slice()
                && (self.top..=self.bottom).contains(&last.0)
                && (self.left..=self.right).contains(&last.1)
        })
    }

    fn intersects(&self, o: &Range) -> bool {
        self.parent == o.parent
            && self.top <= o.bottom
            && o.top <= self.bottom
            && self.left <= o.right
            && o.left <= self.right
    }

    /// What is left of the range with `o` taken out: the rows above it,
    /// the rows below it, then the columns left and right of it.
    fn minus(&self, o: &Range) -> Vec<Range> {
        if !self.intersects(o) {
            return vec![self.clone()];
        }
        let piece = |top, left, bottom, right| Range {
            parent: self.parent.clone(),
            top,
            left,
            bottom,
            right,
        };
        let mut out = Vec::new();
        if self.top < o.top {
            out.push(piece(self.top, self.left, o.top - 1, self.right));
        }
        if o.bottom < self.bottom {
            out.push(piece(o.bottom + 1, self.left, self.bottom, self.right));
        }
        let (top, bottom) = (self.top.max(o.top), self.bottom.min(o.bottom));
        if self.left < o.left {
            out.push(piece(top, self.left, bottom, o.left - 1));
        }
        if o.right < self.right {
            out.push(piece(top, o.right + 1, bottom, self.right));
        }
        out
    }

    fn places(&self) -> impl Iterator<Item = Place> + '_ {
        (self.top..=self.bottom).flat_map(move |r| {
            (self.left..=self.right).map(move |c| {
                let mut p = self.parent.clone();
                p.push((r, c));
                p
            })
        })
    }
}

/// `a` with what `b` has taken out, as Qt merges: the ranges `b` does
/// not touch keep their order, and what is left of the others follows.
fn take_out(a: &[Range], b: &[Range]) -> Vec<Range> {
    let mut kept = Vec::new();
    let mut pieces = Vec::new();
    for r in a {
        if b.iter().any(|o| r.intersects(o)) {
            let mut left = vec![r.clone()];
            for o in b {
                left = left.iter().flat_map(|x| x.minus(o)).collect();
            }
            pieces.extend(left);
        } else {
            kept.push(r.clone());
        }
    }
    kept.extend(pieces);
    kept
}

/// The ranges of `b`, none overlapping another.
fn distinct(b: &[Range]) -> Vec<Range> {
    let mut out: Vec<Range> = Vec::new();
    for r in b {
        let new = take_out(std::slice::from_ref(r), &out);
        out.extend(new);
    }
    out
}

/// `b` merged into `a` as `command` says.
fn merge(a: &[Range], b: &[Range], command: i64) -> Vec<Range> {
    if b.is_empty() {
        return a.to_vec();
    }
    let b = distinct(b);
    let mut out = take_out(a, &b);
    if command & flag::SELECT != 0 {
        out.extend(b);
    } else if command & flag::TOGGLE != 0 {
        out.extend(take_out(&b, a));
    } else if command & flag::DESELECT == 0 {
        return a.to_vec();
    }
    out
}

#[qml_impl]
impl ItemSelectionModel {
    #[qml(signal, name = "selectionChanged")]
    fn selection_changed(selected: Value, deselected: Value);

    #[qml(signal, name = "currentChanged")]
    fn current_changed(current: Value, previous: Value);

    #[qml(signal, name = "currentRowChanged")]
    fn current_row_changed(current: Value, previous: Value);

    #[qml(signal, name = "currentColumnChanged")]
    fn current_column_changed(current: Value, previous: Value);

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), crate::EngineError> {
        let current = invalid_index(cx.engine);
        cx.engine
            .set_internal(cx.this, Self::CURRENT_INDEX, current)?;
        let _ = state_of(cx);
        let me = cx.this;
        cx.engine.watch(me, Self::MODEL.index, move |e, _| {
            let _ = state_of(&Cx::new(e, me));
        })?;
        cx.engine.connect_by_name(me, "destroyed", move |e, _| {
            let state = e.with::<ItemSelectionModel, _>(me, |m| m.state.clone());
            if let Ok(state) = state
                && let Some(f) = state.borrow_mut().following.take()
            {
                f.stop();
            }
        })?;
        Ok(())
    }

    /// `select(index, command)`, or `select(selection, command)` with the
    /// ranges of a selection.
    #[qml(invokable)]
    fn select(cx: &Cx, what: Value, command: Value) {
        let command = flags(&command);
        let ranges = ranges_in(cx, &what, command);
        apply(cx, ranges, command);
    }

    #[qml(invokable, name = "setCurrentIndex")]
    fn set_current_index(cx: &Cx, index: Value, command: Value) {
        let command = flags(&command);
        let place = place_in(cx, &index);
        let ranges = place.as_deref().map(|p| expand(cx, p, p, command));
        apply(cx, ranges.into_iter().flatten().collect(), command);
        make_current(cx, place);
    }

    /// The selection and the current item go, and both are told.
    #[qml(invokable)]
    fn clear(cx: &Cx) {
        apply(cx, Vec::new(), flag::CLEAR);
        make_current(cx, None);
    }

    /// The selection and the current item go, and nothing is told.
    #[qml(invokable)]
    fn reset(cx: &Cx) {
        let Some(state) = state_of(cx) else { return };
        forget(&mut state.borrow_mut());
        publish(cx, &state);
        let _ = cx
            .engine
            .set_internal(cx.this, Self::CURRENT_INDEX, invalid_index(cx.engine));
    }

    #[qml(invokable, name = "clearSelection")]
    fn clear_selection(cx: &Cx) {
        apply(cx, Vec::new(), flag::CLEAR);
    }

    #[qml(invokable, name = "clearCurrentIndex")]
    fn clear_current_index(cx: &Cx) {
        make_current(cx, None);
    }

    #[qml(invokable, name = "isSelected")]
    fn is_selected(cx: &Cx, index: Value) -> bool {
        let Some(place) = place_in(cx, &index) else {
            return false;
        };
        selected_all(cx, &[place])
    }

    /// Whether every column of the row is selected.
    #[qml(invokable, name = "isRowSelected")]
    fn is_row_selected(cx: &Cx, row: Value, parent: Value) -> bool {
        let Some(line) = line(cx, &parent, Line::Row(int(&row))) else {
            return false;
        };
        selected_all(cx, &line)
    }

    /// Whether every row of the column is selected.
    #[qml(invokable, name = "isColumnSelected")]
    fn is_column_selected(cx: &Cx, column: Value, parent: Value) -> bool {
        let Some(line) = line(cx, &parent, Line::Column(int(&column))) else {
            return false;
        };
        selected_all(cx, &line)
    }

    #[qml(invokable, name = "rowIntersectsSelection")]
    fn row_intersects_selection(cx: &Cx, row: Value, parent: Value) -> bool {
        let Some(line) = line(cx, &parent, Line::Row(int(&row))) else {
            return false;
        };
        selected_any(cx, &line)
    }

    #[qml(invokable, name = "columnIntersectsSelection")]
    fn column_intersects_selection(cx: &Cx, column: Value, parent: Value) -> bool {
        let Some(line) = line(cx, &parent, Line::Column(int(&column))) else {
            return false;
        };
        selected_any(cx, &line)
    }

    /// The rows all of whose columns are selected, each as its index in
    /// `column`.
    #[qml(invokable, name = "selectedRows")]
    fn selected_rows(cx: &Cx, column: Value) -> Value {
        whole_lines(cx, true, int_or(&column, 0))
    }

    /// The columns all of whose rows are selected, each as its index in
    /// `row`.
    #[qml(invokable, name = "selectedColumns")]
    fn selected_columns(cx: &Cx, row: Value) -> Value {
        whole_lines(cx, false, int_or(&row, 0))
    }
}

fn forget(s: &mut Selection) {
    s.ranges.clear();
    s.pending.clear();
    s.shown.clear();
    s.current = None;
}

/// The selection, in step with the model: a selection belongs to the
/// model it was made in, and follows that model's changes.
fn state_of(cx: &Cx) -> Option<Rc<RefCell<Selection>>> {
    let state = cx
        .engine
        .with::<ItemSelectionModel, _>(cx.this, |m| m.state.clone())
        .ok()?;
    let model = model_of(cx);
    let moved = {
        let mut s = state.borrow_mut();
        let moved = s.model != model;
        if moved {
            forget(&mut s);
            s.model = model;
            if let Some(f) = s.following.take() {
                f.stop();
            }
        }
        moved
    };
    if moved {
        let following = model.and_then(|m| follow(cx.engine, cx.this, m));
        state.borrow_mut().following = following;
        publish(cx, &state);
        let _ = cx.engine.set_internal(
            cx.this,
            ItemSelectionModel::CURRENT_INDEX,
            invalid_index(cx.engine),
        );
    }
    Some(state)
}

fn follow(engine: &Engine, me: ObjectId, model: ObjectId) -> Option<Following> {
    let items = engine.object_model(model)?;
    if let Some(tree) = items.tree() {
        let id = tree.subscribe(Rc::new(move |e: &Engine, change: &TreeChange| {
            let cx = Cx::new(e, me);
            match change {
                TreeChange::RowsInserted {
                    parent,
                    first,
                    count,
                } => inserted(&cx, parent, *first, *count),
                TreeChange::RowsRemoved {
                    parent,
                    first,
                    count,
                } => removed(&cx, parent, *first, *count),
                TreeChange::ModelReset => model_reset(&cx),
                TreeChange::DataChanged { .. } => {}
            }
        }));
        return Some(Following::Tree(tree, id));
    }
    let id = items.subscribe(Rc::new(move |e: &Engine, change: &ModelChange| {
        let cx = Cx::new(e, me);
        match change {
            ModelChange::RowsInserted { first, count } => inserted(&cx, &[], *first, *count),
            ModelChange::RowsRemoved { first, count } => removed(&cx, &[], *first, *count),
            ModelChange::RowsMoved { from, to, count } => moved(&cx, *from, *to, *count),
            ModelChange::ModelReset => model_reset(&cx),
            _ => {}
        }
    }));
    Some(Following::List(items, id))
}

/// The selection as it is when its model told a change; `None` when the
/// selection model is gone or now has another model.
fn following(cx: &Cx) -> Option<Rc<RefCell<Selection>>> {
    if !cx.engine.is_alive(cx.this) {
        return None;
    }
    let state = cx
        .engine
        .with::<ItemSelectionModel, _>(cx.this, |m| m.state.clone())
        .ok()?;
    let ours = state.borrow().model == model_of(cx);
    ours.then_some(state)
}

/// Whether a range's parent is the row at `at` (rows hang under the
/// first column of their parent).
fn is_at(parent: &[(i32, i32)], at: &[usize]) -> bool {
    parent.len() == at.len() && under(parent, at)
}

/// Whether `place` goes through the row at `at`.
fn under(place: &[(i32, i32)], at: &[usize]) -> bool {
    place.len() >= at.len()
        && place
            .iter()
            .zip(at)
            .all(|(&(r, c), &a)| c == 0 && usize::try_from(r).is_ok_and(|r| r == a))
}

fn to_i32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// Rows were inserted under the row at `at`: the ranges and the current
/// item below them move down, and a range they land in is split.
fn inserted(cx: &Cx, at: &[usize], first: usize, count: usize) {
    let Some(state) = following(cx) else { return };
    let (first, count) = (to_i32(first), to_i32(count));
    let k = at.len();
    let mut shifted = false;
    let mut map = |list: &[Range]| -> Vec<Range> {
        let mut out = Vec::new();
        for r in list {
            if is_at(&r.parent, at) {
                if r.bottom < first {
                    out.push(r.clone());
                } else if r.top >= first {
                    shifted = true;
                    out.push(r.rows(r.top + count, r.bottom + count));
                } else {
                    out.push(r.rows(r.top, first - 1));
                    out.push(r.rows(first + count, r.bottom + count));
                }
            } else if r.parent.len() > k && under(&r.parent, at) && r.parent[k].0 >= first {
                shifted = true;
                let mut r = r.clone();
                r.parent[k].0 += count;
                out.push(r);
            } else {
                out.push(r.clone());
            }
        }
        out
    };
    let current = {
        let mut s = state.borrow_mut();
        s.ranges = map(&s.ranges);
        s.pending = map(&s.pending);
        s.shown = merge(&s.ranges, &s.pending, s.pending_command);
        if let Some(c) = s.current.as_mut()
            && c.len() > k
            && under(c, at)
            && c[k].0 >= first
        {
            c[k].0 += count;
        }
        s.current.clone()
    };
    settle(cx, &state, current);
    if shifted {
        tell(cx, &[], &[]);
    }
}

/// Rows under the row at `at` are gone: the selected items among them
/// leave the selection, the rest below them move up, and a current item
/// among them hands over to the row above them, else the row below them,
/// else their parent.
fn removed(cx: &Cx, at: &[usize], first: usize, count: usize) {
    let Some(state) = following(cx) else { return };
    let rows_now = rows_under(cx, at);
    let (first, count) = (to_i32(first), to_i32(count));
    let last = first + count - 1;
    let k = at.len();
    let gone_row = |row: i32| (first..=last).contains(&row);
    let mut gone: Vec<Range> = Vec::new();
    for r in &state.borrow().shown {
        if is_at(&r.parent, at) {
            let (top, bottom) = (r.top.max(first), r.bottom.min(last));
            if top <= bottom {
                gone.push(r.rows(top, bottom));
            }
        } else if r.parent.len() > k && under(&r.parent, at) && gone_row(r.parent[k].0) {
            gone.push(r.clone());
        }
    }
    let mut shifted = false;
    let mut map = |list: &[Range]| -> Vec<Range> {
        let mut out = Vec::new();
        for r in list {
            if is_at(&r.parent, at) {
                if r.bottom < first {
                    out.push(r.clone());
                } else if r.top > last {
                    shifted = true;
                    out.push(r.rows(r.top - count, r.bottom - count));
                } else {
                    if r.top < first {
                        out.push(r.rows(r.top, first - 1));
                    }
                    if r.bottom > last {
                        out.push(r.rows(first, r.bottom - count));
                    }
                }
            } else if r.parent.len() > k && under(&r.parent, at) {
                let row = r.parent[k].0;
                if gone_row(row) {
                    continue;
                }
                let mut r = r.clone();
                if row > last {
                    shifted = true;
                    r.parent[k].0 -= count;
                }
                out.push(r);
            } else {
                out.push(r.clone());
            }
        }
        out
    };
    let (handed, current) = {
        let mut s = state.borrow_mut();
        s.ranges = map(&s.ranges);
        s.pending = map(&s.pending);
        s.shown = merge(&s.ranges, &s.pending, s.pending_command);
        let mut handed = None;
        if let Some(c) = s.current.clone()
            && c.len() > k
            && under(&c, at)
        {
            let row = c[k].0;
            if gone_row(row) {
                let parent: Place = c[..k].to_vec();
                let column = c[k].1;
                let at_row = |r: i32| {
                    let mut p = parent.clone();
                    p.push((r, column));
                    p
                };
                // Told in the rows as they were; kept in the rows as they are.
                let (told, kept) = if first > 0 {
                    (Some(at_row(first - 1)), Some(at_row(first - 1)))
                } else if last + 1 < rows_now + count {
                    (Some(at_row(last + 1)), Some(at_row(first)))
                } else {
                    let p = (!parent.is_empty()).then_some(parent);
                    (p.clone(), p)
                };
                handed = Some((c.clone(), told));
                s.current = kept;
            } else if row > last {
                s.current.as_mut().expect("there is a current item")[k].0 -= count;
            }
        }
        (handed, s.current.clone())
    };
    if let Some((previous, told)) = handed {
        tell_current(cx, &told, &Some(previous));
    }
    settle(cx, &state, current);
    if !gone.is_empty() {
        tell(cx, &[], &gone);
    } else if shifted {
        tell(cx, &[], &[]);
    }
}

/// How many rows are under the row at `at` now.
fn rows_under(cx: &Cx, at: &[usize]) -> i32 {
    let Some(model) = model_of(cx).and_then(|m| cx.engine.object_model(m)) else {
        return 0;
    };
    match model.tree() {
        Some(tree) => to_i32(tree.row_count(at)),
        None => to_i32(model.row_count()),
    }
}

/// Rows of a list moved: the selection and the current item go with
/// them, and nothing is told, as on qml6.
fn moved(cx: &Cx, from: usize, to: usize, count: usize) {
    let Some(state) = following(cx) else { return };
    let (from, to, count) = (to_i32(from), to_i32(to), to_i32(count));
    let row = |r: i32| {
        if (from..from + count).contains(&r) {
            return to + (r - from);
        }
        let r = if r >= from + count { r - count } else { r };
        if r >= to { r + count } else { r }
    };
    let map = |list: &[Range]| -> Vec<Range> {
        let mut out = Vec::new();
        for r in list {
            if !r.parent.is_empty() {
                out.push(r.clone());
                continue;
            }
            let rows: Vec<i32> = (r.top..=r.bottom).map(row).collect();
            if rows.windows(2).all(|w| w[1] == w[0] + 1) {
                out.push(r.rows(rows[0], rows[rows.len() - 1]));
                continue;
            }
            // The rows of the range are apart now: one range per run.
            let mut rows = rows;
            rows.sort_unstable();
            let mut start = rows[0];
            for (i, &x) in rows.iter().enumerate() {
                let next = rows.get(i + 1);
                if next != Some(&(x + 1)) {
                    out.push(r.rows(start, x));
                    if let Some(&n) = next {
                        start = n;
                    }
                }
            }
        }
        out
    };
    let current = {
        let mut s = state.borrow_mut();
        s.ranges = map(&s.ranges);
        s.pending = map(&s.pending);
        s.shown = merge(&s.ranges, &s.pending, s.pending_command);
        if let Some(c) = s.current.as_mut()
            && let Some(first) = c.first_mut()
        {
            first.0 = row(first.0);
        }
        s.current.clone()
    };
    settle(cx, &state, current);
}

/// The model was reset: nothing is selected or current, untold.
fn model_reset(cx: &Cx) {
    let Some(state) = following(cx) else { return };
    forget(&mut state.borrow_mut());
    settle(cx, &state, None);
}

/// Brings the properties in step after the model changed.
fn settle(cx: &Cx, state: &RefCell<Selection>, current: Option<Place>) {
    publish(cx, state);
    let index = match &current {
        Some(p) => make_index(cx.engine, model_of(cx), p),
        None => invalid_index(cx.engine),
    };
    let _ = cx
        .engine
        .set_internal(cx.this, ItemSelectionModel::CURRENT_INDEX, index);
}

fn model_of(cx: &Cx) -> Option<ObjectId> {
    cx.engine
        .read(cx.this, ItemSelectionModel::MODEL.index)
        .as_object()
}

/// The rows and columns of the level `parent` is the parent of, when the
/// model can say; a list has one column.
fn extent(cx: &Cx, parent: &[(i32, i32)]) -> Option<(i32, i32)> {
    let model = cx.engine.object_model(model_of(cx)?)?;
    if let Some(tree) = model.tree() {
        // A tree's rows hang under the first column of their parent.
        let mut path = Vec::with_capacity(parent.len());
        for &(row, column) in parent {
            if column != 0 {
                return Some((0, 0));
            }
            path.push(usize::try_from(row).ok()?);
        }
        return Some((to_i32(tree.row_count(&path)), to_i32(tree.column_count())));
    }
    if !parent.is_empty() {
        return None;
    }
    Some((to_i32(model.row_count()), 1))
}

/// Where `index` is in this model; `None` for an index that is not valid,
/// is of another model, or is outside the model.
fn place_in(cx: &Cx, index: &Value) -> Option<Place> {
    let model = model_of(cx);
    let place = place_of(cx.engine, index, model)?;
    let (last, parent) = place.split_last()?;
    if let Some((rows, columns)) = extent(cx, parent)
        && !(0..rows).contains(&last.0) | !(0..columns).contains(&last.1)
    {
        return None;
    }
    Some(place)
}

/// The place an index names, read from its members; `None` when it is not
/// valid or of another model than `model`.
pub(crate) fn place_of(engine: &Engine, index: &Value, model: Option<ObjectId>) -> Option<Place> {
    let mut place = Vec::new();
    let mut at = index.clone();
    for _ in 0..DEPTH {
        let valid = engine
            .js_member(&at, "valid")
            .is_some_and(|v| v.to_boolean());
        if !valid {
            place.reverse();
            return (!place.is_empty()).then_some(place);
        }
        if place.is_empty() {
            let of = engine.js_member(&at, "model").and_then(|m| m.as_object());
            if of.is_some() && of != model {
                return None;
            }
        }
        let member = |name| {
            engine
                .js_member(&at, name)
                .map_or(-1, |v| v.to_number() as i32)
        };
        place.push((member("row"), member("column")));
        at = engine.js_member(&at, "parent").unwrap_or(Value::Undefined);
    }
    None
}

fn flags(command: &Value) -> i64 {
    command.to_number() as i64
}

fn int(v: &Value) -> i32 {
    v.to_number() as i32
}

fn int_or(v: &Value, default: i32) -> i32 {
    match v {
        Value::Undefined | Value::Null => default,
        v => int(v),
    }
}

/// The range from `top_left` to `bottom_right`, made whole rows or whole
/// columns when `command` says `Rows` or `Columns`.
fn expand(
    cx: &Cx,
    top_left: &[(i32, i32)],
    bottom_right: &[(i32, i32)],
    command: i64,
) -> Option<Range> {
    let (tl, parent) = top_left.split_last()?;
    let (br, other) = bottom_right.split_last()?;
    if parent != other {
        return None;
    }
    let mut r = Range {
        parent: parent.to_vec(),
        top: tl.0.min(br.0),
        left: tl.1.min(br.1),
        bottom: tl.0.max(br.0),
        right: tl.1.max(br.1),
    };
    let (rows, columns) = extent(cx, parent).unwrap_or((r.bottom + 1, r.right + 1));
    if command & flag::ROWS != 0 {
        (r.left, r.right) = (0, columns - 1);
    }
    if command & flag::COLUMNS != 0 {
        (r.top, r.bottom) = (0, rows - 1);
    }
    (r.top <= r.bottom && r.left <= r.right).then_some(r)
}

/// The ranges `what` names: an index, a range, or a list of ranges (a
/// selection). A view in Rust names a range by a record of its
/// `topLeft` and `bottomRight`.
fn ranges_in(cx: &Cx, what: &Value, command: i64) -> Vec<Range> {
    let e = cx.engine;
    let model = model_of(cx);
    let member = |v: &Value, key: &str| match v.as_map() {
        Some(fields) => fields
            .iter()
            .find(|(k, _)| &**k == key)
            .map(|(_, v)| v.clone()),
        None => e.js_member(v, key),
    };
    let one = |v: &Value| -> Option<Range> {
        if let Some(tl) = member(v, "topLeft") {
            let br = member(v, "bottomRight")?;
            let (tl, br) = (place_of(e, &tl, model)?, place_of(e, &br, model)?);
            return expand(cx, &tl, &br, command);
        }
        let place = place_in(cx, v)?;
        expand(cx, &place, &place, command)
    };
    if e.js_member(what, "valid").is_some() {
        return one(what).into_iter().collect();
    }
    match e.plain(what) {
        Value::List(items) => items.iter().filter_map(one).collect(),
        _ => Vec::new(),
    }
}

/// Changes the selection as `command` says, with `items` the ranges it
/// names, and tells what changed.
fn apply(cx: &Cx, items: Vec<Range>, command: i64) {
    let Some(state) = state_of(cx) else { return };
    let changes = flag::CLEAR | flag::SELECT | flag::DESELECT | flag::TOGGLE;
    if command & changes == 0 {
        return;
    }
    let (old, new) = {
        let mut s = state.borrow_mut();
        let old = s.shown.clone();
        if command & flag::CLEAR != 0 {
            s.ranges.clear();
            s.pending.clear();
        } else if command & flag::CURRENT == 0 {
            s.ranges = merge(&s.ranges, &s.pending, s.pending_command);
            s.pending.clear();
        }
        if command & (flag::SELECT | flag::DESELECT | flag::TOGGLE) != 0 {
            s.pending = items;
            s.pending_command = command;
        }
        s.shown = merge(&s.ranges, &s.pending, s.pending_command);
        (old, s.shown.clone())
    };
    let (added, removed) = (take_out(&new, &old), take_out(&old, &new));
    if added.is_empty() && removed.is_empty() {
        return;
    }
    publish(cx, &state);
    tell(cx, &added, &removed);
}

/// `selectionChanged(selected, deselected)`.
fn tell(cx: &Cx, added: &[Range], removed: &[Range]) {
    let model = model_of(cx);
    let ranges =
        |list: &[Range]| Value::list(list.iter().map(|r| range(cx.engine, model, r)).collect());
    let _ = ItemSelectionModel::emit_selection_changed(cx, ranges(added), ranges(removed));
}

/// Makes `place` the current item (none for `None`) and tells of it when
/// it changed.
fn make_current(cx: &Cx, place: Option<Place>) {
    let Some(state) = state_of(cx) else { return };
    let previous = {
        let mut s = state.borrow_mut();
        if s.current == place {
            return;
        }
        std::mem::replace(&mut s.current, place.clone())
    };
    tell_current(cx, &place, &previous);
}

/// Shows `place` as the current index and tells that it was `previous`.
fn tell_current(cx: &Cx, place: &Option<Place>, previous: &Option<Place>) {
    let model = model_of(cx);
    let index = |p: &Option<Place>| match p {
        Some(p) => make_index(cx.engine, model, p),
        None => invalid_index(cx.engine),
    };
    let (now, before) = (index(place), index(previous));
    let _ = cx
        .engine
        .set_internal(cx.this, ItemSelectionModel::CURRENT_INDEX, now.clone());
    let _ = ItemSelectionModel::emit_current_changed(cx, now.clone(), before.clone());
    let last = |p: &Option<Place>| p.as_ref().and_then(|p| p.last().copied());
    let (a, b) = (last(place), last(previous));
    if a.map(|x| x.0) != b.map(|x| x.0) {
        let _ = ItemSelectionModel::emit_current_row_changed(cx, now.clone(), before.clone());
    }
    if a.map(|x| x.1) != b.map(|x| x.1) {
        let _ = ItemSelectionModel::emit_current_column_changed(cx, now, before);
    }
}

/// Brings `hasSelection`, `selectedIndexes` and `selection` in step.
fn publish(cx: &Cx, state: &RefCell<Selection>) {
    let model = model_of(cx);
    let shown = state.borrow().shown.clone();
    let indexes: Vec<Value> = shown
        .iter()
        .flat_map(Range::places)
        .map(|p| make_index(cx.engine, model, &p))
        .collect();
    let ranges = shown.iter().map(|r| range(cx.engine, model, r)).collect();
    cx.engine.batch(|| {
        let me = cx.this;
        let _ = cx
            .engine
            .set_internal(me, ItemSelectionModel::HAS_SELECTION, !shown.is_empty());
        let _ = cx
            .engine
            .set_internal(me, ItemSelectionModel::SELECTED_INDEXES, indexes);
        let _ = cx
            .engine
            .set_internal(me, ItemSelectionModel::SELECTION, ranges);
    });
}

/// A row or a column of one level.
#[derive(Clone, Copy)]
enum Line {
    Row(i32),
    Column(i32),
}

/// The places of a row or column under `parent`, when the model says how
/// many there are.
fn line(cx: &Cx, parent: &Value, line: Line) -> Option<Vec<Place>> {
    let parent = place_of(cx.engine, parent, model_of(cx)).unwrap_or_default();
    let (rows, columns) = extent(cx, &parent)?;
    let at = |r, c| {
        let mut p = parent.clone();
        p.push((r, c));
        p
    };
    Some(match line {
        Line::Row(r) if (0..rows).contains(&r) => (0..columns).map(|c| at(r, c)).collect(),
        Line::Column(c) if (0..columns).contains(&c) => (0..rows).map(|r| at(r, c)).collect(),
        _ => Vec::new(),
    })
}

fn is_shown(s: &Selection, place: &[(i32, i32)]) -> bool {
    s.shown.iter().any(|r| r.contains(place))
}

fn selected_all(cx: &Cx, places: &[Place]) -> bool {
    let Some(state) = state_of(cx) else {
        return false;
    };
    let s = state.borrow();
    !places.is_empty() && places.iter().all(|p| is_shown(&s, p))
}

fn selected_any(cx: &Cx, places: &[Place]) -> bool {
    let Some(state) = state_of(cx) else {
        return false;
    };
    let s = state.borrow();
    places.iter().any(|p| is_shown(&s, p))
}

/// The whole rows (or columns) of the selection, as indexes at `at`.
fn whole_lines(cx: &Cx, rows: bool, at: i32) -> Value {
    let Some(state) = state_of(cx) else {
        return Value::list(Vec::new());
    };
    let model = model_of(cx);
    let selected: Vec<Place> = state
        .borrow()
        .shown
        .iter()
        .flat_map(Range::places)
        .collect();
    let mut seen: BTreeSet<Place> = BTreeSet::new();
    let mut out = Vec::new();
    for place in selected {
        let (last, parent) = place.split_last().expect("a place has a last step");
        let mut key = parent.to_vec();
        key.push(if rows { (last.0, at) } else { (at, last.1) });
        if !seen.insert(key.clone()) {
            continue;
        }
        let Some((nrows, ncolumns)) = extent(cx, parent) else {
            continue;
        };
        let at_cell = |r, c| {
            let mut p = parent.to_vec();
            p.push((r, c));
            p
        };
        let s = state.borrow();
        let whole = if rows {
            (0..ncolumns).all(|c| is_shown(&s, &at_cell(last.0, c)))
        } else {
            (0..nrows).all(|r| is_shown(&s, &at_cell(r, last.1)))
        };
        if whole {
            out.push(make_index(cx.engine, model, &key));
        }
    }
    Value::list(out)
}

// ---- indexes as scripts see them ---------------------------------------

/// The script functions that make indexes and ranges; one per engine.
#[derive(Default)]
struct Makers(RefCell<Option<(Value, Value, Value)>>);

const MAKERS: &str = r#"(function () {
  const invalid = { row: -1, column: -1, valid: false, model: null, internalId: 0,
    toString() { return "QModelIndex()" } };
  invalid.parent = invalid;
  Object.freeze(invalid);
  function index(model, place) {
    let at = invalid;
    for (const [row, column] of place) {
      at = Object.freeze({ row: row, column: column, parent: at, valid: true, model: model,
        internalId: 0, toString() { return "QModelIndex(" + this.row + "," + this.column + ")" } });
    }
    return at;
  }
  function same(a, b) {
    for (; a.valid && b.valid; a = a.parent, b = b.parent)
      if (a.row !== b.row || a.column !== b.column) return false;
    return a.valid === b.valid;
  }
  function range(model, parent, top, left, bottom, right) {
    const tl = index(model, parent.concat([[top, left]]));
    const br = index(model, parent.concat([[bottom, right]]));
    return Object.freeze({ top: top, left: left, bottom: bottom, right: right,
      width: right - left + 1, height: bottom - top + 1, topLeft: tl, bottomRight: br,
      parent: tl.parent, valid: true, empty: false, model: model,
      contains(i) {
        return !!i && !!i.valid && i.row >= top && i.row <= bottom && i.column >= left
          && i.column <= right && same(i.parent, tl.parent)
      },
      toString() { return "QItemSelectionRange(" + tl + "," + br + ")" } });
  }
  return [invalid, index, range];
})()"#;

fn makers(engine: &Engine) -> Option<(Value, Value, Value)> {
    let cache = engine.extension::<Makers>();
    if let Some(m) = cache.0.borrow().clone() {
        return Some(m);
    }
    let made = engine.eval_js(MAKERS).ok()?;
    let Value::List(parts) = made else {
        return None;
    };
    let [invalid, index, range] = parts.as_slice() else {
        return None;
    };
    let m = (invalid.clone(), index.clone(), range.clone());
    *cache.0.borrow_mut() = Some(m.clone());
    Some(m)
}

/// The index that names no item.
pub(crate) fn invalid_index(engine: &Engine) -> Value {
    makers(engine).map_or(Value::Undefined, |m| m.0)
}

fn place_value(place: &[(i32, i32)]) -> Value {
    Value::list(
        place
            .iter()
            .map(|(r, c)| Value::list(vec![Value::Int(*r), Value::Int(*c)]))
            .collect(),
    )
}

fn model_value(model: Option<ObjectId>) -> Value {
    model.map_or(Value::Null, Value::Object)
}

/// The index of the item at `place` in `model`.
pub(crate) fn make_index(engine: &Engine, model: Option<ObjectId>, place: &[(i32, i32)]) -> Value {
    let Some((_, index, _)) = makers(engine) else {
        return Value::Undefined;
    };
    engine
        .call_js(&index, &[model_value(model), place_value(place)])
        .unwrap_or(Value::Undefined)
}

fn range(engine: &Engine, model: Option<ObjectId>, r: &Range) -> Value {
    let Some((_, _, range)) = makers(engine) else {
        return Value::Undefined;
    };
    let args = [
        model_value(model),
        place_value(&r.parent),
        Value::Int(r.top),
        Value::Int(r.left),
        Value::Int(r.bottom),
        Value::Int(r.right),
    ];
    engine.call_js(&range, &args).unwrap_or(Value::Undefined)
}

/// `index(row, column)` of a model of one level: the index of the item,
/// or the invalid index when the model has no such item.
pub(crate) fn list_index(engine: &Engine, model: ObjectId, row: &Value, column: &Value) -> Value {
    let rows = engine.object_model(model).map_or(0, |m| m.row_count());
    let (row, column) = (row.to_number(), column.to_number());
    let inside = row >= 0.0 && (row as usize) < rows && column == 0.0;
    if inside {
        make_index(engine, Some(model), &[(row as i32, 0)])
    } else {
        invalid_index(engine)
    }
}
