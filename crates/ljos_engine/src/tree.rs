//! Models of rows under rows, as a view of a tree (`TreeView`) reads
//! them: `TreeModel`, and tree models written in Rust
//! ([`Engine::set_object_tree`]).
//!
//! A row is named by its path: its row number under each of its
//! ancestors, the top level first. `[]` is the root, which is not a row.
//! A tree is also an [`ItemModel`] of its top level, so a `ListView`,
//! a `Repeater` or an `ItemSelectionModel` given one sees its first
//! level of rows.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::model::{ItemModel, ModelChange, ModelListener};
use crate::value::Value;

/// What a tree model tells its views, after it changed.
#[derive(Clone, Debug, PartialEq)]
pub enum TreeChange {
    /// `count` rows now start at `first` under the row at `parent`.
    RowsInserted {
        parent: Vec<usize>,
        first: usize,
        count: usize,
    },
    /// The `count` rows that started at `first` under the row at `parent`
    /// are gone, with every row under them.
    RowsRemoved {
        parent: Vec<usize>,
        first: usize,
        count: usize,
    },
    /// Roles of the row at `path` changed; no roles means any. The rows
    /// under it are the same.
    DataChanged { path: Vec<usize>, roles: Vec<Atom> },
    /// Everything may be different.
    ModelReset,
}

/// Receives a tree model's changes.
pub type TreeListener = Rc<dyn Fn(&Engine, &TreeChange)>;

/// Rows of named roles, in columns, with rows under them.
pub trait ItemTree {
    /// How many rows are under the row at `parent`; `&[]` for the top.
    fn row_count(&self, parent: &[usize]) -> usize;

    /// The columns every row has.
    fn column_count(&self) -> usize {
        1
    }

    /// The roles a row has, in a stable order.
    fn role_names(&self) -> Vec<Atom>;

    /// `undefined` for a row, column or role the model does not have.
    fn data(&self, engine: &Engine, path: &[usize], column: usize, role: &str) -> Value;

    /// What `modelData` is in the delegate of a cell; `None` for a
    /// model that does not say, whose cells have their one role as it
    /// when they have one.
    fn model_data(&self, engine: &Engine, path: &[usize], column: usize) -> Option<Value> {
        let _ = (engine, path, column);
        None
    }

    /// False: the model is read-only, or refused the value.
    fn set_data(
        &self,
        engine: &Engine,
        path: &[usize],
        column: usize,
        role: &str,
        value: &Value,
    ) -> bool {
        let _ = (engine, path, column, role, value);
        false
    }

    /// Calls `listener` after every change; the result is for
    /// [`ItemTree::unsubscribe`].
    fn subscribe(&self, listener: TreeListener) -> u64 {
        let _ = listener;
        0
    }

    fn unsubscribe(&self, id: u64) {
        let _ = id;
    }
}

/// The subscription list of a tree model that changes.
#[derive(Default)]
pub struct TreeListeners {
    next: Cell<u64>,
    all: RefCell<Vec<(u64, TreeListener)>>,
}

impl std::fmt::Debug for TreeListeners {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeListeners")
            .field("len", &self.all.borrow().len())
            .finish()
    }
}

impl TreeListeners {
    pub fn add(&self, listener: TreeListener) -> u64 {
        let id = self.next.get() + 1;
        self.next.set(id);
        self.all.borrow_mut().push((id, listener));
        id
    }

    pub fn remove(&self, id: u64) {
        self.all.borrow_mut().retain(|(i, _)| *i != id);
    }

    /// Tells every listener. A listener may subscribe or unsubscribe.
    pub fn emit(&self, engine: &Engine, change: &TreeChange) {
        let all: Vec<TreeListener> = self.all.borrow().iter().map(|(_, l)| l.clone()).collect();
        for l in all {
            l(engine, change);
        }
    }
}

/// The top level of a tree, as a list model; [`ItemModel::tree`] gives
/// the whole tree back.
struct TopLevel(Rc<dyn ItemTree>);

impl ItemModel for TopLevel {
    fn row_count(&self) -> usize {
        self.0.row_count(&[])
    }

    fn role_names(&self) -> Vec<Atom> {
        self.0.role_names()
    }

    fn data(&self, engine: &Engine, row: usize, role: &str) -> Value {
        self.0.data(engine, &[row], 0, role)
    }

    fn set_data(&self, engine: &Engine, row: usize, role: &str, value: &Value) -> bool {
        self.0.set_data(engine, &[row], 0, role, value)
    }

    fn model_data(&self, engine: &Engine, row: usize) -> Option<Value> {
        match self.0.role_names().as_slice() {
            [only] => Some(self.0.data(engine, &[row], 0, only)),
            _ => None,
        }
    }

    fn subscribe(&self, listener: ModelListener) -> u64 {
        self.0.subscribe(Rc::new(move |e: &Engine, c: &TreeChange| {
            let top = match c {
                TreeChange::RowsInserted {
                    parent,
                    first,
                    count,
                } if parent.is_empty() => ModelChange::RowsInserted {
                    first: *first,
                    count: *count,
                },
                TreeChange::RowsRemoved {
                    parent,
                    first,
                    count,
                } if parent.is_empty() => ModelChange::RowsRemoved {
                    first: *first,
                    count: *count,
                },
                TreeChange::DataChanged { path, roles } if path.len() == 1 => {
                    ModelChange::DataChanged {
                        first: path[0],
                        count: 1,
                        roles: roles.clone(),
                    }
                }
                TreeChange::ModelReset => ModelChange::ModelReset,
                // Below the top level: nothing a list of it sees.
                _ => return,
            };
            listener(e, &top);
        }))
    }

    fn unsubscribe(&self, id: u64) {
        self.0.unsubscribe(id);
    }

    fn tree(&self) -> Option<Rc<dyn ItemTree>> {
        Some(self.0.clone())
    }
}

/// `tree` as a model of its top level.
pub fn tree_model(tree: Rc<dyn ItemTree>) -> Rc<dyn ItemModel> {
    Rc::new(TopLevel(tree))
}

impl Engine {
    /// Makes `obj` the tree model `tree`: `model: obj` in a `TreeView`
    /// shows its rows and the rows under them, and in a list view its
    /// top level. For tree models written in Rust.
    pub fn set_object_tree(
        &self,
        obj: ObjectId,
        tree: Rc<dyn ItemTree>,
    ) -> Result<(), EngineError> {
        self.set_object_model(obj, Some(tree_model(tree)))
    }

    /// The tree model `obj` is, when it is one.
    pub fn object_tree(&self, obj: ObjectId) -> Option<Rc<dyn ItemTree>> {
        self.object_model(obj)?.tree()
    }

    /// The model index, as scripts see one, of the item at `place` in
    /// `model`: its row and column under each of its ancestors, the top
    /// level first. In a tree each ancestor is in column 0.
    pub fn model_index(&self, model: Option<ObjectId>, place: &[(i32, i32)]) -> Value {
        crate::item_selection::make_index(self, model, place)
    }

    /// The model index that names no item.
    pub fn invalid_model_index(&self) -> Value {
        crate::item_selection::invalid_index(self)
    }

    /// Where the model index `index` is, as [`Engine::model_index`]
    /// takes it; `None` for an index that is not valid or is of another
    /// model than `model`.
    pub fn model_index_place(
        &self,
        index: &Value,
        model: Option<ObjectId>,
    ) -> Option<Vec<(i32, i32)>> {
        crate::item_selection::place_of(self, index, model)
    }
}
