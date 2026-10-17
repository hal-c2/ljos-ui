//! `TableView` and `TreeView`: a Flickable whose content is cells in
//! rows and columns, with a delegate only for the rows in sight
//! (ARCHITECTURE.md §8.4).
//!
//! The view's rows are the rows of a tree model that are in sight
//! ([`cells`]); a list model is a tree of one level. The engine's
//! [`ViewDelegates`] keeps the delegates of cells, cell `(row, column)`
//! being row `row * columns + column` of the model [`cells::Cells`] is.
//! The layout pass ([`layout`]) says which rows have delegates and where
//! each cell goes; a `TreeView` also tells each delegate where its row is
//! in the tree (`depth`, `expanded`, `hasChildren`, `isTreeNode`).

mod cells;
mod edit;
mod layout;
mod nav;
mod select;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, ItemSelectionModel, LogLevel, MetaRef, ModelChange, NodeId, ObjectId,
    OverrideId, QmlObject, QmlType, Value, Version, ViewDelegates, ViewHost, WriteKind, qml_impl,
};

use super::{models, stack, warn};
use crate::enums::EnumTable;
use crate::flickable::{self, Flickable};
use crate::item::{Item, is_a, is_item, write_computed};
use crate::tree;

pub(crate) use cells::Cells;
pub use select::{CellBlock, CellSelector, cell_block_rect, select_by, set_cell_selector};

/// `TableView.EditTrigger`.
pub mod edit_trigger {
    pub const NO_EDIT_TRIGGERS: i32 = 0;
    pub const SINGLE_TAPPED: i32 = 1;
    pub const DOUBLE_TAPPED: i32 = 2;
    pub const SELECTED_TAPPED: i32 = 4;
    pub const EDIT_KEY_PRESSED: i32 = 8;
    pub const ANY_KEY_PRESSED: i32 = 16;
}

/// `TableView.SelectionBehavior`.
pub mod selection_behavior {
    pub const SELECTION_DISABLED: i32 = 0;
    pub const SELECT_CELLS: i32 = 1;
    pub const SELECT_ROWS: i32 = 2;
    pub const SELECT_COLUMNS: i32 = 3;
}

/// `TableView.SelectionMode`.
pub mod selection_mode {
    pub const SINGLE: i32 = 0;
    pub const CONTIGUOUS: i32 = 1;
    pub const EXTENDED: i32 = 2;
}

fn table_view_enums() -> EnumTable {
    use edit_trigger as t;
    use selection_behavior as b;
    use selection_mode as m;
    let triggers = super::keys(&[
        ("NoEditTriggers", t::NO_EDIT_TRIGGERS),
        ("SingleTapped", t::SINGLE_TAPPED),
        ("DoubleTapped", t::DOUBLE_TAPPED),
        ("SelectedTapped", t::SELECTED_TAPPED),
        ("EditKeyPressed", t::EDIT_KEY_PRESSED),
        ("AnyKeyPressed", t::ANY_KEY_PRESSED),
    ]);
    vec![
        (
            "SelectionBehavior",
            super::keys(&[
                ("SelectionDisabled", b::SELECTION_DISABLED),
                ("SelectCells", b::SELECT_CELLS),
                ("SelectRows", b::SELECT_ROWS),
                ("SelectColumns", b::SELECT_COLUMNS),
            ]),
        ),
        (
            "SelectionMode",
            super::keys(&[
                ("SingleSelection", m::SINGLE),
                ("ContiguousSelection", m::CONTIGUOUS),
                ("ExtendedSelection", m::EXTENDED),
            ]),
        ),
        ("EditTrigger", triggers.clone()),
        ("EditTriggers", triggers),
    ]
}

/// What a table view keeps besides its properties.
#[derive(Default)]
pub(crate) struct TableState {
    pub cells: Rc<Cells>,
    pub delegates: RefCell<Option<ViewDelegates>>,
    /// Bumped to make the layout effect run again.
    pub trigger: Cell<Option<NodeId>>,
    /// The model object, whose model indexes the view's are.
    pub model: Cell<Option<ObjectId>>,
    /// Whether the view is a `TreeView`, which tells delegates their row.
    pub tree: Cell<bool>,
    /// Delegates in the pool, hidden, and the override that hides each.
    pub hidden: RefCell<HashMap<ObjectId, OverrideId>>,
    /// How tall each row is, as far as it was measured; `NaN` not yet.
    pub heights: RefCell<Vec<f64>>,
    /// How wide each column's delegates are, as far as they were
    /// measured; `NaN` not yet.
    pub widths: RefCell<Vec<f64>>,
    /// Where each row starts and how wide each column is, as laid out.
    pub row_y: RefCell<Vec<f64>>,
    pub column_x: RefCell<Vec<(f64, f64)>>,
    /// The cell the Shift moves of the keys began on.
    pub anchor: Cell<Option<(usize, usize)>>,
    /// Where the pointer was pressed, and where the content was then.
    pub press: Cell<Option<(ljos_render::kurbo::Point, (f64, f64))>>,
    /// The last tap, which a tap on its cell soon after makes double.
    pub last_tap: Cell<Option<(ljos_runtime::Time, (usize, usize))>>,
    /// The cell an editor is open on.
    pub editing: Cell<Option<edit::Editing>>,
    /// Bumped when a pass moved a column or a row, for the views synced
    /// to this one.
    pub laid: Cell<Option<NodeId>>,
    /// The content position this view and its `syncView` last shared,
    /// across and down; tells which of the two moved since.
    pub synced_at: Cell<(f64, f64)>,
    /// What selects its cells with the pointer, and how far it got.
    pub select: select::SelectState,
}

impl std::fmt::Debug for TableState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableState")
            .field("rows", &self.cells.line_count())
            .field("columns", &self.cells.columns())
            .finish()
    }
}

impl TableState {
    pub(crate) fn delegates(&self) -> Option<ViewDelegates> {
        self.delegates.borrow().clone()
    }
}

/// `TableView` (QtQuick): cells in rows and columns. A model of one
/// column (a list, a number, a `ListModel`) shows as one column.
///
/// With a `syncView`, the view takes its column widths and spacing, and
/// shares its `contentX`, across, and its row heights, row spacing and
/// `contentY` down, as `syncDirection` says ([`layout`]).
///
/// Kept and read back, with no effect yet: `animate`, `alternatingRows`, `resizableColumns` and `resizableRows`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "TableView",
    extends = Flickable,
    enums = table_view_enums,
    attached = TableViewAttached
)]
pub struct TableView {
    #[qml(base)]
    pub flickable: Flickable,
    #[qml(notify, readonly)]
    pub rows: i32,
    #[qml(notify, readonly)]
    pub columns: i32,
    #[qml(notify)]
    pub row_spacing: f64,
    #[qml(notify)]
    pub column_spacing: f64,
    /// A function of the row that gives its height; a negative height or
    /// none: as tall as its tallest delegate.
    #[qml(notify)]
    pub row_height_provider: Value,
    #[qml(notify)]
    pub column_width_provider: Value,
    #[qml(notify)]
    pub model: Value,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    /// Delegates of rows that leave the view are kept to show the rows
    /// that enter it.
    #[qml(notify)]
    pub reuse_items: bool,
    #[qml(notify, object = "TableView")]
    pub sync_view: Option<ObjectId>,
    #[qml(notify, flags = "Qt.Orientations")]
    pub sync_direction: i32,
    #[qml(notify, readonly)]
    pub left_column: i32,
    #[qml(notify, readonly)]
    pub right_column: i32,
    #[qml(notify, readonly)]
    pub top_row: i32,
    #[qml(notify, readonly)]
    pub bottom_row: i32,
    #[qml(notify, object = "ItemSelectionModel")]
    pub selection_model: Option<ObjectId>,
    #[qml(notify)]
    pub animate: bool,
    #[qml(notify)]
    pub key_navigation_enabled: bool,
    #[qml(notify)]
    pub pointer_navigation_enabled: bool,
    #[qml(notify, readonly)]
    pub current_row: i32,
    #[qml(notify, readonly)]
    pub current_column: i32,
    #[qml(notify)]
    pub alternating_rows: bool,
    #[qml(notify, enumeration = "SelectionBehavior")]
    pub selection_behavior: i32,
    #[qml(notify)]
    pub resizable_columns: bool,
    #[qml(notify)]
    pub resizable_rows: bool,
    #[qml(notify, flags = "EditTriggers")]
    pub edit_triggers: i32,
    #[qml(notify, enumeration = "SelectionMode")]
    pub selection_mode: i32,
    #[qml(skip)]
    pub(crate) state: Rc<TableState>,
}

impl Default for TableView {
    fn default() -> Self {
        Self {
            flickable: Flickable::default(),
            rows: 0,
            columns: 0,
            row_spacing: 0.0,
            column_spacing: 0.0,
            row_height_provider: Value::Undefined,
            column_width_provider: Value::Undefined,
            model: Value::Undefined,
            delegate: None,
            reuse_items: true,
            sync_view: None,
            sync_direction: 3,
            left_column: -1,
            right_column: -1,
            top_row: -1,
            bottom_row: -1,
            selection_model: None,
            animate: true,
            key_navigation_enabled: true,
            pointer_navigation_enabled: true,
            current_row: -1,
            current_column: -1,
            alternating_rows: true,
            selection_behavior: selection_behavior::SELECT_CELLS,
            resizable_columns: false,
            resizable_rows: false,
            edit_triggers: edit_trigger::DOUBLE_TAPPED | edit_trigger::EDIT_KEY_PRESSED,
            selection_mode: selection_mode::EXTENDED,
            state: Rc::default(),
        }
    }
}

/// What `TableView.` attaches to a delegate of a table or tree view.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "TableViewAttached")]
pub struct TableViewAttached {
    #[qml(notify, readonly)]
    pub view: Option<ObjectId>,
    /// What edits the cell: `edit()` makes one over its delegate.
    #[qml(notify, object = "Component")]
    pub edit_delegate: Option<ObjectId>,
}

#[qml_impl]
impl TableViewAttached {
    /// The delegate went to the pool (`reuseItems`).
    #[qml(signal, name = "pooled")]
    fn pooled();

    /// The delegate came out of the pool to show another cell.
    #[qml(signal, name = "reused")]
    fn reused();

    /// Sent to the editor of a cell when what it edited is to be kept,
    /// before it closes.
    #[qml(signal, name = "commit")]
    fn commit();
}

/// The attached object `item` has from `TableView.`, if it ever asked
/// for one.
pub(crate) fn attached_of(e: &Engine, item: ObjectId) -> Option<ObjectId> {
    e.existing_attached_object(item, &MetaRef::Static(TableView::static_meta()))
}

/// The view whose delegate `item` is: its parent is the view's content.
fn view_of(e: &Engine, item: ObjectId) -> Option<ObjectId> {
    let parent = |o: ObjectId| e.untrack(|| e.read(o, Item::PARENT.index).as_object());
    let content = parent(item)?;
    let view = parent(content)?;
    (is_a::<TableView>(e, view) && flickable::content_item(e, view) == Some(content))
        .then_some(view)
}

/// Tells a delegate's attached object its view.
fn tell_attached(e: &Engine, view: ObjectId, att: ObjectId) {
    let v = Value::Object(view);
    warn(
        e,
        "attach",
        e.untrack(|| write_computed(e, att, TableViewAttached::VIEW.index, v)),
    );
}

/// `TreeView` (QtQuick): a table of the rows of a tree model that are in
/// sight, the rows under a row shown while it is expanded.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "TreeView", extends = TableView)]
pub struct TreeView {
    #[qml(base)]
    pub table: TableView,
    /// The row whose rows are the view's top level; the invalid index for
    /// the model's own top level.
    #[qml(notify, reset)]
    pub root_index: Value,
}

impl Default for TreeView {
    fn default() -> Self {
        Self {
            table: TableView {
                selection_behavior: selection_behavior::SELECT_ROWS,
                edit_triggers: edit_trigger::EDIT_KEY_PRESSED,
                ..TableView::default()
            },
            root_index: Value::Undefined,
        }
    }
}

pub(crate) fn state_of(e: &Engine, view: ObjectId) -> Option<Rc<TableState>> {
    e.with(view, |v: &TableView| v.state.clone()).ok()
}

/// What the engine's delegates call on a table view.
struct Host(ObjectId);

impl ViewHost for Host {
    fn allocated(&self, e: &Engine, index: usize, object: ObjectId) {
        let view = self.0;
        if !is_item(e, object) {
            let name = if is_a::<TreeView>(e, view) {
                "TreeView"
            } else {
                "TableView"
            };
            e.log(
                LogLevel::Warning,
                "qml",
                format!("QML {name}: Delegate must be of Item type"),
            );
            return;
        }
        let Some(content) = flickable::content_item(e, view) else {
            return;
        };
        // Before the delegate's bindings run, so `parent` and what the
        // tree says of the row read right in them, and a required
        // property the view gives counts as given.
        e.batch(|| {
            warn(e, "parent", tree::set_parent_item(e, object, Some(content)));
            stack(e, object, 1.0);
            if let Some(st) = state_of(e, view) {
                layout::decorate(e, view, &st, index, object, true);
            }
            // One its handlers asked for while it was made.
            if let Some(att) = attached_of(e, object) {
                tell_attached(e, view, att);
            }
        });
        // A delegate that takes the press (a TreeViewDelegate) is tapped
        // through its own signals.
        for signal in ["clicked", "doubleClicked"] {
            if e.signal_index(object, signal).is_ok() {
                let r = e.connect_by_name(object, signal, move |e, _| {
                    layout::tapped(e, view, object, signal == "doubleClicked");
                });
                warn(e, signal, r.map(drop));
            }
        }
    }

    fn ready(&self, e: &Engine, _: usize, _: ObjectId) {
        layout::refresh(e, self.0);
    }

    fn changed(&self, e: &Engine, change: &ModelChange, gone: &[(usize, ObjectId)]) {
        let Some(st) = state_of(e, self.0) else {
            return;
        };
        if matches!(
            change,
            ModelChange::RowsInserted { .. }
                | ModelChange::RowsRemoved { .. }
                | ModelChange::RowsMoved { .. }
                | ModelChange::ModelReset
        ) {
            st.heights.borrow_mut().clear();
        }
        if matches!(change, ModelChange::ModelReset) {
            st.widths.borrow_mut().clear();
        }
        for (_, obj) in gone {
            st.hidden.borrow_mut().remove(obj);
        }
        layout::bump(e, &st);
    }

    fn destroying(&self, e: &Engine, object: ObjectId) {
        if e.is_alive(object) && is_item(e, object) {
            let _ = tree::set_parent_item(e, object, None);
        }
        if let Some(st) = state_of(e, self.0) {
            st.hidden.borrow_mut().remove(&object);
            edit::forget(e, &st, object);
        }
    }

    fn roles_in_context(&self) -> bool {
        // A delegate with required properties still reads `row`, `model`
        // and the roles by name (observed on qml6).
        true
    }
}

/// Makes the view of `cx` lay out its cells from now on.
fn start(cx: &Cx, is_tree: bool) -> Result<(), EngineError> {
    let (e, me) = (cx.engine, cx.this);
    let st = state_of(e, me).ok_or(EngineError::DeadObject(me))?;
    st.tree.set(is_tree);
    let scope = e.object_scope(me).ok_or(EngineError::DeadObject(me))?;
    st.trigger
        .set(Some(e.graph().property(scope, Value::Int(0))?));
    st.laid.set(Some(e.graph().property(scope, Value::Int(0))?));
    st.synced_at.set((f64::NAN, f64::NAN));
    let delegates = ViewDelegates::new(me, Rc::new(Host(me)));
    *st.delegates.borrow_mut() = Some(delegates.clone());

    if is_tree && crate::item::is_unset(e, me, TreeView::ROOT_INDEX.index) {
        let none = e.invalid_model_index();
        warn(
            e,
            "rootIndex",
            e.write_internal(me, TreeView::ROOT_INDEX.index, none, WriteKind::Initial),
        );
    }

    // A new model, delegate or root: every cell is made again.
    let source = st.clone();
    e.effect_in(me, move |e| {
        let model = e.get(me, TableView::MODEL).unwrap_or_default();
        let delegate = e.get(me, TableView::DELEGATE).ok().flatten();
        let root = if is_tree {
            e.read(me, TreeView::ROOT_INDEX.index)
        } else {
            Value::Undefined
        };
        // A model made in completion, after the view, is read when made.
        e.track_model(&model);
        e.untrack(|| {
            let object = e.plain(&model).as_object();
            let list =
                models::model_of(e, &model).unwrap_or_else(|| e.model_from_value(&model, Some(me)));
            // An editor does not outlive its model, nor commit to the new.
            edit::close(e, me, &source, false);
            source.model.set(object);
            // No model: no rows, and no columns in a tree; a table has
            // one column still (observed on qml6).
            let tree = (!model.is_nullish() || !is_tree).then(|| cells::tree_of(list));
            let root = e
                .model_index_place(&root, object)
                .map(|p| p.iter().map(|(r, _)| *r as usize).collect())
                .unwrap_or_default();
            source.heights.borrow_mut().clear();
            source.widths.borrow_mut().clear();
            source.cells.set_tree(tree, root);
            // The delegates go with the old source, hidden or not.
            source.hidden.borrow_mut().clear();
            let as_model: Rc<dyn ljos_engine::ItemModel> = source.cells.clone();
            delegates.set_source(e, Some(as_model), delegate);
            layout::bump(e, &source);
        });
    })?;

    // A selection model without a model is of the view's.
    e.effect_in(me, move |e| {
        let Some(sm) = e.get(me, TableView::SELECTION_MODEL).ok().flatten() else {
            return;
        };
        let model = e.read(me, TableView::MODEL.index);
        let theirs = e.read(sm, ItemSelectionModel::MODEL.index);
        if theirs.is_nullish() && !model.is_nullish() {
            e.untrack(|| {
                warn(
                    e,
                    "selectionModel",
                    e.write_internal(
                        sm,
                        ItemSelectionModel::MODEL.index,
                        model,
                        WriteKind::Imperative,
                    ),
                );
            });
        }
    })?;

    let lay = st.clone();
    e.effect_in(me, move |e| {
        layout::read_inputs(e, me, &lay);
        e.untrack(|| {
            layout::follow(e, me, &lay);
            layout::pass(e, me, &lay);
        });
        layout::read_sizes(e, &lay);
    })?;
    Ok(())
}

/// A cell as QML names it: `Qt.point(column, row)`.
fn cell_of(e: &Engine, v: &Value) -> Option<(i64, i64)> {
    match e.plain(v) {
        Value::Point(p) => Some((p.x as i64, p.y as i64)),
        Value::IntPoint(p) => Some((i64::from(p.x), i64::from(p.y))),
        Value::Size(s) => Some((s.width as i64, s.height as i64)),
        other => Some((
            other.get("x")?.to_number() as i64,
            other.get("y")?.to_number() as i64,
        )),
    }
}

/// The view's row and column, when there is such a cell.
fn in_range(st: &TableState, row: i64, column: i64) -> Option<(usize, usize)> {
    let (row, column) = (usize::try_from(row).ok()?, usize::try_from(column).ok()?);
    (row < st.cells.line_count() && column < st.cells.columns()).then_some((row, column))
}

/// The model index of cell `(row, column)`.
fn index_of_cell(e: &Engine, st: &TableState, row: usize, column: usize) -> Value {
    let Some(line) = st.cells.line(row) else {
        return e.invalid_model_index();
    };
    let mut place: Vec<(i32, i32)> = line.path.iter().map(|r| (*r as i32, 0)).collect();
    if let Some(last) = place.last_mut() {
        last.1 = column as i32;
    }
    e.model_index(st.model.get(), &place)
}

/// The view's row and column of a model index, when it is in sight.
fn cell_of_index(e: &Engine, st: &TableState, index: &Value) -> Option<(usize, usize)> {
    let place = e.model_index_place(index, st.model.get())?;
    let path: Vec<usize> = place.iter().map(|(r, _)| *r as usize).collect();
    let column = place.last()?.1 as usize;
    Some((st.cells.row_of(&path)?, column))
}

fn delegate_at(e: &Engine, me: ObjectId, row: i64, column: i64) -> Value {
    let Some(st) = state_of(e, me) else {
        return Value::Null;
    };
    in_range(&st, row, column)
        .and_then(|(r, c)| st.delegates()?.object_at(r * st.cells.columns() + c))
        .filter(|o| !edit::is_away(&st, *o))
        .map_or(Value::Null, Value::Object)
}

#[qml_impl]
impl TableView {
    /// A `TreeView` starts as a tree, from its own completion.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let tree = cx
            .engine
            .untrack(|| cx.engine.with::<TreeView, _>(cx.this, |_| ()))
            .is_ok();
        start(cx, tree)
    }

    /// `forceLayout()`: lays the cells out once more.
    #[qml(invokable, name = "forceLayout")]
    fn force_layout(cx: &Cx) {
        layout::refresh(cx.engine, cx.this);
    }

    /// `itemAtCell(cell)` or `itemAtCell(column, row)`: the delegate of a
    /// cell in sight, `null` for any other.
    #[qml(invokable, name = "itemAtCell")]
    fn item_at_cell(cx: &Cx, cell: Value, row: Value) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let (column, row) = if row.is_nullish() {
            match cell_of(e, &cell) {
                Some(c) => c,
                None => return Value::Null,
            }
        } else {
            (cell.to_number() as i64, row.to_number() as i64)
        };
        delegate_at(e, me, row, column)
    }

    /// `itemAtIndex(index)`: the delegate of the cell of a model index.
    #[qml(invokable, name = "itemAtIndex")]
    fn item_at_index(cx: &Cx, index: Value) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let Some(st) = state_of(e, me) else {
            return Value::Null;
        };
        match cell_of_index(e, &st, &index) {
            Some((r, c)) => delegate_at(e, me, r as i64, c as i64),
            None => Value::Null,
        }
    }

    /// `cellAtPosition(point)` or `cellAtPosition(x, y)`: the cell under
    /// a point of the view; `(-1, -1)` for none.
    #[qml(invokable, name = "cellAtPosition")]
    fn cell_at_position(cx: &Cx, at: Value, y: Value) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let (x, y) = match (e.plain(&at), y.is_nullish()) {
            (Value::Point(p), true) => (p.x, p.y),
            (_, false) => (at.to_number(), y.to_number()),
            _ => return Value::int_point(-1, -1),
        };
        let x = x + e.read(me, Flickable::CONTENT_X.index).to_number();
        let y = y + e.read(me, Flickable::CONTENT_Y.index).to_number();
        let Some(st) = state_of(e, me) else {
            return Value::int_point(-1, -1);
        };
        layout::cell_at(&st, x, y).map_or(Value::int_point(-1, -1), |(r, c)| {
            Value::int_point(c as i32, r as i32)
        })
    }

    /// `index(row, column)`: the model index of a cell in sight; the
    /// invalid index for any other.
    #[qml(invokable)]
    fn index(cx: &Cx, row: i32, column: i32) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let Some(st) = state_of(e, me) else {
            return Value::Undefined;
        };
        match in_range(&st, i64::from(row), i64::from(column)) {
            Some((r, c)) => index_of_cell(e, &st, r, c),
            None => e.invalid_model_index(),
        }
    }

    /// `modelIndex(cell)` or `modelIndex(row, column)`.
    #[qml(invokable, name = "modelIndex")]
    fn model_index(cx: &Cx, cell: Value, column: Value) -> Value {
        let e = cx.engine;
        let (column, row) = if column.is_nullish() {
            match cell_of(e, &cell) {
                Some(c) => c,
                None => return e.invalid_model_index(),
            }
        } else {
            (column.to_number() as i64, cell.to_number() as i64)
        };
        let (Ok(row), Ok(column)) = (i32::try_from(row), i32::try_from(column)) else {
            return e.invalid_model_index();
        };
        Self::index(cx, row, column)
    }

    /// `cellAtIndex(index)`: the cell of a model index, as
    /// `Qt.point(column, row)`; `(-1, -1)` when it is not in sight.
    #[qml(invokable, name = "cellAtIndex")]
    fn cell_at_index(cx: &Cx, index: Value) -> Value {
        let (e, me) = (cx.engine, cx.this);
        state_of(e, me)
            .and_then(|st| cell_of_index(e, &st, &index))
            .map_or(Value::int_point(-1, -1), |(r, c)| {
                Value::int_point(c as i32, r as i32)
            })
    }

    #[qml(invokable, name = "rowAtIndex")]
    fn row_at_index(cx: &Cx, index: Value) -> i32 {
        let (e, me) = (cx.engine, cx.this);
        state_of(e, me)
            .and_then(|st| cell_of_index(e, &st, &index))
            .map_or(-1, |(r, _)| r as i32)
    }

    #[qml(invokable, name = "columnAtIndex")]
    fn column_at_index(cx: &Cx, index: Value) -> i32 {
        let (e, me) = (cx.engine, cx.this);
        state_of(e, me)
            .and_then(|st| cell_of_index(e, &st, &index))
            .map_or(-1, |(_, c)| c as i32)
    }

    /// `isRowLoaded(row)`: whether the row has its delegates now.
    #[qml(invokable, name = "isRowLoaded")]
    fn is_row_loaded(cx: &Cx, row: i32) -> bool {
        let (e, me) = (cx.engine, cx.this);
        !delegate_at(e, me, i64::from(row), 0).is_nullish()
    }

    #[qml(invokable, name = "isColumnLoaded")]
    fn is_column_loaded(cx: &Cx, column: i32) -> bool {
        let (e, me) = (cx.engine, cx.this);
        let Some(st) = state_of(e, me) else {
            return false;
        };
        let (Some(d), Ok(column)) = (st.delegates(), usize::try_from(column)) else {
            return false;
        };
        let cols = st.cells.columns();
        column < cols && d.live().iter().any(|(i, _)| i % cols == column)
    }

    /// `columnWidth(column)`: as laid out; -1 for a column that is not.
    #[qml(invokable, name = "columnWidth")]
    fn column_width(cx: &Cx, column: i32) -> f64 {
        state_of(cx.engine, cx.this)
            .and_then(|st| {
                let widths = st.column_x.borrow();
                widths.get(usize::try_from(column).ok()?).map(|c| c.1)
            })
            .unwrap_or(-1.0)
    }

    /// `rowHeight(row)`: as laid out; -1 for a row that is not loaded.
    #[qml(invokable, name = "rowHeight")]
    fn row_height(cx: &Cx, row: i32) -> f64 {
        let (e, me) = (cx.engine, cx.this);
        if !Self::is_row_loaded(cx, row) {
            return -1.0;
        }
        state_of(e, me)
            .and_then(|st| st.heights.borrow().get(row as usize).copied())
            .filter(|h| !h.is_nan())
            .unwrap_or(-1.0)
    }

    /// `positionViewAtRow(row, mode)`: brings the row into sight.
    #[qml(invokable, name = "positionViewAtRow")]
    fn position_view_at_row(cx: &Cx, row: i32, _mode: Value) {
        layout::show_row(cx.engine, cx.this, row);
    }

    /// `positionViewAtCell(cell, mode)`: brings the cell's row into
    /// sight.
    #[qml(invokable, name = "positionViewAtCell")]
    fn position_view_at_cell(cx: &Cx, cell: Value, _mode: Value) {
        if let Some((_, row)) = cell_of(cx.engine, &cell) {
            layout::show_row(cx.engine, cx.this, row as i32);
        }
    }

    /// `positionViewAtIndex(index, mode)`.
    #[qml(invokable, name = "positionViewAtIndex")]
    fn position_view_at_index(cx: &Cx, index: Value, _mode: Value) {
        let row = Self::row_at_index(cx, index);
        layout::show_row(cx.engine, cx.this, row);
    }

    /// `edit(index)`: opens an editor on the cell of a model index,
    /// whatever `editTriggers` say, committing the one open.
    #[qml(invokable, name = "edit")]
    fn edit(cx: &Cx, index: Value) {
        edit::edit_index(cx.engine, cx.this, &index);
    }

    /// `closeEditor()`: closes the open editor without a commit.
    #[qml(invokable, name = "closeEditor")]
    fn close_editor(cx: &Cx) {
        if let Some(st) = state_of(cx.engine, cx.this) {
            edit::close(cx.engine, cx.this, &st, false);
        }
    }

    /// The layout changed.
    #[qml(signal, name = "layoutChanged")]
    fn layout_changed();
}

/// The tree path of the view's row `row`, when there is such a row.
fn path_at(e: &Engine, view: ObjectId, row: i32) -> Option<(Rc<TableState>, Vec<usize>, bool)> {
    let st = state_of(e, view)?;
    let line = st.cells.line(usize::try_from(row).ok()?)?;
    Some((st, line.path, line.has_children))
}

/// Shows the rows newly in sight, and lays them out.
fn settle(e: &Engine, st: &TableState) {
    st.cells.update(e);
    layout::bump(e, st);
}

#[qml_impl]
impl TreeView {
    /// `depth(row)`: how far below the root the row is; -1 for a row the
    /// view does not have.
    #[qml(invokable)]
    fn depth(cx: &Cx, row: i32) -> i32 {
        let st = state_of(cx.engine, cx.this);
        st.and_then(|st| st.cells.line(usize::try_from(row).ok()?))
            .map_or(-1, |l| l.depth as i32)
    }

    #[qml(invokable, name = "isExpanded")]
    fn is_expanded(cx: &Cx, row: i32) -> bool {
        path_at(cx.engine, cx.this, row).is_some_and(|(st, path, _)| st.cells.is_expanded(&path))
    }

    /// `expand(row)`: shows the rows under the row. A row without rows
    /// under it is expanded too, and heard as such, though no rows come
    /// (both observed on qml6).
    #[qml(invokable)]
    fn expand(cx: &Cx, row: i32) {
        let (e, me) = (cx.engine, cx.this);
        let Some((st, path, _)) = path_at(e, me, row) else {
            return;
        };
        if !st.cells.set_expanded(&path, true) {
            return;
        }
        st.cells.fetch_more(e, &path);
        settle(e, &st);
        let r = e.emit_by_name(me, "expanded", &[Value::Int(row), Value::Int(1)]);
        warn(e, "expanded", r);
    }

    /// `collapse(row)`: hides the rows under the row.
    #[qml(invokable)]
    fn collapse(cx: &Cx, row: i32) {
        let (e, me) = (cx.engine, cx.this);
        let Some((st, path, _)) = path_at(e, me, row) else {
            return;
        };
        if !st.cells.set_expanded(&path, false) {
            return;
        }
        settle(e, &st);
        let r = e.emit_by_name(me, "collapsed", &[Value::Int(row), Value::Bool(false)]);
        warn(e, "collapsed", r);
    }

    #[qml(invokable, name = "toggleExpanded")]
    fn toggle_expanded(cx: &Cx, row: i32) {
        if Self::is_expanded(cx, row) {
            Self::collapse(cx, row);
        } else {
            Self::expand(cx, row);
        }
    }

    /// `expandRecursively(row, depth)`: expands the row and the rows
    /// under it `depth` levels down; every row of the tree for row -1,
    /// every level for depth -1.
    #[qml(invokable, name = "expandRecursively")]
    fn expand_recursively(cx: &Cx, row: Value, depth: Value) {
        let (e, me) = (cx.engine, cx.this);
        let Some(st) = state_of(e, me) else {
            return;
        };
        let Some(tree) = st.cells.tree() else {
            return;
        };
        let row = if row.is_nullish() {
            -1
        } else {
            row.to_number() as i32
        };
        let depth = if depth.is_nullish() {
            -1
        } else {
            depth.to_number() as i32
        };
        let start = if row < 0 {
            st.cells.root()
        } else {
            match st.cells.line(row as usize) {
                Some(l) => {
                    st.cells.set_expanded(&l.path, true);
                    st.cells.fetch_more(e, &l.path);
                    l.path
                }
                None => return,
            }
        };
        // Levels below the start: the start itself is one when a row.
        let levels = if depth < 0 {
            usize::MAX
        } else {
            (depth as usize).saturating_sub(usize::from(row >= 0))
        };
        // A row whose rows are not read yet is expanded and asked for
        // them; they come later, and the levels under them stay as the
        // model gives them.
        fn go(
            e: &Engine,
            tree: &dyn ljos_engine::ItemTree,
            cells: &Cells,
            path: &mut Vec<usize>,
            left: usize,
        ) {
            if left == 0 {
                return;
            }
            for r in 0..tree.row_count(path) {
                path.push(r);
                if tree.has_children(path) {
                    cells.set_expanded(path, true);
                    cells.fetch_more(e, path);
                    go(e, tree, cells, path, left - 1);
                }
                path.pop();
            }
        }
        go(e, &*tree, &st.cells, &mut start.clone(), levels);
        settle(e, &st);
        let r = e.emit_by_name(me, "expanded", &[Value::Int(row), Value::Int(depth)]);
        warn(e, "expanded", r);
    }

    /// `collapseRecursively(row)`: collapses the row and every row under
    /// it; every row of the tree for row -1.
    #[qml(invokable, name = "collapseRecursively")]
    fn collapse_recursively(cx: &Cx, row: Value) {
        let (e, me) = (cx.engine, cx.this);
        let Some(st) = state_of(e, me) else {
            return;
        };
        let row = if row.is_nullish() {
            -1
        } else {
            row.to_number() as i32
        };
        if row < 0 {
            st.cells.collapse_under(&st.cells.root());
        } else {
            let Some(l) = st.cells.line(row as usize) else {
                return;
            };
            st.cells.set_expanded(&l.path, false);
            st.cells.collapse_under(&l.path);
        }
        settle(e, &st);
        let r = e.emit_by_name(me, "collapsed", &[Value::Int(row), Value::Bool(true)]);
        warn(e, "collapsed", r);
    }

    /// `expandToIndex(index)`: expands every row above the item of a
    /// model index, so it is in sight. Heard once, for the row at the
    /// view's top level, with how many levels were opened (observed on
    /// qml6).
    #[qml(invokable, name = "expandToIndex")]
    fn expand_to_index(cx: &Cx, index: Value) {
        let (e, me) = (cx.engine, cx.this);
        let Some(st) = state_of(e, me) else {
            return;
        };
        let Some(place) = e.model_index_place(&index, st.model.get()) else {
            return;
        };
        let path: Vec<usize> = place.iter().map(|(r, _)| *r as usize).collect();
        let root = st.cells.root();
        if path.len() <= root.len() + 1 || !path.starts_with(&root) {
            return;
        }
        let above = &path[..path.len() - 1];
        for n in root.len() + 1..=above.len() {
            st.cells.set_expanded(&above[..n], true);
            st.cells.fetch_more(e, &above[..n]);
        }
        let top = st
            .cells
            .row_of(&path[..root.len() + 1])
            .map_or(-1, |r| r as i32);
        settle(e, &st);
        let levels = (above.len() - root.len()) as i32;
        let r = e.emit_by_name(me, "expanded", &[Value::Int(top), Value::Int(levels)]);
        warn(e, "expanded", r);
    }

    /// A row was expanded: `depth` levels of it, -1 for all.
    #[qml(signal, name = "expanded")]
    fn expanded(row: i32, depth: i32);

    #[qml(signal, name = "collapsed")]
    fn collapsed(row: i32, recursively: bool);
}

/// Registers the views.
pub(crate) fn register(engine: &Engine) {
    for meta in [TableView::static_meta(), TreeView::static_meta()] {
        engine.register_type("QtQuick", Version::new(2, 15), MetaRef::Static(meta));
    }
    let meta = MetaRef::Static(TableView::static_meta());
    // A focus scope, as on qml6: an editor or a delegate's item keeps
    // its focus when the view gets it back.
    crate::focus::register_focus_scope(engine, meta.clone());
    crate::keys::register_key_item(engine, meta.clone(), nav::key_hooks());
    crate::pointer_item::register_pointer_item(engine, meta, nav::pointer_hooks());
    engine.on_attached_created(|e, item, att| {
        if is_a::<TableViewAttached>(e, att)
            && let Some(view) = view_of(e, item)
        {
            tell_attached(e, view, att);
        }
        true
    });
}
