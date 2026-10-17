//! The cells a tree view shows, as the model its delegates are made from.
//!
//! A row of the view is a row of the tree that is in sight: a row at the
//! root, or a row under a row that is expanded. The rows are in the
//! order of a walk of the tree, which is the order of their paths. Cell
//! `(row, column)` is row `row * columns + column` of this model, and has
//! the tree's roles and `row` and `column`.
//!
//! When the rows in sight change (a row is expanded or collapsed, the
//! tree gains or loses rows) the old rows and the new ones are merged by
//! path, and the rows that went and came are told as removals and
//! insertions: the delegates of the rows that stay are kept.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::{Rc, Weak};

use ljos_engine::{
    Atom, Engine, ItemModel, ItemTree, ModelChange, ModelListener, ModelListeners, TreeChange,
    Value,
};

/// A row of the view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Line {
    pub path: Vec<usize>,
    /// How far below the view's root the row is; 0 at the root.
    pub depth: usize,
    pub has_children: bool,
}

#[derive(Default)]
pub(crate) struct Cells {
    tree: RefCell<Option<Rc<dyn ItemTree>>>,
    subscription: Cell<u64>,
    lines: RefCell<Vec<Line>>,
    expanded: RefCell<BTreeSet<Vec<usize>>>,
    /// The path of the row whose rows are the view's top level.
    root: RefCell<Vec<usize>>,
    columns: Cell<usize>,
    roles: RefCell<Vec<Atom>>,
    listeners: ModelListeners,
}

/// A list model as a tree of one level, so a tree view of a list shows
/// its rows.
struct ListTree(Rc<dyn ItemModel>);

impl ItemTree for ListTree {
    fn row_count(&self, parent: &[usize]) -> usize {
        if parent.is_empty() {
            self.0.row_count()
        } else {
            0
        }
    }

    fn role_names(&self) -> Vec<Atom> {
        self.0.role_names()
    }

    fn data(&self, engine: &Engine, path: &[usize], column: usize, role: &str) -> Value {
        match (path, column) {
            ([row], 0) => self.0.data(engine, *row, role),
            _ => Value::Undefined,
        }
    }

    fn model_data(&self, engine: &Engine, path: &[usize], column: usize) -> Option<Value> {
        match (path, column) {
            ([row], 0) => self.0.model_data(engine, *row),
            _ => None,
        }
    }

    fn set_data(
        &self,
        engine: &Engine,
        path: &[usize],
        column: usize,
        role: &str,
        value: &Value,
    ) -> bool {
        match (path, column) {
            ([row], 0) => self.0.set_data(engine, *row, role, value),
            _ => false,
        }
    }

    fn can_fetch_more(&self, parent: &[usize]) -> bool {
        parent.is_empty() && self.0.can_fetch_more()
    }

    fn fetch_more(&self, engine: &Engine, parent: &[usize]) {
        if parent.is_empty() {
            self.0.fetch_more(engine);
        }
    }

    fn subscribe(&self, listener: ljos_engine::TreeListener) -> u64 {
        self.0
            .subscribe(Rc::new(move |e: &Engine, c: &ModelChange| {
                let change = match *c {
                    ModelChange::RowsInserted { first, count } => TreeChange::RowsInserted {
                        parent: Vec::new(),
                        first,
                        count,
                    },
                    ModelChange::RowsRemoved { first, count } => TreeChange::RowsRemoved {
                        parent: Vec::new(),
                        first,
                        count,
                    },
                    ModelChange::DataChanged {
                        first,
                        count,
                        ref roles,
                    } => {
                        for row in first..first + count {
                            let change = TreeChange::DataChanged {
                                path: vec![row],
                                roles: roles.clone(),
                            };
                            listener(e, &change);
                        }
                        return;
                    }
                    ModelChange::RowsAboutToBeInserted { .. }
                    | ModelChange::RowsAboutToBeRemoved { .. } => return,
                    ModelChange::RowsMoved { .. }
                    | ModelChange::RolesChanged
                    | ModelChange::ModelReset => TreeChange::ModelReset,
                };
                listener(e, &change);
            }))
    }

    fn unsubscribe(&self, id: u64) {
        self.0.unsubscribe(id);
    }
}

/// The tree `model` is, or the one level of a list.
pub(crate) fn tree_of(model: Rc<dyn ItemModel>) -> Rc<dyn ItemTree> {
    model.tree().unwrap_or_else(|| Rc::new(ListTree(model)))
}

/// Where `path` is after rows were inserted (`delta` > 0) or removed
/// under `parent` from `first`; `None` when it was removed.
fn shifted(
    path: &[usize],
    parent: &[usize],
    first: usize,
    count: usize,
    inserted: bool,
) -> Option<Vec<usize>> {
    let level = parent.len();
    if path.len() <= level || !path.starts_with(parent) || path[level] < first {
        return Some(path.to_vec());
    }
    let mut p = path.to_vec();
    if inserted {
        p[level] += count;
    } else if path[level] < first + count {
        return None;
    } else {
        p[level] -= count;
    }
    Some(p)
}

/// Rows removed or inserted next to each other, told as one change.
enum Run {
    None,
    Removed { first: usize, count: usize },
    Inserted { first: usize, count: usize },
}

impl Cells {
    pub(crate) fn tree(&self) -> Option<Rc<dyn ItemTree>> {
        self.tree.borrow().clone()
    }

    pub(crate) fn line(&self, row: usize) -> Option<Line> {
        self.lines.borrow().get(row).cloned()
    }

    pub(crate) fn line_count(&self) -> usize {
        self.lines.borrow().len()
    }

    pub(crate) fn columns(&self) -> usize {
        self.columns.get()
    }

    /// The row of the view that shows the row of the tree at `path`.
    pub(crate) fn row_of(&self, path: &[usize]) -> Option<usize> {
        self.lines
            .borrow()
            .binary_search_by(|l| l.path.as_slice().cmp(path))
            .ok()
    }

    pub(crate) fn root(&self) -> Vec<usize> {
        self.root.borrow().clone()
    }

    pub(crate) fn is_expanded(&self, path: &[usize]) -> bool {
        self.expanded.borrow().contains(path)
    }

    /// Shows `tree` from the row at `root`, with nothing expanded. The
    /// view makes its delegates again after this: nothing is told.
    pub(crate) fn set_tree(self: &Rc<Self>, tree: Option<Rc<dyn ItemTree>>, root: Vec<usize>) {
        if let Some(old) = self.tree.take() {
            old.unsubscribe(self.subscription.take());
        }
        self.expanded.borrow_mut().clear();
        *self.root.borrow_mut() = root;
        if let Some(tree) = &tree {
            let me: Weak<Cells> = Rc::downgrade(self);
            let id = tree.subscribe(Rc::new(move |e: &Engine, c: &TreeChange| {
                if let Some(me) = me.upgrade() {
                    me.tree_changed(e, c);
                }
            }));
            self.subscription.set(id);
        }
        *self.tree.borrow_mut() = tree;
        self.measure();
        *self.lines.borrow_mut() = self.walk();
    }

    /// Reads the columns and roles of the tree again.
    fn measure(&self) {
        let tree = self.tree();
        self.columns
            .set(tree.as_ref().map_or(0, |t| t.column_count()));
        let mut roles = tree.map(|t| t.role_names()).unwrap_or_default();
        for extra in ["row", "column"] {
            if !roles.iter().any(|r| &**r == extra) {
                roles.push(Atom::intern(extra));
            }
        }
        *self.roles.borrow_mut() = roles;
    }

    /// The rows in sight, from the tree and what is expanded.
    fn walk(&self) -> Vec<Line> {
        let mut out = Vec::new();
        let Some(tree) = self.tree() else {
            return out;
        };
        let expanded = self.expanded.borrow();
        let mut path = self.root.borrow().clone();
        let base = path.len();
        fn go(
            tree: &dyn ItemTree,
            expanded: &BTreeSet<Vec<usize>>,
            path: &mut Vec<usize>,
            base: usize,
            out: &mut Vec<Line>,
        ) {
            for r in 0..tree.row_count(path) {
                path.push(r);
                // The model's word, so a folder not read yet has its arrow.
                let has_children = tree.has_children(path);
                out.push(Line {
                    path: path.clone(),
                    depth: path.len() - base - 1,
                    has_children,
                });
                if has_children && expanded.contains(path.as_slice()) {
                    go(tree, expanded, path, base, out);
                }
                path.pop();
            }
        }
        go(&*tree, &expanded, &mut path, base, &mut out);
        out
    }

    /// Marks the row at `path` expanded or not; true when that changed.
    /// The rows in sight follow at [`Cells::update`].
    pub(crate) fn set_expanded(&self, path: &[usize], on: bool) -> bool {
        let mut expanded = self.expanded.borrow_mut();
        if on {
            expanded.insert(path.to_vec())
        } else {
            expanded.remove(path)
        }
    }

    /// Asks the model for the rows under `path` when it has not read them
    /// yet; they come as rows inserted, now or later.
    pub(crate) fn fetch_more(&self, engine: &Engine, path: &[usize]) {
        if let Some(tree) = self.tree()
            && tree.can_fetch_more(path)
        {
            tree.fetch_more(engine, path);
        }
    }

    /// Takes every row under `path` (all rows, for the root) out of the
    /// expanded ones.
    pub(crate) fn collapse_under(&self, path: &[usize]) {
        self.expanded
            .borrow_mut()
            .retain(|p| !(p.len() > path.len() && p.starts_with(path)));
    }

    /// The rows in sight again, told as the rows that went and came.
    pub(crate) fn update(&self, engine: &Engine) {
        let old: Vec<Option<Vec<usize>>> = self
            .lines
            .borrow()
            .iter()
            .map(|l| Some(l.path.clone()))
            .collect();
        self.merge(engine, old);
    }

    fn merge(&self, engine: &Engine, old: Vec<Option<Vec<usize>>>) {
        let new = self.walk();
        let cols = self.columns.get();
        let mut changes = Vec::new();
        let mut run = Run::None;
        let flush =
            |run: &mut Run, changes: &mut Vec<ModelChange>| match std::mem::replace(run, Run::None)
            {
                Run::None => {}
                Run::Removed { first, count } => changes.push(ModelChange::RowsRemoved {
                    first: first * cols,
                    count: count * cols,
                }),
                Run::Inserted { first, count } => changes.push(ModelChange::RowsInserted {
                    first: first * cols,
                    count: count * cols,
                }),
            };
        let (mut i, mut j, mut k) = (0, 0, 0);
        while i < old.len() || j < new.len() {
            let remove = match (old.get(i), new.get(j)) {
                (Some(None), _) | (Some(Some(_)), None) => true,
                (Some(Some(o)), Some(n)) if *o == n.path => {
                    flush(&mut run, &mut changes);
                    (i, j, k) = (i + 1, j + 1, k + 1);
                    continue;
                }
                (Some(Some(o)), Some(n)) => *o < n.path,
                (None, _) => false,
            };
            if remove {
                match &mut run {
                    Run::Removed { first, count } if *first == k => *count += 1,
                    _ => {
                        flush(&mut run, &mut changes);
                        run = Run::Removed { first: k, count: 1 };
                    }
                }
                i += 1;
            } else {
                match &mut run {
                    Run::Inserted { first, count } if *first + *count == k => *count += 1,
                    _ => {
                        flush(&mut run, &mut changes);
                        run = Run::Inserted { first: k, count: 1 };
                    }
                }
                (j, k) = (j + 1, k + 1);
            }
        }
        flush(&mut run, &mut changes);
        let rows = new.len();
        *self.lines.borrow_mut() = new;
        if cols == 0 || changes.is_empty() {
            return;
        }
        for c in &changes {
            self.listeners.emit(engine, c);
        }
        // Cells after a change are in another row now.
        self.listeners.emit(
            engine,
            &ModelChange::DataChanged {
                first: 0,
                count: rows * cols,
                roles: vec![Atom::intern("row")],
            },
        );
    }

    fn tree_changed(&self, engine: &Engine, change: &TreeChange) {
        match change {
            TreeChange::ModelReset => {
                self.expanded.borrow_mut().clear();
                let root_there = {
                    let root = self.root.borrow();
                    root.is_empty()
                        || self.tree().is_some_and(|t| {
                            let (last, parent) = root.split_last().expect("not empty");
                            *last < t.row_count(parent)
                        })
                };
                if !root_there {
                    self.root.borrow_mut().clear();
                }
                self.measure();
                *self.lines.borrow_mut() = self.walk();
                self.listeners.emit(engine, &ModelChange::ModelReset);
            }
            TreeChange::RowsInserted {
                parent,
                first,
                count,
            }
            | TreeChange::RowsRemoved {
                parent,
                first,
                count,
            } => {
                let inserted = matches!(change, TreeChange::RowsInserted { .. });
                let shift = |p: &[usize]| shifted(p, parent, *first, *count, inserted);
                let expanded: BTreeSet<Vec<usize>> = self
                    .expanded
                    .borrow()
                    .iter()
                    .filter_map(|p| shift(p))
                    .collect();
                *self.expanded.borrow_mut() = expanded;
                let root = self.root();
                *self.root.borrow_mut() = shift(&root).unwrap_or_default();
                let old: Vec<Option<Vec<usize>>> =
                    self.lines.borrow().iter().map(|l| shift(&l.path)).collect();
                self.merge(engine, old);
            }
            TreeChange::DataChanged { path, roles } => {
                let cols = self.columns.get();
                if let Some(row) = self.row_of(path) {
                    self.listeners.emit(
                        engine,
                        &ModelChange::DataChanged {
                            first: row * cols,
                            count: cols,
                            roles: roles.clone(),
                        },
                    );
                }
            }
        }
    }

    /// The row and column of cell `index`.
    fn cell(&self, index: usize) -> Option<(usize, usize)> {
        let cols = self.columns.get();
        (cols > 0).then(|| (index / cols, index % cols))
    }
}

impl ItemModel for Cells {
    fn row_count(&self) -> usize {
        self.lines.borrow().len() * self.columns.get()
    }

    // Reaching the end of a table asks the model for more top-level rows.
    fn can_fetch_more(&self) -> bool {
        self.tree().is_some_and(|t| t.can_fetch_more(&[]))
    }

    fn fetch_more(&self, engine: &Engine) {
        if let Some(t) = self.tree() {
            t.fetch_more(engine, &[]);
        }
    }

    fn role_names(&self) -> Vec<Atom> {
        self.roles.borrow().clone()
    }

    fn data(&self, engine: &Engine, index: usize, role: &str) -> Value {
        let Some((row, column)) = self.cell(index) else {
            return Value::Undefined;
        };
        match role {
            "row" => Value::Int(row as i32),
            "column" => Value::Int(column as i32),
            _ => {
                let Some(path) = self.lines.borrow().get(row).map(|l| l.path.clone()) else {
                    return Value::Undefined;
                };
                self.tree()
                    .map_or(Value::Undefined, |t| t.data(engine, &path, column, role))
            }
        }
    }

    fn set_data(&self, engine: &Engine, index: usize, role: &str, value: &Value) -> bool {
        let Some((row, column)) = self.cell(index) else {
            return false;
        };
        let Some(path) = self.lines.borrow().get(row).map(|l| l.path.clone()) else {
            return false;
        };
        self.tree()
            .is_some_and(|t| t.set_data(engine, &path, column, role, value))
    }

    fn model_data(&self, engine: &Engine, index: usize) -> Option<Value> {
        let tree = self.tree()?;
        let (row, column) = self.cell(index)?;
        let path = self.lines.borrow().get(row).map(|l| l.path.clone())?;
        if let Some(data) = tree.model_data(engine, &path, column) {
            return Some(data);
        }
        let roles = tree.role_names();
        let [only] = roles.as_slice() else {
            return None;
        };
        Some(self.data(engine, index, only))
    }

    fn subscribe(&self, listener: ModelListener) -> u64 {
        self.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.listeners.remove(id);
    }
}
