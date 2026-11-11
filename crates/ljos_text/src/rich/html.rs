//! Qt's `RichText`: a tolerant reader of the documented HTML subset.
//!
//! Unlike `StyledText` this makes blocks: a paragraph, a heading, a list
//! item, a table each lay out on their own, with the margins and indents
//! measured on Qt 6.11 (pixels, whatever the font size):
//!
//! | element | above / below | indent |
//! |---|---|---|
//! | `p`, `ul`, `ol`, `blockquote`, `pre`, `dl` | 12 / 12 | lists 40, quote 40 on both sides |
//! | `h1` `h2` `h3` `h4` `h5` | 18, 16, 14, 12, 12 / 12 (`h5`: 4) | |
//! | `dd` | | 30 |
//! | `div`, `center`, `li`, `h6`, `hr`, `table` | none | |
//!
//! Where two margins meet the larger one counts, and the document's first
//! and last margin are dropped.
//!
//! Nothing here runs or fetches anything. The content of `script` and
//! `title` is dropped, unknown tags are dropped and their content kept,
//! and an image is only its source string. The rules of a `style` element
//! apply to the elements after it that they match, before their own
//! `style` attribute ([`super::sheet`]).
//!
//! Of a `style="…"` attribute four properties are read, as Qt 6.11 reads
//! them (by looking at its pictures): `color`, `background-color` (and
//! `background` when it is one colour), `text-decoration` (`underline`,
//! `overline`, `line-through`, each added to what the text already has, and
//! `none`, which takes all of them away) and `text-decoration-color`. A
//! value is kept as written: nothing in it is resolved here, so a `url()`
//! is only a colour nobody can read. The font properties an editor writes
//! when it saves rich text are read too, so that what it wrote reads back:
//! `font-weight`, `font-style`, `font-size` (pixels, points or a keyword)
//! and the families of `font-family` ([`super::css`]). Every other property is left
//! out. The background of a block is the block's (filled across the
//! document behind its lines), not its letters'; that of a table or a
//! table cell (its `bgcolor` too) is the table's or the cell's.
//!
//! `white-space` says what becomes of spaces and line feeds, and a block
//! or a table cell that is `pre` or `nowrap` (a `<pre>` too) breaks its
//! lines only at its line breaks ([`Block::no_wrap`]). Of a `td`'s style only
//! `white-space` is read. HTML with the mark an editor writes,
//! `<meta name="qrichtext" content="1">`, anywhere in it, is read as the
//! editor wrote it: every space and tab is text and no line feed is,
//! whatever `white-space` says, which says only whether a block wraps. A
//! `<pre>` there wraps unless its style says otherwise, as Qt reads the
//! `<pre>` it writes.
//!
//! White space after a block ends and before the next one has text goes
//! to the block before, as Qt 6.11 has it (hal-c2/ljos-ui#282): one space
//! where spaces collapse, as written where they are kept. One character
//! of it right after the end tag of a block is nothing when a tag follows.

use std::borrow::Cow;
use std::sync::Arc;

use super::css;
use super::doc::HAlign;
use super::entity;
use super::flow::{CellBuf, CellParagraph, Ctx, Flow, table_block};
use super::sheet::Sheet;
use super::styled::{FONT_SCALE, HEADING_SCALE, Marker, marker_text};
use super::{Block, BlockKind, Inline, InlineBuf, InlineImage};
use crate::style::BoxAlign;

/// Elements nested deeper than this are read as if their tags were not
/// there: the stack of open elements stays small whatever the input.
const MAX_DEPTH: usize = 128;

/// An open element.
struct Open {
    name: Box<str>,
    /// The style of text inside it.
    style: Inline,
    /// It is a box of the flow (closing it closes a [`Ctx`]).
    boxed: bool,
    /// What becomes of white space inside.
    white_space: WhiteSpace,
    /// An editor wrote it as an empty paragraph
    /// (`-qt-paragraph-type:empty`): its `<br />` is the paragraph's own
    /// line, not a break in it.
    empty: bool,
    /// It makes a block even when nothing is in it (`<p></p>` is a line).
    always: bool,
    /// A `ul` or `ol`: its bullets and how many items it has so far.
    list: Option<(Marker, u32)>,
    /// In a table cell, where a block is no box of the flow: the box it
    /// would be, which the paragraphs of the cell inside it take their
    /// format from ([`CellBuf::settle`]).
    cell_box: Option<Ctx>,
}

/// How white space in the text of an element is read, after its CSS
/// `white-space`, and whether the lines of a block of it wrap.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum WhiteSpace {
    /// `normal`: a run of white space is one space, and none at the start
    /// of a line.
    #[default]
    Collapse,
    /// `nowrap`: as `normal`, but the one space is a no-break space (measured
    /// on Qt 6.11), and a block of it does not wrap.
    NoWrap,
    /// `pre` and `<pre>`: every space is kept, a line feed ends the
    /// paragraph, and a block of it does not wrap.
    Pre,
    /// `pre-wrap`: as `pre`, but the lines wrap.
    PreWrap,
    /// `pre-line`: spaces collapse, a line feed ends the paragraph.
    Lines,
    /// All of a document an editor wrote (`<meta name="qrichtext"
    /// content="1">`), whatever its elements say: every space and tab is
    /// kept, and a line feed is not text at all (measured on Qt 6.11). Its
    /// `white-space` says only whether a block `wraps`, as the editor writes
    /// a block that does not.
    Verbatim { wraps: bool },
}

impl WhiteSpace {
    fn of(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Collapse),
            "nowrap" => Some(Self::NoWrap),
            "pre" => Some(Self::Pre),
            "pre-wrap" => Some(Self::PreWrap),
            "pre-line" => Some(Self::Lines),
            _ => None,
        }
    }

    fn collapses(self) -> bool {
        matches!(self, Self::Collapse | Self::NoWrap | Self::Lines)
    }

    /// A line feed ends the paragraph.
    fn keeps_line_feeds(self) -> bool {
        matches!(self, Self::Pre | Self::PreWrap | Self::Lines)
    }

    /// A block of it wraps its lines where the item wraps them; one that
    /// does not breaks a line only where the text says so.
    fn wraps(self) -> bool {
        !matches!(
            self,
            Self::NoWrap | Self::Pre | Self::Verbatim { wraps: false }
        )
    }
}

struct TableState {
    rows: Vec<Vec<CellBuf>>,
    /// A cell is open: text goes to it.
    cell: bool,
    border: f32,
    spacing: f32,
    padding: f32,
    background: Option<Arc<str>>,
    width: Option<Arc<str>>,
    /// How many elements were open when the table started.
    depth: usize,
    /// What the table's style says, which its rows inherit.
    white_space: WhiteSpace,
    /// What the row's style says, which its cells inherit.
    row_white_space: WhiteSpace,
    /// The paragraph of the open cell that text goes to does not wrap.
    /// Measured on Qt 6.11, it starts as the cell says for its first
    /// paragraph and as its block says for the others, and a block that
    /// starts with nothing in the paragraph yet joins it: either one saying
    /// `nowrap` makes it not wrap. So does text landing in it where the
    /// block around that text says so.
    no_wrap: bool,
    /// Nothing has landed in that paragraph since a cell or a block in it
    /// started.
    fresh: bool,
}

/// White space read where it may be nothing, or text of another block.
struct Pending {
    text: String,
    style: Inline,
    white_space: WhiteSpace,
    /// It came right after the end of a block or a `<meta>`.
    fresh: bool,
}

/// What comes after white space that waits to be placed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Next {
    Content,
    StartTag,
    EndTag,
    End,
}

/// How far the reader is past the last table it made. Measured on Qt 6.11
/// (hal-c2/ljos-ui#287): every table has a paragraph after it. Text goes
/// into it; a block starts after it and leaves it empty; an empty block
/// right after it is nothing, but where the document ends there it leaves
/// a paragraph of its own.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum AfterTable {
    #[default]
    No,
    /// A table was the last thing made: text read now goes into the
    /// paragraph after it.
    Table,
    /// A block was started after it: the paragraph after the table goes
    /// before the first block made.
    Block,
    /// As [`AfterTable::Block`], and an empty block was dropped.
    Dropped,
}

impl AfterTable {
    fn in_block(self) -> bool {
        matches!(self, Self::Block | Self::Dropped)
    }
}

/// Where the text read now is, if not in a block.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Between {
    /// In a block, or in one that has not ended yet.
    #[default]
    Text,
    /// After a block ended.
    Blocks,
    /// After a cell or a row of a table ended.
    Cells,
}

struct Parser<'a> {
    src: &'a str,
    at: usize,
    flow: Flow,
    buf: InlineBuf,
    open: Vec<Open>,
    table: Option<TableState>,
    /// How many elements were ever opened: a start tag that moved this on
    /// opened one, whatever it closed first.
    opened: u64,
    sheet: Sheet,
    /// A block that keeps line feeds was just opened: a line feed right
    /// after its start tag is not part of its text (measured on Qt 6.11,
    /// as HTML has it for `<pre>`).
    skip_line_feed: bool,
    /// The document is an editor's: its white space is
    /// [`WhiteSpace::Verbatim`].
    verbatim: bool,
    /// The editor's mark was found after text it changes: the document is
    /// read again from the start.
    again: bool,
    /// White space not placed yet: it is placed at the next tag, text or
    /// the end, by what that is.
    pending: Option<Pending>,
    between: Between,
    /// The last tag ended a block, or was a `<meta>`.
    fresh_end: bool,
    /// The last thing read was a comment.
    after_comment: bool,
    /// A box of blocks that keeps line feeds was just opened.
    container_line_feed: bool,
    /// `<html>` and `<body>` are open: their end tags end something.
    html_open: bool,
    body_open: bool,
    after_table: AfterTable,
    /// `<!--StartFragment-->` was read: what came before it is gone, and
    /// `<!--EndFragment-->` ends the text.
    in_fragment: bool,
}

pub(crate) fn parse(source: &str) -> Vec<Block> {
    let mut verbatim = false;
    loop {
        let mut p = Parser::new(source, verbatim);
        p.run();
        // Measured on Qt 6.11: the mark counts wherever it is. An editor
        // writes it first, so a document is read twice only when someone
        // put it last.
        if !p.again {
            return p.finish();
        }
        verbatim = true;
    }
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, verbatim: bool) -> Self {
        Parser {
            src,
            at: 0,
            flow: Flow::default(),
            buf: InlineBuf::default(),
            open: Vec::new(),
            table: None,
            opened: 0,
            sheet: Sheet::default(),
            skip_line_feed: false,
            verbatim,
            again: false,
            pending: None,
            between: Between::Text,
            fresh_end: false,
            after_comment: false,
            container_line_feed: false,
            html_open: false,
            body_open: false,
            after_table: AfterTable::No,
            in_fragment: false,
        }
    }

    fn run(&mut self) {
        while self.at < self.src.len() && !self.again {
            let rest = &self.src[self.at..];
            let Some(special) = rest.find(['<', '&']) else {
                self.text(rest);
                break;
            };
            self.text(&rest[..special]);
            self.at += special;
            if rest.as_bytes()[special] == b'<' {
                // A `<` that never closes swallows the rest, as in Qt.
                if !self.tag() {
                    break;
                }
            } else {
                self.reference();
            }
        }
    }

    fn finish(mut self) -> Vec<Block> {
        self.place_pending(Next::End);
        self.end_table();
        while !self.open.is_empty() {
            self.close_top();
        }
        self.flush(false);
        if self.after_table == AfterTable::Dropped {
            self.flow.push_paragraph();
            self.flow.push_paragraph();
        }
        let mut blocks = self.flow.finish();
        if blocks.is_empty() {
            // Empty text is still a line, as in every format.
            let mut block = Block::new(BlockKind::Paragraph);
            block.seal();
            blocks.push(block);
        }
        blocks
    }

    fn style(&self) -> Inline {
        self.open
            .last()
            .map(|o| o.style.clone())
            .unwrap_or_default()
    }

    fn white_space(&self) -> WhiteSpace {
        self.open.last().map_or_else(
            || self.white_space_of(None),
            |o| self.white_space_of(Some(o)),
        )
    }

    /// What white space in `open` is, `None` being the document.
    fn white_space_of(&self, open: Option<&Open>) -> WhiteSpace {
        let own = open.map(|o| o.white_space).unwrap_or_default();
        if self.verbatim {
            WhiteSpace::Verbatim { wraps: own.wraps() }
        } else {
            own
        }
    }

    /// Where text goes: the open table cell, or the block being collected.
    /// None inside a table but outside its cells, where text is dropped.
    fn target(&mut self) -> Option<&mut InlineBuf> {
        match &mut self.table {
            Some(t) if t.cell => Some(&mut t.rows.last_mut()?.last_mut()?.buf),
            Some(_) => None,
            None => Some(&mut self.buf),
        }
    }

    /// Writes to where text goes with `f`. A line of a cell is one that
    /// does not wrap once something lands in it while its paragraph, or the
    /// block around what landed, does not wrap: white space that collapses
    /// to nothing is not something.
    fn write(&mut self, f: impl FnOnce(&mut InlineBuf)) {
        let reopened = self.reopen_last_cell();
        let block_no_wrap = self.table.as_ref().is_some_and(|t| t.cell) && !self.cell_line_wraps();
        match &mut self.table {
            Some(t) if t.cell => {
                let Some(cell) = t.rows.last_mut().and_then(|r| r.last_mut()) else {
                    return;
                };
                // What lands after a block's end starts a paragraph; white
                // space that collapses to nothing starts none.
                let before = cell.buf.text.len();
                let ended = matches!(cell.paragraph, CellParagraph::Ended | CellParagraph::EndFed);
                if ended {
                    cell.break_paragraph();
                }
                let (len, images) = (cell.buf.text.len(), cell.buf.images.len());
                f(&mut cell.buf);
                if cell.buf.text.len() == len && cell.buf.images.len() == images {
                    if ended {
                        cell.paragraphs.pop();
                        cell.buf.truncate(before);
                    }
                    if let Some(was) = reopened {
                        cell.paragraph = was;
                        t.cell = false;
                        self.between = Between::Cells;
                    }
                    return;
                }
                let depth = t.depth.min(self.open.len());
                cell.settle(
                    self.open[depth..]
                        .iter_mut()
                        .filter_map(|o| o.cell_box.as_mut()),
                );
                cell.paragraph = CellParagraph::Filled;
                t.fresh = false;
                t.no_wrap |= block_no_wrap;
                if t.no_wrap {
                    let line = cell.buf.text[..len].rfind('\n').map_or(0, |i| i + 1);
                    if cell.no_wrap.last() != Some(&line) {
                        cell.no_wrap.push(line);
                    }
                }
            }
            Some(_) => {}
            None => {
                let (len, images) = (self.buf.text.len(), self.buf.images.len());
                f(&mut self.buf);
                let landed = self.buf.text.len() != len || self.buf.images.len() != images;
                if landed && self.after_table == AfterTable::Table {
                    self.after_table = AfterTable::No;
                }
            }
        }
    }

    /// Opens the last cell of the table again when something comes after
    /// it ended and before the next cell, as a paragraph of its own: no
    /// part of the text is lost to the space between cells. Measured on Qt
    /// 6.12, where a row or a table has no cell yet it is dropped. What
    /// the cell's paragraph was, when one was opened.
    fn reopen_last_cell(&mut self) -> Option<CellParagraph> {
        let t = self.table.as_mut().filter(|t| !t.cell)?;
        let cell = t.rows.last_mut().and_then(|row| row.last_mut())?;
        let was = cell.paragraph;
        if was == CellParagraph::Filled {
            cell.paragraph = CellParagraph::Ended;
        }
        t.cell = true;
        t.no_wrap = false;
        t.fresh = true;
        self.between = Between::Text;
        Some(was)
    }

    /// The open table cell.
    fn cell_mut(&mut self) -> Option<&mut CellBuf> {
        let t = self.table.as_mut().filter(|t| t.cell)?;
        t.rows.last_mut()?.last_mut()
    }

    /// Whether the line of the open cell text goes to now wraps: a cell has
    /// no boxes, so it is the innermost block in it, or the cell, that says.
    fn cell_line_wraps(&self) -> bool {
        let depth = self.table.as_ref().map_or(0, |t| t.depth);
        let block = self.open[depth.min(self.open.len())..]
            .iter()
            .rev()
            .find(|o| is_block(&o.name) || matches!(&*o.name, "td" | "th"));
        block.is_none_or(|o| self.white_space_of(Some(o)).wraps())
    }

    fn text(&mut self, s: &str) {
        let mut s = s;
        if std::mem::take(&mut self.skip_line_feed) {
            s = s.strip_prefix('\n').unwrap_or(s);
        }
        if std::mem::take(&mut self.container_line_feed) {
            // Measured on Qt 6.11: after the start tag of a box of blocks a
            // lone line feed is nothing, and so is the first one of text,
            // but white space alone that ends in one is all text.
            let blank = s.chars().all(is_space);
            if s == "\n" || !(blank && s.ends_with('\n')) {
                s = s.strip_prefix('\n').unwrap_or(s);
            }
        }
        // Measured on Qt 6.11: where spaces collapse, a comment eats the
        // white space after it.
        if std::mem::take(&mut self.after_comment) && self.white_space().collapses() {
            s = s.trim_start_matches(|c: char| c != '\u{a0}' && is_space(c));
        }
        if s.is_empty() {
            return;
        }
        if self.defers() {
            let blank = s.find(|c: char| !is_space(c)).unwrap_or(s.len());
            self.defer(&s[..blank]);
            s = &s[blank..];
            if s.is_empty() {
                return;
            }
            self.place_pending(Next::Content);
        } else if self.in_list_box() && s.chars().all(|c| c.is_ascii_whitespace()) {
            // Measured on Qt 6.11: before a list's first item it is
            // nothing, whatever its white space.
            return;
        }
        self.fresh_end = false;
        self.read(s);
    }

    /// Text where it is: in a block, or one of its own.
    fn read(&mut self, s: &str) {
        let white_space = self.white_space();
        if matches!(white_space, WhiteSpace::Verbatim { .. }) {
            let style = self.style();
            self.write(|buf| {
                for line in s.split(['\n', '\r']) {
                    buf.push(line, &style);
                }
            });
            return;
        }
        if !white_space.keeps_line_feeds() {
            self.collapsed(s, white_space);
            return;
        }
        for (i, line) in s.split('\n').enumerate() {
            if i > 0 {
                self.line_feed(white_space);
            }
            if white_space.collapses() {
                self.collapsed(line, white_space);
            } else {
                let style = self.style();
                self.write(|buf| buf.push(line, &style));
            }
        }
    }

    /// Whether white space read now waits to be placed: it may be nothing,
    /// or go to the block before.
    fn defers(&self) -> bool {
        self.pending.is_some() || self.fresh_end || self.in_gap()
    }

    /// Whether text read now is between blocks, or between cells.
    fn in_gap(&self) -> bool {
        match self.between {
            Between::Text => false,
            Between::Blocks => self.table.is_none() && self.buf.is_empty(),
            Between::Cells => self.table.as_ref().is_some_and(|t| !t.cell),
        }
    }

    /// Whether text read now is in a list before its first item.
    fn in_list_box(&self) -> bool {
        self.buf.is_empty()
            && self.table.is_none()
            && self
                .open
                .last()
                .is_some_and(|o| o.boxed && matches!(&*o.name, "ul" | "ol"))
    }

    fn defer(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        if let Some(pending) = &mut self.pending {
            pending.text.push_str(s);
            return;
        }
        self.pending = Some(Pending {
            text: s.to_owned(),
            style: self.style(),
            white_space: self.white_space(),
            fresh: self.fresh_end,
        });
    }

    /// Place the white space that waited, now that `next` comes. Measured
    /// on Qt 6.11 (hal-c2/ljos-ui#282).
    fn place_pending(&mut self, next: Next) {
        let Some(Pending {
            mut text,
            style,
            white_space,
            fresh,
        }) = self.pending.take()
        else {
            return;
        };
        // Text starts a block, and the white space is read as the start of
        // it.
        if next == Next::Content {
            self.read(&text);
            return;
        }
        // Where line feeds end paragraphs, the ones right before an end
        // tag are none.
        if next == Next::EndTag && white_space.keeps_line_feeds() {
            text.truncate(text.trim_end_matches('\n').len());
        }
        // One character right after the end of a block is nothing, unless
        // the document ends there.
        if fresh && next != Next::End && text.chars().count() == 1 {
            return;
        }
        if text.is_empty() {
            return;
        }
        if !self.in_gap() {
            self.read(&text);
        } else if self.between == Between::Cells {
            self.give_cell_before(&text, &style, white_space);
        } else {
            self.give_block_before(&text, &style, white_space);
        }
    }

    /// White space between the cells of a table: in an editor's document,
    /// as written, it goes to the cell before; elsewhere it is nothing.
    fn give_cell_before(&mut self, text: &str, style: &Inline, white_space: WhiteSpace) {
        if !matches!(white_space, WhiteSpace::Verbatim { .. }) {
            return;
        }
        let cell = self
            .table
            .as_mut()
            .and_then(|t| t.rows.last_mut())
            .and_then(|row| row.last_mut());
        if let Some(cell) = cell {
            for line in text.split(['\n', '\r']) {
                cell.buf.push(line, style);
            }
        }
    }

    /// White space after a block ended: at the end of that block. A table
    /// takes none; where spaces are kept, it is the paragraph that follows
    /// one, as Qt has after every table.
    fn give_block_before(&mut self, text: &str, style: &Inline, white_space: WhiteSpace) {
        let after_table = self.flow.last_mut().is_none_or(|b| b.table.is_some());
        let ends_open = self.flow.ends_open();
        match white_space {
            WhiteSpace::Verbatim { .. } | WhiteSpace::Pre | WhiteSpace::PreWrap if after_table => {
                self.read(text);
            }
            WhiteSpace::Verbatim { .. } => {
                let text: String = text.chars().filter(|c| !matches!(c, '\n' | '\r')).collect();
                self.flow.append(&text, style);
            }
            WhiteSpace::Pre | WhiteSpace::PreWrap => {
                // As written; a line feed starts a paragraph.
                let mut lines = text.split('\n');
                self.flow.append(lines.next().unwrap_or_default(), style);
                for line in lines {
                    self.buf.push(line, style);
                    self.flush(true);
                }
            }
            WhiteSpace::Collapse | WhiteSpace::NoWrap | WhiteSpace::Lines => {
                if after_table {
                    return;
                }
                let space = if white_space == WhiteSpace::NoWrap {
                    "\u{a0}"
                } else {
                    " "
                };
                let (first, line_feeds) = match text.split_once('\n') {
                    Some((first, rest)) if white_space == WhiteSpace::Lines => {
                        (first, 1 + rest.matches('\n').count())
                    }
                    _ => (text, 0),
                };
                let (mut first, ends_in_space) = collapsed_gap(first, space, ends_open);
                // pre-line: no space before a line feed.
                if line_feeds > 0 && first.ends_with(' ') {
                    first.pop();
                }
                self.flow.append(&first, style);
                if white_space == WhiteSpace::NoWrap && ends_in_space && !first.is_empty() {
                    self.flow.ends_in_nowrap_space();
                }
                // Each line feed an empty paragraph: what follows one on
                // its line is white space at the start of a line.
                for _ in 0..line_feeds {
                    self.flush(true);
                }
            }
        }
    }

    /// A line feed where line feeds are kept. Measured on Qt 6.11: it ends
    /// the paragraph, and the next one has the same format; an empty line
    /// is an empty paragraph.
    fn line_feed(&mut self, white_space: WhiteSpace) {
        // A table outside its cells takes no text, and a `<pre>` block
        // holds its lines (each one a paragraph to an editor).
        let in_block = self.table.is_some() || self.flow.kind() == Some(BlockKind::Code);
        let style = self.style();
        let lines = white_space == WhiteSpace::Lines;
        let trim = |buf: &mut InlineBuf| {
            if lines && buf.text.ends_with(' ') {
                buf.truncate(buf.text.len() - 1);
            }
        };
        if let Some(cell) = self.cell_mut()
            && cell.paragraph == CellParagraph::Ended
        {
            // The block's end ended its paragraph already.
            cell.paragraph = CellParagraph::EndFed;
        } else if let Some(cell) = self.cell_mut() {
            // A cell's paragraph ends here, and the next is its own.
            trim(&mut cell.buf);
            cell.break_paragraph();
            cell.paragraph = CellParagraph::Fed;
            if let Some(t) = &mut self.table {
                t.fresh = false;
            }
        } else if in_block {
            self.write(|buf| {
                trim(buf);
                buf.push("\n", &style);
            });
        } else {
            if let Some(buf) = self.target() {
                trim(buf);
            }
            self.flush(true);
        }
    }

    /// Text whose runs of white space are one space each.
    fn collapsed(&mut self, s: &str, white_space: WhiteSpace) {
        let style = self.style();
        self.write(|buf| Self::collapse_into(buf, s, white_space, &style));
    }

    fn collapse_into(buf: &mut InlineBuf, s: &str, white_space: WhiteSpace, style: &Inline) {
        let space = if white_space == WhiteSpace::NoWrap {
            "\u{a0}"
        } else {
            " "
        };
        let mut word = 0;
        for (i, c) in s.char_indices() {
            // Measured on Qt 6.11: a vertical tab too.
            if !(c.is_ascii_whitespace() || c == '\u{b}') {
                continue;
            }
            buf.push(&s[word..i], style);
            word = i + 1;
            let after_space =
                buf.text.ends_with([' ', '\n']) || buf.nowrap_space_end == Some(buf.text.len());
            if !buf.is_empty() && !after_space {
                buf.push(space, style);
                if white_space == WhiteSpace::NoWrap {
                    buf.nowrap_space_end = Some(buf.text.len());
                }
            }
        }
        buf.push(&s[word..], style);
    }

    fn line_break(&mut self) {
        let style = self.style();
        // Measured on Qt 6.11: only under `normal` is the space before a
        // break dropped (`nowrap` and `pre-line` keep it).
        let collapse = self.white_space() == WhiteSpace::Collapse;
        self.write(|buf| {
            if collapse && buf.text.ends_with(' ') {
                buf.truncate(buf.text.len() - 1);
            }
            buf.push("\n", &style);
        });
    }

    /// At an `&`: a character when it names one, itself otherwise.
    fn reference(&mut self) {
        self.skip_line_feed = false;
        let rest = &self.src[self.at + 1..];
        let name_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '#'))
            .unwrap_or(rest.len());
        let name = &rest[..name_len];
        match entity::decode(name) {
            // Measured on Qt 6.11: in an editor's document not even a
            // line feed written as a reference is text, nor a carriage
            // return, which is no character elsewhere.
            Some('\n') if self.verbatim => {
                let semicolon = usize::from(rest[name_len..].starts_with(';'));
                self.at += 1 + name_len + semicolon;
            }
            None if self.verbatim && entity::is_carriage_return(name) => {
                let semicolon = usize::from(rest[name_len..].starts_with(';'));
                self.at += 1 + name_len + semicolon;
            }
            Some(c) => {
                let semicolon = usize::from(rest[name_len..].starts_with(';'));
                self.at += 1 + name_len + semicolon;
                self.after_comment = false;
                let mut bytes = [0; 4];
                let c = c.encode_utf8(&mut bytes);
                if self.defers() {
                    if c.chars().all(is_space) {
                        self.defer(c);
                        return;
                    }
                    self.place_pending(Next::Content);
                }
                self.fresh_end = false;
                let style = self.style();
                self.write(|buf| buf.push(c, &style));
            }
            None => {
                self.text("&");
                self.at += 1;
            }
        }
    }

    /// At a `<`. False when it swallows the rest of the text.
    fn tag(&mut self) -> bool {
        self.skip_line_feed = false;
        self.container_line_feed = false;
        let rest = &self.src[self.at + 1..];
        if let Some(comment) = rest.strip_prefix("!--") {
            let Some(end) = comment.find("-->") else {
                return false;
            };
            self.at += 1 + 3 + end + 3;
            // Measured on Qt 6.12: of HTML with the marks of a fragment,
            // as a copy writes it, only what is between them is read. The
            // elements opened before the start do not count, nor does any
            // text or style sheet before it, and the text ends at the end
            // mark (features/text-input/rich-text-clipboard.feature).
            // DECISION: the editor's mark before the start still counts,
            // so that a paste keeps its spaces as Qt's does; Qt's `text`
            // drops it there too.
            match &comment[..end] {
                "StartFragment" if !self.in_fragment => {
                    let (src, at, verbatim) = (self.src, self.at, self.verbatim);
                    *self = Self {
                        at,
                        in_fragment: true,
                        ..Self::new(src, verbatim)
                    };
                }
                "EndFragment" if self.in_fragment => return false,
                _ => {}
            }
            // Measured on Qt 6.11: white space before a comment is read as
            // if the comment were not there; after one see `text`. In an
            // editor's document a comment is not there at all.
            self.after_comment = !self.verbatim;
            return true;
        }
        let Some(end) = tag_end(rest) else {
            return false;
        };
        let inner = &rest[..end];
        self.at += end + 2;
        if let Some(name) = inner.strip_prefix('/') {
            let name = name.trim().to_ascii_lowercase();
            // Measured on Qt 6.11: an end tag that closes nothing is as if
            // it were not there.
            let Some(ends_block) = self.ends(&name) else {
                return true;
            };
            self.place_pending(Next::EndTag);
            self.after_comment = false;
            self.end_tag(&name);
            self.fresh_end = ends_block;
            match name.as_str() {
                "html" => self.html_open = false,
                "body" => self.body_open = false,
                _ => {}
            }
            return true;
        }
        let inner = inner.trim_start();
        let name_len = inner
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(inner.len());
        let (name, attrs) = inner.split_at(name_len);
        // `<>`, `<!doctype html>`, `<?xml ?>`: nothing.
        if name.is_empty() {
            return true;
        }
        let name = name.to_ascii_lowercase();
        self.place_pending(Next::StartTag);
        self.after_comment = false;
        self.fresh_end = name == "meta";
        match name.as_str() {
            "html" => self.html_open = true,
            "body" => self.body_open = true,
            _ => {}
        }
        let closed = attrs.trim_end().ends_with('/');
        let attrs = Attrs(attrs);
        if matches!(name.as_str(), "script" | "style" | "title") {
            if !closed {
                let content = self.skip_raw(&name);
                if name == "style" {
                    self.sheet.add(content);
                }
            }
            return true;
        }
        self.start_tag(&name, &attrs);
        if closed && self.open.last().is_some_and(|o| *o.name == *name) {
            self.end_tag(&name);
        }
        true
    }

    /// Skip to after the end tag of `name`: what is between is not text,
    /// and is returned.
    fn skip_raw(&mut self, name: &str) -> &'a str {
        let src = self.src;
        let rest = &src[self.at..];
        let mut from = 0;
        loop {
            let Some(lt) = rest[from..].find("</") else {
                self.at = src.len();
                return rest;
            };
            let after = &rest[from + lt + 2..];
            let is_end = after
                .get(..name.len())
                .is_some_and(|n| n.eq_ignore_ascii_case(name));
            if is_end {
                let close = after.find('>').map_or(after.len(), |i| i + 1);
                self.at += from + lt + 2 + close;
                return &rest[..from + lt];
            }
            from += lt + 2;
        }
    }

    fn push(&mut self, name: &str, change: impl FnOnce(&mut Open)) {
        if self.open.len() >= MAX_DEPTH {
            return;
        }
        let mut open = Open::new(name);
        open.style = self.style();
        open.white_space = self.white_space();
        change(&mut open);
        self.open.push(open);
        self.opened = self.opened.wrapping_add(1);
    }

    /// Open an element that is a box of the flow.
    fn open_box(&mut self, name: &str, ctx: Ctx, change: impl FnOnce(&mut Open)) {
        if self.open.len() >= MAX_DEPTH {
            return;
        }
        self.flush(false);
        self.block_after_table();
        self.between = Between::Text;
        self.flow.open(ctx);
        self.push(name, |o| {
            o.boxed = true;
            change(o);
        });
    }

    /// The box a block `name` with `attrs` is, before its style: in the
    /// flow, or in a table cell the format of the paragraphs in it. An
    /// item takes the next marker of the list it is in.
    fn box_of(&mut self, name: &str, attrs: &Attrs<'_>) -> Ctx {
        let aligned = || attrs.get("align").as_deref().and_then(align);
        match name {
            "p" => Ctx {
                margin_top: 12.0,
                margin_bottom: 12.0,
                align: aligned(),
                ..Ctx::default()
            },
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = heading_level(name).unwrap_or(1);
                let (margin_top, margin_bottom) = match level {
                    1 => (18.0, 12.0),
                    2 => (16.0, 12.0),
                    3 => (14.0, 12.0),
                    4 => (12.0, 12.0),
                    5 => (12.0, 4.0),
                    _ => (0.0, 0.0),
                };
                Ctx {
                    margin_top,
                    margin_bottom,
                    align: aligned(),
                    kind: Some(BlockKind::Heading(level)),
                    ..Ctx::default()
                }
            }
            "pre" => Ctx {
                margin_top: 12.0,
                margin_bottom: 12.0,
                kind: Some(BlockKind::Code),
                ..Ctx::default()
            },
            "ul" | "ol" => Ctx {
                margin_top: 12.0,
                margin_bottom: 12.0,
                indent: 40.0,
                kind: Some(BlockKind::List),
                ..Ctx::default()
            },
            "li" => {
                let marker = self
                    .open
                    .iter_mut()
                    .rev()
                    .find_map(|o| o.list.as_mut())
                    .map(|(marker, count)| {
                        *count = count.saturating_add(1);
                        Arc::from(marker_text(*marker, *count))
                    });
                Ctx {
                    item: marker.is_some(),
                    marker,
                    ..Ctx::default()
                }
            }
            "blockquote" => Ctx {
                margin_top: 12.0,
                margin_bottom: 12.0,
                indent: 40.0,
                indent_right: 40.0,
                kind: Some(BlockKind::Quote),
                ..Ctx::default()
            },
            "dl" => Ctx {
                margin_top: 12.0,
                margin_bottom: 12.0,
                ..Ctx::default()
            },
            "dd" => Ctx {
                indent: 30.0,
                ..Ctx::default()
            },
            "div" => Ctx {
                align: aligned(),
                ..Ctx::default()
            },
            "center" => Ctx {
                align: Some(HAlign::Center),
                ..Ctx::default()
            },
            _ => Ctx::default(),
        }
    }

    /// The bullets of a list `name` with `attrs`, by how deep it is in
    /// lists.
    fn list_marker_of(&self, name: &str, attrs: &Attrs<'_>) -> Marker {
        let depth = self.open.iter().filter(|o| o.list.is_some()).count();
        list_marker(name == "ol", attrs.get("type").as_deref(), depth)
    }

    /// Make a block of the text collected so far. An empty one only when
    /// `always`.
    fn flush(&mut self, always: bool) {
        let buf = std::mem::take(&mut self.buf);
        if !buf.is_empty() || always {
            self.before_block();
            self.flow.emit(buf, None, None);
        }
    }

    /// A block starts: if it is the first after a table, it starts after
    /// the paragraph that follows the table.
    fn block_after_table(&mut self) {
        if self.after_table == AfterTable::Table {
            self.after_table = AfterTable::Block;
        }
    }

    /// A block other than a table is made now. After a table it is the
    /// paragraph that follows the table, unless a block was started.
    fn before_block(&mut self) {
        if std::mem::take(&mut self.after_table).in_block() {
            self.flow.push_paragraph();
        }
    }

    fn close_top(&mut self) {
        let Some(open) = self.open.pop() else {
            return;
        };
        if let Some(ctx) = &open.cell_box
            && let Some(cell) = self.cell_mut()
        {
            cell.close_box(ctx);
        }
        if !open.boxed {
            return;
        }
        self.between = Between::Blocks;
        // Measured: text after `</div>` goes on in the same block.
        if &*open.name == "div" && !self.buf.is_empty() {
            self.flow.close_carrying();
            return;
        }
        let always = open.always && !self.flow.any();
        if always && self.buf.is_empty() && !open.empty && self.after_table.in_block() {
            self.after_table = AfterTable::Dropped;
            self.flow.close();
            return;
        }
        self.flush(always);
        self.flow.close();
    }

    /// Close the innermost open `name` and everything opened inside it.
    /// Elements opened before the table a cell is in stay open.
    fn close(&mut self, name: &str) {
        let floor = self.table.as_ref().map_or(0, |t| t.depth);
        let Some(inside) = self.open.get(floor..) else {
            return;
        };
        let Some(at) = inside.iter().rposition(|o| &*o.name == name) else {
            return;
        };
        while self.open.len() > floor + at {
            self.close_top();
        }
    }

    /// Whether `name` is open with no box opened after it but those in
    /// `through` (what an implied end tag may close).
    fn open_within(&self, name: &str, through: &[&str]) -> bool {
        for o in self.open.iter().rev() {
            if &*o.name == name {
                return true;
            }
            if o.boxed && !through.contains(&&*o.name) {
                return false;
            }
        }
        false
    }

    fn start_tag(&mut self, name: &str, attrs: &Attrs<'_>) {
        let opened = self.opened;
        self.start_element(name, attrs);
        if self.opened == opened {
            return;
        }
        if !self.sheet.is_empty() {
            let classes = attrs.get("class").unwrap_or_default();
            for css in self.sheet.matching(name, &classes) {
                self.apply_css(&css);
            }
        }
        if let Some(css) = attrs.get("style") {
            self.apply_css(&css);
            if let Some(open) = self.open.last_mut()
                && open.boxed
            {
                open.empty = css.contains("-qt-paragraph-type:empty");
            }
        }
        let Some(open) = self.open.last() else {
            return;
        };
        let white_space = self.white_space_of(Some(open));
        let holds_blocks =
            open.boxed && matches!(&*open.name, "div" | "center" | "blockquote" | "dl");
        self.container_line_feed = holds_blocks && white_space.keeps_line_feeds();
        // A block in a table cell is not a box, nor is the cell, but their
        // line feed goes too.
        self.skip_line_feed = !holds_blocks
            && (open.boxed || is_block(&open.name) || matches!(&*open.name, "td" | "th"))
            && white_space.keeps_line_feeds();
        // It is the block that does not wrap, not the text in it: a span
        // that says `pre` wraps with its paragraph (measured on Qt 6.11).
        if open.boxed
            && let Some(ctx) = self.flow.innermost_mut()
        {
            ctx.no_wrap = !white_space.wraps();
        }
        let cell = matches!(&*open.name, "td" | "th");
        let block = is_block(&open.name);
        if let Some(t) = &mut self.table
            && t.cell
            && (cell || block)
        {
            let joins = block && t.fresh;
            t.no_wrap = !white_space.wraps() || (joins && t.no_wrap);
            t.fresh = true;
        }
    }

    /// What white space in an element `name` with `attrs` would be, opened
    /// in one where it is `inherited`: by the rules of the style sheet that
    /// match it, then its own style.
    fn white_space_for(&self, name: &str, attrs: &Attrs<'_>, inherited: WhiteSpace) -> WhiteSpace {
        let mut probe = Open::new(name);
        probe.white_space = inherited;
        if !self.sheet.is_empty() {
            let classes = attrs.get("class").unwrap_or_default();
            for css in self.sheet.matching(name, &classes) {
                apply_style(&mut probe, &css);
            }
        }
        if let Some(css) = attrs.get("style") {
            apply_style(&mut probe, &css);
        }
        probe.white_space
    }

    /// Apply a style to the element just opened, as its `style` attribute.
    fn apply_css(&mut self, css: &str) {
        let Some(open) = self.open.last_mut() else {
            return;
        };
        // Of a cell's style only its white space is of its text here; its
        // background is the cell's ([`background_of`]).
        if &*open.name == "td" {
            let mut probe = Open::new("td");
            probe.white_space = open.white_space;
            apply_style(&mut probe, css);
            open.white_space = probe.white_space;
            return;
        }
        apply_style(open, css);
        if let Some(ctx) = &mut open.cell_box {
            apply_box_style(ctx, css, false);
        }
        if open.boxed {
            let in_table = self.table.is_some();
            if let Some(ctx) = self.flow.innermost_mut() {
                apply_box_style(ctx, css, !in_table);
            }
        }
    }

    fn start_element(&mut self, name: &str, attrs: &Attrs<'_>) {
        if self.table.is_some() {
            if self.table_tag(name, attrs) {
                return;
            }
        } else if name == "table" {
            self.start_table(attrs);
            return;
        } else if matches!(name, "tr" | "td" | "th") {
            // A cell or a row with no table reads as if one started before
            // it, and ends as a table whose end tag is left out.
            // Qt 6.11: a header cell with no table opens none, and is a
            // paragraph in the middle (6.12 the same).
            self.start_table(&Attrs(""));
            self.table_tag(name, attrs);
            return;
        }
        let in_cell = self.table.is_some();
        match name {
            "b" | "strong" => self.push(name, |o| o.style.bold = true),
            "i" | "em" | "var" | "cite" | "dfn" | "address" => {
                self.push(name, |o| o.style.italic = true);
            }
            "u" => self.push(name, |o| o.style.underline = true),
            "s" | "del" | "strike" => self.push(name, |o| o.style.strike = true),
            "code" | "tt" | "kbd" | "samp" => self.push(name, |o| o.style.mono = true),
            "big" => self.push(name, |o| o.style.scale *= 1.2),
            "small" => self.push(name, |o| o.style.scale *= 0.8),
            "sub" | "sup" => self.push(name, |o| o.style.scale *= 2.0 / 3.0),
            "font" => {
                let size = attrs.get("size").and_then(|s| font_size(&s));
                let color = attrs.get("color").filter(|c| !c.is_empty());
                self.push(name, |o| {
                    if let Some(scale) = size {
                        o.style.scale = scale;
                    }
                    if let Some(color) = color {
                        o.style.color = Some(Arc::from(&*color));
                    }
                });
            }
            "a" => {
                let href = attrs.get("href");
                self.push(name, |o| {
                    // Measured on Qt 6.11: a link is underlined even where
                    // the text around it was told to have no lines, and in
                    // a link's colour whatever colour that text has.
                    if href.is_some() {
                        o.style.undecorated = false;
                        o.style.color = None;
                    }
                    o.style.link = href.as_deref().map(Arc::from);
                });
            }
            "br" if self.open.last().is_some_and(|o| o.empty) => {}
            "br" => self.line_break(),
            "img" => self.image(attrs),
            "meta" if !self.verbatim && is_editors_mark(attrs) => {
                self.verbatim = true;
                self.again = self.opened > 0
                    || !self.buf.is_empty()
                    || self.table.is_some()
                    || self.flow.any_block();
            }
            // Structure without a look of its own, and tags that stand alone.
            "html" | "body" | "head" | "meta" | "link" | "tbody" | "thead" | "tfoot" | "tr"
            | "td" | "th" | "caption" | "table" => {}
            // Inside a table cell there are no boxes: a block is a new line.
            _ if in_cell && is_block(name) => {
                if let Some(cell) = self.cell_mut() {
                    if cell.paragraph != CellParagraph::Opened {
                        cell.break_paragraph();
                    }
                    cell.paragraph = CellParagraph::Opened;
                }
                // The box it would be holds the format of the paragraphs
                // in it, and a list counts its items as one out of a table.
                let list =
                    matches!(name, "ul" | "ol").then(|| (self.list_marker_of(name, attrs), 0));
                let cell_box = (name != "hr").then(|| self.box_of(name, attrs).opened());
                if let Some(level) = heading_level(name) {
                    self.push(name, |o| {
                        heading_style(&mut o.style, level);
                        o.cell_box = cell_box;
                    });
                } else if name == "pre" {
                    let verbatim = self.verbatim;
                    self.push(name, |o| {
                        o.style.mono = true;
                        pre_white_space(o, verbatim);
                        o.cell_box = cell_box;
                    });
                } else if name != "hr" {
                    self.push(name, |o| {
                        o.list = list;
                        o.cell_box = cell_box;
                    });
                }
            }
            "hr" => {
                self.imply_end("p", &[]);
                self.flush(false);
                // Measured on Qt 6.11: a rule takes no white space after it.
                self.between = Between::Text;
                self.block_after_table();
                self.before_block();
                self.flow
                    .emit(InlineBuf::default(), Some(BlockKind::Rule), None);
            }
            "p" => {
                self.imply_end("p", &[]);
                let ctx = self.box_of(name, attrs);
                self.open_box(name, ctx, |o| o.always = true);
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.imply_end("p", &[]);
                let level = heading_level(name).unwrap_or(1);
                let ctx = self.box_of(name, attrs);
                self.open_box(name, ctx, |o| {
                    o.always = true;
                    heading_style(&mut o.style, level);
                });
            }
            "pre" => {
                self.imply_end("p", &[]);
                let ctx = self.box_of(name, attrs);
                let verbatim = self.verbatim;
                self.open_box(name, ctx, |o| {
                    o.always = true;
                    pre_white_space(o, verbatim);
                    o.style.mono = true;
                });
            }
            "ul" | "ol" => {
                self.imply_end("p", &[]);
                let marker = self.list_marker_of(name, attrs);
                let ctx = self.box_of(name, attrs);
                self.open_box(name, ctx, |o| o.list = Some((marker, 0)));
            }
            "li" => {
                self.imply_end("li", &["p", "div"]);
                let ctx = self.box_of(name, attrs);
                self.open_box(name, ctx, |o| o.always = true);
            }
            "blockquote" | "dl" => {
                self.imply_end("p", &[]);
                let ctx = self.box_of(name, attrs);
                self.open_box(name, ctx, |_| {});
            }
            "dt" | "dd" => {
                self.imply_end("dt", &["p"]);
                self.imply_end("dd", &["p"]);
                let ctx = self.box_of(name, attrs);
                self.open_box(name, ctx, |o| o.always = true);
            }
            "div" | "center" => {
                self.imply_end("p", &[]);
                let ctx = self.box_of(name, attrs);
                self.open_box(name, ctx, |_| {});
            }
            // Unknown: dropped, but remembered so its end tag closes what
            // was opened inside it.
            _ => self.push(name, |_| {}),
        }
    }

    fn end_tag(&mut self, name: &str) {
        match name {
            "table" => self.end_table(),
            "td" | "th" => {
                self.close_cell();
                if let Some(t) = &mut self.table {
                    t.cell = false;
                    self.between = Between::Cells;
                }
            }
            "tr" if self.table.is_some() => self.between = Between::Cells,
            "tr" | "tbody" | "thead" | "tfoot" | "html" | "body" | "head" | "br" | "hr" | "img" => {
            }
            _ => {
                let depth = self.table.as_ref().filter(|t| t.cell).map(|t| t.depth);
                let block_in_cell = depth.is_some_and(|depth| {
                    is_block(name)
                        && self.open[depth.min(self.open.len())..]
                            .iter()
                            .any(|o| &*o.name == name)
                });
                self.close(name);
                // Measured on Qt 6.11: what comes after a block's end in a
                // cell is a paragraph of its own, but after a div's it goes
                // on in the div's.
                if block_in_cell && let Some(t) = &mut self.table {
                    t.no_wrap &= name == "div";
                    t.fresh = false;
                    if name != "div"
                        && let Some(cell) = self.cell_mut()
                        && cell.paragraph == CellParagraph::Filled
                    {
                        cell.paragraph = CellParagraph::Ended;
                    }
                }
            }
        }
    }

    /// What the end tag of `name` does: None when it closes nothing, else
    /// whether it ends a block (a table's cells and rows are blocks here).
    fn ends(&self, name: &str) -> Option<bool> {
        match name {
            "table" => self.table.is_some().then_some(true),
            "td" | "th" => self.table.as_ref().is_some_and(|t| t.cell).then_some(true),
            "tr" => self.table.is_some().then_some(true),
            "tbody" | "thead" | "tfoot" => self.table.is_some().then_some(false),
            "html" => self.html_open.then_some(true),
            "body" => self.body_open.then_some(true),
            "head" | "br" | "hr" | "img" => None,
            _ => {
                let floor = self.table.as_ref().map_or(0, |t| t.depth);
                let inside = self.open.get(floor..)?;
                let at = inside.iter().rposition(|o| &*o.name == name)?;
                Some(inside[at..].iter().any(|o| o.boxed))
            }
        }
    }

    /// Close an open `name` that the next start tag ends without saying so.
    fn imply_end(&mut self, name: &str, through: &[&str]) {
        if self.open_within(name, through) {
            self.close(name);
        }
    }

    fn start_table(&mut self, attrs: &Attrs<'_>) {
        self.imply_end("p", &[]);
        self.flush(false);
        self.after_table = AfterTable::No;
        self.between = Between::Text;
        let number = |name, default: f32| {
            attrs
                .get(name)
                .and_then(|v| v.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .map_or(default, |v| v.clamp(0.0, 100.0))
        };
        self.table = Some(TableState {
            rows: Vec::new(),
            cell: false,
            border: number("border", 0.0),
            spacing: number("cellspacing", 2.0),
            padding: number("cellpadding", 0.0),
            background: background_of(attrs),
            width: attrs
                .get("width")
                .as_deref()
                .map(str::trim)
                .filter(|w| !w.is_empty())
                .map(Arc::from),
            depth: self.open.len(),
            white_space: self.white_space_for("table", attrs, self.white_space()),
            row_white_space: WhiteSpace::default(),
            no_wrap: false,
            fresh: true,
        });
        if let Some(t) = &mut self.table {
            t.row_white_space = t.white_space;
        }
    }

    /// Tags that mean something else inside a table. False for the others.
    fn table_tag(&mut self, name: &str, attrs: &Attrs<'_>) -> bool {
        let Some(t) = &mut self.table else {
            return false;
        };
        match name {
            "tr" => {
                self.close_cell();
                let Some(t) = &mut self.table else {
                    return true;
                };
                t.cell = false;
                t.rows.push(Vec::new());
                self.between = Between::Text;
                let inherited = t.white_space;
                let white_space = self.white_space_for(name, attrs, inherited);
                if let Some(t) = &mut self.table {
                    t.row_white_space = white_space;
                }
                true
            }
            "td" | "th" => {
                self.close_cell();
                let Some(t) = &mut self.table else {
                    return true;
                };
                if t.rows.is_empty() {
                    t.rows.push(Vec::new());
                }
                let header = name == "th";
                if let Some(row) = t.rows.last_mut() {
                    row.push(CellBuf {
                        header,
                        background: background_of(attrs),
                        ..CellBuf::default()
                    });
                }
                t.cell = true;
                self.between = Between::Text;
                let white_space = t.row_white_space;
                // Measured on Qt 6.12: the cell's alignment is its
                // paragraphs', unless one has its own.
                let cell_box = Ctx {
                    align: attrs.get("align").as_deref().and_then(align),
                    ..Ctx::default()
                };
                // An element of its own, so that the rules of a style sheet
                // and its style say what white space in it is.
                self.push(name, |o| {
                    o.style.bold = header;
                    o.white_space = white_space;
                    o.cell_box = Some(cell_box.opened());
                });
                true
            }
            // A table in a table is read as more rows of the outer one.
            "table" => true,
            // A block after a cell is in it, one before any is nothing.
            _ if !t.cell && is_block(name) => self.reopen_last_cell().is_none(),
            _ => false,
        }
    }

    /// Close what is open in the table's last cell, innermost first: the
    /// boxes of its paragraphs give it their bottom margins.
    fn close_cell(&mut self) {
        let Some(t) = &mut self.table else {
            return;
        };
        let depth = t.depth.min(self.open.len());
        let mut cell = t.rows.last_mut().and_then(|row| row.last_mut());
        for open in self.open.drain(depth..).rev() {
            if let (Some(ctx), Some(cell)) = (&open.cell_box, cell.as_deref_mut()) {
                cell.close_box(ctx);
            }
        }
    }

    fn end_table(&mut self) {
        self.close_cell();
        let Some(t) = self.table.take() else {
            return;
        };
        self.between = Between::Blocks;
        let (buf, mut table) = table_block(t.rows, t.border, t.spacing, t.padding);
        table.background = t.background;
        table.width = t.width;
        if table.cells.is_empty() {
            return;
        }
        self.flow.emit(buf, Some(BlockKind::Table), Some(table));
        self.after_table = AfterTable::Table;
    }

    fn image(&mut self, attrs: &Attrs<'_>) {
        let size = |name| {
            attrs
                .get(name)
                .and_then(|v| v.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
        };
        let image = InlineImage {
            index: 0,
            source: Arc::from(attrs.get("src").unwrap_or_default()),
            alt: Arc::from(attrs.get("alt").unwrap_or_default()),
            width: size("width"),
            height: size("height"),
            align: match attrs.get("align").as_deref() {
                Some("top") => BoxAlign::Top,
                Some("middle") => BoxAlign::Middle,
                _ => BoxAlign::Baseline,
            },
            in_spaces: false,
        };
        self.write(|buf| {
            buf.images.push(InlineImage {
                index: buf.text.len(),
                ..image
            });
        });
    }
}

/// What a `style` attribute says about colours and lines (see the module's
/// documentation), read the way Qt 6.11 does: declarations count in the
/// order they are written, and one of a property nobody knows or one that
/// is no `name: value` is passed over. Unless that one is the first: then
/// the whole style says nothing.
impl Open {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            style: Inline::default(),
            boxed: false,
            white_space: WhiteSpace::default(),
            empty: false,
            always: false,
            list: None,
            cell_box: None,
        }
    }
}

/// `<meta name="qrichtext" content="1">`, which an editor writes for its
/// document. Measured on Qt 6.11: the name and the value as written, and
/// the name first.
fn is_editors_mark(attrs: &Attrs<'_>) -> bool {
    let mut named = false;
    for (key, value) in attrs.pairs() {
        if key.eq_ignore_ascii_case("name") {
            named = value == "qrichtext";
        } else if key.eq_ignore_ascii_case("content") {
            return named && value == "1";
        }
    }
    false
}

/// A `<pre>` keeps its spaces and does not wrap.
// Qt 6.11: a `<pre>` of a document with the editor's mark wraps, as every
// block of one does, and only its font says it was a pre (6.12 the same).
// Ljós's editor says with `white-space` that a block does not wrap.
fn pre_white_space(open: &mut Open, verbatim: bool) {
    if !verbatim {
        open.white_space = WhiteSpace::Pre;
    }
}

fn apply_style(open: &mut Open, css: &str) {
    let css = without_comments(css);
    for (i, declaration) in css::declarations(&css).enumerate() {
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }
        let (name, value) = match declaration.split_once(':') {
            Some((name, value)) => (name.trim(), important(value.trim())),
            None => ("", ""),
        };
        let is_name = !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !is_name || value.is_empty() {
            if i == 0 {
                return;
            }
            continue;
        }
        let style = &mut open.style;
        match name.to_ascii_lowercase().as_str() {
            "color" => style.color = Some(Arc::from(value)),
            // Of the shorthand only a colour on its own is understood.
            "background" if value.contains(|c: char| c.is_ascii_whitespace()) => {}
            "background-color" | "background" => {
                if !open.boxed && !is_block(&open.name) {
                    style.background = Some(Arc::from(value));
                }
            }
            "text-decoration-color" => style.decoration_color = Some(Arc::from(value)),
            // A value nobody knows leaves what the element inherits.
            "white-space" => {
                if let Some(white_space) = WhiteSpace::of(value) {
                    open.white_space = white_space;
                }
            }
            // What an editor writes when it saves its text, so that the
            // text reads back as it was written.
            "font-weight" => match value.to_ascii_lowercase().as_str() {
                "bold" | "bolder" => style.bold = true,
                "normal" | "lighter" => style.bold = false,
                number => {
                    if let Ok(weight) = number.parse::<f32>() {
                        style.bold = weight >= 600.0;
                    }
                }
            },
            "font-style" => match value.to_ascii_lowercase().as_str() {
                "italic" | "oblique" => style.italic = true,
                "normal" => style.italic = false,
                _ => {}
            },
            "font-size" => {
                if let Some(scale) = size_keyword(value) {
                    style.scale = scale;
                } else if let Some(px) = css_pixels(value) {
                    style.pixel_size = Some(px);
                    style.in_points = value.trim().to_ascii_lowercase().ends_with("pt");
                }
            }
            // A list: the first family is the one the text names, the
            // rest are tried after it for the face, as in Qt.
            "font-family" => {
                if let Some(mut names) = css::families(value) {
                    let rest = names.split_off(1);
                    style.family = names.pop().map(Arc::from);
                    style.fallback_families = (!rest.is_empty())
                        .then(|| Arc::new(rest.into_iter().map(Arc::from).collect()));
                }
            }
            "text-decoration" => {
                let words = value.split(|c: char| c.is_ascii_whitespace() || c == ',');
                for word in words {
                    match word.to_ascii_lowercase().as_str() {
                        "underline" => style.underline = true,
                        "overline" => style.overline = true,
                        "line-through" => style.strike = true,
                        "none" => {
                            style.underline = false;
                            style.overline = false;
                            style.strike = false;
                            style.undecorated = true;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

/// The colour behind a table or a cell: its `bgcolor`, or the
/// `background-color` of its style, which wins as in Qt 6.11.
fn background_of(attrs: &Attrs<'_>) -> Option<Arc<str>> {
    let mut out = attrs
        .get("bgcolor")
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(Arc::from);
    if let Some(css) = attrs.get("style") {
        let css = without_comments(&css);
        for declaration in css::declarations(&css) {
            let Some((name, value)) = declaration.split_once(':') else {
                continue;
            };
            let name = name.trim().to_ascii_lowercase();
            let value = important(value.trim());
            let one_colour = !(name == "background" && value.contains(char::is_whitespace));
            if matches!(name.as_str(), "background-color" | "background")
                && one_colour
                && !value.is_empty()
            {
                out = Some(Arc::from(value));
            }
        }
    }
    out
}

/// The margins of a box (`margin-top` and its kin, in pixels), which an
/// editor writes for every paragraph. The `margin` shorthand is not read.
/// With `background`, a colour behind the box too.
fn apply_box_style(ctx: &mut Ctx, css: &str, background: bool) {
    let css = without_comments(css);
    for declaration in css::declarations(&css) {
        let Some((name, value)) = declaration.split_once(':') else {
            continue;
        };
        let name_lower = name.trim().to_ascii_lowercase();
        if background && matches!(name_lower.as_str(), "background-color" | "background") {
            let value = important(value.trim());
            // Of the shorthand only a colour on its own is understood.
            if !value.is_empty()
                && !(name_lower == "background"
                    && value.contains(|c: char| c.is_ascii_whitespace()))
            {
                ctx.background = Some(Arc::from(value));
            }
            continue;
        }
        if name.trim().eq_ignore_ascii_case("-qt-list-indent") {
            if let Ok(levels) = value.trim().parse::<u8>() {
                ctx.indent = 40.0 * f32::from(levels.min(50));
            }
            continue;
        }
        let Some(px) = value
            .trim()
            .strip_suffix("px")
            .and_then(|n| n.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0.0, 2000.0))
        else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "margin-top" => ctx.margin_top = px,
            "margin-bottom" => ctx.margin_bottom = px,
            "margin-left" => ctx.indent = px,
            "margin-right" => ctx.indent_right = px,
            _ => {}
        }
    }
}

/// The scale of a `font-size` keyword: the steps of `<font size>`.
pub(crate) fn size_keyword(value: &str) -> Option<f32> {
    let step = match value.to_ascii_lowercase().as_str() {
        "xx-small" | "x-small" => 0,
        "small" => 1,
        "medium" => 2,
        "large" => 3,
        "x-large" => 4,
        "xx-large" => 5,
        "xxx-large" => 6,
        _ => return None,
    };
    FONT_SCALE.get(step).copied()
}

/// The keyword of a scale [`size_keyword`] reads, where there is one.
pub(crate) fn scale_keyword(scale: f32) -> Option<&'static str> {
    const NAMES: [&str; 7] = [
        "x-small",
        "small",
        "medium",
        "large",
        "x-large",
        "xx-large",
        "xxx-large",
    ];
    let i = FONT_SCALE.iter().position(|s| (s - scale).abs() < 1e-4)?;
    NAMES.get(i).copied()
}

/// A length of `px` or `pt` (a point at the platform's logical DPI,
/// [`ljos_runtime::points_to_pixels`]), within sizes a font can have.
fn css_pixels(value: &str) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    let px = if let Some(n) = value.strip_suffix("px") {
        n.trim().parse::<f32>().ok()?
    } else {
        let pt = value.strip_suffix("pt")?.trim().parse::<f32>().ok()?;
        ljos_runtime::points_to_pixels(f64::from(pt)) as f32
    };
    (px.is_finite() && px > 0.0).then(|| px.min(1000.0))
}

/// `css` without its `/* comments */`. One that never ends takes the rest.
pub(super) fn without_comments(css: &str) -> Cow<'_, str> {
    if !css.contains("/*") {
        return css.into();
    }
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = match rest[start + 2..].find("*/") {
            Some(end) => &rest[start + 2 + end + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    out.into()
}

/// A value without a trailing `!important`, which changes nothing where
/// there is one style to read.
fn important(value: &str) -> &str {
    match value.rsplit_once('!') {
        Some((before, word)) if word.trim().eq_ignore_ascii_case("important") => before.trim_end(),
        _ => value,
    }
}

/// `value` with each `&name;` that names a character read as it. Unlike in
/// text, Qt 6.12 reads one in an attribute only with its semicolon.
fn unescaped(value: &str) -> Cow<'_, str> {
    if !value.contains('&') {
        return Cow::Borrowed(value);
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        match attr_reference(after) {
            Some((c, len)) => {
                out.push(c);
                rest = &after[len..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// The character the reference after an attribute's `&` names, and the
/// bytes it takes, `;` included. Only a name's characters come before the
/// `;`, so a bare `&` costs its name and not a scan to the next `;`; `+` is
/// one because Qt 6.12 reads `&#+65;` as `A`.
fn attr_reference(after: &str) -> Option<(char, usize)> {
    let name_len = after
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '#' | '+')))
        .unwrap_or(after.len());
    #[cfg(test)]
    tests::SCANNED.with(|n| n.set(n.get() + name_len + 1));
    if !after[name_len..].starts_with(';') {
        return None;
    }
    Some((entity::decode(&after[..name_len])?, name_len + 1))
}

/// Where a tag ends: at the first `>` outside a quoted attribute value.
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote = None;
    for (i, b) in rest.bytes().enumerate() {
        match (quote, b) {
            (None, b'>') => return Some(i),
            (None, b'"' | b'\'') => quote = Some(b),
            (Some(q), _) if q == b => quote = None,
            _ => {}
        }
    }
    // A quote that never closes does not hide the tag's end.
    rest.find('>')
}

/// White space as read between blocks: a no-break space too.
fn is_space(c: char) -> bool {
    c.is_ascii_whitespace() || matches!(c, '\u{b}' | '\u{a0}')
}

/// White space between blocks where spaces collapse: each run of it one
/// `space`, none at the start `after_space`; a no-break space stays one.
fn collapsed_gap(text: &str, space: &str, after_space: bool) -> (String, bool) {
    let mut out = String::new();
    let mut after_space = after_space;
    for c in text.chars() {
        if c == '\u{a0}' {
            out.push(c);
            after_space = false;
        } else if !after_space {
            out.push_str(space);
            after_space = true;
        }
    }
    (out, after_space)
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "pre"
            | "ul"
            | "ol"
            | "li"
            | "blockquote"
            | "dl"
            | "dt"
            | "dd"
            | "div"
            | "center"
            | "hr"
    )
}

fn heading_level(name: &str) -> Option<u8> {
    match name.as_bytes() {
        [b'h', d @ b'1'..=b'6'] => Some(d - b'0'),
        _ => None,
    }
}

/// `h1` to `h5` are bold and scaled; `h6` is plain text in Qt 6.11.
fn heading_style(style: &mut Inline, level: u8) {
    if (1..6).contains(&level) {
        style.bold = true;
        style.scale = HEADING_SCALE[usize::from(level - 1)];
    }
}

/// `size="4"`, `"+1"`, `"-1"`; out of range is the nearest step.
fn font_size(value: &str) -> Option<f32> {
    let value = value.trim();
    let size = if value.starts_with(['+', '-']) {
        value.parse::<i32>().ok()?.saturating_add(3)
    } else {
        value.parse::<i32>().ok()?
    };
    let index = usize::try_from(size.clamp(1, 7) - 1).ok()?;
    FONT_SCALE.get(index).copied()
}

fn align(value: &str) -> Option<HAlign> {
    match value.trim().to_ascii_lowercase().as_str() {
        "left" => Some(HAlign::Left),
        "right" => Some(HAlign::Right),
        "center" => Some(HAlign::Center),
        "justify" => Some(HAlign::Justify),
        _ => None,
    }
}

fn list_marker(ordered: bool, kind: Option<&str>, depth: usize) -> Marker {
    match (ordered, kind) {
        (false, Some("disc")) => Marker::Disc,
        (false, Some("circle")) => Marker::Circle,
        (false, Some("square")) => Marker::Square,
        (false, _) => match depth {
            0 => Marker::Disc,
            1 => Marker::Circle,
            _ => Marker::Square,
        },
        (true, Some("a")) => Marker::LowerAlpha,
        (true, Some("A")) => Marker::UpperAlpha,
        (true, Some("i")) => Marker::LowerRoman,
        (true, Some("I")) => Marker::UpperRoman,
        (true, _) => Marker::Decimal,
    }
}

/// The attributes of a tag: `name="value"`, `name='value'` or `name=value`.
struct Attrs<'a>(&'a str);

impl<'a> Attrs<'a> {
    /// The value of `name`, its character references read as Qt reads
    /// them: what an editor escapes when it writes an attribute comes back.
    fn get(&self, name: &str) -> Option<Cow<'a, str>> {
        self.raw(name).map(unescaped)
    }

    fn raw(&self, name: &str) -> Option<&'a str> {
        self.pairs()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)
    }

    /// Every attribute and its value as written, in order.
    fn pairs(&self) -> impl Iterator<Item = (&'a str, &'a str)> {
        let mut rest = self.0;
        std::iter::from_fn(move || {
            loop {
                rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
                if rest.is_empty() {
                    return None;
                }
                let key_len = rest
                    .find(|c: char| c.is_ascii_whitespace() || c == '=' || c == '/')
                    .unwrap_or(rest.len());
                // A stray `=` is skipped so the scan always moves on.
                if key_len == 0 {
                    rest = &rest[1..];
                    continue;
                }
                let key = &rest[..key_len];
                let after = rest[key_len..].trim_start();
                let Some(value) = after.strip_prefix('=') else {
                    // An attribute without a value.
                    rest = after;
                    return Some((key, ""));
                };
                let value = value.trim_start();
                let (value, next) = match value.chars().next() {
                    Some(q @ ('"' | '\'')) => match value[1..].find(q) {
                        Some(end) => (&value[1..1 + end], &value[end + 2..]),
                        None => (&value[1..], ""),
                    },
                    _ => {
                        let end = value
                            .find(|c: char| c.is_ascii_whitespace())
                            .unwrap_or(value.len());
                        (&value[..end], &value[end..])
                    }
                };
                rest = next;
                return Some((key, value));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use BlockKind::{Code, Heading, List, Paragraph, Quote, Rule};

    /// Kind, text, margins (top, bottom) and indents (left, right).
    type Shape = (BlockKind, String, (f32, f32), (f32, f32));

    /// What every block of `source` is.
    fn shape(source: &str) -> Vec<Shape> {
        parse(source)
            .into_iter()
            .map(|b| {
                (
                    b.kind,
                    b.text,
                    (b.margin_top, b.margin_bottom),
                    (b.indent, b.indent_right),
                )
            })
            .collect()
    }

    fn texts(source: &str) -> Vec<String> {
        parse(source).into_iter().map(|b| b.text).collect()
    }

    #[test]
    fn paragraphs_and_headings_have_qts_margins() {
        assert_eq!(
            shape("<p>abc</p><p>abc</p>"),
            vec![
                (Paragraph, "abc".into(), (12.0, 12.0), (0.0, 0.0)),
                (Paragraph, "abc".into(), (12.0, 12.0), (0.0, 0.0)),
            ]
        );
        assert_eq!(
            shape("abc<h1>abc</h1>abc"),
            vec![
                (Paragraph, "abc".into(), (0.0, 0.0), (0.0, 0.0)),
                (Heading(1), "abc".into(), (18.0, 12.0), (0.0, 0.0)),
                (Paragraph, "abc".into(), (0.0, 0.0), (0.0, 0.0)),
            ]
        );
        assert_eq!(shape("<h5>a</h5>")[0].2, (12.0, 4.0));
        assert_eq!(shape("<h6>a</h6>")[0].2, (0.0, 0.0));
        let h6 = &parse("<h6>a</h6>")[0];
        assert!(h6.spans.is_empty(), "h6 is plain text in Qt");
        let h2 = &parse("<h2>a</h2>")[0];
        assert!(h2.spans[0].style.bold);
        assert_eq!(h2.spans[0].style.scale, 1.5);
    }

    #[test]
    fn an_open_paragraph_ends_where_the_next_block_starts() {
        assert_eq!(texts("<p>abc<p>abc"), ["abc", "abc"]);
        assert_eq!(texts("abc<p>abc"), ["abc", "abc"]);
        assert_eq!(texts("<p>abc</p>abc"), ["abc", "abc"]);
        assert_eq!(texts("<p>abc</p> <p>abc</p>"), ["abc", "abc"]);
        assert_eq!(texts("<p></p>"), [""]);
        assert_eq!(texts(""), [""]);
        // Stray end tags are nothing.
        assert_eq!(texts("abc</p>abc"), ["abcabc"]);
        assert_eq!(texts("<p>abc</h1>abc"), ["abcabc"]);
        // Measured: text after a `div` goes on in its block.
        assert_eq!(texts("<div>abc</div>abc"), ["abcabc"]);
        assert_eq!(texts("<div>abc</div><div>abc</div>"), ["abc", "abc"]);
        assert_eq!(texts("<center>abc</center>abc"), ["abc", "abc"]);
    }

    #[test]
    fn lists_indent_and_number_their_items() {
        let blocks = parse("abc<ul><li>abc</li><li>abc</li></ul>abc");
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[1].kind, List);
        assert_eq!(blocks[1].indent, 40.0);
        assert_eq!(blocks[1].marker.as_deref(), Some("\u{2022}"));
        assert_eq!((blocks[1].margin_top, blocks[1].margin_bottom), (12.0, 0.0));
        assert_eq!((blocks[2].margin_top, blocks[2].margin_bottom), (0.0, 12.0));
        assert_eq!(blocks[3].indent, 0.0);

        let blocks = parse("<ol><li>a<li>b<ul><li>c</ul><li>d</ol>");
        let markers: Vec<_> = blocks.iter().map(|b| b.marker.as_deref()).collect();
        assert_eq!(
            markers,
            [Some("1."), Some("2."), Some("\u{25E6}"), Some("3.")]
        );
        assert_eq!(blocks[2].indent, 80.0);
        // The nested list has its top margin.
        assert_eq!(blocks[2].margin_top, 12.0);
        // Text right in a list is indented like an item, without a marker.
        let blocks = parse("<ul>abc</ul>");
        assert_eq!((blocks[0].indent, blocks[0].marker.clone()), (40.0, None));
        // An item outside a list is a block without either.
        let blocks = parse("<li>abc</li>");
        assert_eq!((blocks[0].indent, blocks[0].marker.clone()), (0.0, None));
    }

    #[test]
    fn quotes_code_rules_and_definitions() {
        assert_eq!(
            shape("abc<blockquote>abc</blockquote>abc")[1],
            (Quote, "abc".into(), (12.0, 12.0), (40.0, 40.0))
        );
        let pre = &parse("<pre>a  b\nc</pre>")[0];
        assert_eq!((pre.kind, pre.text.as_str()), (Code, "a  b\nc"));
        assert!(pre.spans[0].style.mono);
        assert_eq!(
            shape("abc<hr>abc").iter().map(|b| b.0).collect::<Vec<_>>(),
            [Paragraph, Rule, Paragraph]
        );
        let dl = shape("<dl><dt>abc</dt><dd>abc</dd></dl>");
        assert_eq!((dl[0].3.0, dl[1].3.0), (0.0, 30.0));
    }

    #[test]
    fn white_space_collapses_and_breaks_are_kept() {
        assert_eq!(texts("abc \n abc"), ["abc abc"]);
        assert_eq!(texts(" abc "), ["abc "]);
        assert_eq!(texts("abc<br>abc"), ["abc\nabc"]);
        assert_eq!(texts("abc <br> abc"), ["abc\nabc"]);
        assert_eq!(texts("abc<br>"), ["abc\n"]);
    }

    /// What Qt 6.11 makes of each, measured with qml6.
    #[test]
    fn white_space_pre_and_pre_wrap_keep_spaces() {
        let keep =
            |ws: &str, body: &str| texts(&format!("<p style=\"white-space:{ws}\">{body}</p>"));
        assert_eq!(
            texts("<p style=\"margin:0;white-space:pre-wrap\">def f():<br/>    return 1</p>"),
            ["def f():\n    return 1"]
        );
        assert_eq!(keep("pre", "a   b"), ["a   b"]);
        assert_eq!(keep("pre-wrap", "  a   b  "), ["  a   b  "]);
        assert_eq!(keep("PRE-WRAP !important", "a   b"), ["a   b"]);
        assert_eq!(keep("pre-wrap", "a\tb"), ["a\tb"]);
        // The space before a break stays.
        assert_eq!(keep("pre-wrap", "a  <br>  b"), ["a  \n  b"]);
        // A line feed ends the paragraph; the next has the same format,
        // and the paragraph's margins are at the ends of the two.
        assert_eq!(
            shape("<p style=\"white-space:pre-wrap\">a  \n\nb</p>"),
            [
                (Paragraph, "a  ".into(), (12.0, 0.0), (0.0, 0.0)),
                (Paragraph, String::new(), (0.0, 0.0), (0.0, 0.0)),
                (Paragraph, "b".into(), (0.0, 12.0), (0.0, 0.0)),
            ]
        );
        // One line feed right after the start tag is not text, nor one
        // at the end.
        assert_eq!(keep("pre-wrap", "\n\n  a\n"), ["", "  a"]);
        assert_eq!(keep("pre-wrap", "<b>\na</b>"), ["", "a"]);
        assert_eq!(texts("<pre>\n  a</pre>"), ["  a"]);
    }

    #[test]
    fn white_space_from_a_style_sheet_and_inherited() {
        let sheet = "<style>p, li { white-space: pre-wrap; }</style>";
        assert_eq!(texts(&format!("{sheet}<p>  a   b</p>")), ["  a   b"]);
        assert_eq!(
            texts(&format!("{sheet}<ul><li>  a   b</li></ul>")),
            ["  a   b"]
        );
        // The element's own style wins over the rule.
        assert_eq!(
            texts(
                "<style>p { white-space: pre-wrap }</style><p style=\"white-space:normal\">a   b</p>"
            ),
            ["a b"]
        );
        // What is inside inherits it, and may say otherwise.
        assert_eq!(
            texts("<p style=\"white-space:pre-wrap\"><b>  a</b>   b</p>"),
            ["  a   b"]
        );
        assert_eq!(
            texts(
                "<p style=\"white-space:pre-wrap\"><span style=\"white-space:normal\">a   b</span>  c</p>"
            ),
            ["a b  c"]
        );
        assert_eq!(
            texts("<span style=\"white-space:pre-wrap\">a   b</span>  c"),
            ["a   b c"]
        );
        assert_eq!(
            texts(
                "<div style=\"white-space:pre-wrap\"><p style=\"white-space:bogus\">a   b</p></div>"
            ),
            ["a   b"]
        );
        assert_eq!(
            texts(
                "<table><tr><td><p style=\"white-space:pre-wrap\">a   b\nc</p></td></tr></table>"
            )[0],
            "a   b\nc"
        );
        // In a cell too, a line feed right after a block's start tag is
        // not text.
        let cell =
            |html: &str| texts(&format!("<table><tr><td>{html}</td></tr></table>"))[0].clone();
        assert_eq!(cell("<pre>\ncode</pre>"), "code");
        assert_eq!(cell("x<pre>\ncode</pre>"), "x\ncode");
        assert_eq!(cell("<p style=\"white-space:pre-wrap\">\na</p>"), "a");
    }

    #[test]
    fn white_space_normal_nowrap_and_pre_line_collapse_spaces() {
        let ws = |ws: &str, body: &str| texts(&format!("<p style=\"white-space:{ws}\">{body}</p>"));
        assert_eq!(ws("normal", "  a   b"), ["a b"]);
        // nowrap's space is one that does not break, and white space after
        // it collapses into it, but not after a `&nbsp;`.
        assert_eq!(ws("nowrap", "  a   b"), ["a\u{a0}b"]);
        assert_eq!(ws("nowrap", "a&nbsp; b"), ["a\u{a0}\u{a0}b"]);
        assert_eq!(
            ws("nowrap", "a <span style=\"white-space:normal\"> b</span>"),
            ["a\u{a0}b"]
        );
        assert_eq!(ws("nowrap", "a <br> b"), ["a\u{a0}\nb"]);
        assert_eq!(texts("<p>a&nbsp; b</p>"), ["a\u{a0} b"]);
        assert_eq!(ws("bogus", "  a   b"), ["a b"]);
        assert_eq!(
            texts("<pre style=\"white-space:normal\">a   b</pre>"),
            ["a b"]
        );
        // pre-line keeps line feeds, not the spaces around them; the
        // space before a break stays.
        assert_eq!(ws("pre-line", "  a  \n  b"), ["a", "b"]);
        assert_eq!(ws("pre-line", "a <br> b"), ["a \nb"]);
    }

    #[test]
    fn inline_tags_style_their_text() {
        let b = &parse("a<b>b</b><em>c</em><code>d</code><big>e</big><small>f</small><s>g</s>")[0];
        assert_eq!(b.text, "abcdefg");
        let s: Vec<_> = b.spans.iter().map(|s| &s.style).collect();
        assert!(s[0].bold);
        assert!(s[1].italic);
        assert!(s[2].mono);
        assert_eq!(s[3].scale, 1.2);
        assert_eq!(s[4].scale, 0.8);
        assert!(s[5].strike);
        // Attributes do not make a tag unknown here, and need no quotes.
        let b = &parse("<b x=\"y\">abc</b><font size=5 color=red>d</font>")[0];
        assert!(b.spans[0].style.bold);
        assert_eq!(b.spans[1].style.scale, 1.5);
        assert_eq!(b.spans[1].style.color.as_deref(), Some("red"));
        // Out of range is the nearest size.
        assert_eq!(
            parse("<font size=\"8\">a</font>")[0].spans[0].style.scale,
            2.4
        );
        assert_eq!(
            parse("<font size=\"0\">a</font>")[0].spans[0].style.scale,
            0.7
        );
        // A tag closed in itself styles nothing.
        assert!(parse("<b/>abc")[0].spans.is_empty());
        // An end tag that matches nothing is nothing.
        let b = &parse("<b>abc</i>abc")[0];
        assert_eq!(b.spans[0].range, 0..6);
        // A style runs on through blocks.
        let blocks = parse("<b><p>a</p><p>b</p></b>");
        assert!(blocks.iter().all(|b| b.spans[0].style.bold));
    }

    #[test]
    fn links_keep_their_address_as_text() {
        let b = &parse("see <a href=\"javascript:alert(1)\">this</a>")[0];
        assert_eq!(b.text, "see this");
        assert_eq!(
            b.spans[0].style.link.as_deref(),
            Some("javascript:alert(1)"),
            "the address is reported, never followed"
        );
    }

    #[test]
    fn scripts_styles_titles_and_comments_are_dropped() {
        assert_eq!(texts("a<script>alert(1)</script>b"), ["ab"]);
        assert_eq!(texts("a<SCRIPT>alert(1)</ScRiPt >b"), ["ab"]);
        assert_eq!(texts("a<style>p { color: red }</style>b"), ["ab"]);
        assert_eq!(
            texts("<html><head><title>t</title></head><body>abc</body></html>"),
            ["abc"]
        );
        assert_eq!(texts("a<script>never closed"), ["a"]);
        assert_eq!(texts("a<!-- c > d -->b"), ["ab"]);
        assert_eq!(texts("a<foo>b</foo>c"), ["abc"]);
        assert_eq!(texts("a<p style=\"margin:900px\">b</p>")[1], "b");
        assert_eq!(shape("<p style=\"margin:900px\">b</p>")[0].2, (12.0, 12.0));
    }

    #[test]
    fn of_a_fragment_only_what_is_between_its_marks_is_read() {
        // Measured on Qt 6.12: the elements opened before the start do
        // not count, and the end mark ends the text.
        let pre = parse("<pre><!--StartFragment-->x y<!--EndFragment--></pre>");
        assert_eq!(pre.len(), 1);
        assert_eq!((pre[0].kind, pre[0].text.as_str()), (Paragraph, "x y"));
        assert!(!pre[0].no_wrap);
        assert!(pre[0].spans.iter().all(|s| !s.style.mono));
        let bold =
            parse("<p style=\"font-weight:700\"><!--StartFragment-->x<!--EndFragment--></p>");
        assert!(bold[0].spans.iter().all(|s| !s.style.bold));
        assert_eq!(
            texts("<p>a<!--StartFragment-->b<!--EndFragment-->c</p>"),
            ["b"]
        );
        assert_eq!(texts("<h1><!--StartFragment-->x</h1>"), ["x"]);
        assert_eq!(shape("<h1><!--StartFragment-->x</h1>")[0].0, Paragraph);
        assert_eq!(
            texts("<p>x</p><!--StartFragment--><p>y</p><!--EndFragment--><p>z</p>"),
            ["y"]
        );
        // An end mark with no start is a comment like any other.
        assert_eq!(texts("<p>a<!--EndFragment-->b</p>"), ["ab"]);
    }

    #[test]
    fn malformed_tags_swallow_what_qt_swallows() {
        assert_eq!(texts("<b"), [""]);
        assert_eq!(texts("abc<"), ["abc"]);
        assert_eq!(texts("a < b"), ["a "]);
        assert_eq!(texts("a > b"), ["a > b"]);
        assert_eq!(texts("<>"), [""]);
        assert_eq!(texts("<>&\" '"), ["&\" '"]);
    }

    #[test]
    fn references_decode_with_or_without_their_semicolon() {
        assert_eq!(texts("&lt;&gt;&amp;&quot;&apos;"), ["<>&\"'"]);
        assert_eq!(texts("&amp"), ["&"]);
        assert_eq!(texts("&"), ["&"]);
        assert_eq!(texts("a & b"), ["a & b"]);
        assert_eq!(texts("&foo;"), ["&foo;"]);
        assert_eq!(texts("&#65;&#x41;"), ["AA"]);
    }

    #[test]
    fn tables_are_one_block_of_cells() {
        let b = &parse(
            "<table border=\"1\" cellpadding=\"5\"><tr><th>a</th><td>b</td></tr>\
             <tr><td>c</td><td>d</td></tr></table>",
        )[0];
        assert_eq!(b.kind, BlockKind::Table);
        assert_eq!(b.text, "a\tb\nc\td");
        let t = b.table.as_ref().expect("a table");
        assert_eq!((t.columns, t.cells.len()), (2, 4));
        assert_eq!((t.border, t.spacing, t.padding), (1.0, 2.0, 5.0));
        assert!(t.cells[0].header && !t.cells[1].header);
        assert_eq!(t.cells[3].range, 6..7);
        assert_eq!((t.cells[3].row, t.cells[3].column), (1, 1));
        assert!(b.spans[0].style.bold, "a header cell is bold");
        // Around a table: no margins.
        let blocks = shape("abc<table><tr><td>abc</td></tr></table>abc");
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[1].2, (0.0, 0.0));
        // Blocks in a cell are lines of it.
        let b = &parse("<table><tr><td><p>a</p><p>b</p></td></tr></table>")[0];
        assert_eq!(b.text, "a\nb");
        // A table that never ends still is one.
        assert_eq!(texts("<table><tr><td>a<td>b"), ["a\tb"]);
    }

    #[test]
    fn pre_and_nowrap_blocks_do_not_wrap() {
        let no_wrap =
            |source: &str| -> Vec<bool> { parse(source).into_iter().map(|b| b.no_wrap).collect() };
        assert_eq!(no_wrap("<p>a</p><pre>b</pre>"), [false, true]);
        assert_eq!(
            no_wrap(
                "<p style=\"white-space:pre\">a</p><p style=\"white-space:nowrap\">b</p>\
                 <p style=\"white-space:pre-wrap\">c</p><p style=\"white-space:pre-line\">d</p>"
            ),
            [true, true, false, false]
        );
        // The block says it, inherited or its own; a span inside cannot.
        assert_eq!(
            no_wrap(
                "<div style=\"white-space:nowrap\"><p>a</p></div><h3 style=\"white-space:pre\">b</h3>"
            ),
            [true, true]
        );
        assert_eq!(
            no_wrap("<p><span style=\"white-space:nowrap\">a</span></p>"),
            [false]
        );
        assert_eq!(
            no_wrap("<ul><li style=\"white-space:nowrap\">a</li></ul>"),
            [true]
        );
        // A table is no block that wraps or not; its cells say.
        assert_eq!(
            no_wrap("<table style=\"white-space:nowrap\"><tr><td>a</td></tr></table>"),
            [false]
        );
    }

    #[test]
    fn a_cell_says_its_white_space_and_which_lines_do_not_wrap() {
        // Ranges as pairs: clippy reads a one-range vec as a typo.
        let cell = |source: &str| -> (String, Vec<(usize, usize)>) {
            let b = parse(source).remove(0);
            let t = b.table.expect("a table");
            let no_wrap = t.cells[0]
                .no_wrap
                .iter()
                .map(|r| (r.start, r.end))
                .collect();
            (b.text, no_wrap)
        };
        let table = |td: &str| format!("<table><tr>{td}</tr></table>");
        assert_eq!(
            cell(&table("<td style=\"white-space:nowrap\">a   b</td>")),
            ("a\u{a0}b".into(), vec![(0, 4)])
        );
        assert_eq!(
            cell(&table("<td style=\"white-space:pre-wrap\">a   b</td>")),
            ("a   b".into(), vec![])
        );
        assert_eq!(
            cell(&table("<th style=\"white-space:pre\">a   b</th>")),
            ("a   b".into(), vec![(0, 5)])
        );
        // From a rule, the row or the table.
        assert_eq!(
            cell(&format!(
                "<style>td {{ white-space: pre }}</style>{}",
                table("<td>a  b</td>")
            )),
            ("a  b".into(), vec![(0, 4)])
        );
        assert_eq!(
            cell("<table><tr style=\"white-space:pre-wrap\"><td>a  b</td></tr></table>").0,
            "a  b"
        );
        assert_eq!(
            cell("<table style=\"white-space:nowrap\"><tr><td>a b</td></tr></table>").1,
            [(0, 4)]
        );
        // A cell's own colour stays the cell's, not its text's.
        let b = &parse(&table("<td style=\"background-color:red\">a</td>"))[0];
        assert!(b.spans.iter().all(|s| s.style.background.is_none()));
        // Only the lines of a paragraph that does not wrap.
        assert_eq!(
            cell(&table(
                "<td><p>aa</p><p style=\"white-space:nowrap\">b c</p><p>d</p></td>"
            )),
            ("aa\nb\u{a0}c\nd".into(), vec![(3, 7)])
        );
        // What the cell says holds for its first paragraph, which a block
        // that starts it joins. A block after text, or after a block's end,
        // starts one of its own. White space that is nothing marks no line.
        let nowrap = "style=\"white-space:nowrap\"";
        let normal = "style=\"white-space:normal\"";
        let td = |inner: &str| cell(&table(&format!("<td {nowrap}>{inner}</td>")));
        assert_eq!(
            td(&format!("\n<p {normal}>a b</p>")),
            ("a b".into(), vec![(0, 3)])
        );
        assert_eq!(
            td(&format!("<p {normal}>a<br>b c</p>")),
            ("a\nb c".into(), vec![(0, 1), (2, 5)])
        );
        assert_eq!(
            td(&format!("<p {normal}>x</p><p {normal}>a b</p>")),
            ("x\na b".into(), vec![(0, 1)])
        );
        // A paragraph after a line break: the break stays the first
        // paragraph's (measured on Qt 6.11, hal-c2/ljos-ui#283).
        assert_eq!(
            td(&format!("x<br><p {normal}>a b</p>")),
            ("x\n\na b".into(), vec![(0, 1)])
        );
        assert_eq!(
            td(&format!("<p {normal}></p><p {normal}>a b</p>")),
            ("a b".into(), vec![])
        );
        assert_eq!(
            cell(&table(&format!(
                "<td><div {nowrap}><p {normal}>a b</p></div></td>"
            ))),
            ("a b".into(), vec![(0, 3)])
        );
        assert_eq!(
            cell(&table(&format!("<td>x<p {nowrap}></p>a b</td>"))),
            ("x\na b".into(), vec![])
        );
        // Text where the block around it does not wrap makes its paragraph
        // not wrap, and after a div's end it goes on in the div's.
        assert_eq!(
            td(&format!("z<div {normal}>x</div>a b")),
            ("z\nxa\u{a0}b".into(), vec![(0, 1), (2, 7)])
        );
        // In a pre-wrap cell, a line feed right after its start tag is no
        // text; after an inline one it is.
        assert_eq!(
            cell(&table("<td style=\"white-space:pre-wrap\">\na</td>")).0,
            "a"
        );
        assert_eq!(
            cell(&table("<td style=\"white-space:pre-wrap\"><b>\na</b></td>")).0,
            "\na"
        );
    }

    #[test]
    fn a_cell_holds_paragraphs_of_its_own() {
        // Measured on Qt 6.11 (features/text-input/rich-text-cell-paragraphs.feature).
        let cell = |td: &str| -> (String, Vec<usize>) {
            let b = parse(&format!("<table><tr><td{td}</td></tr></table>")).remove(0);
            let t = b.table.expect("a table");
            (b.text, t.cells[0].paragraphs.clone())
        };
        let pre_wrap = " style=\"white-space:pre-wrap\">";
        for (td, text, paragraphs) in [
            ("><p>a</p><p>b</p>", "a\nb", vec![1]),
            (">a<p>b</p>", "a\nb", vec![1]),
            ("><h1>a</h1>b", "a\nb", vec![1]),
            ("><ul><li>a</li></ul>b", "a\nb", vec![1]),
            ("><p>a</p><br>", "a\n\n", vec![1]),
            (">a<br><p>b</p>", "a\n\nb", vec![2]),
            (">a<p></p>", "a\n", vec![1]),
            ("><p></p><p>b</p>", "b", vec![]),
            ("><div>a</div>b", "ab", vec![]),
            ("><pre>a\nb</pre>", "a\nb", vec![1]),
            // White space that collapses to nothing starts none.
            ("><p>a</p> <p>b</p>", "a\nb", vec![1]),
        ] {
            assert_eq!(cell(td), (text.into(), paragraphs), "{td}");
        }
        for (td, text, paragraphs) in [
            ("a\nb", "a\nb", vec![1]),
            ("\na", "a", vec![]),
            // A kept line feed that ends the cell ends nothing.
            ("a\n", "a", vec![]),
            ("a<p>\nb</p>", "a\nb", vec![1]),
            ("<p>a</p>\n<p>b</p>", "a\nb", vec![1]),
            ("x\n<p></p>", "x\n\n", vec![1, 2]),
            ("<span>\n</span><p></p>", "\n\n", vec![0, 1]),
        ] {
            assert_eq!(
                cell(&format!("{pre_wrap}{td}")),
                (text.into(), paragraphs),
                "{td}"
            );
        }
    }

    #[test]
    fn the_editors_mark_has_every_space_read_as_written() {
        let mark = "<meta name=\"qrichtext\" content=\"1\" />";
        let with = |body: &str| texts(&format!("{mark}{body}"));
        assert_eq!(with("<p>  a \t b</p>"), ["  a \t b"]);
        // No line feed is text, not even one written as a reference.
        assert_eq!(with("<p>a\nb\r\nc&#10;d&#13;e</p>"), ["abcde"]);
        assert_eq!(with("<p>a&#9;&nbsp;b</p>"), ["a\t\u{a0}b"]);
        assert_eq!(with("<p>a  <br>  b</p>"), ["a  \n  b"]);
        // white-space keeps no space, and says only whether a block
        // wraps, from a style sheet too; a pre that does not say wraps.
        let b = &parse(&format!(
            "{mark}<style>p {{ white-space: nowrap }}</style><p>a  b</p>\
             <p style=\"white-space:pre-wrap\">c</p><pre>d</pre><pre style=\"white-space:pre\">e</pre>"
        ));
        assert_eq!(b[0].text, "a  b");
        let no_wrap: Vec<bool> = b.iter().map(|b| b.no_wrap).collect();
        assert_eq!(no_wrap, [true, false, false, true]);
        let b = &parse(&format!(
            "{mark}<table><tr><td style=\"white-space:nowrap\">a  b</td><td><pre>c</pre></td></tr></table>"
        ))[0];
        assert_eq!(b.text, "a  b\tc");
        let cells = &b.table.as_ref().expect("a table").cells;
        assert!(!cells[0].no_wrap.is_empty());
        assert!(cells[1].no_wrap.is_empty());
        // White space between blocks goes to the block before, without
        // its line feeds.
        assert_eq!(with("<p>a</p>\n  <p>b</p>\n"), ["a  ", "b"]);
        assert_eq!(with("<ul>\n  <li>a</li>\n</ul>"), ["a"]);
        // The mark counts wherever it stands, outside a comment.
        assert_eq!(texts(&format!("<p>a  b</p>{mark}")), ["a  b"]);
        assert_eq!(texts(&format!("<!-- {mark} --><p>a  b</p>")), ["a b"]);
        // Its name first, its content exactly 1, and the name as written.
        let meta = |meta: &str| texts(&format!("{meta}<p>a  b</p>"));
        assert_eq!(meta("<meta name=qrichtext content=1>"), ["a  b"]);
        assert_eq!(meta("<meta content=\"1\" name=\"qrichtext\">"), ["a b"]);
        assert_eq!(meta("<meta name=\"qrichtext\" content=\"2\">"), ["a b"]);
        assert_eq!(meta("<meta name=\"qrichtext\" content=\" 1\">"), ["a b"]);
        assert_eq!(meta("<meta name=\"QRICHTEXT\" content=\"1\">"), ["a b"]);
    }

    #[test]
    fn images_are_their_source_and_size() {
        let b = &parse("x<img src=\"a.png\" width=30 height=20>y")[0];
        assert_eq!(b.text, "xy");
        assert_eq!(b.images[0].index, 1);
        assert_eq!(&*b.images[0].source, "a.png");
        assert_eq!(b.images[0].align, BoxAlign::Baseline);
        let b = &parse("<img src=\"a.png\">")[0];
        assert_eq!(b.kind, BlockKind::Image);
    }

    #[test]
    fn nesting_is_bounded() {
        let deep = "<blockquote>".repeat(10_000) + "x";
        let blocks = parse(&deep);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].indent <= 2000.0);
        let deep = "<b>".repeat(100_000) + "x" + &"</b>".repeat(100_000);
        assert_eq!(texts(&deep), ["x"]);
        let deep = "<table><tr><td>".repeat(1000) + "x";
        assert_eq!(texts(&deep).len(), 1);
    }

    /// The style of the first span of `source`.
    fn styled(source: &str) -> Inline {
        parse(source)[0].spans[0].style.clone()
    }

    /// The style a `style` attribute of a span gives its text.
    fn css(style: &str) -> Inline {
        styled(&format!("<span style=\"{style}\">abc</span>"))
    }

    #[test]
    fn a_style_gives_colours_as_they_were_written() {
        let s = css("color: green; background-color: LightGrey");
        assert_eq!(s.color.as_deref(), Some("green"));
        assert_eq!(s.background.as_deref(), Some("LightGrey"));
        assert_eq!(
            css("text-decoration-color: #0f0")
                .decoration_color
                .as_deref(),
            Some("#0f0")
        );
        // The shorthand is read when it is one colour and nothing else.
        assert_eq!(css("background: red").background.as_deref(), Some("red"));
        assert!(
            parse("<span style=\"background: red none\">abc</span>")[0]
                .spans
                .is_empty()
        );
        // Nothing a style names is resolved here: it stays a word that the
        // painter fails to read as a colour.
        let s = css("background-color: url(https://example.com/a.png)");
        assert_eq!(
            s.background.as_deref(),
            Some("url(https://example.com/a.png)")
        );
        assert!(
            parse("<span style=\"background: url(a.png)\">abc</span>")[0]
                .images
                .is_empty()
        );
    }

    #[test]
    fn a_style_is_read_whatever_its_case_and_quotes() {
        let s = styled("<span STYLE='BACKGROUND-COLOR: Blue; Text-Decoration: UNDERLINE'>a</span>");
        assert_eq!(s.background.as_deref(), Some("Blue"));
        assert!(s.underline);
        let s = css("color: red !important; /* color: blue; */ background-color: /* x */ green");
        assert_eq!(s.color.as_deref(), Some("red"));
        assert_eq!(s.background.as_deref(), Some("green"));
        // A comment that never ends takes the rest.
        let s = css("color: red; /* background-color: green");
        assert_eq!(s.color.as_deref(), Some("red"));
        assert_eq!(s.background, None);
    }

    #[test]
    fn the_lines_of_a_style_add_up_until_none() {
        let s = css("text-decoration: underline overline");
        assert!(s.underline && s.overline && !s.strike && !s.undecorated);
        let s = css("text-decoration: underline, line-through blink #f00");
        assert!(s.underline && s.strike && !s.overline);
        // A second declaration adds to the first.
        let s = css("text-decoration: underline; text-decoration: overline");
        assert!(s.underline && s.overline);
        let s = css("text-decoration: underline none");
        assert!(!s.underline && s.undecorated);
        let s = css("text-decoration: none underline");
        assert!(s.underline && s.undecorated);
        // The lines of the tag itself count as written lines.
        let s = styled("<u style=\"text-decoration: line-through\">a</u>");
        assert!(s.underline && s.strike);
        let s = styled("<s style=\"text-decoration: none\">a</s>");
        assert!(!s.strike && s.undecorated);
        // What is inside inherits, and may take the lines away again.
        let b = &parse(
            "<span style=\"text-decoration: overline\">a<span style=\"text-decoration: none\">b</span></span>",
        )[0];
        assert!(b.spans[0].style.overline && !b.spans[0].style.undecorated);
        assert!(!b.spans[1].style.overline && b.spans[1].style.undecorated);
    }

    #[test]
    fn a_declaration_that_cannot_be_read() {
        // The last of two counts, and an unknown property is passed over.
        let s = css("background-color: red;; bogus: 1; background-color: green");
        assert_eq!(s.background.as_deref(), Some("green"));
        // Malformed after the first: passed over.
        let s = css("background-color: blue; color red; background-color: red");
        assert_eq!(s.background.as_deref(), Some("red"));
        assert_eq!(s.color, None);
        let s = css("color: red; : blue; two words: 1; color:");
        assert_eq!(s.color.as_deref(), Some("red"));
        // Malformed first: the style says nothing (measured on Qt 6.11).
        for first in ["color red", ": red", "two words: red", "color:"] {
            let b = &parse(&format!(
                "<span style=\"{first}; background-color: red\">a</span>"
            ))[0];
            assert!(b.spans.is_empty(), "{first}");
        }
        assert!(parse("<span style=\"\">a</span>")[0].spans.is_empty());
        assert!(parse("<span style>a</span>")[0].spans.is_empty());
    }

    #[test]
    fn a_table_and_its_cells_have_backgrounds() {
        let blocks = parse(
            "<table bgcolor='red' width='300'><tr><td bgcolor='lime'>a</td><td>b</td>\
             <td bgcolor='lime' style='background-color: blue'>c</td></tr></table>",
        );
        let t = blocks[0].table.as_ref().expect("a table");
        assert_eq!(t.background.as_deref(), Some("red"));
        assert_eq!(t.width.as_deref(), Some("300"));
        let colours: Vec<_> = t.cells.iter().map(|c| c.background.as_deref()).collect();
        assert_eq!(colours, [Some("lime"), None, Some("blue")]);
    }

    #[test]
    fn a_background_is_of_text_and_not_of_a_block() {
        // A block's background fills the block, so it is the block's and
        // not its letters'. A table cell's is the cell's.
        for tag in ["p", "div", "li", "h1", "td"] {
            let source = match tag {
                "td" => "<table><tr><td style=\"background-color: red; color: blue\">a</td></tr></table>".to_owned(),
                "li" => "<ul><li style=\"background-color: red; color: blue\">a</li></ul>".to_owned(),
                // Text right in a `div` goes on past its end, in a block
                // that is not the div's.
                "div" => "<div style=\"background-color: red; color: blue\"><p>a</p></div>".to_owned(),
                _ => format!("<{tag} style=\"background-color: red; color: blue\">a</{tag}>"),
            };
            for block in parse(&source) {
                for span in &block.spans {
                    assert_eq!(span.style.background, None, "{tag}");
                }
                let expected = (tag != "td").then_some("red");
                assert_eq!(block.background.as_deref(), expected, "{tag}");
            }
        }
        let p = styled("<p style=\"color: blue\">a</p>");
        assert_eq!(p.color.as_deref(), Some("blue"));
    }

    #[test]
    fn a_style_is_of_the_tag_it_is_written_on() {
        // Measured on Qt 6.11: a tag nobody knows still carries its style.
        let foo = styled("<foo style=\"color: red\">abc</foo>");
        assert_eq!(foo.color.as_deref(), Some("red"));
        // A tag closed in itself and an image open nothing a style could be
        // the style of.
        assert!(parse("<span style=\"color: red\"/>abc")[0].spans.is_empty());
        let b = &parse("<b>a<img src=\"a.png\" style=\"color: red\">b</b>")[0];
        assert!(b.spans.iter().all(|s| s.style.color.is_none()));
        // It ends with its tag.
        let b = &parse("<span style=\"color: red\">a</span>b")[0];
        assert_eq!(b.spans.len(), 1);
        assert_eq!(b.spans[0].range, 0..1);
    }

    #[test]
    fn a_style_sheet_applies_before_the_style_attribute() {
        let sheet = "<style>a{color:#e0115f;text-decoration:none} .big{font-size:24px;font-weight:600;color:#2e8b57}</style>";
        let link = styled(&format!("{sheet}<a href=\"x\">b</a>"));
        assert_eq!(link.color.as_deref(), Some("#e0115f"));
        assert!(link.undecorated);
        let big = &parse(&format!("{sheet}<p class=\"big\">b</p>"))[0];
        assert_eq!(big.text, "b", "a style element is no text");
        let style = &big.spans[0].style;
        assert_eq!(style.color.as_deref(), Some("#2e8b57"));
        assert!(style.bold);
        let own = styled(&format!(
            "{sheet}<span class=\"big\" style=\"color:red\">b</span>"
        ));
        assert_eq!(own.color.as_deref(), Some("red"));
        // Only what comes after the sheet.
        let before = &parse(
            "<span class=\"c\">a</span><style>.c{color:red}</style><span class=\"c\">b</span>",
        )[0];
        assert_eq!(before.spans.len(), 1);
        assert_eq!(before.spans[0].range, 1..2);
    }

    #[test]
    fn a_link_starts_over_with_colour_and_underline() {
        let b = &parse(
            "<span style=\"color: green; text-decoration: none\">a<a href=\"x\">b</a></span>",
        )[0];
        assert_eq!(b.spans[0].style.color.as_deref(), Some("green"));
        let link = &b.spans[1].style;
        assert_eq!(link.link.as_deref(), Some("x"));
        assert_eq!(link.color, None, "a link is in the link colour");
        assert!(!link.undecorated, "and underlined");
        // Its own style counts.
        let link = styled("<a href=\"x\" style=\"color: red; text-decoration: none\">b</a>");
        assert_eq!(link.color.as_deref(), Some("red"));
        assert!(link.undecorated);
        // An anchor without an address is no link and changes nothing.
        let b = &parse("<span style=\"color: green\"><a name=\"x\">b</a></span>")[0];
        assert!(
            b.spans
                .iter()
                .all(|s| s.style.color.as_deref() == Some("green"))
        );
    }

    thread_local! {
        /// Bytes `attr_reference` has looked at, to show each `&` costs
        /// only its name.
        pub(super) static SCANNED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    #[test]
    fn an_attribute_reference_is_a_name_and_its_semicolon() {
        // Each verified on Qt 6.12 as a font-family's quoted name.
        assert_eq!(unescaped("&amp;"), "&");
        assert_eq!(unescaped("&amp"), "&amp");
        assert_eq!(unescaped("&#0065;&#x41;"), "AA");
        // Qt reads a number with a plus sign, as Rust's parse does.
        assert_eq!(unescaped("&#+65;&#x+41;"), "AA");
        assert_eq!(unescaped("a&b&c&amp;d"), "a&b&c&d");
        for literal in ["&B'x;", "&a mp;", "&#6 5;", "&#-65;", "&#65.0;", "&#65&"] {
            assert_eq!(unescaped(literal), literal);
        }
        assert_eq!(attr_reference("B'x;"), None);
        assert_eq!(attr_reference("amp;rest"), Some(('&', 4)));
    }

    #[test]
    fn each_ampersand_in_an_attribute_looks_only_at_its_name() {
        let value = "ab&".repeat(10_000) + "&amp;";
        SCANNED.with(|n| n.set(0));
        assert_eq!(unescaped(&value), "ab&".repeat(10_000) + "&");
        // A scan to the one `;` at the end would look at ~1.5e8 bytes.
        let scanned = SCANNED.with(std::cell::Cell::get);
        assert!(scanned <= 2 * value.len(), "{scanned} bytes looked at");
    }

    #[test]
    fn a_long_or_hostile_style_is_read_in_time() {
        let long = "color: red;".repeat(50_000) + &"/*".repeat(50_000);
        assert_eq!(css(&long).color.as_deref(), Some("red"));
        let deep = "<span style=\"text-decoration: overline\">".repeat(20_000) + "x";
        assert_eq!(texts(&deep), ["x"]);
        assert!(
            parse("<span style=\"color: \u{0}\u{202e}; : ; ;;; !important\">a</span>").len() == 1
        );
    }

    #[test]
    fn white_space_between_blocks_goes_to_the_block_before() {
        let qrich = "<meta name='qrichtext' content='1'>";
        let pre = |inner: &str| format!("<div style='white-space:pre'>{inner}</div>");
        for (source, expected) in [
            ("<p>a</p>  <p>b</p>".to_owned(), vec!["a ", "b"]),
            ("<p>a</p>\n  <p>b</p>".to_owned(), vec!["a ", "b"]),
            ("<p>a</p> <p>b</p>".to_owned(), vec!["a", "b"]),
            ("<p>a</p>\n".to_owned(), vec!["a "]),
            ("<p>a</p>\n</b>".to_owned(), vec!["a "]),
            ("<p>a</p><b></b>\n<p>b</p>".to_owned(), vec!["a ", "b"]),
            ("<p>a </p>  <p>b</p>".to_owned(), vec!["a ", "b"]),
            ("<p>a</p>  b".to_owned(), vec!["a", "b"]),
            ("<p>a</p> <!-- x --><p>b</p>".to_owned(), vec!["a", "b"]),
            ("<p>a<!-- -->  b</p>".to_owned(), vec!["ab"]),
            (
                "<p>a</p>&nbsp;&nbsp;<p>b</p>".to_owned(),
                vec!["a\u{a0}\u{a0}", "b"],
            ),
            ("<p>a</p>\u{b}\u{b}<p>b</p>".to_owned(), vec!["a ", "b"]),
            (pre("<p>a</p>  <p>b</p>"), vec!["a  ", "b"]),
            (pre("<p>a</p>\n\n\n<p>b</p>"), vec!["a", "", "", "", "b"]),
            (pre("  <p>a</p>"), vec!["  ", "a"]),
            (format!("{qrich}<p>a</p>\n  <p>b</p>"), vec!["a  ", "b"]),
            (format!("{qrich}<p>a</p>  </body>"), vec!["a  "]),
            (format!("{qrich}<div>  <p>a</p></div>"), vec!["  ", "a"]),
            (format!("{qrich}<ul>  <li>a</li></ul>"), vec!["a"]),
            (format!("{qrich}<p>a</p> \t <p>b</p>"), vec!["a \t ", "b"]),
            (format!("{qrich}<p>a</p>  x<p>b</p>"), vec!["a", "  x", "b"]),
            (
                format!("{qrich}<p>a<br>  </p>  <p>b</p>"),
                vec!["a\n    ", "b"],
            ),
            (
                "<div>\n  <p>a</p>\n  <p>b</p>\n</div>".to_owned(),
                vec!["a ", "b"],
            ),
            ("<p>a</p><b> </b><p>b</p>".to_owned(), vec!["a ", "b"]),
            ("<p>a</p> <b> </b> <p>b</p>".to_owned(), vec!["a ", "b"]),
            ("<p>a</p>&#10;&#10;<p>b</p>".to_owned(), vec!["a ", "b"]),
            ("<p>a</p>&#32;<p>b</p>".to_owned(), vec!["a", "b"]),
            ("<p>a</p> &amp;<p>b</p>".to_owned(), vec!["a", "&", "b"]),
            ("<p>a</p><hr>  <p>b</p>".to_owned(), vec!["a", "", "b"]),
        ] {
            assert_eq!(texts(&source), expected, "{source:?}");
        }
    }

    /// Measured on Qt 6.11 (hal-c2/ljos-ui#287).
    #[test]
    fn a_table_has_a_paragraph_after_it() {
        let t = "<table><tr><td>a</td></tr></table>";
        let after = |rest: &str| texts(&format!("{t}{rest}"));
        // A block after it starts after that paragraph, which stays empty.
        assert_eq!(after("<p>b</p>"), ["a", "", "b"]);
        assert_eq!(after("<ul><li>b</li></ul>"), ["a", "", "b"]);
        assert_eq!(after("<p>b</p><p>c</p>"), ["a", "", "b", "c"]);
        assert_eq!(texts(&format!("<div>{t}</div><p>b</p>")), ["a", "", "b"]);
        // Text goes into it.
        assert_eq!(after("b"), ["a", "b"]);
        assert_eq!(after("  b<p>c</p>"), ["a", "b", "c"]);
        // An empty block right after it is nothing, unless the document
        // ends there.
        assert_eq!(after("<p></p><p></p><h1></h1><p>b</p>"), ["a", "", "b"]);
        assert_eq!(after("<p></p>"), ["a", "", ""]);
        assert_eq!(after(""), ["a"]);
        // An empty paragraph an editor wrote is one, as a rule is.
        assert_eq!(
            after("<p style='-qt-paragraph-type:empty'><br /></p><p>b</p>"),
            ["a", "", "", "b"]
        );
        let rule = parse(&format!("{t}<hr><p>b</p>"));
        let kinds: Vec<_> = rule.iter().map(|b| b.kind).collect();
        assert_eq!(kinds, [BlockKind::Table, Paragraph, Rule, Paragraph]);
        // It has none of the format of the block after it.
        let shapes = shape(&format!("{t}<p>b</p>"));
        assert_eq!(
            shapes[1],
            (Paragraph, String::new(), (0.0, 0.0), (0.0, 0.0))
        );
        assert_eq!(shapes[2].2, (12.0, 12.0));
    }

    #[test]
    fn a_cell_or_a_row_with_no_table_opens_one() {
        // Measured on Qt 6.12 (features/text/rich-text-cells-without-a-table.feature).
        type Read = Vec<(BlockKind, String, Option<(usize, Vec<crate::rich::Cell>)>)>;
        let read = |source: &str| -> Read {
            parse(source)
                .into_iter()
                .map(|b| (b.kind, b.text, b.table.map(|t| (t.columns, t.cells))))
                .collect()
        };
        let t = "<table>";
        for (stray, table) in [
            ("<p>a</p><td>b</td>", format!("<p>a</p>{t}<td>b</td>")),
            ("<td>b</td>", format!("{t}<td>b</td>")),
            ("<p>a<td>b</td></p>", format!("<p>a</p>{t}<td>b</td>")),
            ("<p>a</p><td>b", format!("<p>a</p>{t}<td>b")),
            (
                "<td>b</td><td>c</td>",
                format!("{t}<tr><td>b</td><td>c</td></tr>"),
            ),
            (
                "<tr><td>b</td></tr><tr><td>c</td></tr>",
                format!("{t}<tr><td>b</td></tr><tr><td>c</td></tr>"),
            ),
            (
                "<td>b</td><tr><td>c</td></tr>",
                format!("{t}<tr><td>b</td></tr><tr><td>c</td></tr>"),
            ),
            ("<p>a</p><tr>b</tr>", "<p>a</p>".to_owned()),
            ("<td>b</td></table>c", format!("{t}<td>b</td></table>c")),
            ("a<td>b</td>c", format!("a{t}<td>b</td>c</table>")),
            ("<th>b</th><td>c</td>", format!("{t}<th>b</th><td>c</td>")),
        ] {
            assert_eq!(read(stray), read(&table), "{stray:?}");
        }
        let blocks = parse("<p>a</p><td>b</td><td>c</td>");
        assert_eq!(blocks[1].kind, BlockKind::Table);
        assert_eq!(blocks[1].table.as_ref().map(|t| t.columns), Some(2));
        let header = parse("<th>b</th>");
        assert!(header[0].table.as_ref().is_some_and(|t| t.cells[0].header));
    }

    #[test]
    fn what_comes_after_a_cell_is_a_paragraph_of_it() {
        // Measured on Qt 6.12: nothing between cells is lost, but before a
        // row's first cell it is.
        let cells = |source: &str| -> Vec<(String, Vec<usize>)> {
            parse(source)
                .iter()
                .filter_map(|b| Some((b, b.table.as_ref()?)))
                .flat_map(|(b, t)| {
                    t.cells
                        .iter()
                        .map(|c| (b.text[c.range.clone()].to_owned(), c.paragraphs.clone()))
                })
                .collect()
        };
        let texts = |source: &str| -> Vec<String> {
            cells(source).into_iter().map(|(text, _)| text).collect()
        };
        assert_eq!(texts("<table><tr><td>b</td></tr>c</table>"), ["b\nc"]);
        assert_eq!(texts("<table><tr><td>b</td>  c</tr></table>"), ["b\nc"]);
        assert_eq!(
            texts("<table><tr><td>b</td><p>c</p>d</tr></table>"),
            ["b\nc\nd"]
        );
        assert_eq!(
            texts("<table><tr><td>b</td><b>c</b>d<td>e</td></tr></table>"),
            ["b\ncd", "e"]
        );
        assert_eq!(texts("<table><tr><td></td>c</tr></table>"), ["c"]);
        assert_eq!(
            texts("<table><tr><td>b</td> <span> </span> <td>e</td></tr></table>"),
            ["b", "e"]
        );
        assert_eq!(texts("<table>c<tr><td>b</td></tr></table>"), ["b"]);
        assert_eq!(
            texts("<table><tr><td>b</td></tr><tr>c<td>d</td></tr></table>"),
            ["b", "d"]
        );
        assert_eq!(texts("a<td>b</td>c"), ["b\nc"]);
        assert_eq!(texts("<p>a</p><td>b</td><p>c</p>"), ["b\nc"]);
        // Each one a paragraph of the cell, not a line of one.
        let (_, paragraphs) = &cells("<table><tr><td>b</td><p>c</p>d</tr></table>")[0];
        assert_eq!(paragraphs.len(), 2, "{paragraphs:?}");
    }

    #[test]
    fn a_table_takes_no_white_space_between_its_cells_or_after_it() {
        let table = |source: &str| -> (Vec<String>, Vec<String>) {
            let blocks = parse(source);
            let cells = blocks
                .iter()
                .filter_map(|b| Some((b, b.table.as_ref()?)))
                .flat_map(|(b, t)| t.cells.iter().map(|c| b.text[c.range.clone()].to_owned()))
                .collect();
            (blocks.into_iter().map(|b| b.text).collect(), cells)
        };
        let qrich = "<meta name='qrichtext' content='1'>";
        let (_, cells) = table("<table><tr><td>a</td>  <td>b</td></tr>  </table>");
        assert_eq!(cells, ["a", "b"]);
        let (blocks, _) = table("<table><tr><td>x</td></tr></table>  <p>b</p>");
        assert_eq!(blocks.last().map(String::as_str), Some("b"));
        let (_, cells) = table(&format!(
            "{qrich}<table><tr><td>a</td>  <td>b</td></tr></table>"
        ));
        assert_eq!(cells, ["a  ", "b"]);
        let (_, cells) = table(&format!(
            "{qrich}<table><tr><td>a</td>  </tr>  <tr>  <td>b</td></tr></table>"
        ));
        assert_eq!(cells, ["a    ", "b"]);
    }
}
