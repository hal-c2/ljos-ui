//! Text layout for Ljós on Parley (ARCHITECTURE §7.5). Never sees egui.
//!
//! [`TextEngine`] owns the fonts and two caches. [`TextEngine::layout`] turns a
//! string, a [`TextStyle`] and [`LayoutParams`] into a [`TextLayout`]: the
//! numbers a `Text` item reports (content size, line count, baseline) and the
//! glyph runs the render backends draw.
//!
//! Qt semantics are built in here, measured on Qt 6.11 as a black box:
//!
//! * ascent and descent come from the set of metrics Qt picks when a font's
//!   tables disagree (see `face.rs`);
//! * a glyph's advance is cut down to 1/64 pixel and what shaping adds to it
//!   (kerning, letter and word spacing) is rounded to 1/64 on its own, so
//!   widths are Qt's to the last bit at whole pixel sizes (see `snap.rs`);
//! * a line is `ceil(ascent + descent)` tall; the font's line gap is ignored;
//! * `lineHeight` changes the distance between lines, never the baseline of
//!   the first one, and the last line keeps the height of its font;
//! * a line that wraps drops its trailing spaces from the content width, the
//!   last line of a paragraph keeps them;
//! * eliding needs a width; `ElideRight` alone also cuts wrapped text, at
//!   `maximumLineCount` or at the item's height; `ElideMiddle` keeps as many
//!   characters at one end as at the other;
//! * right-to-left text is laid out by the Unicode bidi algorithm and reports
//!   itself, so the item can align it to the right.
//!
//! Layout happens in logical pixels without hinting, so a layout does not
//! depend on the screen's scale factor and one cache entry serves every scale.
//!
//! Nothing here runs on its own: caches age in [`TextEngine::end_frame`], which
//! the scene calls from frames that run anyway.

mod bounds;
mod cache;
mod direction;
mod elide;
mod engine;
mod face;
mod fonts;
mod layout;
mod snap;
mod style;

pub use direction::reads_right_to_left;
pub use engine::{CacheStats, TextEngine};
pub use fonts::{BUNDLED_FAMILY, FontError};
pub use layout::{FontMetrics, Line, LineRun, TextBounds, TextLayout};
pub use style::{ElideMode, LayoutParams, LineHeight, TextStyle, WrapMode};
