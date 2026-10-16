//! The engine: fonts, shaping, line breaking, and the two caches.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use ljos_render::kurbo::Rect;
use ljos_render::{Decorations, FontTables, GlyphRun, PositionedGlyph};
use parley::fontique::GenericFamily;
use parley::{
    Alignment, AlignmentOptions, BaseDirection, BreakReason, FontContext, FontData, FontFamily,
    FontFamilyName, FontStyle, FontWeight, InlineBox, InlineBoxKind, LayoutContext, OverflowWrap,
    PositionedLayoutItem, StyleProperty, WordBreak,
};
use rustc_hash::{FxHashMap, FxHasher};

use crate::cache::FrameCache;
use crate::edit::CaretCluster;
use crate::elide::{Clusters, ELLIPSIS};
use crate::face;
use crate::fonts::{self, FontError};
use crate::layout::{FontMetrics, Line, LineRun, PlacedBox, TextLayout};
use crate::snap;
use crate::style::{
    BoxAlign, ElideMode, InlineObject, LayoutParams, LineHeight, ParamsKey, Ranged, StyleSpan,
    TextStyle, WrapMode,
};
use crate::substitute;

/// A layout unused for this many frames is dropped.
const KEEP_FRAMES: u64 = 30;
/// Upper bounds, for a frame that lays out a great many different strings.
const MAX_LAYOUTS: usize = 2048;
const MAX_SHAPES: usize = 1024;
/// Paragraphs of edited text laid out on their own ([`edit`]).
const MAX_PARAGRAPHS: usize = 4096;
const MAX_STYLES: usize = 256;
/// Font sizes beyond this are a mistake in a binding, not a design.
const MAX_FONT_SIZE: f32 = 4096.0;
/// Mixed into the key of a ranged layout, so it never answers for a plain
/// one of the same text.
const RANGED: u8 = 1;

mod edit;

/// The brush is the tag of the [`StyleSpan`] a glyph belongs to, so Parley
/// starts a new glyph run wherever the caller's parts change, also where only
/// their colour does (which the engine never sees).
type Shape = parley::Layout<u32>;

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
    ranged: OwnedRanged,
    shape: Shape,
    natural_width: f32,
    rtl: bool,
}

struct LayoutEntry {
    text: Box<str>,
    style: TextStyle,
    params: ParamsKey,
    /// `None` for plain text, which keeps no character places.
    ranged: Option<OwnedRanged>,
    layout: Arc<TextLayout>,
}

/// The spans and objects of a request, kept with a cache entry so a hit can
/// compare the whole key.
#[derive(Clone, Debug, Default, PartialEq)]
struct OwnedRanged {
    spans: Box<[StyleSpan]>,
    objects: Box<[InlineObject]>,
    strut: bool,
}

impl OwnedRanged {
    fn of(r: &Ranged<'_>) -> Self {
        Self {
            spans: r.spans.into(),
            objects: r.objects.into(),
            strut: r.strut,
        }
    }

    fn matches(&self, r: &Ranged<'_>) -> bool {
        *self.spans == *r.spans && *self.objects == *r.objects && self.strut == r.strut
    }
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

    fn max(self, other: Self) -> Self {
        Self {
            ascent: self.ascent.max(other.ascent),
            descent: self.descent.max(other.descent),
        }
    }

    /// The line with room for an object `height` tall. Measured on Qt 6.11:
    /// an object stands on the bottom of the line (not on the baseline),
    /// hangs from its top, or is centred on it.
    fn with_box(self, height: f64, align: BoxAlign) -> Self {
        let extra = (height - (self.ascent + self.descent)).max(0.0);
        match align {
            BoxAlign::Bottom => Self {
                ascent: self.ascent + extra,
                ..self
            },
            BoxAlign::Top => Self {
                descent: self.descent + extra,
                ..self
            },
            BoxAlign::Middle => Self {
                ascent: self.ascent + extra / 2.0,
                descent: self.descent + extra / 2.0,
            },
            BoxAlign::Baseline => Self {
                ascent: self.ascent.max(height),
                ..self
            },
        }
    }
}

#[derive(Clone)]
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
    /// How thick the font's lines are.
    line_size: f32,
    /// The tag of the span the glyphs belong to; 0 outside every span.
    tag: u32,
}

/// An inline object where Parley put it on its line.
#[derive(Clone)]
struct RawBox {
    id: u64,
    /// From the line's left edge.
    x: f32,
    width: f32,
    height: f32,
    align: BoxAlign,
}

/// A line as Parley broke it, before it has a place in the layout.
#[derive(Clone)]
struct RawLine {
    range: Range<usize>,
    width: f32,
    vertical: Vertical,
    /// Ends in a line break that is in the text.
    hard_break: bool,
    justified: bool,
    runs: Vec<RawRun>,
    boxes: Vec<RawBox>,
    /// Where the characters are; only filled for ranged layouts.
    clusters: Vec<CaretCluster>,
    /// The line has text of its own (a line that only breaks takes the
    /// height of the style its line break is in).
    has_glyphs: bool,
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
            boxes: Vec::new(),
            clusters: Vec::new(),
            has_glyphs: false,
        }
    }
}

/// Which fonts a style resolves to.
struct Families {
    system: bool,
    default_family: Option<Arc<str>>,
    /// What the platform puts in place of each named family asked for so
    /// far, when it is another family (system fonts only).
    substitutes: RefCell<HashMap<Box<str>, Option<Arc<str>>>>,
}

impl Families {
    /// The family list for a style: what it names, then the engine's
    /// defaults, so a misspelt or missing family still shows text.
    fn stack<'a>(&'a self, style: &'a TextStyle) -> Vec<FontFamilyName<'a>> {
        self.stack_of(&style.family)
    }

    fn stack_of<'a>(&'a self, family: &'a str) -> Vec<FontFamilyName<'a>> {
        let mut names = Vec::new();
        let mut add = |list: &'a str| {
            for part in list.split(',') {
                let name = part.trim().trim_matches(['"', '\'']);
                if name.is_empty() {
                    continue;
                }
                match GenericFamily::parse(name) {
                    Some(generic) => names.push(FontFamilyName::Generic(generic)),
                    None => {
                        names.push(FontFamilyName::Named(Cow::Borrowed(name)));
                        if let Some(sub) = self.substitute(name) {
                            names.push(FontFamilyName::Named(Cow::Owned(sub.to_string())));
                        }
                    }
                }
            }
        };
        add(family);
        if let Some(family) = &self.default_family {
            add(family);
        }
        if self.system {
            names.push(FontFamilyName::Generic(GenericFamily::SystemUi));
        }
        names.push(FontFamilyName::Generic(GenericFamily::SansSerif));
        names
    }

    /// The family the platform's rules name for `name`, as Qt asks the
    /// platform; nothing with the bundled fonts alone.
    fn substitute(&self, name: &str) -> Option<Arc<str>> {
        if !self.system {
            return None;
        }
        self.substitutes
            .borrow_mut()
            .entry(name.into())
            .or_insert_with(|| substitute::platform_substitute(name).map(Arc::from))
            .clone()
    }
}

/// Lays out text. One per scene, or one shared by several scenes.
pub struct TextEngine {
    fcx: FontContext,
    lcx: LayoutContext<u32>,
    families: Families,
    shapes: FrameCache<u64, ShapedEntry>,
    layouts: FrameCache<u64, LayoutEntry>,
    paragraphs: FrameCache<u64, edit::ParagraphEntry>,
    styles: FxHashMap<TextStyle, StyleMetrics>,
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
    // Fixed keys, so cache behaviour is reproducible; a hit is checked
    // against the whole key, so a collision costs only a miss.
    let mut h = FxHasher::default();
    text.hash(&mut h);
    style.hash(&mut h);
    extra.hash(&mut h);
    h.finish()
}

/// A font size the shaper can take.
fn usable_size(size: f32) -> f32 {
    if size.is_finite() {
        size.clamp(0.0, MAX_FONT_SIZE)
    } else {
        0.0
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "internal; a struct would only rename them"
)]
fn shape(
    fcx: &mut FontContext,
    lcx: &mut LayoutContext<u32>,
    families: &Families,
    text: &str,
    style: &TextStyle,
    ranged: &Ranged<'_>,
    class: BreakClass,
    base: BaseDirection,
) -> Shape {
    let names = families.stack(style);
    // Resolved before the builder borrows the contexts.
    let span_names: Vec<Option<Vec<FontFamilyName<'_>>>> = ranged
        .spans
        .iter()
        .map(|s| s.family.as_deref().map(|f| families.stack_of(f)))
        .collect();
    let size = usable_size(style.pixel_size);
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
    for (span, names) in ranged.spans.iter().zip(&span_names) {
        // A span that does not fit the text is a caller's mistake that must
        // not take the application down: it is left out.
        let range = span.range.clone();
        if range.start >= range.end
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            continue;
        }
        if let Some(names) = names {
            b.push(
                StyleProperty::FontFamily(FontFamily::List(Cow::Borrowed(names))),
                range.clone(),
            );
        }
        b.push(
            StyleProperty::FontSize(usable_size(span.pixel_size)),
            range.clone(),
        );
        b.push(
            StyleProperty::FontWeight(FontWeight::new(finite(span.weight).clamp(1.0, 1000.0))),
            range.clone(),
        );
        b.push(
            StyleProperty::FontStyle(if span.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            }),
            range.clone(),
        );
        b.push(StyleProperty::Underline(span.underline), range.clone());
        b.push(StyleProperty::Strikethrough(span.strikeout), range.clone());
        b.push(StyleProperty::Brush(span.tag), range);
    }
    for object in ranged.objects {
        if object.index > text.len() || !text.is_char_boundary(object.index) {
            continue;
        }
        let side = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 16384.0)
            } else {
                0.0
            }
        };
        b.push_inline_box(InlineBox {
            id: object.id,
            kind: InlineBoxKind::InFlow,
            index: object.index,
            width: side(object.width),
            height: side(object.height),
            baseline: None,
            vertical_align: Default::default(),
        });
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

fn extract(
    shape: &Shape,
    fallback: Vertical,
    justify_width: Option<f32>,
    objects: &[InlineObject],
) -> Vec<RawLine> {
    let mut out = Vec::with_capacity(shape.len());
    for line in shape.lines() {
        let m = *line.metrics();
        let reason = line.break_reason();
        let wrapped = matches!(reason, BreakReason::Regular | BreakReason::Emergency);
        let mut runs = Vec::new();
        let mut boxes = Vec::new();
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
            let gr = match item {
                PositionedLayoutItem::GlyphRun(gr) => gr,
                PositionedLayoutItem::InlineBox(placed) => {
                    let align = objects
                        .iter()
                        .find(|o| o.id == placed.id)
                        .map_or(BoxAlign::Bottom, |o| o.align);
                    boxes.push(RawBox {
                        id: placed.id,
                        x: placed.x - shift,
                        width: placed.width,
                        height: placed.height,
                        align,
                    });
                    if !justified {
                        width += placed.width;
                    }
                    continue;
                }
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
                line_size: fm.underline_size,
                tag: style.brush,
            });
        }
        let has_glyphs = !runs.is_empty() || !boxes.is_empty();
        // Measured on Qt 6.11: a line that is only objects standing on the
        // baseline is as tall as they are, without room for descenders.
        let only_standing = runs.is_empty()
            && !boxes.is_empty()
            && boxes.iter().all(|b| b.align == BoxAlign::Baseline);
        let mut vertical = if only_standing {
            Vertical {
                ascent: 0.0,
                descent: 0.0,
            }
        } else {
            vertical.unwrap_or(fallback)
        };
        for b in &boxes {
            vertical = vertical.with_box(f64::from(b.height), b.align);
        }
        out.push(RawLine {
            range: line.text_range(),
            width: match justify_width {
                Some(w) if justified => w,
                _ => width,
            },
            vertical,
            hard_break: reason == BreakReason::Explicit,
            justified,
            runs,
            boxes,
            clusters: Vec::new(),
            has_glyphs,
        });
    }
    out
}

/// The glyphs of a run where Parley put them, and the advance Qt gives each
/// (see [`snap`]).
fn snapped(gr: &parley::GlyphRun<'_, u32>) -> (Vec<parley::Glyph>, Vec<f32>) {
    let run = gr.run();
    let placed: Vec<parley::Glyph> = gr.positioned_glyphs().collect();
    let shaped: Vec<(u32, f32)> = placed.iter().map(|g| (g.id, g.advance)).collect();
    let advances = snap::advances(run.font(), run.font_size(), &coords(run), &shaped);
    (placed, advances)
}

/// Where on its variation axes a run's font is.
fn coords(run: &parley::Run<'_, u32>) -> Vec<i16> {
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
                .map(|item| match item {
                    PositionedLayoutItem::GlyphRun(gr) => snapped(&gr).1.iter().sum::<f32>(),
                    PositionedLayoutItem::InlineBox(placed) => placed.width,
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
        let natural = self.vertical.natural();
        let boxes = self
            .boxes
            .iter()
            .map(|b| {
                let (w, h) = (f64::from(b.width), f64::from(b.height));
                let y = match b.align {
                    BoxAlign::Top => top,
                    BoxAlign::Middle => top + (natural - h) / 2.0,
                    BoxAlign::Bottom => top + natural - h,
                    BoxAlign::Baseline => baseline - h,
                };
                let x = f64::from(b.x);
                PlacedBox {
                    id: b.id,
                    rect: Rect::new(x, y, x + w, y + h),
                }
            })
            .collect();
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
                // Measured on Qt 6.11: an overline starts on the pixel row
                // the font's ascent starts in, which for the line's tallest
                // font is the line's first.
                let over = (baseline - f64::from(run.ascent)).round().max(top);
                let overline = Rect::new(x0, over, x1, over + f64::from(run.line_size.max(1.0)));
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
                    overline,
                    tag: run.tag,
                }
            })
            .collect();
        Line {
            top,
            baseline,
            width,
            justified: self.justified,
            runs,
            range: self.range,
            height: natural,
            clusters: self.clusters,
            boxes,
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
                substitutes: RefCell::default(),
            },
            shapes: FrameCache::new(MAX_SHAPES),
            layouts: FrameCache::new(MAX_LAYOUTS),
            paragraphs: FrameCache::new(MAX_PARAGRAPHS),
            styles: FxHashMap::default(),
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

    /// [`TextEngine::register_font`], saying what each face of the file is
    /// (for `FontLoader.font`).
    pub fn register_font_faces(
        &mut self,
        bytes: Vec<u8>,
    ) -> Result<Vec<fonts::RegisteredFace>, FontError> {
        let faces = fonts::register_faces(&mut self.fcx, bytes)?;
        self.fonts_changed();
        Ok(faces)
    }

    /// Every family a style can name, sorted, once each: the bundled ones,
    /// the registered ones and the system's where the engine reads them
    /// (`Qt.fontFamilies()`).
    pub fn family_names(&mut self) -> Vec<String> {
        let mut names: Vec<String> = self
            .fcx
            .collection
            .family_names()
            .map(str::to_owned)
            .collect();
        names.sort();
        names.dedup();
        names
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
        self.paragraphs.clear();
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
        self.paragraphs.trim(self.frame, self.keep_frames);
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
            && e.ranged.is_none()
        {
            self.stats.layout_hits += 1;
            return e.layout.clone();
        }
        self.stats.layout_misses += 1;
        let layout = Arc::new(self.build_layout(text, style, None, params));
        // A hash collision replaces the other entry: a miss later, never a
        // wrong layout, because hits compare the whole key.
        self.layouts.insert(
            key,
            LayoutEntry {
                text: text.into(),
                style: style.clone(),
                params: pk,
                ranged: None,
                layout: layout.clone(),
            },
            self.frame,
        );
        layout
    }

    /// Lay out `text` whose parts differ in font, with objects standing in
    /// its lines. Unlike [`Self::layout`] the result knows where every
    /// character is ([`Line::clusters`]), so a point can be traced back to
    /// the text, and every glyph run says which span it draws
    /// ([`LineRun::tag`]). Cached like a plain layout.
    pub fn layout_ranged(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        params: &LayoutParams,
    ) -> Arc<TextLayout> {
        let pk = params.key();
        let key = key_hash(
            text,
            style,
            (pk, ranged.spans, ranged.objects, ranged.strut, RANGED),
        );
        if let Some(e) = self.layouts.get_mut(&key, self.frame)
            && &*e.text == text
            && e.style == *style
            && e.params == pk
            && e.ranged.as_ref().is_some_and(|r| r.matches(ranged))
        {
            self.stats.layout_hits += 1;
            return e.layout.clone();
        }
        self.stats.layout_misses += 1;
        let layout = Arc::new(self.build_layout(text, style, Some(ranged), params));
        self.layouts.insert(
            key,
            LayoutEntry {
                text: text.into(),
                style: style.clone(),
                params: pk,
                ranged: Some(OwnedRanged::of(ranged)),
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
        let rtl = self
            .shaped(text, style, &Ranged::NONE, BreakClass::Word)
            .rtl;
        self.elide(
            text,
            style,
            &Ranged::NONE,
            mode,
            width as f32,
            false,
            rtl,
            ellipsis,
        )
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
        self.shape_once(text, style, &Ranged::NONE, base)
    }

    fn shape_once(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        base: BaseDirection,
    ) -> Shape {
        shape(
            &mut self.fcx,
            &mut self.lcx,
            &self.families,
            text,
            style,
            ranged,
            BreakClass::Word,
            base,
        )
    }

    fn shaped(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        class: BreakClass,
    ) -> &mut ShapedEntry {
        let key = key_hash(text, style, (class, ranged.spans, ranged.objects));
        let frame = self.frame;
        let hit = self.shapes.get_mut(&key, frame).is_some_and(|e| {
            &*e.text == text && e.style == *style && e.class == class && e.ranged.matches(ranged)
        });
        if !hit {
            let mut shape = shape(
                &mut self.fcx,
                &mut self.lcx,
                &self.families,
                text,
                style,
                ranged,
                class,
                BaseDirection::Auto,
            );
            break_lines(&mut shape, None, false);
            self.stats.shapes_built += 1;
            let entry = ShapedEntry {
                text: text.into(),
                style: style.clone(),
                class,
                ranged: OwnedRanged::of(ranged),
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
    /// text, its shape (none when not even the ellipsis fits), and how many
    /// bytes at its start are the text's own.
    #[expect(
        clippy::too_many_arguments,
        reason = "internal; a struct would only rename them"
    )]
    fn elide(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        mode: ElideMode,
        width: f32,
        force: bool,
        rtl: bool,
        ellipsis: f32,
    ) -> (String, Option<Shape>, usize) {
        // The ellipsis has no direction of its own: without this a cut that
        // leaves only neutral characters would flip a right-to-left line.
        let base = if rtl {
            BaseDirection::Rtl
        } else {
            BaseDirection::Ltr
        };
        let mut full = self.shape_once(text, style, ranged, base);
        break_lines(&mut full, None, false);
        if !force && snapped_width(&full) <= width {
            return (text.to_owned(), Some(full), text.len());
        }
        if ellipsis > width {
            return (String::new(), None, 0);
        }
        let clusters = Clusters::new(cluster_advances(&full), text.len());
        let mut keep = clusters.fit(mode, width - ellipsis);
        loop {
            let candidate = clusters.elided(text, mode, keep);
            let head = clusters.head_end(mode, keep);
            let cut = cut_at(ranged, head);
            let mut shape = self.shape_once(&candidate, style, &cut.borrowed(), base);
            break_lines(&mut shape, None, false);
            // The estimate ignores kerning against the ellipsis, so check the
            // real width and give up one more cluster if it is over.
            if keep == 0 || snapped_width(&shape) <= width {
                return (candidate, Some(shape), head);
            }
            keep = Clusters::fewer(mode, keep);
        }
    }

    fn build_layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: Option<&Ranged<'_>>,
        params: &LayoutParams,
    ) -> TextLayout {
        // Qt does not start a new line for a line break that ends the text
        // (but see `implicit_height` below).
        let ends_in_break = text.ends_with('\n');
        let text = text.strip_suffix('\n').unwrap_or(text);
        // Only a ranged layout pays for knowing where its characters are.
        let places = ranged.is_some();
        let ranged = ranged.copied().unwrap_or(Ranged::NONE);
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

        let (mut raw, natural, rtl) = if text.is_empty() && ranged.objects.is_empty() {
            (Vec::new(), 0.0, false)
        } else {
            let entry = self.shaped(text, style, &ranged, BreakClass::of(wrap));
            // Parley breaks lines by the shaper's advances, which are a
            // touch wider than Qt's: a text that fits as Qt counts it (an
            // item as wide as its own text) must not be broken.
            let max_advance = max_advance.filter(|w| entry.natural_width > *w);
            break_lines(&mut entry.shape, max_advance, justify);
            let mut lines = extract(
                &entry.shape,
                fallback,
                max_advance.filter(|_| justify),
                ranged.objects,
            );
            if places {
                for (line, carets) in lines.iter_mut().zip(edit::carets(&entry.shape, text)) {
                    line.clusters = carets.clusters;
                }
            }
            (lines, entry.natural_width, entry.rtl)
        };
        if raw.is_empty() {
            raw.push(RawLine::empty(fallback));
        }
        if !ranged.spans.is_empty() {
            self.break_heights(&mut raw, text, style, ranged.spans);
        }
        if ranged.strut {
            for line in &mut raw {
                line.vertical = line.vertical.max(fallback);
            }
        }

        let total = raw.len();
        let by_count = match params.max_lines {
            0 => total,
            n => total.min(n as usize),
        };
        let line_heights = heights(&raw[..by_count], params.line_height);
        let mut implicit_height = line_heights[by_count - 1];
        if ends_in_break && by_count == total {
            // Measured on Qt 6.11: the empty line after a line break that
            // ends the text is neither shown nor counted, but the text wants
            // its height (`abc\n` and `<p>abc</p>` have an implicit height of
            // two lines and a content height of one). It is a line of the
            // text's own font, whatever the line before it is set in.
            let top: f64 = raw
                .iter()
                .map(|l| params.line_height.advance(l.vertical.natural()))
                .sum();
            let natural = fallback.natural();
            let advance = params.line_height.advance(natural);
            implicit_height = top + advance + (natural - advance).floor().max(0.0);
        }
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
                raw[visible - 1] = self.elided_line(
                    rest,
                    start,
                    style,
                    &ranged,
                    places,
                    (elide, w32, true),
                    rtl,
                    &metrics,
                );
            } else if !truncated
                && total == 1
                && !wraps
                && elide != ElideMode::None
                && raw[0].width > w32
            {
                raw[0] = self.elided_line(
                    text,
                    0,
                    style,
                    &ranged,
                    places,
                    (elide, w32, false),
                    rtl,
                    &metrics,
                );
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

    /// A line that ends in a line break is at least as tall as the style its
    /// line break is in, and exactly that tall when it has nothing else
    /// (measured on Qt 6.11: in `abc<h1>def</h1>` the break before the
    /// heading belongs to the heading and makes the first line as tall).
    fn break_heights(
        &mut self,
        lines: &mut [RawLine],
        text: &str,
        style: &TextStyle,
        spans: &[StyleSpan],
    ) {
        for line in lines.iter_mut().filter(|l| l.hard_break) {
            let Some(at) = line.range.end.checked_sub(1) else {
                continue;
            };
            if text.as_bytes().get(at) != Some(&b'\n') {
                continue;
            }
            // A break outside every span is in the text's own style, which
            // still counts on a line whose glyphs are all smaller.
            let of_break = spans
                .iter()
                .find(|s| s.range.contains(&at))
                .map_or_else(|| style.clone(), |s| s.style(style));
            let font = self.style_metrics(&of_break).font;
            let own = Vertical {
                ascent: font.ascent,
                descent: font.descent,
            };
            line.vertical = if line.has_glyphs {
                line.vertical.max(own)
            } else {
                own
            };
        }
    }

    /// One line of `text` cut to a width. `at` is where `text` starts in the
    /// whole text, which is what `ranged` and the character places count in.
    #[expect(
        clippy::too_many_arguments,
        reason = "internal; a struct would only rename them"
    )]
    fn elided_line(
        &mut self,
        text: &str,
        at: usize,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        places: bool,
        (mode, width, force): (ElideMode, f32, bool),
        rtl: bool,
        metrics: &StyleMetrics,
    ) -> RawLine {
        let fallback = Vertical {
            ascent: metrics.font.ascent,
            descent: metrics.font.descent,
        };
        let part = part_of(ranged, at..at + text.len());
        let (shown, shape, own) = self.elide(
            text,
            style,
            &part.borrowed(),
            mode,
            width,
            force,
            rtl,
            metrics.ellipsis,
        );
        let Some(shape) = shape else {
            return RawLine::empty(fallback);
        };
        let Some(mut line) = extract(&shape, fallback, None, &part.objects)
            .into_iter()
            .next()
        else {
            return RawLine::empty(fallback);
        };
        if places {
            // Only what stands before the cut is still where the text says.
            line.clusters = edit::carets(&shape, &shown)
                .into_iter()
                .next()
                .map(|c| c.clusters)
                .unwrap_or_default();
            line.clusters.retain(|c| c.range.end <= own);
            for c in &mut line.clusters {
                c.range = c.range.start + at..c.range.end + at;
            }
        }
        line.range = at..at + text.len();
        if ranged.strut {
            line.vertical = line.vertical.max(fallback);
        }
        line
    }
}

/// The spans and objects of `range`, counted from its start.
fn part_of(ranged: &Ranged<'_>, range: Range<usize>) -> OwnedRanged {
    let spans = ranged
        .spans
        .iter()
        .filter(|s| s.range.start < range.end && s.range.end > range.start)
        .map(|s| StyleSpan {
            range: s.range.start.max(range.start) - range.start
                ..s.range.end.min(range.end) - range.start,
            ..s.clone()
        })
        .collect();
    let objects = ranged
        .objects
        .iter()
        .filter(|o| range.contains(&o.index))
        .map(|o| InlineObject {
            index: o.index - range.start,
            ..*o
        })
        .collect();
    OwnedRanged {
        spans,
        objects,
        strut: ranged.strut,
    }
}

/// The spans and objects that survive a cut after `head` bytes. A span the
/// cut goes through also takes the ellipsis, which follows it; what comes
/// after the ellipsis is in the text's own style.
fn cut_at(ranged: &Ranged<'_>, head: usize) -> OwnedRanged {
    let spans = ranged
        .spans
        .iter()
        .filter(|s| s.range.start < head)
        .map(|s| StyleSpan {
            range: s.range.start..if s.range.end >= head {
                head + ELLIPSIS.len()
            } else {
                s.range.end
            },
            ..s.clone()
        })
        .collect();
    let objects = ranged
        .objects
        .iter()
        .filter(|o| o.index < head)
        .copied()
        .collect();
    OwnedRanged {
        spans,
        objects,
        strut: ranged.strut,
    }
}

impl OwnedRanged {
    fn borrowed(&self) -> Ranged<'_> {
        Ranged {
            spans: &self.spans,
            objects: &self.objects,
            strut: self.strut,
        }
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
