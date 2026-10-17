//! `TreeModel` and `TableModelColumn` (Qt.labs.qmlmodels): a tree of
//! rows written in QML as JavaScript objects, each holding the rows
//! under it in the key `rows`, read in the columns its
//! `TableModelColumn`s describe.
//!
//! The model keeps the rows as Rust values ([`Node`]) and is the
//! [`ItemTree`] a `TreeView` reads. `rows` is written back after every
//! change made through the model's methods, so a script reading it sees
//! the tree as it is.
//!
//! Not built: a role given as a function (Qt calls it with the model
//! index to get the value); such a role reads as `undefined`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::atom::Atom;
use crate::builtins::QtObject;
use crate::cx::Cx;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::list::QmlList;
use crate::model::ItemModel;
use crate::tree::{ItemTree, TreeChange, TreeListener, TreeListeners, tree_model};
use crate::value::Value;
use crate::{QmlObject, qml_impl};

/// The key of a row that holds the rows under it.
const CHILDREN: &str = "rows";

/// The roles a column can have, in the order a model lists them.
const ROLES: [&str; 14] = [
    "display",
    "decoration",
    "edit",
    "toolTip",
    "statusTip",
    "whatsThis",
    "font",
    "textAlignment",
    "background",
    "foreground",
    "checkState",
    "accessibleText",
    "accessibleDescription",
    "sizeHint",
];

/// `TableModelColumn`: what each role of one column reads. A role given
/// a string reads the key of the row by that name.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Qt.labs.qmlmodels", name = "TableModelColumn", extends = QtObject)]
pub struct TableModelColumn {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub display: Value,
    #[qml(notify)]
    pub decoration: Value,
    #[qml(notify)]
    pub edit: Value,
    #[qml(notify)]
    pub tool_tip: Value,
    #[qml(notify)]
    pub status_tip: Value,
    #[qml(notify)]
    pub whats_this: Value,
    #[qml(notify)]
    pub font: Value,
    #[qml(notify)]
    pub text_alignment: Value,
    #[qml(notify)]
    pub background: Value,
    #[qml(notify)]
    pub foreground: Value,
    #[qml(notify)]
    pub check_state: Value,
    #[qml(notify)]
    pub accessible_text: Value,
    #[qml(notify)]
    pub accessible_description: Value,
    #[qml(notify)]
    pub size_hint: Value,
}

/// `TreeModel`: rows of JavaScript objects with rows under them.
#[derive(QmlObject)]
#[qml(
    module = "Qt.labs.qmlmodels",
    name = "TreeModel",
    extends = QtObject,
    default = "columns"
)]
pub struct TreeModel {
    #[qml(base)]
    pub object: QtObject,
    #[qml(list, readonly)]
    pub columns: QmlList,
    #[qml(notify, readonly, rename = "columnCount")]
    pub column_count: i32,
    #[qml(notify)]
    pub rows: Value,
    #[qml(skip)]
    state: Rc<TreeState>,
    #[qml(skip)]
    top: Rc<dyn ItemModel>,
}

impl Default for TreeModel {
    fn default() -> Self {
        let state = Rc::new(TreeState::default());
        Self {
            object: QtObject::default(),
            columns: QmlList::default(),
            column_count: 0,
            rows: Value::Undefined,
            top: tree_model(state.clone()),
            state,
        }
    }
}

impl std::fmt::Debug for TreeModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeModel")
            .field("columns", &self.column_count)
            .field("rows", &self.state.nodes.borrow().len())
            .finish_non_exhaustive()
    }
}

/// A row: its keys but `rows`, and the rows under it.
#[derive(Clone, Debug, Default)]
struct Node {
    fields: Vec<(Atom, Value)>,
    children: Vec<Node>,
}

impl Node {
    /// The row a script gave, already a copy as data.
    fn parse(v: &Value) -> Node {
        let mut node = Node::default();
        for (key, value) in v.as_map().unwrap_or_default() {
            if &**key == CHILDREN {
                node.children = parse_rows(value);
            } else {
                node.fields.push((key.clone(), value.clone()));
            }
        }
        node
    }

    /// The row as a script sees it; `rows` only when there are some.
    fn to_value(&self) -> Value {
        let mut fields = self.fields.clone();
        if !self.children.is_empty() {
            fields.push((Atom::intern(CHILDREN), rows_value(&self.children)));
        }
        Value::map(fields)
    }

    fn field(&self, key: &str) -> Value {
        self.fields
            .iter()
            .find(|(k, _)| &**k == key)
            .map_or(Value::Undefined, |(_, v)| v.clone())
    }
}

fn parse_rows(v: &Value) -> Vec<Node> {
    v.as_list()
        .unwrap_or_default()
        .iter()
        .map(Node::parse)
        .collect()
}

fn rows_value(nodes: &[Node]) -> Value {
    Value::list(nodes.iter().map(Node::to_value).collect())
}

fn at<'a>(nodes: &'a [Node], path: &[usize]) -> Option<&'a Node> {
    let (first, rest) = path.split_first()?;
    let node = nodes.get(*first)?;
    if rest.is_empty() {
        Some(node)
    } else {
        at(&node.children, rest)
    }
}

fn at_mut<'a>(nodes: &'a mut [Node], path: &[usize]) -> Option<&'a mut Node> {
    let (first, rest) = path.split_first()?;
    let node = nodes.get_mut(*first)?;
    if rest.is_empty() {
        Some(node)
    } else {
        at_mut(&mut node.children, rest)
    }
}

/// The rows of a `TreeModel` and what its columns read.
#[derive(Default)]
struct TreeState {
    me: Cell<Option<ObjectId>>,
    nodes: RefCell<Vec<Node>>,
    /// For each column, the key of the row each role reads.
    columns: RefCell<Vec<Vec<(Atom, Atom)>>>,
    listeners: TreeListeners,
    /// Set while the model writes `rows` itself, which is not a new tree.
    writing: Cell<bool>,
}

impl TreeState {
    fn children_len(&self, path: &[usize]) -> Option<usize> {
        let nodes = self.nodes.borrow();
        if path.is_empty() {
            return Some(nodes.len());
        }
        at(&nodes, path).map(|n| n.children.len())
    }

    fn exists(&self, path: &[usize]) -> bool {
        !path.is_empty() && at(&self.nodes.borrow(), path).is_some()
    }

    /// The key the role `role` of `column` reads.
    fn key(&self, column: usize, role: &str) -> Option<Atom> {
        let columns = self.columns.borrow();
        columns
            .get(column)?
            .iter()
            .find(|(r, _)| &**r == role)
            .map(|(_, k)| k.clone())
    }

    fn warn(&self, engine: &Engine, what: &str) {
        let at = self
            .me
            .get()
            .and_then(|o| engine.location_of(o))
            .unwrap_or_default();
        engine.warn(format!("{at}QML TreeModel: {what}"));
    }

    /// Writes `rows` back, so scripts read the tree as it is now.
    fn publish(&self, engine: &Engine) {
        let Some(me) = self.me.get() else { return };
        let rows = rows_value(&self.nodes.borrow());
        self.writing.set(true);
        let _ = engine.set_internal(me, TreeModel::ROWS, rows);
        self.writing.set(false);
    }

    fn changed(&self, engine: &Engine, change: TreeChange) {
        self.publish(engine);
        self.listeners.emit(engine, &change);
    }

    /// Reads the columns' roles again; true when they changed.
    fn read_columns(&self, engine: &Engine, columns: &[ObjectId]) -> bool {
        let read: Vec<Vec<(Atom, Atom)>> = columns
            .iter()
            .map(|&c| {
                ROLES
                    .iter()
                    .filter_map(|role| {
                        let v = engine.read_by_name(c, role);
                        let Value::String(key) = v else { return None };
                        Some((Atom::intern(role), key))
                    })
                    .collect()
            })
            .collect();
        let mut now = self.columns.borrow_mut();
        let changed = *now != read;
        *now = read;
        changed
    }
}

impl ItemTree for TreeState {
    fn row_count(&self, parent: &[usize]) -> usize {
        self.children_len(parent).unwrap_or(0)
    }

    fn column_count(&self) -> usize {
        self.columns.borrow().len()
    }

    fn role_names(&self) -> Vec<Atom> {
        let columns = self.columns.borrow();
        ROLES
            .iter()
            .filter(|role| columns.iter().flatten().any(|(r, _)| &**r == **role))
            .map(|role| Atom::intern(role))
            .collect()
    }

    fn data(&self, _engine: &Engine, path: &[usize], column: usize, role: &str) -> Value {
        let Some(key) = self.key(column, role) else {
            return Value::Undefined;
        };
        at(&self.nodes.borrow(), path).map_or(Value::Undefined, |n| n.field(&key))
    }

    fn set_data(
        &self,
        engine: &Engine,
        path: &[usize],
        column: usize,
        role: &str,
        value: &Value,
    ) -> bool {
        let Some(key) = self.key(column, role) else {
            return false;
        };
        let value = engine.deep_copy(value);
        {
            let mut nodes = self.nodes.borrow_mut();
            let Some(node) = at_mut(&mut nodes, path) else {
                return false;
            };
            match node.fields.iter_mut().find(|(k, _)| *k == key) {
                Some((_, v)) => *v = value,
                None => node.fields.push((key, value)),
            }
        }
        self.changed(
            engine,
            TreeChange::DataChanged {
                path: path.to_vec(),
                roles: vec![Atom::intern(role)],
            },
        );
        true
    }

    fn subscribe(&self, listener: TreeListener) -> u64 {
        self.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.listeners.remove(id);
    }
}

fn state_of(engine: &Engine, obj: ObjectId) -> Option<Rc<TreeState>> {
    let state = engine
        .untrack(|| engine.with::<TreeModel, _>(obj, |m| m.state.clone()))
        .ok()?;
    state.me.set(Some(obj));
    Some(state)
}

impl Engine {
    /// The model the `TreeModel` `obj` is, asked by
    /// [`Engine::object_model`] so a view completed before the model
    /// already reads it.
    pub(crate) fn tree_model_state(&self, obj: ObjectId) -> Option<Rc<dyn ItemModel>> {
        self.untrack(|| self.with::<TreeModel, _>(obj, |m| m.top.clone()))
            .ok()
    }
}

/// The path of the row an index names, and its column; `None` for an
/// index that is not valid or not of `model`.
fn path_of(engine: &Engine, index: &Value, model: ObjectId) -> Option<(Vec<usize>, usize)> {
    let place = crate::item_selection::place_of(engine, index, Some(model))?;
    let mut path = Vec::with_capacity(place.len());
    for (i, &(row, column)) in place.iter().enumerate() {
        // Rows hang under the first column of their parent.
        if i + 1 < place.len() && column != 0 {
            return None;
        }
        path.push(usize::try_from(row).ok()?);
    }
    let column = usize::try_from(place.last()?.1).ok()?;
    Some((path, column))
}

fn index_value(engine: &Engine, model: ObjectId, path: &[usize], column: usize) -> Value {
    let last = path.len().saturating_sub(1);
    let place: Vec<(i32, i32)> = path
        .iter()
        .enumerate()
        .map(|(i, &r)| (r as i32, if i == last { column as i32 } else { 0 }))
        .collect();
    crate::item_selection::make_index(engine, Some(model), &place)
}

fn role_or_display(role: &Value) -> Atom {
    match role {
        Value::String(r) => r.clone(),
        _ => Atom::intern("display"),
    }
}

#[qml_impl]
impl TreeModel {
    /// The index of the row at `path` (an array of rows from the top) in
    /// `column`; or, as every item model has it, of row `path` under
    /// the row `parent` names.
    #[qml(invokable)]
    fn index(cx: &Cx, path: Value, column: Value, parent: Value) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let Some(state) = state_of(e, me) else {
            return crate::item_selection::invalid_index(e);
        };
        let column = column.to_number();
        let rows: Vec<f64> = match e.deep_copy(&path) {
            Value::List(items) => items.iter().map(Value::to_number).collect(),
            v => {
                let mut rows = match path_of(e, &parent, me) {
                    Some((p, 0)) => p.iter().map(|&r| r as f64).collect(),
                    Some(_) => return crate::item_selection::invalid_index(e),
                    None => Vec::new(),
                };
                rows.push(v.to_number());
                rows
            }
        };
        let path: Option<Vec<usize>> = rows
            .iter()
            .map(|&r| (r >= 0.0 && r.fract() == 0.0).then_some(r as usize))
            .collect();
        let inside = column >= 0.0 && (column as usize) < state.column_count();
        match path {
            Some(path) if inside && state.exists(&path) => {
                index_value(e, me, &path, column as usize)
            }
            _ => {
                state.warn(
                    e,
                    "TreeModel::index: could not find any node at the specified index",
                );
                crate::item_selection::invalid_index(e)
            }
        }
    }

    /// How many rows are under the row `parent` names, or at the top.
    #[qml(invokable, name = "rowCount")]
    fn row_count(cx: &Cx, parent: Value) -> i32 {
        let Some(state) = state_of(cx.engine, cx.this) else {
            return 0;
        };
        let path = match path_of(cx.engine, &parent, cx.this) {
            Some((path, 0)) => path,
            Some(_) => return 0,
            None => Vec::new(),
        };
        state.row_count(&path) as i32
    }

    #[qml(invokable, name = "hasChildren")]
    fn has_children(cx: &Cx, parent: Value) -> bool {
        Self::row_count(cx, parent) > 0
    }

    /// The index of the row the row `index` names is under.
    #[qml(invokable)]
    fn parent(cx: &Cx, index: Value) -> Value {
        match path_of(cx.engine, &index, cx.this) {
            Some((path, _)) if path.len() > 1 => {
                index_value(cx.engine, cx.this, &path[..path.len() - 1], 0)
            }
            _ => crate::item_selection::invalid_index(cx.engine),
        }
    }

    #[qml(invokable)]
    fn data(cx: &Cx, index: Value, role: Value) -> Value {
        let Some(state) = state_of(cx.engine, cx.this) else {
            return Value::Undefined;
        };
        match path_of(cx.engine, &index, cx.this) {
            Some((path, column)) => state.data(cx.engine, &path, column, &role_or_display(&role)),
            None => Value::Undefined,
        }
    }

    #[qml(invokable, name = "setData")]
    fn set_data(cx: &Cx, index: Value, value: Value, role: Value) -> bool {
        let Some(state) = state_of(cx.engine, cx.this) else {
            return false;
        };
        match path_of(cx.engine, &index, cx.this) {
            Some((path, column)) => {
                state.set_data(cx.engine, &path, column, &role_or_display(&role), &value)
            }
            None => false,
        }
    }

    /// The row `index` names, with the rows under it.
    #[qml(invokable, name = "getRow")]
    fn get_row(cx: &Cx, index: Value) -> Value {
        let Some(state) = state_of(cx.engine, cx.this) else {
            return Value::Undefined;
        };
        let Some((path, _)) = path_of(cx.engine, &index, cx.this) else {
            return Value::Undefined;
        };
        at(&state.nodes.borrow(), &path).map_or(Value::Undefined, Node::to_value)
    }

    /// Puts `row` in the place of the row `index` names.
    #[qml(invokable, name = "setRow")]
    fn set_row(cx: &Cx, index: Value, row: Value) {
        let (e, me) = (cx.engine, cx.this);
        let Some(state) = state_of(e, me) else { return };
        let Some((path, _)) = path_of(e, &index, me) else {
            state.warn(
                e,
                "TreeModel::setRow: could not find any node at the specified index",
            );
            return;
        };
        let row = Node::parse(&e.deep_copy(&row));
        let shape_changed = {
            let mut nodes = state.nodes.borrow_mut();
            let Some(node) = at_mut(&mut nodes, &path) else {
                return;
            };
            let had = node.children.len();
            let now = row.children.len();
            *node = row;
            had != 0 || now != 0
        };
        let change = if shape_changed {
            TreeChange::ModelReset
        } else {
            TreeChange::DataChanged {
                path,
                roles: Vec::new(),
            }
        };
        state.changed(e, change);
    }

    /// Adds `row` after the rows under the row `parent` names; given
    /// only a row, at the top.
    #[qml(invokable, name = "appendRow")]
    fn append_row(cx: &Cx, parent: Value, row: Value) {
        let (e, me) = (cx.engine, cx.this);
        let Some(state) = state_of(e, me) else { return };
        let (path, row) = match (path_of(e, &parent, me), row) {
            (Some((path, 0)), row) if !matches!(row, Value::Undefined) => (path, row),
            (Some(_), _) => {
                state.warn(
                    e,
                    "TreeModel::appendRow(): could not find any node at the specified index",
                );
                return;
            }
            (None, row) => {
                state.warn(
                    e,
                    "TreeModel::appendRow(): could not find any node at the specified index - the new row will be appended to root",
                );
                let row = if matches!(row, Value::Undefined) {
                    parent
                } else {
                    row
                };
                (Vec::new(), row)
            }
        };
        let row = Node::parse(&e.deep_copy(&row));
        let first = {
            let mut nodes = state.nodes.borrow_mut();
            let list = if path.is_empty() {
                &mut *nodes
            } else {
                match at_mut(&mut nodes, &path) {
                    Some(n) => &mut n.children,
                    None => return,
                }
            };
            list.push(row);
            list.len() - 1
        };
        state.changed(
            e,
            TreeChange::RowsInserted {
                parent: path,
                first,
                count: 1,
            },
        );
    }

    /// Takes out the row `index` names, with the rows under it.
    #[qml(invokable, name = "removeRow")]
    fn remove_row(cx: &Cx, index: Value) {
        let (e, me) = (cx.engine, cx.this);
        let Some(state) = state_of(e, me) else { return };
        let Some((path, _)) = path_of(e, &index, me) else {
            state.warn(
                e,
                "TreeModel::removeRow: could not find any node at the specified index",
            );
            return;
        };
        let (row, parent) = path.split_last().expect("a valid index has a row");
        {
            let mut nodes = state.nodes.borrow_mut();
            let list = if parent.is_empty() {
                &mut *nodes
            } else {
                match at_mut(&mut nodes, parent) {
                    Some(n) => &mut n.children,
                    None => return,
                }
            };
            if *row >= list.len() {
                return;
            }
            list.remove(*row);
        }
        state.changed(
            e,
            TreeChange::RowsRemoved {
                parent: parent.to_vec(),
                first: *row,
                count: 1,
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
        let rows = e.untrack(|| e.get(me, TreeModel::ROWS)).unwrap_or_default();
        *state.nodes.borrow_mut() = parse_rows(&e.deep_copy(&rows));
        let given = state.clone();
        e.watch(me, TreeModel::ROWS.index, move |e, v| {
            if given.writing.get() {
                return;
            }
            *given.nodes.borrow_mut() = parse_rows(&e.deep_copy(v));
            given.listeners.emit(e, &TreeChange::ModelReset);
        })?;
        let shown = state.clone();
        let first = Cell::new(true);
        e.effect_in(me, move |e| {
            let _ = e.get(me, TreeModel::COLUMNS);
            let columns = e
                .untrack(|| e.with::<TreeModel, _>(me, |m| m.columns.objects()))
                .unwrap_or_default();
            // Tracked: a role given later is a change of the columns.
            let changed = shown.read_columns(e, &columns);
            e.untrack(|| {
                let count = columns.len() as i32;
                if e.get(me, TreeModel::COLUMN_COUNT).ok() != Some(count) {
                    let _ = e.set_internal(me, TreeModel::COLUMN_COUNT, count);
                }
                if changed || first.replace(false) {
                    shown.listeners.emit(e, &TreeChange::ModelReset);
                }
            });
        })?;
        Ok(())
    }
}
