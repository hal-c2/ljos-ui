//! Markdown: CommonMark with tables, strike-through and task lists.
//!
//! pulldown-cmark reads the source; this turns its events into blocks. Only
//! the parser is used: its HTML renderer is not linked, and raw HTML in the
//! source (a block or a tag in a line) becomes the text it is. Qt hands such
//! HTML to its own importer; a document here cannot bring in a second
//! language.
//!
//! The geometry is Qt 6.11's as measured: lists and quotes stand in by 40
//! pixels a level (quotes from the right too), headings are bold and sized
//! like `StyledText`'s, a soft line break is a space, and paragraphs are
//! set apart by [`ParseOptions::paragraph_spacing`].

use std::sync::Arc;

use pulldown_cmark::{Event, Options, Parser, Tag};

use super::flow::{Ctx, Flow, table_block};
use super::styled::HEADING_SCALE;
use super::{Block, BlockKind, Inline, InlineBuf, InlineImage, ParseOptions, Parsed};
use crate::style::BoxAlign;

/// Styles nested deeper than this are not remembered: a line of ten thousand
/// `*` must not keep ten thousand copies of a style.
const MAX_STYLE_DEPTH: usize = 256;

/// What an end event closes. pulldown-cmark's events are balanced, so a
/// stack of these mirrors its open tags one to one.
enum Opened {
    Nothing,
    Style,
    /// A box of the flow; `always` makes a block even of nothing, `style`
    /// says a style was pushed with it.
    Box {
        always: bool,
        style: bool,
    },
    List,
    Code,
    Html,
    Table,
    Head,
    Row,
    Cell,
    Image,
}

struct Reader {
    spacing: f32,
    flow: Flow,
    buf: InlineBuf,
    styles: Vec<Inline>,
    open: Vec<Opened>,
    /// The next number of every open list; none for a bulleted one.
    lists: Vec<Option<u64>>,
    rows: Vec<Vec<(InlineBuf, bool)>>,
    row: Vec<(InlineBuf, bool)>,
    header: bool,
    /// Source and alternative text of the image being read.
    image: Option<(Arc<str>, String)>,
}

/// Parse `source`, which starts `base` bytes into the document.
pub(crate) fn parse(source: &str, base: usize, options: &ParseOptions) -> Parsed {
    let extensions =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(source, extensions).into_offset_iter();
    // Known as soon as the parser exists: its first pass is done.
    let references = parser.reference_definitions().iter().next().is_some();
    let spacing = options.paragraph_spacing;
    let mut reader = Reader {
        spacing: if spacing.is_finite() {
            spacing.clamp(0.0, 1000.0)
        } else {
            0.0
        },
        flow: Flow::default(),
        buf: InlineBuf::default(),
        styles: vec![Inline::default()],
        open: Vec::new(),
        lists: Vec::new(),
        rows: Vec::new(),
        row: Vec::new(),
        header: false,
        image: None,
    };
    for (event, range) in parser {
        if reader.open.is_empty() {
            // A streamed update parses again from the line the last
            // top-level block starts on (its indent belongs to it).
            let line = source
                .get(..range.start)
                .and_then(|before| before.rfind(['\n', '\r']))
                .map_or(0, |i| i + 1);
            reader.flow.source_start = base + line;
        }
        reader.event(event);
    }
    Parsed {
        blocks: reader.finish(),
        references,
    }
}

impl Reader {
    fn finish(mut self) -> Vec<Block> {
        self.flush(false);
        self.flow.finish()
    }

    fn style(&self) -> Inline {
        self.styles.last().cloned().unwrap_or_default()
    }

    fn push_style(&mut self, change: impl FnOnce(&mut Inline)) -> bool {
        if self.styles.len() >= MAX_STYLE_DEPTH {
            return false;
        }
        let mut style = self.style();
        change(&mut style);
        self.styles.push(style);
        true
    }

    fn pop_style(&mut self) {
        if self.styles.len() > 1 {
            self.styles.pop();
        }
    }

    /// Open an inline style; what the end event has to undo.
    fn styled(&mut self, change: impl FnOnce(&mut Inline)) -> Opened {
        if self.push_style(change) {
            Opened::Style
        } else {
            Opened::Nothing
        }
    }

    fn text(&mut self, s: &str) {
        match &mut self.image {
            Some((_, alt)) => alt.push_str(s),
            None => {
                let style = self.style();
                self.buf.push(s, &style);
            }
        }
    }

    fn flush(&mut self, always: bool) {
        let buf = std::mem::take(&mut self.buf);
        if !buf.is_empty() || always {
            self.flow.emit(buf, None, None);
        }
    }

    fn open_box(&mut self, ctx: Ctx) {
        // Text right in a list item (a tight list) ends where a block
        // inside the item starts.
        self.flush(false);
        self.flow.open(ctx);
    }

    fn paragraph_ctx(&self) -> Ctx {
        Ctx {
            margin_top: self.spacing,
            margin_bottom: self.spacing,
            ..Ctx::default()
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => {
                let opened = self.start(tag);
                self.open.push(opened);
            }
            Event::End(_) => {
                if let Some(opened) = self.open.pop() {
                    self.end(opened);
                }
            }
            Event::Text(s) => self.text(&s),
            Event::Code(s) => {
                if self.image.is_some() {
                    self.text(&s);
                } else {
                    let mut style = self.style();
                    style.mono = true;
                    self.buf.push(&s, &style);
                }
            }
            // Raw HTML is shown, never read: see the module's note.
            Event::Html(s) | Event::InlineHtml(s) => self.text(&s),
            Event::InlineMath(s) | Event::DisplayMath(s) => self.text(&s),
            Event::FootnoteReference(s) => {
                self.text("[^");
                self.text(&s);
                self.text("]");
            }
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.text("\n"),
            Event::Rule => {
                self.flush(false);
                self.flow
                    .emit(InlineBuf::default(), Some(BlockKind::Rule), None);
            }
            Event::TaskListMarker(checked) => {
                self.flow
                    .set_marker(if checked { "\u{2611}" } else { "\u{2610}" });
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) -> Opened {
        match tag {
            Tag::Paragraph => {
                let ctx = self.paragraph_ctx();
                self.open_box(ctx);
                Opened::Box {
                    always: false,
                    style: false,
                }
            }
            Tag::Heading { level, .. } => {
                let level = (level as usize).clamp(1, 6);
                self.open_box(Ctx {
                    kind: Some(BlockKind::Heading(level as u8)),
                    ..Ctx::default()
                });
                let style = self.push_style(|s| {
                    s.bold = true;
                    s.scale = HEADING_SCALE[level - 1];
                });
                Opened::Box {
                    always: true,
                    style,
                }
            }
            Tag::BlockQuote(_) => {
                self.open_box(Ctx {
                    indent: 40.0,
                    indent_right: 40.0,
                    kind: Some(BlockKind::Quote),
                    ..Ctx::default()
                });
                Opened::Box {
                    always: false,
                    style: false,
                }
            }
            Tag::CodeBlock(_) => {
                self.open_box(Ctx {
                    kind: Some(BlockKind::Code),
                    ..Ctx::default()
                });
                self.push_style(|s| s.mono = true);
                Opened::Code
            }
            Tag::HtmlBlock => {
                let ctx = self.paragraph_ctx();
                self.open_box(ctx);
                Opened::Html
            }
            Tag::List(first) => {
                self.open_box(Ctx {
                    indent: 40.0,
                    kind: Some(BlockKind::List),
                    ..Ctx::default()
                });
                self.lists.push(first);
                Opened::List
            }
            Tag::Item => {
                let depth = self.lists.len();
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let marker = format!("{n}.");
                        *n = n.saturating_add(1);
                        marker
                    }
                    _ => match depth {
                        0 | 1 => "\u{2022}",
                        2 => "\u{25E6}",
                        _ => "\u{25A0}",
                    }
                    .to_owned(),
                };
                let ctx = Ctx {
                    item: true,
                    marker: Some(Arc::from(marker)),
                    ..self.paragraph_ctx()
                };
                self.open_box(ctx);
                Opened::Box {
                    always: true,
                    style: false,
                }
            }
            Tag::Table(_) => {
                self.flush(false);
                self.rows.clear();
                Opened::Table
            }
            Tag::TableHead => {
                self.header = true;
                self.row.clear();
                self.push_style(|s| s.bold = true);
                Opened::Head
            }
            Tag::TableRow => {
                self.row.clear();
                Opened::Row
            }
            Tag::TableCell => {
                self.buf = InlineBuf::default();
                Opened::Cell
            }
            Tag::Emphasis => self.styled(|s| s.italic = true),
            Tag::Strong => self.styled(|s| s.bold = true),
            Tag::Strikethrough => self.styled(|s| s.strike = true),
            Tag::Superscript | Tag::Subscript => self.styled(|s| s.scale *= 2.0 / 3.0),
            Tag::Link { dest_url, .. } => {
                let href: Arc<str> = Arc::from(&*dest_url);
                self.styled(|s| s.link = Some(href))
            }
            Tag::Image { dest_url, .. } => {
                // An image in an image's text is part of that text.
                if self.image.is_some() {
                    Opened::Nothing
                } else {
                    self.image = Some((Arc::from(&*dest_url), String::new()));
                    Opened::Image
                }
            }
            // Extensions that are not switched on.
            _ => Opened::Nothing,
        }
    }

    fn end(&mut self, opened: Opened) {
        match opened {
            Opened::Nothing => {}
            Opened::Style => self.pop_style(),
            Opened::Box { always, style } => {
                if style {
                    self.pop_style();
                }
                self.flush(always && !self.flow.any());
                self.flow.close();
            }
            Opened::List => {
                self.flush(false);
                self.flow.close();
                self.lists.pop();
            }
            Opened::Code | Opened::Html => {
                if matches!(opened, Opened::Code) {
                    self.pop_style();
                }
                // The line break the block's last line ends in is not a
                // line of its own.
                if self.buf.text.ends_with('\n') {
                    self.buf.truncate(self.buf.text.len() - 1);
                }
                self.flush(true);
                self.flow.close();
            }
            Opened::Table => {
                let rows = std::mem::take(&mut self.rows);
                // Qt 6.11 draws a Markdown table with a frame and 2.5
                // pixels around each cell's text.
                let rows = rows
                    .into_iter()
                    .map(|row| row.into_iter().map(|(b, h)| (b, h, None)).collect())
                    .collect();
                let (buf, table) = table_block(rows, 1.0, 2.0, 2.5);
                if table.cells.is_empty() {
                    return;
                }
                self.flow.emit(buf, Some(BlockKind::Table), Some(table));
                if let Some(block) = self.flow.last_mut() {
                    block.tight_above = true;
                }
            }
            Opened::Head => {
                self.header = false;
                self.pop_style();
                let row = std::mem::take(&mut self.row);
                self.rows.push(row);
            }
            Opened::Row => {
                let row = std::mem::take(&mut self.row);
                self.rows.push(row);
            }
            Opened::Cell => {
                let cell = std::mem::take(&mut self.buf);
                self.row.push((cell, self.header));
            }
            Opened::Image => {
                if let Some((source, alt)) = self.image.take() {
                    self.buf.images.push(InlineImage {
                        index: self.buf.text.len(),
                        source,
                        alt: Arc::from(alt),
                        width: None,
                        height: None,
                        align: BoxAlign::Baseline,
                        in_spaces: false,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(source: &str) -> Vec<Block> {
        parse(source, 0, &ParseOptions::default()).blocks
    }

    fn texts(source: &str) -> Vec<String> {
        blocks(source).into_iter().map(|b| b.text).collect()
    }

    fn kinds(source: &str) -> Vec<BlockKind> {
        blocks(source).into_iter().map(|b| b.kind).collect()
    }

    #[test]
    fn paragraphs_headings_and_breaks() {
        assert_eq!(texts("abc\n\nabc"), ["abc", "abc"]);
        assert_eq!(texts("abc\nabc"), ["abc abc"], "a soft break is a space");
        assert_eq!(texts("abc  \nabc"), ["abc\nabc"], "a hard break is one");
        let b = blocks("# abc\n\nabc\n\n###### abc");
        assert_eq!(
            b.iter().map(|b| b.kind).collect::<Vec<_>>(),
            [
                BlockKind::Heading(1),
                BlockKind::Paragraph,
                BlockKind::Heading(6)
            ]
        );
        assert!(b[0].spans[0].style.bold);
        assert_eq!(b[0].spans[0].style.scale, 2.0);
        assert_eq!(b[2].spans[0].style.scale, 0.7);
        assert_eq!(texts("abc\n==="), ["abc"]);
        assert_eq!(texts("#"), [""], "an empty heading is still a line");
        assert!(texts("").is_empty());
    }

    #[test]
    fn paragraph_spacing_is_the_callers() {
        let options = ParseOptions {
            paragraph_spacing: 8.0,
            link_color: None,
        };
        let b = parse("abc\n\n# abc\n\n- abc\n\n> abc\n\n    abc", 0, &options).blocks;
        let margins: Vec<_> = b.iter().map(|b| (b.margin_top, b.margin_bottom)).collect();
        assert_eq!(
            margins,
            [(8.0, 8.0), (0.0, 0.0), (8.0, 8.0), (8.0, 8.0), (0.0, 0.0)]
        );
        let b = blocks("abc\n\nabc");
        assert_eq!((b[0].margin_top, b[0].margin_bottom), (0.0, 0.0));
    }

    #[test]
    fn inline_styles() {
        let b = &blocks("a **b** *c* `d` ~~e~~ [f](http://x/)")[0];
        assert_eq!(b.text, "a b c d e f");
        let s: Vec<_> = b.spans.iter().map(|s| &s.style).collect();
        assert!(s[0].bold);
        assert!(s[1].italic);
        assert!(s[2].mono);
        assert!(s[3].strike);
        assert_eq!(s[4].link.as_deref(), Some("http://x/"));
        let b = &blocks("***a***")[0];
        assert!(b.spans[0].style.bold && b.spans[0].style.italic);
    }

    #[test]
    fn lists_have_markers_and_indents() {
        let b = blocks("- a\n- b\n  - c\n\n1. d\n2. e");
        let shape: Vec<_> = b
            .iter()
            .map(|b| (b.text.as_str(), b.marker.as_deref(), b.indent, b.kind))
            .collect();
        assert_eq!(
            shape,
            [
                ("a", Some("\u{2022}"), 40.0, BlockKind::List),
                ("b", Some("\u{2022}"), 40.0, BlockKind::List),
                ("c", Some("\u{25E6}"), 80.0, BlockKind::List),
                ("d", Some("1."), 40.0, BlockKind::List),
                ("e", Some("2."), 40.0, BlockKind::List),
            ]
        );
        // The number a list starts at is kept.
        assert_eq!(blocks("7. a\n8. b")[1].marker.as_deref(), Some("8."));
        // A second paragraph of an item has no marker.
        let b = blocks("- a\n\n  b");
        assert_eq!(
            (b[1].text.as_str(), b[1].marker.clone(), b[1].indent),
            ("b", None, 40.0)
        );
        // Task lists show their box instead of the bullet.
        let b = blocks("- [ ] a\n- [x] b");
        assert_eq!(b[0].marker.as_deref(), Some("\u{2610}"));
        assert_eq!(b[1].marker.as_deref(), Some("\u{2611}"));
        // An empty item is still a line with its bullet.
        let b = blocks("- a\n-\n- c");
        assert_eq!(b.len(), 3);
        assert_eq!(b[1].marker.as_deref(), Some("\u{2022}"));
    }

    #[test]
    fn quotes_code_and_rules() {
        let b = blocks("> abc\n> > def");
        assert_eq!(
            (b[0].kind, b[0].indent, b[0].indent_right),
            (BlockKind::Quote, 40.0, 40.0)
        );
        assert_eq!((b[1].indent, b[1].indent_right), (80.0, 80.0));
        let b = blocks("```rust\nfn a() {}\n\n  b\n```");
        assert_eq!(b.len(), 1);
        assert_eq!(
            (b[0].kind, b[0].text.as_str()),
            (BlockKind::Code, "fn a() {}\n\n  b")
        );
        assert!(b[0].spans[0].style.mono);
        // A fence that is still open is a block already.
        assert_eq!(kinds("abc\n\n```"), [BlockKind::Paragraph, BlockKind::Code]);
        assert_eq!(
            kinds("abc\n\n---\n\nabc"),
            [BlockKind::Paragraph, BlockKind::Rule, BlockKind::Paragraph]
        );
    }

    #[test]
    fn tables() {
        let b = blocks("abc\n\n| a | b |\n|---|---|\n| c | *d* |");
        assert_eq!(b.len(), 2);
        let t = &b[1];
        assert_eq!(t.kind, BlockKind::Table);
        assert_eq!(t.text, "a\tb\nc\td");
        assert!(t.tight_above);
        let table = t.table.as_ref().expect("a table");
        assert_eq!((table.columns, table.cells.len()), (2, 4));
        assert_eq!(
            (table.border, table.spacing, table.padding),
            (1.0, 2.0, 2.5)
        );
        assert!(table.cells[0].header && table.cells[1].header && !table.cells[2].header);
        assert!(t.spans[0].style.bold && t.spans[0].range == (0..1));
        assert!(t.spans.last().is_some_and(|s| s.style.italic));
    }

    #[test]
    fn raw_html_is_text() {
        assert_eq!(
            texts("a <b>b</b> <script>alert(1)</script>"),
            ["a <b>b</b> <script>alert(1)</script>"]
        );
        let b = blocks("<div onclick=\"x()\">\nabc\n</div>\n\nabc");
        assert_eq!(b[0].text, "<div onclick=\"x()\">\nabc\n</div>");
        assert!(b[0].spans.is_empty());
        assert_eq!(b[1].text, "abc");
    }

    #[test]
    fn images_keep_their_source_and_say_their_text() {
        let b = &blocks("a ![the *alt*](pic.png \"title\") b")[0];
        assert_eq!(b.text, "a  b");
        assert_eq!(b.images.len(), 1);
        assert_eq!(&*b.images[0].source, "pic.png");
        assert_eq!(&*b.images[0].alt, "the alt");
        assert_eq!(b.images[0].index, 2);
        assert_eq!(kinds("![x](y.png)"), [BlockKind::Image]);
    }

    #[test]
    fn blocks_know_the_line_their_top_level_block_starts_on() {
        // One space is too little for `def` to go on in the list item, and
        // it still belongs to the paragraph.
        let source = "abc\n\n- a\n- b\n\n  c\n\n def\nghi\n";
        let b = parse(source, 100, &ParseOptions::default()).blocks;
        let starts: Vec<_> = b.iter().map(|b| b.source_start - 100).collect();
        assert_eq!(starts, [0, 5, 5, 5, 19]);
        assert_eq!(&source[19..], " def\nghi\n");
    }

    #[test]
    fn reference_definitions_are_reported() {
        assert!(!parse("abc [x](y)", 0, &ParseOptions::default()).references);
        let p = parse("[x]\n\n[x]: http://y/", 0, &ParseOptions::default());
        assert!(p.references);
        assert_eq!(
            p.blocks[0].spans[0].style.link.as_deref(),
            Some("http://y/")
        );
    }

    #[test]
    fn nesting_is_bounded() {
        let deep = "> ".repeat(5000) + "x";
        let b = blocks(&deep);
        assert!(b.iter().all(|b| b.indent <= 2000.0));
        let stars = "*a ".repeat(20_000);
        assert!(!blocks(&stars).is_empty());
        let brackets = "[".repeat(50_000);
        assert_eq!(blocks(&brackets).len(), 1);
    }
}
