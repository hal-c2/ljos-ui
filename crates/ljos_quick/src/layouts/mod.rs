//! `QtQuick.Layouts`: `RowLayout`, `ColumnLayout`, `GridLayout`,
//! `StackLayout` and the attached `Layout`.
//!
//! A layout sizes and places its children. Each child says what it can
//! take (a minimum, a preferred and a maximum size per axis, from the
//! `Layout.` attached properties and its implicit size); the layout hands
//! out its own size between them and takes its implicit size from them.
//!
//! Two steps, both in the polish pass (pipeline step 6):
//!
//! - **measure** turns the children into a [`Plan`] and the layout's own
//!   hints. It runs when an input changed, and climbs to the parent layout
//!   when the hints did.
//! - **arrange** shares the layout's size among the plan's rows and
//!   columns and writes the children's geometry. It runs when the plan or
//!   the layout's size changed, and goes down into child layouts.
//!
//! The rules were worked out by experiment with `qml6` (Qt 6.11) and are
//! pinned by `features/layouts/*.feature`.

mod distribute;
mod place;
mod proxy;

pub use proxy::LayoutItemProxy;

use std::collections::HashMap;
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, MetaRef, ObjectId, PropIndex, QmlObject, QmlType, Value,
};

use crate::enums::{EnumTable, align, layout_direction};
use crate::item::{Item, is_a, is_unset, is_unset_tracked, num, write_computed, write_placed};
use crate::mirroring::mirrored;
use crate::positioners::{self, Hidden, flow};
use crate::repeater::Repeater;
use crate::tree::child_items;

use distribute::{Span, Total, Track, distribute, edges, round_half_up, total, tracks};
use place::{Slot, Want, place};

/// A span or a named cell beyond this is a mistake, not a layout.
const MAX_CELLS: f64 = 1024.0;

/// `Layout` (QtQuick.Layouts): the base of every layout, and the type
/// whose attached properties say how a child wants to be laid out.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Layouts",
    name = "Layout",
    extends = Item,
    attached = LayoutAttached,
    uncreatable
)]
pub struct Layout {
    #[qml(base)]
    pub item: Item,
    #[qml(skip)]
    state: State,
}

/// `RowLayout` (QtQuick.Layouts).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Layouts", name = "RowLayout", extends = Layout)]
pub struct RowLayout {
    #[qml(base)]
    pub layout: Layout,
    #[qml(notify)]
    pub spacing: f64,
    /// [`crate::enums::layout_direction`].
    #[qml(notify, enumeration = "Qt.LayoutDirection")]
    pub layout_direction: i32,
    #[qml(notify)]
    pub uniform_cell_sizes: bool,
}

/// `ColumnLayout` (QtQuick.Layouts).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Layouts", name = "ColumnLayout", extends = Layout)]
pub struct ColumnLayout {
    #[qml(base)]
    pub layout: Layout,
    #[qml(notify)]
    pub spacing: f64,
    /// [`crate::enums::layout_direction`].
    #[qml(notify, enumeration = "Qt.LayoutDirection")]
    pub layout_direction: i32,
    #[qml(notify)]
    pub uniform_cell_sizes: bool,
}

/// `GridLayout` (QtQuick.Layouts).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Layouts",
    name = "GridLayout",
    extends = Layout,
    enums = grid_enums
)]
pub struct GridLayout {
    #[qml(base)]
    pub layout: Layout,
    #[qml(notify)]
    pub row_spacing: f64,
    #[qml(notify)]
    pub column_spacing: f64,
    /// Below 1: no limit.
    #[qml(notify)]
    pub rows: i32,
    /// Below 1: no limit.
    #[qml(notify)]
    pub columns: i32,
    /// [`crate::positioners::flow`].
    #[qml(notify, enumeration = "Flow")]
    pub flow: i32,
    /// [`crate::enums::layout_direction`].
    #[qml(notify, enumeration = "Qt.LayoutDirection")]
    pub layout_direction: i32,
    #[qml(notify)]
    pub uniform_cell_widths: bool,
    #[qml(notify)]
    pub uniform_cell_heights: bool,
}

/// `StackLayout` (QtQuick.Layouts).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Layouts", name = "StackLayout", extends = Layout)]
pub struct StackLayout {
    #[qml(base)]
    pub layout: Layout,
    /// The child that is shown; -1 while there is none.
    #[qml(notify)]
    pub current_index: i32,
    #[qml(notify, readonly)]
    pub count: i32,
}

/// Qt's default spacing between the cells of a layout.
const DEFAULT_SPACING: f64 = 5.0;

impl Default for RowLayout {
    fn default() -> Self {
        Self {
            layout: Layout::default(),
            spacing: DEFAULT_SPACING,
            layout_direction: layout_direction::LEFT_TO_RIGHT,
            uniform_cell_sizes: false,
        }
    }
}

impl Default for ColumnLayout {
    fn default() -> Self {
        Self {
            layout: Layout::default(),
            spacing: DEFAULT_SPACING,
            layout_direction: layout_direction::LEFT_TO_RIGHT,
            uniform_cell_sizes: false,
        }
    }
}

impl Default for GridLayout {
    fn default() -> Self {
        Self {
            layout: Layout::default(),
            row_spacing: DEFAULT_SPACING,
            column_spacing: DEFAULT_SPACING,
            rows: -1,
            columns: -1,
            flow: flow::LEFT_TO_RIGHT,
            layout_direction: layout_direction::LEFT_TO_RIGHT,
            uniform_cell_widths: false,
            uniform_cell_heights: false,
        }
    }
}

impl Default for StackLayout {
    fn default() -> Self {
        Self {
            layout: Layout::default(),
            current_index: -1,
            count: 0,
        }
    }
}

fn grid_enums() -> EnumTable {
    vec![(
        "Flow",
        vec![
            ("LeftToRight", i64::from(flow::LEFT_TO_RIGHT)),
            ("TopToBottom", i64::from(flow::TOP_TO_BOTTOM)),
        ],
    )]
}

/// What a side margin of a cell is while it has no value of its own.
fn all_margins(engine: &Engine, attached: ObjectId, _side: PropIndex) -> Value {
    engine.read(attached, LayoutAttached::MARGINS.index)
}

/// `Layout.margins` written: a side never given a value reads it, and is
/// still never given one. A binding would make the side count as set, and
/// a Behavior's start is what gives the side a value of its own.
fn margins_written(cx: &Cx, m: f64) -> Option<f64> {
    let (e, att) = (cx.engine, cx.this);
    for side in [
        LayoutAttached::LEFT_MARGIN.index,
        LayoutAttached::TOP_MARGIN.index,
        LayoutAttached::RIGHT_MARGIN.index,
        LayoutAttached::BOTTOM_MARGIN.index,
    ] {
        if is_unset(e, att, side) {
            let _ = e.reset_to(att, side, Value::Real(m));
        }
    }
    Some(m)
}

/// What `Layout.` attaches to a child of a layout.
///
/// A property counts only once it was given a value: until then the
/// layout uses the child's own sizes, and a layout inside a layout fills.
#[derive(Debug, QmlObject)]
#[qml(name = "LayoutAttached")]
pub struct LayoutAttached {
    #[qml(notify)]
    pub fill_width: bool,
    #[qml(notify)]
    pub fill_height: bool,
    /// Below 0: the implicit width.
    #[qml(notify)]
    pub preferred_width: f64,
    #[qml(notify)]
    pub preferred_height: f64,
    #[qml(notify)]
    pub minimum_width: f64,
    #[qml(notify)]
    pub minimum_height: f64,
    #[qml(notify)]
    pub maximum_width: f64,
    #[qml(notify)]
    pub maximum_height: f64,
    /// [`crate::enums::align`] flags.
    #[qml(notify, flags = "Qt.Alignment")]
    pub alignment: i32,
    #[qml(notify)]
    pub row: i32,
    #[qml(notify)]
    pub column: i32,
    #[qml(notify)]
    pub row_span: i32,
    #[qml(notify)]
    pub column_span: i32,
    #[qml(notify, write = margins_written)]
    pub margins: f64,
    /// The side margins follow `margins` until they get a value.
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_margins))]
    pub left_margin: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_margins))]
    pub top_margin: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_margins))]
    pub right_margin: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_margins))]
    pub bottom_margin: f64,
    /// Below 1: the child takes extra room by its preferred size.
    #[qml(notify)]
    pub horizontal_stretch_factor: i32,
    #[qml(notify)]
    pub vertical_stretch_factor: i32,
    #[qml(skip)]
    proxy: proxy::ProxyAttachedState,
}

impl Default for LayoutAttached {
    fn default() -> Self {
        Self {
            fill_width: false,
            fill_height: false,
            preferred_width: -1.0,
            preferred_height: -1.0,
            minimum_width: 0.0,
            minimum_height: 0.0,
            maximum_width: f64::INFINITY,
            maximum_height: f64::INFINITY,
            alignment: 0,
            row: 0,
            column: 0,
            row_span: 1,
            column_span: 1,
            margins: 0.0,
            left_margin: 0.0,
            top_margin: 0.0,
            right_margin: 0.0,
            bottom_margin: 0.0,
            horizontal_stretch_factor: -1,
            vertical_stretch_factor: -1,
            proxy: proxy::ProxyAttachedState::default(),
        }
    }
}

fn attacher() -> MetaRef {
    MetaRef::Static(Layout::static_meta())
}

/// The two axes: every per-axis pair below is `[horizontal, vertical]`.
const H: usize = 0;
const V: usize = 1;

/// What a child said through `Layout.`; `None` where it said nothing.
#[derive(Clone, Copy, Debug)]
struct Attached {
    fill: [Option<bool>; 2],
    pref: [Option<f64>; 2],
    min: [Option<f64>; 2],
    max: [Option<f64>; 2],
    stretch: [i32; 2],
    align: i32,
    row: Option<usize>,
    column: Option<usize>,
    row_span: usize,
    column_span: usize,
    /// Left, top, right, bottom.
    margins: [f64; 4],
}

impl Default for Attached {
    fn default() -> Self {
        Self {
            fill: [None; 2],
            pref: [None; 2],
            min: [None; 2],
            max: [None; 2],
            stretch: [-1; 2],
            align: 0,
            row: None,
            column: None,
            row_span: 1,
            column_span: 1,
            margins: [0.0; 4],
        }
    }
}

/// Reads everything a child said. Inside an effect this is also what makes
/// the layout follow each of the properties.
fn read_attached(e: &Engine, child: ObjectId) -> Attached {
    if is_a::<LayoutItemProxy>(e, child) {
        return proxy::read_proxy_attached(e, child);
    }
    let Some(a) = e.existing_attached_object(child, &attacher()) else {
        return Attached::default();
    };
    // Whether it is set is read as the value is: a value given later that
    // equals the default changes what the layout does, not what is read.
    let set = |p: PropIndex| {
        let v = e.read(a, p);
        (!is_unset_tracked(e, a, p)).then_some(v)
    };
    let size = |p: PropIndex| set(p).map(|v| v.to_number()).filter(|v| *v >= 0.0);
    let flag = |p: PropIndex| set(p).map(|v| v.to_boolean());
    let cell = |p: PropIndex| size(p).map(|v| v.min(MAX_CELLS) as usize);
    let span = |p: PropIndex| num(e, a, p).clamp(1.0, MAX_CELLS) as usize;
    let all = num(e, a, LayoutAttached::MARGINS.index);
    let side = |p: PropIndex| set(p).map_or(all, |v| v.to_number());
    Attached {
        fill: [
            flag(LayoutAttached::FILL_WIDTH.index),
            flag(LayoutAttached::FILL_HEIGHT.index),
        ],
        pref: [
            size(LayoutAttached::PREFERRED_WIDTH.index),
            size(LayoutAttached::PREFERRED_HEIGHT.index),
        ],
        min: [
            size(LayoutAttached::MINIMUM_WIDTH.index),
            size(LayoutAttached::MINIMUM_HEIGHT.index),
        ],
        max: [
            size(LayoutAttached::MAXIMUM_WIDTH.index),
            size(LayoutAttached::MAXIMUM_HEIGHT.index),
        ],
        stretch: [
            num(e, a, LayoutAttached::HORIZONTAL_STRETCH_FACTOR.index) as i32,
            num(e, a, LayoutAttached::VERTICAL_STRETCH_FACTOR.index) as i32,
        ],
        align: num(e, a, LayoutAttached::ALIGNMENT.index) as i32,
        row: cell(LayoutAttached::ROW.index),
        column: cell(LayoutAttached::COLUMN.index),
        row_span: span(LayoutAttached::ROW_SPAN.index),
        column_span: span(LayoutAttached::COLUMN_SPAN.index),
        margins: [
            side(LayoutAttached::LEFT_MARGIN.index),
            side(LayoutAttached::TOP_MARGIN.index),
            side(LayoutAttached::RIGHT_MARGIN.index),
            side(LayoutAttached::BOTTOM_MARGIN.index),
        ],
    }
}

/// The four layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Row,
    Column,
    Grid,
    Stack,
}

fn kind(engine: &Engine, obj: ObjectId) -> Option<Kind> {
    if !is_a::<Layout>(engine, obj) {
        None
    } else if is_a::<RowLayout>(engine, obj) {
        Some(Kind::Row)
    } else if is_a::<ColumnLayout>(engine, obj) {
        Some(Kind::Column)
    } else if is_a::<GridLayout>(engine, obj) {
        Some(Kind::Grid)
    } else if is_a::<StackLayout>(engine, obj) {
        Some(Kind::Stack)
    } else {
        None
    }
}

/// The sizes a child can take along one axis, as the layout sees them: a
/// child that does not fill has all three alike.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Hint {
    min: f64,
    pref: f64,
    max: f64,
    stretch: i32,
}

/// One child with a place in the layout.
#[derive(Clone, Debug, PartialEq)]
struct Cell {
    item: ObjectId,
    slot: Slot,
    hint: [Hint; 2],
    /// Left, top, right, bottom.
    margins: [f64; 4],
    align: i32,
    /// A layout itself: arranged right after it got its size.
    layout: bool,
}

/// Everything arranging needs, worked out by measuring. Two equal plans
/// arrange alike in the same size.
#[derive(Debug, PartialEq)]
struct Plan {
    kind: Kind,
    cells: Vec<Cell>,
    /// Columns, then rows.
    tracks: [Vec<Track>; 2],
    spacing: [f64; 2],
    /// Right to left: `layoutDirection`, the other way round when mirrored.
    rtl: bool,
    /// A stack's `currentIndex`.
    current: i64,
}

/// X, y, width and height of a child.
type Geometry = [f64; 4];

/// What a layout remembers between polishes.
#[derive(Debug, Default)]
struct State {
    /// An input changed since the last measure.
    dirty: bool,
    hidden: Hidden,
    /// The width and height every child had when the layout first saw it:
    /// its preferred size while it has no implicit one. Qt reads the size
    /// once too, as the layout then owns it.
    seen: HashMap<ObjectId, [f64; 2]>,
    plan: Option<Rc<Plan>>,
    /// The layout's own smallest, preferred and largest size.
    hints: [Total; 2],
    /// The plan changed since it was last arranged.
    stale: bool,
    /// Measured again since the children were last looked at: one of them
    /// may have left the place it was given.
    check: bool,
    /// The size the plan was last arranged in.
    arranged: Option<(f64, f64)>,
    placed: Vec<(ObjectId, Geometry)>,
    runs: u64,
}

fn state<R>(
    engine: &Engine,
    obj: ObjectId,
    f: impl FnOnce(&mut State) -> R,
) -> Result<R, EngineError> {
    engine.with_mut(obj, |l: &mut Layout| f(&mut l.state))
}

/// How many times the layout shared out its size among its children. For
/// tests and tools: an idle scene never adds to it.
pub fn layout_runs(engine: &Engine, obj: ObjectId) -> u64 {
    state(engine, obj, |s| s.runs).unwrap_or(0)
}

/// Whether the scene polishes a type through this module: a layout, or
/// one of the positioners the scene has no kind of its own for. Asked of
/// the type, once (the scene's item class), not of each item.
pub(crate) fn managed_type(m: &ljos_engine::MetaObject) -> bool {
    let layout = m.inherits(Layout::static_meta())
        && [
            RowLayout::static_meta(),
            ColumnLayout::static_meta(),
            GridLayout::static_meta(),
            StackLayout::static_meta(),
        ]
        .into_iter()
        .any(|t| m.inherits(t));
    layout
        || m.inherits(positioners::Grid::static_meta())
        || m.inherits(positioners::Flow::static_meta())
}

/// A child that gets its `Layout` object after its layout was measured
/// makes the layout measure again.
pub(crate) fn register(engine: &Engine) {
    engine.on_attached_created_of::<LayoutAttached>(|e, item, att| {
        proxy::setup_proxy_attached(e, item, att);
        if let Some(parent) = e.untrack(|| e.visual_parent(item))
            && kind(e, parent).is_some()
        {
            // Not worth a warning: the layout then sees the object the
            // next time anything else changes.
            let _ = e.notify(parent, Item::CHILDREN.index);
        }
        true
    });
}

/// Reads every input of a layout. The scene calls it inside an effect, so
/// this is also the dependency list that schedules polish.
pub(crate) fn read_inputs(e: &Engine, obj: ObjectId) {
    if let Some(which) = positioners::which(e, obj) {
        positioners::track(e, obj, which);
        return;
    }
    let Some(kind) = kind(e, obj) else {
        return;
    };
    // The effect runs when an input changed, and only then.
    let _ = state(e, obj, |s| s.dirty = true);
    for p in own_props(kind) {
        e.read(obj, *p);
    }
    e.read(obj, Item::WIDTH.index);
    e.read(obj, Item::HEIGHT.index);
    e.read(obj, Item::VISIBLE.index);
    mirrored(e, obj);
    for c in child_items(e, obj) {
        // A stack shows and hides its children itself.
        if kind != Kind::Stack {
            e.read(c, Item::VISIBLE.index);
        }
        // Not the child's width and height: the layout sets them.
        e.read(c, Item::IMPLICIT_WIDTH.index);
        e.read(c, Item::IMPLICIT_HEIGHT.index);
        read_attached(e, c);
    }
}

fn own_props(kind: Kind) -> &'static [PropIndex] {
    use std::sync::OnceLock;
    static PROPS: OnceLock<[Vec<PropIndex>; 4]> = OnceLock::new();
    let all = PROPS.get_or_init(|| {
        [
            vec![
                RowLayout::SPACING.index,
                RowLayout::LAYOUT_DIRECTION.index,
                RowLayout::UNIFORM_CELL_SIZES.index,
            ],
            vec![
                ColumnLayout::SPACING.index,
                ColumnLayout::LAYOUT_DIRECTION.index,
                ColumnLayout::UNIFORM_CELL_SIZES.index,
            ],
            vec![
                GridLayout::ROW_SPACING.index,
                GridLayout::COLUMN_SPACING.index,
                GridLayout::ROWS.index,
                GridLayout::COLUMNS.index,
                GridLayout::FLOW.index,
                GridLayout::LAYOUT_DIRECTION.index,
                GridLayout::UNIFORM_CELL_WIDTHS.index,
                GridLayout::UNIFORM_CELL_HEIGHTS.index,
            ],
            vec![StackLayout::CURRENT_INDEX.index],
        ]
    });
    match kind {
        Kind::Row => &all[0],
        Kind::Column => &all[1],
        Kind::Grid => &all[2],
        Kind::Stack => &all[3],
    }
}

/// Lays out a layout, or positions the children of a `Grid` or `Flow`.
pub(crate) fn polish(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    if let Some(which) = positioners::which(engine, obj) {
        return positioners::polish(engine, obj, which);
    }
    let Some(kind) = kind(engine, obj) else {
        return Ok(());
    };
    engine.untrack(|| polish_layout(engine, obj, kind))
}

fn polish_layout(e: &Engine, obj: ObjectId, kind: Kind) -> Result<(), EngineError> {
    let changed = measure(e, obj, kind)?;
    if changed
        && let Some(parent) = e.visual_parent(obj)
        && let Some(parent_kind) = self::kind(e, parent)
    {
        // What this layout can take changed, so the parent has to share
        // its size out again; it arranges this layout when it has.
        state(e, parent, |s| s.dirty = true)?;
        polish_layout(e, parent, parent_kind)?;
    }
    // Still to do when there is no parent layout, when the parent saw no
    // reason to arrange, and when this layout is hidden and so has no
    // cell in the parent: a hidden layout still places its children.
    arrange(e, obj)
}

/// The own settings a plan is made from.
struct Settings {
    spacing: [f64; 2],
    uniform: [bool; 2],
    direction: Option<PropIndex>,
}

fn settings(e: &Engine, obj: ObjectId, kind: Kind) -> Settings {
    let flag = |p: PropIndex| e.read(obj, p).to_boolean();
    match kind {
        Kind::Row => Settings {
            spacing: [num(e, obj, RowLayout::SPACING.index), 0.0],
            uniform: [flag(RowLayout::UNIFORM_CELL_SIZES.index), false],
            direction: Some(RowLayout::LAYOUT_DIRECTION.index),
        },
        Kind::Column => Settings {
            spacing: [0.0, num(e, obj, ColumnLayout::SPACING.index)],
            uniform: [false, flag(ColumnLayout::UNIFORM_CELL_SIZES.index)],
            direction: Some(ColumnLayout::LAYOUT_DIRECTION.index),
        },
        Kind::Grid => Settings {
            spacing: [
                num(e, obj, GridLayout::COLUMN_SPACING.index),
                num(e, obj, GridLayout::ROW_SPACING.index),
            ],
            uniform: [
                flag(GridLayout::UNIFORM_CELL_WIDTHS.index),
                flag(GridLayout::UNIFORM_CELL_HEIGHTS.index),
            ],
            direction: Some(GridLayout::LAYOUT_DIRECTION.index),
        },
        Kind::Stack => Settings {
            spacing: [0.0; 2],
            uniform: [false; 2],
            direction: None,
        },
    }
}

/// What one child can take along one axis.
///
/// Preferred: `Layout.preferredWidth`, else the implicit width rounded up,
/// else the width the child had when the layout first saw it. The maximum
/// wins over the minimum, and both bound the preferred size. A nested
/// layout's maximum is its children's unless told; a told minimum or
/// preferred size raises it. A child that does not fill is as large as
/// it prefers, no more and no less.
fn hint(a: &Attached, axis: usize, implicit: f64, seen: f64, own: Option<Total>) -> Hint {
    let from_implicit = || {
        let v = implicit.ceil();
        if v > 0.0 { v } else { seen }
    };
    let pref = a.pref[axis].unwrap_or_else(from_implicit);
    // A layout is as small and as large as its own children let it be,
    // and fills unless told not to. A size it is told to take, smallest
    // or preferred, raises the largest its children allow; only a
    // maximum it is told caps it.
    let max = a.max[axis].unwrap_or_else(|| {
        let told = a.min[axis].unwrap_or(0.0).max(a.pref[axis].unwrap_or(0.0));
        own.map_or(f64::INFINITY, |t| t.max.max(told))
    });
    let min = a.min[axis].unwrap_or(own.map_or(0.0, |t| t.min)).min(max);
    let pref = pref.clamp(min, max);
    if a.fill[axis].unwrap_or(own.is_some()) {
        Hint {
            min,
            pref,
            max,
            stretch: a.stretch[axis],
        }
    } else {
        Hint {
            min: pref,
            pref,
            max: pref,
            stretch: -1,
        }
    }
}

/// Works out the plan and the layout's own hints, if an input changed.
/// Says whether the hints did: then the parent layout has to know.
fn measure(e: &Engine, obj: ObjectId, kind: Kind) -> Result<bool, EngineError> {
    let (dirty, planned) = state(e, obj, |s| (std::mem::take(&mut s.dirty), s.plan.is_some()))?;
    if !dirty && planned {
        return Ok(false);
    }
    let children: Vec<ObjectId> = child_items(e, obj)
        .into_iter()
        .filter(|c| e.is_alive(*c) && !is_a::<Repeater>(e, *c))
        .collect();
    let shown: Vec<ObjectId> = if kind == Kind::Stack {
        children.clone()
    } else {
        let mut hidden = state(e, obj, |s| std::mem::take(&mut s.hidden))?;
        let visible = hidden.update(e, obj, &children);
        state(e, obj, |s| s.hidden = hidden)?;
        children
            .iter()
            .zip(visible)
            .filter_map(|(c, v)| v.then_some(*c))
            .collect()
    };
    let mut seen = state(e, obj, |s| std::mem::take(&mut s.seen))?;
    seen.retain(|c, _| children.contains(c));

    let set = settings(e, obj, kind);
    let mut cells = Vec::with_capacity(shown.len());
    let mut wants = Vec::with_capacity(shown.len());
    for c in &shown {
        let first = *seen.entry(*c).or_insert_with(|| {
            [
                num(e, *c, Item::WIDTH.index),
                num(e, *c, Item::HEIGHT.index),
            ]
        });
        let child_kind = self::kind(e, *c);
        let own = match child_kind {
            Some(k) => {
                measure(e, *c, k)?;
                Some(state(e, *c, |s| s.hints)?)
            }
            None => None,
        };
        let a = read_attached(e, *c);
        // A stack's pages fill it unless told not to.
        let a = if kind == Kind::Stack {
            Attached {
                fill: [a.fill[H].or(Some(true)), a.fill[V].or(Some(true))],
                ..a
            }
        } else {
            a
        };
        let implicit = [
            num(e, *c, Item::IMPLICIT_WIDTH.index),
            num(e, *c, Item::IMPLICIT_HEIGHT.index),
        ];
        cells.push(Cell {
            item: *c,
            slot: Slot {
                row: 0,
                column: 0,
                row_span: 1,
                column_span: 1,
            },
            hint: [
                hint(&a, H, implicit[H], first[H], own.map(|t| t[H])),
                hint(&a, V, implicit[V], first[V], own.map(|t| t[V])),
            ],
            margins: a.margins,
            align: a.align,
            layout: child_kind.is_some(),
        });
        wants.push(Want {
            row: a.row,
            column: a.column,
            row_span: a.row_span,
            column_span: a.column_span,
        });
    }
    state(e, obj, |s| s.seen = seen)?;

    match kind {
        Kind::Row => {
            for (i, c) in cells.iter_mut().enumerate() {
                c.slot.column = i;
            }
        }
        Kind::Column => {
            for (i, c) in cells.iter_mut().enumerate() {
                c.slot.row = i;
            }
        }
        Kind::Grid => {
            let limit = |p: PropIndex| usize::try_from(num(e, obj, p) as i64).ok();
            let top_to_bottom = num(e, obj, GridLayout::FLOW.index) as i32 == flow::TOP_TO_BOTTOM;
            let slots = place(
                &wants,
                limit(GridLayout::ROWS.index),
                limit(GridLayout::COLUMNS.index),
                top_to_bottom,
            );
            for (c, slot) in cells.iter_mut().zip(slots) {
                c.slot = slot;
            }
        }
        Kind::Stack => {}
    }

    let boxed = |c: &Cell, axis: usize| {
        let m = c.margins[axis] + c.margins[axis + 2];
        let h = c.hint[axis];
        (h.min + m, h.pref + m, h.max + m)
    };
    let (plan_tracks, hints) = if kind == Kind::Stack {
        // As large as its largest page wants, and as large as it gets.
        let of = |axis: usize| {
            let mut t = Total {
                min: 0.0,
                pref: 0.0,
                max: f64::INFINITY,
            };
            for c in &cells {
                let (min, pref, _) = boxed(c, axis);
                t.min = t.min.max(min);
                t.pref = t.pref.max(pref);
            }
            t
        };
        ([Vec::new(), Vec::new()], [of(H), of(V)])
    } else {
        let axis = |axis: usize| {
            let spans: Vec<Span> = cells
                .iter()
                .map(|c| {
                    let (min, pref, max) = boxed(c, axis);
                    let (start, len) = if axis == H {
                        (c.slot.column, c.slot.column_span)
                    } else {
                        (c.slot.row, c.slot.row_span)
                    };
                    Span {
                        start,
                        len,
                        min,
                        pref,
                        max,
                        stretch: c.hint[axis].stretch,
                    }
                })
                .collect();
            let count = spans.iter().map(|s| s.start + s.len).max().unwrap_or(0);
            tracks(count, &spans, set.spacing[axis], set.uniform[axis])
        };
        let t = [axis(H), axis(V)];
        let hints = [total(&t[H], set.spacing[H]), total(&t[V], set.spacing[V])];
        (t, hints)
    };

    let mut current = -1;
    if kind == Kind::Stack {
        let count = i32::try_from(cells.len()).unwrap_or(i32::MAX);
        write_computed(e, obj, StackLayout::COUNT.index, Value::Int(count))?;
        // The first page shows by itself; after that the index is the
        // user's.
        if count > 0 && is_unset(e, obj, StackLayout::CURRENT_INDEX.index) {
            write_computed(e, obj, StackLayout::CURRENT_INDEX.index, Value::Int(0))?;
        }
        current = num(e, obj, StackLayout::CURRENT_INDEX.index) as i64;
    }
    let rtl = set.direction.is_some_and(|p| {
        (num(e, obj, p) as i32 == layout_direction::RIGHT_TO_LEFT) != mirrored(e, obj)
    });
    let plan = Rc::new(Plan {
        kind,
        cells,
        tracks: plan_tracks,
        spacing: set.spacing,
        rtl,
        current,
    });
    let changed = state(e, obj, |s| {
        if s.plan.as_deref() != Some(&*plan) {
            s.stale = true;
            s.plan = Some(plan);
        }
        s.check = true;
        let changed = s.hints != hints;
        s.hints = hints;
        changed
    })?;
    write_computed(
        e,
        obj,
        Item::IMPLICIT_WIDTH.index,
        Value::Real(hints[H].pref),
    )?;
    write_computed(
        e,
        obj,
        Item::IMPLICIT_HEIGHT.index,
        Value::Real(hints[V].pref),
    )?;
    Ok(changed)
}

/// Shares the layout's size out and writes the children's geometry, if the
/// plan or the size changed; then does the same for the layouts among the
/// children.
fn arrange(e: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    // Read after measuring: a layout nothing sized follows the implicit
    // size it was just given.
    let size = (
        num(e, obj, Item::WIDTH.index),
        num(e, obj, Item::HEIGHT.index),
    );
    let visible = e.read(obj, Item::VISIBLE.index).to_boolean();
    let work = state(e, obj, |s| {
        let plan = s.plan.clone()?;
        let fresh = s.stale || s.arranged != Some(size);
        let check = std::mem::take(&mut s.check);
        if fresh {
            s.placed = compute(&plan, size.0, size.1);
            s.runs += 1;
            s.arranged = Some(size);
            // A hidden stack cannot show its page yet: every child reads
            // invisible while it is. It arranges again once it shows.
            s.stale = plan.kind == Kind::Stack && !visible;
        }
        (fresh || check).then(|| (plan, s.placed.clone()))
    })?;
    let Some((plan, placed)) = work else {
        return Ok(());
    };
    e.batch(|| -> Result<(), EngineError> {
        for (c, g) in &placed {
            if !e.is_alive(*c) {
                continue;
            }
            let props = [
                Item::X.index,
                Item::Y.index,
                Item::WIDTH.index,
                Item::HEIGHT.index,
            ];
            for (p, v) in props.into_iter().zip(g) {
                write_placed(e, *c, p, Value::Real(*v))?;
            }
            proxy::sync_placed_proxy(e, *c, g[2], g[3])?;
        }
        if plan.kind == Kind::Stack && visible {
            for (i, c) in plan.cells.iter().enumerate() {
                let on = i64::try_from(i).is_ok_and(|i| i == plan.current);
                if e.is_alive(c.item) {
                    write_placed(e, c.item, Item::VISIBLE.index, Value::Bool(on))?;
                }
            }
        }
        Ok(())
    })?;
    for c in plan.cells.iter().filter(|c| c.layout) {
        if e.is_alive(c.item) {
            arrange(e, c.item)?;
        }
    }
    Ok(())
}

/// Where a child smaller than its cell goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Align {
    Near,
    Center,
    Far,
}

/// Left unless the child says otherwise; the other way round right to
/// left.
fn h_align(flags: i32, rtl: bool) -> Align {
    let a = if flags & align::RIGHT != 0 {
        Align::Far
    } else if flags & align::H_CENTER != 0 {
        Align::Center
    } else {
        Align::Near
    };
    match (a, rtl) {
        (Align::Near, true) => Align::Far,
        (Align::Far, true) => Align::Near,
        (a, _) => a,
    }
}

/// Vertically centred unless the child says otherwise.
fn v_align(flags: i32) -> Align {
    if flags & align::TOP != 0 {
        Align::Near
    } else if flags & align::BOTTOM != 0 {
        Align::Far
    } else {
        Align::Center
    }
}

/// A child in its cell `start..end`, margins `before` and `after`: its
/// position and size. It takes the room there is, between its minimum and
/// maximum; if that leaves room it is aligned, on a whole pixel.
fn fit(start: f64, end: f64, before: f64, after: f64, hint: &Hint, align: Align) -> (f64, f64) {
    let room = (end - start - before - after).max(0.0);
    let size = room.min(hint.max).max(hint.min);
    let free = room - size;
    let at = if free <= 0.0 {
        start + before
    } else {
        match align {
            Align::Near => start + before,
            Align::Center => start + before + free / 2.0,
            Align::Far => end - after - size,
        }
    };
    (round_half_up(at), size)
}

/// The geometry of every child of `plan` in a layout `width` by `height`.
fn compute(plan: &Plan, width: f64, height: f64) -> Vec<(ObjectId, Geometry)> {
    if plan.kind == Kind::Stack {
        // Only the page that shows is sized; the others keep what they
        // had.
        return usize::try_from(plan.current)
            .ok()
            .and_then(|i| plan.cells.get(i))
            .map(|c| {
                let (x, w) = fit(
                    0.0,
                    width,
                    c.margins[0],
                    c.margins[2],
                    &c.hint[H],
                    h_align(c.align, false),
                );
                let (y, h) = fit(
                    0.0,
                    height,
                    c.margins[1],
                    c.margins[3],
                    &c.hint[V],
                    v_align(c.align),
                );
                (c.item, [x, y, w, h])
            })
            .into_iter()
            .collect();
    }
    let lines = |axis: usize, size: f64| {
        let t = &plan.tracks[axis];
        let sizes = distribute(t, plan.spacing[axis], size);
        edges(t, &sizes, plan.spacing[axis])
    };
    let (columns, rows) = (lines(H, width), lines(V, height));
    // From the first to the last row or column the child covers that has
    // a size: the others under a span take no room.
    let cover = |axis: usize, edges: &[(f64, f64)], start: usize, len: usize| {
        let mut used = (start..start + len)
            .filter(|i| plan.tracks[axis].get(*i).is_some_and(|t| t.used))
            .filter_map(|i| edges.get(i));
        let first = used.next().copied().unwrap_or((0.0, 0.0));
        let last = used.next_back().copied().unwrap_or(first);
        (first.0, last.1)
    };
    plan.cells
        .iter()
        .map(|c| {
            let (mut x0, mut x1) = cover(H, &columns, c.slot.column, c.slot.column_span);
            if plan.rtl {
                (x0, x1) = (width - x1, width - x0);
            }
            let (y0, y1) = cover(V, &rows, c.slot.row, c.slot.row_span);
            // Margins stay on their side right to left.
            let (x, w) = fit(
                x0,
                x1,
                c.margins[0],
                c.margins[2],
                &c.hint[H],
                h_align(c.align, plan.rtl),
            );
            let (y, h) = fit(
                y0,
                y1,
                c.margins[1],
                c.margins[3],
                &c.hint[V],
                v_align(c.align),
            );
            (c.item, [x, y, w, h])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILL: Hint = Hint {
        min: 0.0,
        pref: 0.0,
        max: f64::INFINITY,
        stretch: -1,
    };

    fn fixed(size: f64) -> Hint {
        Hint {
            min: size,
            pref: size,
            max: size,
            stretch: -1,
        }
    }

    #[test]
    fn a_child_fills_its_cell_between_its_bounds() {
        assert_eq!(
            fit(10.0, 110.0, 0.0, 0.0, &FILL, Align::Near),
            (10.0, 100.0)
        );
        let capped = Hint { max: 30.0, ..FILL };
        assert_eq!(fit(0.0, 61.0, 0.0, 0.0, &capped, Align::Far), (31.0, 30.0));
    }

    #[test]
    fn a_smaller_child_is_aligned_on_a_whole_pixel() {
        let h = fixed(25.0);
        assert_eq!(fit(0.0, 40.0, 0.0, 0.0, &h, Align::Center), (8.0, 25.0));
        assert_eq!(fit(0.0, 40.0, 0.0, 0.0, &h, Align::Far), (15.0, 25.0));
        assert_eq!(fit(0.0, 40.0, 3.0, 4.0, &h, Align::Near), (3.0, 25.0));
        assert_eq!(fit(0.0, 40.0, 3.0, 4.0, &h, Align::Far), (11.0, 25.0));
    }

    #[test]
    fn a_child_larger_than_its_cell_starts_at_the_cell() {
        assert_eq!(
            fit(5.0, 15.0, 2.0, 0.0, &fixed(50.0), Align::Far),
            (7.0, 50.0)
        );
    }

    #[test]
    fn a_child_that_does_not_fill_is_as_large_as_it_prefers() {
        let a = Attached {
            min: [Some(10.0); 2],
            max: [Some(90.0); 2],
            ..Attached::default()
        };
        assert_eq!(hint(&a, H, 50.2, 70.0, None), fixed(51.0));
        // No implicit size: the size it had.
        assert_eq!(hint(&a, H, 0.0, 70.0, None), fixed(70.0));
        // The bounds still hold.
        assert_eq!(hint(&a, H, 0.0, 170.0, None), fixed(90.0));
    }

    #[test]
    fn the_maximum_wins_over_the_minimum() {
        let a = Attached {
            fill: [Some(true); 2],
            pref: [Some(30.0); 2],
            min: [Some(40.0); 2],
            max: [Some(35.0); 2],
            ..Attached::default()
        };
        let h = hint(&a, H, 0.0, 0.0, None);
        assert_eq!((h.min, h.pref, h.max), (35.0, 35.0, 35.0));
    }

    #[test]
    fn a_layout_fills_between_what_its_children_take() {
        let own = Total {
            min: 75.0,
            pref: 75.0,
            max: f64::INFINITY,
        };
        let h = hint(&Attached::default(), H, 75.0, 0.0, Some(own));
        assert_eq!((h.min, h.pref, h.max), (75.0, 75.0, f64::INFINITY));
        let told = Attached {
            fill: [Some(false); 2],
            ..Attached::default()
        };
        assert_eq!(hint(&told, H, 75.0, 0.0, Some(own)), fixed(75.0));
    }

    #[test]
    fn a_layout_told_a_size_may_outgrow_its_children() {
        let own = Total {
            min: 16.0,
            pref: 16.0,
            max: 16.0,
        };
        let at = |a: Attached| {
            let h = hint(&a, H, 16.0, 0.0, Some(own));
            (h.min, h.pref, h.max)
        };
        let pref = |p| Attached {
            pref: [Some(p); 2],
            ..Attached::default()
        };
        assert_eq!(at(pref(22.0)), (16.0, 22.0, 22.0));
        // The children's minimum raises a smaller preferred size.
        assert_eq!(at(pref(10.0)), (16.0, 16.0, 16.0));
        let capped = Attached {
            max: [Some(16.0); 2],
            ..pref(22.0)
        };
        assert_eq!(at(capped), (16.0, 16.0, 16.0));
        let least = Attached {
            min: [Some(20.0); 2],
            ..Attached::default()
        };
        assert_eq!(at(least), (20.0, 20.0, 20.0));
    }
}
