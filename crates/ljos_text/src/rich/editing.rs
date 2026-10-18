//! A rich text being edited: characters, each with its format, in
//! paragraphs that have formats of their own.
//!
//! [`RichText`] holds the text as an editor's model does ('\n' between
//! paragraphs), so the two change together: [`RichText::splice`] is told
//! what the model did and gives the new characters a format the way Qt
//! 6.11 does (measured with qml6):
//!
//! * typed text takes the format of the character before it, or at the
//!   start of a paragraph of the one after it;
//! * text typed over a selection takes the format of the first character
//!   it replaces;
//! * a paragraph split in two leaves both halves the paragraph's format;
//!   two paragraphs joined keep the first one's.
//!
//! A line break inside a paragraph (`<br>`) is [`LINE_BREAK`] and an image
//! is [`OBJECT`], each one character, as in Qt.
//!
//! The text is written back out the way Qt writes it: [`RichText::html`]
//! as Qt's HTML document, [`RichText::markdown`] as its Markdown. Tables
//! are read as their cells' text, a paragraph per row with its cells apart
//! by tabs, with the [`TableFormat`] an editor lays them out by, and are not
//! written back as tables.

use std::fmt::Write as _;
use std::ops::Range;
use std::sync::Arc;

use super::doc::HAlign;
use super::html::scale_keyword;
use super::{Block, BlockKind, Inline, ParseOptions, TextFormat, parse};
use crate::style::{BoxAlign, InlineObject, StyleSpan, TextStyle};

/// Ends a paragraph.
pub const PARAGRAPH: char = '\n';
/// Breaks a line inside a paragraph.
pub const LINE_BREAK: char = '\u{2028}';
/// Stands for an image.
pub const OBJECT: char = '\u{fffc}';

/// How a character is set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharFormat {
    pub inline: Inline,
    /// A whole font was given to the text (a toolbar's `font`): Qt keeps
    /// that apart from an equal format the document gave, and writes the
    /// two as two spans.
    pub whole_font: bool,
    /// The character is an image.
    pub image: Option<Arc<ImageFormat>>,
}

impl CharFormat {
    /// The format text typed next to this character gets: not an image.
    fn for_typing(self: &Arc<Self>) -> Arc<Self> {
        if self.image.is_none() {
            return Arc::clone(self);
        }
        Arc::new(CharFormat {
            image: None,
            ..(**self).clone()
        })
    }
}

/// An image in the text.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageFormat {
    /// As the document wrote it.
    pub source: Arc<str>,
    pub alt: Arc<str>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub align: BoxAlign,
}

/// How a paragraph is set.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockFormat {
    pub kind: BlockKind,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub indent: f32,
    pub indent_right: f32,
    /// The bullet or number of a list item.
    pub marker: Option<Arc<str>>,
    /// The colour behind the paragraph, as written.
    pub background: Option<Arc<str>>,
    pub align: Option<HAlign>,
    /// The paragraph is a row of a table: its cells are set apart by tabs.
    /// The rows of one table share their format.
    pub table: Option<TableFormat>,
}

/// How the cells of a table stand apart (see [`super::Table`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TableFormat {
    pub border: f32,
    pub spacing: f32,
    pub padding: f32,
}

impl Default for BlockFormat {
    fn default() -> Self {
        Self {
            kind: BlockKind::Paragraph,
            margin_top: 0.0,
            margin_bottom: 0.0,
            indent: 0.0,
            indent_right: 0.0,
            marker: None,
            background: None,
            align: None,
            table: None,
        }
    }
}

impl BlockFormat {
    fn of(block: &Block) -> Self {
        Self {
            kind: block.kind,
            margin_top: block.margin_top,
            margin_bottom: block.margin_bottom,
            indent: block.indent,
            indent_right: block.indent_right,
            marker: block.marker.clone(),
            background: block.background.clone(),
            align: block.align,
            table: block.table.as_ref().map(|t| TableFormat {
                border: t.border,
                spacing: t.spacing,
                padding: t.padding,
            }),
        }
    }

    /// A numbered list item (its marker ends in a full stop).
    fn ordered(&self) -> bool {
        self.marker.as_deref().is_some_and(|m| m.ends_with('.'))
    }
}

/// The font a document is set in, which the HTML of it names.
#[derive(Clone, Debug, PartialEq)]
pub struct DocFont {
    pub family: String,
    pub pixel_size: f32,
    pub weight: f32,
    pub italic: bool,
}

impl Default for DocFont {
    fn default() -> Self {
        Self {
            family: String::new(),
            pixel_size: 14.0,
            weight: 400.0,
            italic: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Run {
    len: usize,
    format: Arc<CharFormat>,
}

/// A rich text being edited (see the module's documentation).
#[derive(Clone, Debug, PartialEq)]
pub struct RichText {
    text: String,
    /// Cover the text, '\n' included: the format of a paragraph's end is
    /// what an empty paragraph is typed in.
    runs: Vec<Run>,
    /// One per paragraph.
    blocks: Vec<Arc<BlockFormat>>,
}

impl Default for RichText {
    fn default() -> Self {
        Self {
            text: String::new(),
            runs: Vec::new(),
            blocks: vec![Arc::default()],
        }
    }
}

impl RichText {
    /// Plain text: every character in the default format.
    pub fn plain(text: &str) -> Self {
        let mut out = Self::default();
        out.splice(0..0, text, Some(Arc::default()));
        out
    }

    /// Read `source`. `resolve` writes a colour the document gave the way
    /// the text keeps it (`#rrggbb`), or says it is no colour.
    pub fn parse(
        format: TextFormat,
        source: &str,
        options: &ParseOptions,
        resolve: &dyn Fn(&str) -> Option<Arc<str>>,
    ) -> Self {
        let parsed = parse(format, source, options);
        let mut out = Self::from_blocks(&parsed.blocks, resolve);
        // Measured: white space after the last paragraph of HTML is one
        // space at the end of it; between paragraphs it is nothing.
        if format == TextFormat::Rich && trailing_space(source) {
            let format = out
                .runs
                .last()
                .map_or_else(Arc::default, |r| r.format.for_typing());
            let end = out.text.len();
            out.splice(end..end, " ", Some(format));
        }
        out
    }

    fn from_blocks(blocks: &[Block], resolve: &dyn Fn(&str) -> Option<Arc<str>>) -> Self {
        let mut out = Self::default();
        let mut formats: Vec<(Inline, Arc<CharFormat>)> = Vec::new();
        let mut format_of = |inline: &Inline| -> Arc<CharFormat> {
            if let Some((_, f)) = formats.iter().find(|(i, _)| i == inline) {
                return Arc::clone(f);
            }
            let f = Arc::new(CharFormat {
                inline: resolved(inline, resolve),
                ..CharFormat::default()
            });
            formats.push((inline.clone(), Arc::clone(&f)));
            f
        };
        let plain = Arc::new(CharFormat::default());
        for (bi, block) in blocks.iter().enumerate() {
            let mut bf = BlockFormat::of(block);
            bf.background = bf.background.as_deref().and_then(resolve);
            let bf = Arc::new(bf);
            if bi == 0 {
                out.blocks[0] = Arc::clone(&bf);
            } else {
                out.push(&PARAGRAPH.to_string(), &plain);
                out.blocks.push(Arc::clone(&bf));
            }
            let breaks_paragraph = matches!(block.kind, BlockKind::Code | BlockKind::Table);
            let text = &block.text;
            let mut images = block.images.iter().peekable();
            let mut i = 0;
            loop {
                while let Some(image) = images.next_if(|im| im.index <= i) {
                    let inline = block
                        .span_at(i)
                        .map(|s| s.style.clone())
                        .unwrap_or_default();
                    let base = format_of(&inline);
                    let f = Arc::new(CharFormat {
                        image: Some(Arc::new(ImageFormat {
                            source: Arc::clone(&image.source),
                            alt: Arc::clone(&image.alt),
                            width: image.width,
                            height: image.height,
                            align: image.align,
                        })),
                        ..(*base).clone()
                    });
                    out.push(&OBJECT.to_string(), &f);
                }
                if i >= text.len() {
                    break;
                }
                let span = block.span_at(i);
                let next_span = block
                    .spans
                    .iter()
                    .map(|s| s.range.start)
                    .find(|start| *start > i)
                    .unwrap_or(text.len());
                let mut end = span.map_or(next_span, |s| s.range.end).min(text.len());
                if let Some(image) = images.peek() {
                    end = end.min(image.index.max(i + 1));
                }
                while !text.is_char_boundary(end) {
                    end += 1;
                }
                let f = span.map_or_else(|| Arc::clone(&plain), |s| format_of(&s.style));
                for (k, piece) in text[i..end].split('\n').enumerate() {
                    if k > 0 {
                        if breaks_paragraph {
                            out.push(&PARAGRAPH.to_string(), &f);
                            out.blocks.push(Arc::clone(&bf));
                        } else {
                            out.push(&LINE_BREAK.to_string(), &f);
                        }
                    }
                    out.push(piece, &f);
                }
                i = end;
            }
        }
        out
    }

    /// Append text at the end, in `format`, without looking at paragraphs.
    fn push(&mut self, s: &str, format: &Arc<CharFormat>) {
        if s.is_empty() {
            return;
        }
        self.text.push_str(s);
        match self.runs.last_mut() {
            Some(last) if same(&last.format, format) => last.len += s.len(),
            _ => self.runs.push(Run {
                len: s.len(),
                format: Arc::clone(format),
            }),
        }
    }

    /// The characters, '\n' between paragraphs.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The paragraphs' formats, one per paragraph.
    pub fn blocks(&self) -> &[Arc<BlockFormat>] {
        &self.blocks
    }

    /// The paragraph `byte` is in.
    pub fn block_index(&self, byte: usize) -> usize {
        let byte = byte.min(self.text.len());
        self.text.as_bytes()[..byte]
            .iter()
            .filter(|b| **b == b'\n')
            .count()
    }

    /// The bytes of paragraph `index`, its '\n' not included.
    pub fn block_range(&self, index: usize) -> Range<usize> {
        let mut start = 0;
        for (i, line) in self.text.split('\n').enumerate() {
            if i == index {
                return start..start + line.len();
            }
            start += line.len() + 1;
        }
        self.text.len()..self.text.len()
    }

    /// The format of the character at `byte`.
    pub fn format_at(&self, byte: usize) -> Arc<CharFormat> {
        let mut at = 0;
        for run in &self.runs {
            if byte < at + run.len {
                return Arc::clone(&run.format);
            }
            at += run.len;
        }
        self.runs
            .last()
            .map_or_else(Arc::default, |r| Arc::clone(&r.format))
    }

    /// The format text typed at `byte` gets: that of the character before
    /// it, at the start of a paragraph that of the one after it.
    pub fn typing_format(&self, byte: usize) -> Arc<CharFormat> {
        let byte = byte.min(self.text.len());
        let before = self.text[..byte].chars().next_back();
        let after = self.text[byte..].chars().next();
        let f = match (before, after) {
            (Some(c), _) if c != PARAGRAPH => self.format_at(byte - c.len_utf8()),
            (_, Some(c)) if c != PARAGRAPH => self.format_at(byte),
            (Some(c), _) => self.format_at(byte - c.len_utf8()),
            _ => Arc::default(),
        };
        f.for_typing()
    }

    /// The format a cursor at `byte` reads as (Qt's `cursorSelection`
    /// without a selection): that text would be typed in.
    pub fn cursor_format(&self, byte: usize) -> Arc<CharFormat> {
        self.typing_format(byte)
    }

    /// The link the character at `byte` belongs to.
    pub fn link_at(&self, byte: usize) -> Option<Arc<str>> {
        if byte >= self.text.len() {
            return None;
        }
        self.format_at(byte).inline.link.clone()
    }

    /// Replace `range` with `text`. Without a format the new characters
    /// take one as typed text does.
    pub fn splice(&mut self, range: Range<usize>, text: &str, format: Option<Arc<CharFormat>>) {
        let range = self.clamp(range);
        let format = format.unwrap_or_else(|| {
            if range.is_empty() {
                self.typing_format(range.start)
            } else {
                self.format_at(range.start).for_typing()
            }
        });
        let other = Self {
            text: text.to_owned(),
            runs: if text.is_empty() {
                Vec::new()
            } else {
                vec![Run {
                    len: text.len(),
                    format,
                }]
            },
            blocks: Vec::new(),
        };
        self.splice_rich(range, &other);
    }

    /// Replace `range` with rich text. Its first paragraph goes on in the
    /// paragraph `range` starts in; its other paragraphs keep their formats
    /// (a fragment without them, as [`Self::splice`] makes, gives each new
    /// paragraph the format of the one it was split from).
    pub fn splice_rich(&mut self, range: Range<usize>, other: &RichText) {
        let range = self.clamp(range);
        let b = self.block_index(range.start);
        let removed = self.text[range.clone()].matches('\n').count();
        self.blocks.drain(b + 1..=b + removed);
        let added = other.text.matches('\n').count();
        for k in 0..added {
            let f = other
                .blocks
                .get(k + 1)
                .map_or_else(|| Arc::clone(&self.blocks[b]), Arc::clone);
            self.blocks.insert(b + 1 + k, f);
        }
        let i = self.split(range.start);
        let j = self.split(range.end);
        self.runs.splice(i..j, other.runs.iter().cloned());
        self.text.replace_range(range, &other.text);
        self.normalize();
    }

    /// Change the format of the characters in `range`.
    pub fn set_char_format(&mut self, range: Range<usize>, f: impl Fn(&CharFormat) -> CharFormat) {
        let range = self.clamp(range);
        if range.is_empty() {
            return;
        }
        let i = self.split(range.start);
        let j = self.split(range.end);
        for run in &mut self.runs[i..j] {
            run.format = Arc::new(f(&run.format));
        }
        self.normalize();
    }

    /// Change the format of the paragraphs `range` touches.
    pub fn set_block_format(
        &mut self,
        range: Range<usize>,
        f: impl Fn(&BlockFormat) -> BlockFormat,
    ) {
        let range = self.clamp(range);
        let (a, b) = (self.block_index(range.start), self.block_index(range.end));
        for block in &mut self.blocks[a..=b] {
            *block = Arc::new(f(block));
        }
    }

    /// The part of the text in `range`, with its formats.
    pub fn slice(&self, range: Range<usize>) -> RichText {
        let range = self.clamp(range);
        let mut out = self.clone();
        let end = out.text.len();
        out.splice_rich(range.end..end, &RichText::empty_fragment());
        out.splice_rich(0..range.start, &RichText::empty_fragment());
        // The first paragraph is the one the range starts in.
        out.blocks[0] = Arc::clone(&self.blocks[self.block_index(range.start)]);
        out
    }

    fn empty_fragment() -> RichText {
        Self {
            text: String::new(),
            runs: Vec::new(),
            blocks: Vec::new(),
        }
    }

    fn clamp(&self, range: Range<usize>) -> Range<usize> {
        let floor = |mut i: usize| {
            i = i.min(self.text.len());
            while !self.text.is_char_boundary(i) {
                i -= 1;
            }
            i
        };
        let (a, b) = (floor(range.start), floor(range.end));
        a.min(b)..a.max(b)
    }

    /// Make a run start at `byte`; the index of that run.
    fn split(&mut self, byte: usize) -> usize {
        let mut at = 0;
        for i in 0..self.runs.len() {
            if at == byte {
                return i;
            }
            let len = self.runs[i].len;
            if byte < at + len {
                let format = Arc::clone(&self.runs[i].format);
                self.runs[i].len = byte - at;
                self.runs.insert(
                    i + 1,
                    Run {
                        len: at + len - byte,
                        format,
                    },
                );
                return i + 1;
            }
            at += len;
        }
        self.runs.len()
    }

    fn normalize(&mut self) {
        let mut out: Vec<Run> = Vec::with_capacity(self.runs.len());
        for run in self.runs.drain(..) {
            if run.len == 0 {
                continue;
            }
            match out.last_mut() {
                Some(last) if same(&last.format, &run.format) => last.len += run.len,
                _ => out.push(run),
            }
        }
        self.runs = out;
    }

    /// The runs with where they are: `(bytes, format)`.
    pub fn runs(&self) -> impl Iterator<Item = (Range<usize>, &Arc<CharFormat>)> {
        let mut at = 0;
        self.runs.iter().map(move |r| {
            let range = at..at + r.len;
            at += r.len;
            (range, &r.format)
        })
    }

    /// What the layout needs: a span for every run that is not set in the
    /// text's own style, `tag` the run's index plus one. `shift` maps a
    /// byte of the text to one of the string laid out.
    pub fn style_spans(&self, base: &TextStyle, shift: impl Fn(usize) -> usize) -> Vec<StyleSpan> {
        self.runs()
            .enumerate()
            .filter(|(_, (_, f))| f.inline != Inline::default())
            .filter_map(|(i, (range, f))| {
                let tag = u32::try_from(i + 1).ok()?;
                let range = shift(range.start)..shift(range.end);
                Some(f.inline.style_span(base, range, tag))
            })
            .collect()
    }

    /// The images, as boxes of the size `size` gives each (the size the
    /// document asked for, or the picture's), standing where `shift` maps
    /// the character after the image: the image is between the two
    /// positions around its character.
    pub fn objects(
        &self,
        shift: impl Fn(usize) -> usize,
        size: impl Fn(&ImageFormat) -> (f32, f32),
    ) -> Vec<InlineObject> {
        let mut out = Vec::new();
        for (range, f) in self.runs() {
            let Some(image) = &f.image else { continue };
            let (width, height) = size(image);
            for (i, c) in self.text[range.clone()].char_indices() {
                if c != OBJECT {
                    continue;
                }
                let at = range.start + i;
                out.push(InlineObject {
                    id: at as u64,
                    index: shift(at + c.len_utf8()),
                    width,
                    height,
                    align: image.align,
                });
            }
        }
        out
    }

    /// The text as it is laid out: an image's character takes no room of
    /// its own (its box does) and keeps its length.
    pub fn shown(text: &str) -> std::borrow::Cow<'_, str> {
        if text.contains(OBJECT) {
            text.replace(OBJECT, "\u{200b}").into()
        } else {
            text.into()
        }
    }

    /// Qt's HTML document of the text, set in `font`.
    pub fn html(&self, font: &DocFont) -> String {
        let mut out = String::new();
        write_head(&mut out);
        let _ = writeln!(
            out,
            "<body style=\" font-family:'{}'; font-size:{}px; font-weight:{}; font-style:{};\">",
            escape_attr(&font.family),
            num(font.pixel_size),
            num(font.weight),
            if font.italic { "italic" } else { "normal" },
        );
        self.write_body(&mut out, false);
        out.push_str("</body></html>");
        out
    }

    /// Qt's HTML of a part of the text (`getFormattedText`): a document
    /// with the part marked as a fragment.
    pub fn html_fragment(&self, range: Range<usize>) -> String {
        let part = self.slice(range);
        let mut out = String::new();
        write_head(&mut out);
        out.push_str("<body>\n");
        part.write_body(&mut out, true);
        out.push_str("</body></html>");
        out
    }

    fn write_body(&self, out: &mut String, fragment: bool) {
        let count = self.blocks.len();
        let mut list: Option<bool> = None;
        for (index, block) in self.blocks.iter().enumerate() {
            let range = self.block_range(index);
            let in_list = block.kind == BlockKind::List;
            let ordered = block.ordered();
            if let Some(was) = list
                && (!in_list || was != ordered)
            {
                out.push_str(if was { "</ol>" } else { "</ul>" });
                list = None;
            }
            if index > 0 && !(in_list && list.is_some()) {
                out.push('\n');
            }
            if in_list && list.is_none() {
                let _ = write!(
                    out,
                    "<{} style=\"margin-top: 0px; margin-bottom: 0px; margin-left: 0px; \
                     margin-right: 0px; -qt-list-indent: 1;\">",
                    if ordered { "ol" } else { "ul" }
                );
                list = Some(ordered);
            }
            if block.kind == BlockKind::Rule {
                out.push_str("<hr />");
                continue;
            }
            let tag = match block.kind {
                BlockKind::Heading(n) => format!("h{}", n.clamp(1, 6)),
                BlockKind::Code => "pre".to_owned(),
                BlockKind::List => "li".to_owned(),
                _ => "p".to_owned(),
            };
            let empty = range.is_empty();
            out.push('<');
            out.push_str(&tag);
            if let Some(align) = block.align.and_then(align_name) {
                let _ = write!(out, " align=\"{align}\"");
            }
            let _ = write!(
                out,
                " style=\"{}margin-top:{}px; margin-bottom:{}px; margin-left:{}px; \
                 margin-right:{}px; -qt-block-indent:0; text-indent:0px;{}\">",
                if empty {
                    "-qt-paragraph-type:empty; "
                } else {
                    " "
                },
                num(block.margin_top),
                num(block.margin_bottom),
                num(if in_list { 0.0 } else { block.indent }),
                num(block.indent_right),
                block
                    .background
                    .as_deref()
                    .map(|c| format!(" background-color:{c};"))
                    .unwrap_or_default(),
            );
            if fragment && index == 0 {
                out.push_str("<!--StartFragment-->");
            }
            if empty {
                out.push_str("<br />");
            } else {
                self.write_inline(out, range);
            }
            if fragment && index + 1 == count {
                out.push_str("<!--EndFragment-->");
            }
            let _ = write!(out, "</{tag}>");
        }
        if let Some(was) = list {
            out.push_str(if was { "</ol>" } else { "</ul>" });
        }
    }

    fn write_inline(&self, out: &mut String, range: Range<usize>) {
        for (run, f) in self.runs() {
            let (a, b) = (run.start.max(range.start), run.end.min(range.end));
            if a >= b {
                continue;
            }
            let text = &self.text[a..b];
            if let Some(link) = &f.inline.link {
                let _ = write!(out, "<a href=\"{}\">", escape_attr(link));
            }
            let style = span_style(f);
            if !style.is_empty() {
                let _ = write!(out, "<span style=\"{style}\">");
            }
            for c in text.chars() {
                match c {
                    LINE_BREAK => out.push_str("<br />"),
                    OBJECT => {
                        if let Some(image) = &f.image {
                            let _ = write!(out, "<img src=\"{}\"", escape_attr(&image.source));
                            if let Some(w) = image.width {
                                let _ = write!(out, " width=\"{}\"", num(w));
                            }
                            if let Some(h) = image.height {
                                let _ = write!(out, " height=\"{}\"", num(h));
                            }
                            out.push_str(" />");
                        }
                    }
                    '<' => out.push_str("&lt;"),
                    '>' => out.push_str("&gt;"),
                    '&' => out.push_str("&amp;"),
                    '"' => out.push_str("&quot;"),
                    c => out.push(c),
                }
            }
            if !style.is_empty() {
                out.push_str("</span>");
            }
            if f.inline.link.is_some() {
                out.push_str("</a>");
            }
        }
    }

    /// Qt's Markdown of the text: a paragraph ends in an empty line, the
    /// items of a list follow each other.
    pub fn markdown(&self) -> String {
        let mut out = String::new();
        let count = self.blocks.len();
        let mut number = 0;
        for (index, block) in self.blocks.iter().enumerate() {
            let range = self.block_range(index);
            let next_in_list = self
                .blocks
                .get(index + 1)
                .is_some_and(|b| b.kind == BlockKind::List);
            match block.kind {
                BlockKind::List => {
                    if block.ordered() {
                        number += 1;
                        let _ = write!(out, "{number}.  ");
                    } else {
                        out.push_str("- ");
                    }
                    self.write_markdown_inline(&mut out, range);
                    out.push('\n');
                    if !next_in_list {
                        number = 0;
                        if index + 1 < count {
                            out.push('\n');
                        }
                    }
                    continue;
                }
                BlockKind::Heading(n) => {
                    out.push_str(&"#".repeat(usize::from(n.clamp(1, 6))));
                    out.push(' ');
                    // A heading's own weight is the heading's, not marks.
                    self.write_markdown_inline_as(&mut out, range, true);
                }
                BlockKind::Quote => {
                    out.push_str("> ");
                    self.write_markdown_inline(&mut out, range);
                }
                BlockKind::Code => {
                    out.push_str("```\n");
                    out.push_str(&self.text[range]);
                    out.push_str("\n```");
                }
                BlockKind::Rule => out.push_str("- - -"),
                _ => self.write_markdown_inline(&mut out, range),
            }
            out.push_str("\n\n");
        }
        out
    }

    /// Qt's Markdown of a part of the text (`getFormattedText`).
    pub fn markdown_fragment(&self, range: Range<usize>) -> String {
        self.slice(range).markdown()
    }

    fn write_markdown_inline(&self, out: &mut String, range: Range<usize>) {
        self.write_markdown_inline_as(out, range, false);
    }

    fn write_markdown_inline_as(&self, out: &mut String, range: Range<usize>, heading: bool) {
        for (run, f) in self.runs() {
            let (a, b) = (run.start.max(range.start), run.end.min(range.end));
            if a >= b {
                continue;
            }
            let i = &f.inline;
            let mut marks = String::new();
            if i.bold && !heading {
                marks.push_str("**");
            }
            if i.italic {
                marks.push('*');
            }
            if i.strike {
                marks.push_str("~~");
            }
            if i.mono {
                marks.push('`');
            }
            if i.link.is_some() {
                out.push('[');
            }
            out.push_str(&marks);
            for c in self.text[a..b].chars() {
                match c {
                    LINE_BREAK => out.push_str("  \n"),
                    OBJECT => {
                        if let Some(image) = &f.image {
                            let _ = write!(out, "![{}]({})", image.alt, image.source);
                        }
                    }
                    c => out.push(c),
                }
            }
            out.extend(marks.chars().rev());
            if let Some(link) = &i.link {
                let _ = write!(out, "]({link})");
            }
        }
    }

    /// The text as `getText` and `selectedText` give it: paragraphs apart
    /// by U+2029.
    pub fn plain_of(text: &str) -> String {
        text.replace(PARAGRAPH, "\u{2029}")
    }

    /// This text as new paragraphs after the text it is spliced in at
    /// (`append`): a paragraph break, then the text with its paragraphs'
    /// formats.
    pub fn as_new_paragraphs(&self) -> RichText {
        let mut out = Self::empty_fragment();
        out.blocks.push(Arc::default());
        out.push(&PARAGRAPH.to_string(), &Arc::default());
        out.blocks.extend(self.blocks.iter().cloned());
        out.text.push_str(&self.text);
        out.runs.extend(self.runs.iter().cloned());
        out.normalize();
        out
    }
}

impl ImageFormat {
    /// The size of an image whose picture cannot be shown: what the
    /// document gave, a small square for the rest.
    pub fn placeholder_size(&self) -> (f32, f32) {
        (
            self.width.unwrap_or(super::doc::PLACEHOLDER),
            self.height.unwrap_or(super::doc::PLACEHOLDER),
        )
    }
}

/// Two formats that write the same and are kept as one.
fn same(a: &Arc<CharFormat>, b: &Arc<CharFormat>) -> bool {
    Arc::ptr_eq(a, b) || **a == **b
}

fn resolved(inline: &Inline, resolve: &dyn Fn(&str) -> Option<Arc<str>>) -> Inline {
    let colour = |c: &Option<Arc<str>>| c.as_deref().and_then(resolve);
    let mut out = Inline {
        color: colour(&inline.color),
        background: colour(&inline.background),
        decoration_color: colour(&inline.decoration_color),
        ..inline.clone()
    };
    // Measured: a link in a TextEdit is written blue and underlined, as Qt
    // gives it its look when it reads it.
    if out.link.is_some() {
        if out.color.is_none() {
            out.color = Some(Arc::from("#0000ff"));
        }
        if !out.undecorated {
            out.underline = true;
        }
    }
    out
}

/// Whether HTML ends in white space after the end tag of a block.
fn trailing_space(source: &str) -> bool {
    let trimmed = source.trim_end();
    if trimmed.len() == source.len() || !trimmed.ends_with('>') {
        return false;
    }
    let Some(lt) = trimmed.rfind('<') else {
        return false;
    };
    let tag = &trimmed[lt + 1..trimmed.len() - 1];
    let Some(name) = tag.strip_prefix('/') else {
        return false;
    };
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "p" | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "ul"
            | "ol"
            | "pre"
            | "blockquote"
            | "table"
    )
}

fn write_head(out: &mut String) {
    out.push_str(
        "<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.0//EN\" \
         \"http://www.w3.org/TR/REC-html40/strict.dtd\">\n\
         <html><head><meta name=\"qrichtext\" content=\"1\" /><meta charset=\"utf-8\" />\
         <style type=\"text/css\">\n\
         p, li { white-space: pre-wrap; }\n\
         hr { height: 1px; border-width: 0; }\n\
         li.unchecked::marker { content: \"\\2610\"; }\n\
         li.checked::marker { content: \"\\2612\"; }\n\
         </style></head>",
    );
}

/// The style of a span, as Qt writes it: each property after a space.
fn span_style(f: &CharFormat) -> String {
    let i = &f.inline;
    let mut s = String::new();
    if let Some(family) = &i.family {
        let _ = write!(s, " font-family:'{}';", escape_attr(family));
    } else if i.mono {
        s.push_str(" font-family:'monospace';");
    }
    if let Some(px) = i.pixel_size {
        if i.in_points {
            // Back from pixels without the error of the way there.
            let pt = (f64::from(px * i.scale) * 0.75 * 1e4).round() / 1e4;
            let _ = write!(s, " font-size:{}pt;", num(pt as f32));
        } else {
            let _ = write!(s, " font-size:{}px;", num(px * i.scale));
        }
    } else if (i.scale - 1.0).abs() > 1e-4 {
        match scale_keyword(i.scale) {
            Some(name) => {
                let _ = write!(s, " font-size:{name};");
            }
            None => {
                let _ = write!(s, " font-size:{}%;", num(i.scale * 100.0));
            }
        }
    }
    if i.bold {
        s.push_str(" font-weight:700;");
    }
    if i.italic {
        s.push_str(" font-style:italic;");
    }
    let mut lines = Vec::new();
    if i.underline {
        lines.push("underline");
    }
    if i.strike {
        lines.push("line-through");
    }
    if i.overline {
        lines.push("overline");
    }
    if !lines.is_empty() {
        let _ = write!(s, " text-decoration: {};", lines.join(" "));
    } else if i.undecorated {
        s.push_str(" text-decoration: none;");
    }
    if let Some(c) = &i.color {
        let _ = write!(s, " color:{c};");
    }
    if let Some(c) = &i.background {
        let _ = write!(s, " background-color:{c};");
    }
    if let Some(c) = &i.decoration_color {
        let _ = write!(s, " text-decoration-color:{c};");
    }
    s
}

fn align_name(align: HAlign) -> Option<&'static str> {
    match align {
        HAlign::Left => None,
        HAlign::Right => Some("right"),
        HAlign::Center => Some("center"),
        HAlign::Justify => Some("justify"),
    }
}

/// A number as Qt writes it: no fraction when it has none.
fn num(v: f32) -> String {
    if v.fract() == 0.0 && v.abs() < 1e9 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(c: &str) -> Option<Arc<str>> {
        match c {
            "red" => Some(Arc::from("#ff0000")),
            c if c.starts_with('#') => Some(Arc::from(c)),
            _ => None,
        }
    }

    fn rich(source: &str) -> RichText {
        RichText::parse(TextFormat::Rich, source, &ParseOptions::default(), &hex)
    }

    fn md(source: &str) -> RichText {
        RichText::parse(TextFormat::Markdown, source, &ParseOptions::default(), &hex)
    }

    /// The body of the HTML, as the scenarios compare it.
    fn body(t: &RichText) -> String {
        let html = t.html(&DocFont::default());
        let start = html.find("<body").unwrap();
        let start = start + html[start..].find('>').unwrap() + 2;
        let end = html.find("</body>").unwrap();
        html[start..end]
            .replace(
                " style=\" margin-top:0px; margin-bottom:0px; margin-left:0px; \
                 margin-right:0px; -qt-block-indent:0; text-indent:0px;\"",
                "",
            )
            .replace('"', "'")
            .replace('\n', "|")
    }

    fn bold() -> &'static str {
        "<p>a<span style=' font-weight:700;'>bold</span>c</p>"
    }

    #[test]
    fn html_reads_back_as_qt_writes_it() {
        let t = rich("a<b>bold</b>c");
        assert_eq!(t.text(), "abold".to_owned() + "c");
        assert_eq!(body(&t), bold());
        // What was written reads back the same.
        let again = rich(&t.html(&DocFont::default()));
        assert_eq!(body(&again), bold());
    }

    #[test]
    fn typing_takes_the_format_before_it() {
        let mut t = rich("a<b>bold</b>c");
        t.splice(3..3, "x", None);
        assert_eq!(
            body(&t),
            "<p>a<span style=' font-weight:700;'>boxld</span>c</p>"
        );
        let mut t = rich("a<b>bold</b>c");
        t.splice(5..5, "x", None);
        assert_eq!(
            body(&t),
            "<p>a<span style=' font-weight:700;'>boldx</span>c</p>"
        );
        let mut t = rich("a<b>bold</b>c");
        t.splice(1..1, "x", None);
        assert_eq!(
            body(&t),
            "<p>ax<span style=' font-weight:700;'>bold</span>c</p>"
        );
    }

    #[test]
    fn typing_over_a_selection_takes_its_first_format() {
        let mut t = rich("a<b>bold</b>c");
        t.splice(1..5, "x", None);
        assert_eq!(
            body(&t),
            "<p>a<span style=' font-weight:700;'>x</span>c</p>"
        );
    }

    #[test]
    fn paragraphs_split_and_join() {
        let mut t = rich("a<b>bold</b>c");
        t.splice(3..3, "\n", None);
        assert_eq!(
            body(&t),
            "<p>a<span style=' font-weight:700;'>bo</span></p>|\
             <p><span style=' font-weight:700;'>ld</span>c</p>"
        );
        assert_eq!(t.blocks().len(), 2);
        t.splice(3..4, "", None);
        assert_eq!(body(&t), bold());
        assert_eq!(t.blocks().len(), 1);
    }

    #[test]
    fn rich_text_goes_in_between() {
        let mut t = rich("a<b>bold</b>c");
        t.splice_rich(3..3, &rich("<i>X</i>"));
        assert_eq!(
            body(&t),
            "<p>a<span style=' font-weight:700;'>bo</span><span style=' font-style:italic;'>X\
             </span><span style=' font-weight:700;'>ld</span>c</p>"
        );
    }

    #[test]
    fn a_fragment_is_marked() {
        let t = rich("a<b>bold</b>c");
        let h = t.html_fragment(2..4);
        let a = h.find("<!--StartFragment-->").unwrap() + 20;
        let b = h.find("<!--EndFragment-->").unwrap();
        assert_eq!(&h[a..b], "<span style=\" font-weight:700;\">ol</span>");
    }

    #[test]
    fn formats_change_over_a_range() {
        let mut t = rich("a<b>bold</b>c");
        t.set_char_format(5..6, |f| CharFormat {
            inline: Inline {
                bold: true,
                ..f.inline.clone()
            },
            whole_font: true,
            ..f.clone()
        });
        assert_eq!(
            body(&t),
            "<p>a<span style=' font-weight:700;'>bold</span><span style=' font-weight:700;'>c\
             </span></p>"
        );
        t.set_block_format(0..0, |b| BlockFormat {
            align: Some(HAlign::Center),
            ..b.clone()
        });
        assert!(body(&t).starts_with("<p align='center'>"));
    }

    #[test]
    fn links_are_blue_and_underlined() {
        let t = rich("<a href=\"u\">l</a>");
        assert_eq!(
            body(&t),
            "<p><a href='u'><span style=' text-decoration: underline; color:#0000ff;'>l</span>\
             </a></p>"
        );
        assert_eq!(t.link_at(0).as_deref(), Some("u"));
        assert_eq!(t.link_at(1), None);
    }

    #[test]
    fn an_image_is_one_character() {
        let t = rich("a<img src=\"pic.png\" width=\"20\" height=\"10\">b");
        assert_eq!(t.text(), "a\u{fffc}b");
        assert!(body(&t).contains("<img src='pic.png' width='20' height='10' />"));
        let objects = t.objects(|b| b, |_| (20.0, 10.0));
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].index, 4);
    }

    #[test]
    fn white_space_after_the_last_paragraph_is_a_space() {
        assert_eq!(rich("<p>x <b>y</b></p>").text(), "x y");
        assert_eq!(rich("<p>x <b>y</b></p>\n").text(), "x y ");
        assert_eq!(rich("<p>a</p> <p>b</p>").text(), "a\nb");
    }

    #[test]
    fn markdown_reads_back_as_qt_writes_it() {
        let t = md("a **bold** c\n\nsecond *it*");
        assert_eq!(t.text(), "a bold c\nsecond it");
        assert_eq!(t.markdown(), "a **bold** c\n\nsecond *it*\n\n");
        let mut typed = t.clone();
        typed.splice(4..4, "x", None);
        assert_eq!(typed.markdown(), "a **boxld** c\n\nsecond *it*\n\n");
        assert_eq!(t.markdown_fragment(0..6), "a **bold**\n\n");
        let mut split = t.clone();
        split.splice(4..4, "\n", None);
        assert_eq!(split.markdown(), "a **bo**\n\n**ld** c\n\nsecond *it*\n\n");
    }

    #[test]
    fn markdown_lists_and_headings() {
        let t = md("# Title\n\n- one\n- two\n\nafter");
        assert_eq!(t.markdown(), "# Title\n\n- one\n- two\n\nafter\n\n");
    }

    #[test]
    fn tables_are_read_as_rows() {
        let t = rich("<table><tr><td>a</td><td>b</td></tr><tr><td>c</td></tr></table>");
        assert_eq!(t.blocks().len(), 2);
        assert!(t.text().starts_with('a'));
    }

    #[test]
    fn the_cursor_reads_the_format_before_it() {
        let t = rich("a<b>bold</b>c");
        assert!(t.cursor_format(3).inline.bold);
        assert!(!t.cursor_format(1).inline.bold);
        assert!(t.cursor_format(5).inline.bold);
        // At the start of a paragraph: the character after it.
        let t = rich("<b>x</b>");
        assert!(t.cursor_format(0).inline.bold);
    }

    #[test]
    fn an_image_is_as_wide_as_its_box_between_its_positions() {
        use crate::{EditParams, Ranged, TextEngine, WrapMode};
        let t = rich("a<img src=\"pic.png\" width=\"20\" height=\"10\">b<br>c");
        let shown = RichText::shown(t.text());
        let objects = t.objects(|b| b, |_| (20.0, 10.0));
        let ranged = Ranged {
            spans: &[],
            objects: &objects,
            strut: false,
        };
        let params = EditParams {
            width: None,
            wrap: WrapMode::NoWrap,
        };
        let layout =
            TextEngine::new().edit_layout_ranged(&shown, &TextStyle::default(), &ranged, &params);
        let width = layout.caret(4).x - layout.caret(1).x;
        assert!((width - 20.0).abs() < 0.5, "{width}");
        // The line break inside the paragraph breaks the line.
        assert_eq!(layout.lines.len(), 2);
    }

    #[test]
    fn spans_follow_the_runs() {
        let t = rich("a<b>bold</b>c");
        let spans = t.style_spans(&TextStyle::default(), |b| b);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].range, 1..5);
        assert_eq!(spans[0].weight, 700.0);
    }
}
