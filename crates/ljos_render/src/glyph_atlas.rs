//! B0's glyph atlas: glyph coverage in one egui texture (ARCHITECTURE 7.5).
//!
//! egui cannot fill arbitrary outlines with anti-aliasing, so B0 draws text as
//! textured quads. Each glyph is filled once by vello_cpu, at its size in device
//! pixels and at one of [`BINS`] horizontal sub-pixel offsets, and packed on
//! shelves. The texture holds white with the coverage as alpha; the vertex
//! colour tints it, so one atlas serves every text colour.
//!
//! The atlas starts small and doubles when full. At the largest allowed size it
//! is emptied instead and the frame's glyphs are filled again. Colour glyphs
//! (emoji) have no outline and are not drawn yet.

use egui::{Color32, ColorImage, Context, TextureHandle, TextureId, TextureOptions};
use rustc_hash::FxHashMap;
use vello_cpu::color::palette::css::WHITE;
use vello_cpu::kurbo::{Affine, Stroke};
use vello_cpu::peniko::{Fill, ImageAlphaType};
use vello_cpu::{
    Level, Pixmap, RasterizerSettings, RenderContext, RenderMode, RenderSettings, Resources,
};

use crate::display_list::GlyphRun;
use crate::glyph::OutlineCache;

/// Horizontal sub-pixel positions a glyph is filled at. Four keeps a glyph
/// within an eighth of a pixel of where the shaper put it.
pub(crate) const BINS: i64 = 4;

/// Font sizes are told apart in steps of 1/`SIZE_STEPS` device pixels.
const SIZE_STEPS: f64 = 8.0;

/// Transparent texels around every glyph, so bilinear sampling of a rotated
/// quad never reads a neighbour.
const PAD: usize = 1;

/// Side the atlas starts at, and returns to on reset. A window of a few
/// labels needs a few dozen glyphs, which 256² holds; more doubles it.
const START_SIDE: usize = 256;

/// Side of the opaque block at the atlas origin that solid bars sample.
const WHITE_BLOCK: usize = 2;

/// Which filled glyph: font, glyph, size in device pixels, sub-pixel offset and
/// the synthetic style.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GlyphKey {
    font: u64,
    index: u32,
    glyph: u32,
    size: u32,
    bin: u8,
    embolden: bool,
    skew: u32,
    coords: Vec<i16>,
}

impl GlyphKey {
    /// Key for `glyph` of `run` at `size_px` device pixels. Returns the key and
    /// the size the glyph is actually filled at.
    pub(crate) fn new(run: &GlyphRun, glyph: u32, size_px: f64, bin: i64) -> (Self, f64) {
        let steps = (size_px * SIZE_STEPS).round().max(1.0);
        (
            Self {
                font: run.font.data.id(),
                index: run.font.index,
                glyph,
                size: steps as u32,
                bin: bin as u8,
                embolden: run.embolden,
                skew: run.skew.to_bits(),
                coords: run.normalized_coords.clone(),
            },
            steps / SIZE_STEPS,
        )
    }
}

/// Where a glyph's coverage sits in the atlas and relative to the glyph origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Slot {
    /// Left texel in the atlas.
    pub x: u32,
    /// Top texel in the atlas.
    pub y: u32,
    /// Width in texels; 0 for a glyph without ink.
    pub w: u32,
    /// Height in texels.
    pub h: u32,
    /// Device pixels from the glyph origin to the bitmap's left edge.
    pub left: i32,
    /// Device pixels from the baseline to the bitmap's top edge (negative is up).
    pub top: i32,
}

/// The atlas cannot take another glyph even at its largest size.
#[derive(Debug)]
pub(crate) struct Full;

struct Shelf {
    y: usize,
    height: usize,
    cursor: usize,
}

/// See the module docs.
pub(crate) struct GlyphAtlas {
    side: usize,
    alpha: Vec<u8>,
    shelves: Vec<Shelf>,
    slots: FxHashMap<GlyphKey, Slot>,
    /// Bumped whenever texture coordinates handed out earlier stop being valid.
    generation: u64,
    texture: Option<TextureHandle>,
    /// Texels `[x0, y0, x1, y1)` changed since the last upload, one box a
    /// shelf: only they travel, so a painter that keeps what it drew
    /// knows the glyphs already shown were not touched.
    dirty: Vec<[usize; 4]>,
    outlines: OutlineCache,
    scratch: Option<RenderContext>,
    resources: Resources,
    rasterised: u64,
}

impl std::fmt::Debug for GlyphAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlyphAtlas")
            .field("side", &self.side)
            .field("glyphs", &self.slots.len())
            .finish_non_exhaustive()
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        let mut atlas = Self {
            side: START_SIDE,
            alpha: Vec::new(),
            shelves: Vec::new(),
            slots: FxHashMap::default(),
            generation: 0,
            texture: None,
            dirty: Vec::new(),
            outlines: OutlineCache::default(),
            scratch: None,
            resources: Resources::new(),
            rasterised: 0,
        };
        atlas.clear();
        atlas
    }
}

impl GlyphAtlas {
    /// Largest atlas side for this context: what the GPU takes, and no more
    /// than the architecture's budget (2048 on phones, 4096 on desktop).
    pub(crate) fn max_side(ctx: &Context) -> usize {
        let budget = if cfg!(any(target_os = "android", target_os = "ios")) {
            2048
        } else {
            4096
        };
        crate::b0::max_texture_side(ctx).clamp(START_SIDE, budget)
    }

    /// Texture coordinates from before a change of generation are stale.
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Atlas side in texels.
    pub(crate) fn side(&self) -> usize {
        self.side
    }

    /// Coverage, one byte per texel, row-major.
    pub(crate) fn alpha(&self) -> &[u8] {
        &self.alpha
    }

    /// Glyphs held, counting those without ink.
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    /// Glyphs filled since the backend was created.
    pub(crate) fn rasterised(&self) -> u64 {
        self.rasterised
    }

    /// The texture, once [`Self::upload`] has run and until the atlas grows.
    pub(crate) fn texture_id(&self) -> Option<TextureId> {
        self.texture.as_ref().map(TextureHandle::id)
    }

    /// Normalised coordinates of a fully covered texel, for solid bars.
    pub(crate) fn white_uv(&self) -> egui::Pos2 {
        let c = (WHITE_BLOCK as f32 / 2.0) / self.side as f32;
        egui::Pos2::new(c, c)
    }

    /// Forget every glyph but keep the texture.
    pub(crate) fn clear(&mut self) {
        self.slots.clear();
        self.shelves.clear();
        self.alpha.clear();
        self.alpha.resize(self.side * self.side, 0);
        self.generation += 1;
        self.dirty = vec![[0, 0, self.side, self.side]];
        if let Some((x, y)) = self.allocate(WHITE_BLOCK, WHITE_BLOCK) {
            for row in y..y + WHITE_BLOCK {
                self.alpha[row * self.side + x..row * self.side + x + WHITE_BLOCK].fill(255);
            }
        }
    }

    /// Give the memory back: the texture, the coverage and the outlines.
    pub(crate) fn reset(&mut self) {
        self.texture = None;
        self.side = START_SIDE;
        self.outlines.clear();
        self.scratch = None;
        self.resources = Resources::new();
        self.clear();
    }

    fn allocate(&mut self, w: usize, h: usize) -> Option<(usize, usize)> {
        let (pw, ph) = (w + PAD, h + PAD);
        if pw > self.side {
            return None;
        }
        // The lowest shelf that fits without wasting more than half its height.
        let fit = self
            .shelves
            .iter_mut()
            .filter(|s| s.height >= ph && s.height <= ph * 2 && s.cursor + pw <= self.side)
            .min_by_key(|s| s.height);
        if let Some(s) = fit {
            let at = (s.cursor, s.y);
            s.cursor += pw;
            return Some(at);
        }
        let y = self.shelves.last().map_or(0, |s| s.y + s.height);
        if y + ph > self.side {
            return None;
        }
        self.shelves.push(Shelf {
            y,
            height: ph,
            cursor: pw,
        });
        Some((0, y))
    }

    fn grow(&mut self, max_side: usize) -> bool {
        let side = self.side * 2;
        if side > max_side {
            return false;
        }
        let mut alpha = vec![0u8; side * side];
        for row in 0..self.side {
            alpha[row * side..row * side + self.side]
                .copy_from_slice(&self.alpha[row * self.side..(row + 1) * self.side]);
        }
        self.alpha = alpha;
        self.side = side;
        // Normalised texture coordinates depend on the side.
        self.generation += 1;
        self.dirty = vec![[0, 0, side, side]];
        // The texture of the old side is replaced, and freed, by the next
        // upload. Without it no mesh is made, at the new generation, on a
        // texture that is going away: such a mesh would count as finished and
        // draw nothing until its node changed.
        self.texture = None;
        true
    }

    /// The slot of a glyph that is already in the atlas.
    pub(crate) fn slot(&self, key: &GlyphKey) -> Option<Slot> {
        self.slots.get(key).copied()
    }

    /// Make sure the glyph is in the atlas, filling it at `size_px` if needed.
    pub(crate) fn ensure(
        &mut self,
        key: &GlyphKey,
        run: &GlyphRun,
        size_px: f64,
        max_side: usize,
    ) -> Result<Slot, Full> {
        if let Some(slot) = self.slots.get(key) {
            return Ok(*slot);
        }
        let Some(outline) = self.outlines.get(run, key.glyph) else {
            self.slots.insert(key.clone(), Slot::default());
            return Ok(Slot::default());
        };
        if outline.path.is_empty() {
            self.slots.insert(key.clone(), Slot::default());
            return Ok(Slot::default());
        }
        let shift = f64::from(key.bin) / BINS as f64;
        let ink = outline.ink_bounds(run, size_px);
        let left = (ink.x0 + shift).floor();
        let top = ink.y0.floor();
        let w = ((ink.x1 + shift).ceil() - left).max(1.0) as usize;
        let h = (ink.y1.ceil() - top).max(1.0) as usize;
        if w + PAD > max_side / 2 || h + PAD > max_side / 2 || w > 4096 || h > 4096 {
            tracing::debug!("B0: a {w}x{h} glyph is too large for the atlas; not drawn");
            self.slots.insert(key.clone(), Slot::default());
            return Ok(Slot::default());
        }
        let (x, y) = loop {
            if let Some(at) = self.allocate(w, h) {
                break at;
            }
            if !self.grow(max_side) {
                return Err(Full);
            }
        };

        let mut ctx = match self.scratch.take() {
            Some(mut c) => {
                c.reset_and_resize(w as u16, h as u16);
                c
            }
            None => RenderContext::new_with(
                w as u16,
                h as u16,
                RenderSettings {
                    level: Level::baseline(),
                    num_threads: 0,
                },
            ),
        };
        ctx.set_transform(Affine::translate((shift - left, -top)) * outline.to_size(run, size_px));
        ctx.set_paint(WHITE);
        ctx.set_fill_rule(Fill::NonZero);
        ctx.fill_path(&outline.path);
        if let Some(width) = outline.embolden_width(run) {
            ctx.set_stroke(Stroke::new(width));
            ctx.stroke_path(&outline.path);
        }
        ctx.flush();
        let mut pixmap = Pixmap::new(w as u16, h as u16);
        ctx.render_with(
            &mut pixmap,
            &mut self.resources,
            RasterizerSettings {
                render_mode: RenderMode::OptimizeSpeed,
                ..Default::default()
            },
        );
        self.scratch = Some(ctx);
        let rgba = pixmap.take_rgba8(ImageAlphaType::Alpha);
        for row in 0..h {
            let dst = (y + row) * self.side + x;
            for col in 0..w {
                self.alpha[dst + col] = rgba[(row * w + col) * 4 + 3];
            }
        }
        // A shelf fills from left to right, so what a frame adds to one
        // is one box.
        let sent = |d: &[usize; 4]| d[0] <= x && d[1] <= y && x + w <= d[2] && y + h <= d[3];
        if !self.dirty.iter().any(sent) {
            match self.dirty.iter_mut().find(|d| d[1] == y) {
                Some(d) => *d = [d[0].min(x), y, d[2].max(x + w), d[3].max(y + h)],
                None => self.dirty.push([x, y, x + w, y + h]),
            }
        }
        self.rasterised += 1;

        let slot = Slot {
            x: x as u32,
            y: y as u32,
            w: w as u32,
            h: h as u32,
            left: left as i32,
            top: top as i32,
        };
        self.slots.insert(key.clone(), slot);
        Ok(slot)
    }

    fn image(&self, [x0, y0, x1, y1]: [usize; 4]) -> ColorImage {
        let pixels = (y0..y1)
            .flat_map(|y| &self.alpha[y * self.side + x0..y * self.side + x1])
            .map(|a| Color32::from_rgba_premultiplied(*a, *a, *a, *a))
            .collect();
        ColorImage::new([x1 - x0, y1 - y0], pixels)
    }

    /// Send what changed to egui. Only the glyphs filled since the last
    /// call travel, unless the atlas grew.
    pub(crate) fn upload(&mut self, ctx: &Context) {
        // Linear, so glyphs on a rotated or scaled item are resampled smoothly;
        // upright text is drawn texel on pixel and comes out unchanged.
        let options = TextureOptions::LINEAR;
        let stale = self
            .texture
            .as_ref()
            .is_none_or(|t| t.size() != [self.side, self.side]);
        if stale {
            let image = self.image([0, 0, self.side, self.side]);
            self.texture = Some(crate::b0::upload(ctx, "ljos_glyph_atlas", image, options));
            self.dirty.clear();
        } else {
            for d in std::mem::take(&mut self.dirty) {
                let image = self.image(d);
                if let Some(t) = &mut self.texture {
                    t.set_partial([d[0], d[1]], image, options);
                }
            }
        }
    }
}
