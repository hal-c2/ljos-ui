//! `layout-occluded`: something to operate is under another item.
//!
//! The visible part of each control is sampled on a grid. At each point
//! the items drawn there are walked from the top down to the control:
//! the first one that is neither inside the control nor holds it, and that
//! takes the pointer or paints an opaque fill there, covers the point. A
//! control with `limits::audit::OCCLUDED_SHARE` of its points covered is
//! reported with what covers most of them.
//!
//! Left out: a control an open modal popup shuts off (that is what modal
//! means), and covers in a menu, a tooltip or a combo box's list, which
//! close as soon as the user moves on, or in a dialog or drawer, which is
//! there to be answered and closed. A picture over a control does not
//! count: it is as often an icon laid on it as a cover.

use std::collections::HashMap;

use ljos_controls::{ComboBox, Dialog, Drawer, Menu, ToolTip, item_popup};
use ljos_design::limits::SURFACE_MIN_ALPHA;
use ljos_design::limits::audit::OCCLUDED_SHARE;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{Item, MouseArea};
use ljos_render::kurbo::{Point, Rect};
use ljos_render::{DrawOp, FrameSnapshot, NodeId, PaintItem};

use crate::cx::{Cx, acted_on, ancestors, is_a, scene_rect, within, words};

/// Points along each side of the grid.
const GRID: usize = 5;

/// Whether the cover is part of a popup that closes as the user moves on,
/// or that asks to be answered and closed.
fn transient(engine: &Engine, cover: ObjectId) -> bool {
    std::iter::once(cover)
        .chain(ancestors(engine, cover))
        .filter_map(|a| item_popup(engine, a))
        .any(|p| {
            is_a::<Menu>(engine, p)
                || is_a::<ToolTip>(engine, p)
                || is_a::<Dialog>(engine, p)
                || is_a::<Drawer>(engine, p)
                || engine
                    .untrack(|| engine.read_by_name(p, "parent"))
                    .as_object()
                    .is_some_and(|o| is_a::<ComboBox>(engine, o))
        })
}

/// Whether `paint` hides what is under `p` (world).
fn paints_opaque(snapshot: &FrameSnapshot, paint: &PaintItem, p: Point) -> bool {
    let Some(list) = &paint.list else {
        return false;
    };
    if paint.opacity < SURFACE_MIN_ALPHA || !snapshot.clip_chain_contains(paint.clip, p) {
        return false;
    }
    let local = paint.world.inverse() * p;
    list.ops().iter().any(|op| match op {
        DrawOp::Rect { rect, fill, .. } => {
            rect.contains(local)
                && match fill {
                    ljos_render::Paint::Solid(c) => c.a >= SURFACE_MIN_ALPHA,
                    other => other.is_opaque(),
                }
        }
        _ => false,
    })
}

fn takes_press(engine: &Engine, item: ObjectId) -> bool {
    if is_a::<MouseArea>(engine, item) {
        return engine.untrack(|| {
            engine.read(item, Item::ENABLED.index).to_boolean()
                && engine.read_by_name(item, "acceptedButtons").to_number() != 0.0
        });
    }
    ljos_quick::takes_pointer(engine, item)
}

pub(crate) fn check(cx: &mut Cx) {
    let scene = cx.scene;
    let Some(snapshot) = scene.snapshot() else {
        return;
    };
    let paints: HashMap<NodeId, &PaintItem> = snapshot.items.iter().map(|p| (p.node, p)).collect();
    let window = cx.scene.viewport();
    for (key, _, data) in cx.nodes().iter() {
        if !acted_on(data) {
            continue;
        }
        let Some(item) = cx.item(*key) else { continue };
        let Some(seen) = scene_rect(cx.engine, item).map(|r| r.intersect(window)) else {
            continue;
        };
        if seen.width() < 1.0 || seen.height() < 1.0 {
            continue;
        }
        let (mut looked, mut covered) = (0usize, 0usize);
        let mut by: Vec<(ObjectId, usize)> = Vec::new();
        for p in grid(seen) {
            let drawn = cx.scene.items_drawn_at(cx.engine, p);
            if !drawn.iter().any(|h| within(cx.engine, *h, item)) {
                // Clipped away here, or drawn by none of its own items.
                continue;
            }
            // A modal popup shuts it off here: covered on purpose. What
            // a press meets is what is drawn less what a modal popup shuts
            // off.
            let pressed = cx.scene.items_at(cx.engine, p);
            if !pressed.iter().any(|h| within(cx.engine, *h, item)) {
                continue;
            }
            looked += 1;
            for h in drawn {
                if within(cx.engine, h, item) || within(cx.engine, item, h) {
                    break;
                }
                let opaque = cx
                    .scene
                    .node_of(h)
                    .and_then(|n| paints.get(&n))
                    .is_some_and(|paint| paints_opaque(snapshot, paint, p));
                if (opaque || takes_press(cx.engine, h)) && !transient(cx.engine, h) {
                    covered += 1;
                    match by.iter_mut().find(|(o, _)| *o == h) {
                        Some((_, n)) => *n += 1,
                        None => by.push((h, 1)),
                    }
                    break;
                }
            }
        }
        if looked == 0 || (covered as f32) < OCCLUDED_SHARE * looked as f32 {
            continue;
        }
        // The first of those that cover most.
        let Some(cover) = by
            .iter()
            .fold(None::<(ObjectId, usize)>, |best, &(o, n)| match best {
                Some((_, m)) if m >= n => best,
                _ => Some((o, n)),
            })
            .map(|(o, _)| o)
        else {
            continue;
        };
        let share = covered as f64 / looked as f64;
        let message = format!(
            "{} is {:.0} % covered by {}{}",
            words(cx, item),
            share * 100.0,
            words(cx, cover),
            crate::cx::at(cx.engine, cover),
        );
        let f = cx.report("layout-occluded", Some(item), message);
        f.measured = Some(format!(
            "{:.0} % covered >= {:.0} %",
            share * 100.0,
            OCCLUDED_SHARE * 100.0
        ));
    }
}

/// The points of a `GRID` x `GRID` grid inside `r`, at the centres of its
/// cells.
fn grid(r: Rect) -> impl Iterator<Item = Point> {
    let n = GRID as f64;
    (0..GRID).flat_map(move |j| {
        (0..GRID).map(move |i| {
            Point::new(
                r.x0 + r.width() * (i as f64 + 0.5) / n,
                r.y0 + r.height() * (j as f64 + 0.5) / n,
            )
        })
    })
}
