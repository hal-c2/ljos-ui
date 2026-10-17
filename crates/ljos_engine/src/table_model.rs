//! `TableModel` (Qt.labs.qmlmodels): rows of JavaScript objects, read
//! in the columns its `TableModelColumn`s describe, as a `TableView`
//! shows them.
//!
//! The rows are kept as a `TreeModel` keeps its own, with none under
//! them, and `rows` and `rowCount` are written back after every change
//! made through the model's methods. A row must have the keys its
//! columns read; one that lacks some is refused with qml6's warning.
//!
//! Not built: a role given as a function, and rows that are arrays of
//! cells (both need a role that is a function to be read); such a role
//! reads as `undefined`.

use std::cell::Cell;
use std::rc::Rc;

use crate::builtins::QtObject;
use crate::cx::Cx;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::list::QmlList;
use crate::model::ItemModel;
use crate::tree::{ItemTree, TreeChange, tree_model};
use crate::tree_model::{Node, TreeState, parse_rows, role_or_display};
use crate::value::Value;
use crate::{QmlObject, qml_impl};

/// `TableModel`: rows of JavaScript objects in columns.
#[derive(QmlObject)]
#[qml(
    module = "Qt.labs.qmlmodels",
    name = "TableModel",
    extends = QtObject,
    default = "columns"
)]
pub struct TableModel {
    #[qml(base)]
    pub object: QtObject,
    #[qml(list, readonly)]
    pub columns: QmlList,
    #[qml(notify, readonly, rename = "columnCount")]
    pub column_count: i32,
    #[qml(notify, readonly, rename = "rowCount")]
    pub row_count: i32,
    #[qml(notify)]
    pub rows: Value,
    #[qml(skip)]
    state: Rc<TreeState>,
    #[qml(skip)]
    top: Rc<dyn ItemModel>,
}

impl Default for TableModel {
    fn default() -> Self {
        let state = Rc::new(TreeState {
            name: "TableModel",
            rows_prop: TableModel::ROWS.index,
            count_prop: Some(TableModel::ROW_COUNT.index),
            ..TreeState::default()
        });
        Self {
            object: QtObject::default(),
            columns: QmlList::default(),
            column_count: 0,
            row_count: 0,
            rows: Value::Undefined,
            top: tree_model(state.clone()),
            state,
        }
    }
}

impl std::fmt::Debug for TableModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableModel")
            .field("columns", &self.column_count)
            .field("rows", &self.state.nodes.borrow().len())
            .finish_non_exhaustive()
    }
}

fn state_of(engine: &Engine, obj: ObjectId) -> Option<Rc<TreeState>> {
    let state = engine
        .untrack(|| engine.with::<TableModel, _>(obj, |m| m.state.clone()))
        .ok()?;
    state.me.set(Some(obj));
    Some(state)
}

impl Engine {
    /// The model the `TableModel` `obj` is, asked by
    /// [`Engine::object_model`] so a view completed before the model
    /// already reads it.
    pub(crate) fn table_model_state(&self, obj: ObjectId) -> Option<Rc<dyn ItemModel>> {
        self.untrack(|| self.with::<TableModel, _>(obj, |m| m.top.clone()))
            .ok()
    }
}

/// The row an index of `model` names, and its column.
fn cell_of(engine: &Engine, index: &Value, model: ObjectId) -> Option<(usize, usize)> {
    match crate::item_selection::place_of(engine, index, Some(model))?.as_slice() {
        [(row, column)] => Some((usize::try_from(*row).ok()?, usize::try_from(*column).ok()?)),
        _ => None,
    }
}

/// A row index a script gave: `None` for a negative one, warned of.
fn row_index(state: &TreeState, e: &Engine, method: &str, name: &str, v: &Value) -> Option<usize> {
    let n = v.to_number();
    if n < 0.0 {
        state.warn(e, &format!("{method}(): \"{name}\" cannot be negative"));
        return None;
    }
    Some(n as usize)
}

/// The name Qt gives a value's type in its warnings.
fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Int(_) => "int",
        // Scripts have one kind of number; Qt names a whole one an int.
        Value::Real(n) if n.fract() == 0.0 && n.abs() <= f64::from(i32::MAX) => "int",
        Value::Real(_) => "double",
        Value::Bool(_) => "bool",
        Value::String(_) => "QString",
        Value::Null => "std::nullptr_t",
        _ => "undefined",
    }
}

/// The row a script gave, when it has a key for every role its columns
/// read; otherwise warned of.
fn checked_row(state: &TreeState, e: &Engine, method: &str, row: &Value) -> Option<Node> {
    let row = e.deep_copy(row);
    let Some(fields) = row.as_map() else {
        state.warn(
            e,
            &format!(
                "{method}(): expected \"row\" argument to be a QJSValue, but got {} instead:",
                type_name(&row)
            ),
        );
        return None;
    };
    let columns = state.column_count();
    if fields.len() < columns {
        state.warn(
            e,
            &format!(
                "{method}(): expected {columns} columns, but only got {}",
                fields.len()
            ),
        );
        return None;
    }
    let keys = state.columns.borrow();
    let missing = keys
        .iter()
        .flatten()
        .map(|(_, key)| key)
        .find(|key| !fields.iter().any(|(k, _)| k == *key));
    if let Some(key) = missing {
        state.warn(
            e,
            &format!("{method}(): expected a property named \"{key}\" in row"),
        );
        return None;
    }
    Some(Node::parse(&row))
}

#[qml_impl]
impl TableModel {
    /// The index of the cell at `row` and `column`; the invalid index for
    /// one the model does not have.
    #[qml(invokable)]
    fn index(cx: &Cx, row: Value, column: Value) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let Some(state) = state_of(e, me) else {
            return crate::item_selection::invalid_index(e);
        };
        let (r, c) = (row.to_number(), column.to_number());
        let inside = r >= 0.0
            && c >= 0.0
            && (r as usize) < state.row_count(&[])
            && (c as usize) < state.column_count();
        if !inside {
            return crate::item_selection::invalid_index(e);
        }
        crate::item_selection::make_index(e, Some(me), &[(r as i32, c as i32)])
    }

    #[qml(invokable)]
    fn data(cx: &Cx, index: Value, role: Value) -> Value {
        let Some(state) = state_of(cx.engine, cx.this) else {
            return Value::Undefined;
        };
        match cell_of(cx.engine, &index, cx.this) {
            Some((row, column)) => state.data(cx.engine, &[row], column, &role_or_display(&role)),
            None => Value::Undefined,
        }
    }

    /// Writes the key the role of the cell's column reads; false for a
    /// role the column does not have.
    #[qml(invokable, name = "setData")]
    fn set_data(cx: &Cx, index: Value, value: Value, role: Value) -> bool {
        let Some(state) = state_of(cx.engine, cx.this) else {
            return false;
        };
        let Some((row, column)) = cell_of(cx.engine, &index, cx.this) else {
            return false;
        };
        let role = role_or_display(&role);
        let roles: Vec<String> = state
            .columns
            .borrow()
            .get(column)
            .into_iter()
            .flatten()
            .map(|(r, _)| format!("\"{r}\""))
            .collect();
        if !roles.is_empty()
            && !state.columns.borrow()[column]
                .iter()
                .any(|(r, _)| *r == role)
        {
            state.warn(
                cx.engine,
                &format!(
                    "setData(): no role named \"{role}\" at column index {column}. The available roles for that column are: QList({})",
                    roles.join(", ")
                ),
            );
            return false;
        }
        state.set_data(cx.engine, &[row], column, &role, &value)
    }

    /// A copy of the row at `rowIndex`.
    #[qml(invokable, name = "getRow")]
    fn get_row(cx: &Cx, row_index: Value) -> Value {
        let e = cx.engine;
        let Some(state) = state_of(e, cx.this) else {
            return Value::Undefined;
        };
        let Some(i) = row_index_of(&state, e, "getRow", &row_index) else {
            return Value::Undefined;
        };
        let n = state.nodes.borrow().len();
        if i >= n {
            state.warn(
                e,
                &format!(
                    "getRow(): \"rowIndex\" {i} is greater than or equal to rowCount() of {n}"
                ),
            );
            return Value::Undefined;
        }
        state.nodes.borrow()[i].to_value()
    }

    /// Puts `row` in the place of the row at `rowIndex`, or after the
    /// last when `rowIndex` is the number of rows.
    #[qml(invokable, name = "setRow")]
    fn set_row(cx: &Cx, row_index: Value, row: Value) {
        let e = cx.engine;
        let Some(state) = state_of(e, cx.this) else {
            return;
        };
        let Some(i) = row_index_of(&state, e, "setRow", &row_index) else {
            return;
        };
        let n = state.nodes.borrow().len();
        if i > n {
            state.warn(
                e,
                &format!("setRow(): \"rowIndex\" {i} is greater than rowCount() of {n}"),
            );
            return;
        }
        let Some(node) = checked_row(&state, e, "setRow", &row) else {
            return;
        };
        if i == n {
            insert(&state, e, n, node);
            return;
        }
        state.nodes.borrow_mut()[i] = node;
        state.changed(
            e,
            TreeChange::DataChanged {
                path: vec![i],
                roles: Vec::new(),
            },
        );
    }

    #[qml(invokable, name = "appendRow")]
    fn append_row(cx: &Cx, row: Value) {
        let e = cx.engine;
        let Some(state) = state_of(e, cx.this) else {
            return;
        };
        if let Some(node) = checked_row(&state, e, "appendRow", &row) {
            let n = state.nodes.borrow().len();
            insert(&state, e, n, node);
        }
    }

    /// Puts `row` before the row at `rowIndex`, or after the last.
    #[qml(invokable, name = "insertRow")]
    fn insert_row(cx: &Cx, row_index: Value, row: Value) {
        let e = cx.engine;
        let Some(state) = state_of(e, cx.this) else {
            return;
        };
        let Some(i) = row_index_of(&state, e, "insertRow", &row_index) else {
            return;
        };
        let n = state.nodes.borrow().len();
        if i > n {
            state.warn(
                e,
                &format!("insertRow(): \"rowIndex\" {i} is greater than rowCount() of {n}"),
            );
            return;
        }
        if let Some(node) = checked_row(&state, e, "insertRow", &row) {
            insert(&state, e, i, node);
        }
    }

    /// Takes out `rows` rows (1 when not given) from `rowIndex`.
    #[qml(invokable, name = "removeRow")]
    fn remove_row(cx: &Cx, row_index: Value, rows: Value) {
        let e = cx.engine;
        let Some(state) = state_of(e, cx.this) else {
            return;
        };
        let Some(i) = row_index_of(&state, e, "removeRow", &row_index) else {
            return;
        };
        let count = if matches!(rows, Value::Undefined) {
            1.0
        } else {
            rows.to_number()
        };
        let n = state.nodes.borrow().len();
        if count <= 0.0 {
            state.warn(e, "removeRow(): \"rows\" is less than or equal to zero");
            return;
        }
        let count = count as usize;
        if i >= n {
            state.warn(
                e,
                &format!(
                    "removeRow(): \"rowIndex\" {i} is greater than or equal to rowCount() of {n}"
                ),
            );
            return;
        }
        if i + count > n {
            state.warn(
                e,
                &format!(
                    "removeRow(): \"rows\" {count} exceeds available rowCount() of {n} when removing from \"rowIndex\" {i}"
                ),
            );
            return;
        }
        state.nodes.borrow_mut().drain(i..i + count);
        state.changed(
            e,
            TreeChange::RowsRemoved {
                parent: Vec::new(),
                first: i,
                count,
            },
        );
    }

    /// Moves `rows` rows (1 when not given) from `fromRowIndex` so the
    /// first of them is at `toRowIndex`.
    #[qml(invokable, name = "moveRow")]
    fn move_row(cx: &Cx, from: Value, to: Value, rows: Value) {
        let e = cx.engine;
        let Some(state) = state_of(e, cx.this) else {
            return;
        };
        let Some(from) = row_index(&state, e, "moveRow", "fromRowIndex", &from) else {
            return;
        };
        let Some(to) = row_index(&state, e, "moveRow", "toRowIndex", &to) else {
            return;
        };
        let rows = if matches!(rows, Value::Undefined) {
            1.0
        } else {
            rows.to_number()
        };
        let n = state.nodes.borrow().len();
        let refused = if from >= n {
            Some(format!(
                "\"fromRowIndex\" {from} is greater than or equal to rowCount() of {n}"
            ))
        } else if to >= n {
            Some(format!(
                "\"toRowIndex\" {to} is greater than or equal to rowCount() of {n}"
            ))
        } else if rows <= 0.0 {
            Some("\"rows\" is less than or equal to 0".to_owned())
        } else if from + rows as usize > n {
            Some(format!(
                "\"fromRowIndex\" ({from}) + \"rows\" ({}) = {}, which is greater than rowCount() of {n}",
                rows as usize,
                from + rows as usize
            ))
        } else if to + rows as usize > n {
            Some(format!(
                "\"toRowIndex\" ({to}) + \"rows\" ({}) = {}, which is greater than rowCount() of {n}",
                rows as usize,
                to + rows as usize
            ))
        } else if from == to {
            Some("\"fromRowIndex\" cannot be equal to \"toRowIndex\"".to_owned())
        } else {
            None
        };
        if let Some(what) = refused {
            state.warn(e, &format!("moveRow(): {what}"));
            return;
        }
        let count = rows as usize;
        {
            let mut nodes = state.nodes.borrow_mut();
            let moved: Vec<Node> = nodes.drain(from..from + count).collect();
            nodes.splice(to..to, moved);
        }
        // Views have no move: the rows go and come back where they are.
        state.publish(e);
        state.listeners.emit(
            e,
            &TreeChange::RowsRemoved {
                parent: Vec::new(),
                first: from,
                count,
            },
        );
        state.listeners.emit(
            e,
            &TreeChange::RowsInserted {
                parent: Vec::new(),
                first: to,
                count,
            },
        );
    }

    /// Takes out every row; the columns stay.
    #[qml(invokable)]
    fn clear(cx: &Cx) {
        let Some(state) = state_of(cx.engine, cx.this) else {
            return;
        };
        state.nodes.borrow_mut().clear();
        state.changed(cx.engine, TreeChange::ModelReset);
    }

    /// From completion on the object is a model: of `rows` as they are
    /// given, in the columns as they are.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let Some(state) = state_of(e, me) else {
            return Ok(());
        };
        let rows = e
            .untrack(|| e.get(me, TableModel::ROWS))
            .unwrap_or_default();
        *state.nodes.borrow_mut() = parse_rows(&e.deep_copy(&rows));
        state.count(e);
        let given = state.clone();
        e.watch(me, TableModel::ROWS.index, move |e, v| {
            if given.writing.get() {
                return;
            }
            *given.nodes.borrow_mut() = parse_rows(&e.deep_copy(v));
            given.count(e);
            given.listeners.emit(e, &TreeChange::ModelReset);
        })?;
        let shown = state.clone();
        let first = Cell::new(true);
        e.effect_in(me, move |e| {
            let _ = e.get(me, TableModel::COLUMNS);
            let columns = e
                .untrack(|| e.with::<TableModel, _>(me, |m| m.columns.objects()))
                .unwrap_or_default();
            // Tracked: a role given later is a change of the columns.
            let changed = shown.read_columns(e, &columns);
            e.untrack(|| {
                let count = columns.len() as i32;
                if e.get(me, TableModel::COLUMN_COUNT).ok() != Some(count) {
                    let _ = e.set_internal(me, TableModel::COLUMN_COUNT, count);
                }
                if changed || first.replace(false) {
                    shown.listeners.emit(e, &TreeChange::ModelReset);
                }
            });
        })?;
        Ok(())
    }
}

fn row_index_of(state: &TreeState, e: &Engine, method: &str, v: &Value) -> Option<usize> {
    row_index(state, e, method, "rowIndex", v)
}

/// Puts `node` at `at` and tells the views.
fn insert(state: &TreeState, e: &Engine, at: usize, node: Node) {
    state.nodes.borrow_mut().insert(at, node);
    state.changed(
        e,
        TreeChange::RowsInserted {
            parent: Vec::new(),
            first: at,
            count: 1,
        },
    );
}
