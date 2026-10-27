//! `layout-fill-crushed`: an item that fills a row or a column of a layout
//! is laid out narrower than what it holds needs, while a sibling that
//! does not fill keeps its preferred size and could give the room.
//!
//! What an item needs is a floor, never a guess: a button's implicit size,
//! a `Layout.minimum` size the application gave, and what layouts and
//! positioners add up from those along their axis (the widest across it).
//! A text, an image, a plain item or anything that scrolls needs nothing,
//! so a crushed item is found only when a control in it is squeezed. The
//! other symptoms (a squeezed control, a clipped label, an overlap) are
//! found by their own rules far from the cause; this finding names it.

use ljos_controls::{AbstractButton, Control};
use ljos_engine::{Engine, MetaRef, ObjectId, Provenance, QmlType};
use ljos_quick::{
    Column, ColumnLayout, Flow, Grid, GridLayout, Layout, LayoutAttached, Row, RowLayout,
    StackLayout,
};

use crate::cx::{
    Cx, DEPTH, is_a, is_scroller, number, shown_items, shown_opacity, slack, trim, words,
};

const H: usize = 0;
const V: usize = 1;

const SIZE: [&str; 2] = ["width", "height"];

/// How a container places its children on an axis.
enum Places {
    /// One after another: their needs add up, with the spacing between.
    Along,
    /// Side by side across the axis: the widest is the need.
    Across,
}

fn places(e: &Engine, item: ObjectId, axis: usize) -> Option<Places> {
    let row = is_a::<RowLayout>(e, item) || is_a::<Row>(e, item);
    let column = is_a::<ColumnLayout>(e, item) || is_a::<Column>(e, item);
    if (row && axis == H) || (column && axis == V) {
        Some(Places::Along)
    } else if row
        || column
        || is_a::<GridLayout>(e, item)
        || is_a::<Grid>(e, item)
        || is_a::<Flow>(e, item)
        || is_a::<StackLayout>(e, item)
    {
        // A grid's columns add up too, but the widest cell is a floor.
        Some(Places::Across)
    } else {
        None
    }
}

fn is_layout(e: &Engine, item: ObjectId) -> bool {
    is_a::<RowLayout>(e, item)
        || is_a::<ColumnLayout>(e, item)
        || is_a::<GridLayout>(e, item)
        || is_a::<StackLayout>(e, item)
}

fn attached(e: &Engine, item: ObjectId) -> Option<ObjectId> {
    e.existing_attached_object(item, &MetaRef::Static(Layout::static_meta()))
}

fn given(e: &Engine, att: ObjectId, p: ljos_engine::PropIndex) -> bool {
    matches!(
        e.provenance(att, p),
        Some(Provenance::Assigned | Provenance::Bound)
    )
}

fn read(e: &Engine, att: ObjectId, p: ljos_engine::PropIndex) -> f64 {
    e.untrack(|| e.read(att, p)).to_number()
}

/// Whether a child of a layout takes the extra room on `axis`: as it says,
/// or, said nothing, when it is a layout itself.
fn fills(e: &Engine, item: ObjectId, axis: usize) -> bool {
    let p = [LayoutAttached::FILL_WIDTH, LayoutAttached::FILL_HEIGHT][axis].index;
    match attached(e, item).filter(|a| given(e, *a, p)) {
        Some(att) => e.untrack(|| e.read(att, p)).to_boolean(),
        None => is_layout(e, item),
    }
}

/// The size a child of a layout asks for on `axis`.
fn preferred(e: &Engine, item: ObjectId, axis: usize) -> f64 {
    let p = [
        LayoutAttached::PREFERRED_WIDTH,
        LayoutAttached::PREFERRED_HEIGHT,
    ][axis]
        .index;
    match attached(e, item).map(|a| read(e, a, p)) {
        Some(v) if v >= 0.0 => v,
        _ => number(e, item, ["implicitWidth", "implicitHeight"][axis]),
    }
}

/// The child's `Layout` margins on `axis`, both ends.
fn margins(e: &Engine, item: ObjectId, axis: usize) -> f64 {
    let Some(att) = attached(e, item) else {
        return 0.0;
    };
    let [a, b] = [
        [LayoutAttached::LEFT_MARGIN, LayoutAttached::RIGHT_MARGIN],
        [LayoutAttached::TOP_MARGIN, LayoutAttached::BOTTOM_MARGIN],
    ][axis];
    read(e, att, a.index) + read(e, att, b.index)
}

/// The least `item` needs on `axis`, as a floor; see the module.
fn need(e: &Engine, shown: &[ObjectId], item: ObjectId, axis: usize, depth: usize) -> f64 {
    let own = attached(e, item)
        .map(|a| {
            let p = [
                LayoutAttached::MINIMUM_WIDTH,
                LayoutAttached::MINIMUM_HEIGHT,
            ][axis]
                .index;
            read(e, a, p)
        })
        .filter(|v| v.is_finite())
        .unwrap_or(0.0);
    if depth >= DEPTH || is_scroller(e, item) {
        return own;
    }
    if is_a::<AbstractButton>(e, item) {
        return own.max(number(e, item, ["implicitWidth", "implicitHeight"][axis]));
    }
    let children = || {
        e.visual_children(item)
            .into_iter()
            .filter(|c| shown.contains(c))
    };
    let held = if is_a::<Control>(e, item) {
        // What a control holds needs its padding around it.
        let content = e
            .untrack(|| e.read_by_name(item, "contentItem"))
            .as_object()
            .filter(|c| shown.contains(c));
        let pad = match axis {
            H => number(e, item, "leftPadding") + number(e, item, "rightPadding"),
            _ => number(e, item, "topPadding") + number(e, item, "bottomPadding"),
        };
        content.map_or(0.0, |c| pad + need(e, shown, c, axis, depth + 1))
    } else {
        match places(e, item, axis) {
            Some(Places::Along) => {
                let needs: Vec<f64> = children()
                    .map(|c| need(e, shown, c, axis, depth + 1) + margins(e, c, axis))
                    .collect();
                let gaps = needs.len().saturating_sub(1) as f64 * number(e, item, "spacing");
                needs.iter().sum::<f64>() + gaps
            }
            Some(Places::Across) | None => children()
                .map(|c| need(e, shown, c, axis, depth + 1) + margins(e, c, axis))
                .fold(0.0, f64::max),
        }
    };
    own.max(held)
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let shown = shown_items(cx);
    for &layout in &shown {
        let axis = if is_a::<RowLayout>(e, layout) {
            H
        } else if is_a::<ColumnLayout>(e, layout) {
            V
        } else {
            continue;
        };
        let children: Vec<ObjectId> = e
            .visual_children(layout)
            .into_iter()
            .filter(|c| shown.contains(c) && shown_opacity(e, *c) > 0.0)
            .collect();
        for &fill in children.iter().filter(|c| fills(e, **c, axis)) {
            let size = number(e, fill, SIZE[axis]);
            let needs = need(e, &shown, fill, axis, 0);
            if size + slack() >= needs {
                continue;
            }
            // The sibling that keeps the most room it could give, if that
            // is room enough: a sibling too small to make the difference
            // is not the cause, the layout is too small for its children.
            let short = needs - size;
            let giver = children
                .iter()
                .filter(|s| **s != fill && !fills(e, **s, axis))
                .map(|&s| {
                    let at = number(e, s, SIZE[axis]);
                    (
                        s,
                        at,
                        at - need(e, &shown, s, axis, 0),
                        preferred(e, s, axis),
                    )
                })
                .filter(|(_, at, spare, pref)| *spare + slack() >= short && at + slack() >= *pref)
                .max_by(|a, b| a.2.total_cmp(&b.2));
            let Some((sibling, at, _, _)) = giver else {
                continue;
            };
            let (side, min) = [
                ("wide", "Layout.minimumWidth"),
                ("tall", "Layout.minimumHeight"),
            ][axis];
            let cap = ["Layout.maximumWidth", "Layout.maximumHeight"][axis];
            let message = format!(
                "{} fills its layout but is {} px {side}, while what it holds needs {}; {} \
                 beside it keeps its preferred {}. Give it a `{min}`, or cap the sibling \
                 with `{cap}`",
                words(cx, fill),
                trim(size),
                trim(needs),
                words(cx, sibling),
                trim(at),
            );
            cx.report("layout-fill-crushed", Some(fill), message)
                .measured = Some(format!("{} < {}", trim(size), trim(needs)));
        }
    }
}
