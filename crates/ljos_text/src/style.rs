//! What a caller asks for: the font and the layout constraints.

use std::hash::{Hash, Hasher};
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
