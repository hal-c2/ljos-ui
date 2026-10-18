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
//!   itself, so the item can align it to the right;
//! * small capitals are a small letter's capital at seven tenths of the size,
//!   in a line as tall as the font's ([`Ranged::small_caps`]).
//!
//! Text whose parts differ in font, with objects in its lines, is a ranged
//! layout ([`TextEngine::layout_ranged`]). [`rich`] builds documents on it:
//! blocks read from plain text, Qt's `StyledText`, its HTML subset or
//! Markdown, each keeping its own layout so a change lays out only the
//! blocks it touches.
//!
//! Layout happens in logical pixels without hinting, so a layout does not
//! depend on the screen's scale factor and one cache entry serves every scale.
//!
//! Nothing here runs on its own: caches age in [`TextEngine::end_frame`], which
//! the scene calls from frames that run anyway.

mod bounds;
mod cache;
mod direction;
mod edit;
mod elide;
mod engine;
mod face;
mod fonts;
mod layout;
mod measure;
mod model;
pub mod rich;
mod snap;
mod style;
mod substitute;

pub use direction::reads_right_to_left;
pub use edit::{Caret, CaretCluster, EditLayout, EditLine, EditParams, EditRun};
pub use engine::{CacheStats, TextEngine};
pub use fonts::{BUNDLED_FAMILY, FontError, RegisteredFace, bundled_regular};
pub use layout::{FontMetrics, Line, LineRun, PlacedBox, TextBounds, TextLayout};
pub use model::{
    Checkpoint, DEFAULT_MAX_LENGTH, TextModel, byte_to_utf16, utf16_len, utf16_to_byte,
};
pub use style::{
    BoxAlign, ElideMode, InlineObject, LayoutParams, LineHeight, Ranged, StyleSpan, TextStyle,
    WrapMode,
};
