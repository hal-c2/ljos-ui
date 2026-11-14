//! `Ljos.Layout`'s one arrangement: `Row`, `Column`, `Box` and `Grid`
//! (docs/DIALECT.md).
//!
//! One native in the place of Qt's positioners and layouts. A `Box`
//! lays its children out along its `direction`; a `Row` and a `Column`
//! are a box whose direction is fixed; a `Grid` lays them out in cells.
//! It measures and arranges as the layouts of this module do, with one
//! size rule per axis:
//!
//! - an axis a child says nothing about through `Layout.` (no fill, no
//!   preferred, smallest or largest size) is the child's own: the box
//!   reads its width or height and never writes it, as a positioner;
//! - an axis it does say something about asks for `Layout.preferredX`,
//!   else the size its file gives it, else its implicit size, and then
//!   fills and is bounded as in Qt's layouts.
//!
//! Where nothing can grow, the box does not spread its children over the
//! room it has, as RowLayout does: they stay at its start, as in a
//! positioner. Where room is shared out (a child grows or is squeezed),
//! it is shared in whole pixels, and a child centred or pushed to the far
//! side lands on one, as in Qt's layouts.
//!
//! A box any of whose children has a `Layout` is a layout, and a size a
//! child takes from its content (its implicit size, where its file gives
//! it no width or height) is rounded up to a whole pixel, so no seam falls
//! between two pixels. A box none of whose children has a `Layout` is a
//! positioner, and places each child at the size it has. A size the
//! author gives is never rounded.

use std::hash::{DefaultHasher, Hash, Hasher};

use ljos_engine::{Engine, EngineError, ObjectId, PropIndex, Provenance, QmlObject, Value};

use super::distribute::{Total, Track, distribute, edges};
use super::{Attached, H, Hint, Layout, Plan, V, show_own_minimum, state};
use crate::enums::{EnumTable, align, layout_direction};
use crate::item::{
    Item, is_a, is_unset, num, size_follows_implicit, write_computed, write_given_size,
    write_placed,
};
use crate::mirroring::mirrored;
use crate::positioners::flow;

/// `Box.direction`.
pub mod direction {
    pub const ROW: i32 = 0;
    pub const COLUMN: i32 = 1;
}

/// What `Row`, `Column`, `Box` and `Grid` of `Ljos.Layout` have alike;
/// not made by itself.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Ljos.Layout", name = "Arrangement", extends = Layout, uncreatable)]
pub struct Arrangement {
    #[qml(base)]
    pub layout: Layout,
    /// Between two children. 0, as a positioner's.
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
    /// [`crate::enums::layout_direction`]: a row runs right to left with
    /// `Qt.RightToLeft`, and the other way round when it is mirrored.
    #[qml(notify, enumeration = "Qt.LayoutDirection")]
    pub layout_direction: i32,
}

/// `Row` (Ljos.Layout): children side by side.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Ljos.Layout", name = "Row", extends = Arrangement)]
pub struct BoxRow {
    #[qml(base)]
    pub arrangement: Arrangement,
}

/// `Column` (Ljos.Layout): children one under another.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Ljos.Layout", name = "Column", extends = Arrangement)]
pub struct BoxColumn {
    #[qml(base)]
    pub arrangement: Arrangement,
}

/// `Box` (Ljos.Layout): a row or a column, as `direction` says; it can turn.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Ljos.Layout", name = "Box", extends = Arrangement, enums = box_enums)]
pub struct LjosBox {
    #[qml(base)]
    pub arrangement: Arrangement,
    /// [`direction`].
    #[qml(notify, enumeration = "Direction")]
    pub direction: i32,
}

/// `Grid` (Ljos.Layout): children in cells, along the `flow`. A column is
/// as wide as what its children ask for, a row as tall; a child says which
/// cell it takes, and how many, through `Layout.row`, `Layout.column` and
/// the spans.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Ljos.Layout", name = "Grid", extends = Arrangement, enums = grid_enums)]
pub struct LjosGrid {
    #[qml(base)]
    pub arrangement: Arrangement,
    /// How many cells a row has. Unset (0) with `rows` unset too: as many
    /// as there are children, one row; with `rows` set: as many as the
    /// children need in that many rows.
    #[qml(notify)]
    pub columns: i32,
    /// How many cells a column has, as `columns` says of a row.
    #[qml(notify)]
    pub rows: i32,
    /// Between two columns; `spacing` until it is given.
    #[qml(notify, reset = ljos_engine::Reset::Follow(spacing))]
    pub column_spacing: f64,
    /// Between two rows; `spacing` until it is given.
    #[qml(notify, reset = ljos_engine::Reset::Follow(spacing))]
    pub row_spacing: f64,
    /// [`crate::positioners::flow`]: across a row first, or down a column.
    #[qml(notify, enumeration = "Flow")]
    pub flow: i32,
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

fn spacing(engine: &Engine, obj: ObjectId, _gap: PropIndex) -> Value {
    engine.read(obj, Arrangement::SPACING.index)
}

/// What a grid reads of itself, besides what every arrangement does.
pub(super) fn grid_props() -> [PropIndex; 5] {
    [
        LjosGrid::COLUMNS.index,
        LjosGrid::ROWS.index,
        LjosGrid::COLUMN_SPACING.index,
        LjosGrid::ROW_SPACING.index,
        LjosGrid::FLOW.index,
    ]
}

/// A grid's spacing between columns, then between rows.
pub(super) fn grid_spacing(e: &Engine, obj: ObjectId) -> [f64; 2] {
    let all = num(e, obj, Arrangement::SPACING.index);
    let gap = |p: PropIndex| {
        if is_unset(e, obj, p) {
            all
        } else {
            num(e, obj, p)
        }
    };
    [
        gap(LjosGrid::COLUMN_SPACING.index),
        gap(LjosGrid::ROW_SPACING.index),
    ]
}

/// How many rows and columns a grid of `count` children has, as far as it
/// limits them, and whether it fills columns first. A count it is given
/// bounds its line; given the other one only, it is what the children
/// need in that many lines.
pub(super) fn grid_limits(
    e: &Engine,
    obj: ObjectId,
    count: usize,
) -> (Option<usize>, Option<usize>, bool) {
    let given = |p: PropIndex| {
        usize::try_from(num(e, obj, p) as i64)
            .ok()
            .filter(|n| *n > 0)
    };
    let top_to_bottom = num(e, obj, LjosGrid::FLOW.index) as i32 == flow::TOP_TO_BOTTOM;
    let (rows, columns) = match (given(LjosGrid::ROWS.index), given(LjosGrid::COLUMNS.index)) {
        (Some(r), None) if !top_to_bottom => (Some(r), Some(count.div_ceil(r).max(1))),
        (None, Some(c)) if top_to_bottom => (Some(count.div_ceil(c).max(1)), Some(c)),
        other => other,
    };
    (rows, columns, top_to_bottom)
}

fn box_enums() -> EnumTable {
    vec![(
        "Direction",
        vec![
            ("Row", i64::from(direction::ROW)),
            ("Column", i64::from(direction::COLUMN)),
        ],
    )]
}

fn all_padding(engine: &Engine, obj: ObjectId, _side: PropIndex) -> Value {
    engine.read(obj, Arrangement::PADDING.index)
}

/// What a box reads of itself, besides its size.
pub(super) fn own_props() -> [PropIndex; 7] {
    [
        Arrangement::SPACING.index,
        Arrangement::PADDING.index,
        Arrangement::LEFT_PADDING.index,
        Arrangement::RIGHT_PADDING.index,
        Arrangement::TOP_PADDING.index,
        Arrangement::BOTTOM_PADDING.index,
        Arrangement::LAYOUT_DIRECTION.index,
    ]
}

/// Whether the box lays its children out one under another. Read as an
/// input: a `Box` whose direction changes lays them out again.
pub(super) fn vertical(e: &Engine, obj: ObjectId) -> bool {
    if is_a::<BoxColumn>(e, obj) {
        true
    } else if is_a::<LjosBox>(e, obj) {
        num(e, obj, LjosBox::DIRECTION.index) as i32 == direction::COLUMN
    } else {
        false
    }
}

/// Left, top, right and bottom padding: a side's own once it is given
/// one, else `padding`.
pub(super) fn paddings(e: &Engine, obj: ObjectId) -> [f64; 4] {
    let all = num(e, obj, Arrangement::PADDING.index);
    let side = |p: PropIndex| {
        let own = num(e, obj, p);
        if is_unset(e, obj, p) { all } else { own }
    };
    [
        side(Arrangement::LEFT_PADDING.index),
        side(Arrangement::TOP_PADDING.index),
        side(Arrangement::RIGHT_PADDING.index),
        side(Arrangement::BOTTOM_PADDING.index),
    ]
}

/// Whether a child says anything through `Layout.` about one axis. An
/// axis it says nothing about is its own.
pub(super) fn managed(a: &Attached, axis: usize) -> bool {
    a.fill[axis] == Some(true)
        || a.pref[axis].is_some()
        || a.min[axis].is_some()
        || a.max[axis].is_some()
}

/// An implicit size as a box that is a layout takes it: up to a whole
/// pixel.
// Qt 6.11: RowLayout and ColumnLayout round a child's implicit size up (a
// Text of implicit width 67.734375 is 68 wide in one, and the next child is
// at 68); Row and Column, the positioners, place it at the fraction.
pub(super) fn whole(implicit: f64) -> f64 {
    implicit.ceil()
}

/// Whether a child's width (`H`) or height is its implicit size: it
/// follows it, or holds the size the box `gave` it for following it, as
/// the box wrote it ([`write_given_size`]). The author's write of the same
/// value is a size given, not one followed.
pub(super) fn follows(e: &Engine, c: ObjectId, axis: usize, gave: f64) -> bool {
    let size = if axis == H {
        Item::WIDTH.index
    } else {
        Item::HEIGHT.index
    };
    size_follows_implicit(e, c, axis == H)
        || (!gave.is_nan()
            && num(e, c, size) == gave
            && e.provenance(c, size) == Some(Provenance::Host))
}

/// What one child asks for along one axis: the one size rule.
///
/// `current` is its width or height now, `told` the one its file gave it
/// when the box first saw it (`NaN` when it followed its implicit size).
/// `least` is a nested layout's smallest size, its children's: its own
/// unless it is told one.
pub(super) fn hint(
    a: &Attached,
    axis: usize,
    current: f64,
    told: f64,
    implicit: f64,
    least: Option<f64>,
) -> Hint {
    if !managed(a, axis) {
        return Hint {
            min: current,
            pref: current,
            max: current,
            stretch: -1,
        };
    }
    let asked = a.pref[axis].unwrap_or(if told.is_nan() { implicit } else { told });
    let max = a.max[axis].unwrap_or(f64::INFINITY);
    let min = a.min[axis].or(least).unwrap_or(0.0).min(max);
    let pref = asked.clamp(min, max);
    if a.fill[axis] == Some(true) {
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

/// The box's own hints: its children's along each axis and its padding.
pub(super) fn with_padding(mut hints: [Total; 2], pad: [f64; 4]) -> [Total; 2] {
    for (axis, extra) in [(H, pad[0] + pad[2]), (V, pad[1] + pad[3])] {
        hints[axis].min += extra;
        hints[axis].pref += extra;
        hints[axis].max += extra;
    }
    hints
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Near,
    Center,
    Far,
}

/// At the start unless the child says otherwise, as in a positioner; a row
/// right to left starts at the right.
fn align_of(flags: i32, axis: usize, rtl: bool) -> Align {
    let (far, center) = if axis == H {
        (align::RIGHT, align::H_CENTER)
    } else {
        (align::BOTTOM, align::V_CENTER)
    };
    let a = if flags & far != 0 {
        Align::Far
    } else if flags & center != 0 {
        Align::Center
    } else {
        Align::Near
    };
    match (a, rtl && axis == H) {
        (Align::Near, true) => Align::Far,
        (Align::Far, true) => Align::Near,
        (a, _) => a,
    }
}

/// A child in `start..end` with margins: its position and size.
fn fit(start: f64, end: f64, before: f64, after: f64, hint: &Hint, align: Align) -> (f64, f64) {
    let room = (end - start - before - after).max(0.0);
    let size = room.min(hint.max).max(hint.min);
    let free = room - size;
    let at = match align {
        _ if free <= 0.0 => start + before,
        Align::Near => start + before,
        Align::Center => super::round_half_up(start + before + free / 2.0),
        Align::Far => super::round_half_up(end - after - size),
    };
    (at, size)
}

/// Where each of `tracks` starts and ends in `content` along an axis.
fn lines(tracks: &[Track], spacing: f64, content: f64) -> Vec<(f64, f64)> {
    let used = tracks.iter().filter(|t| t.used).count();
    let most: f64 = tracks.iter().filter(|t| t.used).map(|t| t.max).sum::<f64>()
        + spacing * used.saturating_sub(1) as f64;
    // Room nobody can take is left at the end, not shared out.
    let sizes = distribute(tracks, spacing, content.min(most));
    // Room shared out is shared in whole pixels, as Qt's layouts share it;
    // a line of children that keep their sizes is placed as they ask.
    let shared = tracks
        .iter()
        .zip(&sizes)
        .any(|(t, s)| t.used && *s != t.pref);
    if shared {
        edges(tracks, &sizes, spacing)
    } else {
        let mut at = 0.0;
        tracks
            .iter()
            .zip(&sizes)
            .map(|(t, s)| {
                let line = (at, at + s);
                if t.used {
                    at += s + spacing;
                }
                line
            })
            .collect()
    }
}

/// The geometry of every child of a grid `width` by `height`: each axis
/// as a box's line, a child across the cells it spans.
pub(super) fn compute_cells(plan: &Plan, width: f64, height: f64) -> Vec<(ObjectId, [f64; 4])> {
    let size = [width, height];
    let pad = plan.pad;
    let (lead, trail) = ([pad[0], pad[1]], [pad[2], pad[3]]);
    let content = |axis: usize| (size[axis] - lead[axis] - trail[axis]).max(0.0);
    let edges = [H, V].map(|axis| lines(&plan.tracks[axis], plan.spacing[axis], content(axis)));
    // From the first to the last track the child covers that has a size:
    // the others under a span take no room.
    let cover = |axis: usize, start: usize, len: usize| {
        let mut used = (start..start + len)
            .filter(|i| plan.tracks[axis].get(*i).is_some_and(|t| t.used))
            .filter_map(|i| edges[axis].get(i));
        let first = used.next().copied().unwrap_or((0.0, 0.0));
        let last = used.next_back().copied().unwrap_or(first);
        (first.0, last.1)
    };
    plan.cells
        .iter()
        .map(|c| {
            let mut g = [0.0; 4];
            for axis in [H, V] {
                let (start, len) = if axis == H {
                    (c.slot.column, c.slot.column_span)
                } else {
                    (c.slot.row, c.slot.row_span)
                };
                let (mut s0, mut s1) = cover(axis, start, len);
                let rtl = plan.rtl && axis == H;
                if rtl {
                    (s0, s1) = (content(H) - s1, content(H) - s0);
                }
                let (b, a) = (c.margins[axis], c.margins[axis + 2]);
                let (at, length) = fit(s0, s1, b, a, &c.hint[axis], align_of(c.align, axis, rtl));
                g[axis] = lead[axis] + at;
                g[2 + axis] = length;
            }
            (c.item, g)
        })
        .collect()
}

/// The geometry of every child of a box `width` by `height`.
pub(super) fn compute(plan: &Plan, width: f64, height: f64) -> Vec<(ObjectId, [f64; 4])> {
    let main = usize::from(plan.vertical);
    let cross = 1 - main;
    let size = [width, height];
    let pad = plan.pad;
    let (lead, trail) = ([pad[0], pad[1]], [pad[2], pad[3]]);
    let content = |axis: usize| (size[axis] - lead[axis] - trail[axis]).max(0.0);
    let lines = lines(&plan.tracks[main], plan.spacing[main], content(main));
    let rtl = plan.rtl && main == H;
    plan.cells
        .iter()
        .map(|c| {
            let i = if main == H { c.slot.column } else { c.slot.row };
            let (mut s0, mut s1) = lines[i];
            if rtl {
                (s0, s1) = (content(H) - s1, content(H) - s0);
            }
            let m = |axis: usize| (c.margins[axis], c.margins[axis + 2]);
            let (b, a) = m(main);
            let (p_main, l_main) = fit(s0, s1, b, a, &c.hint[main], align_of(c.align, main, rtl));
            let (b, a) = m(cross);
            let (p_cross, l_cross) = fit(
                0.0,
                content(cross),
                b,
                a,
                &c.hint[cross],
                align_of(c.align, cross, rtl),
            );
            let mut g = [0.0; 4];
            g[main] = lead[main] + p_main;
            g[cross] = lead[cross] + p_cross;
            g[2 + main] = l_main;
            g[2 + cross] = l_cross;
            (c.item, g)
        })
        .collect()
}

/// Whether a child of a box is left out: one with no width or height, as
/// a positioner leaves it, unless it asks the box for a size. A size the
/// box `gave` it for following its implicit size is that implicit size
/// now: given 0 once, it is back once its content grows.
pub(super) fn skipped(e: &Engine, c: ObjectId, gave: [f64; 2]) -> bool {
    let empty = [(H, Item::WIDTH.index), (V, Item::HEIGHT.index)]
        .into_iter()
        .zip([Item::IMPLICIT_WIDTH.index, Item::IMPLICIT_HEIGHT.index])
        .any(|((axis, size), implicit)| {
            let p = if follows(e, c, axis, gave[axis]) {
                implicit
            } else {
                size
            };
            num(e, c, p) == 0.0
        });
    empty && e.existing_attached_object(c, &super::attacher()).is_none()
}

/// A box none of whose `shown` children has a `Layout`: each keeps its
/// size, so the box is a positioner, and places them as one does, in one
/// pass and keeping nothing of them. Gives whether its own size hints
/// changed, as measuring does.
pub(super) fn line(
    e: &Engine,
    obj: ObjectId,
    children: &[ObjectId],
    shown: &[ObjectId],
) -> Result<bool, EngineError> {
    let vertical = vertical(e, obj);
    let main = usize::from(vertical);
    let cross = 1 - main;
    let pad = paddings(e, obj);
    let (lead, trail) = ([pad[0], pad[1]], [pad[2], pad[3]]);
    let spacing = num(e, obj, Arrangement::SPACING.index);
    let rtl = !vertical
        && ((num(e, obj, Arrangement::LAYOUT_DIRECTION.index) as i32
            == layout_direction::RIGHT_TO_LEFT)
            != mirrored(e, obj));
    // A child the box gave a size while it was a layout holds it: while it
    // still follows its implicit size, it is given that again, unrounded.
    let mut gave = state(e, obj, |s| std::mem::take(&mut s.gave))?;
    gave.retain(|c, _| children.contains(c));
    let (sizes, given): (Vec<[f64; 2]>, Vec<[bool; 2]>) = shown
        .iter()
        .map(|c| {
            let had = gave.remove(c).unwrap_or([f64::NAN; 2]);
            let mut own = [
                num(e, *c, Item::WIDTH.index),
                num(e, *c, Item::HEIGHT.index),
            ];
            let mut gives = [f64::NAN; 2];
            for (axis, p) in [
                (H, Item::IMPLICIT_WIDTH.index),
                (V, Item::IMPLICIT_HEIGHT.index),
            ] {
                if !had[axis].is_nan() && follows(e, *c, axis, had[axis]) {
                    own[axis] = num(e, *c, p);
                    gives[axis] = own[axis];
                }
            }
            if gives.iter().any(|v| !v.is_nan()) {
                gave.insert(*c, gives);
            }
            (own, gives.map(|v| !v.is_nan()))
        })
        .unzip();
    let gave_back = state(e, obj, |s| s.gave = gave);
    gave_back?;
    let room = (num(e, obj, Item::WIDTH.index) - pad[0] - pad[2]).max(0.0);
    // Everything the children's places follow from. The box's own width
    // only counts from the right: from the left it is set after the
    // implicit size the box gives itself, and changes nothing placed.
    let mut inputs = DefaultHasher::new();
    (vertical, rtl, shown.len()).hash(&mut inputs);
    for v in pad
        .into_iter()
        .chain([spacing, if rtl { room } else { 0.0 }])
    {
        v.to_bits().hash(&mut inputs);
    }
    let mut along = 0.0;
    let mut widest: f64 = 0.0;
    for (i, (c, s)) in shown.iter().zip(&sizes).enumerate() {
        if i > 0 {
            along += spacing;
        }
        (c, s[0].to_bits(), s[1].to_bits()).hash(&mut inputs);
        along += s[main];
        widest = widest.max(s[cross]);
    }
    let inputs = inputs.finish();
    if state(e, obj, |s| s.lined == Some(inputs))? {
        return Ok(false);
    }
    let mut extent = [0.0; 2];
    extent[main] = along;
    extent[cross] = widest;
    let fixed = |axis: usize| {
        let v = extent[axis] + lead[axis] + trail[axis];
        Total {
            min: v,
            pref: v,
            max: v,
        }
    };
    let hints = [fixed(H), fixed(V)];
    let changed = state(e, obj, |s| {
        let changed = s.hints != hints;
        s.hints = hints;
        // Nothing of a plan is kept: a child given a `Layout` makes the box
        // measure again, and plan.
        s.plan = None;
        s.lined = Some(inputs);
        // What the box first saw of a child is kept, as a plan's measure
        // keeps it: a hidden child it sized, shown again with a `Layout`,
        // asks for its own size, not the one the box wrote.
        s.seen.retain(|c, _| children.contains(c));
        if !s.placed.is_empty() {
            s.placed = Vec::new();
        }
        s.stale = false;
        s.check = false;
        s.arranged = None;
        s.runs += 1;
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
    show_own_minimum(e, obj, &hints)?;
    let props = [
        Item::X.index,
        Item::Y.index,
        Item::WIDTH.index,
        Item::HEIGHT.index,
    ];
    e.batch(|| -> Result<(), EngineError> {
        let mut at = 0.0;
        for ((c, s), given) in shown.iter().zip(&sizes).zip(&given) {
            let mut p = [0.0; 2];
            p[main] = if rtl { room - at - s[main] } else { at } + lead[main];
            p[cross] = lead[cross];
            at += s[main] + spacing;
            // A size equal to the child's own is not written.
            let given = [false, false, given[H], given[V]];
            for ((prop, v), given) in props.into_iter().zip([p[0], p[1], s[0], s[1]]).zip(given) {
                if given {
                    write_given_size(e, *c, prop, Value::Real(v))?;
                } else {
                    write_placed(e, *c, prop, Value::Real(v))?;
                }
            }
        }
        Ok(())
    })?;
    Ok(changed)
}
