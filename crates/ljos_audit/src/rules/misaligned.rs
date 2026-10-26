//! `layout-misaligned` (Hint): things meant to line up miss by a little.
//! A design places things apart on purpose by more than a few pixels;
//! a few pixels off is a slip the eye still sees as a ragged edge.
//!
//! Three slips are looked for:
//! - controls stacked in one column whose left edges differ by 1 to
//!   `limits::MISALIGN_MAX_PX`;
//! - fields of one kind (combo boxes, text fields, spin boxes) stacked in
//!   one column, most of one width and some of another;
//! - a menu where some items have an icon and others not, so their words
//!   start at different places.

use ljos_controls::{ComboBox, Menu, MenuItem, SpinBox, TextField, item_popup};
use ljos_design::limits::MISALIGN_MAX_PX;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::Text;
use ljos_render::kurbo::Rect;

use super::controls::button_look;
use crate::cx::{Cx, DEPTH, acted_on, is_a, scene_rect, shown_items, slack, trim};

/// The kind of field, when it is one whose width a form lines up.
fn field_kind(e: &Engine, item: ObjectId) -> Option<&'static str> {
    if is_a::<ComboBox>(e, item) {
        Some("combo box")
    } else if is_a::<TextField>(e, item) {
        Some("text field")
    } else if is_a::<SpinBox>(e, item) {
        Some("spin box")
    } else {
        None
    }
}

/// Siblings stacked one under another: each starts below the one before.
fn stacked(rects: &[Rect]) -> bool {
    rects.len() >= 2
        && rects
            .windows(2)
            .all(|w| w[1].y0 >= w[0].y1 - slack() && w[1].x0 < w[0].x1 && w[0].x0 < w[1].x1)
}

/// The left edge of the first text an item shows.
fn text_start(e: &Engine, item: ObjectId) -> Option<f64> {
    let mut stack = vec![item];
    let mut n = 0;
    while let Some(o) = stack.pop() {
        n += 1;
        if n > DEPTH {
            return None;
        }
        if is_a::<Text>(e, o)
            && e.untrack(|| e.read_by_name(o, "visible").to_boolean())
            && !e
                .untrack(|| e.read(o, Text::TEXT.index))
                .to_js_string()
                .as_str()
                .trim()
                .is_empty()
        {
            return scene_rect(e, o).map(|r| r.x0);
        }
        stack.extend(e.visual_children(o).into_iter().rev());
    }
    None
}

fn columns(cx: &mut Cx) {
    let e = cx.engine;
    let max = f64::from(MISALIGN_MAX_PX);
    let mut parents: Vec<ObjectId> = Vec::new();
    for item in shown_items(cx) {
        if let Some(p) = e.visual_parent(item)
            && !parents.contains(&p)
        {
            parents.push(p);
        }
    }
    for parent in parents {
        let controls: Vec<(ObjectId, Rect)> = e
            .visual_children(parent)
            .into_iter()
            .filter(|c| e.untrack(|| e.read_by_name(*c, "visible").to_boolean()))
            .filter(|c| {
                cx.scene
                    .accessible_node(*c)
                    .and_then(|k| cx.scene.accessibility_tree().get(k).cloned())
                    .is_some_and(|d| acted_on(&d))
            })
            .filter_map(|c| scene_rect(e, c).map(|r| (c, r)))
            .collect();
        let rects: Vec<Rect> = controls.iter().map(|(_, r)| *r).collect();
        if !stacked(&rects) {
            continue;
        }
        // Left edges: the most common one is the line; a few pixels off it
        // is a slip.
        let line = most_common(rects.iter().map(|r| r.x0));
        for (c, r) in &controls {
            let off = (r.x0 - line).abs();
            if off > slack() && off <= max {
                cx.report(
                    "layout-misaligned",
                    Some(*c),
                    format!(
                        "this control starts {} px from the left edge the others in its column \
                         share; let one layout align them",
                        trim(off)
                    ),
                )
                .measured = Some(format!("{} px off", trim(off)));
            }
        }
        // Fields of a kind: most share a width; the others are off.
        let fields: Vec<(ObjectId, &'static str, f64)> = controls
            .iter()
            .filter_map(|(c, r)| field_kind(e, *c).map(|k| (*c, k, r.width())))
            .collect();
        for kind in ["combo box", "text field", "spin box"] {
            let of: Vec<_> = fields.iter().filter(|f| f.1 == kind).collect();
            if of.len() < 3 {
                continue;
            }
            let width = most_common(of.iter().map(|f| f.2));
            let same = of.iter().filter(|f| (f.2 - width).abs() <= slack()).count();
            if same * 2 <= of.len() {
                continue;
            }
            for f in of.iter().filter(|f| (f.2 - width).abs() > slack()) {
                cx.report(
                    "layout-misaligned",
                    Some(f.0),
                    format!(
                        "this {kind} is {} px wide where the others of its column are {}; give \
                         fields of a kind one width",
                        trim(f.2),
                        trim(width)
                    ),
                )
                .measured = Some(format!("{} px wide, the others {}", trim(f.2), trim(width)));
            }
        }
    }
}

fn menus(cx: &mut Cx) {
    let e = cx.engine;
    let mut seen: Vec<ObjectId> = Vec::new();
    let items: Vec<ObjectId> = shown_items(cx)
        .into_iter()
        .filter(|i| is_a::<MenuItem>(e, *i))
        .collect();
    for item in &items {
        let Some(menu) = std::iter::once(*item)
            .chain(crate::cx::ancestors(e, *item))
            .find_map(|a| item_popup(e, a))
            .filter(|p| is_a::<Menu>(e, *p))
        else {
            continue;
        };
        if seen.contains(&menu) {
            continue;
        }
        seen.push(menu);
        let of: Vec<(ObjectId, bool, Option<f64>)> = items
            .iter()
            .filter(|i| {
                std::iter::once(**i)
                    .chain(crate::cx::ancestors(e, **i))
                    .find_map(|a| item_popup(e, a))
                    == Some(menu)
            })
            .map(|i| {
                let icon = button_look(e, *i).is_some_and(|l| l.icon);
                (*i, icon, text_start(e, *i))
            })
            .collect();
        let Some(with_icon) = of.iter().find(|(_, icon, at)| *icon && at.is_some()) else {
            continue;
        };
        let start = with_icon.2.unwrap_or_default();
        for (i, _, at) in of.iter().filter(|(_, icon, _)| !icon) {
            let Some(at) = at else { continue };
            let off = (start - at).abs();
            if off > slack() {
                cx.report(
                    "layout-misaligned",
                    Some(*i),
                    format!(
                        "this menu item has no icon and its words start {} px from those of the \
                         items with one; give every item an icon, or none",
                        trim(off)
                    ),
                )
                .measured = Some(format!("{} px off", trim(off)));
            }
        }
    }
}

/// The value most of them have, the first of those on a tie.
fn most_common(values: impl Iterator<Item = f64>) -> f64 {
    let values: Vec<f64> = values.collect();
    let mut best = (0usize, values.first().copied().unwrap_or_default());
    for v in &values {
        let n = values.iter().filter(|w| (*w - v).abs() <= slack()).count();
        if n > best.0 {
            best = (n, *v);
        }
    }
    best.1
}

pub(crate) fn check(cx: &mut Cx) {
    columns(cx);
    menus(cx);
}
