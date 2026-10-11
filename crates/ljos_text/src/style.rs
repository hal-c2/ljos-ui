//! What a caller asks for: the font and the layout constraints.

use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

/// A resolved font request, in logical pixels.
#[derive(Clone, Debug)]
pub struct TextStyle {
    /// Family name. Empty means the engine's default family.
    pub family: Arc<str>,
    /// Font size in logical pixels.
    pub pixel_size: f32,
    /// Weight on the 1 to 1000 scale (400 normal, 700 bold).
    pub weight: f32,
    /// Italic. Slanted synthetically when the family has no italic face.
    pub italic: bool,
    /// Draw an underline.
    pub underline: bool,
    /// Draw a strike-through.
    pub strikeout: bool,
    /// Extra space after every character, in pixels.
    pub letter_spacing: f32,
    /// Extra space at every word space, in pixels.
    pub word_spacing: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            family: Arc::from(""),
            pixel_size: 14.0,
            weight: 400.0,
            italic: false,
            underline: false,
            strikeout: false,
            letter_spacing: 0.0,
            word_spacing: 0.0,
        }
    }
}

impl TextStyle {
    /// The default style at another size.
    pub fn sized(pixel_size: f32) -> Self {
        Self {
            pixel_size,
            ..Self::default()
        }
    }

    fn bits(&self) -> (&str, [u32; 4], [bool; 3]) {
        (
            &self.family,
            [
                self.pixel_size.to_bits(),
                self.weight.to_bits(),
                self.letter_spacing.to_bits(),
                self.word_spacing.to_bits(),
            ],
            [self.italic, self.underline, self.strikeout],
        )
    }
}

// Styles are cache keys, so floats compare by bit pattern: a NaN equals itself
// and the cache cannot grow without bound on one.
impl PartialEq for TextStyle {
    fn eq(&self, other: &Self) -> bool {
        self.bits() == other.bits()
    }
}

impl Eq for TextStyle {}

impl Hash for TextStyle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bits().hash(state);
    }
}

/// A part of a text set in another font: what `<b>`, a heading or a
/// highlighted keyword is to the layout. The colour of a part is not the
/// engine's business; `tag` comes back on the glyph runs
/// ([`crate::LineRun::tag`]) so whoever draws them knows which part they are.
#[derive(Clone, Debug)]
pub struct StyleSpan {
    /// Bytes of the text. Spans do not overlap; text outside every span is
    /// in the layout's own style and has tag 0.
    pub range: Range<usize>,
    /// Any number but 0 the caller can find the part by.
    pub tag: u32,
    /// Font size in logical pixels.
    pub pixel_size: f32,
    /// Weight on the 1 to 1000 scale.
    pub weight: f32,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    /// Another family; none keeps the layout's.
    pub family: Option<Arc<str>>,
}

impl StyleSpan {
    /// A span that changes nothing but the tag, to start from.
    pub fn of(base: &TextStyle, range: Range<usize>, tag: u32) -> Self {
        Self {
            range,
            tag,
            pixel_size: base.pixel_size,
            weight: base.weight,
            italic: base.italic,
            underline: base.underline,
            strikeout: base.strikeout,
            family: None,
        }
    }

    /// The whole style of the span's text, in a layout whose own is `base`.
    pub fn style(&self, base: &TextStyle) -> TextStyle {
        TextStyle {
            family: self.family.clone().unwrap_or_else(|| base.family.clone()),
            pixel_size: self.pixel_size,
            weight: self.weight,
            italic: self.italic,
            underline: self.underline,
            strikeout: self.strikeout,
            letter_spacing: base.letter_spacing,
            word_spacing: base.word_spacing,
        }
    }

    fn bits(&self) -> (Range<usize>, u32, [u32; 2], [bool; 3], Option<&str>) {
        (
            self.range.clone(),
            self.tag,
            [self.pixel_size.to_bits(), self.weight.to_bits()],
            [self.italic, self.underline, self.strikeout],
            self.family.as_deref(),
        )
    }
}

// Cache keys, like `TextStyle`: floats compare by bit pattern.
impl PartialEq for StyleSpan {
    fn eq(&self, other: &Self) -> bool {
        self.bits() == other.bits()
    }
}

impl Eq for StyleSpan {}

impl Hash for StyleSpan {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bits().hash(state);
    }
}

/// Where an object stands on a line taller or lower than it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BoxAlign {
    /// Its bottom on the bottom of the line.
    #[default]
    Bottom,
    /// Centred on the line.
    Middle,
    /// Its top at the top of the line.
    Top,
    /// Standing on the baseline, as a letter without a descender does.
    Baseline,
}

/// Something that is not text and stands in a line of it, such as an image.
/// The layout leaves room for it and reports where
/// ([`crate::Line::boxes`]); drawing it is the caller's business.
#[derive(Clone, Copy, Debug)]
pub struct InlineObject {
    /// The caller's name for it.
    pub id: u64,
    /// The byte of the text it stands before.
    pub index: usize,
    pub width: f32,
    pub height: f32,
    pub align: BoxAlign,
}

impl InlineObject {
    fn bits(&self) -> (u64, usize, [u32; 2], BoxAlign) {
        (
            self.id,
            self.index,
            [self.width.to_bits(), self.height.to_bits()],
            self.align,
        )
    }
}

impl PartialEq for InlineObject {
    fn eq(&self, other: &Self) -> bool {
        self.bits() == other.bits()
    }
}

impl Eq for InlineObject {}

impl Hash for InlineObject {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bits().hash(state);
    }
}

/// What a ranged layout has besides its text and its own style.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ranged<'a> {
    /// Sorted by start, not overlapping.
    pub spans: &'a [StyleSpan],
    pub objects: &'a [InlineObject],
    /// Every line is at least as tall as a line of the text's own style,
    /// whatever its glyphs are set in. Small capitals are smaller letters
    /// in a line of the font, where a line of smaller markup is lower.
    pub strut: bool,
}

impl Ranged<'_> {
    /// Nothing: plain text.
    pub const NONE: Ranged<'static> = Ranged {
        spans: &[],
        objects: &[],
        strut: false,
    };

    /// `text` in small capitals: the text with every small letter replaced
    /// by its capital, and the spans that set those capitals at the smaller
    /// size. Measured on Qt 6.11: seven tenths of the font's size, as a
    /// whole number of pixels. A letter whose capital is not one character
    /// of as many UTF-16 units stays as it is, so positions in the two
    /// texts are the same.
    pub fn small_caps(text: &str, base: &TextStyle) -> (String, Vec<StyleSpan>) {
        let size = (base.pixel_size * 0.7).round().max(1.0);
        let mut drawn = String::with_capacity(text.len());
        let mut spans: Vec<StyleSpan> = Vec::new();
        for c in text.chars() {
            let mut capitals = c.to_uppercase();
            let capital = match (capitals.next(), capitals.next()) {
                (Some(u), None) if u != c && u.len_utf16() == c.len_utf16() => Some(u),
                _ => None,
            };
            let start = drawn.len();
            drawn.push(capital.unwrap_or(c));
            if capital.is_none() {
                continue;
            }
            match spans.last_mut() {
                Some(last) if last.range.end == start => last.range.end = drawn.len(),
                _ => spans.push(StyleSpan {
                    pixel_size: size,
                    ..StyleSpan::of(base, start..drawn.len(), SMALL_CAPS_TAG)
                }),
            }
        }
        (drawn, spans)
    }
}

/// Any tag but 0, which is the text outside every span: nobody draws small
/// capitals in another colour than the text.
const SMALL_CAPS_TAG: u32 = 1;

/// Where lines may break (Qt's `Text.wrapMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WrapMode {
    /// Only at line breaks in the text.
    #[default]
    NoWrap,
    /// Between words; a word wider than the line overflows.
    WordWrap,
    /// Wherever the line is full, also inside a word.
    WrapAnywhere,
    /// Between words when possible, inside a word otherwise.
    Wrap,
}

/// Where text that does not fit is cut (Qt's `Text.elide`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ElideMode {
    /// Not at all.
    #[default]
    None,
    /// At the start.
    Left,
    /// In the middle.
    Middle,
    /// At the end.
    Right,
}

/// The distance from one line to the next (Qt's `lineHeight` with `lineHeightMode`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeight {
    /// A multiple of the font's line height.
    Proportional(f64),
    /// A number of pixels.
    Fixed(f64),
}

impl Default for LineHeight {
    fn default() -> Self {
        Self::Proportional(1.0)
    }
}

impl LineHeight {
    fn key(self) -> (bool, u64) {
        match self {
            Self::Proportional(f) => (false, f.to_bits()),
            Self::Fixed(px) => (true, px.to_bits()),
        }
    }

    /// Distance to the next line, for a line that is `natural` tall on its
    /// own. A value that makes no sense leaves the line as it is.
    pub(crate) fn advance(self, natural: f64) -> f64 {
        let advance = match self {
            Self::Proportional(f) => natural * f,
            Self::Fixed(px) => px,
        };
        if advance.is_finite() && advance >= 0.0 {
            advance
        } else {
            natural
        }
    }
}

/// Everything that shapes a layout besides the text and its font.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutParams {
    /// The width text has to fit, when the item has one of its own. Wrapping
    /// and eliding do nothing without it.
    pub width: Option<f64>,
    /// The height text has to fit, when the item has one of its own. Only
    /// `ElideRight` looks at it.
    pub height: Option<f64>,
    /// Line breaking.
    pub wrap: WrapMode,
    /// Eliding.
    pub elide: ElideMode,
    /// Keep at most this many lines; 0 keeps all.
    pub max_lines: u32,
    /// Distance between lines.
    pub line_height: LineHeight,
    /// Stretch wrapped lines to the full width (Qt's `AlignJustify`).
    pub justify: bool,
}

/// [`LayoutParams`] as a cache key: widths in 1/64 pixel, and constraints that
/// cannot change the result left out, so equal layouts share one entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ParamsKey {
    width: Option<i32>,
    height: Option<i32>,
    wrap: WrapMode,
    elide: ElideMode,
    max_lines: u32,
    line_height: (bool, u64),
    justify: bool,
}

impl LayoutParams {
    /// The width, if it can do anything: a positive, finite number.
    pub(crate) fn usable_width(&self) -> Option<f64> {
        self.width.filter(|w| w.is_finite() && *w > 0.0)
    }

    /// The height that limits lines: only `ElideRight` with a width cuts by height.
    pub(crate) fn usable_height(&self) -> Option<f64> {
        if self.elide != ElideMode::Right || self.usable_width().is_none() {
            return None;
        }
        self.height.filter(|h| h.is_finite() && *h >= 0.0)
    }

    pub(crate) fn key(&self) -> ParamsKey {
        let q = |v: f64| (v * 64.0).round().clamp(0.0, f64::from(i32::MAX)) as i32;
        let constrains = self.wrap != WrapMode::NoWrap || self.elide != ElideMode::None;
        let width = self.usable_width().filter(|_| constrains).map(q);
        ParamsKey {
            width,
            height: self.usable_height().map(q),
            wrap: if width.is_some() {
                self.wrap
            } else {
                WrapMode::NoWrap
            },
            elide: if width.is_some() {
                self.elide
            } else {
                ElideMode::None
            },
            max_lines: self.max_lines,
            line_height: self.line_height.key(),
            justify: self.justify && width.is_some() && self.wrap != WrapMode::NoWrap,
        }
    }
}
