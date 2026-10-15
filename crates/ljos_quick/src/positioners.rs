//! `Row`, `Column`, `Grid` and `Flow`, and the attached `Positioner`.
//!
//! The scene runs [`polish`] in the polish pass (pipeline step 6) for every
//! positioner whose inputs changed; nothing is laid out synchronously.
//!
//! A positioner places its children and takes its implicit size from them.
//! It never sizes a child. Children that are invisible by themselves or
//! have no width or height are left out, as the Qt docs say. The rules are
//! pinned by `features/layouts/{positioners,flow,grid,positioner-attached}`.

use std::collections::{HashMap, HashSet};

use ljos_engine::{
    Cx, Engine, EngineError, LogLevel, MetaRef, ObjectId, PropIndex, QmlObject, QmlType, Value,
    qml_impl,
};

use crate::enums::{EnumTable, align, layout_direction};
use crate::item::{Item, is_a, is_unset, num, size_follows_implicit, write_computed, write_placed};
use crate::mirroring::mirrored;
use crate::repeater::Repeater;
use crate::scene::SCENE_CATEGORY;
use crate::tree::child_items;

/// `Grid.flow` and `Flow.flow`.
pub mod flow {
    pub const LEFT_TO_RIGHT: i32 = 0;
    pub const TOP_TO_BOTTOM: i32 = 1;
}

/// `Row` (QtQuick).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Row", extends = Item)]
pub struct Row {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub spacing: f64,
    #[qml(notify, reset)]
    pub padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub left_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub right_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub top_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub bottom_padding: f64,
    /// [`crate::enums::layout_direction`].
    #[qml(notify, enumeration = "Qt.LayoutDirection")]
    pub layout_direction: i32,
    /// `layoutDirection`, the other way round under `LayoutMirroring`.
    #[qml(notify, readonly, enumeration = "Qt.LayoutDirection")]
    pub effective_layout_direction: i32,
    #[qml(skip)]
    hidden: Hidden,
    /// The children it last positioned, in order: what a `Positioner`
    /// object made later is told.
    #[qml(skip)]
    placed: Vec<ObjectId>,
    /// The transitions a positioner runs for its children: `populate` for
    /// those it has when it is first laid out, `add` for one that joins the
    /// layout later (made, or made visible), `move` for one the layout moves.
    #[qml(notify)]
    pub populate: Option<ObjectId>,
    #[qml(notify)]
    pub add: Option<ObjectId>,
    #[qml(notify, rename = "move")]
    pub move_transition: Option<ObjectId>,
    #[qml(skip)]
    memory: Memory,
    #[qml(skip)]
    across: Across,
}

/// `Column` (QtQuick).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Column", extends = Item)]
pub struct Column {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub spacing: f64,
    #[qml(notify, reset)]
    pub padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub left_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub right_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub top_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub bottom_padding: f64,
    #[qml(skip)]
    hidden: Hidden,
    /// The children it last positioned, in order: what a `Positioner`
    /// object made later is told.
    #[qml(skip)]
    placed: Vec<ObjectId>,
    /// The transitions a positioner runs for its children: `populate` for
    /// those it has when it is first laid out, `add` for one that joins the
    /// layout later (made, or made visible), `move` for one the layout moves.
    #[qml(notify)]
    pub populate: Option<ObjectId>,
    #[qml(notify)]
    pub add: Option<ObjectId>,
    #[qml(notify, rename = "move")]
    pub move_transition: Option<ObjectId>,
    #[qml(skip)]
    memory: Memory,
    #[qml(skip)]
    across: Across,
}

/// `Grid` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Grid", extends = Item, enums = grid_enums)]
pub struct Grid {
    #[qml(base)]
    pub item: Item,
    /// Below 1: as many as the children need.
    #[qml(notify)]
    pub rows: i32,
    /// Below 1: four, or as many as `rows` leaves.
    #[qml(notify)]
    pub columns: i32,
    #[qml(notify)]
    pub spacing: f64,
    /// Follows `spacing` until it is given a value.
    #[qml(notify, reset = OWN_SPACING_RESET)]
    pub row_spacing: f64,
    /// Follows `spacing` until it is given a value.
    #[qml(notify, reset = OWN_SPACING_RESET)]
    pub column_spacing: f64,
    /// [`flow`].
    #[qml(notify, enumeration = "Flow")]
    pub flow: i32,
    /// [`crate::enums::layout_direction`].
    #[qml(notify, enumeration = "Qt.LayoutDirection")]
    pub layout_direction: i32,
    #[qml(notify, readonly, enumeration = "Qt.LayoutDirection")]
    pub effective_layout_direction: i32,
    /// [`crate::enums::align`]: left, right or horizontal centre.
    #[qml(notify, enumeration = "HAlignment")]
    pub horizontal_item_alignment: i32,
    /// `horizontalItemAlignment` with left and right swapped when the grid
    /// runs right to left.
    #[qml(notify, readonly, enumeration = "HAlignment")]
    pub effective_horizontal_item_alignment: i32,
    /// [`crate::enums::align`]: top, bottom or vertical centre.
    #[qml(notify, enumeration = "VAlignment")]
    pub vertical_item_alignment: i32,
    #[qml(notify, reset)]
    pub padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub left_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub right_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub top_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub bottom_padding: f64,
    #[qml(skip)]
    hidden: Hidden,
    /// The children it last positioned, in order: what a `Positioner`
    /// object made later is told.
    #[qml(skip)]
    placed: Vec<ObjectId>,
    /// The transitions a positioner runs for its children: `populate` for
    /// those it has when it is first laid out, `add` for one that joins the
    /// layout later (made, or made visible), `move` for one the layout moves.
    #[qml(notify)]
    pub populate: Option<ObjectId>,
    #[qml(notify)]
    pub add: Option<ObjectId>,
    #[qml(notify, rename = "move")]
    pub move_transition: Option<ObjectId>,
    #[qml(skip)]
    memory: Memory,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            item: Item::default(),
            rows: -1,
            columns: -1,
            spacing: 0.0,
            row_spacing: -1.0,
            column_spacing: -1.0,
            flow: flow::LEFT_TO_RIGHT,
            layout_direction: layout_direction::LEFT_TO_RIGHT,
            effective_layout_direction: layout_direction::LEFT_TO_RIGHT,
            horizontal_item_alignment: align::LEFT,
            effective_horizontal_item_alignment: align::LEFT,
            vertical_item_alignment: align::TOP,
            padding: 0.0,
            left_padding: 0.0,
            right_padding: 0.0,
            top_padding: 0.0,
            bottom_padding: 0.0,
            hidden: Hidden::default(),
            placed: Vec::new(),
            populate: None,
            add: None,
            move_transition: None,
            memory: Memory::default(),
        }
    }
}

/// `Flow` (QtQuick).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Flow", extends = Item, enums = flow_enums)]
pub struct Flow {
    #[qml(base)]
    pub item: Item,
    /// [`flow`].
    #[qml(notify, enumeration = "Flow")]
    pub flow: i32,
    /// [`crate::enums::layout_direction`].
    #[qml(notify, enumeration = "Qt.LayoutDirection")]
    pub layout_direction: i32,
    #[qml(notify, readonly, enumeration = "Qt.LayoutDirection")]
    pub effective_layout_direction: i32,
    #[qml(notify)]
    pub spacing: f64,
    #[qml(notify, reset)]
    pub padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub left_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub right_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub top_padding: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_padding))]
    pub bottom_padding: f64,
    #[qml(skip)]
    hidden: Hidden,
    /// The children it last positioned, in order: what a `Positioner`
    /// object made later is told.
    #[qml(skip)]
    placed: Vec<ObjectId>,
    /// The transitions a positioner runs for its children: `populate` for
    /// those it has when it is first laid out, `add` for one that joins the
    /// layout later (made, or made visible), `move` for one the layout moves.
    #[qml(notify)]
    pub populate: Option<ObjectId>,
    #[qml(notify)]
    pub add: Option<ObjectId>,
    #[qml(notify, rename = "move")]
    pub move_transition: Option<ObjectId>,
    #[qml(skip)]
    memory: Memory,
}

/// `Positioner` (QtQuick): only ever used attached, by a child of a
/// positioner that wants to know its place.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Positioner",
    attached = PositionerAttached,
    uncreatable
)]
pub struct Positioner {}

/// What `Positioner.` attaches to a positioner's child.
#[derive(Debug, QmlObject)]
#[qml(name = "PositionerAttached")]
pub struct PositionerAttached {
    /// The child's place among the positioned children; -1 when it is not
    /// positioned.
    #[qml(notify, readonly)]
    pub index: i32,
    #[qml(notify, readonly)]
    pub is_first_item: bool,
    #[qml(notify, readonly)]
    pub is_last_item: bool,
}

impl Default for PositionerAttached {
    fn default() -> Self {
        Self {
            index: -1,
            is_first_item: false,
            is_last_item: false,
        }
    }
}

fn flow_keys() -> (&'static str, Vec<(&'static str, i64)>) {
    (
        "Flow",
        vec![
            ("LeftToRight", i64::from(flow::LEFT_TO_RIGHT)),
            ("TopToBottom", i64::from(flow::TOP_TO_BOTTOM)),
        ],
    )
}

fn flow_enums() -> EnumTable {
    vec![flow_keys()]
}

fn grid_enums() -> EnumTable {
    vec![
        flow_keys(),
        (
            "HAlignment",
            vec![
                ("AlignLeft", i64::from(align::LEFT)),
                ("AlignRight", i64::from(align::RIGHT)),
                ("AlignHCenter", i64::from(align::H_CENTER)),
            ],
        ),
        (
            "VAlignment",
            vec![
                ("AlignTop", i64::from(align::TOP)),
                ("AlignBottom", i64::from(align::BOTTOM)),
                ("AlignVCenter", i64::from(align::V_CENTER)),
            ],
        ),
    ]
}

#[qml_impl]
impl Row {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init(cx.engine, cx.this, Which::Row)
    }
}

#[qml_impl]
impl Column {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init(cx.engine, cx.this, Which::Column)
    }
}

#[qml_impl]
impl Grid {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init(cx.engine, cx.this, Which::Grid)
    }
}

#[qml_impl]
impl Flow {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init(cx.engine, cx.this, Which::Flow)
    }
}

/// Which axis a positioner stacks along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// The four positioners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Which {
    Row,
    Column,
    Grid,
    Flow,
}

/// Which positioner `obj` is, if any.
pub(crate) fn which(engine: &Engine, obj: ObjectId) -> Option<Which> {
    if is_a::<Row>(engine, obj) {
        Some(Which::Row)
    } else if is_a::<Column>(engine, obj) {
        Some(Which::Column)
    } else if is_a::<Grid>(engine, obj) {
        Some(Which::Grid)
    } else if is_a::<Flow>(engine, obj) {
        Some(Which::Flow)
    } else {
        None
    }
}

/// The children a positioner or layout last saw hidden by themselves.
///
/// Qt leaves out a child whose own `visible` is false, while a child that
/// is hidden only because the positioner is keeps its place. Ljós can read
/// the effective value only ([`Item`]), so the own one is remembered from
/// when the positioner was last visible. A positioner that was never
/// visible takes all its children to be.
#[derive(Debug, Default)]
pub(crate) struct Hidden(HashSet<ObjectId>);

impl Hidden {
    /// Whether each of `children` counts as visible.
    pub(crate) fn update(
        &mut self,
        engine: &Engine,
        parent: ObjectId,
        children: &[ObjectId],
    ) -> Vec<bool> {
        if engine.read(parent, Item::VISIBLE.index).to_boolean() {
            self.0.clear();
            children
                .iter()
                .map(|c| {
                    let visible = engine.read(*c, Item::VISIBLE.index).to_boolean();
                    if !visible {
                        self.0.insert(*c);
                    }
                    visible
                })
                .collect()
        } else {
            children.iter().map(|c| !self.0.contains(c)).collect()
        }
    }
}

/// What a Row or a Column remembers of the coordinate it does not place
/// along: the padding each child was last moved by. A Row leaves `y` to the
/// child and a Column `x`, apart from moving it by the padding on that side
/// and, when the padding changes, by the difference (as Qt does: a child
/// with `x: 7` in a Column with `leftPadding: 5` is at 12).
#[derive(Debug, Default)]
struct Across(HashMap<ObjectId, f64>);

impl Across {
    /// Moves every placed child by what the padding changed since it was
    /// last moved, and forgets the children that left.
    fn apply(
        &mut self,
        engine: &Engine,
        children: &[ObjectId],
        placed: &[Placed],
        prop: PropIndex,
        padding: f64,
    ) -> Result<(), EngineError> {
        self.0.retain(|c, _| children.contains(c));
        for k in placed {
            let before = self.0.insert(k.id, padding).unwrap_or(0.0);
            if before != padding {
                let at = num(engine, k.id, prop);
                write_placed(engine, k.id, prop, Value::Real(at + padding - before))?;
            }
        }
        Ok(())
    }
}

fn swap_across(engine: &Engine, obj: ObjectId, which: Which, with: &mut Across) {
    // A failure was logged when the other memory was swapped.
    let _ = match which {
        Which::Row => engine.with_mut(obj, |p: &mut Row| std::mem::swap(&mut p.across, with)),
        Which::Column => engine.with_mut(obj, |p: &mut Column| std::mem::swap(&mut p.across, with)),
        Which::Grid | Which::Flow => Ok(()),
    };
}

/// What a positioner remembers of its last layout, so that it knows which
/// children arrived and which moved.
#[derive(Debug, Default)]
struct Memory {
    /// Where each child was put, as the layout last wrote it.
    at: HashMap<ObjectId, [f64; 2]>,
    /// The first layout was done.
    populated: bool,
    /// Children a transition is moving, and the place it takes them to.
    going: HashMap<ObjectId, [f64; 2]>,
}

fn with_memory<R>(
    engine: &Engine,
    obj: ObjectId,
    which: Which,
    f: impl FnOnce(&mut Memory) -> R,
) -> Option<R> {
    match which {
        Which::Row => engine.with_mut(obj, |p: &mut Row| f(&mut p.memory)),
        Which::Column => engine.with_mut(obj, |p: &mut Column| f(&mut p.memory)),
        Which::Grid => engine.with_mut(obj, |p: &mut Grid| f(&mut p.memory)),
        Which::Flow => engine.with_mut(obj, |p: &mut Flow| f(&mut p.memory)),
    }
    .ok()
}

/// `[populate, add, move]` as the positioner has them.
fn transitions_of(engine: &Engine, obj: ObjectId, which: Which) -> [Option<ObjectId>; 3] {
    let read = |p: PropIndex| engine.read(obj, p).as_object();
    match which {
        Which::Row => [
            read(Row::POPULATE.index),
            read(Row::ADD.index),
            read(Row::MOVE_TRANSITION.index),
        ],
        Which::Column => [
            read(Column::POPULATE.index),
            read(Column::ADD.index),
            read(Column::MOVE_TRANSITION.index),
        ],
        Which::Grid => [
            read(Grid::POPULATE.index),
            read(Grid::ADD.index),
            read(Grid::MOVE_TRANSITION.index),
        ],
        Which::Flow => [
            read(Flow::POPULATE.index),
            read(Flow::ADD.index),
            read(Flow::MOVE_TRANSITION.index),
        ],
    }
}

/// Runs `f` on the list of children the positioner last positioned.
fn with_placed<R>(
    engine: &Engine,
    obj: ObjectId,
    which: Which,
    f: impl FnOnce(&mut Vec<ObjectId>) -> R,
) -> Option<R> {
    match which {
        Which::Row => engine.with_mut(obj, |p: &mut Row| f(&mut p.placed)),
        Which::Column => engine.with_mut(obj, |p: &mut Column| f(&mut p.placed)),
        Which::Grid => engine.with_mut(obj, |p: &mut Grid| f(&mut p.placed)),
        Which::Flow => engine.with_mut(obj, |p: &mut Flow| f(&mut p.placed)),
    }
    .ok()
}

/// Swaps the positioner's memory with `with`: the memory is taken out
/// while the engine is read, and put back after.
fn swap_hidden(engine: &Engine, obj: ObjectId, which: Which, with: &mut Hidden) {
    let r = match which {
        Which::Row => engine.with_mut(obj, |p: &mut Row| std::mem::swap(&mut p.hidden, with)),
        Which::Column => engine.with_mut(obj, |p: &mut Column| std::mem::swap(&mut p.hidden, with)),
        Which::Grid => engine.with_mut(obj, |p: &mut Grid| std::mem::swap(&mut p.hidden, with)),
        Which::Flow => engine.with_mut(obj, |p: &mut Flow| std::mem::swap(&mut p.hidden, with)),
    };
    if let Err(err) = r {
        engine.log(
            LogLevel::Warning,
            SCENE_CATEGORY,
            format!("positioner: {err}"),
        );
    }
}

struct Props {
    spacing: PropIndex,
    padding: PropIndex,
    left: PropIndex,
    right: PropIndex,
    top: PropIndex,
    bottom: PropIndex,
    /// `layoutDirection` and `effectiveLayoutDirection`; a Column has none.
    direction: Option<(PropIndex, PropIndex)>,
}

fn props(which: Which) -> Props {
    match which {
        Which::Row => Props {
            spacing: Row::SPACING.index,
            padding: Row::PADDING.index,
            left: Row::LEFT_PADDING.index,
            right: Row::RIGHT_PADDING.index,
            top: Row::TOP_PADDING.index,
            bottom: Row::BOTTOM_PADDING.index,
            direction: Some((
                Row::LAYOUT_DIRECTION.index,
                Row::EFFECTIVE_LAYOUT_DIRECTION.index,
            )),
        },
        Which::Column => Props {
            spacing: Column::SPACING.index,
            padding: Column::PADDING.index,
            left: Column::LEFT_PADDING.index,
            right: Column::RIGHT_PADDING.index,
            top: Column::TOP_PADDING.index,
            bottom: Column::BOTTOM_PADDING.index,
            direction: None,
        },
        Which::Grid => Props {
            spacing: Grid::SPACING.index,
            padding: Grid::PADDING.index,
            left: Grid::LEFT_PADDING.index,
            right: Grid::RIGHT_PADDING.index,
            top: Grid::TOP_PADDING.index,
            bottom: Grid::BOTTOM_PADDING.index,
            direction: Some((
                Grid::LAYOUT_DIRECTION.index,
                Grid::EFFECTIVE_LAYOUT_DIRECTION.index,
            )),
        },
        Which::Flow => Props {
            spacing: Flow::SPACING.index,
            padding: Flow::PADDING.index,
            left: Flow::LEFT_PADDING.index,
            right: Flow::RIGHT_PADDING.index,
            top: Flow::TOP_PADDING.index,
            bottom: Flow::BOTTOM_PADDING.index,
            direction: Some((
                Flow::LAYOUT_DIRECTION.index,
                Flow::EFFECTIVE_LAYOUT_DIRECTION.index,
            )),
        },
    }
}

/// What a positioner's completion sets up: side paddings that read
/// `padding` until they get a value of their own, and the effective
/// direction and alignment.
fn init(engine: &Engine, obj: ObjectId, which: Which) -> Result<(), EngineError> {
    let p = props(which);
    for side in [p.left, p.right, p.top, p.bottom] {
        if is_unset(engine, obj, side) {
            let all = p.padding;
            engine.bind(obj, side, move |e| e.read(obj, all))?;
        }
    }
    if let Some((_, effective)) = p.direction {
        engine.bind_initial(obj, effective, move |e| {
            Value::Int(if reversed(e, obj, &props(which)) {
                layout_direction::RIGHT_TO_LEFT
            } else {
                layout_direction::LEFT_TO_RIGHT
            })
        })?;
    }
    if which == Which::Grid {
        let effective = Grid::EFFECTIVE_HORIZONTAL_ITEM_ALIGNMENT.index;
        engine.bind_initial(obj, effective, move |e| {
            let a = num(e, obj, Grid::HORIZONTAL_ITEM_ALIGNMENT.index) as i32;
            let flipped = match a {
                align::LEFT => align::RIGHT,
                align::RIGHT => align::LEFT,
                other => other,
            };
            Value::Int(if reversed(e, obj, &props(Which::Grid)) {
                flipped
            } else {
                a
            })
        })?;
    }
    Ok(())
}

/// `rowSpacing = undefined`: the grid uses `spacing` between its rows
/// again. The property goes on reading the number it had, as it does in
/// Qt 6.11.
const OWN_SPACING_RESET: ljos_engine::Reset = ljos_engine::Reset::Custom {
    apply: |engine, obj, own| {
        let kept = engine.untrack(|| engine.read(obj, own));
        engine.reset_to(obj, own, kept)
    },
    value: |engine, obj, _own| engine.read(obj, Grid::SPACING.index),
};

/// What a side's padding is while it has no value of its own, for the
/// four positioners alike.
fn all_padding(engine: &Engine, obj: ObjectId, _side: PropIndex) -> Value {
    engine.read_by_name(obj, "padding")
}

/// A side's padding: its own value once set, else `padding` (Qt's rule).
/// A completed positioner has the unset sides bound to `padding`
/// ([`init`]); the check here covers one that is laid out before.
fn side_padding(engine: &Engine, obj: ObjectId, side: PropIndex, all: PropIndex) -> f64 {
    let own = num(engine, obj, side);
    let common = num(engine, obj, all);
    if is_unset(engine, obj, side) {
        common
    } else {
        own
    }
}

/// Left, top, right and bottom padding.
#[derive(Clone, Copy, Debug)]
struct Pad {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

fn paddings(engine: &Engine, obj: ObjectId, p: &Props) -> Pad {
    Pad {
        left: side_padding(engine, obj, p.left, p.padding),
        top: side_padding(engine, obj, p.top, p.padding),
        right: side_padding(engine, obj, p.right, p.padding),
        bottom: side_padding(engine, obj, p.bottom, p.padding),
    }
}

/// Whether the positioner runs right to left: its `layoutDirection`, the
/// other way round when it is mirrored.
fn reversed(engine: &Engine, obj: ObjectId, p: &Props) -> bool {
    let Some((direction, _)) = p.direction else {
        return false;
    };
    let rtl = num(engine, obj, direction) as i32 == layout_direction::RIGHT_TO_LEFT;
    rtl != mirrored(engine, obj)
}

/// Reads every input of a `Row` or `Column`. See [`track`].
pub fn read_inputs(engine: &Engine, obj: ObjectId, axis: Axis) {
    track(engine, obj, which_axis(axis));
}

/// Positions the children of a `Row` or `Column` and sets its implicit
/// size.
pub fn layout(engine: &Engine, obj: ObjectId, axis: Axis) -> Result<(), EngineError> {
    polish(engine, obj, which_axis(axis))
}

fn which_axis(axis: Axis) -> Which {
    match axis {
        Axis::Horizontal => Which::Row,
        Axis::Vertical => Which::Column,
    }
}

/// Reads every input of a positioner's layout. The scene calls it inside
/// an effect, so this is also the dependency list that schedules polish.
pub(crate) fn track(engine: &Engine, obj: ObjectId, which: Which) {
    let p = props(which);
    for i in [p.spacing, p.padding, p.left, p.right, p.top, p.bottom] {
        engine.read(obj, i);
    }
    // Which children count depends on it (`Hidden`).
    engine.read(obj, Item::VISIBLE.index);
    let rtl = reversed(engine, obj, &p);
    // The positioner's own size matters only where children are placed
    // from the far edge or wrap at it.
    let mut width = rtl;
    let mut height = false;
    match which {
        Which::Row | Which::Column => {}
        Which::Grid => {
            for i in [
                Grid::ROWS.index,
                Grid::COLUMNS.index,
                Grid::ROW_SPACING.index,
                Grid::COLUMN_SPACING.index,
                Grid::FLOW.index,
                Grid::HORIZONTAL_ITEM_ALIGNMENT.index,
                Grid::VERTICAL_ITEM_ALIGNMENT.index,
            ] {
                engine.read(obj, i);
            }
        }
        Which::Flow => {
            if num(engine, obj, Flow::FLOW.index) as i32 == flow::TOP_TO_BOTTOM {
                height = true;
                size_follows_implicit(engine, obj, false);
            } else {
                width = true;
                size_follows_implicit(engine, obj, true);
            }
        }
    }
    if width {
        engine.read(obj, Item::WIDTH.index);
    }
    if height {
        engine.read(obj, Item::HEIGHT.index);
    }
    for c in child_items(engine, obj) {
        engine.read(c, Item::VISIBLE.index);
        engine.read(c, Item::WIDTH.index);
        engine.read(c, Item::HEIGHT.index);
    }
}

/// A child a positioner places.
#[derive(Clone, Copy, Debug)]
struct Kid {
    id: ObjectId,
    w: f64,
    h: f64,
}

/// Where a child goes: from the content's left and top edges, as if the
/// positioner ran left to right.
#[derive(Clone, Copy, Debug)]
struct Placed {
    id: ObjectId,
    x: f64,
    y: f64,
    w: f64,
}

/// Positions the children and sets the positioner's implicit size.
///
/// Invisible children and children of zero size are skipped, as the Qt
/// docs say ("items with a width or height of 0 are not positioned").
pub(crate) fn polish(engine: &Engine, obj: ObjectId, which: Which) -> Result<(), EngineError> {
    engine.untrack(|| run(engine, obj, which))
}

fn run(engine: &Engine, obj: ObjectId, which: Which) -> Result<(), EngineError> {
    let p = props(which);
    let pad = paddings(engine, obj, &p);
    let spacing = num(engine, obj, p.spacing);
    let rtl = reversed(engine, obj, &p);
    let children = child_items(engine, obj);
    let mut hidden = Hidden::default();
    swap_hidden(engine, obj, which, &mut hidden);
    let visible = hidden.update(engine, obj, &children);
    swap_hidden(engine, obj, which, &mut hidden);
    let kids: Vec<Kid> = children
        .iter()
        .zip(visible)
        .filter(|(c, visible)| *visible && !is_a::<Repeater>(engine, **c))
        .map(|(c, _)| Kid {
            id: *c,
            w: num(engine, *c, Item::WIDTH.index),
            h: num(engine, *c, Item::HEIGHT.index),
        })
        .filter(|k| k.w > 0.0 && k.h > 0.0)
        .collect();
    if let Some(conflict) = anchor_conflict(engine, which, &kids) {
        // Qt does nothing at all then, size included.
        engine.log(LogLevel::Warning, crate::anchors::CATEGORY, conflict);
        return Ok(());
    }
    let (content, placed) = match which {
        Which::Row => line(&kids, spacing, true),
        Which::Column => line(&kids, spacing, false),
        Which::Grid => grid(engine, obj, &kids, spacing),
        Which::Flow => wrap(engine, obj, &kids, spacing, &pad),
    };
    engine.batch(|| {
        write_computed(
            engine,
            obj,
            Item::IMPLICIT_WIDTH.index,
            Value::Real(pad.left + content.0 + pad.right),
        )?;
        write_computed(
            engine,
            obj,
            Item::IMPLICIT_HEIGHT.index,
            Value::Real(pad.top + content.1 + pad.bottom),
        )?;
        // Right to left the children are placed from the right edge of
        // the positioner's actual width, which may follow the implicit
        // width just set.
        let width = if rtl {
            num(engine, obj, Item::WIDTH.index)
        } else {
            0.0
        };
        let moves = transitions_of(engine, obj, which);
        // Taken out while the transitions run, and put back after.
        let mut memory = with_memory(engine, obj, which, std::mem::take);
        for k in &placed {
            let x = if rtl {
                width - pad.right - k.x - k.w
            } else {
                pad.left + k.x
            };
            let y = pad.top + k.y;
            if let Some(mem) = memory.as_mut()
                && let Some(index) = placed.iter().position(|p| p.id == k.id)
                && arrive(engine, obj, which, mem, &moves, k.id, index, [x, y])
            {
                continue;
            }
            match which {
                Which::Row => write_placed(engine, k.id, Item::X.index, Value::Real(x))?,
                Which::Column => write_placed(engine, k.id, Item::Y.index, Value::Real(y))?,
                Which::Grid | Which::Flow => set_pos(engine, k.id, x, y)?,
            }
        }
        if let Some(mem) = memory.take() {
            let mut mem = mem;
            mem.populated = true;
            mem.at.retain(|id, _| placed.iter().any(|p| p.id == *id));
            with_memory(engine, obj, which, |m| *m = mem);
        }
        let across = match which {
            Which::Row => Some((Item::Y.index, pad.top)),
            Which::Column => Some((Item::X.index, pad.left)),
            Which::Grid | Which::Flow => None,
        };
        if let Some((prop, padding)) = across {
            let mut memory = Across::default();
            swap_across(engine, obj, which, &mut memory);
            let done = memory.apply(engine, &children, &placed, prop, padding);
            swap_across(engine, obj, which, &mut memory);
            done?;
        }
        with_placed(engine, obj, which, |last| {
            last.clear();
            last.extend(placed.iter().map(|k| k.id));
        });
        tell(engine, &children, &placed)
    })
}

/// A child that is anchored along an axis the positioner places it on
/// makes the positioner give up: the warning Qt gives for it, or `None`.
fn anchor_conflict(engine: &Engine, which: Which, kids: &[Kid]) -> Option<String> {
    let a = crate::item::AnchorProps::get();
    let set =
        |c: ObjectId, p: PropIndex| !matches!(engine.read(c, p), Value::Undefined | Value::Null);
    let (name, what) = match which {
        Which::Row => (
            "Row",
            "left, right, horizontalCenter, fill or centerIn anchors",
        ),
        Which::Column => (
            "Column",
            "top, bottom, verticalCenter, fill or centerIn anchors",
        ),
        Which::Grid => ("Grid", "anchors"),
        Which::Flow => ("Flow", "anchors"),
    };
    let vertical = [a.top, a.bottom, a.vertical_center];
    let horizontal = [a.left, a.right, a.horizontal_center];
    let along: &[PropIndex] = match which {
        Which::Row => &horizontal,
        Which::Column => &vertical,
        Which::Grid | Which::Flow => &[],
    };
    let any = kids.iter().any(|k| {
        set(k.id, a.fill)
            || set(k.id, a.center_in)
            || along.iter().any(|p| set(k.id, *p))
            || (along.is_empty() && horizontal.iter().chain(&vertical).any(|p| set(k.id, *p)))
    });
    any.then(|| {
        format!(
            "QML {name}: Cannot specify {what} for items inside {name}. {name} will not function."
        )
    })
}

/// One row or one column.
fn line(kids: &[Kid], spacing: f64, horizontal: bool) -> ((f64, f64), Vec<Placed>) {
    let mut along = 0.0;
    let mut across: f64 = 0.0;
    let mut placed = Vec::with_capacity(kids.len());
    for (i, k) in kids.iter().enumerate() {
        if i > 0 {
            along += spacing;
        }
        let (len, thick, x, y) = if horizontal {
            (k.w, k.h, along, 0.0)
        } else {
            (k.h, k.w, 0.0, along)
        };
        placed.push(Placed {
            id: k.id,
            x,
            y,
            w: k.w,
        });
        along += len;
        across = across.max(thick);
    }
    let content = if horizontal {
        (along, across)
    } else {
        (across, along)
    };
    (content, placed)
}

/// A `Grid`: cells filled along the flow, every column as wide as its
/// widest child and every row as tall as its tallest.
fn grid(engine: &Engine, obj: ObjectId, kids: &[Kid], spacing: f64) -> ((f64, f64), Vec<Placed>) {
    let count = |p: PropIndex| usize::try_from(num(engine, obj, p) as i64).unwrap_or(0);
    let gap = |p: PropIndex| {
        let own = num(engine, obj, p);
        // Followed: taking the gap back changes this and not the number.
        if crate::item::is_unset_tracked(engine, obj, p) {
            spacing
        } else {
            own
        }
    };
    let n = kids.len();
    let (rows, columns) = match (count(Grid::ROWS.index), count(Grid::COLUMNS.index)) {
        (0, 0) => (n.div_ceil(4), 4),
        (rows, 0) => (rows, n.div_ceil(rows)),
        (0, columns) => (n.div_ceil(columns), columns),
        both => both,
    };
    let top_to_bottom = num(engine, obj, Grid::FLOW.index) as i32 == flow::TOP_TO_BOTTOM;
    let (row_gap, column_gap) = (
        gap(Grid::ROW_SPACING.index),
        gap(Grid::COLUMN_SPACING.index),
    );
    // Children beyond the last cell are left where they are.
    let kids = &kids[..n.min(rows * columns)];
    let cell = |i: usize| {
        if top_to_bottom {
            (i % rows, i / rows)
        } else {
            (i / columns, i % columns)
        }
    };
    let mut widths = vec![0.0f64; columns];
    let mut heights = vec![0.0f64; rows];
    for (i, k) in kids.iter().enumerate() {
        let (r, c) = cell(i);
        widths[c] = widths[c].max(k.w);
        heights[r] = heights[r].max(k.h);
    }
    let starts = |sizes: &[f64], gap: f64| -> Vec<f64> {
        let mut at = 0.0;
        sizes
            .iter()
            .map(|s| {
                let start = at;
                at += s + gap;
                start
            })
            .collect()
    };
    let (xs, ys) = (starts(&widths, column_gap), starts(&heights, row_gap));
    let h_align = num(engine, obj, Grid::HORIZONTAL_ITEM_ALIGNMENT.index) as i32;
    let v_align = num(engine, obj, Grid::VERTICAL_ITEM_ALIGNMENT.index) as i32;
    let placed = kids
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let (r, c) = cell(i);
            let free_w = widths[c] - k.w;
            let free_h = heights[r] - k.h;
            Placed {
                id: k.id,
                x: xs[c]
                    + match h_align {
                        align::RIGHT => free_w,
                        align::H_CENTER => free_w / 2.0,
                        _ => 0.0,
                    },
                y: ys[r]
                    + match v_align {
                        align::BOTTOM => free_h,
                        align::V_CENTER => free_h / 2.0,
                        _ => 0.0,
                    },
                w: k.w,
            }
        })
        .collect();
    // As measured on Qt 6.11: across the flow every row or column counts
    // with its spacing, empty or not. Along the flow the size is how far
    // the fullest line got, and that includes the spacing after its last
    // child unless the child sits in the last cell of the line.
    let across = |sizes: &[f64], gap: f64| {
        sizes.iter().sum::<f64>() + gap * sizes.len().saturating_sub(1) as f64
    };
    let along = |sizes: &[f64], gap: f64| {
        let filled = kids.len().min(sizes.len());
        sizes.iter().sum::<f64>() + gap * filled.min(sizes.len().saturating_sub(1)) as f64
    };
    let content = if top_to_bottom {
        (across(&widths, column_gap), along(&heights, row_gap))
    } else {
        (along(&widths, column_gap), across(&heights, row_gap))
    };
    (content, placed)
}

/// A `Flow`: children side by side, on a new line when the next one does
/// not fit. A Flow that was given no size along its flow is one line.
fn wrap(
    engine: &Engine,
    obj: ObjectId,
    kids: &[Kid],
    spacing: f64,
    pad: &Pad,
) -> ((f64, f64), Vec<Placed>) {
    let top_to_bottom = num(engine, obj, Flow::FLOW.index) as i32 == flow::TOP_TO_BOTTOM;
    let limit = if size_follows_implicit(engine, obj, !top_to_bottom) {
        f64::INFINITY
    } else if top_to_bottom {
        num(engine, obj, Item::HEIGHT.index) - pad.top - pad.bottom
    } else {
        num(engine, obj, Item::WIDTH.index) - pad.left - pad.right
    };
    // `along` runs in the flow direction, `across` counts the lines.
    let (mut along, mut across) = (0.0f64, 0.0f64);
    let (mut thick, mut longest) = (0.0f64, 0.0f64);
    let mut first = true;
    let mut placed = Vec::with_capacity(kids.len());
    for k in kids {
        let (len, breadth) = if top_to_bottom {
            (k.h, k.w)
        } else {
            (k.w, k.h)
        };
        if !first && along + len > limit {
            across += thick + spacing;
            along = 0.0;
            thick = 0.0;
        }
        first = false;
        let (x, y) = if top_to_bottom {
            (across, along)
        } else {
            (along, across)
        };
        placed.push(Placed {
            id: k.id,
            x,
            y,
            w: k.w,
        });
        longest = longest.max(along + len);
        along += len + spacing;
        thick = thick.max(breadth);
    }
    let content = if top_to_bottom {
        (across + thick, longest)
    } else {
        (longest, across + thick)
    };
    (content, placed)
}

/// Sees to a child that is placed at `dest`: runs the transition that is
/// for it, if there is one. True when the child is then the transition's
/// to move (or it is already doing so), and the layout leaves it alone.
#[allow(clippy::too_many_arguments)]
fn arrive(
    engine: &Engine,
    obj: ObjectId,
    which: Which,
    mem: &mut Memory,
    [populate, add, moved]: &[Option<ObjectId>; 3],
    item: ObjectId,
    index: usize,
    dest: [f64; 2],
) -> bool {
    // The axis the layout writes of a row or a column is the one that is
    // compared; the other is the item's own.
    let cross = |axis: usize| {
        num(
            engine,
            item,
            if axis == 0 {
                Item::X.index
            } else {
                Item::Y.index
            },
        )
    };
    let dest = match which {
        Which::Row => [dest[0], cross(1)],
        Which::Column => [cross(0), dest[1]],
        Which::Grid | Which::Flow => dest,
    };
    let before = mem.at.insert(item, dest);
    if crate::animation::driver::has_run(engine, item) {
        if mem.going.get(&item) == Some(&dest) {
            return true;
        }
        crate::views::view_transition::forget(engine, obj, item);
        mem.going.remove(&item);
    }
    let differs = |a: [f64; 2]| match which {
        Which::Row => a[0] != dest[0],
        Which::Column => a[1] != dest[1],
        Which::Grid | Which::Flow => a != dest,
    };
    let t = if !mem.populated {
        *populate
    } else {
        match before {
            None => *add,
            Some(b) if differs(b) => *moved,
            Some(_) => None,
        }
    };
    let Some(t) = t else { return false };
    let targets = std::rc::Rc::new(crate::views::state::Targets {
        indexes: vec![index as i32],
        items: vec![item],
    });
    let going = crate::views::view_transition::start(
        engine,
        obj,
        Some(t),
        item,
        index as i32,
        dest,
        targets,
    );
    if going.iter().any(|g| *g) {
        mem.going.insert(item, dest);
    }
    // Written by `start` for the axes that are not moving.
    true
}

fn set_pos(engine: &Engine, c: ObjectId, x: f64, y: f64) -> Result<(), EngineError> {
    for (p, v) in [(Item::X.index, x), (Item::Y.index, y)] {
        write_placed(engine, c, p, Value::Real(v))?;
    }
    Ok(())
}

fn attacher() -> MetaRef {
    MetaRef::Static(Positioner::static_meta())
}

/// Tells every child that asked (`Positioner.index`) where it stands.
fn tell(engine: &Engine, children: &[ObjectId], placed: &[Placed]) -> Result<(), EngineError> {
    let att = attacher();
    // Only made once a child asks: most positioners have none that do.
    let mut places: Option<HashMap<ObjectId, usize>> = None;
    for c in children {
        let Some(a) = engine.existing_attached_object(*c, &att) else {
            continue;
        };
        let places = places
            .get_or_insert_with(|| placed.iter().enumerate().map(|(i, k)| (k.id, i)).collect());
        tell_one(engine, a, places.get(c).copied(), placed.len())?;
    }
    Ok(())
}

/// Writes the place `at` of `count` into the `Positioner` object `a`.
fn tell_one(
    engine: &Engine,
    a: ObjectId,
    at: Option<usize>,
    count: usize,
) -> Result<(), EngineError> {
    let index = at.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1);
    write_computed(
        engine,
        a,
        PositionerAttached::INDEX.index,
        Value::Int(index),
    )?;
    write_computed(
        engine,
        a,
        PositionerAttached::IS_FIRST_ITEM.index,
        Value::Bool(at == Some(0)),
    )?;
    write_computed(
        engine,
        a,
        PositionerAttached::IS_LAST_ITEM.index,
        Value::Bool(at.is_some_and(|i| i + 1 == count)),
    )
}

/// A child that gets its `Positioner` object after its positioner ran is
/// told where it stood then, as Qt tells it: in the statement that asked.
/// A child the positioner has not placed yet (it has only just become
/// one) has no place until the positioner runs again.
pub(crate) fn register(engine: &Engine) {
    engine.on_attached_created(|e, item, att| {
        if is_a::<PositionerAttached>(e, att)
            && let Some(parent) = e.untrack(|| e.visual_parent(item))
            && let Some(which) = which(e, parent)
        {
            let place = with_placed(e, parent, which, |last| {
                (last.iter().position(|c| *c == item), last.len())
            });
            if let Some((at, count)) = place {
                // Not an error worth a warning: the values stay the
                // defaults, which say "not positioned".
                let _ = e.untrack(|| tell_one(e, att, at, count));
            }
        }
        true
    });
}
