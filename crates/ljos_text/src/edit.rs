//! A layout an editing item can put a cursor in.
//!
//! [`crate::TextLayout`] is for showing text; this one also knows where every
//! character is, so a cursor position becomes a place on screen and a point
//! on screen becomes a cursor position. It keeps a line break at the end of
//! the text as an empty last line, where the cursor then stands, and never
//! elides.
//!
//! Positions here are byte offsets into the laid-out string. The item
//! converts from and to the UTF-16 positions QML counts in.

use std::ops::Range;

use ljos_render::kurbo::Rect;

use crate::layout::LineRun;
use crate::style::WrapMode;

/// What shapes an editing layout besides the text and its font.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EditParams {
    /// The width lines wrap at, when the item has one of its own.
    pub width: Option<f64>,
    /// Line breaking. Without a width nothing wraps.
    pub wrap: WrapMode,
}

/// One character as the cursor sees it: a grapheme cluster and where it sits
/// on its line.
#[derive(Clone, Debug, PartialEq)]
pub struct CaretCluster {
    /// Bytes of the text.
    pub range: Range<usize>,
    /// Left edge, from the line's left edge.
    pub x0: f64,
    /// Right edge.
    pub x1: f64,
    /// The character reads right to left: its start is its right edge.
    pub rtl: bool,
}

impl CaretCluster {
    /// The edge the cursor stands at before the character.
    fn leading(&self) -> f64 {
        if self.rtl { self.x1 } else { self.x0 }
    }

    /// The edge the cursor stands at after the character.
    fn trailing(&self) -> f64 {
        if self.rtl { self.x0 } else { self.x1 }
    }
}

/// Glyphs of one font and direction on a line, and which character each
/// belongs to (so a selection can colour its own).
#[derive(Clone, Debug)]
pub struct EditRun {
    pub run: LineRun,
    /// For every glyph of the run, the byte its character starts at.
    pub glyph_bytes: Vec<usize>,
}

/// One line of an editing layout.
#[derive(Clone, Debug)]
pub struct EditLine {
    /// Bytes of the text on this line, its line break included.
    pub range: Range<usize>,
    /// Where the line's own text ends: before the line break, and before the
    /// spaces a wrapped line leaves hanging.
    pub end: usize,
    /// Top of the line, from the layout's top.
    pub top: f64,
    /// Height of the line, which is the height of a cursor on it.
    pub height: f64,
    /// Baseline, from the layout's top.
    pub baseline: f64,
    /// Width of the line.
    pub width: f64,
    /// The characters, left to right.
    pub clusters: Vec<CaretCluster>,
    /// The glyph runs, left to right.
    pub runs: Vec<EditRun>,
}

/// Where a cursor stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    /// Index of the line.
    pub line: usize,
    /// From the line's left edge.
    pub x: f64,
    /// Top of the line.
    pub top: f64,
    /// Height of the line.
    pub height: f64,
}

/// A layout with a place for every cursor position.
#[derive(Clone, Debug)]
pub struct EditLayout {
    /// The lines; never empty, since empty text still has a line.
    pub lines: Vec<EditLine>,
    /// Width of the widest line when nothing wraps.
    pub natural_width: f64,
    /// Width of the widest line.
    pub content_width: f64,
    /// Height of all lines.
    pub content_height: f64,
    /// Distance from the top to the first line's baseline.
    pub baseline: f64,
    /// The text reads right to left.
    pub rtl: bool,
    /// Length of the text in bytes.
    pub len: usize,
}

impl EditLayout {
    /// The line a position is on. A position where a line wraps is on the
    /// line that starts there.
    pub fn line_of(&self, byte: usize) -> usize {
        self.lines
            .iter()
            .rposition(|line| line.range.start <= byte)
            .unwrap_or(0)
    }

    /// Where the cursor stands at a position.
    pub fn caret(&self, byte: usize) -> Caret {
        let index = self.line_of(byte);
        let line = &self.lines[index];
        let x = line
            .clusters
            .iter()
            .find(|c| c.range.start == byte)
            .map(CaretCluster::leading)
            .or_else(|| {
                line.clusters
                    .iter()
                    .find(|c| c.range.end == byte)
                    .map(CaretCluster::trailing)
            })
            .or_else(|| {
                // Inside a character (a position an input method may name):
                // in proportion.
                line.clusters
                    .iter()
                    .find(|c| c.range.contains(&byte))
                    .map(|c| {
                        let part = (byte - c.range.start) as f64 / c.range.len() as f64;
                        c.leading() + (c.trailing() - c.leading()) * part
                    })
            })
            .or_else(|| {
                // Past the line's text (in its hanging spaces or its line
                // break): after its last character.
                line.clusters
                    .iter()
                    .max_by_key(|c| c.range.end)
                    .filter(|c| byte >= c.range.end)
                    .map(CaretCluster::trailing)
            })
            .unwrap_or(0.0);
        Caret {
            line: index,
            x,
            top: line.top,
            height: line.height,
        }
    }

    /// The line at a height: the first above the text, the last below it.
    pub fn line_at(&self, y: f64) -> usize {
        self.lines
            .iter()
            .rposition(|line| line.top <= y)
            .unwrap_or(0)
    }

    /// The position at `x` on a line. The nearest gap between two
    /// characters; with `on_character` the start of the character under `x`.
    pub fn hit_line(&self, index: usize, x: f64, on_character: bool) -> usize {
        let Some(line) = self.lines.get(index) else {
            return self.len;
        };
        let (Some(first), Some(last)) = (line.clusters.first(), line.clusters.last()) else {
            return line.range.start;
        };
        let before = |c: &CaretCluster| if c.rtl { c.range.end } else { c.range.start };
        let after = |c: &CaretCluster| if c.rtl { c.range.start } else { c.range.end };
        let found = if x < first.x0 {
            before(first)
        } else if x >= last.x1 {
            after(last)
        } else {
            line.clusters
                .iter()
                .find(|c| x < c.x1)
                .map_or(after(last), |c| {
                    if on_character {
                        c.range.start
                    } else if x < (c.x0 + c.x1) / 2.0 {
                        before(c)
                    } else {
                        after(c)
                    }
                })
        };
        // The spaces that hang off a wrapped line belong to no place on it.
        found.min(line.end.max(line.range.start))
    }

    /// The position at a point. Above the text is its start, below its end.
    pub fn hit(&self, x: f64, y: f64, on_character: bool) -> usize {
        if y < 0.0 {
            return 0;
        }
        if y >= self.content_height {
            return self.len;
        }
        self.hit_line(self.line_at(y), x, on_character)
    }

    /// The boxes that cover a range of text, one or more per line it is on,
    /// x from each line's left edge.
    pub fn range_rects(&self, range: Range<usize>) -> Vec<(usize, Rect)> {
        let mut out = Vec::new();
        if range.is_empty() {
            return out;
        }
        for (index, line) in self.lines.iter().enumerate() {
            for (x0, x1) in line.range_extents(&range) {
                out.push((index, line_rect(line, x0, x1)));
            }
        }
        out
    }
}

impl EditLine {
    /// The stretches of the line a range of text covers: left and right
    /// edge of each, from the line's left edge. More than one where text
    /// of both directions is in the range.
    pub fn range_extents(&self, range: &Range<usize>) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        if self.range.start >= range.end || self.range.end <= range.start {
            return out;
        }
        let mut open: Option<(f64, f64)> = None;
        for c in &self.clusters {
            let inside = c.range.start >= range.start && c.range.end <= range.end;
            match (&mut open, inside) {
                (Some((_, x1)), true) if (c.x0 - *x1).abs() < 1e-3 => *x1 = c.x1,
                (_, true) => out.extend(open.replace((c.x0, c.x1))),
                (_, false) => out.extend(open.take()),
            }
        }
        out.extend(open);
        out
    }
}

fn line_rect(line: &EditLine, x0: f64, x1: f64) -> Rect {
    Rect::new(x0, line.top, x1, line.top + line.height)
}
