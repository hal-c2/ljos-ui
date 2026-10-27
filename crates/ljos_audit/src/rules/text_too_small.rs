//! `text-too-small`, as the scene draws it: text whose size, after every
//! binding and the theme, is below `limits::TEXT_MIN_PX`. The static lint
//! reads a literal `font.pixelSize`; this reads what each glyph run is
//! drawn at, so a size computed from the window or a scale comes too.
//!
//! A run's size before any scale of the item: a preview drawn smaller on
//! purpose (a thumbnail of a page) is left out.

use ljos_design::limits::TEXT_MIN_PX;
use ljos_engine::ObjectId;
use ljos_render::DrawOp;

use crate::cx::{Cx, trim};

pub(crate) fn check(cx: &mut Cx) {
    let scene = cx.scene;
    let Some(snapshot) = scene.snapshot() else {
        return;
    };
    let mut seen: Vec<ObjectId> = Vec::new();
    for paint in &snapshot.items {
        let Some(list) = &paint.list else { continue };
        if paint.opacity <= 0.0 {
            continue;
        }
        let size = list.ops().iter().find_map(|op| match op {
            DrawOp::Glyphs { run, .. } if !run.glyphs.is_empty() => Some(run.font_size),
            DrawOp::Text { size, text, .. } if !text.trim().is_empty() => Some(*size),
            _ => None,
        });
        let Some(size) = size.filter(|s| *s > 0.0 && *s < TEXT_MIN_PX) else {
            continue;
        };
        let Some(item) = cx.scene.item_of(paint.node) else {
            continue;
        };
        if seen.contains(&item) {
            continue;
        }
        seen.push(item);
        cx.report(
            "text-too-small",
            Some(item),
            format!(
                "the text is drawn at {} px, below the {} px the smallest reading size allows; \
                 use a step of the type scale at or above it, and set secondary text apart by \
                 weight or colour",
                trim(f64::from(size)),
                trim(f64::from(TEXT_MIN_PX))
            ),
        )
        .measured = Some(format!(
            "{} px < {} px",
            trim(f64::from(size)),
            trim(f64::from(TEXT_MIN_PX))
        ));
    }
}
