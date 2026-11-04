//! `Ljos.Ui`'s one arrangement: `Row`, `Column` and `Box` (docs/DIALECT.md).
//!
//! One native in the place of Qt's positioners and line layouts. A `Box`
//! lays its children out along its `direction`; a `Row` and a `Column`
//! are a box whose direction is fixed. It measures and arranges as the
//! layouts of this module do, with one size rule per axis:
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
//! side lands on one, as in Qt's layouts; nothing else is rounded.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use ljos_engine::{Engine, EngineError, ObjectId, PropIndex, QmlObject, Value};

use super::distribute::{Total, Track, distribute, edges};
use super::{Attached, H, Hint, Layout, Plan, V, show_own_minimum, state};
use crate::enums::{EnumTable, align, layout_direction};
use crate::item::{Item, is_a, is_unset, num, write_computed, write_placed};
use crate::mirroring::mirrored;

/// `Box.direction`.
pub mod direction {
    pub const ROW: i32 = 0;
    pub const COLUMN: i32 = 1;
}

/// What `Row`, `Column` and `Box` of `Ljos.Ui` have alike; not made by
/// itself.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Ljos.Ui", name = "Arrangement", extends = Layout, uncreatable)]
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

/// `Row` (Ljos.Ui): children side by side.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Ljos.Ui", name = "Row", extends = Arrangement)]
pub struct BoxRow {
    #[qml(base)]
    pub arrangement: Arrangement,
}

/// `Column` (Ljos.Ui): children one under another.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Ljos.Ui", name = "Column", extends = Arrangement)]
pub struct BoxColumn {
    #[qml(base)]
    pub arrangement: Arrangement,
}

/// `Box` (Ljos.Ui): a row or a column, as `direction` says; it can turn.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Ljos.Ui", name = "Box", extends = Arrangement, enums = box_enums)]
pub struct LjosBox {
    #[qml(base)]
    pub arrangement: Arrangement,
    /// [`direction`].
    #[qml(notify, enumeration = "Direction")]
    pub direction: i32,
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

/// What one child asks for along one axis: the one size rule.
///
/// `current` is its width or height now, `told` the one its file gave it
/// when the box first saw it (`NaN` when it followed its implicit size).
pub(super) fn hint(a: &Attached, axis: usize, current: f64, told: f64, implicit: f64) -> Hint {
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
    let min = a.min[axis].unwrap_or(0.0).min(max);
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

/// The geometry of every child of a box `width` by `height`.
pub(super) fn compute(plan: &Plan, width: f64, height: f64) -> Vec<(ObjectId, [f64; 4])> {
    let main = usize::from(plan.vertical);
    let cross = 1 - main;
    let size = [width, height];
    let pad = plan.pad;
    let (lead, trail) = ([pad[0], pad[1]], [pad[2], pad[3]]);
    let content = |axis: usize| (size[axis] - lead[axis] - trail[axis]).max(0.0);
    let tracks: &[Track] = &plan.tracks[main];
    let spacing = plan.spacing[main];
    let used = tracks.iter().filter(|t| t.used).count();
    let most: f64 = tracks.iter().filter(|t| t.used).map(|t| t.max).sum::<f64>()
        + spacing * used.saturating_sub(1) as f64;
    // Room nobody can take is left at the end, not shared out.
    let sizes = distribute(tracks, spacing, content(main).min(most));
    // Room shared out is shared in whole pixels, as Qt's layouts share it;
    // a line of children that keep their sizes is placed as they ask.
    let shared = tracks
        .iter()
        .zip(&sizes)
        .any(|(t, s)| t.used && *s != t.pref);
    let lines: Vec<(f64, f64)> = if shared {
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
    };
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
/// a positioner leaves it, unless it asks the box for a size.
pub(super) fn skipped(e: &Engine, c: ObjectId) -> bool {
    let empty = num(e, c, crate::item::Item::WIDTH.index) == 0.0
        || num(e, c, crate::item::Item::HEIGHT.index) == 0.0;
    empty && e.existing_attached_object(c, &super::attacher()).is_none()
}

/// A box none of whose `shown` children has a `Layout`: each keeps its
/// size, so the box is a positioner, and places them as one does, in one
/// pass and keeping nothing of them. Gives whether its own size hints
/// changed, as measuring does.
pub(super) fn line(e: &Engine, obj: ObjectId, shown: &[ObjectId]) -> Result<bool, EngineError> {
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
    let size = |c: ObjectId| [num(e, c, Item::WIDTH.index), num(e, c, Item::HEIGHT.index)];
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
    for (i, c) in shown.iter().enumerate() {
        if i > 0 {
            along += spacing;
        }
        let s = size(*c);
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
        if !s.seen.is_empty() {
            s.seen = HashMap::new();
        }
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
    let props = [Item::X.index, Item::Y.index];
    e.batch(|| -> Result<(), EngineError> {
        let mut at = 0.0;
        for c in shown {
            let s = size(*c);
            let mut p = [0.0; 2];
            p[main] = if rtl { room - at - s[main] } else { at } + lead[main];
            p[cross] = lead[cross];
            at += s[main] + spacing;
            for (prop, v) in props.into_iter().zip(p) {
                write_placed(e, *c, prop, Value::Real(v))?;
            }
        }
        Ok(())
    })?;
    Ok(changed)
}
