//! A part of the last frame drawn to pixels, by the renderer every
//! machine has (R, `vello_cpu`), for the checks that ask what a user
//! sees rather than what was asked to be painted: whether a part stands
//! out from what is behind it, whether focus changed anything, what shows
//! through a surface.
//!
//! The frame is drawn over the audit's backdrop, so every pixel is
//! opaque and has one colour; at one physical pixel to a logical pixel,
//! which is what the guidelines measure in.

use std::collections::HashSet;

use ljos_design::color::{Srgb, delta_e_ok};
use ljos_design::limits::audit::{VISIBLE_CHANGE_MIN_PX, VISIBLE_DELTA_E};
use ljos_engine::ObjectId;
use ljos_render::kurbo::Rect;
use ljos_render::{
    ComposeStats, FrameSnapshot, FrameView, NodeId, PaintItem, RasterConfig, Rgba,
    VelloCpuReference,
};

use crate::Options;
use crate::acting::Act;
use crate::cx::{clipped_away, scene_rect, shown_items, shown_opacity, within};

/// Pixels of a part of the scene.
pub(crate) struct Picture {
    pub width: u32,
    pub height: u32,
    data: Vec<u8>,
}

impl Picture {
    /// Every pixel, row by row.
    pub(crate) fn pixels(&self) -> impl Iterator<Item = Srgb> + '_ {
        self.data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| Srgb::from_u8(p[0], p[1], p[2]))
    }

    /// The pixels of two pictures of the same region side by side.
    pub(crate) fn pairs<'a>(
        &'a self,
        other: &'a Picture,
    ) -> impl Iterator<Item = (Srgb, Srgb)> + 'a {
        self.pixels().zip(other.pixels())
    }
}

/// What the scene is drawn on where nothing is painted: as
/// `color-text-contrast` has it.
pub(crate) fn backdrop(engine: &ljos_engine::Engine, options: &Options) -> Srgb {
    match options.backdrop {
        Some([r, g, b]) => Srgb::from_u8(r, g, b),
        None => match ljos_quick::system_prefs(engine).color_scheme {
            ljos_a11y::ColorScheme::Dark => Srgb::BLACK,
            _ => Srgb::WHITE,
        },
    }
}

pub(crate) struct Painter {
    renderer: VelloCpuReference,
    /// The part of the frame drawn last. Its lists are emptied and filled
    /// again for each draw, not made anew: a check draws many parts of
    /// one scene.
    part: FrameSnapshot,
}

impl Painter {
    pub(crate) fn new(backdrop: Srgb) -> Self {
        let [r, g, b] = backdrop.to_u8();
        Painter {
            renderer: VelloCpuReference::new(RasterConfig {
                background: Rgba::rgb(r, g, b),
                ..RasterConfig::default()
            }),
            part: FrameSnapshot {
                frame: 0,
                viewport: Rect::ZERO,
                items: Vec::new(),
                clips: Vec::new(),
                stats: ComposeStats::default(),
            },
        }
    }

    /// `region` of the scene, cut to the window and rounded out to whole
    /// pixels, with only the paint `keep` keeps. `None` when no pixel of
    /// it is in the window.
    pub(crate) fn draw(
        &mut self,
        snapshot: &FrameSnapshot,
        region: Rect,
        keep: impl Fn(&PaintItem) -> bool,
    ) -> Option<Picture> {
        let r = region.intersect(snapshot.viewport);
        let r = Rect::new(r.x0.floor(), r.y0.floor(), r.x1.ceil(), r.y1.ceil());
        if r.width() < 1.0 || r.height() < 1.0 {
            return None;
        }
        let part = &mut self.part;
        part.frame = snapshot.frame;
        part.viewport = r;
        part.stats = snapshot.stats;
        part.items.clear();
        part.items
            .extend(snapshot.items.iter().filter(|i| keep(i)).cloned());
        part.clips.clear();
        part.clips.extend_from_slice(&snapshot.clips);
        let image = self.renderer.render(&FrameView::new(part, 1.0)).ok()?;
        Some(Picture {
            width: image.width,
            height: image.height,
            data: image.data,
        })
    }
}

/// How many pixels of two pictures of one region differ by a step anyone
/// sees; every one of them when the pictures are not of one size.
pub(crate) fn changed(a: &Picture, b: &Picture) -> usize {
    if a.width != b.width || a.height != b.height {
        return usize::MAX;
    }
    a.pairs(b)
        .filter(|(x, y)| delta_e_ok(*x, *y) >= VISIBLE_DELTA_E)
        .count()
}

/// Whether the window shows `item`: it is not clipped away, not drawn
/// with an opacity of 0, and what is drawn after it, a popup, its dim or
/// a sibling, does not cover it. An item the window does not show
/// changes no pixel when its state changes, whatever it draws, so a rule
/// that finds no change asks this before it blames the item.
///
/// Where its own drawing shows over what is behind it, only those pixels
/// count as covered; where it draws nothing that shows (a MouseArea or a
/// bare Item laid over a sibling's drawing), its box is what a user looks
/// at, and an item that draws nothing is shown, not hidden. Either way
/// only its box counts, not the margin a rule draws around it: a
/// neighbour drawn after it against its edge covers none of it.
pub(crate) fn shows(act: &Act, painter: &mut Painter, item: ObjectId) -> bool {
    let e = act.engine;
    let cx = act.cx();
    if clipped_away(&cx, item) || shown_opacity(e, item) <= 0.0 {
        return false;
    }
    let (Some(snapshot), Some(region)) = (act.scene.snapshot(), scene_rect(e, item)) else {
        return false;
    };
    let own = |p: &PaintItem| {
        act.scene
            .item_of(p.node)
            .is_some_and(|i| within(e, i, item))
    };
    // What is drawn after the item: the items after it and all it holds,
    // in the order the scene draws them.
    let order = shown_items(&cx);
    let Some(at) = order.iter().position(|i| *i == item) else {
        return false;
    };
    let later: HashSet<ObjectId> = order[at + 1..]
        .iter()
        .copied()
        .filter(|i| !within(e, *i, item))
        .collect();
    let start = match snapshot.items.iter().rposition(&own) {
        Some(last) => last + 1,
        None => snapshot
            .items
            .iter()
            .position(|p| {
                act.scene
                    .item_of(p.node)
                    .is_some_and(|i| later.contains(&i))
            })
            .unwrap_or(snapshot.items.len()),
    };
    // Against what is drawn behind it, not the bare backdrop: a white
    // field on a grey popup shows.
    let upto = FrameSnapshot {
        frame: snapshot.frame,
        viewport: snapshot.viewport,
        items: snapshot.items[..start].to_vec(),
        clips: snapshot.clips.clone(),
        stats: snapshot.stats,
    };
    let seen = |x: Srgb, y: Srgb| delta_e_ok(x, y) >= VISIBLE_DELTA_E;
    let (Some(behind), Some(with)) = (
        painter.draw(&upto, region, |p| !own(p)),
        painter.draw(&upto, region, |_| true),
    ) else {
        return false;
    };
    let mut mine: Vec<bool> = behind.pairs(&with).map(|(x, y)| seen(x, y)).collect();
    if mine.iter().filter(|m| **m).count() < VISIBLE_CHANGE_MIN_PX {
        mine.fill(true);
    }
    let over: HashSet<NodeId> = snapshot.items[start..]
        .iter()
        .filter(|p| !own(p))
        .map(|p| p.node)
        .collect();
    if over.is_empty() {
        return true;
    }
    let (Some(all), Some(under)) = (
        painter.draw(snapshot, region, |_| true),
        painter.draw(snapshot, region, |p| !over.contains(&p.node)),
    ) else {
        return false;
    };
    if all.width != under.width || all.height != under.height {
        return false;
    }
    let covered = all
        .pairs(&under)
        .zip(&mine)
        .filter(|((x, y), m)| **m && seen(*x, *y))
        .count();
    covered < VISIBLE_CHANGE_MIN_PX
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drawing again fills the lists of the last draw: no paint list is
    /// made per draw.
    #[test]
    fn a_painter_draws_into_the_lists_it_has() {
        let snapshot = FrameSnapshot {
            frame: 1,
            viewport: Rect::new(0.0, 0.0, 8.0, 8.0),
            items: Vec::new(),
            clips: Vec::new(),
            stats: ComposeStats::default(),
        };
        let mut painter = Painter::new(Srgb::WHITE);
        painter.part.items.reserve(4);
        painter.part.clips.reserve(4);
        let lists = (painter.part.items.as_ptr(), painter.part.clips.as_ptr());
        for _ in 0..3 {
            let picture = painter.draw(&snapshot, snapshot.viewport, |_| true);
            assert_eq!(picture.map(|p| (p.width, p.height)), Some((8, 8)));
        }
        assert_eq!(
            (painter.part.items.as_ptr(), painter.part.clips.as_ptr()),
            lists
        );
    }
}
