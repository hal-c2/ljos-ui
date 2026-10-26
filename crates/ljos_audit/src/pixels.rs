//! A part of the last frame drawn to pixels, by the renderer every
//! machine has (R, `vello_cpu`), for the checks that ask what a user
//! sees rather than what was asked to be painted: whether a part stands
//! out from what is behind it, whether focus changed anything, what shows
//! through a surface.
//!
//! The frame is drawn over the audit's backdrop, so every pixel is
//! opaque and has one colour; at one physical pixel to a logical pixel,
//! which is what the guidelines measure in.

use ljos_design::color::Srgb;
use ljos_render::kurbo::Rect;
use ljos_render::{FrameSnapshot, FrameView, PaintItem, RasterConfig, Rgba, VelloCpuReference};

use crate::Options;

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
}

impl Painter {
    pub(crate) fn new(backdrop: Srgb) -> Self {
        let [r, g, b] = backdrop.to_u8();
        Painter {
            renderer: VelloCpuReference::new(RasterConfig {
                background: Rgba::rgb(r, g, b),
                ..RasterConfig::default()
            }),
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
        let part = FrameSnapshot {
            frame: snapshot.frame,
            viewport: r,
            items: snapshot.items.iter().filter(|i| keep(i)).cloned().collect(),
            clips: snapshot.clips.clone(),
            stats: snapshot.stats,
        };
        let image = self.renderer.render(&FrameView::new(&part, 1.0)).ok()?;
        Some(Picture {
            width: image.width,
            height: image.height,
            data: image.data,
        })
    }
}
