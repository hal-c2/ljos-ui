//! `layout-overlap`: two controls, or two texts, side by side in one
//! parent are drawn over each other, so one hides part of the other.
//!
//! Siblings only: a badge or a label laid on a control on purpose is its
//! child, not its sibling. What a stack shows one at a time (a
//! `StackLayout`, `SwipeView`, `StackView`) is left out, and so is a text's
//! empty room: a text is measured by the words it paints, not its box.
//! Scroll bars are left out too: two of them meet in a corner by design.
//!
//! Controls in two cells of one layout or positioner are measured as well:
//! the layout keeps its cells apart, so a control crossing into its
//! neighbour's cell has outgrown its own (a flow of buttons narrower than
//! they are, beside a Send button). Only what the cells show counts, after
//! the clip of anything between the control and the layout.

use ljos_controls::{ScrollBar, ScrollIndicator, StackView, SwipeView};
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{
    Column, ColumnLayout, Flow, Grid, GridLayout, Item, Row, RowLayout, StackLayout, Text,
};
use ljos_render::kurbo::Rect;

use crate::cx::{
    Cx, acted_on, ancestors, is_a, is_scroller, scene_rect, shown_items, shown_opacity, slack,
    trim, within,
};

/// `Text.AlignRight`, `Text.AlignHCenter`, `Text.AlignBottom` and
/// `Text.AlignVCenter`, as Qt documents them.
const ALIGN_RIGHT: i64 = 0x2;
const ALIGN_HCENTER: i64 = 0x4;
const ALIGN_BOTTOM: i64 = 0x40;
const ALIGN_VCENTER: i64 = 0x80;

/// What a text paints: its words' extent, placed in its box as aligned.
fn painted(e: &Engine, text: ObjectId, r: Rect) -> Option<Rect> {
    let (words, cw, ch, h, v) = e.untrack(|| {
        (
            e.read(text, Text::TEXT.index)
                .to_js_string()
                .as_str()
                .trim()
                .to_string(),
            e.read_by_name(text, "contentWidth").to_number(),
            e.read_by_name(text, "contentHeight").to_number(),
            e.read_by_name(text, "horizontalAlignment").to_number() as i64,
            e.read_by_name(text, "verticalAlignment").to_number() as i64,
        )
    });
    if words.is_empty() || !cw.is_finite() || !ch.is_finite() {
        return None;
    }
    let (cw, ch) = (cw.min(r.width()), ch.min(r.height()));
    let x0 = if h & ALIGN_RIGHT != 0 {
        r.x1 - cw
    } else if h & ALIGN_HCENTER != 0 {
        r.x0 + (r.width() - cw) / 2.0
    } else {
        r.x0
    };
    let y0 = if v & ALIGN_BOTTOM != 0 {
        r.y1 - ch
    } else if v & ALIGN_VCENTER != 0 {
        r.y0 + (r.height() - ch) / 2.0
    } else {
        r.y0
    };
    Some(Rect::new(x0, y0, x0 + cw, y0 + ch))
}

fn stacks(e: &Engine, parent: ObjectId) -> bool {
    std::iter::once(parent)
        .chain(ancestors(e, parent).take(2))
        .any(|p| is_a::<StackLayout>(e, p) || is_a::<SwipeView>(e, p) || is_a::<StackView>(e, p))
}

/// Whether `obj` places its children apart from each other.
fn keeps_apart(e: &Engine, obj: ObjectId) -> bool {
    is_a::<RowLayout>(e, obj)
        || is_a::<ColumnLayout>(e, obj)
        || is_a::<GridLayout>(e, obj)
        || is_a::<Row>(e, obj)
        || is_a::<Column>(e, obj)
        || is_a::<Grid>(e, obj)
        || is_a::<Flow>(e, obj)
}

/// What of `item` shows inside `top`: its box cut by the clip of every
/// item between them, or `None` when something between them scrolls it.
fn shown_below(e: &Engine, item: ObjectId, top: ObjectId, r: Rect) -> Option<Rect> {
    let mut r = r;
    for a in ancestors(e, item).take_while(|a| *a != top) {
        if is_scroller(e, a) {
            return None;
        }
        if e.untrack(|| e.read(a, Item::CLIP.index)).to_boolean() {
            r = r.intersect(scene_rect(e, a)?);
        }
    }
    Some(r)
}

/// The nearest item above both `a` and `b`.
fn common(e: &Engine, a: ObjectId, b: ObjectId) -> Option<ObjectId> {
    let above_a: Vec<ObjectId> = ancestors(e, a).collect();
    ancestors(e, b).find(|x| above_a.contains(x))
}

fn report(cx: &mut Cx, a: ObjectId, b: ObjectId, over: Rect, what: &str) {
    let other = crate::cx::words(cx, a);
    cx.report(
        "layout-overlap",
        Some(b),
        format!(
            "this is drawn over {} beside it ({} by {} px of the two {what} \
             overlap); let a layout place them, rather than positions that ignore \
             each other's size",
            other,
            trim(over.width()),
            trim(over.height())
        ),
    )
    .measured = Some(format!(
        "{}x{} overlap",
        trim(over.width()),
        trim(over.height())
    ));
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let shown = shown_items(cx);
    let mut parents: Vec<ObjectId> = Vec::new();
    for item in &shown {
        if let Some(p) = e.visual_parent(*item)
            && !parents.contains(&p)
        {
            parents.push(p);
        }
    }
    for parent in parents {
        if stacks(e, parent) {
            continue;
        }
        // (item, what it shows, "control" or "text")
        let mut pieces: Vec<(ObjectId, Rect, &'static str)> = Vec::new();
        for c in e.visual_children(parent) {
            if !shown.contains(&c)
                || shown_opacity(e, c) <= 0.0
                || is_a::<ScrollBar>(e, c)
                || is_a::<ScrollIndicator>(e, c)
            {
                continue;
            }
            let Some(r) = scene_rect(e, c) else { continue };
            let node = cx
                .scene
                .accessible_node(c)
                .and_then(|k| cx.scene.accessibility_tree().get(k).cloned());
            if node.as_ref().is_some_and(acted_on) {
                pieces.push((c, r, "control"));
            } else if is_a::<Text>(e, c)
                && let Some(p) = painted(e, c, r)
            {
                pieces.push((c, p, "text"));
            }
        }
        for (i, (a, ra, ka)) in pieces.iter().enumerate() {
            for (b, rb, kb) in &pieces[i + 1..] {
                if ka != kb {
                    continue;
                }
                let over = ra.intersect(*rb);
                if over.width() <= slack() || over.height() <= slack() {
                    continue;
                }
                let what = if *ka == "text" { "texts" } else { "controls" };
                report(cx, *a, *b, over, what);
            }
        }
    }
    across_cells(cx, &shown);
}

/// Controls in two cells of one layout that cross each other.
fn across_cells(cx: &mut Cx, shown: &[ObjectId]) {
    let e = cx.engine;
    let nodes = cx.scene.accessibility_tree();
    let mut controls: Vec<(ObjectId, Rect)> = Vec::new();
    for &c in shown {
        if shown_opacity(e, c) <= 0.0 || is_a::<ScrollBar>(e, c) || is_a::<ScrollIndicator>(e, c) {
            continue;
        }
        let acted = cx
            .scene
            .accessible_node(c)
            .and_then(|k| nodes.get(k))
            .is_some_and(acted_on);
        // A control's own parts are the control.
        if !acted || controls.iter().any(|(o, _)| within(e, c, *o)) {
            continue;
        }
        if let Some(r) = scene_rect(e, c) {
            controls.push((c, r));
        }
    }
    for (i, (a, ra)) in controls.iter().enumerate() {
        for (b, rb) in &controls[i + 1..] {
            let crude = ra.intersect(*rb);
            if crude.width() <= slack() || crude.height() <= slack() {
                continue;
            }
            // Siblings were measured above.
            if e.visual_parent(*a) == e.visual_parent(*b) {
                continue;
            }
            let Some(top) = common(e, *a, *b) else {
                continue;
            };
            if !keeps_apart(e, top) || stacks(e, top) {
                continue;
            }
            let (Some(sa), Some(sb)) = (shown_below(e, *a, top, *ra), shown_below(e, *b, top, *rb))
            else {
                continue;
            };
            let over = sa.intersect(sb);
            if over.width() <= slack() || over.height() <= slack() {
                continue;
            }
            report(cx, *a, *b, over, "controls");
        }
    }
}
