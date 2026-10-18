//! Builds [`EditLayout`]s: the lines of [`TextEngine::layout`] with a place
//! for every character.

use std::ops::Range;

use ljos_render::kurbo::Rect;
use parley::{BaseDirection, PositionedLayoutItem};

use super::{
    BreakClass, OwnedRanged, RawLine, Shape, TextEngine, Vertical, break_lines, ceil64, extract,
    key_hash, reported_width, shape, snapped, snapped_widths,
};
use crate::direction::in_right_to_left_block;
use crate::edit::{
    CaretCluster, EditBlock, EditLayout, EditLine, EditParams, EditRun, EditTable, PlacedBlock,
};
use crate::layout::LineRun;
use crate::style::{InlineObject, Ranged, StyleSpan, TextStyle, WrapMode};

/// The characters of one line and, for every glyph run that has glyphs, the
/// byte each glyph's character starts at.
#[derive(Clone)]
pub(super) struct LineCarets {
    pub(super) clusters: Vec<CaretCluster>,
    pub(super) glyph_bytes: Vec<Vec<usize>>,
}

/// Where the characters of every line are. Walks the shape the way
/// [`extract`] does and takes the same amount off every advance, so a cursor
/// stands exactly between two glyphs.
pub(super) fn carets(shape: &Shape, text: &str) -> Vec<LineCarets> {
    let mut out = Vec::with_capacity(shape.len());
    for line in shape.lines() {
        let m = *line.metrics();
        let content = m.advance - m.hanging_advance;
        let hangs = |pen: f32, advance: f32| {
            m.hanging_advance > 0.0 && (pen >= content - 1e-3 || pen + advance <= 1e-3)
        };
        let mut shift = 0.0_f32;
        let mut clusters = Vec::new();
        let mut glyph_bytes = Vec::new();
        // A run whose text changes brush (a span that only tags or
        // underlines) comes as several glyph runs: each takes the next
        // clusters of the run, as many as its glyphs belong to.
        let mut taken: Option<((usize, std::ops::Range<usize>), usize)> = None;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(gr) = item else {
                continue;
            };
            let (placed, advances) = snapped(&gr);
            let mut x = gr.offset() - shift;
            let mut pen = gr.offset();
            let mut next = 0;
            let mut bytes = Vec::with_capacity(placed.len());
            let run_key = (gr.run().index(), gr.run().cluster_range());
            let skip = match &taken {
                Some((key, n)) if *key == run_key => *n,
                _ => 0,
            };
            let mut used = skip;
            for cluster in gr.run().visual_clusters().skip(skip) {
                let range = cluster.text_range();
                let count = cluster.glyphs().count();
                // A cluster without glyphs (the rest of a ligature) stays
                // with the glyph run before it.
                if count > 0 && next >= placed.len() {
                    break;
                }
                used += 1;
                let (mut shaped, mut qt) = (0.0_f32, 0.0_f32);
                for (g, snapped) in placed.iter().zip(&advances).skip(next).take(count) {
                    shaped += g.advance;
                    qt += snapped;
                    if !hangs(pen, g.advance) {
                        shift += g.advance - snapped;
                    }
                    pen += g.advance;
                    bytes.push(range.start);
                }
                next += count;
                // A character with glyphs of its own is as wide as Qt makes
                // them, to the bit. A ligature shares its glyphs' width out
                // between its characters, so only the difference is taken.
                let advance = if count > 0 && (cluster.advance() - shaped).abs() < 1e-3 {
                    qt
                } else {
                    cluster.advance() - (shaped - qt)
                };
                let breaks = text
                    .get(range.clone())
                    .is_some_and(|s| s.starts_with(['\n', '\r']));
                if !breaks && !range.is_empty() {
                    clusters.push(CaretCluster {
                        range,
                        x0: f64::from(x),
                        x1: f64::from(x + advance),
                        rtl: cluster.is_rtl(),
                    });
                }
                x += advance;
            }
            taken = Some((run_key, used));
            if !placed.is_empty() {
                glyph_bytes.push(bytes);
            }
        }
        out.push(LineCarets {
            clusters,
            glyph_bytes,
        });
    }
    out
}

/// A paragraph of edited text (its line break included) laid out on its
/// own, with bytes counted from its start and lines from the top.
pub(super) struct ParagraphEntry {
    text: Box<str>,
    style: TextStyle,
    ranged: OwnedRanged,
    class: BreakClass,
    max_advance: Option<u32>,
    lines: Vec<(RawLine, LineCarets)>,
    natural: f32,
}

/// Where the paragraphs of `text` are, each with its line break, when they
/// can be laid out one by one: there are several, and nothing in the text
/// reads right to left. The whole text has one direction, which the first
/// letter of all of it decides; a paragraph on its own would decide by its
/// own first letter, so a text that has any letter that may differ is laid
/// out whole.
fn paragraphs(text: &str) -> Option<Vec<Range<usize>>> {
    let breaks = text.bytes().filter(|b| *b == b'\n').count();
    if breaks == 0 || (breaks == 1 && text.ends_with('\n')) {
        return None;
    }
    let mixed = !text.is_ascii()
        && text.chars().any(|c| {
            in_right_to_left_block(c)
                || matches!(c, '\u{200F}' | '\u{061C}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        });
    if mixed {
        return None;
    }
    let mut cuts = Vec::with_capacity(breaks + 1);
    let mut start = 0;
    for (i, _) in text.match_indices('\n') {
        cuts.push(start..i + 1);
        start = i + 1;
    }
    // A text that ends with a line break has no paragraph after it: the
    // cursor's line there is added for the whole text.
    if start < text.len() {
        cuts.push(start..text.len());
    }
    Some(cuts)
}

/// The spans and objects of `ranged` inside `cut`, counted from its start.
fn within(ranged: &Ranged<'_>, cut: Range<usize>) -> (Vec<StyleSpan>, Vec<InlineObject>) {
    // Spans are sorted and do not overlap: the first that ends inside the
    // cut is found by halving.
    let first = ranged.spans.partition_point(|s| s.range.end <= cut.start);
    let spans = ranged.spans[first..]
        .iter()
        .take_while(|s| s.range.start < cut.end)
        .map(|s| StyleSpan {
            range: s.range.start.max(cut.start) - cut.start..s.range.end.min(cut.end) - cut.start,
            ..s.clone()
        })
        .filter(|s| !s.range.is_empty())
        .collect();
    let objects = ranged
        .objects
        .iter()
        .filter(|o| cut.contains(&o.index))
        .map(|o| InlineObject {
            index: o.index - cut.start,
            ..*o
        })
        .collect();
    (spans, objects)
}

/// A line of a paragraph that starts at byte `at`, in the bytes of the
/// whole text.
fn moved(mut line: RawLine, mut places: LineCarets, at: usize) -> (RawLine, LineCarets) {
    if at == 0 {
        return (line, places);
    }
    let shift = |r: &mut Range<usize>| *r = r.start + at..r.end + at;
    shift(&mut line.range);
    for c in &mut line.clusters {
        shift(&mut c.range);
    }
    for c in &mut places.clusters {
        shift(&mut c.range);
    }
    for b in places.glyph_bytes.iter_mut().flatten() {
        *b += at;
    }
    (line, places)
}

/// A cell of a row of a table: where its text was laid out on the row's
/// line and where it goes.
#[derive(Clone, Debug)]
struct CellPlace {
    /// Bytes of its text.
    bytes: Range<usize>,
    /// The tab after it, which sets it apart from the next.
    tab: Option<Range<usize>>,
    /// Left and right edge of its text as laid out on one line.
    from: f64,
    to: f64,
    /// How far its text moves right.
    dx: f64,
    /// Where the next cell's text starts, for the tab.
    next: f64,
}

/// A table of an editing layout: its rows, which are lines
/// `first_line..first_line + cells.len()`, and where everything goes.
struct Grid {
    first_line: usize,
    cells: Vec<Vec<CellPlace>>,
    /// The top of each row's text, from the table's top.
    tops: Vec<f64>,
    width: f64,
    height: f64,
    /// The frames, from the table's top left.
    rules: Vec<Rect>,
}

/// The tables among the lines, laid out as a rich `Text` lays out its own
/// (`rich::doc`): a column is as wide as its widest text, rounded up, and a
/// row as high as its line; cells stand apart by the spacing and by a frame
/// of one pixel each when the table has a border. A table one of whose rows
/// takes more than one line (a cell with a line break) is left as lines.
fn grids(
    text: &str,
    raw: &[(RawLine, LineCarets)],
    table_of: &dyn Fn(usize) -> Option<(usize, EditTable)>,
) -> Vec<Grid> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < raw.len() {
        let Some((mut paragraph, table)) = table_of(index) else {
            index += 1;
            continue;
        };
        let first_line = index;
        let (mut end, mut one_line_each) = (index + 1, true);
        while let Some((next, t)) = table_of(end).filter(|_| end < raw.len()) {
            if next == paragraph {
                one_line_each = false;
            } else if t.row == 0 {
                break;
            }
            paragraph = next;
            end += 1;
        }
        index = end;
        let rows = &raw[first_line..end];
        if !one_line_each {
            continue;
        }
        let mut cells: Vec<Vec<CellPlace>> = rows
            .iter()
            .map(|(line, places)| cells_of(text, line, places))
            .collect();
        let columns = cells.iter().map(Vec::len).max().unwrap_or(0);
        let mut widths = vec![0.0_f64; columns];
        for row in &cells {
            for (c, cell) in row.iter().enumerate() {
                widths[c] = widths[c].max((cell.to - cell.from).ceil());
            }
        }
        let heights: Vec<f64> = rows.iter().map(|(l, _)| l.vertical.natural()).collect();
        let (border, spacing, padding) = (table.border, table.spacing, table.padding);
        let frame = if border > 0.0 { 1.0 } else { 0.0 };
        let around = 2.0 * (padding + frame);
        let edges = |sizes: &[f64]| {
            let mut at = border + spacing;
            let mut out = Vec::with_capacity(sizes.len());
            for size in sizes {
                out.push(at);
                at += size + around + spacing;
            }
            (out, at + border)
        };
        let (lefts, width) = edges(&widths);
        let (tops, height) = edges(&heights);
        let mut rules = Vec::new();
        for (r, row) in cells.iter_mut().enumerate() {
            for c in 0..columns {
                if frame > 0.0 {
                    let (x0, y0) = (lefts[c], tops[r]);
                    let (x1, y1) = (x0 + widths[c] + around, y0 + heights[r] + around);
                    rules.extend([
                        Rect::new(x0, y0, x1, y0 + 1.0),
                        Rect::new(x0, y1 - 1.0, x1, y1),
                        Rect::new(x0, y0, x0 + 1.0, y1),
                        Rect::new(x1 - 1.0, y0, x1, y1),
                    ]);
                }
                let next = lefts.get(c + 1).map(|l| l + frame + padding);
                if let Some(cell) = row.get_mut(c) {
                    cell.dx = lefts[c] + frame + padding - cell.from;
                    cell.next = next.unwrap_or(cell.to + cell.dx);
                }
            }
        }
        if border > 0.0 {
            rules.extend([
                Rect::new(0.0, 0.0, width, border),
                Rect::new(0.0, height - border, width, height),
                Rect::new(0.0, 0.0, border, height),
                Rect::new(width - border, 0.0, width, height),
            ]);
        }
        out.push(Grid {
            first_line,
            cells,
            tops: tops.iter().map(|t| t + frame + padding).collect(),
            width,
            height,
            rules,
        });
    }
    out
}

/// The cells of a row laid out on one line: its text between tabs.
fn cells_of(text: &str, line: &RawLine, places: &LineCarets) -> Vec<CellPlace> {
    let mut out = Vec::new();
    let (mut start, mut from) = (line.range.start, 0.0_f64);
    let mut to = from;
    for c in &places.clusters {
        if text.get(c.range.clone()) == Some("\t") {
            out.push(CellPlace {
                bytes: start..c.range.start,
                tab: Some(c.range.clone()),
                from,
                to: c.x0.max(from),
                dx: 0.0,
                next: 0.0,
            });
            (start, from) = (c.range.end, c.x1);
            to = from;
        } else {
            to = to.max(c.x1);
        }
    }
    out.push(CellPlace {
        bytes: start..line.range.end,
        tab: None,
        from,
        to,
        dx: 0.0,
        next: 0.0,
    });
    out
}

/// Moves the text of every cell of a row's line to its column: its
/// characters, its glyphs and the lines under and over them. A tab between
/// two cells reaches from the end of the one's text to the start of the
/// other's, and draws nothing.
fn place_cells(line: &mut EditLine, cells: &[CellPlace]) {
    let cell_of = |byte: usize| cells.iter().position(|c| c.bytes.contains(&byte));
    for c in &mut line.clusters {
        if let Some(cell) = cells
            .iter()
            .find(|cell| cell.tab.as_ref() == Some(&c.range))
        {
            (c.x0, c.x1) = (cell.to + cell.dx, cell.next);
        } else if let Some(cell) = cell_of(c.range.start).map(|i| &cells[i]) {
            c.x0 += cell.dx;
            c.x1 += cell.dx;
        }
    }
    let mut runs = Vec::with_capacity(line.runs.len());
    for run in std::mem::take(&mut line.runs) {
        let mut at = 0;
        while at < run.glyph_bytes.len() {
            let cell = cell_of(run.glyph_bytes[at]);
            let end = (at..run.glyph_bytes.len())
                .find(|&g| cell_of(run.glyph_bytes[g]) != cell)
                .unwrap_or(run.glyph_bytes.len());
            if let Some(cell) = cell.map(|i| &cells[i]) {
                runs.push(moved_run(&run, at..end, cell));
            }
            at = end;
        }
    }
    line.runs = runs;
}

/// Glyphs `glyphs` of a run, moved with the cell they are in, and the
/// lines under and over the run where they cross the cell.
fn moved_run(run: &EditRun, glyphs: Range<usize>, cell: &CellPlace) -> EditRun {
    let dx = cell.dx;
    let mut glyph_run = (*run.run.glyphs).clone();
    glyph_run.glyphs = glyph_run.glyphs[glyphs.clone()]
        .iter()
        .map(|g| ljos_render::PositionedGlyph {
            x: g.x + dx as f32,
            ..*g
        })
        .collect();
    glyph_run.bounds = glyph_run.bounds + ljos_render::kurbo::Vec2::new(dx, 0.0);
    let within = |r: Rect| {
        Rect::new(
            r.x0.max(cell.from) + dx,
            r.y0,
            r.x1.min(cell.to).max(r.x0.max(cell.from)) + dx,
            r.y1,
        )
    };
    let mut decorations = run.run.decorations;
    decorations.underline = decorations.underline.map(within);
    decorations.strike_through = decorations.strike_through.map(within);
    EditRun {
        run: LineRun {
            glyphs: std::sync::Arc::new(glyph_run),
            decorations,
            overline: within(run.run.overline),
            tag: run.run.tag,
        },
        glyph_bytes: run.glyph_bytes[glyphs].to_vec(),
    }
}

impl TextEngine {
    /// The lines of one paragraph (see [`paragraphs`]) and its width when
    /// nothing wraps; kept while recent frames use them.
    fn paragraph(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        class: BreakClass,
        max_advance: Option<f32>,
        fallback: Vertical,
    ) -> (Vec<(RawLine, LineCarets)>, f32) {
        let max_bits = max_advance.map(f32::to_bits);
        let key = key_hash(
            text,
            style,
            (class, ranged.spans, ranged.objects, ranged.strut, max_bits),
        );
        let frame = self.frame;
        if let Some(e) = self.paragraphs.get_mut(&key, frame)
            && &*e.text == text
            && e.style == *style
            && e.class == class
            && e.max_advance == max_bits
            && e.ranged.matches(ranged)
        {
            return (e.lines.clone(), e.natural);
        }
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
        let (_, natural) = snapped_widths(&shape);
        // As in `build_layout`: a text that fits as Qt counts it is not
        // broken because the shaper's advances are a touch wider.
        let max = max_advance.filter(|w| natural > *w);
        break_lines(&mut shape, max, false);
        let lines = extract(&shape, fallback, None, ranged.objects);
        let places = carets(&shape, text);
        let lines: Vec<_> = lines.into_iter().zip(places).collect();
        let entry = ParagraphEntry {
            text: text.into(),
            style: style.clone(),
            ranged: OwnedRanged::of(ranged),
            class,
            max_advance: max_bits,
            lines: lines.clone(),
            natural,
        };
        self.paragraphs.insert(key, entry, frame);
        (lines, natural)
    }

    /// Lay out `text` for editing. Not cached as a whole (the item keeps the
    /// result while nothing changes); the shaping behind it is.
    pub fn edit_layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        params: &EditParams,
    ) -> EditLayout {
        self.edit_layout_ranged(text, style, &Ranged::NONE, params)
    }

    /// [`Self::edit_layout`] for a text whose parts differ in font (syntax
    /// highlighting): every run says which span it draws.
    pub fn edit_layout_ranged(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        params: &EditParams,
    ) -> EditLayout {
        self.lay_edit(text, style, ranged, params, None)
    }

    /// [`Self::edit_layout_ranged`] for rich text whose paragraphs stand
    /// apart and in as `blocks` say, one for each paragraph ('\n' between
    /// them): each is laid out in the width its indents leave, and they are
    /// stacked as a rich `Text` stacks its blocks, so the editor and the
    /// label of the same document look the same. A paragraph past the end
    /// of `blocks` takes the last one's room.
    pub fn edit_layout_blocks(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        params: &EditParams,
        blocks: &[EditBlock],
    ) -> EditLayout {
        self.lay_edit(text, style, ranged, params, Some(blocks))
    }

    fn lay_edit(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        params: &EditParams,
        blocks: Option<&[EditBlock]>,
    ) -> EditLayout {
        let metrics = self.style_metrics(style);
        let fallback = Vertical {
            ascent: metrics.font.ascent,
            descent: metrics.font.descent,
        };
        let width = params.width.filter(|w| w.is_finite() && *w > 0.0);
        let wrap = match width {
            Some(_) => params.wrap,
            None => WrapMode::NoWrap,
        };
        let wraps = wrap != WrapMode::NoWrap;
        let plain = EditBlock::default();
        let block_of = |i: usize| -> &EditBlock {
            blocks.map_or(&plain, |b| b.get(i).or(b.last()).unwrap_or(&plain))
        };
        let inset = |b: &EditBlock| b.indent + b.indent_right;
        let limit_of = |b: &EditBlock| {
            width.filter(|_| wraps).map(|w| {
                if blocks.is_some() {
                    (w - inset(b)).max(1.0)
                } else {
                    w
                }
            })
        };

        let class = BreakClass::of(wrap);
        // Each raw line with the paragraph it is in, when the paragraphs
        // stand apart.
        let mut owner: Vec<usize> = Vec::new();
        let cuts = match blocks {
            // Every paragraph is one, an empty last one too.
            Some(_) => {
                let mut cuts: Vec<Range<usize>> = Vec::new();
                let mut start = 0;
                for (i, _) in text.match_indices('\n') {
                    cuts.push(start..i + 1);
                    start = i + 1;
                }
                cuts.push(start..text.len());
                Some(cuts)
            }
            None if text.is_empty() => None,
            None => paragraphs(text),
        };
        let (mut raw, mut natural, rtl): (Vec<(RawLine, LineCarets)>, f64, bool) =
            if text.is_empty() && blocks.is_none() {
                (Vec::new(), 0.0, false)
            } else if let Some(cuts) = cuts {
                // Each paragraph on its own, so a key typed in one lays out that
                // one again and not the whole document.
                let mut raw = Vec::new();
                let mut natural = 0.0_f64;
                let count = cuts.len();
                for (i, cut) in cuts.into_iter().enumerate() {
                    let block = block_of(i);
                    let (at, len) = (cut.start, cut.len());
                    if len == 0 {
                        // An empty last paragraph: the cursor's line.
                        let mut line = RawLine::empty(fallback);
                        line.range = at..at;
                        raw.push((
                            line,
                            LineCarets {
                                clusters: Vec::new(),
                                glyph_bytes: Vec::new(),
                            },
                        ));
                        owner.push(i);
                        continue;
                    }
                    let (spans, objects) = within(ranged, cut.clone());
                    let part = Ranged {
                        spans: &spans,
                        objects: &objects,
                        strut: ranged.strut,
                    };
                    // The cells of a table are as wide as their text.
                    let max_advance = limit_of(block)
                        .filter(|_| block.table.is_none())
                        .map(|w| w as f32);
                    let (mut lines, width) =
                        self.paragraph(&text[cut], style, &part, class, max_advance, fallback);
                    if block.table.is_none() {
                        let inset = if blocks.is_some() { inset(block) } else { 0.0 };
                        natural = natural.max(f64::from(width) + inset);
                    }
                    // The empty line after a paragraph's break is the next
                    // paragraph's first.
                    if i + 1 < count && lines.last().is_some_and(|(l, _)| l.range.start == len) {
                        lines.pop();
                    }
                    owner.extend(std::iter::repeat_n(i, lines.len()));
                    raw.extend(
                        lines
                            .into_iter()
                            .map(|(line, places)| moved(line, places, at)),
                    );
                }
                (raw, natural, false)
            } else {
                let entry = self.shaped(text, style, ranged, class);
                // As in `build_layout`: a text that fits as Qt counts it is not
                // broken because the shaper's advances are a touch wider.
                let max_advance = width
                    .filter(|_| wraps)
                    .map(|w| w as f32)
                    .filter(|w| entry.natural_width > *w);
                break_lines(&mut entry.shape, max_advance, false);
                let lines = extract(&entry.shape, fallback, None, ranged.objects);
                let places = carets(&entry.shape, text);
                (
                    lines.into_iter().zip(places).collect(),
                    f64::from(entry.natural_width),
                    entry.rtl,
                )
            };
        // The cursor needs a line to stand on after a line break that ends
        // the text, and in an empty text.
        let needs_last = blocks.is_none()
            && raw
                .last()
                .is_none_or(|(line, _)| text.ends_with('\n') && line.range.start < text.len());
        if needs_last {
            let mut line = RawLine::empty(fallback);
            line.range = text.len()..text.len();
            raw.push((
                line,
                LineCarets {
                    clusters: Vec::new(),
                    glyph_bytes: Vec::new(),
                },
            ));
        }

        let count = raw.len();
        let table_of = |index: usize| {
            let i = *blocks.and(owner.get(index))?;
            block_of(i).table.map(|t| (i, t))
        };
        let grids = grids(text, &raw, &table_of);
        let mut row_of_line: Vec<Option<(&Grid, usize)>> = vec![None; count];
        for grid in &grids {
            for row in 0..grid.cells.len() {
                row_of_line[grid.first_line + row] = Some((grid, row));
            }
            let inset = owner
                .get(grid.first_line)
                .map_or(0.0, |i| inset(block_of(*i)));
            natural = natural.max(grid.width + inset);
        }
        let mut rules = Vec::new();
        let mut lines = Vec::with_capacity(count);
        let mut placed = Vec::new();
        let (mut top, mut content_width, mut below) = (0.0_f64, 0.0_f64, 0.0_f64);
        let mut table_top = 0.0_f64;
        for (index, (line, places)) in raw.into_iter().enumerate() {
            let row = row_of_line[index];
            let (block, inset) = match blocks {
                Some(_) => {
                    let i = owner.get(index).copied().unwrap_or(0);
                    let block = block_of(i);
                    if index == 0 || owner.get(index - 1) != Some(&i) {
                        // Between two paragraphs the larger margin counts,
                        // as a rich Text has it; between two rows of a
                        // table none does.
                        if index > 0 && row.is_none_or(|(_, r)| r == 0) {
                            top += below.max(block.margin_top);
                        }
                        below = block.margin_bottom;
                        placed.push(PlacedBlock {
                            first_line: index,
                            indent: block.indent,
                            indent_right: block.indent_right,
                            marker: block.marker.as_deref().map(|m| {
                                let marker = self.edit_layout(m, style, &EditParams::default());
                                // Right-aligned in the indent, a little
                                // before the text.
                                let gap = f64::from(style.pixel_size) * 0.4;
                                (-gap - marker.natural_width, std::rc::Rc::new(marker))
                            }),
                        });
                    }
                    (block, inset(block))
                }
                None => (&plain, 0.0),
            };
            let height = line.vertical.natural();
            let width = match row {
                Some((grid, _)) => grid.width,
                None => reported_width(line.width, limit_of(block)),
            };
            content_width = content_width.max(width + inset);
            let line_top =
                match row {
                    Some((grid, r)) => {
                        if r == 0 {
                            table_top = top;
                            rules.extend(grid.rules.iter().map(|rule| {
                                (index, *rule + ljos_render::kurbo::Vec2::new(0.0, top))
                            }));
                        }
                        table_top + grid.tops[r]
                    }
                    None => top,
                };
            let range = line.range.clone();
            let own = text.get(range.clone()).unwrap_or("");
            let trimmed = if line.hard_break {
                own.trim_end_matches(['\n', '\r'])
            } else if index + 1 < count {
                own.trim_end_matches(char::is_whitespace)
            } else {
                own
            };
            let finished = line.finish(line_top, width);
            let mut glyph_bytes = places.glyph_bytes.into_iter();
            let mut edit_line = EditLine {
                end: range.start + trimmed.len(),
                range,
                top: line_top,
                height,
                baseline: finished.baseline,
                width,
                clusters: places.clusters,
                runs: finished
                    .runs
                    .into_iter()
                    .map(|run| EditRun {
                        glyph_bytes: glyph_bytes.next().unwrap_or_default(),
                        run,
                    })
                    .collect(),
            };
            match row {
                Some((grid, r)) => {
                    place_cells(&mut edit_line, &grid.cells[r]);
                    if r + 1 == grid.cells.len() {
                        top = table_top + grid.height;
                    }
                }
                None => top += height,
            }
            lines.push(edit_line);
        }
        EditLayout {
            baseline: lines.first().map_or(0.0, |l| l.baseline),
            lines,
            natural_width: ceil64(natural as f32),
            content_width,
            content_height: top,
            rtl,
            len: text.len(),
            blocks: placed,
            rules,
        }
    }
}

#[cfg(test)]
mod tests {
    //! The numbers are Qt 6.11's (qml6, offscreen) for Liberation Sans at
    //! 14 px, from `positionToRectangle`, `positionAt` and the content sizes
    //! of `TextInput` and `TextEdit`.

    use super::*;

    fn lay(text: &str, width: Option<f64>, wrap: WrapMode) -> EditLayout {
        TextEngine::new().edit_layout(text, &TextStyle::default(), &EditParams { width, wrap })
    }

    fn one_line(text: &str) -> EditLayout {
        lay(text, None, WrapMode::NoWrap)
    }

    #[test]
    fn empty_text_has_one_line_for_the_cursor() {
        let l = one_line("");
        assert_eq!(l.lines.len(), 1);
        assert_eq!((l.content_width, l.content_height), (0.0, 16.0));
        assert_eq!(l.baseline, 12.671875);
        let c = l.caret(0);
        assert_eq!((c.line, c.x, c.top, c.height), (0, 0.0, 0.0, 16.0));
        assert_eq!(l.hit(50.0, 5.0, false), 0);
    }

    #[test]
    fn the_cursor_stands_where_qt_puts_it() {
        let l = one_line("hello brave world");
        assert_eq!(l.content_width, 105.75);
        assert_eq!(l.caret(0).x, 0.0);
        assert_eq!(l.caret(1).x, 7.78125);
        assert_eq!(l.caret(5).x, 29.5625);
        assert_eq!(l.caret(17).x, 105.75);
    }

    #[test]
    fn a_point_is_the_nearest_gap() {
        let l = one_line("hello brave world");
        assert_eq!(l.hit(3.0, 5.0, false), 0);
        assert_eq!(l.hit(4.0, 5.0, false), 1);
        assert_eq!(l.hit(7.0, 5.0, true), 0);
        assert_eq!(l.hit(500.0, 5.0, false), 17);
        assert_eq!(l.hit(-3.0, 5.0, false), 0);
    }

    #[test]
    fn a_line_break_at_the_end_starts_a_line() {
        let l = one_line("a\n");
        assert_eq!(l.lines.len(), 2);
        assert_eq!(l.content_height, 32.0);
        let c = l.caret(2);
        assert_eq!((c.line, c.x, c.top), (1, 0.0, 16.0));
        let l = one_line("\n\n");
        assert_eq!((l.lines.len(), l.content_height), (3, 48.0));
        assert_eq!(l.caret(0).line, 0);
        assert_eq!(l.caret(1).line, 1);
        assert_eq!(l.caret(2).line, 2);
    }

    #[test]
    fn positions_in_a_document() {
        let l = one_line("hello brave world\nsecond line");
        assert_eq!(l.lines.len(), 2);
        let c = l.caret(20);
        assert_eq!((c.x, c.top, c.height), (14.78125, 16.0, 16.0));
        assert_eq!((l.caret(18).x, l.caret(18).top), (0.0, 16.0));
        assert_eq!((l.caret(17).x, l.caret(17).top), (105.75, 0.0));
        assert_eq!(l.hit(5.0, 20.0, false), 19);
        assert_eq!(l.hit(500.0, 5.0, false), 17);
        assert_eq!(l.hit(50.0, 15.0, false), 9);
        assert_eq!(l.hit(50.0, 31.0, false), 25);
        assert_eq!(l.hit(50.0, 32.0, false), 29);
        assert_eq!(l.hit(50.0, -1.0, false), 0);
        assert_eq!((l.lines[0].range.clone(), l.lines[0].end), (0..18, 17));
    }

    #[test]
    fn wrapped_lines_keep_their_places() {
        let l = lay("hello brave world again", Some(60.0), WrapMode::Wrap);
        assert_eq!(l.lines.len(), 4);
        assert_eq!(l.content_height, 64.0);
        assert_eq!(l.content_width, 35.0);
        assert_eq!(l.natural_width, 143.859375);
        // After the space the cursor is on the next line.
        assert_eq!((l.caret(6).line, l.caret(6).x), (1, 0.0));
        assert_eq!((l.caret(5).line, l.caret(5).x), (0, 29.5625));
        // Beyond the end of a wrapped line is before its hanging space.
        assert_eq!(l.hit(500.0, 5.0, false), 5);
    }

    #[test]
    fn a_range_is_one_box_per_line() {
        let l = one_line("hello\nworld");
        let rects = l.range_rects(2..8);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].0, 0);
        assert_eq!((rects[0].1.x0, rects[0].1.x1), (l.caret(2).x, l.caret(5).x));
        assert_eq!((rects[1].0, rects[1].1.x0, rects[1].1.y0), (1, 0.0, 16.0));
        assert_eq!(rects[1].1.x1, l.caret(8).x);
        assert!(l.range_rects(3..3).is_empty());
    }

    #[test]
    fn right_to_left_text_starts_at_the_right() {
        let text = "שלום";
        let l = one_line(text);
        assert!(l.rtl);
        assert_eq!(l.content_width, 30.734375);
        assert_eq!(l.caret(0).x, 30.734375);
        assert_eq!(l.caret(2).x, 20.515625);
        assert_eq!(l.caret(text.len()).x, 0.0);
        assert_eq!(l.hit(30.0, 5.0, false), 0);
        assert_eq!(l.hit(1.0, 5.0, false), text.len());
        assert_eq!(l.hit(-5.0, 5.0, false), text.len());
        assert_eq!(l.hit(500.0, 5.0, false), 0);
    }

    #[test]
    fn every_glyph_knows_its_character() {
        let l = one_line("abc");
        let run = &l.lines[0].runs[0];
        assert_eq!(run.glyph_bytes, vec![0, 1, 2]);
        assert_eq!(run.run.glyphs.glyphs.len(), 3);
    }

    #[test]
    fn spaces_at_the_end_have_a_width() {
        let l = one_line("ab ");
        assert!(l.content_width > one_line("ab").content_width);
        assert_eq!(l.caret(3).x, l.content_width);
        // Also before a line break.
        assert_eq!(one_line("ab\ncd").content_width, 15.5625);
        let l = one_line("ab \ncd");
        assert_eq!(l.content_width, 19.4375);
        assert_eq!(l.caret(3).x, 19.4375);
    }

    #[test]
    fn paragraphs_stand_apart_and_in_as_their_blocks_say() {
        let p = EditBlock {
            margin_top: 12.0,
            margin_bottom: 12.0,
            ..EditBlock::default()
        };
        let item = EditBlock {
            indent: 40.0,
            marker: Some("\u{2022}".into()),
            ..EditBlock::default()
        };
        let text = "one\ntwo words here\nend";
        let l = TextEngine::new().edit_layout_blocks(
            text,
            &TextStyle::default(),
            &Ranged::NONE,
            &EditParams {
                width: Some(80.0),
                wrap: WrapMode::WordWrap,
            },
            &[p.clone(), item, p],
        );
        // The margins between two paragraphs collapse; none at the ends.
        assert_eq!(l.lines[0].top, 0.0);
        assert_eq!(l.lines[1].top, 28.0);
        // The item wraps in the 40 pixels its indent leaves.
        let item_lines = l
            .lines
            .iter()
            .filter(|line| line.range.start >= 4 && line.range.start < 19)
            .count();
        assert!(item_lines > 1, "{:?}", l.lines);
        let last = l.lines.last().expect("a line");
        assert_eq!(last.range, 19..22);
        assert_eq!(last.top, 28.0 + 16.0 * item_lines as f64 + 12.0);
        assert_eq!(l.content_height, last.top + 16.0);
        // Every byte is on a line, the breaks too, and the cursor finds
        // its paragraph.
        assert_eq!(l.lines[0].range, 0..4);
        assert_eq!(l.caret(4).line, 1);
        assert_eq!(l.caret(20).line, l.lines.len() - 1);
        let block = l.block_of_line(2).expect("the item");
        assert_eq!(block.first_line, 1);
        assert_eq!(block.indent, 40.0);
        // The bullet ends a little before the item's text.
        let (x, marker) = block.marker.as_ref().expect("a bullet");
        assert!(*x < 0.0 && *x + marker.natural_width < 0.0 && *x > -40.0);
        assert!(l.block_of_line(0).expect("first").marker.is_none());
    }

    #[test]
    fn the_rows_of_a_table_stand_in_columns() {
        let row = |row| EditBlock {
            table: Some(EditTable {
                row,
                border: 1.0,
                spacing: 2.0,
                padding: 0.0,
            }),
            ..EditBlock::default()
        };
        // Two rows of two cells between two paragraphs.
        let text = "x\nab\tc\nabcd\te\ny";
        let mut engine = TextEngine::new();
        let style = TextStyle::default();
        let mut width = |s: &str| {
            engine
                .edit_layout(s, &style, &EditParams::default())
                .natural_width
        };
        let (abcd, e, a_b) = (width("abcd").ceil(), width("e").ceil(), width("ab"));
        let l = engine.edit_layout_blocks(
            text,
            &style,
            &Ranged::NONE,
            &EditParams::default(),
            &[EditBlock::default(), row(0), row(1), EditBlock::default()],
        );
        assert_eq!(l.lines.len(), 4);
        // Border, spacing and the cell's frame before the first column;
        // its width (the widest text, rounded up), the frame, the spacing
        // and the next frame before the second.
        let first = 1.0 + 2.0 + 1.0;
        let second = first + abcd + 1.0 + 2.0 + 1.0;
        assert_eq!(l.caret(2).x, first);
        assert_eq!(l.caret(4).x, first + a_b);
        assert_eq!(l.caret(5).x, second);
        assert_eq!(l.caret(7).x, first);
        assert_eq!(l.caret(12).x, second);
        // A row is as high as its line, and its frames go round it.
        let (line, top) = (16.0, l.lines[1].top);
        assert_eq!(top, line + first);
        assert_eq!(l.lines[2].top, top + line + 1.0 + 2.0 + 1.0);
        let height = 1.0 + 2.0 * (2.0 + line + 2.0) + 2.0 + 1.0;
        assert_eq!(l.lines[3].top, line + height);
        let table_width = second + e + 1.0 + 2.0 + 1.0;
        assert_eq!(l.natural_width, table_width);
        assert_eq!(l.lines[1].width, table_width);
        // Four frames of cells and the table's.
        assert_eq!(l.rules.len(), 5 * 4);
        assert!(l.rules.iter().all(|(line, _)| *line == 1));
        // A click between two cells is at the end of the one or the start
        // of the other.
        assert_eq!(l.hit(first + a_b + 1.0, top + 5.0, false), 4);
        assert_eq!(l.hit(second - 1.0, top + 5.0, false), 5);
        // The tab draws nothing, and every glyph moved with its cell.
        let glyphs: Vec<(usize, f32)> = l.lines[1]
            .runs
            .iter()
            .flat_map(|r| {
                r.glyph_bytes
                    .iter()
                    .copied()
                    .zip(r.run.glyphs.glyphs.iter().map(|g| g.x))
            })
            .collect();
        assert_eq!(
            glyphs.iter().map(|(b, _)| *b).collect::<Vec<_>>(),
            vec![2, 3, 5]
        );
        assert_eq!(f64::from(glyphs[2].1), second);
    }
}
