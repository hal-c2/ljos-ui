//! What a layout reports and what a backend draws.

use std::sync::Arc;

use ljos_render::kurbo::Rect;
use ljos_render::{Decorations, GlyphRun};

/// One run of glyphs on a line: one font, one direction.
#[derive(Clone, Debug)]
pub struct LineRun {
    /// The glyphs. Positions are relative to the line's left edge and the
    /// layout's top, so a drawing origin only adds the alignment offset.
    pub glyphs: Arc<GlyphRun>,
    /// Underline and strike-through bars, in the same coordinates.
    pub decorations: Decorations,
}

/// One laid-out line.
#[derive(Clone, Debug)]
pub struct Line {
    /// Top of the line, from the layout's top.
    pub top: f64,
    /// Baseline, from the layout's top.
    pub baseline: f64,
    /// Width of the line. A wrapped line does not count its trailing spaces.
    pub width: f64,
    /// The line was stretched to the layout's width, so aligning it does nothing.
    pub justified: bool,
    /// The glyph runs, in visual order.
    pub runs: Vec<LineRun>,
}

/// A finished layout: numbers for the item, glyph runs for the renderer.
#[derive(Clone, Debug)]
pub struct TextLayout {
    /// The lines that are shown.
    pub lines: Vec<Line>,
    /// Width of the widest line when nothing wraps or elides (Qt's
    /// `implicitWidth`).
    pub natural_width: f64,
    /// Width of the widest shown line (Qt's `contentWidth`).
    pub content_width: f64,
    /// Height of the shown lines (Qt's `contentHeight`).
    pub content_height: f64,
    /// Height the text wants: every line `max_lines` allows, whatever the
    /// item's height is (Qt's `implicitHeight`).
    pub implicit_height: f64,
    /// Lines were dropped or text was replaced by an ellipsis.
    pub truncated: bool,
    /// Distance from the top to the first line's baseline.
    pub baseline: f64,
    /// The text reads right to left: its first strongly directed character is
    /// a right-to-left one. Such text aligns to the right unless told otherwise.
    pub rtl: bool,
}

impl TextLayout {
    /// Number of shown lines; never 0, since empty text still has a line.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Number of glyph runs in the layout.
    pub fn run_count(&self) -> usize {
        self.lines.iter().map(|l| l.runs.len()).sum()
    }

    /// Number of glyphs in the layout.
    pub fn glyph_count(&self) -> usize {
        self.lines
            .iter()
            .flat_map(|l| &l.runs)
            .map(|r| r.glyphs.glyphs.len())
            .sum()
    }
}

/// Metrics of the font a style resolves to, in logical pixels.
///
/// Rounded as Qt reports them, measured on Qt 6.11: ascent and descent down
/// to 1/64 pixel, the other heights and widths to the nearest 1/64.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FontMetrics {
    /// Baseline to the top of the tallest letters.
    pub ascent: f64,
    /// Baseline to the bottom of the lowest letters.
    pub descent: f64,
    /// `ascent + descent`.
    pub height: f64,
    /// Height of a line of text: `height` rounded up to a whole pixel.
    pub line_height: f64,
    /// Extra gap between lines the font asks for. Lines do not use it.
    pub leading: f64,
    /// Height of a lower-case x.
    pub x_height: f64,
    /// Height of a capital letter.
    pub cap_height: f64,
    /// Top of the underline, below the baseline.
    pub underline_position: f64,
    /// Top of the strike-through, above the baseline.
    pub strikeout_position: f64,
    /// Thickness of the underline, at least one pixel.
    pub line_width: f64,
    /// Average advance of the font's glyphs, as the font states it.
    pub average_char_width: f64,
    /// The widest advance of any glyph, in whole pixels.
    pub max_char_width: f64,
    /// The smallest left side bearing of any glyph: negative when some glyph
    /// has ink to the left of its origin.
    pub min_left_bearing: f64,
    /// The smallest right side bearing of any glyph: negative when some glyph
    /// has ink beyond its advance.
    pub min_right_bearing: f64,
}

/// What a text measures when it is set on one line (`FontMetrics.boundingRect`
/// and its relatives in Qt). The origin is the start of the baseline, and y
/// points down, so ink above the baseline has negative y.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextBounds {
    /// How far the pen moves. A line break in the text moves it nowhere.
    pub advance: f64,
    /// The ink, widened to whole pixels and at least as tall as the font
    /// (`ascent` above the baseline, `descent` below).
    pub bounding: Rect,
    /// The ink alone, widened to whole pixels. Ink that stays above the
    /// baseline still reaches down to it, as in Qt.
    pub tight: Rect,
}
