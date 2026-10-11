//! The document: blocks, their cached layouts, and what a point or a range
//! of it is.
//!
//! Two shapes of document exist, and Qt has both:
//!
//! * **flat** (plain and styled text): one block laid out as plain text is,
//!   so wrapping, eliding and `maximumLineCount` all apply, and the line
//!   count is the number of lines;
//! * **flow** (rich text and Markdown): blocks stacked with margins, each
//!   wrapped to the width its indents leave. Eliding and line limits do
//!   nothing, and the document counts as one line, as Qt reports it.
//!
//! A block's layout is kept with the block ([`Block::layout`]) under a key
//! of everything it was made from. [`RichDoc::update`] hands the layouts of
//! the old blocks to the new ones that show the same, and
//! [`RichDoc::layout`] lays out only blocks without a layout for the current
//! key: appending to the last paragraph of a long document lays out that
//! paragraph. [`DocStats`] counts both, so tests can hold it to that.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use ljos_render::kurbo::Rect;

use super::{Block, BlockKind, InlineImage, ParseOptions, TextFormat, markdown, parse};
use crate::engine::TextEngine;
use crate::layout::{Line, TextLayout};
use crate::style::{InlineObject, LayoutParams, Ranged, StyleSpan, TextStyle, WrapMode};

/// How lines stand in the width they have.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HAlign {
    #[default]
    Left,
    Right,
    Center,
    Justify,
}

/// What a document is laid out with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocParams {
    /// The style of text no tag changes.
    pub style: TextStyle,
    /// Width, wrapping and the rest, as for plain text. A flow document
    /// reads only the width, the wrap mode, the line height and `justify`.
    pub layout: LayoutParams,
}

/// The width a laid-out document is shown in and how its lines stand in it.
/// Aligning moves lines; it never lays anything out again.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct View {
    pub width: f64,
    pub align: HAlign,
}

/// What a document measures.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DocMetrics {
    /// Width when nothing wraps (Qt's `implicitWidth`).
    pub natural_width: f64,
    /// Width of what is shown (`contentWidth`).
    pub content_width: f64,
    /// Height of what is shown (`contentHeight`).
    pub content_height: f64,
    /// Height the text wants (`implicitHeight`).
    pub implicit_height: f64,
    /// `lineCount`: the lines of a flat document, 1 for a flow document.
    pub line_count: usize,
    pub truncated: bool,
    /// From the top to the first baseline.
    pub baseline: f64,
    /// The first block reads right to left.
    pub rtl: bool,
}

/// Counters since the document was made. They only grow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DocStats {
    /// Updates that changed something.
    pub updates: u64,
    /// Blocks that came out of a parser.
    pub parsed: u64,
    /// Blocks an update did not parse again.
    pub kept: u64,
    /// Blocks that were laid out.
    pub laid_out: u64,
    /// Blocks whose layout a layout pass found still good.
    pub reused: u64,
}

/// One laid-out piece of a block: all of its text, or one cell of a table.
#[derive(Clone, Debug)]
pub struct Part {
    pub layout: Arc<TextLayout>,
    /// Left edge, from the document's left edge.
    pub x: f64,
    /// Top, from the block's top.
    pub y: f64,
    /// How far the lines stay from the document's right edge.
    pub right: f64,
    /// The byte of the block's text this piece starts at; the layout's own
    /// byte offsets count from here.
    pub offset: usize,
    /// The piece stands where it is whatever the alignment (a table cell).
    pub fixed: bool,
}

/// A block as laid out.
#[derive(Clone, Debug, Default)]
pub struct BlockLayout {
    pub parts: Vec<Part>,
    /// The bullet or number, in the block's indent.
    pub marker: Option<Part>,
    /// Frames of a table: from the document's left edge and the block's top.
    pub rules: Vec<Rect>,
    /// Width when nothing wraps, indents included.
    pub natural_width: f64,
    /// Width as laid out, indents included.
    pub width: f64,
    pub height: f64,
    /// Height the block wants: more than `height` for flat text that ends
    /// in a line break, or that a line limit cuts short.
    pub implicit_height: f64,
    /// From the block's top to its first baseline.
    pub baseline: f64,
}

/// What a block caches: its layout, the key it was made for and where the
/// block stands.
#[derive(Clone, Debug)]
pub(crate) struct Laid {
    key: u64,
    /// How large each image of the block is drawn; the room it has in its
    /// line may be wider ([`InlineImage::in_spaces`]).
    sizes: Vec<(f32, f32)>,
    pub(crate) layout: BlockLayout,
    pub(crate) y: f64,
}

/// A line of a laid-out document, where it is drawn.
#[derive(Clone, Copy, Debug)]
pub struct PlacedLine<'a> {
    pub block: &'a Block,
    pub line: &'a Line,
    /// Left edge of the line and top of its layout, from the document's
    /// top left: a glyph run of the line is drawn at exactly this origin.
    pub x: f64,
    pub y: f64,
    /// The byte of the block's text the line's own offsets count from.
    pub offset: usize,
    /// The line is a list marker, not text of the block.
    pub marker: bool,
}

/// An inline image, where it is drawn.
#[derive(Clone, Copy, Debug)]
pub struct PlacedImage<'a> {
    pub image: &'a InlineImage,
    pub rect: Rect,
}

/// A link of the document.
#[derive(Clone, Debug, PartialEq)]
pub struct DocLink {
    pub href: Arc<str>,
    /// The text that is the link.
    pub text: String,
    /// Where it is, in the document's offsets ([`RichDoc::text`]).
    pub range: Range<usize>,
    /// What it covers; more than one rectangle when it wraps.
    pub rects: Vec<Rect>,
}

/// The size of an image nobody knows the size of.
const PLACEHOLDER: f32 = 16.0;
/// No image is laid out larger than this, whatever the document asks for.
const MAX_IMAGE: f32 = 16_384.0;

/// The size an image has when only the document is asked: what it wrote, or
/// a small placeholder.
pub fn default_image_size(image: &InlineImage) -> (f32, f32) {
    (
        image.width.unwrap_or(PLACEHOLDER),
        image.height.unwrap_or(PLACEHOLDER),
    )
}

/// A rich text document: see the module documentation.
#[derive(Clone, Debug, Default)]
pub struct RichDoc {
    source: String,
    format: TextFormat,
    options: ParseOptions,
    parsed: bool,
    /// The source defines link references: never parsed in part.
    references: bool,
    blocks: Vec<Block>,
    /// Where each block's text starts in the document's offsets.
    starts: Vec<usize>,
    metrics: DocMetrics,
    stats: DocStats,
}

impl RichDoc {
    pub fn new() -> Self {
        Self::default()
    }

    /// Make the document show `source`. False when it already does.
    ///
    /// Markdown that only grew at its end is parsed again from the start of
    /// the last top-level block that began before its unfinished last line
    /// and that follows an empty line (or starts the document). Only an
    /// empty line promises that what comes after cannot join what came
    /// before: a table whose second row is still being written is, until
    /// that row is whole, more lines of the paragraph above it. Nothing
    /// before that block can change, and those blocks are kept as they are.
    /// (Not when the document defines link references: those reach back
    /// into finished blocks.) Whatever was parsed again takes over the
    /// layout of an old block that shows the same.
    pub fn update(&mut self, source: &str, format: TextFormat, options: &ParseOptions) -> bool {
        if self.parsed && self.format == format && self.options == *options && self.source == source
        {
            return false;
        }
        let tail = self.appended_tail(source, format, options);
        let (keep, new_blocks, references) = match tail {
            Some((keep, blocks)) => (keep, blocks, false),
            None => {
                let parsed = parse(format, source, options);
                (0, parsed.blocks, parsed.references)
            }
        };
        let mut old: HashMap<u64, Vec<Block>> = HashMap::new();
        for block in self.blocks.drain(keep..) {
            old.entry(block.hash()).or_default().push(block);
        }
        self.stats.updates += 1;
        self.stats.kept += keep as u64;
        self.stats.parsed += new_blocks.len() as u64;
        for mut block in new_blocks {
            if let Some(same) = old.get_mut(&block.hash())
                && let Some(i) = same.iter().position(|b| *b == block)
            {
                block.laid = same.swap_remove(i).laid;
            }
            self.blocks.push(block);
        }
        if self.blocks.is_empty() {
            // Empty text is still a line.
            let mut block = Block::new(BlockKind::Paragraph);
            block.seal();
            self.blocks.push(block);
        }
        self.source.clear();
        self.source.push_str(source);
        self.format = format;
        self.options = *options;
        self.references = references;
        self.parsed = true;
        self.starts.clear();
        let mut at = 0;
        for block in &self.blocks {
            self.starts.push(at);
            at += block.text.len() + 1;
        }
        true
    }

    /// When `source` is the old Markdown with more at its end: how many
    /// blocks stay, and the blocks of the rest.
    fn appended_tail(
        &self,
        source: &str,
        format: TextFormat,
        options: &ParseOptions,
    ) -> Option<(usize, Vec<Block>)> {
        let grew = self.parsed
            && format == TextFormat::Markdown
            && self.format == format
            && self.options == *options
            && !self.references
            && source.len() > self.source.len()
            && source.starts_with(self.source.as_str());
        if !grew {
            return None;
        }
        // Lines that are complete never change what they are; the last
        // line, while it has no end, may: `|` is a paragraph until it grows
        // into a row of the table above it, and `***` is a rule until a
        // letter makes it the next line of the paragraph before. So the
        // block to start from is one that began before that line.
        let last_line = self.source.rfind(['\n', '\r']).map_or(0, |i| i + 1);
        let mut at = self
            .blocks
            .iter()
            .rposition(|b| b.source_start < last_line)
            .unwrap_or(0);
        // And one that stays a block of its own whatever the last line
        // becomes, which only an empty line before it promises: the row
        // under a table's first is what makes that first row a table and
        // not one more line of the paragraph above.
        while at > 0 && !follows_an_empty_line(&self.source, self.blocks[at].source_start) {
            at -= 1;
        }
        let start = self.blocks.get(at).map_or(0, |b| b.source_start);
        let parsed = markdown::parse(source.get(start..)?, start, options);
        if parsed.references {
            return None;
        }
        let keep = self.blocks.partition_point(|b| b.source_start < start);
        Some((keep, parsed.blocks))
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn format(&self) -> TextFormat {
        self.format
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn stats(&self) -> DocStats {
        self.stats
    }

    /// What the last [`Self::layout`] measured.
    pub fn metrics(&self) -> DocMetrics {
        self.metrics
    }

    /// One block laid out as plain text is, not a stack of blocks.
    pub fn is_flat(&self) -> bool {
        matches!(self.format, TextFormat::Plain | TextFormat::Styled)
    }

    /// Whether any text of the document is a link.
    pub fn has_links(&self) -> bool {
        self.blocks
            .iter()
            .any(|b| b.spans.iter().any(|s| s.style.link.is_some()))
    }

    /// Whether the document has images.
    pub fn has_images(&self) -> bool {
        self.blocks.iter().any(|b| !b.images.is_empty())
    }

    /// Lay out what has no layout for `params`, and place every block.
    /// `image_size` says how large an image is (see [`default_image_size`]).
    pub fn layout(
        &mut self,
        engine: &mut TextEngine,
        params: &DocParams,
        image_size: &mut dyn FnMut(&InlineImage) -> (f32, f32),
    ) -> DocMetrics {
        let flat = self.is_flat();
        let generation = engine.generation();
        let width = params.layout.usable_width();
        let wraps = params.layout.wrap != WrapMode::NoWrap && width.is_some();
        // The width of a space of the document's font, asked for when the
        // first image needs it.
        let mut space: Option<f32> = None;
        for block in &mut self.blocks {
            let sizes: Vec<(f32, f32)> = block
                .images
                .iter()
                .map(|image| {
                    let (w, h) = image_size(image);
                    let clean = |v: f32| {
                        if v.is_finite() {
                            v.clamp(0.0, MAX_IMAGE)
                        } else {
                            PLACEHOLDER
                        }
                    };
                    (clean(w), clean(h))
                })
                .collect();
            let rooms: Vec<(f32, f32)> = block
                .images
                .iter()
                .zip(&sizes)
                .map(|(image, &(w, h))| {
                    if !image.in_spaces {
                        return (w, h);
                    }
                    let space = *space
                        .get_or_insert_with(|| engine.advance_width(" ", &params.style) as f32);
                    if space > 0.0 {
                        (((w / space).floor() + 1.0) * space, h)
                    } else {
                        (w, h)
                    }
                })
                .collect();
            let block_params = if flat {
                params.layout
            } else {
                let inset = f64::from(block.indent) + f64::from(block.indent_right);
                LayoutParams {
                    width: width.filter(|_| wraps).map(|w| (w - inset).max(1.0)),
                    height: None,
                    wrap: params.layout.wrap,
                    elide: crate::style::ElideMode::None,
                    max_lines: 0,
                    line_height: params.layout.line_height,
                    justify: params.layout.justify || block.align == Some(HAlign::Justify),
                }
            };
            let key = {
                let mut h = DefaultHasher::new();
                (generation, &params.style, block_params.key(), flat).hash(&mut h);
                for (w, h_) in sizes.iter().chain(&rooms) {
                    (w.to_bits(), h_.to_bits()).hash(&mut h);
                }
                h.finish()
            };
            if block.laid.as_ref().is_some_and(|l| l.key == key) {
                self.stats.reused += 1;
                continue;
            }
            self.stats.laid_out += 1;
            let layout = if flat {
                lay_flat(engine, block, &params.style, &block_params, &rooms)
            } else {
                lay_block(engine, block, &params.style, &block_params, &rooms)
            };
            block.laid = Some(Laid {
                key,
                sizes,
                layout,
                y: 0.0,
            });
        }
        self.place(params, wraps)
    }

    /// [`Self::layout`] with images as large as the document says.
    pub fn layout_simple(&mut self, engine: &mut TextEngine, params: &DocParams) -> DocMetrics {
        self.layout(engine, params, &mut default_image_size)
    }

    /// Stack the blocks and measure the document.
    fn place(&mut self, params: &DocParams, wraps: bool) -> DocMetrics {
        let flat = self.is_flat();
        let mut y = 0.0_f64;
        let mut below = 0.0_f32;
        let mut natural = 0.0_f64;
        let mut used = 0.0_f64;
        for (i, block) in self.blocks.iter_mut().enumerate() {
            let gap = if i == 0 {
                0.0
            } else if block.tight_above {
                block.margin_top
            } else {
                below.max(block.margin_top)
            };
            below = block.margin_bottom;
            let Some(laid) = &mut block.laid else {
                continue;
            };
            y += f64::from(gap);
            laid.y = y;
            y += laid.layout.height;
            natural = natural.max(laid.layout.natural_width);
            used = used.max(laid.layout.width);
        }
        let first = self.blocks.first().and_then(|b| b.laid.as_ref());
        let first_layout = first
            .and_then(|l| l.layout.parts.first())
            .map(|p| &p.layout);
        let metrics = if flat {
            let layout = first.map(|l| &l.layout);
            DocMetrics {
                natural_width: natural,
                content_width: used,
                content_height: y,
                implicit_height: layout.map_or(y, |l| l.implicit_height),
                line_count: first_layout.map_or(1, |l| l.line_count()),
                truncated: first_layout.is_some_and(|l| l.truncated),
                baseline: layout.map_or(0.0, |l| l.baseline),
                rtl: first_layout.is_some_and(|l| l.rtl),
            }
        } else {
            let width = params.layout.usable_width().filter(|_| wraps);
            DocMetrics {
                natural_width: natural,
                // Measured: a wrapped document is as wide as its item, or
                // as wide as the least it can be made.
                content_width: width.map_or(used, |w| w.max(used)),
                content_height: y,
                implicit_height: y,
                line_count: 1,
                truncated: false,
                baseline: first.map_or(0.0, |l| l.y + l.layout.baseline),
                rtl: first_layout.is_some_and(|l| l.rtl),
            }
        };
        self.metrics = metrics;
        metrics
    }

    /// The document's text: the blocks' texts with a line break between
    /// two. Offsets into it are the document's offsets.
    pub fn text(&self) -> String {
        let mut out = String::with_capacity(self.len());
        for (i, block) in self.blocks.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&block.text);
        }
        out
    }

    /// Length of [`Self::text`] in bytes.
    pub fn len(&self) -> usize {
        match (self.starts.last(), self.blocks.last()) {
            (Some(start), Some(block)) => start + block.text.len(),
            _ => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The text in `range` of the document's offsets. Ends that fall inside
    /// a character move out to its edges.
    pub fn text_of(&self, range: Range<usize>) -> String {
        let mut out = String::new();
        for (i, block) in self.blocks.iter().enumerate() {
            let start = self.starts[i];
            let end = start + block.text.len();
            if range.end <= start || range.start > end {
                continue;
            }
            let from = floor_boundary(&block.text, range.start.saturating_sub(start));
            let to = ceil_boundary(&block.text, (range.end - start).min(block.text.len()));
            if let Some(s) = block.text.get(from..to.max(from)) {
                out.push_str(s);
            }
            if range.end > end && i + 1 < self.blocks.len() {
                out.push('\n');
            }
        }
        out
    }

    /// Where line `line` of `part` of `block` starts, for a view.
    fn line_x(block: &Block, part: &Part, line: &Line, view: &View) -> f64 {
        if part.fixed || line.justified {
            return part.x;
        }
        let free = view.width - part.x - part.right - line.width;
        match block.align.unwrap_or(view.align) {
            HAlign::Left | HAlign::Justify => part.x,
            HAlign::Center => part.x + free / 2.0,
            HAlign::Right => part.x + free,
        }
    }

    /// Every line of the document where a view shows it, markers included.
    pub fn lines(&self, view: &View) -> Vec<PlacedLine<'_>> {
        let mut out = Vec::new();
        for block in &self.blocks {
            let Some(laid) = &block.laid else {
                continue;
            };
            let mut shift = 0.0;
            for part in &laid.layout.parts {
                for (i, line) in part.layout.lines.iter().enumerate() {
                    let x = Self::line_x(block, part, line, view);
                    if i == 0 && shift == 0.0 {
                        shift = x - part.x;
                    }
                    out.push(PlacedLine {
                        block,
                        line,
                        x,
                        y: laid.y + part.y,
                        offset: part.offset,
                        marker: false,
                    });
                }
            }
            if let Some(marker) = &laid.layout.marker {
                // The marker goes with its line when that is centred or
                // set to the right.
                for line in &marker.layout.lines {
                    out.push(PlacedLine {
                        block,
                        line,
                        x: marker.x + shift,
                        y: laid.y + marker.y,
                        offset: 0,
                        marker: true,
                    });
                }
            }
        }
        out
    }

    /// The frames of tables and the horizontal rules, as rectangles to fill.
    pub fn rules(&self, view: &View) -> Vec<Rect> {
        let mut out = Vec::new();
        for block in &self.blocks {
            let Some(laid) = &block.laid else {
                continue;
            };
            out.extend(
                laid.layout
                    .rules
                    .iter()
                    .map(|r| Rect::new(r.x0, r.y0 + laid.y, r.x1, r.y1 + laid.y)),
            );
            if block.kind == BlockKind::Rule {
                let x0 = f64::from(block.indent);
                let x1 = (view.width - f64::from(block.indent_right)).max(x0);
                let y = (laid.y + laid.layout.height / 2.0).floor();
                out.push(Rect::new(x0, y, x1, y + 1.0));
            }
        }
        out
    }

    /// The images and where they stand.
    pub fn images(&self, view: &View) -> Vec<PlacedImage<'_>> {
        let mut out = Vec::new();
        for placed in self.lines(view) {
            if placed.marker {
                continue;
            }
            for b in &placed.line.boxes {
                let index = usize::try_from(b.id).ok();
                let Some(image) = index.and_then(|i| placed.block.images.get(i)) else {
                    continue;
                };
                // Drawn as large as it is at the start of its room.
                let width = index
                    .and_then(|i| placed.block.laid.as_ref()?.sizes.get(i))
                    .map_or(b.rect.width(), |(w, _)| f64::from(*w));
                let rect = Rect::new(b.rect.x0, b.rect.y0, b.rect.x0 + width, b.rect.y1);
                out.push(PlacedImage {
                    image,
                    rect: rect + ljos_render::kurbo::Vec2::new(placed.x, placed.y),
                });
            }
        }
        out
    }

    /// The address of the link at a point, if one is there: on a character
    /// of it, not merely on its line.
    pub fn link_at(&self, x: f64, y: f64, view: &View) -> Option<&str> {
        for placed in self.lines(view) {
            if placed.marker {
                continue;
            }
            let top = placed.y + placed.line.top;
            if y < top || y >= top + placed.line.height {
                continue;
            }
            let Some(byte) = placed.line.byte_at(x - placed.x) else {
                continue;
            };
            return placed
                .block
                .span_at(byte + placed.offset)
                .and_then(|s| s.style.link.as_deref());
        }
        None
    }

    /// The gap between two characters nearest to a point, as an offset of
    /// the document.
    pub fn position_at(&self, x: f64, y: f64, view: &View) -> usize {
        let mut best: Option<(f64, f64, usize)> = None;
        let mut block_index = 0;
        let mut last_block: *const Block = std::ptr::null();
        for placed in self.lines(view) {
            if !std::ptr::eq(last_block, placed.block) {
                if !last_block.is_null() {
                    block_index += 1;
                }
                last_block = placed.block;
            }
            if placed.marker {
                continue;
            }
            let top = placed.y + placed.line.top;
            let bottom = top + placed.line.height;
            let dy = if y < top {
                top - y
            } else if y >= bottom {
                y - bottom + f64::EPSILON
            } else {
                0.0
            };
            let left = placed.x;
            let right = placed.x + placed.line.width;
            let dx = if x < left {
                left - x
            } else if x > right {
                x - right
            } else {
                0.0
            };
            if best.is_some_and(|(by, bx, _)| (by, bx) <= (dy, dx)) {
                continue;
            }
            let start = self.starts.get(block_index).copied().unwrap_or(0);
            let local =
                (placed.line.gap_at(x - placed.x) + placed.offset).min(placed.block.text.len());
            best = Some((dy, dx, start + local));
        }
        best.map_or(0, |(_, _, offset)| offset)
    }

    /// What `range` of the document covers, a rectangle a line.
    pub fn selection_rects(&self, range: Range<usize>, view: &View) -> Vec<Rect> {
        let mut out = Vec::new();
        if range.start >= range.end {
            return out;
        }
        let mut block_index = 0;
        let mut last_block: *const Block = std::ptr::null();
        for placed in self.lines(view) {
            if !std::ptr::eq(last_block, placed.block) {
                if !last_block.is_null() {
                    block_index += 1;
                }
                last_block = placed.block;
            }
            if placed.marker {
                continue;
            }
            let start = self.starts.get(block_index).copied().unwrap_or(0) + placed.offset;
            // In the layout's own offsets.
            let local = range.start.saturating_sub(start)..range.end.saturating_sub(start);
            if local.start >= local.end {
                continue;
            }
            let top = placed.y + placed.line.top;
            for (x0, x1) in placed.line.covered(&local) {
                out.push(Rect::new(
                    placed.x + x0,
                    top,
                    placed.x + x1,
                    top + placed.line.height,
                ));
            }
        }
        out
    }

    /// The links of the document in reading order. A link set in several
    /// styles (`<a><b>x</b>y</a>`) is one link.
    pub fn links(&self, view: &View) -> Vec<DocLink> {
        let mut out: Vec<DocLink> = Vec::new();
        for (i, block) in self.blocks.iter().enumerate() {
            let start = self.starts[i];
            for span in &block.spans {
                let Some(href) = &span.style.link else {
                    continue;
                };
                let range = start + span.range.start..start + span.range.end;
                if let Some(last) = out.last_mut()
                    && last.range.end == range.start
                    && last.href == *href
                {
                    last.range.end = range.end;
                    continue;
                }
                out.push(DocLink {
                    href: href.clone(),
                    text: String::new(),
                    range,
                    rects: Vec::new(),
                });
            }
        }
        for link in &mut out {
            link.text = self.text_of(link.range.clone());
            link.rects = self.selection_rects(link.range.clone(), view);
        }
        out
    }
}

/// Whether the line before the one starting at `start` holds nothing.
fn follows_an_empty_line(source: &str, start: usize) -> bool {
    let Some(before) = source.get(..start) else {
        return false;
    };
    let before = before
        .strip_suffix('\n')
        .map(|b| b.strip_suffix('\r').unwrap_or(b))
        .or_else(|| before.strip_suffix('\r'));
    let Some(before) = before else {
        // Not the start of a line at all.
        return start == 0;
    };
    let line = before
        .rfind(['\n', '\r'])
        .map_or(before, |i| &before[i + 1..]);
    line.bytes().all(|b| b == b' ' || b == b'\t')
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Lay out `range` of a block's text with the spans and images in it.
fn lay_range(
    engine: &mut TextEngine,
    block: &Block,
    range: Range<usize>,
    style: &TextStyle,
    params: &LayoutParams,
    sizes: &[(f32, f32)],
) -> Arc<TextLayout> {
    let text = block.text.get(range.clone()).unwrap_or_default();
    let spans: Vec<StyleSpan> = block
        .spans
        .iter()
        .enumerate()
        .filter_map(|(i, span)| {
            let start = span.range.start.max(range.start);
            let end = span.range.end.min(range.end);
            let tag = u32::try_from(i + 1).ok()?;
            (start < end).then(|| {
                span.style
                    .style_span(style, start - range.start..end - range.start, tag)
            })
        })
        .collect();
    let objects: Vec<InlineObject> = block
        .images
        .iter()
        .zip(sizes)
        .enumerate()
        .filter(|(_, (image, _))| (range.start..=range.end).contains(&image.index))
        .map(|(i, (image, (width, height)))| InlineObject {
            id: i as u64,
            index: image.index - range.start,
            width: *width,
            height: *height,
            align: image.align,
        })
        .collect();
    let ranged = Ranged {
        spans: &spans,
        objects: &objects,
        strut: false,
    };
    engine.layout_ranged(text, style, &ranged, params)
}

/// A flat document's one block: plain text with spans.
fn lay_flat(
    engine: &mut TextEngine,
    block: &Block,
    style: &TextStyle,
    params: &LayoutParams,
    sizes: &[(f32, f32)],
) -> BlockLayout {
    let layout = lay_range(engine, block, 0..block.text.len(), style, params, sizes);
    let implicit_height = layout.implicit_height;
    BlockLayout {
        natural_width: layout.natural_width,
        width: layout.content_width,
        height: layout.content_height,
        implicit_height,
        baseline: layout.baseline,
        parts: vec![Part {
            layout,
            x: 0.0,
            y: 0.0,
            right: 0.0,
            offset: 0,
            fixed: false,
        }],
        marker: None,
        rules: Vec::new(),
    }
}

/// One block of a flow document.
fn lay_block(
    engine: &mut TextEngine,
    block: &Block,
    style: &TextStyle,
    params: &LayoutParams,
    sizes: &[(f32, f32)],
) -> BlockLayout {
    let indent = f64::from(block.indent);
    let right = f64::from(block.indent_right);
    if block.kind == BlockKind::Rule {
        // Measured: a rule takes one line of the document's font.
        let height = engine.font_metrics(style).line_height;
        return BlockLayout {
            natural_width: indent + right,
            width: indent + right,
            height,
            implicit_height: height,
            ..BlockLayout::default()
        };
    }
    if let Some(table) = &block.table {
        return lay_table(engine, block, table, style, params, sizes);
    }
    let layout = lay_range(engine, block, 0..block.text.len(), style, params, sizes);
    let marker = block.marker.as_deref().map(|marker| {
        let marker = engine.layout(marker, style, &LayoutParams::default());
        // Right-aligned in the indent, a little before the text, on the
        // first line's baseline.
        let gap = f64::from(style.pixel_size) * 0.4;
        Part {
            x: indent - gap - marker.natural_width,
            y: layout.baseline - marker.baseline,
            right: 0.0,
            offset: 0,
            fixed: true,
            layout: marker,
        }
    });
    BlockLayout {
        natural_width: indent + layout.natural_width + right,
        width: indent + layout.content_width + right,
        height: layout.content_height,
        implicit_height: layout.content_height,
        baseline: layout.baseline,
        parts: vec![Part {
            layout,
            x: indent,
            y: 0.0,
            right,
            offset: 0,
            fixed: false,
        }],
        marker,
        rules: Vec::new(),
    }
}

/// A table: every cell on its own, columns as wide as their widest cell.
///
/// The geometry is Qt's as measured: `spacing` between cells and around
/// them, `padding` inside, a frame of `border` around the table and, when
/// there is one, a frame of one pixel around each cell. Column widths are
/// whole pixels. Cells do not wrap.
fn lay_table(
    engine: &mut TextEngine,
    block: &Block,
    table: &super::Table,
    style: &TextStyle,
    params: &LayoutParams,
    sizes: &[(f32, f32)],
) -> BlockLayout {
    let cell_params = LayoutParams {
        line_height: params.line_height,
        ..LayoutParams::default()
    };
    let rows = table.cells.iter().map(|c| c.row + 1).max().unwrap_or(0);
    let mut widths = vec![0.0_f64; table.columns];
    let mut heights = vec![0.0_f64; rows];
    let mut layouts = Vec::with_capacity(table.cells.len());
    for cell in &table.cells {
        let layout = lay_range(
            engine,
            block,
            cell.range.clone(),
            style,
            &cell_params,
            sizes,
        );
        if let Some(w) = widths.get_mut(cell.column) {
            *w = w.max(layout.natural_width.ceil());
        }
        if let Some(h) = heights.get_mut(cell.row) {
            *h = h.max(layout.content_height);
        }
        layouts.push(layout);
    }
    let border = f64::from(table.border);
    let spacing = f64::from(table.spacing);
    let padding = f64::from(table.padding);
    let frame = if border > 0.0 { 1.0 } else { 0.0 };
    let around = 2.0 * (padding + frame);
    // Left edges of the columns and top edges of the rows (of the cells'
    // frames), and where the last one ends.
    let edges = |sizes: &[f64]| {
        let mut at = border + spacing;
        let mut out = Vec::with_capacity(sizes.len());
        for size in sizes {
            out.push(at);
            at += size + around + spacing;
        }
        (out, at + border)
    };
    let (lefts, total_width) = edges(&widths);
    let (tops, total_height) = edges(&heights);
    let indent = f64::from(block.indent);
    let mut parts = Vec::with_capacity(layouts.len());
    let mut rules = Vec::new();
    let mut baseline = 0.0;
    for (cell, layout) in table.cells.iter().zip(layouts) {
        let (Some(&left), Some(&top)) = (lefts.get(cell.column), tops.get(cell.row)) else {
            continue;
        };
        let (width, height) = (widths[cell.column] + around, heights[cell.row] + around);
        if frame > 0.0 {
            let (x0, y0, x1, y1) = (indent + left, top, indent + left + width, top + height);
            rules.extend([
                Rect::new(x0, y0, x1, y0 + 1.0),
                Rect::new(x0, y1 - 1.0, x1, y1),
                Rect::new(x0, y0, x0 + 1.0, y1),
                Rect::new(x1 - 1.0, y0, x1, y1),
            ]);
        }
        if parts.is_empty() {
            baseline = top + frame + padding + layout.baseline;
        }
        parts.push(Part {
            layout,
            x: indent + left + frame + padding,
            y: top + frame + padding,
            right: 0.0,
            offset: cell.range.start,
            fixed: true,
        });
    }
    if border > 0.0 {
        let (x0, x1) = (indent, indent + total_width);
        rules.extend([
            Rect::new(x0, 0.0, x1, border),
            Rect::new(x0, total_height - border, x1, total_height),
            Rect::new(x0, 0.0, x0 + border, total_height),
            Rect::new(x1 - border, 0.0, x1, total_height),
        ]);
    }
    let width = indent + total_width + f64::from(block.indent_right);
    BlockLayout {
        parts,
        marker: None,
        rules,
        natural_width: width,
        width,
        height: total_height,
        implicit_height: total_height,
        baseline,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(size: f32) -> DocParams {
        DocParams {
            style: TextStyle::sized(size),
            layout: LayoutParams::default(),
        }
    }

    fn doc(source: &str, format: TextFormat) -> RichDoc {
        let mut doc = RichDoc::new();
        doc.update(source, format, &ParseOptions::default());
        doc
    }

    /// Line height and the width of `abc` at 20 pixels.
    fn base(engine: &mut TextEngine) -> (f64, f64) {
        let style = TextStyle::sized(20.0);
        (
            engine.font_metrics(&style).line_height,
            engine.advance_width("abc", &style),
        )
    }

    const VIEW: View = View {
        width: 400.0,
        align: HAlign::Left,
    };

    #[test]
    fn styled_text_wants_the_line_after_its_last_break_but_does_not_show_it() {
        let mut engine = TextEngine::new();
        let (line, abc) = base(&mut engine);
        let mut d = doc("<p>abc</p>", TextFormat::Styled);
        let m = d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(m.line_count, 1);
        assert_eq!(m.content_height, line);
        assert_eq!(m.implicit_height, 2.0 * line);
        assert_eq!(m.natural_width, abc);
        // A break alone: an empty line that wants two.
        let mut d = doc("<br>", TextFormat::Styled);
        let m = d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(
            (m.line_count, m.content_height, m.implicit_height),
            (1, line, 2.0 * line)
        );
        assert_eq!(m.content_width, 0.0);
        // Plain text is no different.
        let mut d = doc("abc\n", TextFormat::Plain);
        let m = d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(
            (m.line_count, m.content_height, m.implicit_height),
            (1, line, 2.0 * line)
        );
        // A heading's last line is followed by one of the text's own font.
        let mut d = doc("<h1>abc</h1>", TextFormat::Styled);
        let m = d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(m.line_count, 1);
        assert!(m.content_height > 1.5 * line);
        assert_eq!(m.implicit_height, m.content_height + line);
    }

    #[test]
    fn flow_blocks_stack_with_the_larger_margin_and_count_as_one_line() {
        let mut engine = TextEngine::new();
        let (line, abc) = base(&mut engine);
        let mut d = doc("<p>abc</p><p>abc</p>", TextFormat::Rich);
        let m = d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(m.content_height, 2.0 * line + 12.0);
        assert_eq!(m.implicit_height, m.content_height);
        assert_eq!(m.line_count, 1);
        assert_eq!(m.natural_width, abc);
        assert_eq!(d.blocks()[1].y(), line + 12.0);
        // A heading's larger margin above wins over the paragraph's.
        let mut d = doc("<p>abc</p><h1>abc</h1><p>abc</p>", TextFormat::Rich);
        d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(d.blocks()[1].y(), line + 18.0);
        let h1 = d.blocks()[1].height();
        assert!(h1 > line * 1.5, "a heading is set larger: {h1}");
        assert_eq!(d.blocks()[2].y(), line + 18.0 + h1 + 12.0);
    }

    #[test]
    fn indents_count_in_the_width() {
        let mut engine = TextEngine::new();
        let (line, abc) = base(&mut engine);
        let mut d = doc("<ul><li>abc</li></ul>", TextFormat::Rich);
        let m = d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(m.natural_width, 40.0 + abc);
        let marker = d.blocks()[0].layout().and_then(|l| l.marker.clone());
        let marker = marker.expect("a bullet");
        assert!(marker.x > 0.0 && marker.x + marker.layout.natural_width < 40.0);
        let mut d = doc("<blockquote>abc</blockquote>", TextFormat::Rich);
        let m = d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(m.natural_width, 80.0 + abc);
        // A rule is a line high.
        let mut d = doc("abc<hr>abc", TextFormat::Rich);
        let m = d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(m.content_height, 3.0 * line);
        assert_eq!(d.rules(&VIEW).len(), 1);
    }

    #[test]
    fn a_wrapped_flow_document_is_as_wide_as_its_item() {
        let mut engine = TextEngine::new();
        let (line, abc) = base(&mut engine);
        let mut d = doc("<p>abc abc abc abc abc abc</p>", TextFormat::Rich);
        let mut p = params(20.0);
        p.layout.width = Some(100.0);
        p.layout.wrap = WrapMode::WordWrap;
        let m = d.layout_simple(&mut engine, &p);
        assert_eq!(m.content_width, 100.0);
        assert!(m.content_height >= 2.0 * line);
        assert!(m.natural_width > 100.0);
        assert_eq!(m.line_count, 1);
        // What cannot be made narrower sticks out.
        let mut d = doc("<blockquote>abc</blockquote>", TextFormat::Rich);
        p.layout.width = Some(60.0);
        let m = d.layout_simple(&mut engine, &p);
        assert_eq!(m.content_width, 80.0 + abc);
    }

    #[test]
    fn tables_have_qts_geometry() {
        let mut engine = TextEngine::new();
        let (line, abc) = base(&mut engine);
        let cell = abc.ceil();
        let size = |source: &str, engine: &mut TextEngine| {
            let mut d = doc(source, TextFormat::Rich);
            let m = d.layout_simple(engine, &params(20.0));
            (m.content_width, m.content_height)
        };
        assert_eq!(
            size("<table><tr><td>abc</td></tr></table>", &mut engine),
            (cell + 4.0, line + 4.0)
        );
        assert_eq!(
            size(
                "<table><tr><td>abc</td></tr><tr><td>abc</td></tr></table>",
                &mut engine
            ),
            (cell + 4.0, 2.0 * line + 6.0)
        );
        assert_eq!(
            size(
                "<table><tr><td>abc</td><td>abc</td></tr></table>",
                &mut engine
            ),
            (2.0 * cell + 6.0, line + 4.0)
        );
        assert_eq!(
            size(
                "<table border=\"1\"><tr><td>abc</td><td>abc</td></tr></table>",
                &mut engine
            ),
            (2.0 * cell + 12.0, line + 8.0)
        );
        assert_eq!(
            size(
                "<table cellpadding=\"5\"><tr><td>abc</td></tr></table>",
                &mut engine
            ),
            (cell + 14.0, line + 14.0)
        );
        let mut d = doc("| abc |\n|---|\n| abc |", TextFormat::Markdown);
        let m = d.layout_simple(&mut engine, &params(20.0));
        // Bold header: the column is as wide as it.
        let bold = TextStyle {
            weight: 700.0,
            ..TextStyle::sized(20.0)
        };
        let header = engine.advance_width("abc", &bold).ceil();
        assert_eq!(m.content_width, header + 9.0 + 4.0);
        assert_eq!(m.content_height, 2.0 * (line + 9.0) + 4.0);
        assert!(!d.rules(&VIEW).is_empty());
    }

    #[test]
    fn a_link_is_where_its_characters_are() {
        let mut engine = TextEngine::new();
        let (line, _) = base(&mut engine);
        let style = TextStyle::sized(20.0);
        let before = engine.advance_width("see ", &style);
        for format in [TextFormat::Styled, TextFormat::Rich] {
            let mut d = doc("see <a href=\"http://x/\">this</a> one", format);
            d.layout_simple(&mut engine, &params(20.0));
            assert_eq!(d.link_at(before + 2.0, 5.0, &VIEW), Some("http://x/"));
            assert_eq!(d.link_at(before - 2.0, 5.0, &VIEW), None);
            assert_eq!(d.link_at(before + 2.0, line + 1.0, &VIEW), None);
            assert_eq!(d.link_at(before + 2.0, -1.0, &VIEW), None);
            assert_eq!(d.link_at(390.0, 5.0, &VIEW), None);
            let links = d.links(&VIEW);
            assert_eq!(links.len(), 1);
            assert_eq!(links[0].text, "this");
            assert_eq!(links[0].range, 4..8);
            assert_eq!(links[0].rects.len(), 1);
            assert!((links[0].rects[0].x0 - before).abs() < 0.01);
            assert!(d.has_links());
        }
        let mut d = doc("see [this](http://x/) one", TextFormat::Markdown);
        d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(d.link_at(before + 2.0, 5.0, &VIEW), Some("http://x/"));
        // Aligning moves the link with its line.
        let right = View {
            width: 400.0,
            align: HAlign::Right,
        };
        assert_eq!(d.link_at(before + 2.0, 5.0, &right), None);
        let width = d.metrics().natural_width;
        assert_eq!(
            d.link_at(400.0 - width + before + 2.0, 5.0, &right),
            Some("http://x/")
        );
    }

    #[test]
    fn a_link_in_several_styles_is_one_link() {
        let mut engine = TextEngine::new();
        let mut d = doc(
            "<a href=\"u\">a<b>b</b>c</a><a href=\"v\">d</a>",
            TextFormat::Rich,
        );
        d.layout_simple(&mut engine, &params(20.0));
        let links = d.links(&VIEW);
        assert_eq!(links.len(), 2);
        assert_eq!((links[0].text.as_str(), &*links[0].href), ("abc", "u"));
        assert_eq!((links[1].text.as_str(), &*links[1].href), ("d", "v"));
    }

    #[test]
    fn offsets_run_through_the_blocks_in_reading_order() {
        let mut engine = TextEngine::new();
        let (line, abc) = base(&mut engine);
        let mut d = doc("<p>abc</p><p>def</p>", TextFormat::Rich);
        d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(d.text(), "abc\ndef");
        assert_eq!(d.len(), 7);
        assert_eq!(d.position_at(-5.0, 2.0, &VIEW), 0);
        assert_eq!(d.position_at(abc + 50.0, 2.0, &VIEW), 3);
        assert_eq!(d.position_at(-5.0, line + 14.0, &VIEW), 4);
        assert_eq!(d.position_at(abc + 50.0, 900.0, &VIEW), 7);
        // Between the blocks: the nearer one.
        assert_eq!(d.position_at(abc + 50.0, line + 2.0, &VIEW), 3);
        assert_eq!(d.text_of(1..6), "bc\nde");
        assert_eq!(d.text_of(0..7), "abc\ndef");
        assert_eq!(d.text_of(4..4), "");
        assert_eq!(d.text_of(0..900), "abc\ndef");
        let rects = d.selection_rects(1..6, &VIEW);
        assert_eq!(rects.len(), 2);
        assert!(rects[0].x0 > 0.0 && (rects[0].x1 - abc).abs() < 0.01);
        assert_eq!(rects[1].x0, 0.0);
        assert_eq!(rects[1].y0, line + 12.0);
        assert!(d.selection_rects(3..3, &VIEW).is_empty());
    }

    #[test]
    fn text_of_never_cuts_a_character() {
        let d = doc("a\u{e9}\u{1F600}b", TextFormat::Plain);
        for start in 0..=d.len() {
            for end in start..=d.len() + 1 {
                let _ = d.text_of(start..end);
            }
        }
        assert_eq!(d.text_of(2..3), "\u{e9}");
    }

    #[test]
    fn only_a_changed_block_is_laid_out_again() {
        let mut engine = TextEngine::new();
        let mut d = doc("<p>one</p><p>two</p><p>three</p>", TextFormat::Rich);
        d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(d.stats().laid_out, 3);
        // Nothing changed: nothing is laid out.
        d.layout_simple(&mut engine, &params(20.0));
        assert_eq!((d.stats().laid_out, d.stats().reused), (3, 3));
        assert!(!d.update(
            "<p>one</p><p>two</p><p>three</p>",
            TextFormat::Rich,
            &ParseOptions::default()
        ));
        // One paragraph changed, one was put in front.
        assert!(d.update(
            "<p>new</p><p>one</p><p>2</p><p>three</p>",
            TextFormat::Rich,
            &ParseOptions::default()
        ));
        d.layout_simple(&mut engine, &params(20.0));
        assert_eq!(d.stats().laid_out, 5);
        // Another font size lays out everything.
        d.layout_simple(&mut engine, &params(22.0));
        assert_eq!(d.stats().laid_out, 9);
        // A width does nothing while nothing wraps.
        let mut p = params(22.0);
        p.layout.width = Some(300.0);
        d.layout_simple(&mut engine, &p);
        assert_eq!(d.stats().laid_out, 9);
    }

    #[test]
    fn appended_markdown_is_parsed_from_the_block_before_its_last_line() {
        let options = ParseOptions::default();
        let mut d = doc("# Title\n\nfirst\n\nsecond", TextFormat::Markdown);
        assert_eq!(d.stats().parsed, 3);
        // `second` is all of the last line: it and the block before it are
        // read again, the title is kept.
        assert!(d.update(
            "# Title\n\nfirst\n\nsecond one",
            TextFormat::Markdown,
            &options
        ));
        assert_eq!((d.stats().parsed, d.stats().kept), (5, 1));
        assert_eq!(d.blocks()[2].text, "second one");
        // Once its line has ended, a block is read for the last time when
        // the next line arrives.
        d.update(
            "# Title\n\nfirst\n\nsecond one\n",
            TextFormat::Markdown,
            &options,
        );
        assert_eq!((d.stats().parsed, d.stats().kept), (7, 2));
        d.update(
            "# Title\n\nfirst\n\nsecond one\n\n- a\n- b",
            TextFormat::Markdown,
            &options,
        );
        assert_eq!((d.stats().parsed, d.stats().kept), (10, 4));
        // A list is one top-level block: all of it is read again.
        d.update(
            "# Title\n\nfirst\n\nsecond one\n\n- a\n- b\n- c",
            TextFormat::Markdown,
            &options,
        );
        assert_eq!((d.stats().parsed, d.stats().kept), (13, 7));
        assert_eq!(d.blocks().len(), 6);
        // A change that is not at the end reads everything.
        d.update("# T\n\nfirst", TextFormat::Markdown, &options);
        assert_eq!((d.stats().parsed, d.stats().kept), (15, 7));
    }

    #[test]
    fn an_unfinished_last_line_may_still_join_the_block_before_it() {
        let options = ParseOptions::default();
        // A lone `|` is a paragraph; with a cell it is a row of the table.
        let mut d = doc("| a |\n|---|\n|", TextFormat::Markdown);
        assert_eq!(d.blocks().len(), 2);
        d.update("| a |\n|---|\n| 1 |", TextFormat::Markdown, &options);
        assert_eq!(d.blocks().len(), 1);
        assert_eq!(d.blocks()[0].text, "a\n1");
        // `***` is a rule; `***x` goes on with the paragraph.
        let mut d = doc("abc\n***", TextFormat::Markdown);
        assert_eq!(d.blocks().len(), 2);
        d.update("abc\n***x", TextFormat::Markdown, &options);
        assert_eq!(d.blocks().len(), 1);
        assert_eq!(d.blocks()[0].text, "abc ***x");
        // Once the line has ended, it is what it is. Nothing is kept all
        // the same: only an empty line promises that the block after it
        // stays a block of its own, and the rule has none before it.
        let mut d = doc("abc\n***\n", TextFormat::Markdown);
        d.update("abc\n***\nx", TextFormat::Markdown, &options);
        assert_eq!(d.blocks().len(), 3);
        assert_eq!(d.stats().kept, 0);
        let mut d = doc("abc\n\n***\n", TextFormat::Markdown);
        d.update("abc\n\n***\nx", TextFormat::Markdown, &options);
        assert_eq!(d.blocks().len(), 3);
        assert_eq!(d.stats().kept, 1);
    }

    #[test]
    fn a_reference_definition_makes_the_whole_document_be_read_again() {
        let mut d = doc("see [x]\n\nmore", TextFormat::Markdown);
        assert!(d.blocks()[0].spans.is_empty());
        d.update(
            "see [x]\n\nmore\n\n[x]: http://y/",
            TextFormat::Markdown,
            &ParseOptions::default(),
        );
        assert_eq!(d.stats().kept, 0);
        assert_eq!(
            d.blocks()[0].spans[0].style.link.as_deref(),
            Some("http://y/")
        );
        // And from then on.
        d.update(
            "see [x]\n\nmore\n\n[x]: http://y/\n\nlast [x]",
            TextFormat::Markdown,
            &ParseOptions::default(),
        );
        assert_eq!(d.stats().kept, 0);
        assert!(d.blocks()[2].spans[0].style.link.is_some());
    }

    #[test]
    fn images_take_the_size_they_are_given() {
        let mut engine = TextEngine::new();
        let (line, _) = base(&mut engine);
        let mut d = doc(
            "<img src=\"a.png\" width=\"30\" height=\"60\">",
            TextFormat::Rich,
        );
        let m = d.layout_simple(&mut engine, &params(20.0));
        assert_eq!((m.content_width, m.content_height), (30.0, 60.0));
        let images = d.images(&VIEW);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].rect, Rect::new(0.0, 0.0, 30.0, 60.0));
        // The caller knows better, and nonsense is not believed.
        let m = d.layout(&mut engine, &params(20.0), &mut |_| (10.0, f32::NAN));
        assert_eq!(m.content_width, 10.0);
        assert_eq!(m.content_height, 16.0);
        let mut d = doc("![x](missing.png)", TextFormat::Markdown);
        let m = d.layout_simple(&mut engine, &params(20.0));
        assert_eq!((m.content_width, m.content_height), (16.0, 16.0));
        assert!(m.content_height < line);
        assert!(d.has_images());
    }

    #[test]
    fn empty_documents_are_a_line() {
        let mut engine = TextEngine::new();
        let (line, _) = base(&mut engine);
        for format in [
            TextFormat::Plain,
            TextFormat::Styled,
            TextFormat::Rich,
            TextFormat::Markdown,
        ] {
            let mut d = doc("", format);
            let m = d.layout_simple(&mut engine, &params(20.0));
            assert_eq!(m.content_height, line, "{format:?}");
            assert_eq!(m.natural_width, 0.0);
            assert_eq!(d.position_at(5.0, 5.0, &VIEW), 0);
            assert_eq!(d.link_at(5.0, 5.0, &VIEW), None);
            assert!(d.is_empty());
        }
    }
}
