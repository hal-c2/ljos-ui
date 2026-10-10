//! Glyph outlines and device placement, shared by both backends.
//!
//! R fills the outlines directly and B0 fills them into its atlas. Because both
//! go through this module they agree on geometry: the same unhinted outline,
//! the same synthetic bold and slant, and the same baseline rounded to a whole
//! device pixel.

use std::collections::HashMap;
use std::sync::Arc;

use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use vello_cpu::kurbo::{Affine, BezPath, Rect, Shape, Vec2};
use vello_cpu::peniko::FontData;

use crate::display_list::GlyphRun;

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

/// The ink box of every glyph of `run`, in the run's own coordinates (the
/// ones its glyph positions use), with the run's synthetic bold and slant
/// applied. `None` for a glyph without ink, such as a space.
///
/// Outlines are read each time: this is for measuring a text once, not for
/// drawing, which goes through the backends' caches.
pub fn glyph_ink_bounds(run: &GlyphRun) -> Vec<Option<Rect>> {
    let mut seen: HashMap<u32, Option<Rect>> = HashMap::new();
    let size = f64::from(run.font_size);
    run.glyphs
        .iter()
        .map(|g| {
            let at_origin = *seen.entry(g.id).or_insert_with(|| {
                load(run, g.id)
                    .filter(|outline| !outline.path.is_empty())
                    .map(|outline| outline.ink_bounds(run, size))
            });
            at_origin.map(|r| r + Vec2::new(f64::from(g.x), f64::from(g.y)))
        })
        .collect()
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
