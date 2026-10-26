//! `HorizontalHeaderView`, `VerticalHeaderView` and their delegates, and
//! `TableViewDelegate` (QtQuick.Templates).
//!
//! A header view is a `TableView` synced to the view it heads, across
//! (horizontal) or down (vertical); the syncing is the table view's own.
//! With no model of its own its model is a proxy of its `syncView`'s: one
//! row of a section per column, or one column of a section per row, every
//! role of a section being its number counted from one, as qml6's are.
//! Learned by running `qml6` (Qt 6.11.2), pinned by
//! `features/controls/header-view.feature`; documentation:
//! doc.qt.io/qt-6/qml-qtquick-controls-horizontalheaderview.html,
//! -verticalheaderview, -headerviewdelegate and -tableviewdelegate.

use std::rc::Rc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, ItemModel, ItemTree, ModelChange, ObjectId, QmlObject,
    TreeChange, TreeListener, Value, qml_impl,
};
use ljos_quick::TableView;

use crate::combobox::ItemDelegate;
use crate::util::{bind_default, check, is_a, object};

/// `Qt.Horizontal` and `Qt.Vertical`.
const HORIZONTAL: i32 = 1;
const VERTICAL: i32 = 2;

// ----------------------------------------------------- TableViewDelegate

/// `TableViewDelegate`: the stock delegate of a `TableView`. What the
/// view gives it is required, as in Qt.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "TableViewDelegate", extends = ItemDelegate)]
pub struct TableViewDelegate {
    #[qml(base)]
    pub row: ItemDelegate,
    #[qml(notify, required, rename = "tableView", object = "TableView")]
    pub table_view: Option<ObjectId>,
    #[qml(notify, required)]
    pub current: bool,
    #[qml(notify, required)]
    pub selected: bool,
    #[qml(notify, required)]
    pub editing: bool,
}

/// `HeaderViewDelegate`: a section of a header view. `orientation` is
/// the header's.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "HeaderViewDelegate", extends = TableViewDelegate)]
pub struct HeaderViewDelegate {
    #[qml(base)]
    pub cell: TableViewDelegate,
    #[qml(notify, readonly, enumeration = "Qt.Orientation")]
    pub orientation: i32,
    #[qml(notify, required, rename = "headerView", object = "TableView")]
    pub header_view: Option<ObjectId>,
    #[qml(notify, required)]
    pub model: Value,
}

impl Default for HeaderViewDelegate {
    fn default() -> Self {
        Self {
            cell: TableViewDelegate::default(),
            orientation: HORIZONTAL,
            header_view: None,
            model: Value::Undefined,
        }
    }
}

#[qml_impl]
impl HeaderViewDelegate {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        cx.engine
            .bind_initial(me, HeaderViewDelegate::ORIENTATION.index, move |e| {
                let header = object(e, me, HeaderViewDelegate::HEADER_VIEW.index);
                Value::Int(match header {
                    Some(h) if is_a::<VerticalHeaderView>(e, h) => VERTICAL,
                    _ => HORIZONTAL,
                })
            })
    }
}

// ----------------------------------------------------------- header views

/// What the two header views share: `textRole`, the role a section shows.
#[derive(Debug, QmlObject)]
#[qml(name = "HeaderViewBase", extends = TableView)]
pub struct HeaderViewBase {
    #[qml(base)]
    pub table: TableView,
    #[qml(notify)]
    pub text_role: String,
}

impl HeaderViewBase {
    fn along(direction: i32) -> Self {
        let mut table = TableView::default();
        table.sync_direction = direction;
        table.flickable.flickable_direction = direction;
        table.resizable_columns = direction == HORIZONTAL;
        table.resizable_rows = direction == VERTICAL;
        Self {
            table,
            text_role: String::new(),
        }
    }
}

impl Default for HeaderViewBase {
    fn default() -> Self {
        Self::along(HORIZONTAL)
    }
}

#[qml_impl]
impl HeaderViewBase {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        start(cx.engine, cx.this)
    }
}

/// `HorizontalHeaderView`: a section per column of its `syncView`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "HorizontalHeaderView", extends = HeaderViewBase)]
pub struct HorizontalHeaderView {
    #[qml(base)]
    pub header: HeaderViewBase,
    /// Kept and read back, with no effect yet.
    #[qml(notify)]
    pub movable_columns: bool,
}

impl Default for HorizontalHeaderView {
    fn default() -> Self {
        Self {
            header: HeaderViewBase::along(HORIZONTAL),
            movable_columns: false,
        }
    }
}

/// `VerticalHeaderView`: a section per row of its `syncView`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "VerticalHeaderView", extends = HeaderViewBase)]
pub struct VerticalHeaderView {
    #[qml(base)]
    pub header: HeaderViewBase,
    /// Kept and read back, with no effect yet.
    #[qml(notify)]
    pub movable_rows: bool,
}

impl Default for VerticalHeaderView {
    fn default() -> Self {
        Self {
            header: HeaderViewBase::along(VERTICAL),
            movable_rows: false,
        }
    }
}

/// A header without a model of its own shows a proxy of its `syncView`'s,
/// remade when the `syncView` or its model is another; synced, its
/// `textRole` is `display` unless given.
fn start(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let across = !is_a::<VerticalHeaderView>(e, me);
    bind_default(e, me, HeaderViewBase::TEXT_ROLE.index, move |e| {
        let synced = object(e, me, TableView::SYNC_VIEW.index).is_some();
        Value::string(if synced { "display" } else { "" })
    })?;
    if !crate::util::is_unset(e, me, TableView::MODEL.index) {
        return Ok(());
    }
    let proxy = e.create_native(ProxyObject::default(), Some(me))?;
    // Without its sections yet the proxy would be a model of one object,
    // and the header would make a delegate for it.
    let empty: Rc<dyn ItemTree> = Rc::new(Proxy {
        source: None,
        across,
    });
    check(e, "header model", e.set_object_tree(proxy, empty));
    e.bind(me, TableView::MODEL.index, move |e| {
        if object(e, me, TableView::SYNC_VIEW.index).is_some() {
            Value::Object(proxy)
        } else {
            Value::Undefined
        }
    })?;
    e.effect_in(me, move |e| {
        let source = object(e, me, TableView::SYNC_VIEW.index)
            .map(|v| e.read(v, TableView::MODEL.index))
            .unwrap_or_default();
        e.track_model(&source);
        e.untrack(|| {
            let source = e.plain(&source).as_object().and_then(|o| e.object_model(o));
            let tree: Rc<dyn ItemTree> = Rc::new(Proxy { source, across });
            check(e, "header model", e.set_object_tree(proxy, tree));
        });
    })?;
    Ok(())
}

/// The object a header's proxy model is, as scripts see it.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "HeaderDataProxyModel")]
pub struct ProxyObject {}

/// The sections of a header: the columns (`across`) or the rows of the
/// source model, every role of each its number from one. A source that is
/// no model object (a number, a list) has none.
struct Proxy {
    source: Option<Rc<dyn ItemModel>>,
    across: bool,
}

impl Proxy {
    fn sections(&self) -> usize {
        let Some(source) = &self.source else {
            return 0;
        };
        match (self.across, source.tree()) {
            (true, Some(tree)) => tree.column_count(),
            (true, None) => 1,
            (false, _) => source.row_count(),
        }
    }

    /// The section of the cell at `path` and `column`.
    fn section(&self, path: &[usize], column: usize) -> Option<usize> {
        let section = match (self.across, path) {
            (true, [0]) => column,
            (false, [row]) if column == 0 => *row,
            _ => return None,
        };
        (section < self.sections()).then_some(section)
    }
}

impl ItemTree for Proxy {
    fn row_count(&self, parent: &[usize]) -> usize {
        match (parent.is_empty(), self.across) {
            (false, _) => 0,
            (true, true) => usize::from(self.sections() > 0),
            (true, false) => self.sections(),
        }
    }

    fn column_count(&self) -> usize {
        if self.across { self.sections() } else { 1 }
    }

    fn role_names(&self) -> Vec<Atom> {
        self.source
            .as_ref()
            .map(|s| s.role_names())
            .unwrap_or_default()
    }

    fn data(&self, _: &Engine, path: &[usize], column: usize, role: &str) -> Value {
        match self.section(path, column) {
            Some(s) if self.role_names().iter().any(|r| r.as_str() == role) => {
                Value::Int(s as i32 + 1)
            }
            _ => Value::Undefined,
        }
    }

    fn model_data(&self, _: &Engine, path: &[usize], column: usize) -> Option<Value> {
        self.section(path, column).map(|s| Value::Int(s as i32 + 1))
    }

    fn subscribe(&self, listener: TreeListener) -> u64 {
        let Some(source) = &self.source else {
            return 0;
        };
        // Any change to the source may add or take sections.
        source.subscribe(Rc::new(move |e: &Engine, c: &ModelChange| {
            if !matches!(
                c,
                ModelChange::RowsAboutToBeInserted { .. }
                    | ModelChange::RowsAboutToBeRemoved { .. }
            ) {
                listener(e, &TreeChange::ModelReset);
            }
        }))
    }

    fn unsubscribe(&self, id: u64) {
        if let Some(source) = &self.source {
            source.unsubscribe(id);
        }
    }
}

pub(crate) fn register(engine: &Engine) {
    use ljos_engine::{MetaRef, QmlType, Version};
    for meta in [
        TableViewDelegate::static_meta(),
        HeaderViewDelegate::static_meta(),
        HorizontalHeaderView::static_meta(),
        VerticalHeaderView::static_meta(),
    ] {
        engine.register_type(
            "QtQuick.Templates",
            Version::new(2, 15),
            MetaRef::Static(meta),
        );
    }
}
