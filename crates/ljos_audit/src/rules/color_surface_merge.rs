//! `color-surface-merge`: a popup is drawn in nearly the colour of what is
//! behind it, with no border and no shadow, so its edge cannot be seen.
//!
//! The background's fill, laid over what is behind it, is held to
//! `SURFACE_MIN_DELTA_E` against the colour that covers most of what is
//! behind it, the scene drawn without the popup. A background the style
//! makes is the style's, which gives its popups a border or a shadow; a
//! card or a pane on the window is not looked at, as nothing in the scene
//! tells a raised surface from a plain rectangle.

use ljos_design::color::{delta_e_ok, over};
use ljos_design::limits::SURFACE_MIN_DELTA_E;
use ljos_engine::ObjectId;
use ljos_render::{DrawOp, FrameSnapshot};

use crate::acting::Act;
use crate::cx::{Cx, declared_by_app, scene_rect, within};
use crate::pixels::{Painter, backdrop};
use crate::surfaces::{dominant, open_surfaces, own_rect};

/// Whether anything inside `item` casts a shadow.
fn shadowed(cx: &Cx, snapshot: &FrameSnapshot, item: ObjectId) -> bool {
    snapshot.items.iter().any(|p| {
        cx.scene
            .item_of(p.node)
            .is_some_and(|i| within(cx.engine, i, item))
            && p.list
                .as_ref()
                .is_some_and(|l| l.ops().iter().any(|op| matches!(op, DrawOp::Shadow { .. })))
    })
}

pub(crate) fn check(act: &mut Act) {
    act.rest();
    let Some(snapshot) = act.scene.snapshot().cloned() else {
        return;
    };
    let mut found = Vec::new();
    {
        let cx = act.cx();
        let e = cx.engine;
        let mut painter = Painter::new(backdrop(e, cx.options));
        for s in open_surfaces(&cx) {
            let Some(bg) = s.background else { continue };
            let own = declared_by_app(e, bg);
            let Some((fill, edged)) = own_rect(&cx, &snapshot, bg) else {
                continue;
            };
            if !own || edged || fill.a <= 0.0 || shadowed(&cx, &snapshot, s.item) {
                continue;
            }
            let Some(region) = scene_rect(e, bg) else {
                continue;
            };
            // What is behind the popup's edge, a few pixels out.
            let Some(under) = painter.draw(&snapshot, region.inflate(4.0, 4.0), |p| {
                !cx.scene
                    .item_of(p.node)
                    .is_some_and(|i| within(e, i, s.item))
            }) else {
                continue;
            };
            let Some((behind, _)) = dominant(under.pixels()) else {
                continue;
            };
            let shown = over(fill, behind);
            let d = delta_e_ok(shown, behind);
            if d < SURFACE_MIN_DELTA_E {
                found.push((s.popup, bg, shown, behind, d));
            }
        }
    }
    for (popup, bg, shown, behind, d) in found {
        let what = crate::cx::item_words(act.engine, popup);
        let message = format!(
            "{what} is drawn in {} over {} behind it, {d:.3} apart in OKLab where {} is the \
             least, and has no border or shadow: its edge cannot be seen",
            shown.to_hex(),
            behind.to_hex(),
            SURFACE_MIN_DELTA_E,
        );
        let f = act.report("color-surface-merge", Some(bg), message);
        f.measured = Some(format!("ΔE {d:.3} < {SURFACE_MIN_DELTA_E}"));
    }
}
