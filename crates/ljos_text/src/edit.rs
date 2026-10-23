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
    /// Where the cursor stands on the line when it has no characters: the
    /// left of a cell's text in a table, the right of a table beside one.
    pub left: f64,
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

/// How a paragraph of rich text stands among the others: the room above
/// and below it, how far it stands in, and its list marker.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditBlock {
    /// Between two paragraphs the larger of the one's bottom and the
    /// other's top counts, and at the ends of the text none does, as for a
    /// rich `Text`.
    pub margin_top: f64,
    pub margin_bottom: f64,
    pub indent: f64,
    pub indent_right: f64,
    /// The bullet or number of a list item, drawn in the indent.
    pub marker: Option<std::sync::Arc<str>>,
    /// The paragraph is a row of a table, its cells set apart by tabs.
    pub table: Option<EditTable>,
}

/// How a row of a table stands among the others, as a rich `Text` sets
/// its tables (`ljos_text::rich`): every cell laid out on its own, in
/// columns as wide as their widest text or as the width shares out, apart
/// by the spacing and each cell's frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditTable {
    /// Which row of its table the paragraph is; 0 starts a table.
    pub row: usize,
    /// Width of the frame around the table and around every cell.
    pub border: f64,
    /// Space between cells, and between them and the frame.
    pub spacing: f64,
    /// Space between a cell's frame and its text.
    pub padding: f64,
    /// The colour behind the whole table, as written (the row starting
    /// the table says).
    pub background: Option<std::sync::Arc<str>>,
    /// The colour behind each cell of the row, inside its padding.
    pub cells: Vec<Option<std::sync::Arc<str>>>,
}

/// A table of an [`EditLayout`]: where its rows and columns are, for a
/// click to find its cell and the cursor to go up and down in it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditGrid {
    /// Its lines, every cell's in the order of the text.
    pub lines: Range<usize>,
    /// Top and bottom of the table, from the layout's top.
    pub top: f64,
    pub bottom: f64,
    /// Its right edge, from its lines' left edge.
    pub right: f64,
    /// The top of each row's text, from the layout's top.
    pub rows: Vec<f64>,
    /// The left of each column's text, from its lines' left edge.
    pub columns: Vec<f64>,
    /// The cells of each row.
    pub cells: Vec<Vec<EditCell>>,
    /// The position after the table.
    pub after: usize,
}

/// A cell of an [`EditGrid`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditCell {
    /// Its lines.
    pub lines: Range<usize>,
    /// Bytes of its text.
    pub bytes: Range<usize>,
    /// The bottom of its text, from the layout's top.
    pub bottom: f64,
}

/// Where a paragraph of an [`EditLayout`] laid out from [`EditBlock`]s is.
#[derive(Clone, Debug)]
pub struct PlacedBlock {
    /// Its first line.
    pub first_line: usize,
    pub indent: f64,
    pub indent_right: f64,
    /// The marker, laid out on its own: its left edge from the paragraph's
    /// text's left edge (so less than nothing), and its layout, whose
    /// baseline goes on the paragraph's first.
    pub marker: Option<(f64, std::rc::Rc<EditLayout>)>,
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
    /// The paragraphs, when the text was laid out from [`EditBlock`]s;
    /// empty otherwise.
    pub blocks: Vec<PlacedBlock>,
    /// The frames of tables, as wide as the border: each with the line
    /// whose left edge its x counts from (the table's first), y from the
    /// layout's top.
    pub rules: Vec<(usize, Rect)>,
    /// The backgrounds of tables and their cells, counted as the frames
    /// are, with their colours as written.
    pub fills: Vec<(usize, Rect, std::sync::Arc<str>)>,
    /// The tables.
    pub tables: Vec<EditGrid>,
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

    /// The paragraph line `index` is in, when it was laid out from
    /// [`EditBlock`]s.
    pub fn block_of_line(&self, index: usize) -> Option<&PlacedBlock> {
        let at = self.blocks.partition_point(|b| b.first_line <= index);
        at.checked_sub(1).map(|i| &self.blocks[i])
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
            .unwrap_or(line.left);
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
                    // Measured: exactly halfway is before the character
                    // (features/text-input/pointer-halfway.feature).
                    } else if x <= (c.x0 + c.x1) / 2.0 {
                        before(c)
                    } else {
                        after(c)
                    }
                })
        };
        // The spaces that hang off a wrapped line belong to no place on it.
        found.min(line.end.max(line.range.start))
    }

    /// The bytes of the character under `x` on a line, if there is one: a
    /// character holds its right edge and not its left. Past either end of
    /// the line, and on its line break, there is none.
    pub fn cluster_at(&self, index: usize, x: f64) -> Option<Range<usize>> {
        let line = self.lines.get(index)?;
        line.clusters
            .iter()
            .find(|c| c.x0 < x && x <= c.x1 && c.range.start < line.end)
            .map(|c| c.range.clone())
    }

    /// The position at a point. Above the text is its start, below its end.
    pub fn hit(&self, x: f64, y: f64, on_character: bool) -> usize {
        if y < 0.0 {
            return 0;
        }
        if y >= self.content_height {
            return self.len;
        }
        if let Some(grid) = self.grid_at(y) {
            return self.hit_grid(grid, x, y, on_character);
        }
        self.hit_line(self.line_at(y), x, on_character)
    }

    /// The table at a height: a line's bottom edge is the line's, so its
    /// top edge is not the table's (measured on Qt 6.11).
    pub fn grid_at(&self, y: f64) -> Option<&EditGrid> {
        self.tables.iter().find(|g| g.top < y && y <= g.bottom)
    }

    /// The position at a point of a table, `x` from its lines' left edge,
    /// as Qt 6.11 puts it: in the cell whose text starts last before the
    /// point, at the start of its text above it and its end below it, and
    /// right of the table after the table.
    pub fn hit_grid(&self, grid: &EditGrid, x: f64, y: f64, on_character: bool) -> usize {
        if x > grid.right {
            return grid.after;
        }
        let row = grid.rows.iter().rposition(|top| *top < y).unwrap_or(0);
        let Some(cells) = grid.cells.get(row).filter(|c| !c.is_empty()) else {
            return grid.after;
        };
        let column = grid.columns.iter().rposition(|left| *left < x).unwrap_or(0);
        let cell = &cells[column.min(cells.len() - 1)];
        if y < grid.rows[row] {
            return cell.bytes.start;
        }
        if y >= cell.bottom {
            return cell.bytes.end;
        }
        let line = (cell.lines.clone())
            .rfind(|i| self.lines[*i].top <= y)
            .unwrap_or(cell.lines.start);
        self.hit_line(line, x, on_character)
    }

    /// The table, row and column of a line in a cell.
    pub fn cell_of_line(&self, line: usize) -> Option<(&EditGrid, usize, usize)> {
        let grid = self.tables.iter().find(|g| g.lines.contains(&line))?;
        grid.cells.iter().enumerate().find_map(|(r, row)| {
            let c = row.iter().position(|c| c.lines.contains(&line))?;
            Some((grid, r, c))
        })
    }

    /// The line a cursor goes to from `line` going down or up: in a table
    /// the one below or above in the same column, out of the table the
    /// paragraph after or before it, and into one its first or last cell,
    /// as in Qt 6.11.
    pub fn vertical(&self, line: usize, down: bool) -> Option<usize> {
        let Some((grid, r, c)) = self.cell_of_line(line) else {
            return if down {
                Some(line + 1).filter(|l| *l < self.lines.len())
            } else {
                line.checked_sub(1)
            };
        };
        let cell = &grid.cells[r][c];
        let near = |row: &Vec<EditCell>| row.get(c).or(row.last()).map(|c| c.lines.clone());
        if down {
            if line + 1 < cell.lines.end {
                return Some(line + 1);
            }
            match grid.cells.get(r + 1).and_then(near) {
                Some(lines) => Some(lines.start),
                None => Some(grid.lines.end).filter(|l| *l < self.lines.len()),
            }
        } else {
            if line > cell.lines.start {
                return Some(line - 1);
            }
            match r.checked_sub(1).and_then(|r| near(&grid.cells[r])) {
                Some(lines) => Some(lines.end - 1),
                None => grid.lines.start.checked_sub(1),
            }
        }
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
