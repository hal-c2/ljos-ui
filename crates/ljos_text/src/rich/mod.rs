//! Rich text: a document of blocks (ARCHITECTURE §7.5).
//!
//! A [`RichDoc`] is a flat list of [`Block`]s. Each block has its own text,
//! the parts of it that are set differently ([`Span`]), and a cache of its
//! layout, so a change lays out only the blocks it touches. Nesting (a list
//! in a quote) is flattened into each block's indent and margins.
//!
//! Four front ends fill a document:
//!
//! * [`TextFormat::Plain`]: one block per line;
//! * [`TextFormat::Styled`]: Qt's `StyledText`, a strict subset of tags that
//!   becomes one block of text with line breaks in it, like plain text;
//! * [`TextFormat::Rich`]: Qt's `RichText`, a tolerant tokenizer over the
//!   documented HTML subset;
//! * [`TextFormat::Markdown`]: CommonMark with tables, strike-through and
//!   task lists, through pulldown-cmark.
//!
//! Rich text is untrusted input: it often comes from the network. Nothing in
//! a document is ever run, fetched or included by this module. A link is a
//! string the application is told about; an image is a source string the
//! caller may or may not resolve; raw HTML inside Markdown is shown as the
//! text it is; `style`, `script` and unknown tags of `RichText` are dropped,
//! and of a `style` attribute only colours, lines and the font are read.
//!
//! [`RichText`] is the other side: a rich text a user edits, character by
//! character with the format of each, written back out as HTML or Markdown.
//! Malformed input never panics (see `tests/rich_fuzz.rs`).

mod auto;
mod doc;
mod editing;
mod entity;
mod flow;
mod html;
mod markdown;
mod plain;
mod select;
mod sheet;
mod styled;

use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

pub use auto::might_be_rich;
pub use doc::{
    BlockLayout, DocLink, DocMetrics, DocParams, DocStats, HAlign, Part, PlacedImage, PlacedLine,
    RichDoc, View, default_image_size,
};
pub use editing::{
    BlockFormat, CELL, CellFormat, CharFormat, DocFont, ImageFormat, LINE_BREAK, OBJECT, PARAGRAPH,
    RichText, TableCell, TableFormat, TableRow,
};
pub use select::SelectionScope;

pub(crate) use doc::column_widths;

use crate::style::{BoxAlign, StyleSpan, TextStyle};

/// How the source of a document is read (Qt's `Text.textFormat`, with
/// `AutoText` resolved by [`TextFormat::detect`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextFormat {
    #[default]
    Plain,
    Styled,
    Rich,
    Markdown,
}

impl TextFormat {
    /// What `AutoText` makes of `text`: styled text when it looks like
    /// markup ([`might_be_rich`]), plain text otherwise. Markdown is never
    /// guessed.
    pub fn detect(text: &str) -> Self {
        if might_be_rich(text) {
            Self::Styled
        } else {
            Self::Plain
        }
    }
}

/// What the front ends need to know besides the source.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParseOptions {
    /// The space between two paragraphs of Markdown, in pixels. Qt 6.11
    /// takes two thirds of the font's point size, and nothing when the font
    /// was sized in pixels; the caller knows which.
    pub paragraph_spacing: f32,
    /// The colour, as RGBA, an editor gives a link its document gives
    /// none, as it reads the document: the application palette's link
    /// colour, which the caller knows. `None` leaves it to the caller to
    /// draw.
    pub link_color: Option<[u8; 4]>,
}

/// How a part of a block differs from the document's own style.
#[derive(Clone, Debug, PartialEq)]
pub struct Inline {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// A line along the top of the text. The layout does not know it: the
    /// caller draws it where [`crate::LineRun::overline`] says.
    pub overline: bool,
    /// The document took every line away (`text-decoration: none`): neither
    /// the document's own style nor a link around it underlines this text.
    pub undecorated: bool,
    /// Set in a fixed-width font.
    pub mono: bool,
    /// Font size as a multiple of the document's.
    pub scale: f32,
    /// A colour as the document wrote it (`red`, `#ff0000`); the caller
    /// resolves it, and ignores what it cannot read.
    pub color: Option<Arc<str>>,
    /// The colour behind the text, written and resolved like `color`. It
    /// fills the text's part of its line, from the line's top to its bottom.
    pub background: Option<Arc<str>>,
    /// The colour of the lines under, over and through the text when it is
    /// not the text's own.
    pub decoration_color: Option<Arc<str>>,
    /// The address of the link this text belongs to. Never followed here.
    pub link: Option<Arc<str>>,
    /// A family the document named (`font-family`), the first of a list.
    pub family: Option<Arc<str>>,
    /// A size in pixels the document gave (`font-size: 20px`); `scale`
    /// still applies on top of it.
    pub pixel_size: Option<f32>,
    /// `pixel_size` was given in points (`font-size: 20pt`), and is
    /// written back in them, as Qt keeps the unit a size came in.
    pub in_points: bool,
}

impl Default for Inline {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            overline: false,
            undecorated: false,
            mono: false,
            scale: 1.0,
            color: None,
            background: None,
            decoration_color: None,
            link: None,
            family: None,
            pixel_size: None,
            in_points: false,
        }
    }
}

impl Inline {
    fn is_plain(&self) -> bool {
        *self == Self::default()
    }

    /// The span as the layout engine wants it, in a document set in `base`.
    /// Links are underlined, as in Qt.
    pub fn style_span(&self, base: &TextStyle, range: Range<usize>, tag: u32) -> StyleSpan {
        // Measured on Qt 6.11: a decoration of none also takes away what the
        // item's font and a link around the text would draw.
        let inherits = !self.undecorated;
        StyleSpan {
            range,
            tag,
            pixel_size: self.pixel_size.unwrap_or(base.pixel_size) * self.scale,
            weight: if self.bold { 700.0 } else { base.weight },
            italic: base.italic || self.italic,
            underline: self.underline || (inherits && (base.underline || self.link.is_some())),
            strikeout: self.strike || (inherits && base.strikeout),
            family: match &self.family {
                Some(family) => Some(Arc::clone(family)),
                None => self.mono.then(|| Arc::from(MONOSPACE)),
            },
        }
    }
}

impl Hash for Inline {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (
            [
                self.bold,
                self.italic,
                self.underline,
                self.strike,
                self.overline,
                self.undecorated,
                self.mono,
                self.in_points,
            ],
            self.scale.to_bits(),
            &self.color,
            &self.background,
            &self.decoration_color,
            &self.link,
            &self.family,
            self.pixel_size.map(f32::to_bits),
        )
            .hash(state);
    }
}

/// The family fixed-width text asks for: the generic one, so the engine's
/// fallbacks apply where no such font is installed.
pub const MONOSPACE: &str = "monospace";

/// A part of a block's text and how it is set.
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Span {
    /// Bytes of [`Block::text`]. Spans are sorted and do not overlap.
    pub range: Range<usize>,
    pub style: Inline,
}

/// An image in a line of text.
#[derive(Clone, Debug, PartialEq)]
pub struct InlineImage {
    /// The byte of the block's text it stands before.
    pub index: usize,
    /// As the document wrote it; resolving it is the caller's decision.
    pub source: Arc<str>,
    /// What to say instead of the image.
    pub alt: Arc<str>,
    /// Sizes the document asked for.
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub align: BoxAlign,
    /// The room it takes in its line is a whole number of spaces of the
    /// document's font, the fewest that are wider than it. That is how
    /// StyledText makes room for an image in Qt 6.11.2 (by measuring).
    pub in_spaces: bool,
}

impl Hash for InlineImage {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (
            self.index,
            &self.source,
            &self.alt,
            self.width.map(f32::to_bits),
            self.height.map(f32::to_bits),
            self.align,
            self.in_spaces,
        )
            .hash(state);
    }
}

/// What a block is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockKind {
    Paragraph,
    /// A heading of level 1 to 6.
    Heading(u8),
    /// A paragraph of a quotation.
    Quote,
    /// Preformatted text: line breaks and spaces are kept.
    Code,
    /// A paragraph of a list item; the first one carries the marker.
    List,
    Table,
    /// A horizontal rule.
    Rule,
    /// A paragraph that is one image and nothing else.
    Image,
}

/// The cells of a table block. The block's text holds the cells' texts,
/// tabs between the cells of a row and line breaks between rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub columns: usize,
    pub cells: Vec<Cell>,
    /// Width of the frame around the table and around every cell.
    pub border: f32,
    /// Space between cells, and between them and the frame.
    pub spacing: f32,
    /// Space between a cell's edge and its text.
    pub padding: f32,
    /// The colour behind the whole table, spacing included, as written.
    pub background: Option<Arc<str>>,
    /// The width the table was given (`width="…"`), as written. Kept for
    /// an editor to write back; the layout does not read it.
    pub width: Option<Arc<str>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Cell {
    pub row: usize,
    pub column: usize,
    /// Bytes of the block's text.
    pub range: Range<usize>,
    pub header: bool,
    /// The colour behind the cell inside its padding, as written.
    pub background: Option<Arc<str>>,
}

impl Hash for Table {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (
            self.columns,
            &self.cells,
            (&self.background, &self.width),
            [
                self.border.to_bits(),
                self.spacing.to_bits(),
                self.padding.to_bits(),
            ],
        )
            .hash(state);
    }
}

/// One block of a document.
#[derive(Clone, Debug)]
pub struct Block {
    pub kind: BlockKind,
    /// The text that is shown. Line breaks in it break the line.
    pub text: String,
    pub spans: Vec<Span>,
    pub images: Vec<InlineImage>,
    /// Space above and below; between two blocks the larger one counts, and
    /// at the ends of the document none does.
    pub margin_top: f32,
    pub margin_bottom: f32,
    /// How far the text stands in from the left and from the right.
    pub indent: f32,
    pub indent_right: f32,
    /// The bullet or number of a list item, drawn in the indent.
    pub marker: Option<Arc<str>>,
    /// A colour filled behind the block's lines across the document, as
    /// written; nothing resolves it here.
    pub background: Option<Arc<str>>,
    /// An alignment the document asked for; none follows the item's.
    pub align: Option<HAlign>,
    pub table: Option<Table>,
    /// The block stands right under the one before it, whatever that one's
    /// bottom margin is (a Markdown table in Qt 6.11).
    pub tight_above: bool,
    /// Where in the source the top-level construct this block came from
    /// starts (the start of its line). Only Markdown fills this: it is what
    /// a streamed update parses again from.
    pub source_start: usize,
    hash: u64,
    laid: Option<doc::Laid>,
}

impl PartialEq for Block {
    /// Blocks are equal when they show the same; where they came from in the
    /// source and what is cached do not count.
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
            && self.kind == other.kind
            && self.text == other.text
            && self.spans == other.spans
            && self.images == other.images
            && self.marker == other.marker
            && self.background == other.background
            && self.table == other.table
    }
}

impl Block {
    pub(crate) fn new(kind: BlockKind) -> Self {
        Self {
            kind,
            text: String::new(),
            spans: Vec::new(),
            images: Vec::new(),
            margin_top: 0.0,
            margin_bottom: 0.0,
            indent: 0.0,
            indent_right: 0.0,
            marker: None,
            background: None,
            align: None,
            table: None,
            tight_above: false,
            source_start: 0,
            hash: 0,
            laid: None,
        }
    }

    /// Everything that decides how the block looks, in one number: a block
    /// whose hash did not change keeps its layout.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    pub(crate) fn seal(&mut self) {
        let mut h = DefaultHasher::new();
        (
            self.kind,
            &self.text,
            &self.spans,
            &self.images,
            [
                self.margin_top.to_bits(),
                self.margin_bottom.to_bits(),
                self.indent.to_bits(),
                self.indent_right.to_bits(),
            ],
            &self.marker,
            &self.background,
            self.align,
            &self.table,
            self.tight_above,
        )
            .hash(&mut h);
        self.hash = h.finish();
    }

    /// The style of the text at `byte`, if it is in a span.
    pub fn span_at(&self, byte: usize) -> Option<&Span> {
        let i = self.spans.partition_point(|s| s.range.end <= byte);
        self.spans.get(i).filter(|s| s.range.contains(&byte))
    }

    /// The span a glyph run's tag names ([`crate::LineRun::tag`]).
    pub fn span_of_tag(&self, tag: u32) -> Option<&Span> {
        self.spans.get((tag as usize).checked_sub(1)?)
    }

    /// The block's layout, where it is in the document and how tall it is,
    /// once [`RichDoc::layout`] ran.
    pub fn layout(&self) -> Option<&BlockLayout> {
        self.laid.as_ref().map(|l| &l.layout)
    }

    /// Top of the block, from the document's top.
    pub fn y(&self) -> f64 {
        self.laid.as_ref().map_or(0.0, |l| l.y)
    }

    /// Height of the block, margins not included.
    pub fn height(&self) -> f64 {
        self.laid.as_ref().map_or(0.0, |l| l.layout.height)
    }
}

/// Text and spans being collected for a block.
#[derive(Default)]
pub(crate) struct InlineBuf {
    pub(crate) text: String,
    pub(crate) spans: Vec<Span>,
    pub(crate) images: Vec<InlineImage>,
}

impl InlineBuf {
    pub(crate) fn push(&mut self, s: &str, style: &Inline) {
        if s.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(s);
        if style.is_plain() {
            return;
        }
        if let Some(last) = self.spans.last_mut()
            && last.range.end == start
            && last.style == *style
        {
            last.range.end = self.text.len();
            return;
        }
        self.spans.push(Span {
            range: start..self.text.len(),
            style: style.clone(),
        });
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty() && self.images.is_empty()
    }

    /// Cut the text back to `len` bytes (a character boundary).
    pub(crate) fn truncate(&mut self, len: usize) {
        self.text.truncate(len);
        self.spans.retain_mut(|s| {
            s.range.end = s.range.end.min(len);
            s.range.start < s.range.end
        });
        for image in &mut self.images {
            image.index = image.index.min(len);
        }
    }

    /// Append another buffer.
    pub(crate) fn append(&mut self, other: InlineBuf) {
        let at = self.text.len();
        self.text.push_str(&other.text);
        self.spans.extend(other.spans.into_iter().map(|s| Span {
            range: s.range.start + at..s.range.end + at,
            style: s.style,
        }));
        self.images
            .extend(other.images.into_iter().map(|i| InlineImage {
                index: i.index + at,
                ..i
            }));
    }

    pub(crate) fn into_block(self, mut block: Block) -> Block {
        block.text = self.text;
        block.spans = self.spans;
        block.images = self.images;
        block.seal();
        block
    }
}

/// Parse `source` in a format. Plain text and styled text cannot fail, and
/// neither can the others: what they do not understand is text.
pub(crate) fn parse(format: TextFormat, source: &str, options: &ParseOptions) -> Parsed {
    match format {
        TextFormat::Plain => Parsed::of(plain::parse(source)),
        TextFormat::Styled => Parsed::of(styled::parse(source)),
        TextFormat::Rich => Parsed::of(html::parse(source)),
        TextFormat::Markdown => markdown::parse(source, 0, options),
    }
}

pub(crate) struct Parsed {
    pub(crate) blocks: Vec<Block>,
    /// The source defines link references, which reach across blocks: such
    /// a document is always parsed whole.
    pub(crate) references: bool,
}

impl Parsed {
    fn of(blocks: Vec<Block>) -> Self {
        Self {
            blocks,
            references: false,
        }
    }
}
