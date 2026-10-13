//! The picture of a QML `Canvas`: Context2D drawing on vello_cpu.
//!
//! A canvas keeps what was drawn on it until it is cleared, so drawing
//! adds to the pixels already there. Commands are collected in a vello_cpu
//! scene whose first command lays down those pixels; the scene is turned
//! into pixels only when someone needs them (the picture, `getImageData`),
//! so a paint handler that draws a hundred shapes rasterises once.
//!
//! Paths arrive in canvas pixels, already through the transform that was
//! current when each point was added (as the HTML canvas does it, and as
//! Qt's measured). Strokes are drawn in the space the transform makes, so
//! a scaled line is a thicker line.
//!
//! Composition is per drawing: a mode that removes what is underneath does
//! so only under the shape drawn, as Qt measured it (`copy` leaves the rest
//! of the canvas alone).

use std::collections::HashMap;
use std::sync::Arc;

use vello_cpu::color::AlphaColor;
use vello_cpu::kurbo::{Affine, BezPath, Point, Rect, Shape, Stroke, Vec2};
use vello_cpu::peniko::{
    BlendMode, Compose, Extend, Fill, ImageAlphaType, ImageQuality, ImageSampler, Mix,
};
use vello_cpu::{
    Image, ImageSource, Level, PaintType, PixelMetadata, Pixmap, RasterizerSettings, RenderContext,
    RenderMode, RenderSettings, Resources, TargetInit,
};

use crate::Rgba;
use crate::display_list::{Brush, FillRule, GlyphRun};
use crate::glyph::OutlineCache;
use crate::image::{ImageHandle, WeakImage};
use crate::shape::{paint_of, stroke_is_visible, stroke_outline};

/// How finely curves are cut into lines, in canvas pixels.
const TOLERANCE: f64 = 0.05;

/// What a drawing is filled with.
#[derive(Clone, Debug)]
pub enum CanvasPaint {
    /// A colour or a gradient, in the space of the current transform.
    Brush(Brush),
    /// An image repeated along the axes asked for, from the origin of the
    /// current transform.
    Pattern {
        /// The pixels.
        image: ImageHandle,
        /// Repeat sideways and downwards.
        repeat: [bool; 2],
    },
    /// A hatching (Qt's `createPattern(colour, brushStyle)`): a small cell
    /// repeated in canvas pixels, never scaled or smoothed, moved only by
    /// the translation of the current transform (as Qt measured).
    Hatch {
        /// The cell.
        image: ImageHandle,
    },
}

/// The shadow drawn under each drawing: the drawing's coverage, moved,
/// blurred and painted in one colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasShadow {
    /// The colour, premultiplied.
    pub color: Rgba,
    /// How far the shadow lies from the drawing, in canvas pixels.
    pub offset: Vec2,
    /// The standard deviation of the Gaussian blur, in canvas pixels; zero
    /// for a sharp shadow.
    pub sigma: f64,
}

/// The state a drawing is made in.
#[derive(Clone, Copy, Debug)]
pub struct DrawState<'a> {
    /// User space to canvas pixels.
    pub transform: Affine,
    /// `globalAlpha`.
    pub alpha: f32,
    /// `globalCompositeOperation`.
    pub blend: BlendMode,
    /// The clips, in canvas pixels, each within the one before; none
    /// clips nothing.
    pub clips: &'a [BezPath],
    /// The shadow, when one shows.
    pub shadow: Option<CanvasShadow>,
}

impl Default for DrawState<'_> {
    fn default() -> Self {
        Self {
            transform: Affine::IDENTITY,
            alpha: 1.0,
            blend: BlendMode::default(),
            clips: &[],
            shadow: None,
        }
    }
}

/// The pixels of one canvas and the drawing not yet rasterised.
pub struct CanvasPainter {
    width: u16,
    height: u16,
    /// What was drawn up to the start of `scene`, premultiplied; `None`
    /// is all transparent.
    pixels: Option<ImageHandle>,
    /// Drawing since `pixels`; `None` when there is none.
    scene: Option<RenderContext>,
    resources: Resources,
    outlines: OutlineCache,
    images: HashMap<u64, (WeakImage, Arc<Pixmap>)>,
}

impl std::fmt::Debug for CanvasPainter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CanvasPainter")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("drawn", &self.pixels.is_some())
            .field("pending", &self.scene.is_some())
            .finish()
    }
}

fn settings() -> RenderSettings {
    RenderSettings {
        level: Level::baseline(),
        num_threads: 0,
    }
}

fn pixmap_of(image: &ImageHandle) -> Option<Arc<Pixmap>> {
    let (w, h) = (
        u16::try_from(image.width()).ok()?,
        u16::try_from(image.height()).ok()?,
    );
    let meta = PixelMetadata::new(ImageAlphaType::AlphaPremultiplied, !image.is_opaque());
    Some(Arc::new(Pixmap::from_parts(
        image.data().to_vec(),
        w,
        h,
        meta,
    )))
}

/// Blurs `values` (`width` by `height`, row by row) with a Gaussian of
/// standard deviation `sigma`, cut off `reach` samples from its centre:
/// once along the rows and once along the columns. Only the part around
/// what is not zero is worked on.
fn blur(values: &mut [f32], width: usize, height: usize, sigma: f64, reach: usize) {
    if sigma <= 0.0 || reach == 0 || width == 0 || height == 0 {
        return;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (width, height, 0, 0);
    for y in 0..height {
        for x in 0..width {
            if values[y * width + x] > 0.0 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    if x0 >= x1 {
        return;
    }
    let (x0, y0) = (x0.saturating_sub(reach), y0.saturating_sub(reach));
    let (x1, y1) = ((x1 + reach).min(width), (y1 + reach).min(height));
    let mut kernel: Vec<f32> = (0..=2 * reach)
        .map(|i| {
            let d = i as f64 - reach as f64;
            (-d * d / (2.0 * sigma * sigma)).exp() as f32
        })
        .collect();
    let sum: f32 = kernel.iter().sum();
    kernel.iter_mut().for_each(|k| *k /= sum);
    let r = reach as isize;
    let mut line = Vec::new();
    for y in y0..y1 {
        line.clear();
        line.extend_from_slice(&values[y * width + x0..y * width + x1]);
        for x in x0..x1 {
            let mut acc = 0.0;
            for (i, k) in kernel.iter().enumerate() {
                let sx = (x - x0) as isize + i as isize - r;
                if sx >= 0 && (sx as usize) < line.len() {
                    acc += k * line[sx as usize];
                }
            }
            values[y * width + x] = acc;
        }
    }
    for x in x0..x1 {
        line.clear();
        line.extend((y0..y1).map(|y| values[y * width + x]));
        for y in y0..y1 {
            let mut acc = 0.0;
            for (i, k) in kernel.iter().enumerate() {
                let sy = (y - y0) as isize + i as isize - r;
                if sy >= 0 && (sy as usize) < line.len() {
                    acc += k * line[sy as usize];
                }
            }
            values[y * width + x] = acc;
        }
    }
}

fn is_plain(blend: BlendMode) -> bool {
    blend.mix == Mix::Normal && blend.compose == Compose::SrcOver
}

impl CanvasPainter {
    /// A transparent canvas. A side past 65535 pixels is cut to that.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: width.min(u32::from(u16::MAX)) as u16,
            height: height.min(u32::from(u16::MAX)) as u16,
            pixels: None,
            scene: None,
            resources: Resources::new(),
            outlines: OutlineCache::default(),
            images: HashMap::new(),
        }
    }

    /// Width and height in pixels.
    pub fn size(&self) -> (u32, u32) {
        (u32::from(self.width), u32::from(self.height))
    }

    /// Makes the canvas `width` by `height`, keeping the pixels that still
    /// fit at the top left (as Qt measured: a grown canvas keeps its
    /// drawing).
    pub fn resize(&mut self, width: u32, height: u32) {
        let (w, h) = (
            width.min(u32::from(u16::MAX)) as u16,
            height.min(u32::from(u16::MAX)) as u16,
        );
        if (w, h) == (self.width, self.height) {
            return;
        }
        self.flush();
        let old = self.pixels.take();
        let (ow, oh) = (usize::from(self.width), usize::from(self.height));
        self.width = w;
        self.height = h;
        let Some(old) = old else { return };
        let (nw, nh) = (usize::from(w), usize::from(h));
        if nw == 0 || nh == 0 {
            return;
        }
        let mut px = vec![0u8; nw * nh * 4];
        let src = old.data();
        let (cw, ch) = (ow.min(nw), oh.min(nh));
        for row in 0..ch {
            let s = row * ow * 4;
            let d = row * nw * 4;
            px[d..d + cw * 4].copy_from_slice(&src[s..s + cw * 4]);
        }
        if px.iter().any(|b| *b != 0) {
            self.pixels = ImageHandle::from_premultiplied(u32::from(w), u32::from(h), px).ok();
        }
    }

    /// Makes every pixel transparent.
    pub fn clear(&mut self) {
        self.pixels = None;
        self.scene = None;
    }

    /// Drops what can be made again: copies of images and glyph outlines.
    /// The picture stays.
    pub fn trim(&mut self) {
        self.images.clear();
        self.outlines.clear();
    }

    fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, f64::from(self.width), f64::from(self.height))
    }

    /// The scene to draw into, started with the pixels drawn so far.
    fn scene(&mut self) -> Option<&mut RenderContext> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        if self.scene.is_none() {
            let mut ctx = RenderContext::new_with(self.width, self.height, settings());
            if let Some(base) = self.pixels.as_ref().and_then(pixmap_of) {
                ctx.set_paint(Image {
                    image: ImageSource::Pixmap(base),
                    sampler: ImageSampler {
                        x_extend: Extend::Pad,
                        y_extend: Extend::Pad,
                        quality: ImageQuality::Low,
                        alpha: 1.0,
                    },
                });
                ctx.fill_rect(&Rect::new(
                    0.0,
                    0.0,
                    f64::from(self.width),
                    f64::from(self.height),
                ));
            }
            self.scene = Some(ctx);
        }
        self.scene.as_mut()
    }

    /// Rasterises what was drawn since the last time.
    fn flush(&mut self) {
        let Some(mut ctx) = self.scene.take() else {
            return;
        };
        ctx.flush();
        let mut pixmap = Pixmap::new(self.width, self.height);
        ctx.render_with(
            &mut pixmap,
            &mut self.resources,
            RasterizerSettings {
                render_mode: RenderMode::OptimizeSpeed,
                target_init: TargetInit::Clear(AlphaColor::TRANSPARENT),
                ..Default::default()
            },
        );
        let data = pixmap.data_as_u8_slice().to_vec();
        self.pixels = if data.iter().all(|b| *b == 0) {
            None
        } else {
            ImageHandle::from_premultiplied(u32::from(self.width), u32::from(self.height), data)
                .ok()
        };
    }

    /// The picture as it is now; `None` while nothing is drawn.
    pub fn picture(&mut self) -> Option<ImageHandle> {
        self.flush();
        self.pixels.clone()
    }

    /// Runs `draw` with the state's clip and composition set up.
    fn with_state(&mut self, st: &DrawState<'_>, draw: impl FnOnce(&mut RenderContext)) {
        let Some(ctx) = self.scene() else { return };
        ctx.set_transform(Affine::IDENTITY);
        for clip in st.clips {
            ctx.push_clip_path(clip);
        }
        let plain = is_plain(st.blend);
        if !plain {
            ctx.set_blend_mode(st.blend);
        }
        draw(ctx);
        if !plain {
            ctx.set_blend_mode(BlendMode::default());
        }
        for _ in st.clips {
            ctx.pop_clip();
        }
        ctx.set_transform(Affine::IDENTITY);
        ctx.reset_paint_transform();
        ctx.set_fill_rule(Fill::NonZero);
    }

    /// Draws the shadow of a drawing, when the state has one: `draw` draws
    /// the drawing into a scene of its own with `shift` before its
    /// transform; its coverage is blurred, painted in the shadow's colour
    /// and laid down with the state's clip and composition. Called before
    /// the drawing itself, which goes on top.
    fn shadow(&mut self, st: &DrawState<'_>, draw: impl FnOnce(&mut RenderContext, Affine)) {
        let Some(sh) = st.shadow else { return };
        if self.width == 0 || self.height == 0 || sh.color.a <= 0.0 {
            return;
        }
        if sh.sigma <= 0.0 && sh.offset == Vec2::ZERO {
            return;
        }
        let (w, h) = (usize::from(self.width), usize::from(self.height));
        // Coverage just outside the canvas still blurs into it.
        let reach = (sh.sigma * 3.0).ceil().max(0.0) as usize;
        let max = usize::from(u16::MAX);
        let margin = reach.min((max - w.max(h)) / 2);
        let (mw, mh) = (w + 2 * margin, h + 2 * margin);
        let mut ctx = RenderContext::new_with(mw as u16, mh as u16, settings());
        let m = margin as f64;
        draw(
            &mut ctx,
            Affine::translate((sh.offset.x + m, sh.offset.y + m)),
        );
        ctx.flush();
        let mut pixmap = Pixmap::new(mw as u16, mh as u16);
        ctx.render_with(
            &mut pixmap,
            &mut self.resources,
            RasterizerSettings {
                render_mode: RenderMode::OptimizeSpeed,
                target_init: TargetInit::Clear(AlphaColor::TRANSPARENT),
                ..Default::default()
            },
        );
        let mut alpha: Vec<f32> = pixmap
            .data_as_u8_slice()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| f32::from(p[3]) / 255.0)
            .collect();
        blur(&mut alpha, mw, mh, sh.sigma, reach);
        let c = sh.color;
        let mut px = vec![0u8; w * h * 4];
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        for y in 0..h {
            for x in 0..w {
                let a = alpha[(y + margin) * mw + x + margin];
                if a <= 0.0 {
                    continue;
                }
                let d = (y * w + x) * 4;
                let to = |v: f32| (v * a * 255.0).round().clamp(0.0, 255.0) as u8;
                px[d] = to(c.r);
                px[d + 1] = to(c.g);
                px[d + 2] = to(c.b);
                px[d + 3] = to(c.a);
                if px[d + 3] > 0 {
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
                }
            }
        }
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let meta = PixelMetadata::new(ImageAlphaType::AlphaPremultiplied, true);
        let image = Arc::new(Pixmap::from_parts(px, w as u16, h as u16, meta));
        // Only where the shadow is, so that a composition that removes what
        // is underneath does not reach the rest of the canvas.
        let area = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
        self.with_state(st, |ctx| {
            ctx.set_paint(Image {
                image: ImageSource::Pixmap(image),
                sampler: ImageSampler {
                    x_extend: Extend::Pad,
                    y_extend: Extend::Pad,
                    quality: ImageQuality::Low,
                    alpha: 1.0,
                },
            });
            ctx.set_paint_transform(Affine::IDENTITY);
            ctx.fill_rect(&area);
        });
    }

    fn pattern(&mut self, image: &ImageHandle) -> Option<Arc<Pixmap>> {
        self.images.retain(|_, (owner, _)| owner.is_alive());
        if let Some((_, p)) = self.images.get(&image.id()) {
            return Some(Arc::clone(p));
        }
        let p = pixmap_of(image)?;
        self.images
            .insert(image.id(), (image.downgrade(), Arc::clone(&p)));
        Some(p)
    }

    /// What `paint` paints with at `alpha`, and the map from the paint's
    /// space to the geometry's (`paint_space` before the paint's own).
    /// `geometry` is the transform the geometry is drawn with, so that
    /// `geometry * paint_space` is the drawing's whole transform. `None`
    /// paints nothing.
    fn set_paint(
        &mut self,
        paint: &CanvasPaint,
        alpha: f32,
        paint_space: Affine,
        geometry: Affine,
    ) -> Option<(PaintType, Affine)> {
        match paint {
            CanvasPaint::Hatch { image } => {
                let pixmap = self.pattern(image)?;
                let p = Image {
                    image: ImageSource::Pixmap(pixmap),
                    sampler: ImageSampler {
                        x_extend: Extend::Repeat,
                        y_extend: Extend::Repeat,
                        quality: ImageQuality::Low,
                        alpha: alpha.clamp(0.0, 1.0),
                    },
                };
                if geometry.determinant().abs() < 1e-12 {
                    return None;
                }
                let moved = Affine::translate((geometry * paint_space).translation());
                Some((p.into(), geometry.inverse() * moved))
            }
            CanvasPaint::Brush(brush) => {
                let (p, own) = paint_of(&brush.with_opacity(alpha))?;
                Some((p, paint_space * own.unwrap_or(Affine::IDENTITY)))
            }
            CanvasPaint::Pattern { image, repeat } => {
                let pixmap = self.pattern(image)?;
                let extend = |r: bool| if r { Extend::Repeat } else { Extend::Pad };
                let p = Image {
                    image: ImageSource::Pixmap(pixmap),
                    sampler: ImageSampler {
                        x_extend: extend(repeat[0]),
                        y_extend: extend(repeat[1]),
                        quality: ImageQuality::Medium,
                        alpha: alpha.clamp(0.0, 1.0),
                    },
                };
                Some((p.into(), paint_space))
            }
        }
    }

    /// Fills `path` (in canvas pixels).
    pub fn fill(
        &mut self,
        path: &BezPath,
        rule: FillRule,
        paint: &CanvasPaint,
        st: &DrawState<'_>,
    ) {
        if st.alpha <= 0.0 || path.elements().is_empty() {
            return;
        }
        let Some((p, t)) = self.set_paint(paint, st.alpha, st.transform, Affine::IDENTITY) else {
            return;
        };
        let fill = match rule {
            FillRule::NonZero => Fill::NonZero,
            FillRule::EvenOdd => Fill::EvenOdd,
        };
        self.shadow(st, |ctx, shift| {
            ctx.set_paint(p.clone());
            ctx.set_paint_transform(t);
            ctx.set_transform(shift);
            ctx.set_fill_rule(fill);
            ctx.fill_path(path);
        });
        self.with_state(st, |ctx| {
            ctx.set_paint(p);
            ctx.set_paint_transform(t);
            ctx.set_transform(Affine::IDENTITY);
            ctx.set_fill_rule(match rule {
                FillRule::NonZero => Fill::NonZero,
                FillRule::EvenOdd => Fill::EvenOdd,
            });
            ctx.fill_path(path);
        });
    }

    /// Strokes `path` (in canvas pixels) with `stroke` in the space of the
    /// state's transform.
    pub fn stroke(
        &mut self,
        path: &BezPath,
        stroke: &Stroke,
        paint: &CanvasPaint,
        st: &DrawState<'_>,
    ) {
        if st.alpha <= 0.0 || path.elements().is_empty() || !stroke_is_visible(stroke) {
            return;
        }
        let t = st.transform;
        if t.determinant().abs() < 1e-12 {
            return;
        }
        let mut user = path.clone();
        user.apply_affine(t.inverse());
        let scale = t.determinant().abs().sqrt().max(1e-6);
        let outline = stroke_outline(&user, stroke, TOLERANCE / scale);
        let Some((p, pt)) = self.set_paint(paint, st.alpha, Affine::IDENTITY, t) else {
            return;
        };
        self.shadow(st, |ctx, shift| {
            ctx.set_paint(p.clone());
            ctx.set_paint_transform(pt);
            ctx.set_transform(shift * t);
            ctx.set_fill_rule(Fill::NonZero);
            ctx.fill_path(&outline);
        });
        self.with_state(st, |ctx| {
            ctx.set_paint(p);
            ctx.set_paint_transform(pt);
            ctx.set_transform(t);
            ctx.set_fill_rule(Fill::NonZero);
            ctx.fill_path(&outline);
        });
    }

    /// Makes `rect` (in the state's user space) transparent, within the clip.
    pub fn clear_rect(&mut self, rect: Rect, st: &DrawState<'_>) {
        let mut area = rect.to_path(0.1);
        area.apply_affine(st.transform);
        // Clearing casts no shadow (measured).
        let clear = DrawState {
            alpha: 1.0,
            blend: BlendMode::new(Mix::Normal, Compose::Clear),
            shadow: None,
            ..*st
        };
        self.fill(
            &area,
            FillRule::NonZero,
            &CanvasPaint::Brush(Brush::Solid(crate::Rgba::BLACK)),
            &clear,
        );
    }

    /// Draws the part `src` of `image` (in image pixels) into `dst` (in
    /// user space).
    pub fn draw_image(&mut self, image: &ImageHandle, src: Rect, dst: Rect, st: &DrawState<'_>) {
        if st.alpha <= 0.0 || src.width() <= 0.0 || src.height() <= 0.0 || dst.area() == 0.0 {
            return;
        }
        let Some(pixmap) = self.pattern(image) else {
            return;
        };
        let place = Affine::translate((dst.x0, dst.y0))
            * Affine::scale_non_uniform(dst.width() / src.width(), dst.height() / src.height())
            * Affine::translate((-src.x0, -src.y0));
        let t = st.transform;
        let paint = Image {
            image: ImageSource::Pixmap(pixmap),
            sampler: ImageSampler {
                x_extend: Extend::Pad,
                y_extend: Extend::Pad,
                quality: ImageQuality::Medium,
                alpha: st.alpha.clamp(0.0, 1.0),
            },
        };
        self.shadow(st, |ctx, shift| {
            ctx.set_paint(paint.clone());
            ctx.set_transform(shift * t);
            ctx.set_paint_transform(place);
            ctx.fill_rect(&dst);
        });
        self.with_state(st, |ctx| {
            ctx.set_paint(paint);
            ctx.set_transform(t);
            ctx.set_paint_transform(place);
            ctx.fill_rect(&dst);
        });
    }

    /// The outlines of `run` with its origin at `origin`, in the run's
    /// space: what `fillText` fills and `strokeText` strokes.
    pub fn glyph_path(&mut self, run: &GlyphRun, origin: Point) -> BezPath {
        let mut out = BezPath::new();
        let size = f64::from(run.font_size);
        for g in &run.glyphs {
            let Some(outline) = self.outlines.get(run, g.id) else {
                continue;
            };
            if outline.path.is_empty() {
                continue;
            }
            let at = Affine::translate((origin.x + f64::from(g.x), origin.y + f64::from(g.y)))
                * outline.to_size(run, size);
            for el in outline.path.elements() {
                out.push(at * *el);
            }
        }
        out
    }

    /// The pixels of the rectangle at `x`, `y`, straight RGBA, row by row;
    /// transparent outside the canvas.
    pub fn image_data(&mut self, x: i64, y: i64, width: u32, height: u32) -> Vec<u8> {
        self.flush();
        let mut out = vec![0u8; width as usize * height as usize * 4];
        let Some(pixels) = &self.pixels else {
            return out;
        };
        let (cw, ch) = (i64::from(self.width), i64::from(self.height));
        let data = pixels.data();
        for row in 0..i64::from(height) {
            let sy = y + row;
            if !(0..ch).contains(&sy) {
                continue;
            }
            for col in 0..i64::from(width) {
                let sx = x + col;
                if !(0..cw).contains(&sx) {
                    continue;
                }
                let s = ((sy * cw + sx) * 4) as usize;
                let d = ((row * i64::from(width) + col) * 4) as usize;
                let a = data[s + 3];
                out[d + 3] = a;
                if a > 0 {
                    for c in 0..3 {
                        let v = (u32::from(data[s + c]) * 255 + u32::from(a) / 2) / u32::from(a);
                        out[d + c] = v.min(255) as u8;
                    }
                }
            }
        }
        out
    }

    /// Writes straight RGBA `data`, `width` by `height`, with its top left
    /// at `x`, `y`, replacing what is there (no composition). Only the part
    /// `dirty` (in the data's pixels) is written.
    pub fn put_image_data(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        x: i64,
        y: i64,
        dirty: Rect,
    ) {
        if data.len() < width as usize * height as usize * 4 {
            return;
        }
        self.flush();
        let (cw, ch) = (i64::from(self.width), i64::from(self.height));
        if cw == 0 || ch == 0 {
            return;
        }
        let mut px = match &self.pixels {
            Some(p) => p.data().to_vec(),
            None => vec![0u8; (cw * ch * 4) as usize],
        };
        let part = dirty.intersect(Rect::new(0.0, 0.0, f64::from(width), f64::from(height)));
        let (c0, c1) = (part.x0.max(0.0) as i64, part.x1.max(0.0) as i64);
        let (r0, r1) = (part.y0.max(0.0) as i64, part.y1.max(0.0) as i64);
        for row in r0..r1 {
            let dy = y + row;
            if !(0..ch).contains(&dy) {
                continue;
            }
            for col in c0..c1 {
                let dx = x + col;
                if !(0..cw).contains(&dx) {
                    continue;
                }
                let s = ((row * i64::from(width) + col) * 4) as usize;
                let d = ((dy * cw + dx) * 4) as usize;
                let a = u32::from(data[s + 3]);
                for c in 0..3 {
                    px[d + c] = ((u32::from(data[s + c]) * a + 127) / 255) as u8;
                }
                px[d + 3] = a as u8;
            }
        }
        self.pixels = ImageHandle::from_premultiplied(cw as u32, ch as u32, px).ok();
    }

    /// The canvas's own rectangle, in pixels.
    pub fn rect(&self) -> Rect {
        self.bounds()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rgba;

    fn red() -> CanvasPaint {
        CanvasPaint::Brush(Brush::Solid(Rgba::rgb(255, 0, 0)))
    }

    fn at(p: &mut CanvasPainter, x: i64, y: i64) -> [u8; 4] {
        let d = p.image_data(x, y, 1, 1);
        [d[0], d[1], d[2], d[3]]
    }

    #[test]
    fn drawing_adds_to_what_is_there() {
        let mut p = CanvasPainter::new(20, 20);
        let st = DrawState::default();
        p.fill(
            &Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1),
            FillRule::NonZero,
            &red(),
            &st,
        );
        assert_eq!(at(&mut p, 5, 5), [255, 0, 0, 255]);
        p.fill(
            &Rect::new(10.0, 0.0, 20.0, 10.0).to_path(0.1),
            FillRule::NonZero,
            &CanvasPaint::Brush(Brush::Solid(Rgba::rgb(0, 0, 255))),
            &st,
        );
        assert_eq!(at(&mut p, 5, 5), [255, 0, 0, 255]);
        assert_eq!(at(&mut p, 15, 5), [0, 0, 255, 255]);
        assert_eq!(at(&mut p, 5, 15), [0, 0, 0, 0]);
    }

    #[test]
    fn clearing_and_copying_touch_only_their_area() {
        let mut p = CanvasPainter::new(20, 20);
        let st = DrawState::default();
        p.fill(
            &Rect::new(0.0, 0.0, 20.0, 20.0).to_path(0.1),
            FillRule::NonZero,
            &red(),
            &st,
        );
        p.clear_rect(Rect::new(0.0, 0.0, 5.0, 5.0), &st);
        assert_eq!(at(&mut p, 2, 2), [0, 0, 0, 0]);
        assert_eq!(at(&mut p, 7, 7), [255, 0, 0, 255]);
        let copy = DrawState {
            blend: BlendMode::new(Mix::Normal, Compose::Copy),
            ..st
        };
        p.fill(
            &Rect::new(10.0, 10.0, 15.0, 15.0).to_path(0.1),
            FillRule::NonZero,
            &CanvasPaint::Brush(Brush::Solid(Rgba::from_straight_u8(0, 0, 255, 128))),
            &copy,
        );
        let c = at(&mut p, 12, 12);
        assert_eq!([c[0], c[1], c[2]], [0, 0, 255]);
        assert!((127..=129).contains(&c[3]), "{c:?}");
        assert_eq!(at(&mut p, 17, 17), [255, 0, 0, 255]);
    }

    #[test]
    fn a_clip_bounds_the_drawing() {
        let mut p = CanvasPainter::new(20, 20);
        let clip = Rect::new(0.0, 0.0, 5.0, 20.0).to_path(0.1);
        let st = DrawState {
            clips: std::slice::from_ref(&clip),
            ..DrawState::default()
        };
        p.fill(
            &Rect::new(0.0, 0.0, 20.0, 20.0).to_path(0.1),
            FillRule::NonZero,
            &red(),
            &st,
        );
        assert_eq!(at(&mut p, 2, 2), [255, 0, 0, 255]);
        assert_eq!(at(&mut p, 8, 2), [0, 0, 0, 0]);
    }

    #[test]
    fn a_stroke_follows_the_transform() {
        let mut p = CanvasPainter::new(40, 40);
        let mut line = BezPath::new();
        line.move_to((0.0, 20.0));
        line.line_to((40.0, 20.0));
        let st = DrawState {
            transform: Affine::scale(4.0),
            ..DrawState::default()
        };
        p.stroke(&line, &Stroke::new(1.0), &red(), &st);
        // One unit at four times the size is four pixels wide.
        assert_eq!(at(&mut p, 10, 18), [255, 0, 0, 255]);
        assert_eq!(at(&mut p, 10, 21), [255, 0, 0, 255]);
        assert_eq!(at(&mut p, 10, 23), [0, 0, 0, 0]);
    }

    #[test]
    fn image_data_round_trips() {
        let mut p = CanvasPainter::new(4, 4);
        let data = [0, 0, 255, 128].repeat(4);
        p.put_image_data(&data, 2, 2, 1, 1, Rect::new(0.0, 0.0, 2.0, 2.0));
        let back = p.image_data(1, 1, 2, 2);
        assert_eq!(&back[..4], &[0, 0, 255, 128]);
        assert_eq!(at(&mut p, 0, 0), [0, 0, 0, 0]);
        assert_eq!(at(&mut p, -1, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn a_resize_keeps_what_still_fits() {
        let mut p = CanvasPainter::new(4, 4);
        p.fill(
            &Rect::new(0.0, 0.0, 4.0, 4.0).to_path(0.1),
            FillRule::NonZero,
            &red(),
            &DrawState::default(),
        );
        p.resize(6, 2);
        assert_eq!(p.size(), (6, 2));
        assert_eq!(at(&mut p, 3, 1), [255, 0, 0, 255]);
        assert_eq!(at(&mut p, 5, 1), [0, 0, 0, 0]);
    }

    #[test]
    fn nothing_drawn_is_no_picture() {
        let mut p = CanvasPainter::new(4, 4);
        assert!(p.picture().is_none());
        p.fill(
            &Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1),
            FillRule::NonZero,
            &red(),
            &DrawState::default(),
        );
        let pic = p.picture().expect("a picture");
        assert_eq!((pic.width(), pic.height()), (4, 4));
        // Asking again without drawing is the same picture.
        assert_eq!(p.picture(), Some(pic));
    }

    fn square(x: f64, y: f64, size: f64) -> BezPath {
        Rect::new(x, y, x + size, y + size).to_path(0.1)
    }

    #[test]
    fn a_sharp_shadow_is_an_offset_copy() {
        let mut p = CanvasPainter::new(40, 20);
        let st = DrawState {
            shadow: Some(CanvasShadow {
                color: Rgba::from_straight_u8(0, 0, 255, 255),
                offset: Vec2::new(20.0, 0.0),
                sigma: 0.0,
            }),
            ..DrawState::default()
        };
        p.fill(&square(0.0, 0.0, 10.0), FillRule::NonZero, &red(), &st);
        assert_eq!(at(&mut p, 5, 5), [255, 0, 0, 255]);
        assert_eq!(at(&mut p, 25, 5), [0, 0, 255, 255]);
        assert_eq!(at(&mut p, 15, 5), [0, 0, 0, 0]);
        assert_eq!(at(&mut p, 31, 5), [0, 0, 0, 0]);
    }

    #[test]
    fn a_blurred_shadow_fades_across_the_edge() {
        let mut p = CanvasPainter::new(60, 20);
        let st = DrawState {
            shadow: Some(CanvasShadow {
                color: Rgba::from_straight_u8(0, 0, 0, 255),
                offset: Vec2::ZERO,
                sigma: 2.0,
            }),
            ..DrawState::default()
        };
        // Taller than the canvas so that only the vertical edge blurs.
        let shape = Rect::new(0.0, -20.0, 30.0, 40.0).to_path(0.1);
        let clear = DrawState {
            blend: BlendMode::new(Mix::Normal, Compose::Clear),
            ..DrawState::default()
        };
        p.fill(&shape, FillRule::NonZero, &red(), &st);
        // Take the shape away to see only the shadow.
        p.fill(&shape, FillRule::NonZero, &red(), &clear);
        let alphas: Vec<u8> = (30..40).map(|x| at(&mut p, x, 10)[3]).collect();
        assert!(alphas.windows(2).all(|w| w[0] >= w[1]), "{alphas:?}");
        assert!((100..140).contains(&alphas[0]), "{alphas:?}");
        assert_eq!(alphas[9], 0, "{alphas:?}");
    }

    #[test]
    fn blurring_keeps_the_total() {
        let mut v = vec![0.0f32; 21 * 21];
        v[10 * 21 + 10] = 1.0;
        blur(&mut v, 21, 21, 1.5, 5);
        let total: f32 = v.iter().sum();
        assert!((total - 1.0).abs() < 1e-4, "{total}");
        assert!(v[10 * 21 + 10] > v[10 * 21 + 11]);
        assert_eq!(v[10 * 21 + 10 - 11 + 21], v[10 * 21 + 10 + 11 - 21]);
    }

    #[test]
    fn a_hatch_repeats_from_the_translation_only() {
        let mut cell = vec![0u8; 8 * 8 * 4];
        for y in 0..8 {
            cell[(y * 8 + 4) * 4..(y * 8 + 5) * 4].copy_from_slice(&[0, 255, 0, 255]);
        }
        let image = ImageHandle::from_straight(8, 8, cell).unwrap();
        let hatch = CanvasPaint::Hatch { image };
        let mut p = CanvasPainter::new(40, 20);
        let st = DrawState {
            transform: Affine::translate((1.0, 0.0)) * Affine::scale(2.0),
            ..DrawState::default()
        };
        // Fills take their path in canvas pixels.
        p.fill(
            &(st.transform * square(0.0, 0.0, 10.0)),
            FillRule::NonZero,
            &hatch,
            &st,
        );
        // The columns sit at 1 + 4 and 1 + 12, one pixel wide whatever the
        // scale.
        assert_eq!(at(&mut p, 5, 3), [0, 255, 0, 255]);
        assert_eq!(at(&mut p, 13, 3), [0, 255, 0, 255]);
        assert_eq!(at(&mut p, 6, 3), [0, 0, 0, 0]);
        assert_eq!(at(&mut p, 4, 3), [0, 0, 0, 0]);
    }
}
