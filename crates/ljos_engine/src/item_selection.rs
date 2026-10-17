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
//! Not there: selections of ranges (`select(selection, command)`),
//! `Current`'s separate current selection (it selects as `Select`
//! does), and following the model: rows inserted into or removed from
//! the model leave the selection where it was until it is cleared.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::builtins::QtObject;
use crate::cx::Cx;
use crate::engine::Engine;
use crate::ids::ObjectId;
use crate::value::Value;
use crate::{QmlObject, qml_impl};

/// The keys of `ItemSelectionModel.SelectionFlag`.
mod flag {
    pub const CLEAR: i64 = 0x1;
    pub const SELECT: i64 = 0x2;
    pub const DESELECT: i64 = 0x4;
    pub const TOGGLE: i64 = 0x8;
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
        ("Current", 0x10),
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
    selected: BTreeSet<Place>,
    current: Option<Place>,
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
            .set_internal(cx.this, Self::CURRENT_INDEX, current)
    }

    #[qml(invokable)]
    fn select(cx: &Cx, index: Value, command: Value) {
        let place = place_in(cx, &index);
        apply(cx, place.as_ref(), flags(&command));
    }

    #[qml(invokable, name = "setCurrentIndex")]
    fn set_current_index(cx: &Cx, index: Value, command: Value) {
        let place = place_in(cx, &index);
        apply(cx, place.as_ref(), flags(&command));
        make_current(cx, place);
    }

    /// The selection and the current item go, and both are told.
    #[qml(invokable)]
    fn clear(cx: &Cx) {
        apply(cx, None, flag::CLEAR);
        make_current(cx, None);
    }

    /// The selection and the current item go, and nothing is told.
    #[qml(invokable)]
    fn reset(cx: &Cx) {
        let Some(state) = state_of(cx) else { return };
        {
            let mut s = state.borrow_mut();
            s.selected.clear();
            s.current = None;
        }
        publish(cx, &state);
        let _ = cx
            .engine
            .set_internal(cx.this, Self::CURRENT_INDEX, invalid_index(cx.engine));
    }

    #[qml(invokable, name = "clearSelection")]
    fn clear_selection(cx: &Cx) {
        apply(cx, None, flag::CLEAR);
    }

    #[qml(invokable, name = "clearCurrentIndex")]
    fn clear_current_index(cx: &Cx) {
        make_current(cx, None);
    }

    #[qml(invokable, name = "isSelected")]
    fn is_selected(cx: &Cx, index: Value) -> bool {
        let (Some(state), Some(place)) = (state_of(cx), place_in(cx, &index)) else {
            return false;
        };
        state.borrow().selected.contains(&place)
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

fn state_of(cx: &Cx) -> Option<Rc<RefCell<Selection>>> {
    let state = cx
        .engine
        .with::<ItemSelectionModel, _>(cx.this, |m| m.state.clone())
        .ok()?;
    // A selection belongs to the model it was made in.
    let model = model_of(cx);
    let mut s = state.borrow_mut();
    if s.model != model {
        *s = Selection {
            model,
            ..Selection::default()
        };
    }
    drop(s);
    Some(state)
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
    let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
    if let Some(tree) = model.tree() {
        // A tree's rows hang under the first column of their parent.
        let mut path = Vec::with_capacity(parent.len());
        for &(row, column) in parent {
            if column != 0 {
                return Some((0, 0));
            }
            path.push(usize::try_from(row).ok()?);
        }
        return Some((count(tree.row_count(&path)), count(tree.column_count())));
    }
    if !parent.is_empty() {
        return None;
    }
    Some((count(model.row_count()), 1))
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

/// Selects, deselects or toggles the item at `place` (or its whole row or
/// column) as `command` says, after clearing when it says `Clear`, and
/// tells what changed.
fn apply(cx: &Cx, place: Option<&Place>, command: i64) {
    let Some(state) = state_of(cx) else { return };
    let mut items: Vec<Place> = Vec::new();
    if let Some(place) = place {
        let (last, parent) = place.split_last().expect("a place has a last step");
        let (rows, columns) = extent(cx, parent).unwrap_or((last.0 + 1, last.1 + 1));
        let at = |r, c| {
            let mut p = parent.to_vec();
            p.push((r, c));
            p
        };
        if command & flag::ROWS != 0 {
            items.extend((0..columns).map(|c| at(last.0, c)));
        } else if command & flag::COLUMNS != 0 {
            items.extend((0..rows).map(|r| at(r, last.1)));
        } else {
            items.push(place.clone());
        }
    }
    let (mut added, mut removed) = (Vec::new(), Vec::new());
    {
        let mut s = state.borrow_mut();
        if command & flag::CLEAR != 0 {
            let keep: BTreeSet<&Place> = if command & flag::SELECT != 0 {
                items.iter().collect()
            } else {
                BTreeSet::new()
            };
            let gone: Vec<Place> = s
                .selected
                .iter()
                .filter(|p| !keep.contains(p))
                .cloned()
                .collect();
            for p in gone {
                s.selected.remove(&p);
                removed.push(p);
            }
        }
        for item in items {
            let has = s.selected.contains(&item);
            let select = if command & flag::SELECT != 0 {
                true
            } else if command & flag::DESELECT != 0 {
                false
            } else if command & flag::TOGGLE != 0 {
                !has
            } else {
                has
            };
            if select && !has {
                s.selected.insert(item.clone());
                added.push(item);
            } else if !select && has {
                s.selected.remove(&item);
                removed.push(item);
            }
        }
    }
    if added.is_empty() && removed.is_empty() {
        return;
    }
    publish(cx, &state);
    let model = model_of(cx);
    let ranges =
        |places: &[Place]| Value::list(places.iter().map(|p| range(cx.engine, model, p)).collect());
    let (added, removed) = (ranges(&added), ranges(&removed));
    let _ = ItemSelectionModel::emit_selection_changed(cx, added, removed);
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
    let model = model_of(cx);
    let index = |p: &Option<Place>| match p {
        Some(p) => make_index(cx.engine, model, p),
        None => invalid_index(cx.engine),
    };
    let (now, before) = (index(&place), index(&previous));
    let _ = cx
        .engine
        .set_internal(cx.this, ItemSelectionModel::CURRENT_INDEX, now.clone());
    let _ = ItemSelectionModel::emit_current_changed(cx, now.clone(), before.clone());
    let last = |p: &Option<Place>| p.as_ref().and_then(|p| p.last().copied());
    let (a, b) = (last(&place), last(&previous));
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
    let places: Vec<Place> = state.borrow().selected.iter().cloned().collect();
    let indexes: Vec<Value> = places
        .iter()
        .map(|p| make_index(cx.engine, model, p))
        .collect();
    let ranges = places.iter().map(|p| range(cx.engine, model, p)).collect();
    cx.engine.batch(|| {
        let me = cx.this;
        let _ = cx
            .engine
            .set_internal(me, ItemSelectionModel::HAS_SELECTION, !places.is_empty());
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

fn selected_all(cx: &Cx, places: &[Place]) -> bool {
    let Some(state) = state_of(cx) else {
        return false;
    };
    let s = state.borrow();
    !places.is_empty() && places.iter().all(|p| s.selected.contains(p))
}

fn selected_any(cx: &Cx, places: &[Place]) -> bool {
    let Some(state) = state_of(cx) else {
        return false;
    };
    let s = state.borrow();
    places.iter().any(|p| s.selected.contains(p))
}

/// The whole rows (or columns) of the selection, as indexes at `at`.
fn whole_lines(cx: &Cx, rows: bool, at: i32) -> Value {
    let Some(state) = state_of(cx) else {
        return Value::list(Vec::new());
    };
    let model = model_of(cx);
    let selected: Vec<Place> = state.borrow().selected.iter().cloned().collect();
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
        let whole = if rows {
            (0..ncolumns).all(|c| {
                let mut p = parent.to_vec();
                p.push((last.0, c));
                state.borrow().selected.contains(&p)
            })
        } else {
            (0..nrows).all(|r| {
                let mut p = parent.to_vec();
                p.push((r, last.1));
                state.borrow().selected.contains(&p)
            })
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
  function range(model, place) {
    const at = index(model, place);
    return Object.freeze({ top: at.row, left: at.column, bottom: at.row, right: at.column,
      width: 1, height: 1, topLeft: at, bottomRight: at, parent: at.parent, valid: true,
      empty: false, model: model,
      contains(i) { return !!i && i.valid && i.row === at.row && i.column === at.column },
      toString() { return "QItemSelectionRange(" + at + "," + at + ")" } });
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

fn range(engine: &Engine, model: Option<ObjectId>, place: &[(i32, i32)]) -> Value {
    let Some((_, _, range)) = makers(engine) else {
        return Value::Undefined;
    };
    engine
        .call_js(&range, &[model_value(model), place_value(place)])
        .unwrap_or(Value::Undefined)
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
