//! The engine: fonts, shaping, line breaking, and the two caches.

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use ljos_render::kurbo::Rect;
use ljos_render::{Decorations, FontTables, GlyphRun, PositionedGlyph};
use parley::fontique::GenericFamily;
use parley::{
    Alignment, AlignmentOptions, BaseDirection, BreakReason, FontContext, FontData, FontFamily,
    FontFamilyName, FontStyle, FontWeight, LayoutContext, OverflowWrap, PositionedLayoutItem,
    StyleProperty, WordBreak,
};

use crate::cache::FrameCache;
use crate::elide::{Clusters, ELLIPSIS};
use crate::face;
use crate::fonts::{self, FontError};
use crate::layout::{FontMetrics, Line, LineRun, TextLayout};
use crate::snap;
use crate::style::{ElideMode, LayoutParams, LineHeight, ParamsKey, TextStyle, WrapMode};

/// A layout unused for this many frames is dropped.
const KEEP_FRAMES: u64 = 30;
/// Upper bounds, for a frame that lays out a great many different strings.
const MAX_LAYOUTS: usize = 2048;
const MAX_SHAPES: usize = 1024;
const MAX_STYLES: usize = 256;
/// Font sizes beyond this are a mistake in a binding, not a design.
const MAX_FONT_SIZE: f32 = 4096.0;

type Shape = parley::Layout<()>;

/// Counters for tests and diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Layouts held now.
    pub layouts: usize,
    /// Shaped texts held now.
    pub shapes: usize,
    /// Layout requests answered from the cache.
    pub layout_hits: u64,
    /// Layout requests that had to break lines.
    pub layout_misses: u64,
    /// Texts that had to be shaped (the expensive step).
    pub shapes_built: u64,
}

/// How a text may break, which Parley decides while shaping. Texts of
/// different classes cannot share a shaped entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum BreakClass {
    /// At word boundaries only.
    Word,
    /// At any character.
    Anywhere,
    /// At word boundaries, inside a word when nothing else fits.
    WordOrAnywhere,
}

impl BreakClass {
    fn of(wrap: WrapMode) -> Self {
        match wrap {
            WrapMode::NoWrap | WrapMode::WordWrap => Self::Word,
            WrapMode::WrapAnywhere => Self::Anywhere,
            WrapMode::Wrap => Self::WordOrAnywhere,
        }
    }
}

/// A text shaped once. Changing the width only breaks its lines again.
struct ShapedEntry {
    text: Box<str>,
    style: TextStyle,
    class: BreakClass,
    shape: Shape,
    natural_width: f32,
    rtl: bool,
}

struct LayoutEntry {
    text: Box<str>,
    style: TextStyle,
    params: ParamsKey,
    layout: Arc<TextLayout>,
}

#[derive(Clone, Copy)]
struct StyleMetrics {
    font: FontMetrics,
    ellipsis: f32,
}

/// Ascent and descent of one line.
#[derive(Clone, Copy)]
struct Vertical {
    ascent: f64,
    descent: f64,
}

impl Vertical {
    /// Height of the line on its own. Qt rounds it up to a whole pixel.
    fn natural(self) -> f64 {
        (self.ascent + self.descent).ceil()
    }
}

struct RawRun {
    font: FontData,
    font_size: f32,
    /// x from the line's left edge, y from the baseline.
    glyphs: Vec<PositionedGlyph>,
    offset: f32,
    advance: f32,
    ascent: f32,
    descent: f32,
    coords: Vec<i16>,
    embolden: bool,
    skew: f32,
    /// Top above the baseline, and thickness.
    underline: Option<(f32, f32)>,
    strikeout: Option<(f32, f32)>,
}

/// A line as Parley broke it, before it has a place in the layout.
struct RawLine {
    range: Range<usize>,
    width: f32,
    vertical: Vertical,
    /// Ends in a line break that is in the text.
    hard_break: bool,
    justified: bool,
    runs: Vec<RawRun>,
}

impl RawLine {
    fn empty(vertical: Vertical) -> Self {
        Self {
            range: 0..0,
            width: 0.0,
            vertical,
            hard_break: false,
            justified: false,
            runs: Vec::new(),
        }
    }
}

/// Which fonts a style resolves to.
struct Families {
    system: bool,
    default_family: Option<Arc<str>>,
}

impl Families {
    /// The family list for a style: what it names, then the engine's
    /// defaults, so a misspelt or missing family still shows text.
    fn stack<'a>(&'a self, style: &'a TextStyle) -> Vec<FontFamilyName<'a>> {
        let mut names = Vec::new();
        let mut add = |list: &'a str| {
            for part in list.split(',') {
                let name = part.trim().trim_matches(['"', '\'']);
                if name.is_empty() {
                    continue;
                }
                names.push(match GenericFamily::parse(name) {
                    Some(generic) => FontFamilyName::Generic(generic),
                    None => FontFamilyName::Named(Cow::Borrowed(name)),
                });
            }
        };
        add(&style.family);
        if let Some(family) = &self.default_family {
            add(family);
        }
        if self.system {
            names.push(FontFamilyName::Generic(GenericFamily::SystemUi));
        }
        names.push(FontFamilyName::Generic(GenericFamily::SansSerif));
        names
    }
}

/// Lays out text. One per scene, or one shared by several scenes.
pub struct TextEngine {
    fcx: FontContext,
    lcx: LayoutContext<()>,
    families: Families,
    shapes: FrameCache<u64, ShapedEntry>,
    layouts: FrameCache<u64, LayoutEntry>,
    styles: HashMap<TextStyle, StyleMetrics>,
    frame: u64,
    keep_frames: u64,
    generation: u64,
    stats: CacheStats,
}

impl std::fmt::Debug for TextEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextEngine")
            .field("system_fonts", &self.families.system)
            .field("default_family", &self.families.default_family)
            .field("frame", &self.frame)
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn floor64(v: f64) -> f64 {
    (v * 64.0).floor() / 64.0
}

/// The shaper's own idea of a font's vertical metrics.
fn shaped_vertical(fm: &parley::FontMetrics) -> face::FaceMetrics {
    face::FaceMetrics {
        ascent: f64::from(fm.ascent),
        descent: f64::from(fm.descent),
        leading: f64::from(fm.leading),
    }
}

/// To the nearest 1/64 pixel, halves up.
fn round64(v: f64) -> f64 {
    (v * 64.0 + 0.5).floor() / 64.0
}

fn ceil64(v: f32) -> f64 {
    (f64::from(v) * 64.0).ceil() / 64.0
}

fn key_hash(text: &str, style: &TextStyle, extra: impl Hash) -> u64 {
    // DefaultHasher::new() has fixed keys: the same text hashes the same in
    // every run, so cache behaviour is reproducible.
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    style.hash(&mut h);
    extra.hash(&mut h);
    h.finish()
}

fn shape(
    fcx: &mut FontContext,
    lcx: &mut LayoutContext<()>,
    families: &Families,
    text: &str,
    style: &TextStyle,
    class: BreakClass,
    base: BaseDirection,
) -> Shape {
    let names = families.stack(style);
    let size = if style.pixel_size.is_finite() {
        style.pixel_size.clamp(0.0, MAX_FONT_SIZE)
    } else {
        0.0
    };
    let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
    // Scale 1 and no quantising: layouts are in logical pixels and unhinted,
    // so they do not depend on the screen and one cache entry fits every scale.
    let mut b = lcx.ranged_builder(fcx, text, 1.0, false);
    b.push_default(StyleProperty::FontFamily(FontFamily::List(Cow::Borrowed(
        &names,
    ))));
    b.push_default(StyleProperty::FontSize(size));
    b.push_default(StyleProperty::FontWeight(FontWeight::new(
        finite(style.weight).clamp(1.0, 1000.0),
    )));
    b.push_default(StyleProperty::FontStyle(if style.italic {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    }));
    b.push_default(StyleProperty::Underline(style.underline));
    b.push_default(StyleProperty::Strikethrough(style.strikeout));
    b.push_default(StyleProperty::LetterSpacing(finite(style.letter_spacing)));
    b.push_default(StyleProperty::WordSpacing(finite(style.word_spacing)));
    match class {
        BreakClass::Word => {}
        BreakClass::Anywhere => {
            b.push_default(StyleProperty::WordBreak(WordBreak::BreakAll));
            b.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
        }
        BreakClass::WordOrAnywhere => {
            b.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
        }
    }
    b.set_base_direction(base);
    b.build(text)
}

/// Break lines at `max` and put every line at x 0. Alignment is the item's
/// business, so one layout serves every alignment.
fn break_lines(shape: &mut Shape, max: Option<f32>, justify: bool) {
    shape.break_all_lines(max);
    let (alignment, last) = if justify {
        // Qt leaves the last line of a paragraph unstretched.
        (Alignment::Justify, Some(Alignment::Left))
    } else {
        (Alignment::Left, None)
    };
    shape.align(
        alignment,
        AlignmentOptions {
            align_when_overflowing: true,
            last_line_alignment: last,
        },
    );
}

fn extract(shape: &Shape, fallback: Vertical, justify_width: Option<f32>) -> Vec<RawLine> {
    let mut out = Vec::with_capacity(shape.len());
    for line in shape.lines() {
        let m = *line.metrics();
        let reason = line.break_reason();
        let wrapped = matches!(reason, BreakReason::Regular | BreakReason::Emergency);
        let mut runs = Vec::new();
        let mut vertical: Option<Vertical> = None;
        let justified = wrapped && justify_width.is_some();
        // Spaces at a wrap hang outside the line and do not count.
        let content = m.advance - m.hanging_advance;
        let hangs = |pen: f32, advance: f32| {
            m.hanging_advance > 0.0 && (pen >= content - 1e-3 || pen + advance <= 1e-3)
        };
        // Qt's advances are a little shorter than the shaper's, so every
        // glyph moves left by what the ones before it gave up. A justified
        // line stays as it was stretched.
        let mut shift = 0.0_f32;
        let mut width = 0.0_f32;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(gr) = item else {
                continue;
            };
            let run = gr.run();
            let fm = *run.font_metrics();
            let chosen = face::vertical(run.font(), run.font_size(), shaped_vertical(&fm));
            let (ascent, descent) = (floor64(chosen.ascent), floor64(chosen.descent));
            vertical = Some(vertical.map_or(Vertical { ascent, descent }, |v| Vertical {
                ascent: v.ascent.max(ascent),
                descent: v.descent.max(descent),
            }));
            let baseline = gr.baseline();
            let (placed, advances) = snapped(&gr);
            let offset = gr.offset() - shift;
            let mut pen = gr.offset();
            let mut advance = 0.0_f32;
            let mut glyphs = Vec::with_capacity(placed.len());
            for (g, qt) in placed.iter().zip(advances) {
                glyphs.push(PositionedGlyph {
                    id: g.id,
                    x: g.x - shift,
                    y: g.y - baseline,
                });
                if justified {
                    advance += g.advance;
                } else {
                    advance += qt;
                    if !hangs(pen, g.advance) {
                        shift += g.advance - qt;
                        width += qt;
                    }
                }
                pen += g.advance;
            }
            if glyphs.is_empty() {
                continue;
            }
            let style = gr.style();
            let underline = style.underline.as_ref().map(|d| {
                (
                    d.offset.unwrap_or(fm.underline_offset),
                    d.size.unwrap_or(fm.underline_size),
                )
            });
            let strikeout = style.strikethrough.as_ref().map(|d| {
                (
                    d.offset.unwrap_or(fm.strikethrough_offset),
                    // Qt draws both bars with the underline's thickness.
                    d.size.unwrap_or(fm.underline_size),
                )
            });
            let synthesis = run.synthesis();
            runs.push(RawRun {
                font: run.font().clone(),
                font_size: run.font_size(),
                glyphs,
                offset,
                advance,
                ascent: chosen.ascent as f32,
                descent: chosen.descent as f32,
                coords: coords(run),
                embolden: synthesis.embolden(),
                skew: synthesis
                    .skew()
                    .map_or(0.0, |degrees| degrees.to_radians().tan()),
                underline,
                strikeout,
            });
        }
        out.push(RawLine {
            range: line.text_range(),
            width: match justify_width {
                Some(w) if justified => w,
                _ => width,
            },
            vertical: vertical.unwrap_or(fallback),
            hard_break: reason == BreakReason::Explicit,
            justified,
            runs,
        });
    }
    out
}

/// The glyphs of a run where Parley put them, and the advance Qt gives each
/// (see [`snap`]).
fn snapped(gr: &parley::GlyphRun<'_, ()>) -> (Vec<parley::Glyph>, Vec<f32>) {
    let run = gr.run();
    let placed: Vec<parley::Glyph> = gr.positioned_glyphs().collect();
    let shaped: Vec<(u32, f32)> = placed.iter().map(|g| (g.id, g.advance)).collect();
    let advances = snap::advances(run.font(), run.font_size(), &coords(run), &shaped);
    (placed, advances)
}

/// Where on its variation axes a run's font is.
fn coords(run: &parley::Run<'_, ()>) -> Vec<i16> {
    run.normalized_coords()
        .iter()
        .map(|c| c.to_bits())
        .collect()
}

/// Width of the widest line as Qt counts it, spaces at the end included.
fn snapped_width(shape: &Shape) -> f32 {
    shape
        .lines()
        .map(|line| {
            line.items()
                .filter_map(|item| match item {
                    PositionedLayoutItem::GlyphRun(gr) => Some(snapped(&gr).1.iter().sum::<f32>()),
                    _ => None,
                })
                .sum::<f32>()
        })
        .fold(0.0, f32::max)
}

/// Byte range and advance of every cluster of a shaped text.
fn cluster_advances(shape: &Shape) -> Vec<(Range<usize>, f32)> {
    let mut out = Vec::new();
    for line in shape.lines() {
        for run in line.runs() {
            let coords = coords(&run);
            for cluster in run.clusters() {
                let shaped: Vec<(u32, f32)> = cluster.glyphs().map(|g| (g.id, g.advance)).collect();
                let qt = snap::advances(run.font(), run.font_size(), &coords, &shaped);
                // What Qt's glyphs give up, taken off the cluster's own
                // advance, which also holds any spacing.
                let given_up: f32 = shaped.iter().zip(qt).map(|((_, a), q)| a - q).sum();
                out.push((cluster.text_range(), cluster.advance() - given_up));
            }
        }
    }
    out
}

/// `heights[k]`: height of the first `k + 1` lines.
///
/// Measured on Qt 6.11: lines are `advance` apart, and the last line keeps
/// what is left of its natural height, rounded down to a whole pixel.
fn heights(lines: &[RawLine], line_height: LineHeight) -> Vec<f64> {
    let mut top = 0.0;
    lines
        .iter()
        .map(|line| {
            let natural = line.vertical.natural();
            let advance = line_height.advance(natural);
            let height = top + advance + (natural - advance).floor().max(0.0);
            top += advance;
            height
        })
        .collect()
}

impl RawLine {
    fn finish(self, top: f64, width: f64) -> Line {
        let baseline = top + self.vertical.ascent;
        let base = baseline as f32;
        let runs = self
            .runs
            .into_iter()
            .map(|mut run| {
                for g in &mut run.glyphs {
                    g.y += base;
                }
                let (x0, x1) = (f64::from(run.offset), f64::from(run.offset + run.advance));
                let bar = |(above, size): (f32, f32)| {
                    let y = baseline - f64::from(above);
                    Rect::new(x0, y, x1, y + f64::from(size))
                };
                let decorations = Decorations {
                    underline: run.underline.map(bar),
                    strike_through: run.strikeout.map(bar),
                };
                // Glyphs may reach past their advance (an italic f, an
                // accent above the ascent), so the culling box has a margin.
                let margin = f64::from(run.font_size) * (0.3 + f64::from(run.skew.abs()));
                let bounds = Rect::new(
                    x0 - margin,
                    baseline - f64::from(run.ascent) - margin,
                    x1 + margin,
                    baseline + f64::from(run.descent) + margin,
                );
                LineRun {
                    glyphs: Arc::new(GlyphRun {
                        font: run.font,
                        font_size: run.font_size,
                        glyphs: run.glyphs,
                        baseline: base,
                        bounds,
                        normalized_coords: run.coords,
                        embolden: run.embolden,
                        skew: run.skew,
                    }),
                    decorations,
                }
            })
            .collect();
        Line {
            top,
            baseline,
            width,
            justified: self.justified,
            runs,
        }
    }
}

impl TextEngine {
    /// An engine with the bundled fonts only. Layouts are the same on every
    /// machine, which is what tests want.
    pub fn new() -> Self {
        Self::build(false)
    }

    /// An engine that also uses the fonts installed on the system, with the
    /// platform's interface font as the default family and the bundled font
    /// as the last resort.
    pub fn with_system_fonts() -> Self {
        Self::build(true)
    }

    fn build(system: bool) -> Self {
        Self {
            fcx: fonts::font_context(system),
            lcx: LayoutContext::new(),
            families: Families {
                system,
                default_family: None,
            },
            shapes: FrameCache::new(MAX_SHAPES),
            layouts: FrameCache::new(MAX_LAYOUTS),
            styles: HashMap::new(),
            frame: 0,
            keep_frames: KEEP_FRAMES,
            generation: 0,
            stats: CacheStats::default(),
        }
    }

    /// Add a font file (TrueType, OpenType, or a collection). Returns the
    /// family names it provides.
    pub fn register_font(&mut self, bytes: Vec<u8>) -> Result<Vec<String>, FontError> {
        let names = fonts::register(&mut self.fcx, bytes)?;
        self.fonts_changed();
        Ok(names)
    }

    /// The family used by styles that name none, ahead of the built-in
    /// defaults. An empty name removes it.
    pub fn set_default_family(&mut self, family: &str) {
        let family = family.trim();
        let new = (!family.is_empty()).then(|| Arc::from(family));
        if new != self.families.default_family {
            self.families.default_family = new;
            self.fonts_changed();
        }
    }

    /// True when a family of this name can be used.
    pub fn has_family(&mut self, name: &str) -> bool {
        self.fcx.collection.family_id(name).is_some()
    }

    /// Changes whenever the set of fonts changes; text laid out under another
    /// generation has to be laid out again.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn fonts_changed(&mut self) {
        self.generation += 1;
        self.clear_caches();
    }

    /// Drop every cached layout and shape.
    pub fn clear_caches(&mut self) {
        self.shapes.clear();
        self.layouts.clear();
        self.styles.clear();
    }

    /// How many frames a layout survives without being asked for.
    pub fn set_keep_frames(&mut self, frames: u64) {
        self.keep_frames = frames;
    }

    /// Mark the end of a frame and drop what recent frames did not use.
    ///
    /// The scene calls this from frames that run anyway. Nothing here wakes
    /// up on its own, so an idle application keeps its caches and its 0 % CPU.
    pub fn end_frame(&mut self) {
        self.frame += 1;
        self.shapes.trim(self.frame, self.keep_frames);
        self.layouts.trim(self.frame, self.keep_frames);
        if self.styles.len() > MAX_STYLES {
            self.styles.clear();
        }
    }

    /// Cache sizes and counters.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            layouts: self.layouts.len(),
            shapes: self.shapes.len(),
            ..self.stats
        }
    }

    /// Lay out `text`. Cheap when the same request was made in a recent frame.
    pub fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        params: &LayoutParams,
    ) -> Arc<TextLayout> {
        let pk = params.key();
        let key = key_hash(text, style, pk);
        if let Some(e) = self.layouts.get_mut(&key, self.frame)
            && &*e.text == text
            && e.style == *style
            && e.params == pk
        {
            self.stats.layout_hits += 1;
            return e.layout.clone();
        }
        self.stats.layout_misses += 1;
        let layout = Arc::new(self.build_layout(text, style, params));
        // A hash collision replaces the other entry: a miss later, never a
        // wrong layout, because hits compare the whole key.
        self.layouts.insert(
            key,
            LayoutEntry {
                text: text.into(),
                style: style.clone(),
                params: pk,
                layout: layout.clone(),
            },
            self.frame,
        );
        layout
    }

    /// Metrics of the font `style` resolves to.
    pub fn font_metrics(&mut self, style: &TextStyle) -> FontMetrics {
        self.style_metrics(style).font
    }

    /// Width of `text` on one line, as [`TextLayout::natural_width`].
    pub fn advance_width(&mut self, text: &str, style: &TextStyle) -> f64 {
        self.layout(text, style, &LayoutParams::default())
            .natural_width
    }

    /// `text` cut to fit `width` on one line, with an ellipsis where text was
    /// removed. Text that fits comes back unchanged; if not even the ellipsis
    /// fits, the result is empty.
    pub fn elided_text(
        &mut self,
        text: &str,
        style: &TextStyle,
        mode: ElideMode,
        width: f64,
    ) -> String {
        if mode == ElideMode::None || text.is_empty() {
            return text.to_owned();
        }
        let ellipsis = self.style_metrics(style).ellipsis;
        let rtl = self.shaped(text, style, BreakClass::Word).rtl;
        self.elide(text, style, mode, width as f32, false, rtl, ellipsis)
            .0
    }

    fn style_metrics(&mut self, style: &TextStyle) -> StyleMetrics {
        if let Some(m) = self.styles.get(style) {
            return *m;
        }
        let mut probe = self.shape_plain("x", style, BaseDirection::Ltr);
        break_lines(&mut probe, None, false);
        let x_advance = f64::from(probe.full_width());
        let run = probe.lines().flat_map(|line| line.runs()).next();
        let fm = run.as_ref().map(|run| *run.font_metrics());
        let tables = run
            .as_ref()
            .and_then(|run| ljos_render::font_tables(run.font()));
        let size = f64::from(style.pixel_size.clamp(0.0, MAX_FONT_SIZE));
        let font = match fm {
            Some(fm) => {
                let chosen = tables.map_or(shaped_vertical(&fm), |t| face::chosen(&t, size));
                let (ascent, descent) = (floor64(chosen.ascent), floor64(chosen.descent));
                let scaled = |units: f64, t: &FontTables| units * size / f64::from(t.units_per_em);
                let wide =
                    |units: fn(&FontTables) -> f64| tables.as_ref().map(|t| scaled(units(t), t));
                // A font without the tables is measured by its x instead.
                let average = tables
                    .as_ref()
                    .and_then(|t| Some(scaled(f64::from(t.os2?.x_avg_char_width), t)))
                    .unwrap_or(x_advance);
                FontMetrics {
                    ascent,
                    descent,
                    height: ascent + descent,
                    line_height: (ascent + descent).ceil(),
                    leading: round64(chosen.leading),
                    x_height: fm.x_height.map_or(ascent / 2.0, |h| round64(f64::from(h))),
                    cap_height: fm.cap_height.map_or(ascent, |h| round64(f64::from(h))),
                    underline_position: round64(f64::from(-fm.underline_offset)),
                    strikeout_position: round64(f64::from(fm.strikethrough_offset)),
                    line_width: round64(f64::from(fm.underline_size)).max(1.0),
                    average_char_width: round64(average),
                    max_char_width: wide(|t| f64::from(t.advance_width_max))
                        .unwrap_or(x_advance)
                        .round(),
                    min_left_bearing: wide(|t| f64::from(t.min_left_side_bearing)).unwrap_or(0.0),
                    min_right_bearing: wide(|t| f64::from(t.min_right_side_bearing)).unwrap_or(0.0),
                }
            }
            // No font at all can only happen if the bundled one failed to
            // load; keep lines a sensible height all the same.
            None => FontMetrics {
                ascent: size * 0.8,
                descent: size * 0.2,
                height: size,
                line_height: size.ceil(),
                x_height: size * 0.4,
                cap_height: size * 0.8,
                line_width: 1.0,
                average_char_width: size / 2.0,
                max_char_width: size.ceil(),
                ..FontMetrics::default()
            },
        };
        let mut dots = self.shape_plain(ELLIPSIS, style, BaseDirection::Ltr);
        break_lines(&mut dots, None, false);
        let metrics = StyleMetrics {
            font,
            ellipsis: snapped_width(&dots),
        };
        self.styles.insert(style.clone(), metrics);
        metrics
    }

    /// Shape without caching: probes and elide candidates are used once.
    fn shape_plain(&mut self, text: &str, style: &TextStyle, base: BaseDirection) -> Shape {
        shape(
            &mut self.fcx,
            &mut self.lcx,
            &self.families,
            text,
            style,
            BreakClass::Word,
            base,
        )
    }

    fn shaped(&mut self, text: &str, style: &TextStyle, class: BreakClass) -> &mut ShapedEntry {
        let key = key_hash(text, style, class);
        let frame = self.frame;
        let hit = self
            .shapes
            .get_mut(&key, frame)
            .is_some_and(|e| &*e.text == text && e.style == *style && e.class == class);
        if !hit {
            let mut shape = shape(
                &mut self.fcx,
                &mut self.lcx,
                &self.families,
                text,
                style,
                class,
                BaseDirection::Auto,
            );
            break_lines(&mut shape, None, false);
            self.stats.shapes_built += 1;
            let entry = ShapedEntry {
                text: text.into(),
                style: style.clone(),
                class,
                natural_width: snapped_width(&shape),
                rtl: shape.is_rtl(),
                shape,
            };
            return self.shapes.insert(key, entry, frame);
        }
        // Looked up again because the borrow checker cannot yet see that the
        // first borrow ends on a miss.
        self.shapes
            .get_mut(&key, frame)
            .expect("the entry was found a moment ago")
    }

    /// Cut `text` (one line) to `width`. `force` puts the ellipsis even when
    /// the text fits: more text follows that is not shown. Returns the new
    /// text and its shape; no shape when not even the ellipsis fits.
    #[expect(
        clippy::too_many_arguments,
        reason = "internal; a struct would only rename them"
    )]
    fn elide(
        &mut self,
        text: &str,
        style: &TextStyle,
        mode: ElideMode,
        width: f32,
        force: bool,
        rtl: bool,
        ellipsis: f32,
    ) -> (String, Option<Shape>) {
        // The ellipsis has no direction of its own: without this a cut that
        // leaves only neutral characters would flip a right-to-left line.
        let base = if rtl {
            BaseDirection::Rtl
        } else {
            BaseDirection::Ltr
        };
        let mut full = self.shape_plain(text, style, base);
        break_lines(&mut full, None, false);
        if !force && snapped_width(&full) <= width {
            return (text.to_owned(), Some(full));
        }
        if ellipsis > width {
            return (String::new(), None);
        }
        let clusters = Clusters::new(cluster_advances(&full), text.len());
        let mut keep = clusters.fit(mode, width - ellipsis);
        loop {
            let candidate = clusters.elided(text, mode, keep);
            let mut shape = self.shape_plain(&candidate, style, base);
            break_lines(&mut shape, None, false);
            // The estimate ignores kerning against the ellipsis, so check the
            // real width and give up one more cluster if it is over.
            if keep == 0 || snapped_width(&shape) <= width {
                return (candidate, Some(shape));
            }
            keep = Clusters::fewer(mode, keep);
        }
    }

    fn build_layout(&mut self, text: &str, style: &TextStyle, params: &LayoutParams) -> TextLayout {
        // Qt does not start a new line for a line break that ends the text.
        let text = text.strip_suffix('\n').unwrap_or(text);
        let metrics = self.style_metrics(style);
        let fallback = Vertical {
            ascent: metrics.font.ascent,
            descent: metrics.font.descent,
        };
        let width = params.usable_width();
        let (wrap, elide) = match width {
            Some(_) => (params.wrap, params.elide),
            None => (WrapMode::NoWrap, ElideMode::None),
        };
        let wraps = wrap != WrapMode::NoWrap;
        let justify = params.justify && wraps;
        let max_advance = width.filter(|_| wraps).map(|w| w as f32);

        let (mut raw, natural, rtl) = if text.is_empty() {
            (Vec::new(), 0.0, false)
        } else {
            let entry = self.shaped(text, style, BreakClass::of(wrap));
            // Parley breaks lines by the shaper's advances, which are a
            // touch wider than Qt's: a text that fits as Qt counts it (an
            // item as wide as its own text) must not be broken.
            let max_advance = max_advance.filter(|w| entry.natural_width > *w);
            break_lines(&mut entry.shape, max_advance, justify);
            (
                extract(&entry.shape, fallback, max_advance.filter(|_| justify)),
                entry.natural_width,
                entry.rtl,
            )
        };
        if raw.is_empty() {
            raw.push(RawLine::empty(fallback));
        }

        let total = raw.len();
        let by_count = match params.max_lines {
            0 => total,
            n => total.min(n as usize),
        };
        let line_heights = heights(&raw[..by_count], params.line_height);
        let implicit_height = line_heights[by_count - 1];
        let mut visible = by_count;
        if let Some(limit) = params.usable_height() {
            // At least one line shows, however low the item is.
            visible = line_heights
                .iter()
                .take_while(|h| **h <= limit)
                .count()
                .max(1);
        }

        let mut truncated = visible < total;
        if let Some(w) = width {
            let w32 = w as f32;
            if truncated && elide == ElideMode::Right {
                // The last shown line takes the rest of its paragraph and
                // always ends in an ellipsis, because text follows it.
                let start = raw[visible - 1].range.start.min(text.len());
                let end = raw[visible - 1..]
                    .iter()
                    .find(|l| l.hard_break)
                    .map_or(text.len(), |l| l.range.end.min(text.len()));
                let rest = text
                    .get(start..end.max(start))
                    .unwrap_or("")
                    .trim_end_matches(['\n', '\r']);
                raw[visible - 1] = self.elided_line(rest, style, elide, w32, true, rtl, &metrics);
            } else if !truncated
                && total == 1
                && !wraps
                && elide != ElideMode::None
                && raw[0].width > w32
            {
                raw[0] = self.elided_line(text, style, elide, w32, false, rtl, &metrics);
                truncated = true;
            }
        }
        raw.truncate(visible);

        let limit = width.filter(|_| wraps || elide != ElideMode::None);
        let mut lines = Vec::with_capacity(raw.len());
        let (mut top, mut content_height, mut content_width) = (0.0, 0.0, 0.0_f64);
        for line in raw {
            let natural = line.vertical.natural();
            let advance = params.line_height.advance(natural);
            content_height = top + advance + (natural - advance).floor().max(0.0);
            let width = reported_width(line.width, limit);
            content_width = content_width.max(width);
            lines.push(line.finish(top, width));
            top += advance;
        }
        TextLayout {
            baseline: lines.first().map_or(0.0, |l| l.baseline),
            lines,
            natural_width: ceil64(natural),
            content_width,
            content_height,
            implicit_height,
            truncated,
            rtl,
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "internal; a struct would only rename them"
    )]
    fn elided_line(
        &mut self,
        text: &str,
        style: &TextStyle,
        mode: ElideMode,
        width: f32,
        force: bool,
        rtl: bool,
        metrics: &StyleMetrics,
    ) -> RawLine {
        let fallback = Vertical {
            ascent: metrics.font.ascent,
            descent: metrics.font.descent,
        };
        let (_, shape) = self.elide(text, style, mode, width, force, rtl, metrics.ellipsis);
        shape
            .and_then(|shape| extract(&shape, fallback, None).into_iter().next())
            .unwrap_or_else(|| RawLine::empty(fallback))
    }
}

/// The width a line reports: rounded up to 1/64 pixel, so an item sized to
/// its text never wraps or elides it, but never beyond a limit the line fits.
fn reported_width(width: f32, limit: Option<f64>) -> f64 {
    let reported = ceil64(width);
    match limit {
        Some(limit) if reported > limit && f64::from(width) <= limit + 1e-4 => limit,
        _ => reported,
    }
}
