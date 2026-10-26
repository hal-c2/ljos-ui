//! The surfaces that float over the window: the popups that are open
//! (menus, dialogs, drawers, tool tips), each with the item that draws it
//! and its background.

use ljos_design::color::{Srgb, Srgba};
use ljos_engine::{Engine, ObjectId};
use ljos_render::{DrawOp, FrameSnapshot, Paint, Rgba};

use crate::cx::{Cx, shown_items};

pub(crate) struct Surface {
    pub popup: ObjectId,
    /// The item the popup is drawn as.
    pub item: ObjectId,
    pub background: Option<ObjectId>,
}

/// The popups the scene shows, in the order they are drawn.
pub(crate) fn open_surfaces(cx: &Cx) -> Vec<Surface> {
    let e = cx.engine;
    let mut out: Vec<Surface> = Vec::new();
    for item in shown_items(cx) {
        let Some(popup) = ljos_controls::item_popup(e, item) else {
            continue;
        };
        if out.iter().any(|s| s.popup == popup) {
            continue;
        }
        let background = object(e, popup, "background").filter(|b| ljos_quick::is_item(e, *b));
        out.push(Surface {
            popup,
            item,
            background,
        });
    }
    out
}

/// An object property, `None` when it holds none.
pub(crate) fn object(e: &Engine, obj: ObjectId, name: &str) -> Option<ObjectId> {
    e.untrack(|| e.read_by_name(obj, name))
        .as_object()
        .filter(|o| e.is_alive(*o))
}

/// The renderer's premultiplied colour as straight sRGB.
pub(crate) fn straight(c: Rgba) -> Srgba {
    if c.a <= 0.0 {
        return Srgba::TRANSPARENT;
    }
    Srgba::new(c.r / c.a, c.g / c.a, c.b / c.a, c.a)
}

/// The first rectangle an item paints: its fill, as shown (with the
/// item's opacity), and whether it has a border that can be seen.
pub(crate) fn own_rect(cx: &Cx, snapshot: &FrameSnapshot, item: ObjectId) -> Option<(Srgba, bool)> {
    for p in &snapshot.items {
        if cx.scene.item_of(p.node) != Some(item) {
            continue;
        }
        // A node of the item's that draws nothing (one that only clips or
        // holds others) is passed by: a later one may paint its fill.
        let Some(list) = p.list.as_ref() else {
            continue;
        };
        for op in list.ops() {
            if let DrawOp::Rect { fill, border, .. } = op {
                let fill = match fill {
                    Paint::Solid(c) => straight(*c),
                    _ => continue,
                };
                let fill = Srgba::new(fill.r, fill.g, fill.b, fill.a * p.opacity);
                let edged = border
                    .as_ref()
                    .is_some_and(|b| b.width > 0.0 && !b.paint.is_invisible());
                return Some((fill, edged));
            }
        }
    }
    None
}

/// The colour that covers most of a picture, and the share of it.
pub(crate) fn dominant(colours: impl Iterator<Item = Srgb>) -> Option<(Srgb, f32)> {
    let mut counts: std::collections::HashMap<[u8; 3], usize> = Default::default();
    let mut total = 0usize;
    for c in colours {
        total += 1;
        *counts.entry(c.to_u8()).or_default() += 1;
    }
    let (k, n) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    Some((
        Srgb::from_u8(k[0], k[1], k[2]),
        n as f32 / total.max(1) as f32,
    ))
}
