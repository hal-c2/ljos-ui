//! What the block front ends share: the nesting of a document, flattened.
//!
//! A quote in a list item in a list is three boxes in the source and none
//! in the result: every block carries the sum of the indents around it, and
//! a box's margins go to the first and the last block inside it (the larger
//! one counts where two meet, as when margins collapse).

use std::sync::Arc;

use super::doc::HAlign;
use super::{Block, BlockKind, Inline, InlineBuf, Table};

/// One open box.
#[derive(Default)]
pub(crate) struct Ctx {
    pub(crate) margin_top: f32,
    pub(crate) margin_bottom: f32,
    pub(crate) indent: f32,
    pub(crate) indent_right: f32,
    pub(crate) align: Option<HAlign>,
    /// What text directly inside is; none leaves it to the box around.
    pub(crate) kind: Option<BlockKind>,
    /// The bullet or number the first block inside gets.
    pub(crate) marker: Option<Arc<str>>,
    /// The colour behind the blocks inside, unless a box further in has
    /// its own.
    pub(crate) background: Option<Arc<str>>,
    /// A list item: blocks inside take their marker from it, never from an
    /// item further out.
    pub(crate) item: bool,
    /// Text directly inside breaks its lines only where it says so
    /// ([`Block::no_wrap`]); boxes inside say it again for themselves.
    pub(crate) no_wrap: bool,
    /// No block inside took the top margin yet.
    pub(super) top_pending: bool,
    /// A block was made inside.
    pub(super) any: bool,
}

/// How far text can stand in. A hostile document nests as deep as it is
/// long; its text still starts somewhere on the page.
const MAX_INDENT: f32 = 2000.0;

#[derive(Default)]
pub(crate) struct Flow {
    blocks: Vec<Block>,
    ctxs: Vec<Ctx>,
    /// Boxes closed on text that goes on after them, outermost first: the
    /// block that text is in is still theirs, as in Qt 6.12
    /// (features/text/rich-text-white-space.feature).
    carried: Vec<Ctx>,
    /// Goes on every block made from now on ([`Block::source_start`]).
    pub(crate) source_start: usize,
    /// The last block made ends in a space that collapsed where spaces do
    /// not break. Measured on Qt 6.11, white space after that block adds
    /// none, as after a block that ends in a space; after one written as
    /// `&nbsp;` it does.
    nowrap_space_end: bool,
}

impl Flow {
    pub(crate) fn open(&mut self, mut ctx: Ctx) {
        ctx.top_pending = true;
        ctx.any = false;
        self.ctxs.push(ctx);
    }

    /// Close the innermost box. True when a block was made inside it.
    pub(crate) fn close(&mut self) -> bool {
        let Some(ctx) = self.ctxs.pop() else {
            return false;
        };
        if ctx.any
            && let Some(last) = self.blocks.last_mut()
        {
            last.margin_bottom = last.margin_bottom.max(ctx.margin_bottom);
        }
        ctx.any
    }

    /// Close the innermost box on text that goes on after it, in the block
    /// the box made.
    pub(crate) fn close_carrying(&mut self) {
        if let Some(ctx) = self.ctxs.pop() {
            self.carried.insert(0, ctx);
        }
    }

    /// The innermost box, while nothing was made inside it.
    pub(crate) fn innermost_mut(&mut self) -> Option<&mut Ctx> {
        self.ctxs.last_mut().filter(|c| !c.any)
    }

    /// What a block made now would be, by the boxes it is in.
    pub(crate) fn kind(&self) -> Option<BlockKind> {
        self.ctxs.iter().rev().find_map(|c| c.kind)
    }

    /// Whether a block was made yet.
    pub(crate) fn any_block(&self) -> bool {
        !self.blocks.is_empty()
    }

    /// Whether the innermost box has a block yet.
    pub(crate) fn any(&self) -> bool {
        self.ctxs.last().is_some_and(|c| c.any)
    }

    /// Replace the marker of the innermost list item, if it still has it.
    pub(crate) fn set_marker(&mut self, marker: &str) {
        if let Some(item) = self.ctxs.iter_mut().rev().find(|c| c.item)
            && item.marker.is_some()
        {
            item.marker = Some(Arc::from(marker));
        }
    }

    /// Make a block of `buf` in the open boxes.
    pub(crate) fn emit(&mut self, buf: InlineBuf, kind: Option<BlockKind>, table: Option<Table>) {
        let mut block = Block::new(BlockKind::Paragraph);
        let mut inherited = None;
        let mut carried = std::mem::take(&mut self.carried);
        for ctx in self.ctxs.iter_mut().chain(&mut carried) {
            block.indent = (block.indent + ctx.indent).min(MAX_INDENT);
            block.indent_right = (block.indent_right + ctx.indent_right).min(MAX_INDENT);
            if ctx.top_pending {
                block.margin_top = block.margin_top.max(ctx.margin_top);
                ctx.top_pending = false;
            }
            ctx.any = true;
            block.align = ctx.align.or(block.align);
            inherited = ctx.kind.or(inherited);
            if ctx.background.is_some() {
                block.background.clone_from(&ctx.background);
            }
        }
        if let Some(item) = self.ctxs.iter_mut().rev().find(|c| c.item) {
            block.marker = item.marker.take();
        }
        block.kind = kind.or(inherited).unwrap_or(BlockKind::Paragraph);
        // A table's cells say it for their own lines.
        let innermost = carried.last().or(self.ctxs.last());
        block.no_wrap = table.is_none() && innermost.is_some_and(|c| c.no_wrap);
        for ctx in &carried {
            block.margin_bottom = block.margin_bottom.max(ctx.margin_bottom);
        }
        block.table = table;
        block.source_start = self.source_start;
        self.nowrap_space_end = buf.nowrap_space_end == Some(buf.text.len());
        let mut block = buf.into_block(block);
        if block.kind == BlockKind::Paragraph && block.text.is_empty() && block.images.len() == 1 {
            block.kind = BlockKind::Image;
        }
        self.blocks.push(block);
    }

    /// An empty paragraph in no box: the one Qt has after a table when
    /// what follows the table is a block of its own.
    pub(crate) fn push_paragraph(&mut self) {
        let mut block = Block::new(BlockKind::Paragraph);
        block.source_start = self.source_start;
        self.nowrap_space_end = false;
        self.blocks.push(block);
    }

    /// Put `s` at the end of the last block made: white space read after
    /// it ended. A picture with text is a paragraph.
    pub(crate) fn append(&mut self, s: &str, style: &Inline) {
        let Some(block) = self.blocks.last_mut() else {
            return;
        };
        if s.is_empty() {
            return;
        }
        self.nowrap_space_end = false;
        let mut buf = InlineBuf {
            text: std::mem::take(&mut block.text),
            spans: std::mem::take(&mut block.spans),
            images: std::mem::take(&mut block.images),
            nowrap_space_end: None,
        };
        buf.push(s, style);
        block.text = buf.text;
        block.spans = buf.spans;
        block.images = buf.images;
        if block.kind == BlockKind::Image {
            block.kind = BlockKind::Paragraph;
        }
    }

    /// Whether the last block made ends in a space or a line break, so
    /// white space after it adds no space.
    pub(crate) fn ends_open(&self) -> bool {
        self.nowrap_space_end
            || self
                .blocks
                .last()
                .is_some_and(|b| b.text.ends_with([' ', '\n']))
    }

    /// The text [`Flow::append`] put at the end of the last block ends in
    /// a space that collapsed where spaces do not break.
    pub(crate) fn ends_in_nowrap_space(&mut self) {
        self.nowrap_space_end = true;
    }

    /// The last block made, to change it before the document is finished.
    pub(crate) fn last_mut(&mut self) -> Option<&mut Block> {
        self.blocks.last_mut()
    }

    pub(crate) fn finish(mut self) -> Vec<Block> {
        while !self.ctxs.is_empty() {
            self.close();
        }
        // Margins moved after the blocks were made, so the hashes are
        // taken last.
        for block in &mut self.blocks {
            block.seal();
        }
        self.blocks
    }
}

/// A cell as read.
#[derive(Default)]
pub(crate) struct CellBuf {
    pub(crate) buf: InlineBuf,
    pub(crate) header: bool,
    pub(crate) background: Option<Arc<str>>,
    /// Where the lines of it that do not wrap start, in its text, in order.
    pub(crate) no_wrap: Vec<usize>,
    /// Where in its text the line feeds are that end a paragraph of it, in
    /// order ([`super::Cell::paragraphs`]).
    pub(crate) paragraphs: Vec<usize>,
    /// What the paragraph that text goes to holds so far.
    pub(crate) paragraph: CellParagraph,
}

/// The last paragraph of a cell as read. Measured on Qt 6.11
/// (hal-c2/ljos-ui#283): a block that starts in a paragraph that only a
/// tag opened takes that paragraph, in any other it starts one; a kept line
/// feed always starts one, but right after a block's end, where it ends
/// nothing; what comes after a block's end, but a div's, starts one; and a cell's last paragraph is none if a kept line feed
/// opened it and nothing came after.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CellParagraph {
    /// Nothing, and the start of the cell or of a block opened it.
    #[default]
    Opened,
    /// Nothing, and a kept line feed opened it.
    Fed,
    /// Something landed in it.
    Filled,
    /// A block in it ended after something landed.
    Ended,
    /// As [`Self::Ended`], and a kept line feed came right after the end.
    EndFed,
}

impl CellBuf {
    /// End the paragraph text goes to and start another.
    pub(crate) fn break_paragraph(&mut self) {
        self.paragraphs.push(self.buf.text.len());
        self.buf.push("\n", &super::Inline::default());
    }
}

/// The text of a table and where its cells are in it: cells of a row are
/// set apart by tabs, rows by line breaks.
pub(crate) fn table_block(
    rows: Vec<Vec<CellBuf>>,
    border: f32,
    spacing: f32,
    padding: f32,
) -> (InlineBuf, Table) {
    let mut buf = InlineBuf::default();
    let mut cells = Vec::new();
    let mut columns = 0;
    for (row, row_cells) in rows.into_iter().filter(|r| !r.is_empty()).enumerate() {
        if row > 0 {
            buf.text.push('\n');
        }
        columns = columns.max(row_cells.len());
        for (column, mut cell) in row_cells.into_iter().enumerate() {
            if column > 0 {
                buf.text.push('\t');
            }
            let start = buf.text.len();
            if cell.paragraph == CellParagraph::Fed
                && let Some(at) = cell.paragraphs.pop()
            {
                cell.buf.truncate(at);
            }
            let end = start + cell.buf.text.len();
            let paragraphs = cell.paragraphs.iter().map(|&at| start + at).collect();
            let no_wrap = cell
                .no_wrap
                .iter()
                .map(|&line| {
                    let line_end = cell.buf.text[line..]
                        .find('\n')
                        .map_or(end, |i| start + line + i);
                    start + line..line_end
                })
                .collect();
            buf.append(cell.buf);
            cells.push(super::Cell {
                row,
                column,
                range: start..end,
                header: cell.header,
                background: cell.background,
                no_wrap,
                paragraphs,
            });
        }
    }
    let table = Table {
        columns,
        cells,
        border,
        spacing,
        padding,
        background: None,
        width: None,
    };
    (buf, table)
}
