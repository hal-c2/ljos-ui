//! `PathView` (QtQuick): the rows of a model along a `Path`.
//!
//! The items lie at even shares of the path (`PathPercent` makes some
//! stretches hold more of them than others) and move along it as `offset`
//! changes; `offset` goes from 0 to the number of rows and comes round
//! again. Only the rows on the path have a delegate, and the current row.
//! Everything here was checked on qml6 as a black box; where it is not
//! what Qt does, it says so.
//!
//! [`layout`] puts the delegates where `offset` says, [`motion`] moves
//! `offset` by itself (to the current row, after a flick) and [`drag`] lets
//! the user move it along the path.

mod drag;
mod layout;
mod motion;
mod track;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, MetaBuilder, MetaRef, NodeId, ObjectId, PropertyFlags,
    QmlObject, QmlType, TypeTag, Value, Version, ViewDelegates, qml_impl,
};

use super::state::position_mode;
use super::{models, warn};
use crate::enums::EnumTable;
use crate::item::{Item, is_a, write_computed};
use track::Track;

pub(crate) use drag::steal;

/// `PathView.highlightRangeMode`.
pub mod highlight_range {
    pub const NONE: i32 = 0;
    pub const APPLY: i32 = 1;
    pub const STRICT: i32 = 2;
}

/// `PathView.snapMode`.
pub mod snap_mode {
    pub const NO_SNAP: i32 = 0;
    pub const SNAP_TO_ITEM: i32 = 1;
    pub const SNAP_ONE_ITEM: i32 = 2;
}

/// `PathView.movementDirection`: which way `offset` goes to bring the
/// current row to the highlight.
pub mod movement_direction {
    pub const SHORTEST: i32 = 0;
    pub const NEGATIVE: i32 = 1;
    pub const POSITIVE: i32 = 2;
}

fn path_view_enums() -> EnumTable {
    use position_mode as p;
    vec![
        (
            "HighlightRangeMode",
            vec![
                ("NoHighlightRange", 0),
                ("ApplyRange", 1),
                ("StrictlyEnforceRange", 2),
            ],
        ),
        (
            "SnapMode",
            vec![("NoSnap", 0), ("SnapToItem", 1), ("SnapOneItem", 2)],
        ),
        (
            "MovementDirection",
            vec![("Shortest", 0), ("Negative", 1), ("Positive", 2)],
        ),
        (
            "PositionMode",
            vec![
                ("Beginning", i64::from(p::BEGINNING)),
                ("Center", i64::from(p::CENTER)),
                ("End", i64::from(p::END)),
                ("Contain", i64::from(p::CONTAIN)),
                ("SnapPosition", i64::from(p::SNAP_POSITION)),
            ],
        ),
    ]
}

/// `PathView` (QtQuick).
///
/// `preferredHighlightEnd` only says, under `StrictlyEnforceRange`,
/// whether there is a range (it may not end before it begins); the current
/// row is brought to the range's beginning. Under `ApplyRange` a current
/// row past the end is brought back to it. `cacheItemCount` rows more have
/// a delegate, those within half of it of either end of the path; they
/// wait off the path at its end (qml6). Qt makes them over several frames;
/// here they are made at once.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "PathView",
    extends = Item,
    attached = PathViewAttached,
    enums = path_view_enums
)]
pub struct PathView {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub model: Value,
    #[qml(notify, object = "Path")]
    pub path: Option<ObjectId>,
    #[qml(notify)]
    pub current_index: i32,
    #[qml(notify, readonly)]
    pub current_item: Option<ObjectId>,
    /// From 0 to the number of rows: how far the rows have moved along
    /// the path, in rows.
    #[qml(notify)]
    pub offset: f64,
    #[qml(notify, object = "Component")]
    pub highlight: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub highlight_item: Option<ObjectId>,
    #[qml(notify)]
    pub preferred_highlight_begin: f64,
    #[qml(notify)]
    pub preferred_highlight_end: f64,
    #[qml(notify, enumeration = "HighlightRangeMode")]
    pub highlight_range_mode: i32,
    /// Milliseconds.
    #[qml(notify)]
    pub highlight_move_duration: i32,
    /// How far from the path a press may be and still move the rows.
    #[qml(notify)]
    pub drag_margin: f64,
    #[qml(notify)]
    pub maximum_flick_velocity: f64,
    #[qml(notify)]
    pub flick_deceleration: f64,
    #[qml(notify)]
    pub interactive: bool,
    #[qml(notify, readonly)]
    pub moving: bool,
    #[qml(notify, readonly)]
    pub flicking: bool,
    #[qml(notify, readonly)]
    pub dragging: bool,
    #[qml(notify, readonly)]
    pub count: i32,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    /// How many rows the path holds at once; -1 (and `undefined`) for all.
    #[qml(notify, reset)]
    pub path_item_count: i32,
    #[qml(notify, enumeration = "SnapMode")]
    pub snap_mode: i32,
    #[qml(notify, enumeration = "MovementDirection")]
    pub movement_direction: i32,
    #[qml(notify)]
    pub cache_item_count: i32,
    #[qml(skip)]
    pub(crate) state: Rc<State>,
}

impl Default for PathView {
    fn default() -> Self {
        Self {
            item: Item::default(),
            model: Value::Undefined,
            path: None,
            current_index: 0,
            current_item: None,
            offset: 0.0,
            highlight: None,
            highlight_item: None,
            preferred_highlight_begin: 0.0,
            preferred_highlight_end: 0.0,
            highlight_range_mode: highlight_range::STRICT,
            highlight_move_duration: 300,
            drag_margin: 0.0,
            maximum_flick_velocity: 2500.0,
            flick_deceleration: 100.0,
            interactive: true,
            moving: false,
            flicking: false,
            dragging: false,
            count: 0,
            delegate: None,
            path_item_count: -1,
            snap_mode: snap_mode::NO_SNAP,
            movement_direction: movement_direction::SHORTEST,
            cache_item_count: 0,
            state: Rc::default(),
        }
    }
}

/// What `PathView.` attaches to the delegate of a row. The view also gives
/// it, read-only, every attribute its path names with `PathAttribute`.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "PathViewAttached")]
pub struct PathViewAttached {
    #[qml(notify, readonly)]
    pub view: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub is_current_item: bool,
    #[qml(notify, readonly)]
    pub on_path: bool,
}

#[qml_impl]
impl PathViewAttached {
    /// Qt's name for the change of `isCurrentItem`.
    #[qml(signal, name = "currentItemChanged")]
    fn current_item_changed();

    /// Qt's name for the change of `onPath`.
    #[qml(signal, name = "pathChanged")]
    fn path_changed();
}

/// Where the rows are, as the last layout worked it out: the attached
/// object of a delegate that is being made reads its row's values here.
#[derive(Clone, Debug)]
pub(crate) struct Frame {
    /// Rows.
    pub n: usize,
    /// Rows the path holds.
    pub k: usize,
    /// `offset` and the shift that brings the current row to the
    /// highlight.
    pub shift: f64,
    pub current: Option<usize>,
    pub track: Rc<Track>,
    /// `cacheItemCount`.
    pub cache: usize,
}

impl Frame {
    /// Whether row `i` has a delegate (besides the current row): on the
    /// path, or less than half the cache from either end of it. The
    /// bounds of the cache are not in it (qml6: a cache of 2 makes the
    /// row one place past the end, not the one a place before the start).
    pub(crate) fn wanted(&self, i: usize) -> bool {
        let rel = self.rel(i);
        let k = self.k as f64;
        if rel < k {
            return true;
        }
        let half = self.cache as f64 / 2.0;
        self.cache > 0 && (rel < k + half || rel - self.n as f64 > -half)
    }

    /// Where row `i` is among the places on the path, 0 at its start; at
    /// `k` or beyond it is off the path.
    pub(crate) fn rel(&self, i: usize) -> f64 {
        let n = self.n as f64;
        let r = (i as f64 + self.shift).rem_euclid(n);
        // A sum that should be a whole number of rows may come out just
        // under it.
        if n - r < 1e-9 { 0.0 } else { r }
    }

    pub(crate) fn on_path(&self, i: usize) -> bool {
        self.rel(i) < self.k as f64
    }

    /// The length along the path of row `i`; the end for a row off it.
    pub(crate) fn len_of(&self, i: usize) -> f64 {
        let rel = self.rel(i);
        let share = if rel < self.k as f64 {
            rel / self.k as f64
        } else {
            1.0
        };
        self.track.len_of_percent(share)
    }
}

#[derive(Default)]
pub(crate) struct State {
    pub delegates: RefCell<Option<ViewDelegates>>,
    /// Bumped to lay the rows out again.
    pub trigger: Cell<Option<NodeId>>,
    pub track: RefCell<Option<Rc<Track>>>,
    pub frame: RefCell<Option<Frame>>,
    /// The attributes the attached objects have, and the type that gives
    /// them those.
    pub attached: RefCell<Option<(Vec<Atom>, MetaRef)>>,
    /// The highlight's component and the item made of it.
    pub highlight: Cell<Option<(ObjectId, ObjectId)>>,
    pub motion: Cell<Option<motion::Run>>,
    /// The view writes `currentIndex` itself.
    pub writing_current: Cell<bool>,
    /// The view writes `offset` itself.
    pub writing_offset: Cell<bool>,
    /// A model with rows was seen: a later one starts from the first row.
    pub sourced: Cell<bool>,
    pub press: RefCell<Option<drag::Press>>,
    /// The press ([`crate::flickable::drag::press_serial`]) the view last
    /// watched on behalf of an item inside it.
    pub watching: Cell<Option<u64>>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PathViewState").finish_non_exhaustive()
    }
}

impl State {
    pub(crate) fn delegates(&self) -> Option<ViewDelegates> {
        self.delegates.borrow().clone()
    }

    pub(crate) fn frame(&self) -> Option<Frame> {
        self.frame.borrow().clone()
    }
}

pub(crate) fn state_of(e: &Engine, view: ObjectId) -> Option<Rc<State>> {
    e.with(view, |v: &PathView| v.state.clone()).ok()
}

/// Which `PathView` made which delegate.
#[derive(Default)]
struct Registry {
    items: RefCell<HashMap<ObjectId, ObjectId>>,
}

pub(crate) fn enrol(e: &Engine, item: ObjectId, view: ObjectId) {
    e.extension::<Registry>()
        .items
        .borrow_mut()
        .insert(item, view);
}

pub(crate) fn strike(e: &Engine, item: ObjectId) {
    e.extension::<Registry>().items.borrow_mut().remove(&item);
}

fn view_of(e: &Engine, item: ObjectId) -> Option<ObjectId> {
    e.extension::<Registry>().items.borrow().get(&item).copied()
}

/// The attached object `item` has from a `PathView`, if it asked for one.
pub(crate) fn attached_of(e: &Engine, item: ObjectId) -> Option<ObjectId> {
    e.existing_attached_object(item, &MetaRef::Static(PathView::static_meta()))
}

/// The type the attached objects of `st`'s view have: it has a property
/// for each attribute in `names`, and for every one it had before.
fn attached_type(st: &State, names: &[Atom]) -> MetaRef {
    let mut cur = st.attached.borrow_mut();
    let (have, meta) =
        cur.get_or_insert_with(|| (Vec::new(), MetaRef::Static(PathViewAttached::static_meta())));
    let missing: Vec<Atom> = names
        .iter()
        .filter(|n| !have.contains(n) && meta.property_index(n.as_str()).is_none())
        .cloned()
        .collect();
    if !missing.is_empty() {
        let mut b = MetaBuilder::new("PathViewAttached")
            .native::<PathViewAttached>()
            .base(meta.clone());
        for n in missing {
            b = b.property(n.as_str(), TypeTag::Var, PropertyFlags::READONLY, true);
            have.push(n);
        }
        *meta = b.build_shared();
    }
    meta.clone()
}

/// Tells the attached object of row `row`'s delegate what it is now.
pub(crate) fn tell(e: &Engine, view: ObjectId, st: &State, row: usize, att: ObjectId) {
    let Some(f) = st.frame() else {
        return;
    };
    let names: Vec<Atom> = f.track.attrs.iter().map(|a| a.0.clone()).collect();
    let meta = attached_type(st, &names);
    warn(e, "attach", e.extend_type(att, meta));
    let put = |p, v: bool, signal: &str| {
        let was = e.untrack(|| e.read(att, p)).to_boolean();
        warn(e, "attach", write_computed(e, att, p, Value::Bool(v)));
        if was != v {
            warn(e, "attach", e.emit_by_name(att, signal, &[]));
        }
    };
    warn(
        e,
        "attach",
        write_computed(e, att, PathViewAttached::VIEW.index, Value::Object(view)),
    );
    put(
        PathViewAttached::IS_CURRENT_ITEM.index,
        f.current == Some(row),
        "currentItemChanged",
    );
    put(
        PathViewAttached::ON_PATH.index,
        f.on_path(row),
        "pathChanged",
    );
    let len = f.len_of(row);
    for (i, name) in names.iter().enumerate() {
        if let Ok(p) = e.property_index(att, name.as_str()) {
            let v = Value::Real(f.track.attr(i, len));
            warn(e, "attach", write_computed(e, att, p, v));
        }
    }
}

/// An item that asks for `PathView.isCurrentItem` (or another) is told in
/// the statement that asked, even while it is being made.
fn tell_attached(e: &Engine, item: ObjectId, att: ObjectId) {
    let Some(view) = view_of(e, item) else {
        return;
    };
    let Some(st) = state_of(e, view) else {
        return;
    };
    let Some(row) = st.delegates().and_then(|d| d.index_of(item)) else {
        return;
    };
    e.untrack(|| tell(e, view, &st, row, att));
}

/// Writes a property the view itself computes.
pub(crate) fn put(e: &Engine, view: ObjectId, p: ljos_engine::PropIndex, v: Value) {
    crate::flickable::put(e, view, p, v);
}

pub(crate) fn rd(e: &Engine, obj: ObjectId, p: ljos_engine::PropIndex) -> f64 {
    e.read(obj, p).to_number()
}

/// `highlightRangeMode` as it acts: a range that ends before it begins is
/// no range, whatever the mode (qml6).
pub(crate) fn range_mode(e: &Engine, view: ObjectId) -> i32 {
    let mode = rd(e, view, PathView::HIGHLIGHT_RANGE_MODE.index) as i32;
    let begin = rd(e, view, PathView::PREFERRED_HIGHLIGHT_BEGIN.index);
    let end = rd(e, view, PathView::PREFERRED_HIGHLIGHT_END.index);
    if end < begin {
        highlight_range::NONE
    } else {
        mode
    }
}

/// Lays the rows out again.
pub(crate) fn bump(e: &Engine, st: &State) {
    if let Some(n) = st.trigger.get() {
        let g = e.graph();
        let now = g.get_untracked(n).to_number() as i32;
        g.set(n, Value::Int(now.wrapping_add(1)));
    }
}

/// Makes the view lay out its rows from now on.
fn start(cx: &Cx) -> Result<(), EngineError> {
    let (e, me) = (cx.engine, cx.this);
    let st = state_of(e, me).ok_or_else(|| EngineError::dead(me))?;
    let scope = e.object_scope(me).ok_or_else(|| EngineError::dead(me))?;
    st.trigger
        .set(Some(e.graph().property(scope, Value::Int(0))?));
    let delegates = ViewDelegates::new(me, Rc::new(layout::Host(me)));
    *st.delegates.borrow_mut() = Some(delegates.clone());

    // A new model or delegate: every row is made again, and a model that
    // replaces one starts from its first row (qml6).
    let source = st.clone();
    e.effect_in(me, move |e| {
        let model = e.get(me, PathView::MODEL).unwrap_or_default();
        let delegate = e.get(me, PathView::DELEGATE).ok().flatten();
        e.untrack(|| {
            let model =
                models::model_of(e, &model).unwrap_or_else(|| e.model_from_value(&model, Some(me)));
            layout::release_all(e, me, &source);
            delegates.set_source(e, Some(model), delegate);
            motion::sourced(e, me, &source);
            bump(e, &source);
        });
    })?;

    let lay = st.clone();
    e.effect_in(me, move |e| {
        layout::read_inputs(e, me, &lay);
        e.untrack(|| layout::pass(e, me, &lay));
        layout::read_sizes(e, me, &lay);
    })?;

    e.connect_by_name(me, "currentIndexChanged", move |e, _| {
        motion::current_changed(e, me);
    })?;
    e.connect_by_name(me, "offsetChanged", move |e, _| {
        motion::offset_changed(e, me);
    })?;
    Ok(())
}

#[qml_impl]
impl PathView {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        start(cx)
    }

    #[qml(signal)]
    fn movement_started();
    #[qml(signal)]
    fn movement_ended();
    #[qml(signal)]
    fn flick_started();
    #[qml(signal)]
    fn flick_ended();
    #[qml(signal)]
    fn drag_started();
    #[qml(signal)]
    fn drag_ended();

    /// `incrementCurrentIndex()`: the next row is the current one, the
    /// first after the last.
    #[qml(invokable, name = "incrementCurrentIndex")]
    fn increment_current_index(cx: &Cx) {
        motion::step(cx.engine, cx.this, 1);
    }

    #[qml(invokable, name = "decrementCurrentIndex")]
    fn decrement_current_index(cx: &Cx) {
        motion::step(cx.engine, cx.this, -1);
    }

    /// `positionViewAtIndex(index, mode)`: puts row `index` where `mode`
    /// says on the path, at once.
    #[qml(invokable, name = "positionViewAtIndex")]
    fn position_view_at_index(cx: &Cx, index: i32, mode: i32) {
        motion::position_at(cx.engine, cx.this, index, mode);
    }

    /// `indexAt(x, y)`: the row whose delegate is at a point of the view;
    /// -1 for none.
    #[qml(invokable, name = "indexAt")]
    fn index_at(cx: &Cx, x: f64, y: f64) -> i32 {
        layout::hit(cx.engine, cx.this, x, y).map_or(-1, |(i, _)| i as i32)
    }

    /// `itemAt(x, y)`: the delegate at a point of the view.
    #[qml(invokable, name = "itemAt")]
    fn item_at(cx: &Cx, x: f64, y: f64) -> Value {
        layout::hit(cx.engine, cx.this, x, y).map_or(Value::Null, |(_, o)| Value::Object(o))
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
}

/// Registers `PathView` and what moves it.
pub(crate) fn register(engine: &Engine) {
    engine.register_type(
        "QtQuick",
        Version::new(2, 15),
        MetaRef::Static(PathView::static_meta()),
    );
    crate::pointer_item::register_pointer_item(
        engine,
        MetaRef::Static(PathView::static_meta()),
        drag::hooks(),
    );
    engine.on_attached_created(|e, item, att| {
        if is_a::<PathViewAttached>(e, att) {
            tell_attached(e, item, att);
        }
        true
    });
}

pub(crate) use motion::{next_wake, tick};
