//! `ListView` and `GridView`: a Flickable whose content is the rows of a
//! model, with a delegate only for the rows that are shown or near
//! (ARCHITECTURE.md §8.4).
//!
//! A view is a [`Flickable`]: the pointer, the wheel and an assistive
//! technology scroll it as they scroll any other. What it adds is the
//! content. The engine's [`ViewDelegates`] keeps one delegate per row the
//! view asks for and keeps each in step with the model; this module says
//! which rows those are and where each goes ([`pass`]), which row is the
//! current one and where its highlight is ([`current`]), how the view is
//! brought to a row ([`position`]) and what happens when rows come and go
//! ([`changes`]).
//!
//! Everything is measured along the direction the rows run in, whichever
//! way that is on screen ([`state::Geo`]): the layout is written once and
//! mapped to content coordinates at the end.

mod changes;
mod current;
pub(crate) mod delegate_groups;
mod models;
mod pass;
pub mod path_view;
mod position;
pub(crate) mod state;
pub(crate) mod view_transition;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, LogLevel, MetaRef, ObjectId, QmlObject, QmlType, Value, Version,
    ViewDelegates, WriteKind, qml_impl,
};

use crate::enums::{EnumTable, layout_direction};
use crate::flickable::{self, Flickable};
use crate::item::{is_a, write_computed};

pub use models::{
    DelegateModel, DelegateModelAttached, DelegateModelGroup, ObjectModel, drive_delegate_model,
    object_model_changed, object_model_rows,
};
pub use path_view::{PathView, PathViewAttached};
pub(crate) use state::ViewState;
pub use state::position_mode;
pub use view_transition::{ViewTransition, ViewTransitionAttached};

/// The log category of what a view could not do.
pub const CATEGORY: &str = "qt.quick.itemview";

/// `ListView.orientation`.
pub mod orientation {
    pub const HORIZONTAL: i32 = 1;
    pub const VERTICAL: i32 = 2;
}

/// `verticalLayoutDirection` of both views.
pub mod vertical_layout_direction {
    pub const TOP_TO_BOTTOM: i32 = 2;
    pub const BOTTOM_TO_TOP: i32 = 3;
}

/// `GridView.flow`.
pub mod grid_flow {
    pub const LEFT_TO_RIGHT: i32 = 0;
    pub const TOP_TO_BOTTOM: i32 = 1;
}

/// `ViewSection.criteria`.
pub mod section_criteria {
    pub const FULL_STRING: i32 = 0;
    pub const FIRST_CHARACTER: i32 = 1;
}

fn keys(list: &[(&'static str, i32)]) -> Vec<(&'static str, i64)> {
    list.iter().map(|(k, v)| (*k, i64::from(*v))).collect()
}

fn item_view_enums() -> EnumTable {
    use position_mode as p;
    use vertical_layout_direction as v;
    vec![
        (
            "PositionMode",
            keys(&[
                ("Beginning", p::BEGINNING),
                ("Center", p::CENTER),
                ("End", p::END),
                ("Visible", p::VISIBLE),
                ("Contain", p::CONTAIN),
                ("SnapPosition", p::SNAP_POSITION),
            ]),
        ),
        (
            "VerticalLayoutDirection",
            keys(&[
                ("TopToBottom", v::TOP_TO_BOTTOM),
                ("BottomToTop", v::BOTTOM_TO_TOP),
            ]),
        ),
        (
            "HighlightRangeMode",
            keys(&[
                ("NoHighlightRange", 0),
                ("ApplyRange", 1),
                ("StrictlyEnforceRange", 2),
            ]),
        ),
    ]
}

fn list_view_enums() -> EnumTable {
    vec![
        (
            "Orientation",
            keys(&[
                ("Horizontal", orientation::HORIZONTAL),
                ("Vertical", orientation::VERTICAL),
            ]),
        ),
        (
            "SnapMode",
            keys(&[("NoSnap", 0), ("SnapToItem", 1), ("SnapOneItem", 2)]),
        ),
        (
            "HeaderPositioning",
            keys(&[
                ("InlineHeader", 0),
                ("OverlayHeader", 1),
                ("PullBackHeader", 2),
            ]),
        ),
        (
            "FooterPositioning",
            keys(&[
                ("InlineFooter", 0),
                ("OverlayFooter", 1),
                ("PullBackFooter", 2),
            ]),
        ),
    ]
}

fn grid_view_enums() -> EnumTable {
    vec![
        (
            "Flow",
            keys(&[
                ("FlowLeftToRight", grid_flow::LEFT_TO_RIGHT),
                ("FlowTopToBottom", grid_flow::TOP_TO_BOTTOM),
            ]),
        ),
        (
            "SnapMode",
            keys(&[("NoSnap", 0), ("SnapToRow", 1), ("SnapOneRow", 2)]),
        ),
    ]
}

fn section_enums() -> EnumTable {
    vec![
        (
            "SectionCriteria",
            keys(&[
                ("FullString", section_criteria::FULL_STRING),
                ("FirstCharacter", section_criteria::FIRST_CHARACTER),
            ]),
        ),
        (
            "LabelPositioning",
            keys(&[
                ("InlineLabels", 1),
                ("CurrentLabelAtStart", 2),
                ("NextLabelAtEnd", 4),
            ]),
        ),
    ]
}

/// What `ListView` and `GridView` share. Not a QML type of its own.
///
/// Kept and read back, with no effect yet: `highlightRangeMode` and the
/// preferred highlight range.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "ItemView",
    extends = Flickable,
    enums = item_view_enums,
    uncreatable
)]
pub struct ItemView {
    #[qml(base)]
    pub flickable: Flickable,
    /// A `ListModel`, a number, an array, a list of objects or a native
    /// model ([`ljos_engine::QmlListModel`]).
    #[qml(notify)]
    pub model: Value,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub count: i32,
    #[qml(notify)]
    pub current_index: i32,
    #[qml(notify, readonly)]
    pub current_item: Option<ObjectId>,
    #[qml(notify)]
    pub key_navigation_wraps: bool,
    #[qml(notify)]
    pub key_navigation_enabled: bool,
    /// How far outside what is shown delegates are kept, in pixels.
    #[qml(notify)]
    pub cache_buffer: i32,
    #[qml(notify)]
    pub display_margin_beginning: i32,
    #[qml(notify)]
    pub display_margin_end: i32,
    #[qml(notify, enumeration = "Qt.LayoutDirection")]
    pub layout_direction: i32,
    #[qml(notify, readonly, enumeration = "Qt.LayoutDirection")]
    pub effective_layout_direction: i32,
    /// One of [`vertical_layout_direction`].
    #[qml(notify, enumeration = "VerticalLayoutDirection")]
    pub vertical_layout_direction: i32,
    #[qml(notify, object = "Component")]
    pub header: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub header_item: Option<ObjectId>,
    #[qml(notify, object = "Component")]
    pub footer: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub footer_item: Option<ObjectId>,
    #[qml(notify)]
    pub populate: Option<ObjectId>,
    #[qml(notify)]
    pub add: Option<ObjectId>,
    #[qml(notify)]
    pub add_displaced: Option<ObjectId>,
    #[qml(notify, rename = "move")]
    pub move_transition: Option<ObjectId>,
    #[qml(notify)]
    pub move_displaced: Option<ObjectId>,
    #[qml(notify)]
    pub remove: Option<ObjectId>,
    #[qml(notify)]
    pub remove_displaced: Option<ObjectId>,
    #[qml(notify)]
    pub displaced: Option<ObjectId>,
    #[qml(notify, object = "Component")]
    pub highlight: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub highlight_item: Option<ObjectId>,
    #[qml(notify)]
    pub highlight_follows_current_item: bool,
    #[qml(notify, enumeration = "HighlightRangeMode")]
    pub highlight_range_mode: i32,
    #[qml(notify, reset)]
    pub preferred_highlight_begin: f64,
    #[qml(notify, reset)]
    pub preferred_highlight_end: f64,
    /// Milliseconds; -1 for none (the velocity decides).
    #[qml(notify)]
    pub highlight_move_duration: i32,
    /// Delegates of rows that leave the view are kept to show the rows
    /// that enter it.
    #[qml(notify)]
    pub reuse_items: bool,
    #[qml(skip)]
    pub(crate) state: Rc<ViewState>,
}

impl Default for ItemView {
    fn default() -> Self {
        Self {
            flickable: Flickable::default(),
            model: Value::Undefined,
            delegate: None,
            count: 0,
            current_index: -1,
            current_item: None,
            key_navigation_wraps: false,
            key_navigation_enabled: true,
            cache_buffer: 320,
            display_margin_beginning: 0,
            display_margin_end: 0,
            layout_direction: layout_direction::LEFT_TO_RIGHT,
            effective_layout_direction: layout_direction::LEFT_TO_RIGHT,
            vertical_layout_direction: vertical_layout_direction::TOP_TO_BOTTOM,
            header: None,
            header_item: None,
            footer: None,
            footer_item: None,
            populate: None,
            add: None,
            add_displaced: None,
            move_transition: None,
            move_displaced: None,
            remove: None,
            remove_displaced: None,
            displaced: None,
            highlight: None,
            highlight_item: None,
            highlight_follows_current_item: true,
            highlight_range_mode: 0,
            preferred_highlight_begin: 0.0,
            preferred_highlight_end: 0.0,
            highlight_move_duration: -1,
            reuse_items: false,
            state: Rc::default(),
        }
    }
}

/// The `section` group of [`ListView`]: rows whose `property` reads the
/// same form a section, and `delegate` makes the heading of each.
/// `labelPositioning` is kept: headings are in line with the rows.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "ViewSection",
    enums = section_enums,
    uncreatable
)]
pub struct ViewSection {
    #[qml(notify)]
    pub property: Atom,
    /// One of [`section_criteria`].
    #[qml(notify, enumeration = "SectionCriteria")]
    pub criteria: i32,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(notify)]
    pub label_positioning: i32,
}

impl Default for ViewSection {
    fn default() -> Self {
        Self {
            property: Atom::EMPTY,
            criteria: section_criteria::FULL_STRING,
            delegate: None,
            label_positioning: 1,
        }
    }
}

/// `ListView` (QtQuick): the rows of a model one after another.
///
/// Kept and read back, with no effect yet: `headerPositioning` and
/// `footerPositioning` (both are in line with the rows) and the highlight
/// resize velocity and duration (the highlight takes its size at once).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "ListView",
    extends = ItemView,
    attached = ViewAttached,
    enums = list_view_enums
)]
pub struct ListView {
    #[qml(base)]
    pub view: ItemView,
    /// One of [`orientation`].
    #[qml(notify, enumeration = "Orientation")]
    pub orientation: i32,
    #[qml(notify)]
    pub spacing: f64,
    #[qml(group)]
    pub section: ViewSection,
    /// The section of the first row in sight.
    #[qml(notify, readonly)]
    pub current_section: Atom,
    /// Pixels a second.
    #[qml(notify)]
    pub highlight_move_velocity: f64,
    #[qml(notify)]
    pub highlight_resize_velocity: f64,
    #[qml(notify)]
    pub highlight_resize_duration: i32,
    #[qml(notify, enumeration = "SnapMode")]
    pub snap_mode: i32,
    #[qml(notify, enumeration = "HeaderPositioning")]
    pub header_positioning: i32,
    #[qml(notify, enumeration = "FooterPositioning")]
    pub footer_positioning: i32,
}

impl Default for ListView {
    fn default() -> Self {
        Self {
            view: ItemView::default(),
            orientation: orientation::VERTICAL,
            spacing: 0.0,
            section: ViewSection::default(),
            current_section: Atom::EMPTY,
            highlight_move_velocity: 400.0,
            highlight_resize_velocity: 400.0,
            highlight_resize_duration: -1,
            snap_mode: 0,
            header_positioning: 0,
            footer_positioning: 0,
        }
    }
}

/// `GridView` (QtQuick): the rows of a model in cells of one size.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "GridView",
    extends = ItemView,
    attached = ViewAttached,
    enums = grid_view_enums
)]
pub struct GridView {
    #[qml(base)]
    pub view: ItemView,
    /// One of [`grid_flow`].
    #[qml(notify, enumeration = "Flow")]
    pub flow: i32,
    #[qml(notify)]
    pub cell_width: f64,
    #[qml(notify)]
    pub cell_height: f64,
    #[qml(notify, enumeration = "SnapMode")]
    pub snap_mode: i32,
}

impl Default for GridView {
    fn default() -> Self {
        Self {
            view: ItemView {
                // A grid's highlight takes a time, a list's has a speed.
                highlight_move_duration: 150,
                ..ItemView::default()
            },
            flow: grid_flow::LEFT_TO_RIGHT,
            cell_width: 100.0,
            cell_height: 100.0,
            snap_mode: 0,
        }
    }
}

/// What `ListView.` and `GridView.` attach to the root item of a delegate
/// (and `view` to a header, a footer, a section heading and a highlight).
#[derive(Debug, Default, QmlObject)]
#[qml(name = "ItemViewAttached")]
pub struct ViewAttached {
    #[qml(notify, readonly)]
    pub view: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub is_current_item: bool,
    #[qml(notify, readonly)]
    pub section: Atom,
    #[qml(notify, readonly)]
    pub previous_section: Atom,
    #[qml(notify, readonly)]
    pub next_section: Atom,
    /// While true the delegate of a row that left the model stays.
    #[qml(notify)]
    pub delay_remove: bool,
}

#[qml_impl]
impl ViewAttached {
    /// The row arrived in the model after the view was filled.
    #[qml(signal, name = "add")]
    fn add();

    /// The row left the model; the delegate goes after this.
    #[qml(signal, name = "remove")]
    fn remove();

    /// The delegate went to the pool (`reuseItems`).
    #[qml(signal, name = "pooled")]
    fn pooled();

    /// The delegate came out of the pool to show another row.
    #[qml(signal, name = "reused")]
    fn reused();
}

/// What an item a view made is to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    /// The delegate of a row.
    Row,
    /// A header, a footer, a section heading or the highlight.
    Extra,
}

/// Which view made which item: an item finds its view here when it asks
/// for `ListView.view`, and the accessibility pass its place in the set.
#[derive(Default)]
struct Registry {
    items: RefCell<HashMap<ObjectId, (ObjectId, Part)>>,
}

pub(crate) fn enrol(e: &Engine, item: ObjectId, view: ObjectId, part: Part) {
    e.extension::<Registry>()
        .items
        .borrow_mut()
        .insert(item, (view, part));
}

pub(crate) fn strike(e: &Engine, item: ObjectId) {
    e.extension::<Registry>().items.borrow_mut().remove(&item);
}

pub(crate) fn view_of(e: &Engine, item: ObjectId) -> Option<(ObjectId, Part)> {
    e.extension::<Registry>().items.borrow().get(&item).copied()
}

/// The delegate a view has for row `index`, if it has one now: a view
/// has delegates only for the rows near what it shows.
pub fn view_item_at(e: &Engine, view: ObjectId, index: usize) -> Option<ObjectId> {
    state_of(e, view)?.delegates()?.object_at(index)
}

/// The row `item` is the delegate of in `view`.
pub fn view_index_of(e: &Engine, view: ObjectId, item: ObjectId) -> Option<usize> {
    state_of(e, view)?.delegates()?.index_of(item)
}

pub(crate) fn state_of(e: &Engine, view: ObjectId) -> Option<Rc<ViewState>> {
    e.with(view, |v: &ItemView| v.state.clone()).ok()
}

pub(crate) fn is_grid(e: &Engine, view: ObjectId) -> bool {
    is_a::<GridView>(e, view)
}

/// The attached object `item` has from its view, if it ever asked for
/// one: neither the views nor this module make one for it.
pub(crate) fn attached_of(e: &Engine, view: ObjectId, item: ObjectId) -> Option<ObjectId> {
    let meta = if is_grid(e, view) {
        GridView::static_meta()
    } else {
        ListView::static_meta()
    };
    e.existing_attached_object(item, &MetaRef::Static(meta))
}

pub(crate) fn warn(e: &Engine, what: &str, r: Result<(), EngineError>) {
    if let Err(err) = r {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("ItemView: {what}: {err}"),
        );
    }
}

/// The name of the view's type, for what it tells the author.
pub(crate) fn type_name(e: &Engine, view: ObjectId) -> &'static str {
    if is_grid(e, view) {
        "GridView"
    } else {
        "ListView"
    }
}

/// The place of the delegate `obj` among the rows of its view and how
/// many rows there are, for an assistive technology: most rows have no
/// delegate, so the tree cannot be counted. A tracked read.
pub(crate) fn place_in_view(e: &Engine, obj: ObjectId) -> Option<(usize, usize)> {
    let (view, part) = view_of(e, obj)?;
    if part != Part::Row {
        return None;
    }
    let st = state_of(e, view)?;
    // Bumped whenever rows come, go or move.
    if let Some(n) = st.trigger.get() {
        let _ = e.graph().get(n);
    }
    let row = st.delegates()?.index_of(obj)?;
    let of = e.read(view, ItemView::COUNT.index).to_number() as usize;
    Some((row + 1, of.max(row + 1)))
}

/// Makes the view of `cx` lay out its rows from now on.
fn start(cx: &Cx, grid: bool) -> Result<(), EngineError> {
    let (e, me) = (cx.engine, cx.this);
    let st = e.with(me, |v: &ItemView| v.state.clone())?;
    st.grid.set(grid);
    let scope = e.object_scope(me).ok_or(EngineError::DeadObject(me))?;
    st.trigger
        .set(Some(e.graph().property(scope, Value::Int(0))?));
    let delegates = ViewDelegates::new(me, Rc::new(changes::Host(me)));
    *st.delegates.borrow_mut() = Some(delegates.clone());

    // A new model or delegate: every row is made again.
    let source = st.clone();
    e.effect_in(me, move |e| {
        let model = e.get(me, ItemView::MODEL).unwrap_or_default();
        let delegate = e.get(me, ItemView::DELEGATE).ok().flatten();
        e.untrack(|| {
            let model =
                models::model_of(e, &model).unwrap_or_else(|| e.model_from_value(&model, Some(me)));
            changes::forget(e, me, &source);
            delegates.set_source(e, Some(model), delegate);
            pass::bump(e, &source);
        });
    })?;

    let layout = st.clone();
    e.effect_in(me, move |e| {
        pass::read_inputs(e, me, &layout);
        e.untrack(|| pass::pass(e, me, &layout, None));
        pass::read_sizes(e, me, &layout);
    })?;

    // `snapMode`: the content comes to rest on a row.
    e.connect_by_name(me, "movementEnded", move |e, _| position::snap(e, me))?;
    // A view the user moves stops following its highlight.
    e.connect_by_name(me, "movementStarted", move |e, _| {
        if let Some(st) = state_of(e, me) {
            st.follow.set(false);
        }
    })?;
    Ok(())
}

#[qml_impl]
impl ItemView {
    /// `positionViewAtIndex(index, mode)`: brings row `index` where
    /// `mode` (one of [`position_mode`]) says. Nothing for a row there is
    /// not.
    #[qml(invokable, name = "positionViewAtIndex")]
    fn position_view_at_index(cx: &Cx, index: i32, mode: i32) {
        position::at_index(cx.engine, cx.this, index, mode);
    }

    #[qml(invokable, name = "positionViewAtBeginning")]
    fn position_view_at_beginning(cx: &Cx) {
        position::at_end(cx.engine, cx.this, false);
    }

    #[qml(invokable, name = "positionViewAtEnd")]
    fn position_view_at_end(cx: &Cx) {
        position::at_end(cx.engine, cx.this, true);
    }

    /// `indexAt(x, y)`: the row shown at a point of the content; -1 when
    /// there is none.
    #[qml(invokable, name = "indexAt")]
    fn index_at(cx: &Cx, x: f64, y: f64) -> i32 {
        position::index_at(cx.engine, cx.this, x, y).map_or(-1, |i| i as i32)
    }

    /// `itemAt(x, y)`: the delegate shown at a point of the content.
    #[qml(invokable, name = "itemAt")]
    fn item_at(cx: &Cx, x: f64, y: f64) -> Value {
        let (e, me) = (cx.engine, cx.this);
        position::index_at(e, me, x, y)
            .and_then(|i| state_of(e, me)?.delegates()?.object_at(i))
            .map_or(Value::Null, Value::Object)
    }

    /// `itemAtIndex(index)`: the delegate of a row, `null` for a row that
    /// has none now.
    #[qml(invokable, name = "itemAtIndex")]
    fn item_at_index(cx: &Cx, index: i32) -> Value {
        let (e, me) = (cx.engine, cx.this);
        usize::try_from(index)
            .ok()
            .and_then(|i| state_of(e, me)?.delegates()?.object_at(i))
            .map_or(Value::Null, Value::Object)
    }

    /// `forceLayout()`: the rows are laid out when the model changes
    /// already; this lays them out once more.
    #[qml(invokable, name = "forceLayout")]
    fn force_layout(cx: &Cx) {
        pass::refresh(cx.engine, cx.this);
    }
}

#[qml_impl]
impl ListView {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        start(cx, false)
    }

    /// `incrementCurrentIndex()`: the next row is the current one; from
    /// the last row to the first only with `keyNavigationWraps`.
    #[qml(invokable, name = "incrementCurrentIndex")]
    fn increment_current_index(cx: &Cx) {
        current::step(cx.engine, cx.this, current::Step::Next);
    }

    #[qml(invokable, name = "decrementCurrentIndex")]
    fn decrement_current_index(cx: &Cx) {
        current::step(cx.engine, cx.this, current::Step::Previous);
    }
}

#[qml_impl]
impl GridView {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        start(cx, true)
    }

    #[qml(invokable, name = "moveCurrentIndexUp")]
    fn move_current_index_up(cx: &Cx) {
        current::step(cx.engine, cx.this, current::Step::Up);
    }

    #[qml(invokable, name = "moveCurrentIndexDown")]
    fn move_current_index_down(cx: &Cx) {
        current::step(cx.engine, cx.this, current::Step::Down);
    }

    #[qml(invokable, name = "moveCurrentIndexLeft")]
    fn move_current_index_left(cx: &Cx) {
        current::step(cx.engine, cx.this, current::Step::Left);
    }

    #[qml(invokable, name = "moveCurrentIndexRight")]
    fn move_current_index_right(cx: &Cx) {
        current::step(cx.engine, cx.this, current::Step::Right);
    }
}

/// An item that asks for `ListView.view` (or any of the others) after its
/// view placed it is told in the statement that asked.
fn tell_attached(e: &Engine, item: ObjectId, att: ObjectId) {
    let Some((view, part)) = view_of(e, item) else {
        return;
    };
    e.untrack(|| {
        warn(
            e,
            "attach",
            write_computed(e, att, ViewAttached::VIEW.index, Value::Object(view)),
        );
        if part == Part::Row
            && let Some(st) = state_of(e, view)
        {
            pass::tell_row(e, view, &st, item, att);
        }
    });
}

/// Registers the views and what makes them take keys.
pub(crate) fn register(engine: &Engine) {
    models::register(engine);
    path_view::register(engine);
    for meta in [
        ListView::static_meta(),
        GridView::static_meta(),
        ViewSection::static_meta(),
        view_transition::ViewTransition::static_meta(),
    ] {
        engine.register_type("QtQuick", Version::new(2, 15), MetaRef::Static(meta));
    }
    crate::keys::register_key_item(
        engine,
        MetaRef::Static(ItemView::static_meta()),
        current::key_hooks(),
    );
    engine.on_attached_created(|e, item, att| {
        if is_a::<ViewAttached>(e, att) {
            tell_attached(e, item, att);
        }
        true
    });
}

/// Writes a property of a view the view itself computes.
pub(crate) fn put(e: &Engine, view: ObjectId, p: ljos_engine::PropIndex, v: Value) {
    flickable::put(e, view, p, v);
}

/// Gives an item a view made its place above or below the others, unless
/// its author gave it one.
pub(crate) fn stack(e: &Engine, item: ObjectId, z: f64) {
    if crate::item::is_unset(e, item, crate::item::Item::Z.index) {
        warn(
            e,
            "z",
            e.write_internal(
                item,
                crate::item::Item::Z.index,
                Value::Real(z),
                WriteKind::Initial,
            ),
        );
    }
}
