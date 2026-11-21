//! Glyph outlines and device placement, shared by both backends.
//!
//! R fills the outlines directly and B0 fills them into its atlas. Because both
//! go through this module they agree on geometry: the same unhinted outline,
//! the same synthetic bold and slant, and the same baseline rounded to a whole
//! device pixel.

use rustc_hash::FxHashMap as HashMap;
use std::sync::Arc;

use skrifa::bitmap::{BitmapData, BitmapFormat, BitmapStrikes, Origin};
use skrifa::color::{Brush, ColorPainter, ColorStop, CompositeMode, Transform};
use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::raw::types::BoundingBox;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use vello_cpu::kurbo::{Affine, BezPath, Rect, Shape, Vec2};
use vello_cpu::peniko::{FontData, ImageAlphaType};
use vello_cpu::{Glyph, Level, Pixmap, RenderContext, RenderSettings, Resources};

use crate::backend::RgbaImage;
use crate::display_list::GlyphRun;
use crate::image::ImageHandle;
use crate::rgba::Rgba;

/// A synthetic bold grows every outline by this fraction of the em in total.
const EMBOLDEN_EM: f64 = 1.0 / 24.0;

/// One glyph's outline in font units, y pointing down.
#[derive(Debug)]
pub(crate) struct Outline {
    /// Empty for a glyph without ink (a space).
    pub path: BezPath,
    /// Font units per em.
    pub units_per_em: f64,
    /// Bounding box of `path`.
    pub bounds: Rect,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct OutlineKey {
    font: u64,
    index: u32,
    glyph: u32,
    coords: Vec<i16>,
}

/// Outlines by font, glyph and variation position. A glyph the font cannot
/// draw is remembered as `None`, so it is not looked up again every frame.
#[derive(Debug, Default)]
pub(crate) struct OutlineCache {
    map: HashMap<OutlineKey, Option<Arc<Outline>>>,
}

impl std::fmt::Debug for OutlineKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:x}#{}:{}", self.font, self.index, self.glyph)
    }
}

struct PathPen(BezPath);

impl OutlinePen for PathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((f64::from(x), -f64::from(y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((f64::from(x), -f64::from(y)));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(
            (f64::from(cx0), -f64::from(cy0)),
            (f64::from(x), -f64::from(y)),
        );
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to(
            (f64::from(cx0), -f64::from(cy0)),
            (f64::from(cx1), -f64::from(cy1)),
            (f64::from(x), -f64::from(y)),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

fn load(run: &GlyphRun, glyph: u32) -> Option<Outline> {
    let font = FontRef::from_index(run.font.data.as_ref(), run.font.index).ok()?;
    let units_per_em = f64::from(font.head().ok()?.units_per_em());
    if units_per_em <= 0.0 {
        return None;
    }
    let outline = font.outline_glyphs().get(GlyphId::new(glyph))?;
    let coords: Vec<NormalizedCoord> = run
        .normalized_coords
        .iter()
        .map(|c| NormalizedCoord::from_bits(*c))
        .collect();
    let mut pen = PathPen(BezPath::new());
    outline
        .draw(
            DrawSettings::unhinted(Size::unscaled(), LocationRef::new(&coords)),
            &mut pen,
        )
        .ok()?;
    let bounds = if pen.0.is_empty() {
        Rect::ZERO
    } else {
        pen.0.bounding_box()
    };
    Some(Outline {
        path: pen.0,
        units_per_em,
        bounds,
    })
}

impl OutlineCache {
    /// The outline of `glyph` in the run's font, or `None` when the font has
    /// no outline for it (a bitmap or colour glyph, or a damaged font).
    pub(crate) fn get(&mut self, run: &GlyphRun, glyph: u32) -> Option<Arc<Outline>> {
        let key = OutlineKey {
            font: run.font.data.id(),
            index: run.font.index,
            glyph,
            coords: run.normalized_coords.clone(),
        };
        self.map
            .entry(key)
            .or_insert_with(|| load(run, glyph).map(Arc::new))
            .clone()
    }

    /// Drop everything.
    pub(crate) fn clear(&mut self) {
        self.map.clear();
    }
}

impl Outline {
    /// Maps the outline to a glyph of `size` pixels at the origin: scale, then
    /// the run's slant.
    pub(crate) fn to_size(&self, run: &GlyphRun, size: f64) -> Affine {
        // y points down, so ink above the baseline (negative y) moves right.
        let slant = Affine::new([1.0, 0.0, -f64::from(run.skew), 1.0, 0.0, 0.0]);
        slant * Affine::scale(size / self.units_per_em)
    }

    /// Stroke width, in font units, that draws the run's synthetic bold.
    pub(crate) fn embolden_width(&self, run: &GlyphRun) -> Option<f64> {
        run.embolden.then_some(self.units_per_em * EMBOLDEN_EM)
    }

    /// Ink bounds of the glyph at `size` pixels, at the origin.
    pub(crate) fn ink_bounds(&self, run: &GlyphRun, size: f64) -> Rect {
        let grow = self.embolden_width(run).unwrap_or(0.0) / 2.0;
        self.to_size(run, size)
            .transform_rect_bbox(self.bounds.inflate(grow, grow))
    }
}

/// Colour glyph sizes are told apart in steps of 1/`COLOR_SIZE_STEPS`
/// device pixels: the picture is resampled to whole pixels anyway.
const COLOR_SIZE_STEPS: f64 = 4.0;

/// Past this many pictures, those a frame did not draw are let go before
/// the next, so a text whose size or colour animates does not keep a picture
/// for every step.
const KEEP_COLOR_PICTURES: usize = 256;

/// Past this many bytes of pictures, those a frame did not draw are let go
/// too: a big emoji whose size animates makes a picture of up to 4 MiB a
/// frame, so the count alone would let the cache reach a gigabyte.
const KEEP_COLOR_BYTES: usize = 64 << 20;

/// A colour glyph (an emoji) as a picture, at one size in device pixels.
#[derive(Debug)]
pub(crate) struct ColorGlyph {
    /// Premultiplied RGBA, one texel a device pixel.
    pub image: ImageHandle,
    /// Where `image` goes, in device pixels from the glyph origin, y down.
    /// Whole pixels wide and high, so an upright glyph is drawn texel on
    /// pixel; one larger than [`MAX_COLOR_SIDE`] has a picture that large,
    /// stretched over it.
    pub rect: Rect,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: u64,
    index: u32,
    glyph: u32,
    coords: Vec<i16>,
}

/// How a font draws one glyph.
#[derive(Clone, Copy, Debug)]
enum ColorKind {
    /// With its outline, in the text's colour (or not at all).
    Plain,
    /// As a picture of the font's: `CBDT` or `sbix`.
    Bitmap,
    /// As layers of outlines in the font's palette (`COLR`), some perhaps
    /// in the text's colour (`foreground`).
    Colr { foreground: bool },
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ColorKey {
    glyph: GlyphKey,
    size: u32,
    /// The text's colour, premultiplied, for a glyph that has layers in
    /// it; 0 for every other glyph, whose picture is the same in any.
    foreground: u32,
}

struct Picture {
    glyph: Option<Arc<ColorGlyph>>,
    /// The frame that last drew it.
    used: u64,
}

impl Picture {
    fn bytes(&self) -> usize {
        self.glyph.as_ref().map_or(0, |g| {
            g.image.width() as usize * g.image.height() as usize * 4
        })
    }
}

/// Colour glyph pictures by font, glyph, size and, for a glyph with layers
/// in the text's colour, that colour. A glyph that has none is remembered as
/// [`ColorKind::Plain`], so text without emoji asks once per glyph.
#[derive(Default)]
pub(crate) struct ColorGlyphCache {
    kinds: HashMap<GlyphKey, ColorKind>,
    pictures: HashMap<ColorKey, Picture>,
    /// What the pictures in `pictures` take, in bytes.
    bytes: usize,
    frame: u64,
}

impl std::fmt::Debug for GlyphKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:x}#{}:{}", self.font, self.index, self.glyph)
    }
}

impl std::fmt::Debug for ColorGlyphCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColorGlyphCache")
            .field("kinds", &self.kinds.len())
            .field("pictures", &self.pictures.len())
            .field("bytes", &self.bytes)
            .finish()
    }
}

impl ColorGlyphCache {
    /// The picture of `glyph` of `run` at `size_px` device pixels, its
    /// layers in the text's colour drawn in `foreground`, or `None` when the
    /// font draws that glyph with an outline (or not at all).
    pub(crate) fn get(
        &mut self,
        run: &GlyphRun,
        glyph: u32,
        size_px: f64,
        foreground: Rgba,
    ) -> Option<Arc<ColorGlyph>> {
        let glyph_key = GlyphKey {
            font: run.font.data.id(),
            index: run.font.index,
            glyph,
            coords: run.normalized_coords.clone(),
        };
        let kind = *self
            .kinds
            .entry(glyph_key.clone())
            .or_insert_with(|| color_kind(run, glyph));
        let (foreground, packed) = match kind {
            ColorKind::Plain => return None,
            ColorKind::Bitmap | ColorKind::Colr { foreground: false } => (Rgba::BLACK, 0),
            ColorKind::Colr { foreground: true } => {
                (foreground, u32::from_le_bytes(foreground.to_premul_u8()))
            }
        };
        let steps = color_size_steps(size_px);
        let key = ColorKey {
            glyph: glyph_key,
            size: steps as u32,
            foreground: packed,
        };
        let frame = self.frame;
        let bytes = &mut self.bytes;
        let picture = self.pictures.entry(key).or_insert_with(|| {
            let size = steps / COLOR_SIZE_STEPS;
            let glyph = match kind {
                ColorKind::Bitmap => load_bitmap(run, glyph, size),
                _ => load_colr(run, glyph, size, foreground),
            };
            let picture = Picture {
                glyph: glyph.map(Arc::new),
                used: frame,
            };
            *bytes += picture.bytes();
            picture
        });
        picture.used = frame;
        picture.glyph.clone()
    }

    /// Starts a frame: with many pictures kept, or many bytes of them, the
    /// ones the last frame did not ask for and no one else holds are let go.
    /// A renderer that keeps what it built (B0's cached lists) holds its
    /// pictures, so a picture it still draws stays here too and isn't made
    /// again.
    pub(crate) fn begin_frame(&mut self) {
        if self.pictures.len() > KEEP_COLOR_PICTURES || self.bytes > KEEP_COLOR_BYTES {
            let frame = self.frame;
            let bytes = &mut self.bytes;
            self.pictures.retain(|_, p| {
                let keep =
                    p.used == frame || p.glyph.as_ref().is_some_and(|g| Arc::strong_count(g) > 1);
                if !keep {
                    *bytes -= p.bytes();
                }
                keep
            });
        }
        self.frame += 1;
    }

    /// Drop everything.
    pub(crate) fn clear(&mut self) {
        self.kinds.clear();
        self.pictures.clear();
        self.bytes = 0;
    }
}

/// `size_px` in steps of 1/[`COLOR_SIZE_STEPS`] pixels, at least one.
fn color_size_steps(size_px: f64) -> f64 {
    (size_px * COLOR_SIZE_STEPS).round().max(1.0)
}

/// How the font of `run` draws `glyph`.
fn color_kind(run: &GlyphRun, glyph: u32) -> ColorKind {
    let Ok(font) = FontRef::from_index(run.font.data.as_ref(), run.font.index) else {
        return ColorKind::Plain;
    };
    if has_color_bitmaps(&font) {
        // A bitmap font with a strike for the glyph at any size draws it
        // so at every size.
        if color_bitmap(&font, glyph, f64::from(run.font_size.max(1.0))).is_some() {
            return ColorKind::Bitmap;
        }
    }
    match colr_survey(&font, glyph, &coords(run)) {
        Some(survey) => ColorKind::Colr {
            foreground: survey.foreground,
        },
        None => ColorKind::Plain,
    }
}

fn coords(run: &GlyphRun) -> Vec<NormalizedCoord> {
    run.normalized_coords
        .iter()
        .map(|c| NormalizedCoord::from_bits(*c))
        .collect()
}

fn has_color_bitmaps(font: &FontRef<'_>) -> bool {
    BitmapStrikes::with_format(font, BitmapFormat::Cbdt).is_some()
        || BitmapStrikes::with_format(font, BitmapFormat::Sbix).is_some()
}

/// The largest side, in device pixels, a colour glyph is drawn at: past it
/// a picture would cost more memory than any text is worth.
const MAX_COLOR_SIDE: f64 = 1024.0;

/// A colour bitmap glyph (`CBDT` or `sbix`, the tables colour emoji fonts
/// on Linux, Android and Apple systems use) at `size_px` pixels, and where
/// its bitmap goes from the glyph origin, y down. Monochrome bitmaps
/// (`EBDT`) are left to the outline.
fn color_bitmap<'a>(
    font: &FontRef<'a>,
    glyph: u32,
    size_px: f64,
) -> Option<(skrifa::bitmap::BitmapGlyph<'a>, Rect)> {
    // A font may have both tables, each holding only some glyphs: `CBDT`
    // first, then `sbix` for a glyph `CBDT` lacks.
    let bitmap = [BitmapFormat::Cbdt, BitmapFormat::Sbix]
        .into_iter()
        .find_map(|format| {
            BitmapStrikes::with_format(font, format)?
                .glyph_for_size(Size::new(size_px as f32), GlyphId::new(glyph))
        })?;
    if bitmap.width == 0 || bitmap.height == 0 || bitmap.ppem_x <= 0.0 || bitmap.ppem_y <= 0.0 {
        return None;
    }
    let units_per_em = f64::from(font.head().ok()?.units_per_em());
    if units_per_em <= 0.0 {
        return None;
    }
    let sx = size_px / f64::from(bitmap.ppem_x);
    let sy = size_px / f64::from(bitmap.ppem_y);
    let units = size_px / units_per_em;
    let (w, h) = (f64::from(bitmap.width) * sx, f64::from(bitmap.height) * sy);
    let x0 = f64::from(bitmap.bearing_x) * units + f64::from(bitmap.inner_bearing_x) * sx;
    // The inner bearing is in the strike's pixels, y up: to the top edge
    // for `CBDT`, to the bottom edge for `sbix`.
    let y0 = match bitmap.placement_origin {
        Origin::TopLeft => {
            -(f64::from(bitmap.bearing_y) * units) - f64::from(bitmap.inner_bearing_y) * sy
        }
        Origin::BottomLeft => {
            -(f64::from(bitmap.bearing_y) * units + f64::from(bitmap.inner_bearing_y) * sy) - h
        }
    };
    Some((bitmap, Rect::new(x0, y0, x0 + w, y0 + h)))
}

/// Reads a colour bitmap glyph and resamples it to `size_px`.
fn load_bitmap(run: &GlyphRun, glyph: u32, size_px: f64) -> Option<ColorGlyph> {
    let font = FontRef::from_index(run.font.data.as_ref(), run.font.index).ok()?;
    let (bitmap, at) = color_bitmap(&font, glyph, size_px)?;
    let rect = whole_pixels(at);
    let (w, h) = (bitmap.width, bitmap.height);
    let picture = match bitmap.data {
        BitmapData::Png(bytes) => {
            let png = RgbaImage::from_png(bytes).ok()?;
            if (png.width, png.height) != (w, h) {
                return None;
            }
            ImageHandle::from_straight(w, h, png.data).ok()?
        }
        BitmapData::Bgra(bytes) => {
            let len = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
            let rgba = bytes
                .get(..len)?
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|[b, g, r, a]| [*r, *g, *b, *a])
                .collect();
            ImageHandle::from_premultiplied(w, h, rgba).ok()?
        }
        BitmapData::Mask(_) => return None,
    };
    let (pw, ph) = picture_size(rect);
    let image = picture.resized(pw as u32, ph as u32)?;
    Some(ColorGlyph { image, rect })
}

/// The box a colour bitmap glyph placed at `at` is drawn in: whole pixels
/// wide and high, so an upright glyph is drawn texel on pixel.
fn whole_pixels(at: Rect) -> Rect {
    let (w, h) = (at.width().round().max(1.0), at.height().round().max(1.0));
    Rect::new(at.x0, at.y0, at.x0 + w, at.y0 + h)
}

/// The size of the picture drawn over `rect`: its own, or, for a glyph past
/// [`MAX_COLOR_SIDE`], that large on its longer side. A huge emoji is
/// drawn blurred rather than not at all: it has no outline to fall back to,
/// and a text scaled down by its item (B0 pictures it at its own size)
/// must still show it.
fn picture_size(rect: Rect) -> (f64, f64) {
    let (w, h) = (rect.width(), rect.height());
    let shrink = (MAX_COLOR_SIDE / w.max(h)).min(1.0);
    ((w * shrink).round().max(1.0), (h * shrink).round().max(1.0))
}

/// The palette index that stands for the text's own colour.
const FOREGROUND_INDEX: u16 = 0xFFFF;

/// What a `COLR` glyph covers and whether it uses the text's colour, read by
/// walking its paint without drawing it.
struct ColrSurvey {
    /// Font units, y up.
    bounds: Rect,
    foreground: bool,
}

/// Walks a `COLR` glyph: the box its layers can cover is the union of every
/// clip it fills inside (layer outlines and clip boxes, each cut by the
/// clips around it), so nothing outside the font's own shapes counts.
struct Surveyor<'a> {
    outlines: skrifa::outline::OutlineGlyphCollection<'a>,
    location: LocationRef<'a>,
    transforms: Vec<Affine>,
    clips: Vec<Rect>,
    bounds: Option<Rect>,
    /// For each layer pushed: how it is put down, and the bounds under it.
    layers: Vec<(CompositeMode, Option<Rect>)>,
    foreground: bool,
}

impl Surveyor<'_> {
    fn transform(&self) -> Affine {
        self.transforms.last().copied().unwrap_or_default()
    }

    fn push_clip(&mut self, rect: Rect) {
        let rect = self.transform().transform_rect_bbox(rect);
        let active = self
            .clips
            .last()
            .map_or(rect, |parent| parent.intersect(rect));
        self.clips.push(active);
    }

    fn brush(&mut self, brush: &Brush<'_>) {
        let stops: &[ColorStop] = match brush {
            Brush::Solid { palette_index, .. } => {
                self.foreground |= *palette_index == FOREGROUND_INDEX;
                &[]
            }
            Brush::LinearGradient { color_stops, .. }
            | Brush::RadialGradient { color_stops, .. }
            | Brush::SweepGradient { color_stops, .. } => color_stops,
        };
        self.foreground |= stops.iter().any(|s| s.palette_index == FOREGROUND_INDEX);
    }
}

impl ColorPainter for Surveyor<'_> {
    fn push_transform(&mut self, t: Transform) {
        let t = Affine::new([t.xx, t.yx, t.xy, t.yy, t.dx, t.dy].map(f64::from));
        self.transforms.push(self.transform() * t);
    }

    fn pop_transform(&mut self) {
        self.transforms.pop();
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        let mut pen = BoundsPen::default();
        let drawn = self.outlines.get(glyph_id).is_some_and(|outline| {
            outline
                .draw(
                    DrawSettings::unhinted(Size::unscaled(), self.location),
                    &mut pen,
                )
                .is_ok()
        });
        // A glyph without an outline clips everything away.
        let rect = pen.bounds.filter(|_| drawn).unwrap_or(Rect::ZERO);
        self.push_clip(rect);
    }

    fn push_clip_box(&mut self, b: BoundingBox<f32>) {
        self.push_clip(Rect::new(
            f64::from(b.x_min),
            f64::from(b.y_min),
            f64::from(b.x_max),
            f64::from(b.y_max),
        ));
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
    }

    fn fill(&mut self, brush: Brush<'_>) {
        self.brush(&brush);
        // A fill outside every clip would cover the plane; a font that does
        // that has no box to draw it in, so it counts for nothing.
        if let Some(clip) = self.clips.last().copied().filter(|c| c.area() > 0.0) {
            self.bounds = Some(self.bounds.map_or(clip, |b| b.union(clip)));
        }
    }

    fn push_layer(&mut self, mode: CompositeMode) {
        self.layers.push((mode, self.bounds.take()));
    }

    fn pop_layer(&mut self) {
        let Some((mode, backdrop)) = self.layers.pop() else {
            return;
        };
        let source = self.bounds.take();
        let union = |a: Option<Rect>, b: Option<Rect>| match (a, b) {
            (Some(a), Some(b)) => Some(a.union(b)),
            (a, b) => a.or(b),
        };
        let meet = |a: Option<Rect>, b: Option<Rect>| {
            a.zip(b)
                .map(|(a, b)| a.intersect(b))
                .filter(|r| r.area() > 0.0)
        };
        // Where the result can have ink, by the Porter-Duff operators; a
        // blend mode keeps both.
        self.bounds = match mode {
            CompositeMode::Clear => None,
            CompositeMode::Src | CompositeMode::SrcOut => source,
            CompositeMode::Dest | CompositeMode::DestOut => backdrop,
            CompositeMode::SrcIn | CompositeMode::DestIn => meet(source, backdrop),
            CompositeMode::SrcAtop => backdrop,
            CompositeMode::DestAtop => source,
            _ => union(source, backdrop),
        };
    }
}

#[derive(Default)]
struct BoundsPen {
    bounds: Option<Rect>,
}

impl BoundsPen {
    fn point(&mut self, x: f32, y: f32) {
        let (x, y) = (f64::from(x), f64::from(y));
        let at = Rect::new(x, y, x, y);
        self.bounds = Some(self.bounds.map_or(at, |b| b.union(at)));
    }
}

impl OutlinePen for BoundsPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.point(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.point(x, y);
    }
    // Control points bound a curve, so counting them gives a box that holds
    // it.
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.point(cx, cy);
        self.point(x, y);
    }
    fn curve_to(&mut self, c0x: f32, c0y: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        self.point(c0x, c0y);
        self.point(c1x, c1y);
        self.point(x, y);
    }
    fn close(&mut self) {}
}

/// Surveys `glyph` of `font` as a `COLR` glyph (version 1 before 0, as
/// fonts that have both mean). `None` when the font has no colour layers
/// for it, or they cover nothing.
fn colr_survey(font: &FontRef<'_>, glyph: u32, coords: &[NormalizedCoord]) -> Option<ColrSurvey> {
    let color = font.color_glyphs().get(GlyphId::new(glyph))?;
    let location = LocationRef::new(coords);
    let mut surveyor = Surveyor {
        outlines: font.outline_glyphs(),
        location,
        transforms: vec![Affine::IDENTITY],
        clips: Vec::new(),
        bounds: None,
        layers: Vec::new(),
        foreground: false,
    };
    color.paint(location, &mut surveyor).ok()?;
    // A version 1 glyph may state its box (`ClipList`); it is the font's
    // promise of where it draws, tighter than the layers' outlines.
    let stated = color.bounding_box(location, Size::unscaled()).map(|b| {
        Rect::new(
            f64::from(b.x_min),
            f64::from(b.y_min),
            f64::from(b.x_max),
            f64::from(b.y_max),
        )
    });
    let bounds = stated.or(surveyor.bounds).filter(|b| b.area() > 0.0)?;
    Some(ColrSurvey {
        bounds,
        foreground: surveyor.foreground,
    })
}

/// Where a `COLR` glyph is drawn at `size_px`, in whole device pixels from
/// the glyph origin, y down.
fn colr_rect(font: &FontRef<'_>, survey: &ColrSurvey, size_px: f64) -> Option<Rect> {
    let units_per_em = f64::from(font.head().ok()?.units_per_em());
    if units_per_em <= 0.0 {
        return None;
    }
    let s = size_px / units_per_em;
    let b = survey.bounds;
    let rect = Rect::new(
        (b.x0 * s).floor(),
        (-b.y1 * s).floor(),
        (b.x1 * s).ceil(),
        (-b.y0 * s).ceil(),
    );
    (rect.width() >= 1.0 && rect.height() >= 1.0).then_some(rect)
}

/// The picture a `COLR` glyph covering `rect` is drawn into: its side in
/// pixels on each axis, the one scale from glyph pixels to picture pixels,
/// and the box the picture is placed over. A capped glyph keeps its shape:
/// both axes take the same scale and each side is rounded up, so the
/// picture may hold a sliver of empty pixels past `rect`, and the box grows
/// by the same sliver so the picture is never stretched.
fn colr_picture(rect: Rect) -> (f64, f64, f64, Rect) {
    let (w, h) = (rect.width(), rect.height());
    let shrink = (MAX_COLOR_SIDE / w.max(h)).min(1.0);
    // The long side lands on MAX_COLOR_SIDE up to rounding; don't let that
    // rounding add a pixel past the cap.
    let side = |v: f64| ((v * shrink - 1e-6).ceil()).clamp(1.0, MAX_COLOR_SIDE);
    let (pw, ph) = (side(w), side(h));
    let placed = Rect::new(
        rect.x0,
        rect.y0,
        rect.x0 + pw / shrink,
        rect.y0 + ph / shrink,
    );
    (pw, ph, shrink, placed)
}

/// From glyph pixels to the pixels of a picture placed at `placed`.
fn picture_transform(placed: Rect, shrink: f64) -> Affine {
    Affine::scale(shrink) * Affine::translate((-placed.x0, -placed.y0))
}

/// Draws a `COLR` glyph (the colour outline tables Windows' and newer
/// Android's emoji fonts use) at `size_px` into a picture, its layers in the
/// text's colour filled with `foreground`.
fn load_colr(run: &GlyphRun, glyph: u32, size_px: f64, foreground: Rgba) -> Option<ColorGlyph> {
    let font = FontRef::from_index(run.font.data.as_ref(), run.font.index).ok()?;
    let coords = coords(run);
    let survey = colr_survey(&font, glyph, &coords)?;
    let (pw, ph, shrink, rect) = colr_picture(colr_rect(&font, &survey, size_px)?);
    let (w, h) = (pw as u16, ph as u16);
    let mut ctx = RenderContext::new_with(
        w,
        h,
        RenderSettings {
            level: Level::baseline(),
            num_threads: 0,
        },
    );
    let mut resources = Resources::new();
    let [r, g, b, a] = foreground.to_straight();
    ctx.set_paint(vello_cpu::color::AlphaColor::<vello_cpu::color::Srgb>::new(
        [r, g, b, a],
    ));
    ctx.set_transform(picture_transform(rect, shrink));
    ctx.glyph_run(&mut resources, &run.font)
        .font_size(size_px as f32)
        .normalized_coords(&run.normalized_coords)
        .fill_glyphs(std::iter::once(Glyph {
            id: glyph,
            x: 0.0,
            y: 0.0,
        }))
        .ok()?;
    ctx.flush();
    let mut pixmap = Pixmap::new(w, h);
    ctx.render(&mut pixmap, &mut resources);
    let image = ImageHandle::from_premultiplied(
        u32::from(w),
        u32::from(h),
        pixmap.take_rgba8(ImageAlphaType::AlphaPremultiplied),
    )
    .ok()?;
    Some(ColorGlyph { image, rect })
}

/// True when `t` only scales (by positive factors) and translates, so text
/// stays upright and can be snapped to the pixel grid.
pub(crate) fn is_upright(t: Affine) -> bool {
    let [a, b, c, d, _, _] = t.as_coeffs();
    b.abs() < 1e-6 && c.abs() < 1e-6 && a > 0.0 && d > 0.0
}

/// How far to move a run down, in device pixels, so that its baseline lands on
/// a whole device pixel. Zero for rotated or sheared text.
pub(crate) fn baseline_snap(to_device: Affine, baseline_y: f64) -> f64 {
    if !is_upright(to_device) {
        return 0.0;
    }
    let [_, _, _, d, _, f] = to_device.as_coeffs();
    let y = d * baseline_y + f;
    y.round() - y
}

/// An underline or strike-through bar in device pixels: its top and its ends
/// on whole pixels and at least one pixel thick, so a thin bar does not blur
/// over two rows. Whole-pixel ends also keep the two backends alike: B0 draws
/// the bar as a plain quad, which has hard edges.
/// `None` when the transform is not upright (draw the bar unsnapped then).
pub(crate) fn snap_bar(to_device: Affine, bar: Rect) -> Option<Rect> {
    if !is_upright(to_device) {
        return None;
    }
    let r = to_device.transform_rect_bbox(bar);
    let y0 = r.y0.round();
    let h = r.height().round().max(1.0);
    Some(Rect::new(r.x0.round(), y0, r.x1.round(), y0 + h))
}

/// Numbers a font states about itself as a whole, in font units, straight
/// from its `head`, `hhea` and `OS/2` tables. Which of the three sets of
/// vertical metrics to believe is the text engine's decision, not ours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontTables {
    /// Font units per em.
    pub units_per_em: u16,
    /// `hhea` ascender.
    pub ascender: i16,
    /// `hhea` descender; negative below the baseline.
    pub descender: i16,
    /// `hhea` line gap.
    pub line_gap: i16,
    /// The widest advance of any glyph.
    pub advance_width_max: u16,
    /// The smallest left side bearing of any glyph: negative when some glyph
    /// has ink to the left of its origin.
    pub min_left_side_bearing: i16,
    /// The smallest right side bearing of any glyph: negative when some glyph
    /// has ink beyond its advance.
    pub min_right_side_bearing: i16,
    /// The `OS/2` table, for a font that has one.
    pub os2: Option<Os2Table>,
}

/// The part of a font's `OS/2` table that describes the font as a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Os2Table {
    /// Typographic ascender.
    pub typo_ascender: i16,
    /// Typographic descender; negative below the baseline.
    pub typo_descender: i16,
    /// Typographic line gap.
    pub typo_line_gap: i16,
    /// Windows ascent.
    pub win_ascent: u16,
    /// Windows descent; positive below the baseline.
    pub win_descent: u16,
    /// The font asks for its typographic metrics to set line spacing
    /// (`fsSelection` bit 7).
    pub use_typo_metrics: bool,
    /// Average advance of the font's glyphs.
    pub x_avg_char_width: i16,
}

/// Reads [`FontTables`]. `None` for data that is not a font with horizontal
/// metrics. Cheap: it looks up three tables and copies a few fields.
pub fn font_tables(font: &FontData) -> Option<FontTables> {
    let face = FontRef::from_index(font.data.as_ref(), font.index).ok()?;
    let units_per_em = face.head().ok()?.units_per_em();
    if units_per_em == 0 {
        return None;
    }
    let hhea = face.hhea().ok()?;
    let os2 = face.os2().ok().map(|os2| Os2Table {
        typo_ascender: os2.s_typo_ascender(),
        typo_descender: os2.s_typo_descender(),
        typo_line_gap: os2.s_typo_line_gap(),
        win_ascent: os2.us_win_ascent(),
        win_descent: os2.us_win_descent(),
        use_typo_metrics: os2.fs_selection().bits() & 0x80 != 0,
        x_avg_char_width: os2.x_avg_char_width(),
    });
    Some(FontTables {
        units_per_em,
        ascender: hhea.ascender().to_i16(),
        descender: hhea.descender().to_i16(),
        line_gap: hhea.line_gap().to_i16(),
        advance_width_max: hhea.advance_width_max().to_u16(),
        min_left_side_bearing: hhea.min_left_side_bearing().to_i16(),
        min_right_side_bearing: hhea.min_right_side_bearing().to_i16(),
        os2,
    })
}

/// Whether any glyph of `run` is drawn in its own colours (an emoji, of a
/// bitmap font or one of colour layers): a run in a transparent colour
/// still shows those.
pub fn run_has_color_glyphs(run: &GlyphRun) -> bool {
    // Asked on every paint of a transparent text, so it reads the font's
    // table directory once and looks each glyph up without walking or
    // decoding anything.
    let Ok(font) = FontRef::from_index(run.font.data.as_ref(), run.font.index) else {
        return false;
    };
    let bitmaps = has_color_bitmaps(&font);
    let layers = font.colr().is_ok();
    if !bitmaps && !layers {
        return false;
    }
    let size = f64::from(run.font_size.max(1.0));
    let colr = font.color_glyphs();
    run.glyphs.iter().any(|g| {
        (bitmaps && color_bitmap(&font, g.id, size).is_some())
            || (layers && colr.get(GlyphId::new(g.id)).is_some())
    })
}

/// The box the colour glyphs (emoji) of `run` are drawn in, in the run's own
/// coordinates; `None` when it has none. A shaper's culling box for a run
/// is guessed from its advance and the font's metrics, which a picture may
/// reach past.
pub fn color_glyph_ink(run: &GlyphRun) -> Option<Rect> {
    if !run_has_color_glyphs(run) {
        return None;
    }
    let size = f64::from(run.font_size);
    let mut seen: HashMap<u32, Option<Rect>> = HashMap::default();
    run.glyphs
        .iter()
        .filter_map(|g| {
            let at_origin = *seen
                .entry(g.id)
                .or_insert_with(|| color_ink_bounds(run, g.id, size));
            at_origin.map(|r| r + Vec2::new(f64::from(g.x), f64::from(g.y)))
        })
        .reduce(|a, b| a.union(b))
}

/// The ink box of every glyph of `run`, in the run's own coordinates (the
/// ones its glyph positions use), with the run's synthetic bold and slant
/// applied. `None` for a glyph without ink, such as a space.
///
/// A colour glyph (an emoji) counts the box its picture is drawn in.
///
/// Outlines are read each time: this is for measuring a text once, not for
/// drawing, which goes through the backends' caches.
pub fn glyph_ink_bounds(run: &GlyphRun) -> Vec<Option<Rect>> {
    let mut seen: HashMap<u32, Option<Rect>> = HashMap::default();
    let size = f64::from(run.font_size);
    run.glyphs
        .iter()
        .map(|g| {
            let at_origin = *seen.entry(g.id).or_insert_with(|| {
                color_ink_bounds(run, g.id, size).or_else(|| {
                    load(run, g.id)
                        .filter(|outline| !outline.path.is_empty())
                        .map(|outline| outline.ink_bounds(run, size))
                })
            });
            at_origin.map(|r| r + Vec2::new(f64::from(g.x), f64::from(g.y)))
        })
        .collect()
}

/// The box a colour glyph is drawn in at `size` pixels, at the origin,
/// without drawing its picture.
fn color_ink_bounds(run: &GlyphRun, glyph: u32, size: f64) -> Option<Rect> {
    let font = FontRef::from_index(run.font.data.as_ref(), run.font.index).ok()?;
    // At the size and in the box it is drawn in.
    let size = color_size_steps(size) / COLOR_SIZE_STEPS;
    match color_kind(run, glyph) {
        ColorKind::Plain => None,
        ColorKind::Bitmap => color_bitmap(&font, glyph, size).map(|(_, at)| whole_pixels(at)),
        ColorKind::Colr { .. } => {
            let survey = colr_survey(&font, glyph, &coords(run))?;
            colr_rect(&font, &survey, size)
        }
    }
}

/// The advance width the font gives each of `glyphs`, in font units, and the
/// font's units per em. `normalized_coords` picks the instance of a variable
/// font, as in [`GlyphRun::normalized_coords`]. A glyph the font does not have
/// advances by 0. `None` for data that is not a font.
///
/// These are the advances before a shaper kerns or spaces them: a text
/// engine that rounds them the way another toolkit does needs both.
pub fn glyph_advances(
    font: &FontData,
    normalized_coords: &[i16],
    glyphs: &[u32],
) -> Option<(u16, Vec<f32>)> {
    let face = FontRef::from_index(font.data.as_ref(), font.index).ok()?;
    let units_per_em = face.head().ok()?.units_per_em();
    if units_per_em == 0 {
        return None;
    }
    let coords: Vec<NormalizedCoord> = normalized_coords
        .iter()
        .map(|c| NormalizedCoord::from_bits(*c))
        .collect();
    let metrics = face.glyph_metrics(Size::unscaled(), LocationRef::new(&coords));
    let advances = glyphs
        .iter()
        .map(|id| metrics.advance_width(GlyphId::new(*id)).unwrap_or(0.0))
        .collect();
    Some((units_per_em, advances))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display_list::PositionedGlyph;
    use vello_cpu::peniko::Blob;

    const COLR_FONT: &[u8] =
        include_bytes!("../../../features/text/fonts/LjosColrTest-Regular.ttf");

    #[test]
    fn a_capped_picture_holds_a_glyph_much_taller_than_wide() {
        let rect = Rect::new(-1.0, -2000.0, 2.0, 48.0);
        let (pw, ph, shrink, placed) = colr_picture(rect);
        assert_eq!((pw, ph), (2.0, 1024.0));
        let t = picture_transform(placed, shrink);
        let corner = |x, y| {
            let p = t * vello_cpu::kurbo::Point::new(x, y);
            (p.x, p.y)
        };
        assert_eq!(corner(placed.x0, placed.y0), (0.0, 0.0));
        let (x, y) = corner(placed.x1, placed.y1);
        assert!((x - pw).abs() < 1e-9 && (y - ph).abs() < 1e-9, "{x}, {y}");
        // Everything the glyph covers is inside its picture.
        let (x, y) = corner(rect.x1, rect.y1);
        assert!(x <= pw + 1e-9 && y <= ph + 1e-9, "{x}, {y}");
    }

    #[test]
    fn a_capped_thin_glyph_is_scaled_alike_on_both_axes() {
        // 3 px by 2050 px: rounding each side on its own would draw the
        // short side at 1/3 of its width and the long one at about 1/2.
        for rect in [
            Rect::new(0.0, -2050.0, 3.0, 0.0),
            Rect::new(0.0, -3.0, 2050.0, 0.0),
            Rect::new(-7.0, -1500.0, 4.0, 500.0),
        ] {
            let (pw, ph, shrink, placed) = colr_picture(rect);
            let [a, b, c, d, _, _] = picture_transform(placed, shrink).as_coeffs();
            assert_eq!((b, c), (0.0, 0.0));
            assert_eq!(a, d, "{rect:?} is stretched");
            assert!(pw.max(ph) <= MAX_COLOR_SIDE, "{rect:?}: {pw} x {ph}");
            assert!(placed.contains(rect.origin()));
            assert!(
                placed.x1 >= rect.x1 && placed.y1 >= rect.y1,
                "{rect:?} in {placed:?}"
            );
            // The empty sliver is less than one picture pixel on each axis.
            assert!(placed.x1 - rect.x1 < 1.0 / shrink && placed.y1 - rect.y1 < 1.0 / shrink);
        }
    }

    #[test]
    fn an_uncapped_colr_picture_is_the_glyph_box_itself() {
        let rect = Rect::new(-2.0, -30.0, 26.0, 6.0);
        let (pw, ph, shrink, placed) = colr_picture(rect);
        assert_eq!((pw, ph, shrink, placed), (28.0, 36.0, 1.0, rect));
    }

    #[test]
    fn huge_pictures_whose_size_animates_stay_within_the_byte_budget() {
        let font = FontData::new(Blob::new(Arc::new(COLR_FONT)), 0);
        let glyphs = vec![PositionedGlyph {
            id: 1,
            x: 0.0,
            y: 0.0,
        }];
        let run = GlyphRun::new(font, 20.0, glyphs, 0.0, Rect::ZERO);
        let mut cache = ColorGlyphCache::default();
        let mut most = 0;
        // Each step is a new size past the cap, so a new 1024 px picture.
        for step in 0..40 {
            cache.begin_frame();
            let size = 2000.0 + f64::from(step);
            let picture = cache.get(&run, 1, size, Rgba::BLACK).unwrap();
            assert_eq!(picture.image.width().max(picture.image.height()), 1024);
            most = most.max(cache.bytes);
        }
        let one = 1024 * 1024 * 4;
        assert!(most <= KEEP_COLOR_BYTES + one, "{most} bytes kept");
        let sum: usize = cache.pictures.values().map(Picture::bytes).sum();
        assert_eq!(cache.bytes, sum);
    }
}
