//! `layout-resize-handle`: the handle of a `SplitView` draws nothing, so
//! the edge that resizes the panes is found only by chance.
//!
//! A handle is what the view makes from its `handle` component: one of
//! its own visual children that is neither its content nor its
//! background. It is looked at as it is drawn at rest; one drawn only on
//! hover is not there for a user to aim at. A frameless window's own
//! resize edges are left out: the scene cannot tell an edge drag from a
//! press there.

use std::collections::HashMap;

use ljos_controls::SplitView;
use ljos_engine::{Engine, ObjectId};
use ljos_render::{DrawOp, NodeId, Paint, PaintItem};

use crate::cx::{Cx, DEPTH, is_a, scene_rect, shown_items, trim};

fn shows(paint: &Paint) -> bool {
    match paint {
        Paint::Solid(c) => c.a > 0.0,
        _ => true,
    }
}

/// Whether one of the ops draws something one can see.
fn draws(paint: &PaintItem) -> bool {
    let Some(list) = &paint.list else {
        return false;
    };
    paint.opacity > 0.0
        && list.ops().iter().any(|op| match op {
            DrawOp::Rect {
                rect, fill, border, ..
            } => {
                rect.width() > 0.0
                    && rect.height() > 0.0
                    && (shows(fill)
                        || border
                            .as_ref()
                            .is_some_and(|b| b.width > 0.0 && shows(&b.paint)))
            }
            DrawOp::Shadow { color, .. } => color.a > 0.0,
            DrawOp::Text { color, .. } => color.a > 0.0,
            DrawOp::Glyphs { paint, .. } | DrawOp::Path { paint, .. } => shows(paint),
            _ => true,
        })
}

/// The handles of a split view: its own children that are not its
/// content or background.
fn handles(e: &Engine, view: ObjectId) -> Vec<ObjectId> {
    let own = |name| e.untrack(|| e.read_by_name(view, name)).as_object();
    let (content, background) = (own("contentItem"), own("background"));
    e.visual_children(view)
        .into_iter()
        .filter(|c| Some(*c) != content && Some(*c) != background)
        .collect()
}

fn subtree(e: &Engine, root: ObjectId) -> Vec<ObjectId> {
    let mut out = vec![root];
    let mut at = 0;
    while at < out.len() && out.len() < DEPTH {
        out.extend(e.visual_children(out[at]));
        at += 1;
    }
    out
}

pub(crate) fn check(cx: &mut Cx) {
    let scene = cx.scene;
    let Some(snapshot) = scene.snapshot() else {
        return;
    };
    let paints: HashMap<NodeId, &PaintItem> = snapshot.items.iter().map(|p| (p.node, p)).collect();
    let e = cx.engine;
    let shown = shown_items(cx);
    for view in shown.iter().copied().filter(|i| is_a::<SplitView>(e, *i)) {
        for handle in handles(e, view) {
            if !shown.contains(&handle) {
                continue;
            }
            let drawn = subtree(e, handle).into_iter().any(|i| {
                cx.scene
                    .node_of(i)
                    .and_then(|n| paints.get(&n))
                    .is_some_and(|p| draws(p))
            });
            if drawn {
                continue;
            }
            let size = scene_rect(e, handle)
                .map(|r| format!(" ({} by {} px)", trim(r.width()), trim(r.height())))
                .unwrap_or_default();
            cx.report(
                "layout-resize-handle",
                Some(handle),
                format!(
                    "the split view's handle{size} draws nothing, so the edge that resizes \
                     the panes is found only by chance; draw a line or grip with 3:1 against \
                     the panes, and change it on `SplitHandle.hovered` and `SplitHandle.pressed`"
                ),
            );
        }
    }
}
