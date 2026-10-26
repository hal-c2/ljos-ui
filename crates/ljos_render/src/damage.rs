//! Where two frames differ, in device pixels.
//!
//! A window drawn on the processor draws again only what changed, and tells
//! the window system only that part (`wl_surface.damage_buffer`). An item
//! that moved, changed what it draws, its opacity or its clips, came, went
//! or fell behind another item damages where it was and where it is now.

use rustc_hash::FxHashMap;
use vello_cpu::kurbo::{Affine, Rect};

use crate::compose::{FrameSnapshot, PaintItem, bbox_of};
use crate::tree::NodeId;

/// A rectangle of device pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PixelRect {
    /// Left column.
    pub x: u32,
    /// Top row.
    pub y: u32,
    /// Columns.
    pub width: u32,
    /// Rows.
    pub height: u32,
}

impl PixelRect {
    /// All of a `width` × `height` picture.
    pub fn whole(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// No pixels at all.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// How many pixels.
    pub fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    fn right(&self) -> u32 {
        self.x + self.width
    }

    fn bottom(&self) -> u32 {
        self.y + self.height
    }

    /// The smallest rectangle holding both.
    pub fn union(&self, other: &Self) -> Self {
        let (x, y) = (self.x.min(other.x), self.y.min(other.y));
        Self {
            x,
            y,
            width: self.right().max(other.right()) - x,
            height: self.bottom().max(other.bottom()) - y,
        }
    }

    /// Whether the two overlap or lie within `gap` pixels of each other.
    fn near(&self, other: &Self, gap: u32) -> bool {
        self.x <= other.right() + gap
            && other.x <= self.right() + gap
            && self.y <= other.bottom() + gap
            && other.y <= self.bottom() + gap
    }

    /// The pixels `r` touches, at least partly, within `width` × `height`.
    fn covering(r: Rect, width: u32, height: u32) -> Option<Self> {
        let x0 = r.x0.floor().max(0.0);
        let y0 = r.y0.floor().max(0.0);
        let x1 = r.x1.ceil().min(f64::from(width));
        let y1 = r.y1.ceil().min(f64::from(height));
        (x1 > x0 && y1 > y0).then_some(Self {
            x: x0 as u32,
            y: y0 as u32,
            width: (x1 - x0) as u32,
            height: (y1 - y0) as u32,
        })
    }
}

/// Device pixels around an item's bounds that antialiasing and a snapped
/// baseline may still reach.
const MARGIN: f64 = 2.0;

/// Rectangles closer than this are drawn as one: each part drawn on its
/// own walks the scene again.
const JOIN_GAP: u32 = 16;

/// More parts than this are drawn as the one rectangle around them all.
const MAX_PARTS: usize = 8;

/// Where `now` differs from `before` when both are drawn at `scale` into
/// `width` × `height` pixels. Empty when they look the same.
pub fn frame_damage(
    before: &FrameSnapshot,
    now: &FrameSnapshot,
    scale: f32,
    width: u32,
    height: u32,
) -> Vec<PixelRect> {
    if width == 0 || height == 0 || std::ptr::eq(before, now) {
        return Vec::new();
    }
    if before.viewport != now.viewport {
        return vec![PixelRect::whole(width, height)];
    }
    let base =
        Affine::scale(f64::from(scale)) * Affine::translate((-now.viewport.x0, -now.viewport.y0));
    let mut parts = Vec::new();
    let mut add = |snap: &FrameSnapshot, item: &PaintItem| {
        parts.extend(item_box(snap, item, base, width, height));
    };
    let mut earlier: FxHashMap<NodeId, usize> = before
        .items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.list.is_some())
        .map(|(n, i)| (i.node, n))
        .collect();
    // The furthest place in the old order any item kept: one found before
    // it now was painted after it then.
    let mut furthest = 0;
    for item in now.items.iter().filter(|i| i.list.is_some()) {
        let Some(n) = earlier.remove(&item.node) else {
            add(now, item);
            continue;
        };
        let old = &before.items[n];
        let fell_behind = n < furthest;
        furthest = furthest.max(n);
        if fell_behind || !looks_the_same(before, old, now, item) {
            add(before, old);
            add(now, item);
        }
    }
    for n in earlier.into_values() {
        add(before, &before.items[n]);
    }
    merge(parts, width, height)
}

/// The pixels `item` may paint, within its clips.
fn item_box(
    snap: &FrameSnapshot,
    item: &PaintItem,
    base: Affine,
    width: u32,
    height: u32,
) -> Option<PixelRect> {
    let list = item.list.as_ref()?;
    let mut b = bbox_of(base * item.world, list.bounds());
    if let Some(clip) = item.clip {
        b = b.intersect(bbox_of(base, snap.clip(clip).bbox));
    }
    if b.width() <= 0.0 || b.height() <= 0.0 {
        return None;
    }
    PixelRect::covering(b.inflate(MARGIN, MARGIN), width, height)
}

fn looks_the_same(
    before: &FrameSnapshot,
    old: &PaintItem,
    now: &FrameSnapshot,
    new: &PaintItem,
) -> bool {
    let lists = match (&old.list, &new.list) {
        (Some(a), Some(b)) => a.id() == b.id(),
        (None, None) => true,
        _ => false,
    };
    lists
        && old.world == new.world
        && old.opacity == new.opacity
        && same_clips(before, old.clip, now, new.clip)
}

/// Clip ids are only good within their own snapshot: the chains are
/// compared entry by entry.
fn same_clips(
    before: &FrameSnapshot,
    mut a: Option<crate::compose::ClipId>,
    now: &FrameSnapshot,
    mut b: Option<crate::compose::ClipId>,
) -> bool {
    loop {
        match (a, b) {
            (None, None) => return true,
            (Some(x), Some(y)) => {
                let (x, y) = (before.clip(x), now.clip(y));
                if x.transform != y.transform || x.rect != y.rect || x.radii != y.radii {
                    return false;
                }
                (a, b) = (x.parent, y.parent);
            }
            _ => return false,
        }
    }
}

/// Joins rectangles that overlap or nearly touch, and all of them when they
/// are many.
pub fn merge(parts: Vec<PixelRect>, width: u32, height: u32) -> Vec<PixelRect> {
    let mut out: Vec<PixelRect> = Vec::with_capacity(parts.len());
    for mut r in parts.into_iter().filter(|r| !r.is_empty()) {
        // A join can bring the new rectangle near one passed before.
        while let Some(i) = out.iter().position(|o| o.near(&r, JOIN_GAP)) {
            r = r.union(&out.swap_remove(i));
        }
        out.push(r);
    }
    if out.len() > MAX_PARTS {
        let all = out.iter().skip(1).fold(out[0], |acc, r| acc.union(r));
        out = vec![all];
    }
    let whole = PixelRect::whole(width, height);
    if out.iter().map(PixelRect::area).sum::<u64>() * 4 >= whole.area() * 3 {
        // Most of the window: one pass over it all walks the scene once.
        return vec![whole];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: u32, y: u32, width: u32, height: u32) -> PixelRect {
        PixelRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn far_apart_parts_stay_apart_and_near_ones_join() {
        let parts = merge(vec![r(0, 0, 10, 10), r(100, 100, 10, 10)], 400, 400);
        assert_eq!(parts.len(), 2);
        let parts = merge(vec![r(0, 0, 10, 10), r(20, 0, 10, 10)], 400, 400);
        assert_eq!(parts, vec![r(0, 0, 30, 10)]);
    }

    #[test]
    fn a_join_that_reaches_an_earlier_part_takes_it_in() {
        let parts = merge(
            vec![r(0, 0, 10, 10), r(200, 0, 10, 10), r(10, 0, 200, 10)],
            400,
            400,
        );
        assert_eq!(parts, vec![r(0, 0, 210, 10)]);
    }

    #[test]
    fn most_of_the_window_is_all_of_it() {
        let parts = merge(vec![r(0, 0, 100, 80), r(0, 90, 100, 10)], 100, 100);
        assert_eq!(parts, vec![r(0, 0, 100, 100)]);
    }

    #[test]
    fn many_parts_are_one() {
        let parts: Vec<_> = (0..20).map(|i| r(i * 40, i * 40, 4, 4)).collect();
        assert_eq!(merge(parts, 2000, 2000), vec![r(0, 0, 764, 764)]);
    }
}
