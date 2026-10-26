//! The engine: fonts, shaping, line breaking, and the two caches.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use ljos_render::kurbo::Rect;
use ljos_render::{Decorations, FontTables, GlyphRun, PositionedGlyph};
use ljos_runtime::Platform;
use parley::fontique::{Blob, CharmapIndex, FontWidth, GenericFamily, SourceInfo, Synthesis};
use parley::setting::Tag;
use parley::{
    Alignment, AlignmentOptions, BaseDirection, BreakReason, FontContext, FontData, FontFamily,
    FontFamilyName, FontStyle, FontVariation, FontVariations, FontWeight, InlineBox, InlineBoxKind,
    LayoutContext, OverflowWrap, PositionedLayoutItem, StyleProperty, TextStyle as ParleyStyle,
    WordBreak,
};
use parley_engine::itemize::Segment;
use parley_engine::shape::CharCluster;
use parley_engine::{
    Analysis, AnalysisOptions, Analyzer, FontInstanceRef, FontSelector, LineBreakConfig,
    ShapeOptions, ShapedText, Shaper,
};
use rustc_hash::{FxHashMap, FxHasher};

use crate::cache::FrameCache;
use crate::edit::CaretCluster;
use crate::elide::{Clusters, ELLIPSIS};
use crate::face;
use crate::fonts::{self, FontError};
use crate::layout::{FontMetrics, Line, LineRun, PlacedBox, TextLayout};
use crate::measure;
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

/// About what a shaped text holds per byte of the text: Parley keeps a
/// cluster, a glyph and what it found of the character for each, and
/// says nothing of its size.
const SHAPED_BYTES_PER_TEXT_BYTE: usize = 56;

/// What the text engine keeps, for a tool that shows where memory went.
/// Counts are exact; `bytes` is an estimate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextMemory {
    /// Layouts held.
    pub layouts: usize,
    /// Shaped texts held.
    pub shapes: usize,
    /// Paragraphs of edited text held.
    pub paragraphs: usize,
    /// Styles whose metrics are kept.
    pub styles: usize,
    /// One-line runs kept for being shaped again.
    pub runs: usize,
    /// Glyphs whose advance or ink this thread keeps, for every engine on it.
    pub glyphs: usize,
    /// About the bytes of all of it.
    pub bytes: usize,
}

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
    /// Glyphs whose advance or ink was read from a font, by every engine on
    /// this thread: each is read once and then kept.
    pub glyphs_measured: u64,
    /// Texts shaped with only the family every character was found in, in
    /// place of the whole family stack.
    pub shapes_pinned: u64,
    /// Of those, the ones whose run was kept from shaping the same text in
    /// the same font before, and not shaped again.
    pub runs_reused: u64,
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
    /// `None` for a text shaped without Parley's builder ([`shape_direct`])
    /// until a layout needs Parley's lines: to break them, or for carets.
    shape: Option<Shape>,
    /// The width and justification `shape` is broken at now: a layout
    /// that asks the same again does not break it again.
    broken: (Option<f32>, bool),
    /// The widest line by its advances alone.
    advance: f32,
    natural_width: f32,
    rtl: bool,
    /// Every glyph of `shape` broken without a width, with Qt's advance,
    /// as the widths above were counted: a layout that does not break it
    /// again takes them in place of reading and counting them once more.
    advances: Snapped,
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
    fn bytes(&self) -> usize {
        self.spans.len() * std::mem::size_of::<StyleSpan>()
            + self.objects.len() * std::mem::size_of::<InlineObject>()
    }
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
    fn bytes(&self) -> usize {
        let runs: usize = self
            .runs
            .iter()
            .map(|r| {
                r.glyphs.capacity() * std::mem::size_of::<PositionedGlyph>()
                    + r.coords.capacity() * 2
            })
            .sum();
        self.runs.capacity() * std::mem::size_of::<RawRun>()
            + runs
            + self.boxes.capacity() * std::mem::size_of::<RawBox>()
    }
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
    /// The first font the stack of a family and style yields, by family
    /// list; `None` when there is none to pin. Cleared when fonts change.
    first_fonts: RefCell<FxHashMap<Box<str>, Vec<FirstFontEntry>>>,
    /// Texts shaped with a pinned family (`CacheStats::shapes_pinned`).
    pinned: Cell<u64>,
    /// Off only to compare against the whole stack in tests.
    pin: bool,
    /// A text without spans or objects given to Parley as one style; off
    /// only to compare against the style built a property at a time.
    one_style: bool,
}

/// A style's weight bits and italic, and the first font of its stack.
type FirstFontEntry = (u32, bool, Option<FirstFont>);

/// The font a query over a family stack tries first, and its family.
struct FirstFont {
    family: Arc<str>,
    blob: Blob<u8>,
    source: SourceInfo,
    /// In a collection, and the synthesis the style asks of it.
    index: u32,
    synthesis: Synthesis,
    charmap: CharmapIndex,
    /// Of the characters below [`PINNED_BELOW`], a bit for each one looked
    /// up in the character map, and one for each found there: a delegate's
    /// text is checked on every layout, and a lookup costs some twenty
    /// times a bit.
    seen: Box<[u64; PINNED_BELOW / 64]>,
    has: Box<[u64; PINNED_BELOW / 64]>,
}

/// A family pinned for a text ([`Families::pinned`]), and the font Parley
/// takes from it for every character.
struct PinnedFont {
    family: Arc<str>,
    font: FontData,
    /// The file `font` was read from, for the source cache.
    source: SourceInfo,
    synthesis: Synthesis,
}

/// Texts with characters from here up are never pinned.
const PINNED_BELOW: usize = 0x2000;

impl FirstFont {
    fn new(
        family: Arc<str>,
        blob: Blob<u8>,
        source: SourceInfo,
        index: u32,
        synthesis: Synthesis,
        charmap: CharmapIndex,
    ) -> Self {
        Self {
            family,
            blob,
            source,
            index,
            synthesis,
            charmap,
            seen: Box::new([0; PINNED_BELOW / 64]),
            has: Box::new([0; PINNED_BELOW / 64]),
        }
    }

    /// Whether the font has a glyph for every character of `text`, all
    /// below [`PINNED_BELOW`].
    fn covers(&mut self, text: &str) -> bool {
        let mut charmap = None;
        for c in text.chars() {
            let at = c as usize;
            let (word, bit) = (at / 64, 1_u64 << (at % 64));
            if self.seen[word] & bit == 0 {
                if charmap.is_none() {
                    charmap = Some(self.charmap.charmap(self.blob.as_ref()));
                }
                let Some(Some(map)) = &charmap else {
                    return false;
                };
                if map.map(c).is_some_and(|g| g != 0) {
                    self.has[word] |= bit;
                }
                self.seen[word] |= bit;
            }
            if self.has[word] & bit == 0 {
                return false;
            }
        }
        true
    }
}

/// The optical size a platform's own text gives a font with an `opsz`
/// axis at `pixel_size`. CoreText sets the axis to the size in points,
/// which on macOS is the size in pixels; elsewhere Qt leaves a font at its
/// default optical size. A font without the axis is not changed.
fn optical_size(platform: Platform, pixel_size: f32) -> Option<FontVariations<'static>> {
    (platform == Platform::MacOs).then(|| {
        FontVariations::List(Cow::Owned(vec![FontVariation {
            tag: Tag::new(b"opsz"),
            value: pixel_size,
        }]))
    })
}

/// The generic family `name` stands for: a CSS generic, or the name Qt
/// gives macOS's system UI font ([`ljos_runtime::platform::SYSTEM_UI_FAMILY`]),
/// which is `system-ui` whatever the font's file is called.
fn generic_family(name: &str) -> Option<GenericFamily> {
    GenericFamily::parse(name).or_else(|| {
        name.eq_ignore_ascii_case(ljos_runtime::platform::SYSTEM_UI_FAMILY)
            .then_some(GenericFamily::SystemUi)
    })
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
                match generic_family(name) {
                    // Fontique's own reading of the platform's generic
                    // families can miss the rules fontconfig applies (a
                    // user's preferred monospace face among them), so the
                    // face fontconfig names comes first, as it does in Qt.
                    Some(generic) => {
                        if let Some(sub) = self.substitute(name) {
                            names.push(FontFamilyName::Named(Cow::Owned(sub.to_string())));
                        }
                        names.push(FontFamilyName::Generic(generic));
                    }
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

    /// The one family a plain text can be shaped with in place of its
    /// whole stack. Parley takes a cluster's font from the first font of
    /// the stack that covers it completely, so when the first font it would
    /// try covers every character, that font's family alone picks the same
    /// fonts, without the query expanding each generic family into every
    /// font the platform lists on every layout.
    fn pinned(
        &self,
        fcx: &mut FontContext,
        text: &str,
        family: &str,
        weight: FontWeight,
        italic: bool,
    ) -> Option<PinnedFont> {
        // Below U+2000 no character starts an emoji presentation (that
        // needs U+FE0F, U+20E3 or a pictograph above it, © and ® aside),
        // which parley shapes against an extended stack.
        if !self.pin
            || text.is_empty()
            || !text
                .chars()
                .all(|c| (c as usize) < PINNED_BELOW && c != '\u{a9}' && c != '\u{ae}')
        {
            return None;
        }
        let mut fonts = self.first_fonts.borrow_mut();
        if !fonts.contains_key(family) {
            fonts.insert(family.into(), Vec::new());
        }
        let entries = fonts.get_mut(family)?;
        let bits = weight.value().to_bits();
        let at = match entries
            .iter()
            .position(|(w, i, _)| *w == bits && *i == italic)
        {
            Some(at) => at,
            None => {
                let first = self.first_font(fcx, family, weight, italic);
                entries.push((bits, italic, first));
                entries.len() - 1
            }
        };
        let first = entries[at].2.as_mut()?;
        first.covers(text).then(|| PinnedFont {
            family: first.family.clone(),
            font: FontData::new(first.blob.clone(), first.index),
            source: first.source.clone(),
            synthesis: first.synthesis,
        })
    }

    /// The font Fontique's query yields first for `family`'s stack: for each
    /// family in turn, every font of its best match, then its default font,
    /// skipping those that do not load or have no character map.
    fn first_font(
        &self,
        fcx: &mut FontContext,
        family: &str,
        weight: FontWeight,
        italic: bool,
    ) -> Option<FirstFont> {
        let style = if italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };
        let mut ids = Vec::new();
        for name in self.stack_of(family) {
            match name {
                FontFamilyName::Named(name) => {
                    ids.extend(fcx.collection.family_by_name(&name).map(|f| f.id()));
                }
                FontFamilyName::Generic(generic) => {
                    ids.extend(fcx.collection.generic_families(generic));
                }
            }
        }
        for id in ids {
            let Some(info) = fcx.collection.family(id) else {
                continue;
            };
            let fonts = info.fonts();
            let best = info
                .match_index(FontWidth::NORMAL, style, weight, true)
                .and_then(|i| fonts.get(i))
                .map(|f| (f.width(), f.style(), f.weight()));
            let mut order: Vec<usize> = (0..fonts.len())
                .filter(|&i| {
                    let f = &fonts[i];
                    best == Some((f.width(), f.style(), f.weight()))
                })
                .collect();
            if !order.contains(&info.default_font_index()) {
                order.push(info.default_font_index());
            }
            for index in order {
                let Some(font) = fonts.get(index) else {
                    continue;
                };
                let Some(blob) = font.load(Some(&mut fcx.source_cache)) else {
                    continue;
                };
                let charmap = font.charmap_index();
                if charmap.charmap(blob.as_ref()).is_none() {
                    continue;
                }
                // The family has to come back as itself by name, or naming
                // it would pick another.
                let name = info.name();
                if fcx.collection.family_by_name(name).map(|f| f.id()) != Some(id) {
                    return None;
                }
                // As Fontique's query has it.
                let synthesis = font.synthesis(FontWidth::NORMAL, style, weight);
                return Some(FirstFont::new(
                    Arc::from(name),
                    blob,
                    font.source().clone(),
                    font.index(),
                    synthesis,
                    charmap,
                ));
            }
        }
        None
    }

    /// The family the platform's rules name for `name`, as Qt asks the
    /// platform; nothing with the bundled fonts alone.
    fn substitute(&self, name: &str) -> Option<Arc<str>> {
        if !self.system {
            return None;
        }
        if let Some(known) = self.substitutes.borrow().get(name) {
            return known.clone();
        }
        let sub = substitute::platform_substitute(name).map(Arc::from);
        self.substitutes
            .borrow_mut()
            .insert(name.into(), sub.clone());
        sub
    }
}

/// Lays out text. One per scene, or one shared by several scenes.
pub struct TextEngine {
    fcx: FontContext,
    lcx: LayoutContext<u32>,
    direct: Direct,
    families: Families,
    shapes: FrameCache<u64, ShapedEntry>,
    layouts: FrameCache<u64, LayoutEntry>,
    paragraphs: FrameCache<u64, edit::ParagraphEntry>,
    styles: FxHashMap<TextStyle, StyleMetrics>,
    frame: u64,
    keep_frames: u64,
    generation: u64,
    stats: CacheStats,
    /// What laying out cost since it was last taken, while it is timed.
    timed: Option<TextTimes>,
}

/// Laying out text that was not in the cache: how long and how often.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextTimes {
    pub spent: std::time::Duration,
    pub layouts: u64,
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

fn finite_weight(weight: f32) -> f32 {
    if weight.is_finite() {
        weight.clamp(1.0, 1000.0)
    } else {
        1.0
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
    let weight = FontWeight::new(finite_weight(style.weight));
    let pinned = (ranged.spans.is_empty() && ranged.objects.is_empty())
        .then(|| families.pinned(fcx, text, &style.family, weight, style.italic))
        .flatten();
    if pinned.is_some() {
        families.pinned.set(families.pinned.get() + 1);
    }
    let names = match &pinned {
        Some(pinned) => vec![FontFamilyName::Named(Cow::Borrowed(&*pinned.family))],
        None => families.stack(style),
    };
    // Resolved before the builder borrows the contexts.
    let span_names: Vec<Option<Vec<FontFamilyName<'_>>>> = ranged
        .spans
        .iter()
        .map(|s| s.family.as_deref().map(|f| families.stack_of(f)))
        .collect();
    let size = usable_size(style.pixel_size);
    let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
    let (word_break, overflow_wrap) = match class {
        BreakClass::Word => (WordBreak::Normal, OverflowWrap::Normal),
        BreakClass::Anywhere => (WordBreak::BreakAll, OverflowWrap::Anywhere),
        BreakClass::WordOrAnywhere => (WordBreak::Normal, OverflowWrap::Anywhere),
    };
    if families.one_style && ranged.spans.is_empty() && ranged.objects.is_empty() {
        // One style for all of it, given whole: the ranged builder first
        // resolves a default style of its own (a family list parsed from
        // text) and then each property pushed over it, for every text made.
        let mut b = lcx.style_run_builder(fcx, text, 1.0, false);
        let whole = b.push_style(ParleyStyle {
            font_family: FontFamily::List(Cow::Borrowed(&names)),
            font_size: size,
            font_variations: optical_size(Platform::CURRENT, size)
                .unwrap_or(FontVariations::empty()),
            font_weight: weight,
            font_style: if style.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            has_underline: style.underline,
            has_strikethrough: style.strikeout,
            letter_spacing: finite(style.letter_spacing),
            word_spacing: finite(style.word_spacing),
            word_break,
            overflow_wrap,
            ..ParleyStyle::default()
        });
        b.push_style_run(whole, ..);
        b.set_base_direction(base);
        return b.build(text);
    }
    // Scale 1 and no quantising: layouts are in logical pixels and unhinted,
    // so they do not depend on the screen and one cache entry fits every scale.
    let mut b = lcx.ranged_builder(fcx, text, 1.0, false);
    b.push_default(StyleProperty::FontFamily(FontFamily::List(Cow::Borrowed(
        &names,
    ))));
    b.push_default(StyleProperty::FontSize(size));
    if let Some(v) = optical_size(Platform::CURRENT, size) {
        b.push_default(StyleProperty::FontVariations(v));
    }
    b.push_default(StyleProperty::FontWeight(weight));
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
        let span_size = usable_size(span.pixel_size);
        b.push(StyleProperty::FontSize(span_size), range.clone());
        if let Some(v) = optical_size(Platform::CURRENT, span_size) {
            b.push(StyleProperty::FontVariations(v), range.clone());
        }
        b.push(
            StyleProperty::FontWeight(FontWeight::new(finite_weight(span.weight))),
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

/// Parley's own analysis and shaping, kept warm for [`shape_direct`].
struct Direct {
    analyzer: Analyzer,
    shaper: Shaper,
    shaped: ShapedText,
    /// A style index per character, all the one style.
    styles: Vec<u16>,
    /// Analyses by [`fold`]ed text: what Parley's analyser says of a
    /// printable ASCII character depends only on its class there, so "Row
    /// 12" and "Col 99" have one analysis, and a list's rows share a few.
    analyses: FrameCache<u64, KeptAnalysis>,
    /// For a text analysed while `analyses` is full.
    analysis: Analysis,
    /// The folded text being looked up, kept for its allocation.
    fold: Vec<u8>,
    /// Runs as shaped and snapped, for the same text in the same font
    /// again: a delegate made anew, or a row scrolled back into view after
    /// its layout was dropped.
    runs: FrameCache<u64, KeptRun>,
    /// The engine's frame, as the caches above are stamped with it.
    frame: u64,
    /// Runs taken from `runs`.
    reused: u64,
}

/// An analysis and the folded text it is for.
struct KeptAnalysis {
    fold: Box<[u8]>,
    analysis: Analysis,
}

/// What [`shape_direct`] made of a text in one font at one size, but the
/// font's blob, which is taken as Parley's path would take it.
struct KeptRun {
    text: Box<str>,
    blob: u64,
    index: u32,
    size: u32,
    synthesis: Synthesis,
    /// Each glyph with Qt's advance, in one allocation.
    glyphs: Box<[(parley::Glyph, f32)]>,
    coords: Box<[i16]>,
    font_size: f32,
    metrics: parley::FontMetrics,
}

/// Folded texts kept from frame to frame, and the most a frame can leave:
/// a list's rows have a few between them.
const MAX_ANALYSES: usize = 64;
const MAX_ANALYSES_AT_ONCE: usize = 2 * MAX_ANALYSES;
/// Runs kept from frame to frame, the most a frame can leave, and the
/// longest text one is kept for: about 0.5 MB for short rows, never more
/// than 4 MB.
const MAX_RUNS: usize = 2048;
const MAX_RUNS_AT_ONCE: usize = MAX_RUNS + MAX_RUNS / 2;
const MAX_RUN_TEXT: usize = 64;
/// A run unused for this many frames is dropped: far longer than a
/// layout, as a row scrolled away comes back seconds later.
const KEEP_RUN_FRAMES: u64 = 600;

impl Direct {
    fn new() -> Self {
        Self {
            analyzer: Analyzer::new(),
            shaper: Shaper::default(),
            shaped: ShapedText::new(),
            styles: Vec::new(),
            analyses: FrameCache::splitting_frames(MAX_ANALYSES),
            analysis: Analysis::new(),
            fold: Vec::new(),
            runs: FrameCache::splitting_frames(MAX_RUNS),
            frame: 0,
            reused: 0,
        }
    }

    fn clear(&mut self) {
        self.analyses.clear();
        self.runs.clear();
    }

    fn bytes(&self) -> usize {
        let runs: usize = self
            .runs
            .values()
            .map(|r| {
                std::mem::size_of::<KeptRun>()
                    + r.text.len()
                    + std::mem::size_of_val(&*r.glyphs)
                    + std::mem::size_of_val(&*r.coords)
            })
            .sum();
        let analyses: usize = self
            .analyses
            .values()
            .map(|a| {
                std::mem::size_of::<KeptAnalysis>()
                    + a.fold.len()
                    + std::mem::size_of_val(a.analysis.char_info())
                    + std::mem::size_of_val(a.analysis.bidi_levels())
            })
            .sum();
        runs + analyses + self.runs.table_bytes() + self.analyses.table_bytes()
    }

    fn trim(&mut self, frame: u64, keep: u64) {
        self.frame = frame;
        self.analyses.trim(frame, keep);
        self.runs.trim(frame, keep.max(KEEP_RUN_FRAMES));
    }
}

/// A printable ASCII text with every letter made `a` and every digit `0`,
/// behind a byte for the line breaking. Parley's analysis of a character
/// (script, grapheme, word and line boundaries, bidi class) is the same
/// for every ASCII letter, and for every digit: Latin or Common, ALetter
/// or Numeric, AL or NU. So a text analyses as its fold does; the test
/// below holds it to that.
fn fold(text: &[u8], word_break: WordBreak, out: &mut Vec<u8>) {
    out.clear();
    out.push(u8::from(word_break == WordBreak::BreakAll));
    out.extend(text.iter().map(|&b| match b {
        b'a'..=b'z' | b'A'..=b'Z' => b'a',
        b'0'..=b'9' => b'0',
        b => b,
    }));
}

/// Gives every cluster the one font a pinned text is shaped with.
struct OneFont<'a>(FontInstanceRef<'a>);

impl FontSelector for OneFont<'_> {
    fn select_font(
        &mut self,
        _: &Segment,
        _: &ShapeOptions<'_>,
        _: &mut CharCluster,
    ) -> Option<FontInstanceRef<'_>> {
        Some(self.0)
    }
}

/// The glyphs of a text that Parley lays out as one run on one line, read
/// as [`snapped_widths_kept`] reads them from Parley's layout, but shaped
/// with Parley's analyser and shaper alone, as its builder drives them.
/// Building a layout costs more than shaping such a text: resolving its
/// style, asking the font query for every cluster, and making the runs
/// and lines. `None` when the text is not that simple (it is then shaped
/// in full) or Parley would take more than one run for it.
///
/// The text is printable ASCII, so it has one character per byte, is left
/// to right in one script, and needs no emoji font; with its family pinned
/// ([`Families::pinned`]) the font query would choose the pinned font for
/// every cluster, which is the one given here. Without letter or word
/// spacing Parley neither turns ligatures off nor moves a glyph, and a
/// line that does not end in a space has nothing hanging. A glyph moved
/// off the baseline would come out of `extract` rounded through it, so
/// such a text is shaped in full too.
///
/// A run shaped before in the same font at the same size is taken from
/// [`Direct::runs`]; otherwise the analysis of a text of the same
/// [`fold`] is taken from [`Direct::analyses`], and the text is shaped.
fn shape_direct(
    direct: &mut Direct,
    fcx: &mut FontContext,
    families: &Families,
    text: &str,
    style: &TextStyle,
    ranged: &Ranged<'_>,
    class: BreakClass,
) -> Option<Snapped> {
    let frame = direct.frame;
    let bytes = text.as_bytes();
    let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
    if !families.one_style
        || !ranged.spans.is_empty()
        || !ranged.objects.is_empty()
        || bytes.is_empty()
        || bytes.last() == Some(&b' ')
        || !bytes.iter().all(|b| (0x20..0x7f).contains(b))
        || finite(style.letter_spacing) != 0.0
        || finite(style.word_spacing) != 0.0
    {
        return None;
    }
    let weight = FontWeight::new(finite_weight(style.weight));
    let pinned = families.pinned(fcx, text, &style.family, weight, style.italic)?;
    let size = usable_size(style.pixel_size);
    let optical = optical_size(Platform::CURRENT, size);
    let variations = match &optical {
        Some(FontVariations::List(list)) => &list[..],
        _ => &[],
    };
    // As Parley's analysis of the one style's breaking.
    let word_break = match class {
        BreakClass::Anywhere => WordBreak::BreakAll,
        BreakClass::Word | BreakClass::WordOrAnywhere => WordBreak::Normal,
    };
    let config = LineBreakConfig {
        word_break,
        ..LineBreakConfig::default()
    };
    let line_break = [(0..text.len(), config)];
    let line_break = if config == LineBreakConfig::default() {
        &line_break[..0]
    } else {
        &line_break[..]
    };
    let options = AnalysisOptions {
        base_direction: BaseDirection::Auto,
        line_break,
        break_spaces: &[],
        line_break_override: None,
    };
    // The optical size's variations follow from the size, and no features
    // are asked for, so the font, its synthesis and the size say what the
    // shaper is given.
    let (blob, index, size_bits) = (pinned.font.data.id(), pinned.font.index, size.to_bits());
    let run_key = (bytes.len() <= MAX_RUN_TEXT).then(|| {
        let mut h = FxHasher::default();
        (text, blob, index, size_bits).hash(&mut h);
        h.finish()
    });
    let same = |k: &KeptRun| {
        &*k.text == text
            && k.blob == blob
            && k.index == index
            && k.size == size_bits
            && k.synthesis == pinned.synthesis
    };
    let mut kept = Snapped::default();
    let hit = run_key
        .and_then(|key| direct.runs.get_mut(&key, frame))
        .filter(|k| same(k));
    let (font, font_size, metrics, coords) = if let Some(run) = hit {
        direct.reused += 1;
        kept.glyphs.extend(run.glyphs.iter().map(|g| g.0));
        kept.qt.extend(run.glyphs.iter().map(|g| g.1));
        let (font_size, metrics, coords) = (run.font_size, run.metrics, run.coords.to_vec());
        (source_font(fcx, &pinned)?, font_size, metrics, coords)
    } else {
        fold(bytes, word_break, &mut direct.fold);
        let fold_key = {
            let mut h = FxHasher::default();
            direct.fold.hash(&mut h);
            h.finish()
        };
        let known = direct
            .analyses
            .get_mut(&fold_key, frame)
            .is_some_and(|k| *k.fold == direct.fold[..]);
        let keep = known || direct.analyses.len() < MAX_ANALYSES_AT_ONCE;
        if !known && keep {
            let mut analysis = Analysis::new();
            direct.analyzer.analyze(text, &options, &mut analysis);
            let fold = direct.fold.as_slice().into();
            direct
                .analyses
                .insert(fold_key, KeptAnalysis { fold, analysis }, frame);
        } else if !keep {
            direct
                .analyzer
                .analyze(text, &options, &mut direct.analysis);
        }
        let analysis = if keep {
            &direct.analyses.get_mut(&fold_key, frame)?.analysis
        } else {
            &direct.analysis
        };
        direct.styles.clear();
        direct.styles.resize(bytes.len(), 0);
        let item = parley_engine::itemize::Item {
            char_end: u32::try_from(bytes.len()).ok()?,
            options: ShapeOptions {
                font_size: size,
                language: None,
                features: &[],
                variations,
            },
        };
        let one = OneFont(FontInstanceRef {
            data: &pinned.font.data,
            index,
            synthesis: &pinned.synthesis,
        });
        direct.shaper.shape_text(
            text,
            analysis,
            &direct.styles,
            [item],
            one,
            &mut direct.shaped,
        );
        let [run] = direct.shaped.runs() else {
            return None;
        };
        if run.bidi_level.is_rtl() {
            return None;
        }
        let slice = direct.shaped.run_slice(0);
        kept.glyphs.reserve(bytes.len());
        kept.qt.reserve(bytes.len());
        for cluster in slice.shaped_clusters_range() {
            kept.glyphs.extend(slice.shaped_cluster_glyphs(cluster));
        }
        if kept.glyphs.iter().any(|g| g.y != 0.0) {
            return None;
        }
        let coords: Vec<i16> = direct
            .shaped
            .normalized_coords()
            .get(run.normalized_coords_range.clone())
            .unwrap_or(&[])
            .iter()
            .map(|c| c.to_bits())
            .collect();
        let font = source_font(fcx, &pinned)?;
        snap::advances(&font, run.font_size, &coords, &kept.glyphs, &mut kept.qt);
        // The trim keeps a frame's runs together, and one frame can lay
        // out any number of texts: past a ceiling nothing new is kept
        // until frames make room.
        if let Some(key) = run_key.filter(|_| direct.runs.len() < MAX_RUNS_AT_ONCE) {
            let run = KeptRun {
                text: text.into(),
                blob,
                index,
                size: size_bits,
                synthesis: pinned.synthesis,
                glyphs: kept
                    .glyphs
                    .iter()
                    .copied()
                    .zip(kept.qt.iter().copied())
                    .collect(),
                coords: coords.as_slice().into(),
                font_size: run.font_size,
                metrics: run.font_metrics,
            };
            direct.runs.insert(key, run, frame);
        }
        (font, run.font_size, run.font_metrics, coords)
    };
    // A style's line has no offset or size of its own, as Parley's
    // default style has it.
    let line = |on: bool| on.then_some((None, None));
    kept.items.push(Item::Run(RunFacts {
        glyphs: 0..kept.glyphs.len(),
        offset: 0.0,
        // Only ever added to and taken from a glyph's y, which is 0.
        baseline: 0.0,
        font,
        font_size,
        metrics,
        synthesis: pinned.synthesis,
        coords,
        underline: line(style.underline),
        strikethrough: line(style.strikeout),
        tag: 0,
    }));
    kept.lines.push(kept.items.len());
    families.pinned.set(families.pinned.get() + 1);
    Some(kept)
}

/// The pinned font's file as Parley's builder takes it when made: its
/// source cache drops a file no recent builder loaded, so the file is
/// loaded again, as a new blob, where Parley's path would load it again.
fn source_font(fcx: &mut FontContext, pinned: &PinnedFont) -> Option<FontData> {
    fcx.source_cache.prune(128, false);
    Some(FontData::new(
        fcx.source_cache.get(&pinned.source)?,
        pinned.font.index,
    ))
}

/// Break lines at `max` and put every line at x 0. Alignment is the item's
/// business, so one layout serves every alignment.
fn break_lines(shape: &mut Shape, max: Option<f32>, justify: bool) {
    match max {
        Some(max) => break_inside(shape, max),
        None => shape.break_all_lines(None),
    }
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

/// `known` is every glyph of `shape` as it is broken now, with Qt's
/// advance ([`snapped_widths_kept`]), where the caller has them.
fn extract(
    shape: &Shape,
    fallback: Vertical,
    justify_width: Option<f32>,
    objects: &[InlineObject],
    known: Option<&Snapped>,
) -> Vec<RawLine> {
    let read;
    let known = match known {
        Some(known) => known,
        None => {
            read = Snapped::read(shape);
            &read
        }
    };
    let lines = shape.lines().map(|line| LineFacts {
        advance: line.metrics().advance,
        hanging_advance: line.metrics().hanging_advance,
        reason: line.break_reason(),
        range: line.text_range(),
    });
    extract_lines(lines, known, fallback, justify_width, objects)
}

/// What [`extract_lines`] takes from a line besides its glyphs.
struct LineFacts {
    advance: f32,
    /// Of the spaces at a wrap, which hang outside the line.
    hanging_advance: f32,
    reason: BreakReason,
    range: Range<usize>,
}

/// [`extract`] of lines given by their facts and their glyphs in `known`.
fn extract_lines(
    lines: impl ExactSizeIterator<Item = LineFacts>,
    known: &Snapped,
    fallback: Vertical,
    justify_width: Option<f32>,
    objects: &[InlineObject],
) -> Vec<RawLine> {
    let mut out = Vec::with_capacity(lines.len());
    for (m, items) in lines.zip(known.lines()) {
        let reason = m.reason;
        let wrapped = matches!(reason, BreakReason::Regular | BreakReason::Emergency);
        let mut runs = Vec::new();
        let mut boxes = Vec::new();
        let mut vertical: Option<Vertical> = None;
        let mut unseen: Option<Vertical> = None;
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
        // The glyph that ends the line: its run and id, and its advance.
        let mut last: Option<(usize, u32, f32)> = None;
        for item in items {
            let run = match item {
                Item::Run(run) => run,
                Item::Box(placed) => {
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
                    last = None;
                    continue;
                }
            };
            let fm = run.metrics;
            let chosen = face::vertical(&run.font, run.font_size, shaped_vertical(&fm));
            let (ascent, descent) = (floor64(chosen.ascent), floor64(chosen.descent));
            let tall = |v: Option<Vertical>| {
                Some(v.map_or(Vertical { ascent, descent }, |v| Vertical {
                    ascent: v.ascent.max(ascent),
                    descent: v.descent.max(descent),
                }))
            };
            let (placed, advances) = known.glyphs(run);
            // A run that draws nothing (the character an editor keeps for
            // an image) is as tall as its font only on a line without
            // objects.
            if placed.iter().all(|g| g.advance == 0.0) {
                unseen = tall(unseen);
            } else {
                vertical = tall(vertical);
            }
            let baseline = run.baseline;
            let offset = run.offset - shift;
            let mut pen = run.offset;
            let mut advance = 0.0_f32;
            let mut glyphs = Vec::with_capacity(placed.len());
            for (g, &qt) in placed.iter().zip(advances) {
                // Placed as `GlyphRun::positioned_glyphs` places them, whose
                // pen is this one.
                glyphs.push(PositionedGlyph {
                    id: g.id,
                    x: (g.x + pen) - shift,
                    y: (g.y + baseline) - baseline,
                });
                if justified {
                    advance += g.advance;
                } else {
                    advance += qt;
                    if !hangs(pen, g.advance) {
                        shift += g.advance - qt;
                        width += qt;
                        last = Some((runs.len(), g.id, qt));
                    }
                }
                pen += g.advance;
            }
            if glyphs.is_empty() {
                continue;
            }
            let underline = run.underline.map(|(offset, size)| {
                (
                    offset.unwrap_or(fm.underline_offset),
                    size.unwrap_or(fm.underline_size),
                )
            });
            let strikeout = run.strikethrough.map(|(offset, size)| {
                (
                    offset.unwrap_or(fm.strikethrough_offset),
                    // Qt draws both bars with the underline's thickness.
                    size.unwrap_or(fm.underline_size),
                )
            });
            let synthesis = run.synthesis;
            runs.push(RawRun {
                font: run.font.clone(),
                font_size: run.font_size,
                glyphs,
                offset,
                advance,
                ascent: chosen.ascent as f32,
                descent: chosen.descent as f32,
                coords: run.coords.clone(),
                embolden: synthesis.embolden(),
                skew: synthesis
                    .skew()
                    .map_or(0.0, |degrees| degrees.to_radians().tan()),
                underline,
                strikeout,
                line_size: fm.underline_size,
                tag: run.tag,
            });
        }
        if let Some((r, id, qt)) = last
            && let Some(run) = runs.get(r)
        {
            width += overhang(
                &run.font,
                run.font_size,
                &run.coords,
                run.embolden,
                run.skew,
                id,
                qt,
            );
        }
        if boxes.is_empty() {
            vertical = vertical.or(unseen);
        }
        let has_glyphs = !runs.is_empty() || !boxes.is_empty();
        // Measured on Qt 6.11: a line that is only objects standing on the
        // baseline is as tall as they are, without room for descenders.
        let only_standing = vertical.is_none()
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
        // A line holding only an inline object has no text, and its range
        // comes back inverted (`usize::MAX..0`): it stands, empty, where
        // the line before it ended.
        let range = m.range;
        let range = if range.start > range.end {
            let at = out.last().map_or(0, |l: &RawLine| l.range.end);
            at..at
        } else {
            range
        };
        out.push(RawLine {
            range,
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

/// Break lines at `max` so that each line's last glyph keeps its ink inside
/// it, as Qt's breaker does: measured on Qt 6.11, "aWaW" broken anywhere at
/// a width the advances of "aW" fit but the ink of its `W` crosses takes
/// three lines. Parley breaks by advances alone, so a line whose ink
/// crosses is broken again a little shorter, and the lines after it anew.
/// That is rare, and costs nothing where no line comes near the width.
fn break_inside(shape: &mut Shape, max: f32) {
    // Each line's own limit, below `max` where it was broken again.
    let mut limits: Vec<f32> = Vec::new();
    // Lines before this one are as they stay. One that could be no
    // shorter (a single cluster, or none sooner Parley would break at)
    // keeps its ink past the width.
    let mut from = 0;
    break_by(shape, max, &limits);
    loop {
        let Some((index, advance)) = shape
            .lines()
            .enumerate()
            .skip(from)
            .find_map(|(i, l)| ink_crosses(&l, max).then(|| (i, l.metrics().advance)))
        else {
            return;
        };
        let before = shape.get(index).map(|l| l.text_range());
        limits.resize(index, max);
        // Never past `max`, which Parley asks of every line's limit.
        limits.push((advance - 0.01).min(max));
        break_by(shape, max, &limits);
        from = if shape.get(index).map(|l| l.text_range()) == before {
            index + 1
        } else {
            index
        };
    }
}

/// Break lines at `max`, each line at its own limit where `limits` has one.
fn break_by(shape: &mut Shape, max: f32, limits: &[f32]) {
    let mut breaker = shape.break_lines();
    breaker.state_mut().set_layout_max_advance(max);
    let mut line = 0;
    loop {
        let limit = limits.get(line).copied().unwrap_or(max);
        breaker.state_mut().set_line_max_advance(limit);
        match breaker.break_next() {
            None => break,
            Some(parley::YieldData::InlineBoxBreak(_)) => {
                // As `break_remaining` does: placed where it is, so
                // breaking goes on.
                let x = breaker.state().line_x();
                breaker.state_mut().append_inline_box_to_line(x, 0.0, 0.0);
            }
            Some(_) => line += 1,
        }
    }
    breaker.finish();
}

/// Whether a line is wider than `max` as Qt counts it: its advances and
/// the ink its last glyph reaches past them.
fn ink_crosses(line: &parley::Line<'_, u32>, max: f32) -> bool {
    // Ink past an advance is a few pixels at most: a line well inside
    // the width needs no glyph's outline.
    if line.metrics().advance + line_reach(line) <= max {
        return false;
    }
    let (width, past) = line_widths(line);
    past > 0.0 && width + past > max
}

/// The most a line's last glyph can reach past its advance.
fn line_reach(line: &parley::Line<'_, u32>) -> f32 {
    line.runs().last().map_or(0.0, |r| r.font_size())
}

/// A line's width by Qt's advances, and how far its last glyph's ink
/// reaches past them ([`overhang`]).
fn line_widths(line: &parley::Line<'_, u32>) -> (f32, f32) {
    line_widths_kept(line, &mut Snapped::default())
}

/// [`line_widths`], reading the line into `keep`.
fn line_widths_kept(line: &parley::Line<'_, u32>, keep: &mut Snapped) -> (f32, f32) {
    let from = keep.items.len();
    keep.read_line(line);
    items_widths(keep, from)
}

/// [`line_widths`] of the line whose items in `keep` start at `from`.
fn items_widths(keep: &Snapped, from: usize) -> (f32, f32) {
    let mut width = 0.0;
    let mut last = 0.0;
    for item in &keep.items[from..] {
        match item {
            Item::Run(run) => {
                let (glyphs, qt) = keep.glyphs(run);
                width += qt.iter().sum::<f32>();
                if let (Some(g), Some(a)) = (glyphs.last(), qt.last()) {
                    last = overhang(
                        &run.font,
                        run.font_size,
                        &run.coords,
                        run.synthesis.embolden(),
                        run.synthesis.skew().map_or(0.0, |d| d.to_radians().tan()),
                        g.id,
                        *a,
                    );
                }
            }
            Item::Box(placed) => {
                width += placed.width;
                last = 0.0;
            }
        }
    }
    (width, last)
}

/// A shape's lines as read from Parley: every run with its glyphs as
/// shaped and the advance Qt gives each (see [`snap`]), and every inline
/// object. Reading them from Parley costs more than anything done with
/// them (a line's items are found anew each time they are asked for, its
/// glyphs at about 110 instructions each), so a layout reads them once.
#[derive(Default)]
struct Snapped {
    glyphs: Vec<parley::Glyph>,
    qt: Vec<f32>,
    items: Vec<Item>,
    /// Where each line's items end.
    lines: Vec<usize>,
}

enum Item {
    Run(RunFacts),
    Box(parley::PositionedInlineBox),
}

/// What a glyph run's glyphs are drawn with.
struct RunFacts {
    /// In [`Snapped::glyphs`].
    glyphs: Range<usize>,
    /// Along the line to the first glyph, and to the baseline.
    offset: f32,
    baseline: f32,
    font: FontData,
    font_size: f32,
    metrics: parley::FontMetrics,
    synthesis: parley::fontique::Synthesis,
    coords: Vec<i16>,
    /// Offset and size each, where the style asks for the line.
    underline: Option<(Option<f32>, Option<f32>)>,
    strikethrough: Option<(Option<f32>, Option<f32>)>,
    tag: u32,
}

impl Snapped {
    fn read(shape: &Shape) -> Self {
        let mut s = Self::default();
        for line in shape.lines() {
            s.read_line(&line);
        }
        s
    }

    fn read_line(&mut self, line: &parley::Line<'_, u32>) {
        for item in line.items() {
            match item {
                PositionedLayoutItem::GlyphRun(gr) => self.push(&gr),
                PositionedLayoutItem::InlineBox(placed) => self.items.push(Item::Box(placed)),
            }
        }
        self.lines.push(self.items.len());
    }

    fn push(&mut self, gr: &parley::GlyphRun<'_, u32>) {
        let start = self.glyphs.len();
        let run = gr.run();
        // The glyphs come cluster by cluster with no length known ahead.
        // A glyph per byte is seldom short and is known without counting
        // the clusters, so this is usually the one growth.
        let bytes = run.text_range().len();
        self.glyphs.reserve(bytes);
        self.qt.reserve(bytes);
        self.glyphs.extend(gr.glyphs());
        let coords = coords(run);
        snap::advances(
            run.font(),
            run.font_size(),
            &coords,
            &self.glyphs[start..],
            &mut self.qt,
        );
        let style = gr.style();
        self.items.push(Item::Run(RunFacts {
            glyphs: start..self.glyphs.len(),
            offset: gr.offset(),
            baseline: gr.baseline(),
            font: run.font().clone(),
            font_size: run.font_size(),
            metrics: *run.font_metrics(),
            synthesis: run.synthesis(),
            coords,
            underline: style.underline.as_ref().map(|d| (d.offset, d.size)),
            strikethrough: style.strikethrough.as_ref().map(|d| (d.offset, d.size)),
            tag: style.brush,
        }));
    }

    /// A run's glyphs and their advances.
    fn glyphs(&self, run: &RunFacts) -> (&[parley::Glyph], &[f32]) {
        (
            &self.glyphs[run.glyphs.clone()],
            &self.qt[run.glyphs.clone()],
        )
    }

    /// Every line's items, in the order they were read.
    fn lines(&self) -> impl Iterator<Item = &[Item]> {
        let starts = std::iter::once(0).chain(self.lines.iter().copied());
        starts
            .zip(&self.lines)
            .map(|(start, &end)| &self.items[start..end])
    }
}

/// The glyphs of a run as shaped, and the advance Qt gives each (see
/// [`snap`]).
fn snapped(gr: &parley::GlyphRun<'_, u32>) -> (Vec<parley::Glyph>, Vec<f32>) {
    let mut s = Snapped::default();
    s.push(gr);
    (s.glyphs, s.qt)
}

/// Where on its variation axes a run's font is.
fn coords(run: &parley::Run<'_, u32>) -> Vec<i16> {
    run.normalized_coords()
        .iter()
        .map(|c| c.to_bits())
        .collect()
}

/// Width of the widest line as Qt counts it, spaces at the end included.
/// The widest line of `shape` by Qt's advances, and with the ink of each
/// line's last glyph that reaches past them ([`overhang`]).
fn snapped_widths(shape: &Shape) -> (f32, f32) {
    snapped_widths_kept(shape).0
}

/// [`snapped_widths`], and every glyph with Qt's advance, run after run in
/// the order of the lines, for [`extract`] of `shape` as it is broken now.
fn snapped_widths_kept(shape: &Shape) -> ((f32, f32), Snapped) {
    let mut keep = Snapped::default();
    let (mut advance, mut natural) = (0.0_f32, 0.0_f32);
    for line in shape.lines() {
        let (width, last) = line_widths_kept(&line, &mut keep);
        advance = advance.max(width);
        natural = natural.max(width + last);
    }
    ((advance, natural), keep)
}

/// How far the ink of the glyph that ends a line reaches past its advance,
/// in whole pixels, which Qt counts in the line's width (`implicitWidth`
/// and `contentWidth` of every text item; not `advanceWidth`). Measured on
/// Qt 6.11 as a black box: the ink's right edge rounded up against the
/// advance rounded, as hinted metrics have them, so `t` and `W` at 12 px
/// add a pixel and at 9 px none. Where a font's hinting widens an advance
/// at a small size Qt adds nothing and this still does.
fn overhang(
    font: &FontData,
    font_size: f32,
    coords: &[i16],
    embolden: bool,
    skew: f32,
    glyph: u32,
    advance: f32,
) -> f32 {
    let Some(right) = measure::ink_right(font, font_size, coords, embolden, skew, glyph) else {
        return 0.0;
    };
    // On Qt's 1/64 px grid before rounding, so an edge on a whole pixel
    // stays there.
    let right = ((right * 64.0).round() / 64.0).ceil() as f32;
    (right - advance.round()).max(0.0)
}

/// Byte range and advance of every cluster of a shaped text.
fn cluster_advances(shape: &Shape) -> Vec<(Range<usize>, f32)> {
    let mut out = Vec::new();
    for line in shape.lines() {
        for run in line.runs() {
            let coords = coords(&run);
            let (mut shaped, mut qt) = (Vec::new(), Vec::new());
            for cluster in run.clusters() {
                shaped.clear();
                qt.clear();
                shaped.extend(cluster.glyphs());
                snap::advances(run.font(), run.font_size(), &coords, &shaped, &mut qt);
                // What Qt's glyphs give up, taken off the cluster's own
                // advance, which also holds any spacing.
                let given_up: f32 = shaped.iter().zip(&qt).map(|(g, q)| g.advance - q).sum();
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
        let fcx = if system {
            // The platform's rules for families that are not installed are
            // loaded beside Fontique's own reading of the fonts, not by the
            // first text that names one: each takes 15-25 ms.
            std::thread::scope(|s| {
                let _ = std::thread::Builder::new()
                    .name("ljos-fontrules".into())
                    .spawn_scoped(s, substitute::load_rules);
                fonts::font_context(true)
            })
        } else {
            fonts::font_context(false)
        };
        let mut engine = Self {
            fcx,
            lcx: LayoutContext::new(),
            direct: Direct::new(),
            families: Families {
                system,
                default_family: None,
                substitutes: RefCell::default(),
                first_fonts: RefCell::default(),
                pinned: Cell::new(0),
                pin: true,
                one_style: true,
            },
            shapes: FrameCache::new(MAX_SHAPES),
            layouts: FrameCache::new(MAX_LAYOUTS),
            paragraphs: FrameCache::new(MAX_PARAGRAPHS),
            styles: FxHashMap::default(),
            frame: 0,
            keep_frames: KEEP_FRAMES,
            generation: 0,
            stats: CacheStats::default(),
            timed: None,
        };
        if system {
            engine.warm_fallback();
        }
        engine
    }

    /// Looks up, here, the font a character the default family lacks falls
    /// back to. The first lookup asks fontconfig to sort every installed
    /// font (5-10 ms with 2 400 of them), and this runs on the thread that
    /// reads the fonts rather than in the first frame that draws a symbol;
    /// the font found is kept in the collection. Symbols, arrows and
    /// punctuation (the Common script) share one lookup. Other scripts are
    /// looked up when text in them is first laid out.
    fn warm_fallback(&mut self) {
        let _ = self.layout("\u{215f}", &TextStyle::default(), &LayoutParams::default());
        self.clear_caches();
        self.stats = CacheStats::default();
        self.families.pinned.set(0);
        self.direct.reused = 0;
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
        self.families.first_fonts.borrow_mut().clear();
        self.clear_caches();
    }

    /// Drop every cached layout and shape.
    pub fn clear_caches(&mut self) {
        self.shapes.clear();
        self.layouts.clear();
        self.paragraphs.clear();
        self.styles.clear();
        self.direct.clear();
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
        self.direct.trim(self.frame, self.keep_frames);
        if self.styles.len() > MAX_STYLES {
            self.styles.clear();
        }
    }

    /// Times the layouts made from now on, for [`Self::take_times`], or
    /// stops. Off, no clock is read.
    pub fn set_timed(&mut self, on: bool) {
        self.timed = on.then(TextTimes::default);
    }

    /// What laying out cost since the last call; nothing while not timed.
    pub fn take_times(&mut self) -> TextTimes {
        self.timed.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// [`Self::build_layout`], timed while the engine is.
    fn timed_layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: Option<&Ranged<'_>>,
        params: &LayoutParams,
    ) -> TextLayout {
        let Some(started) = self.timed.is_some().then(std::time::Instant::now) else {
            return self.build_layout(text, style, ranged, params);
        };
        let layout = self.build_layout(text, style, ranged, params);
        if let Some(t) = &mut self.timed {
            t.spent += started.elapsed();
            t.layouts += 1;
        }
        layout
    }

    /// Cache sizes and counters.
    /// What the caches hold now: one pass over their entries.
    pub fn memory(&self) -> TextMemory {
        let shaped: usize = self
            .shapes
            .values()
            .map(|e| {
                std::mem::size_of::<ShapedEntry>()
                    + e.text.len() * (1 + SHAPED_BYTES_PER_TEXT_BYTE)
                    + e.ranged.bytes()
            })
            .sum();
        let laid: usize = self
            .layouts
            .values()
            .map(|e| {
                std::mem::size_of::<LayoutEntry>()
                    + e.text.len()
                    + e.ranged.as_ref().map_or(0, OwnedRanged::bytes)
                    + e.layout.bytes()
            })
            .sum();
        let paragraphs: usize = self.paragraphs.values().map(|e| e.bytes()).sum();
        let styles = self.styles.capacity()
            * (std::mem::size_of::<TextStyle>() + std::mem::size_of::<StyleMetrics>());
        let direct = self.direct.bytes();
        let (glyphs, faces) = measure::kept();
        TextMemory {
            layouts: self.layouts.len(),
            shapes: self.shapes.len(),
            paragraphs: self.paragraphs.len(),
            styles: self.styles.len(),
            runs: self.direct.runs.len(),
            glyphs,
            bytes: shaped
                + direct
                + laid
                + paragraphs
                + styles
                + faces
                + self.shapes.table_bytes()
                + self.layouts.table_bytes()
                + self.paragraphs.table_bytes(),
        }
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            layouts: self.layouts.len(),
            shapes: self.shapes.len(),
            glyphs_measured: measure::measured(),
            shapes_pinned: self.families.pinned.get(),
            runs_reused: self.direct.reused,
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
        let layout = Arc::new(self.timed_layout(text, style, None, params));
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
        let layout = Arc::new(self.timed_layout(text, style, Some(ranged), params));
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

    /// Width of `text` on one line by its advances, as
    /// [`TextLayout::advance`].
    pub fn advance_width(&mut self, text: &str, style: &TextStyle) -> f64 {
        self.layout(text, style, &LayoutParams::default()).advance
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
            .and_then(|run| measure::font_tables(run.font()));
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
            ellipsis: snapped_widths(&dots).0,
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
        let key = self.shaped_key(text, style, ranged, class, true);
        self.shapes
            .get_mut(&key, self.frame)
            .expect("the entry was made a moment ago")
    }

    /// [`Self::shaped`] with Parley's layout of the text in it.
    fn shaped_whole(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        class: BreakClass,
    ) -> &mut ShapedEntry {
        let key = self.shaped_key(text, style, ranged, class, false);
        self.give_shape(key, text, style, ranged, class);
        self.shapes
            .get_mut(&key, self.frame)
            .expect("the entry was made a moment ago")
    }

    /// Gives the entry at `key` Parley's layout of its text where it was
    /// shaped without one.
    fn give_shape(
        &mut self,
        key: u64,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        class: BreakClass,
    ) {
        let frame = self.frame;
        let found = "the entry was made a moment ago";
        if self
            .shapes
            .get_mut(&key, frame)
            .expect(found)
            .shape
            .is_none()
        {
            let shape = self.unbroken(text, style, ranged, class);
            self.shapes.get_mut(&key, frame).expect(found).shape = Some(shape);
        }
    }

    /// The key of `text` shaped in the cache, shaping it on a miss:
    /// directly ([`shape_direct`]) where it can be and `direct` allows,
    /// which a caller that will need Parley's layout anyway turns off so
    /// as not to shape twice.
    fn shaped_key(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        class: BreakClass,
        direct: bool,
    ) -> u64 {
        let key = key_hash(text, style, (class, ranged.spans, ranged.objects));
        let frame = self.frame;
        let hit = self.shapes.get_mut(&key, frame).is_some_and(|e| {
            &*e.text == text && e.style == *style && e.class == class && e.ranged.matches(ranged)
        });
        if hit {
            return key;
        }
        self.stats.shapes_built += 1;
        let direct = direct
            .then(|| {
                shape_direct(
                    &mut self.direct,
                    &mut self.fcx,
                    &self.families,
                    text,
                    style,
                    ranged,
                    class,
                )
            })
            .flatten();
        let (shape, (advance, natural_width), advances) = match direct {
            Some(advances) => {
                let (width, last) = items_widths(&advances, 0);
                (None, (width.max(0.0), (width + last).max(0.0)), advances)
            }
            None => {
                let shape = self.unbroken(text, style, ranged, class);
                let (widths, advances) = snapped_widths_kept(&shape);
                (Some(shape), widths, advances)
            }
        };
        let entry = ShapedEntry {
            text: text.into(),
            style: style.clone(),
            class,
            ranged: OwnedRanged::of(ranged),
            advance,
            natural_width,
            rtl: shape.as_ref().is_some_and(Shape::is_rtl),
            shape,
            broken: (None, false),
            advances,
        };
        self.shapes.insert(key, entry, frame);
        key
    }

    /// Parley's layout of `text` as a shaped entry keeps it, on one line
    /// for every paragraph.
    fn unbroken(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        class: BreakClass,
    ) -> Shape {
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
        shape
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
        // By advances, as Qt elides: a text whose last glyph's ink alone
        // is past the width is truncated but keeps every character
        // (measured on Qt 6.11).
        if !force && snapped_widths(&full).0 <= width {
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
            if keep == 0 || snapped_widths(&shape).0 <= width {
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

        let (mut raw, (advance, natural), rtl) = if text.is_empty() && ranged.objects.is_empty() {
            (Vec::new(), (0.0, 0.0), false)
        } else {
            let class = BreakClass::of(wrap);
            // A wrapped text needs Parley's lines only when it turns out
            // too wide; a ranged or justified one always does.
            let key = self.shaped_key(text, style, &ranged, class, !places && !justify);
            let frame = self.frame;
            let found = "the entry was made a moment ago";
            let entry = self.shapes.get_mut(&key, frame).expect(found);
            // Parley breaks lines by the shaper's advances, which are a
            // touch wider than Qt's: a text that fits as Qt counts it (an
            // item as wide as its own text) must not be broken.
            let max_advance = max_advance.filter(|w| entry.natural_width > *w);
            if places || (max_advance, justify) != (None, false) {
                self.give_shape(key, text, style, &ranged, class);
            }
            let entry = self.shapes.get_mut(&key, frame).expect(found);
            let lines = match &mut entry.shape {
                Some(shape) => {
                    if entry.broken != (max_advance, justify) {
                        break_lines(shape, max_advance, justify);
                        entry.broken = (max_advance, justify);
                    }
                    let unbroken = entry.broken == (None, false);
                    let mut lines = extract(
                        shape,
                        fallback,
                        max_advance.filter(|_| justify),
                        ranged.objects,
                        unbroken.then_some(&entry.advances),
                    );
                    if places {
                        for (line, carets) in lines.iter_mut().zip(edit::carets(shape, text)) {
                            line.clusters = carets.clusters;
                        }
                    }
                    lines
                }
                // Shaped directly: one line, unbroken, as it was read.
                None => {
                    let line = LineFacts {
                        advance: entry.advances.glyphs.iter().map(|g| g.advance).sum(),
                        hanging_advance: 0.0,
                        reason: BreakReason::None,
                        range: 0..text.len(),
                    };
                    extract_lines(
                        std::iter::once(line),
                        &entry.advances,
                        fallback,
                        None,
                        ranged.objects,
                    )
                }
            };
            (lines, (entry.advance, entry.natural_width), entry.rtl)
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
            advance: ceil64(advance),
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
        let Some(mut line) = extract(&shape, fallback, None, &part.objects, None)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_on_macos_takes_the_optical_size_of_its_size() {
        let Some(FontVariations::List(v)) = optical_size(Platform::MacOs, 13.0) else {
            panic!("macOS sets an optical size");
        };
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].tag, Tag::new(b"opsz"));
        assert_eq!(v[0].value, 13.0);
        assert!(optical_size(Platform::Other, 13.0).is_none());
        assert!(optical_size(Platform::Windows, 13.0).is_none());
    }

    #[test]
    fn qts_name_for_the_macos_system_font_is_system_ui() {
        assert_eq!(
            generic_family(".AppleSystemUIFont"),
            Some(GenericFamily::SystemUi)
        );
        assert_eq!(generic_family("system-ui"), Some(GenericFamily::SystemUi));
        assert_eq!(generic_family("sans-serif"), Some(GenericFamily::SansSerif));
        assert_eq!(generic_family("Sans Serif"), None);
        assert_eq!(generic_family("Helvetica"), None);
        let e = TextEngine::new();
        let stack = e.families.stack_of(".AppleSystemUIFont");
        assert!(matches!(
            stack[0],
            FontFamilyName::Generic(GenericFamily::SystemUi)
        ));
    }

    type Glyphs = Vec<(u64, u32, Vec<(u32, u32, u32)>, Vec<i16>, bool, u32)>;

    fn glyphs(e: &mut TextEngine, text: &str, style: &TextStyle) -> Glyphs {
        let l = e.layout(text, style, &LayoutParams::default());
        l.lines
            .iter()
            .flat_map(|line| &line.runs)
            .map(|r| {
                let g = &r.glyphs;
                // Each engine loads its own copy of a font, so the bytes say
                // which one it is.
                let mut h = FxHasher::default();
                g.font.data.as_ref().hash(&mut h);
                (
                    h.finish(),
                    g.font.index,
                    g.glyphs
                        .iter()
                        .map(|p| (p.id, p.x.to_bits(), p.y.to_bits()))
                        .collect(),
                    g.normalized_coords.clone(),
                    g.embolden,
                    g.skew.to_bits(),
                )
            })
            .collect()
    }

    #[test]
    fn a_pinned_family_shapes_as_the_whole_stack() {
        let texts = [
            "Row 12",
            "Hello, world!",
            "Grüße Ωμέγα Кириллица",
            "שלום עולם",
            "مرحبا بالعالم",
            "a\u{301}e\u{301}",
            "1\u{fe0f}\u{20e3} keycap",
            "☃ snow",
            "漢字 and Latin",
            "© 2026",
        ];
        let families = [
            "",
            "Liberation Sans",
            "monospace",
            "serif",
            "Not A Font, sans-serif",
        ];
        for system in [false, true] {
            let mut pinned = TextEngine::build(system);
            let mut whole = TextEngine::build(system);
            whole.families.pin = false;
            for family in families {
                for (weight, italic) in [(400.0, false), (700.0, true)] {
                    let style = TextStyle {
                        family: family.into(),
                        weight,
                        italic,
                        ..TextStyle::default()
                    };
                    for text in texts {
                        assert_eq!(
                            glyphs(&mut pinned, text, &style),
                            glyphs(&mut whole, text, &style),
                            "{text:?} in {family:?} {weight} {italic}, system fonts {system}"
                        );
                    }
                }
            }
            assert_eq!(whole.stats().shapes_pinned, 0);
            // Latin text in an installed family is the common case it serves.
            assert!(pinned.stats().shapes_pinned >= 20, "{:?}", pinned.stats());
        }
    }

    #[test]
    fn a_plain_text_given_as_one_style_shapes_as_one_built_by_property() {
        let texts = [
            "Row 12",
            "Hello, world!",
            "Grüße Ωμέγα Кириллица",
            "שלום עולם",
            "مرحبا بالعالم",
            "a\u{301}e\u{301}",
            "漢字 and Latin",
            "",
            "two\nlines  spaced",
        ];
        let styles = [
            TextStyle::default(),
            TextStyle {
                family: "serif".into(),
                weight: 700.0,
                italic: true,
                underline: true,
                strikeout: true,
                pixel_size: 23.0,
                ..TextStyle::default()
            },
            TextStyle {
                letter_spacing: 1.5,
                word_spacing: 4.0,
                ..TextStyle::default()
            },
        ];
        for system in [false, true] {
            let mut one = TextEngine::build(system);
            let mut built = TextEngine::build(system);
            built.families.one_style = false;
            for style in &styles {
                for text in texts {
                    for wrap in [None, Some(30.0)] {
                        let params = LayoutParams {
                            width: wrap,
                            wrap: if wrap.is_some() {
                                WrapMode::WrapAnywhere
                            } else {
                                WrapMode::NoWrap
                            },
                            ..LayoutParams::default()
                        };
                        // The lines' measures and each run's glyphs; a font
                        // by its bytes, as each engine loads its own copy.
                        let summary = |l: &TextLayout| {
                            let lines: Vec<_> = l
                                .lines
                                .iter()
                                .map(|line| {
                                    let runs: Vec<_> = line
                                        .runs
                                        .iter()
                                        .map(|r| {
                                            let mut h = FxHasher::default();
                                            r.glyphs.font.data.as_ref().hash(&mut h);
                                            let g: Vec<_> = r
                                                .glyphs
                                                .glyphs
                                                .iter()
                                                .map(|p| (p.id, p.x.to_bits(), p.y.to_bits()))
                                                .collect();
                                            (
                                                h.finish(),
                                                r.glyphs.font.index,
                                                r.glyphs.font_size.to_bits(),
                                                g,
                                                r.glyphs.normalized_coords.clone(),
                                                r.glyphs.embolden,
                                                r.glyphs.skew.to_bits(),
                                            )
                                        })
                                        .collect();
                                    let m = [line.top, line.baseline, line.width, line.height];
                                    (m.map(f64::to_bits), line.range.clone(), runs)
                                })
                                .collect();
                            let m = [l.advance, l.natural_width, l.content_width];
                            (m.map(f64::to_bits), lines)
                        };
                        assert_eq!(
                            summary(&one.layout(text, style, &params)),
                            summary(&built.layout(text, style, &params)),
                            "{text:?} {style:?} {wrap:?}, system fonts {system}"
                        );
                    }
                }
            }
        }
    }

    /// [`shape_direct`] shapes a text with the analysis of another that
    /// folds as it does, so the two have to analyse alike.
    #[test]
    fn a_printable_ascii_text_analyses_as_its_fold() {
        let mut analyzer = Analyzer::new();
        let (mut theirs, mut ours) = (Analysis::new(), Analysis::new());
        let mut folded = Vec::new();
        let mut check = |text: &str| {
            for word_break in [WordBreak::Normal, WordBreak::BreakAll] {
                fold(text.as_bytes(), word_break, &mut folded);
                let other = std::str::from_utf8(&folded[1..]).expect("ASCII");
                let config = LineBreakConfig {
                    word_break,
                    ..LineBreakConfig::default()
                };
                let line_break = [(0..text.len(), config)];
                let options = AnalysisOptions {
                    base_direction: BaseDirection::Auto,
                    line_break: &line_break,
                    break_spaces: &[],
                    line_break_override: None,
                };
                analyzer.analyze(text, &options, &mut theirs);
                analyzer.analyze(other, &options, &mut ours);
                assert_eq!(
                    theirs.char_info(),
                    ours.char_info(),
                    "{text:?} {word_break:?}"
                );
                assert_eq!(theirs.bidi_levels(), ours.bidi_levels(), "{text:?}");
                assert_eq!(theirs.paragraph_level(), ours.paragraph_level(), "{text:?}");
            }
        };
        let printable: Vec<char> = (0x20u8..0x7f).map(char::from).collect();
        for &a in &printable {
            check(&a.to_string());
            for &b in &printable {
                check(&format!("{a}{b}"));
            }
        }
        // Every class three deep: some letters of each case and digits,
        // and every other character as itself.
        let classes: Vec<char> = printable
            .iter()
            .copied()
            .filter(|c| !c.is_ascii_alphanumeric() || "aZq09".contains(*c))
            .collect();
        for &a in &classes {
            for &b in &classes {
                for &c in &classes {
                    check(&format!("{a}{b}{c}"));
                }
            }
        }
        // Longer texts, from a fixed sequence so a failure repeats.
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..2000 {
            let len = 4 + (next() % 40) as usize;
            let text: String = (0..len)
                .map(|_| printable[(next() % printable.len() as u64) as usize])
                .collect();
            check(&text);
        }
    }
}
