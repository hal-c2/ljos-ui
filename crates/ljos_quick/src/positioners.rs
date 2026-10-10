//! `Row` and `Column`.
//!
//! The scene runs [`layout`] in the polish pass (pipeline step 6) for every
//! positioner whose inputs changed; nothing is laid out synchronously.

use ljos_engine::{Engine, EngineError, ObjectId, PropIndex, QmlObject, Value, WriteKind};

use crate::enums::layout_direction;
use crate::item::{Item, num, write_computed};
use crate::tree::child_items;

/// `Row` (QtQuick).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Row", extends = Item)]
pub struct Row {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub spacing: f64,
    #[qml(notify)]
    pub padding: f64,
    #[qml(notify)]
    pub left_padding: f64,
    #[qml(notify)]
    pub right_padding: f64,
    #[qml(notify)]
    pub top_padding: f64,
    #[qml(notify)]
    pub bottom_padding: f64,
    /// [`crate::enums::layout_direction`].
    #[qml(notify)]
    pub layout_direction: i32,
}

/// `Column` (QtQuick).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Column", extends = Item)]
pub struct Column {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub spacing: f64,
    #[qml(notify)]
    pub padding: f64,
    #[qml(notify)]
    pub left_padding: f64,
    #[qml(notify)]
    pub right_padding: f64,
    #[qml(notify)]
    pub top_padding: f64,
    #[qml(notify)]
    pub bottom_padding: f64,
}

/// Which axis a positioner stacks along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

struct Props {
    spacing: PropIndex,
    padding: PropIndex,
    left: PropIndex,
    right: PropIndex,
    top: PropIndex,
    bottom: PropIndex,
}

fn props(axis: Axis) -> Props {
    match axis {
        Axis::Horizontal => Props {
            spacing: Row::SPACING.index,
            padding: Row::PADDING.index,
            left: Row::LEFT_PADDING.index,
            right: Row::RIGHT_PADDING.index,
            top: Row::TOP_PADDING.index,
            bottom: Row::BOTTOM_PADDING.index,
        },
        Axis::Vertical => Props {
            spacing: Column::SPACING.index,
            padding: Column::PADDING.index,
            left: Column::LEFT_PADDING.index,
            right: Column::RIGHT_PADDING.index,
            top: Column::TOP_PADDING.index,
            bottom: Column::BOTTOM_PADDING.index,
        },
    }
}

/// A side's padding: its own value once set, else `padding` (Qt's rule).
fn side_padding(engine: &Engine, obj: ObjectId, side: PropIndex, all: PropIndex) -> f64 {
    match engine.provenance(obj, side) {
        Some(ljos_engine::Provenance::Default) | None => num(engine, obj, all),
        _ => num(engine, obj, side),
    }
}

/// Reads every input of a positioner's layout. The scene calls it inside
/// an effect, so this is also the dependency list that schedules polish.
pub fn read_inputs(engine: &Engine, obj: ObjectId, axis: Axis) {
    let p = props(axis);
    for i in [p.spacing, p.padding, p.left, p.right, p.top, p.bottom] {
        engine.read(obj, i);
    }
    if axis == Axis::Horizontal {
        engine.read(obj, Row::LAYOUT_DIRECTION.index);
        engine.read(obj, Item::WIDTH.index);
    }
    for c in child_items(engine, obj) {
        engine.read(c, Item::VISIBLE.index);
        engine.read(c, Item::WIDTH.index);
        engine.read(c, Item::HEIGHT.index);
    }
}

/// Positions the children and sets the positioner's implicit size.
///
/// Invisible children and children of zero size are skipped, as the Qt
/// docs say ("items with a width or height of 0 are not positioned").
pub fn layout(engine: &Engine, obj: ObjectId, axis: Axis) -> Result<(), EngineError> {
    let p = props(axis);
    let (spacing, left, right, top, bottom) = engine.untrack(|| {
        (
            num(engine, obj, p.spacing),
            side_padding(engine, obj, p.left, p.padding),
            side_padding(engine, obj, p.right, p.padding),
            side_padding(engine, obj, p.top, p.padding),
            side_padding(engine, obj, p.bottom, p.padding),
        )
    });
    let rtl = axis == Axis::Horizontal
        && engine.untrack(|| num(engine, obj, Row::LAYOUT_DIRECTION.index)) as i32
            == layout_direction::RIGHT_TO_LEFT;
    let children: Vec<(ObjectId, f64, f64)> = engine.untrack(|| {
        child_items(engine, obj)
            .into_iter()
            .filter(|c| engine.read(*c, Item::VISIBLE.index).to_boolean())
            .map(|c| {
                (
                    c,
                    num(engine, c, Item::WIDTH.index),
                    num(engine, c, Item::HEIGHT.index),
                )
            })
            .filter(|(_, w, h)| *w > 0.0 && *h > 0.0)
            .collect()
    });
    let mut along = 0.0;
    let mut across: f64 = 0.0;
    let mut placed = Vec::with_capacity(children.len());
    for (i, (c, w, h)) in children.iter().enumerate() {
        if i > 0 {
            along += spacing;
        }
        let (len, thick) = match axis {
            Axis::Horizontal => (*w, *h),
            Axis::Vertical => (*h, *w),
        };
        placed.push((*c, along, len));
        along += len;
        across = across.max(thick);
    }
    let (iw, ih) = match axis {
        Axis::Horizontal => (left + along + right, top + across + bottom),
        Axis::Vertical => (left + across + right, top + along + bottom),
    };
    engine.batch(|| {
        write_computed(engine, obj, Item::IMPLICIT_WIDTH.index, Value::Real(iw))?;
        write_computed(engine, obj, Item::IMPLICIT_HEIGHT.index, Value::Real(ih))?;
        // In right-to-left the first child sits at the right edge of the
        // row's actual width, which may follow the implicit width just set.
        let width = if rtl {
            engine.untrack(|| num(engine, obj, Item::WIDTH.index))
        } else {
            0.0
        };
        for (c, at, len) in &placed {
            let (x, y) = match axis {
                Axis::Horizontal if rtl => (width - right - at - len, top),
                Axis::Horizontal => (left + at, top),
                Axis::Vertical => (left, top + at),
            };
            set_pos(engine, *c, x, y)?;
        }
        Ok(())
    })
}

fn set_pos(engine: &Engine, c: ObjectId, x: f64, y: f64) -> Result<(), EngineError> {
    for (p, v) in [(Item::X.index, x), (Item::Y.index, y)] {
        let cur = engine.untrack(|| num(engine, c, p));
        if cur != v {
            engine.write(c, p, Value::Real(v), WriteKind::Imperative)?;
        }
    }
    Ok(())
}
