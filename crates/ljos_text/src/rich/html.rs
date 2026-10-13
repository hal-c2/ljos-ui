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
//! Nothing here runs or fetches anything. The content of `script`, `style`
//! and `title` is dropped, unknown tags are dropped and their content kept,
//! and an image is only its source string.
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
//! and the first family of `font-family`. Every other property is left
//! out, and so is the background of a block or a table cell, which Qt
//! paints across the block.

use std::sync::Arc;

use super::doc::HAlign;
use super::entity;
use super::flow::{Ctx, Flow, table_block};
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
    /// White space inside is kept.
    pre: bool,
    /// An editor wrote it as an empty paragraph
    /// (`-qt-paragraph-type:empty`): its `<br />` is the paragraph's own
    /// line, not a break in it.
    empty: bool,
    /// It makes a block even when nothing is in it (`<p></p>` is a line).
    always: bool,
    /// A `ul` or `ol`: its bullets and how many items it has so far.
    list: Option<(Marker, u32)>,
}

struct TableState {
    rows: Vec<Vec<(InlineBuf, bool)>>,
    /// A cell is open: text goes to it.
    cell: bool,
    border: f32,
    spacing: f32,
    padding: f32,
    /// How many elements were open when the table started.
    depth: usize,
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
}

pub(crate) fn parse(source: &str) -> Vec<Block> {
    let mut p = Parser {
        src: source,
        at: 0,
        flow: Flow::default(),
        buf: InlineBuf::default(),
        open: Vec::new(),
        table: None,
        opened: 0,
    };
    p.run();
    p.finish()
}

impl Parser<'_> {
    fn run(&mut self) {
        while self.at < self.src.len() {
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
        self.end_table();
        while !self.open.is_empty() {
            self.close_top();
        }
        self.flush(false);
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

    fn pre(&self) -> bool {
        self.open.last().is_some_and(|o| o.pre)
    }

    /// Where text goes: the open table cell, or the block being collected.
    /// None inside a table but outside its cells, where text is dropped.
    fn target(&mut self) -> Option<&mut InlineBuf> {
        match &mut self.table {
            Some(t) if t.cell => t.rows.last_mut()?.last_mut().map(|c| &mut c.0),
            Some(_) => None,
            None => Some(&mut self.buf),
        }
    }

    fn text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let style = self.style();
        let pre = self.pre();
        let Some(buf) = self.target() else {
            return;
        };
        if pre {
            buf.push(s, &style);
            return;
        }
        let mut word = 0;
        for (i, c) in s.char_indices() {
            if !c.is_ascii_whitespace() {
                continue;
            }
            buf.push(&s[word..i], &style);
            word = i + 1;
            if !buf.is_empty() && !buf.text.ends_with([' ', '\n']) {
                buf.push(" ", &style);
            }
        }
        buf.push(&s[word..], &style);
    }

    fn line_break(&mut self) {
        let style = self.style();
        if let Some(buf) = self.target() {
            if buf.text.ends_with(' ') {
                buf.truncate(buf.text.len() - 1);
            }
            buf.push("\n", &style);
        }
    }

    /// At an `&`: a character when it names one, itself otherwise.
    fn reference(&mut self) {
        let rest = &self.src[self.at + 1..];
        let name_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '#'))
            .unwrap_or(rest.len());
        match entity::decode(&rest[..name_len]) {
            Some(c) => {
                let style = self.style();
                if let Some(buf) = self.target() {
                    buf.push(c.encode_utf8(&mut [0; 4]), &style);
                }
                let semicolon = usize::from(rest[name_len..].starts_with(';'));
                self.at += 1 + name_len + semicolon;
            }
            None => {
                self.text("&");
                self.at += 1;
            }
        }
    }

    /// At a `<`. False when it swallows the rest of the text.
    fn tag(&mut self) -> bool {
        let rest = &self.src[self.at + 1..];
        if let Some(comment) = rest.strip_prefix("!--") {
            let Some(end) = comment.find("-->") else {
                return false;
            };
            self.at += 1 + 3 + end + 3;
            return true;
        }
        let Some(end) = tag_end(rest) else {
            return false;
        };
        let inner = &rest[..end];
        self.at += end + 2;
        if let Some(name) = inner.strip_prefix('/') {
            let name = name.trim().to_ascii_lowercase();
            self.end_tag(&name);
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
        let closed = attrs.trim_end().ends_with('/');
        let attrs = Attrs(attrs);
        if matches!(name.as_str(), "script" | "style" | "title") {
            if !closed {
                self.skip_raw(&name);
            }
            return true;
        }
        self.start_tag(&name, &attrs);
        if closed && self.open.last().is_some_and(|o| *o.name == *name) {
            self.end_tag(&name);
        }
        true
    }

    /// Skip to after the end tag of `name`: what is between is not text.
    fn skip_raw(&mut self, name: &str) {
        let rest = &self.src[self.at..];
        let mut from = 0;
        loop {
            let Some(lt) = rest[from..].find("</") else {
                self.at = self.src.len();
                return;
            };
            let after = &rest[from + lt + 2..];
            let is_end = after
                .get(..name.len())
                .is_some_and(|n| n.eq_ignore_ascii_case(name));
            if is_end {
                let close = after.find('>').map_or(after.len(), |i| i + 1);
                self.at += from + lt + 2 + close;
                return;
            }
            from += lt + 2;
        }
    }

    fn push(&mut self, name: &str, change: impl FnOnce(&mut Open)) {
        if self.open.len() >= MAX_DEPTH {
            return;
        }
        let mut open = Open {
            name: name.into(),
            style: self.style(),
            boxed: false,
            pre: self.pre(),
            empty: false,
            always: false,
            list: None,
        };
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
        self.flow.open(ctx);
        self.push(name, |o| {
            o.boxed = true;
            change(o);
        });
    }

    /// Make a block of the text collected so far. An empty one only when
    /// `always`.
    fn flush(&mut self, always: bool) {
        let buf = std::mem::take(&mut self.buf);
        if !buf.is_empty() || always {
            self.flow.emit(buf, None, None);
        }
    }

    fn close_top(&mut self) {
        let Some(open) = self.open.pop() else {
            return;
        };
        if !open.boxed {
            return;
        }
        // Measured: text after `</div>` goes on in the same block.
        if &*open.name == "div" && !self.buf.is_empty() {
            self.flow.close();
            return;
        }
        self.flush(open.always && !self.flow.any());
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
        if self.opened != opened
            && let Some(css) = attrs.get("style")
            && let Some(open) = self.open.last_mut()
        {
            apply_style(open, css);
            if open.boxed {
                open.empty = css.contains("-qt-paragraph-type:empty");
                if let Some(ctx) = self.flow.innermost_mut() {
                    apply_box_style(ctx, css);
                }
            }
        }
    }

    fn start_element(&mut self, name: &str, attrs: &Attrs<'_>) {
        if self.table.is_some() {
            if self.table_tag(name) {
                return;
            }
        } else if name == "table" {
            self.start_table(attrs);
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
                let size = attrs.get("size").and_then(font_size);
                let color = attrs.get("color").filter(|c| !c.is_empty());
                self.push(name, |o| {
                    if let Some(scale) = size {
                        o.style.scale = scale;
                    }
                    if let Some(color) = color {
                        o.style.color = Some(Arc::from(color));
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
                    o.style.link = href.map(Arc::from);
                });
            }
            "br" if self.open.last().is_some_and(|o| o.empty) => {}
            "br" => self.line_break(),
            "img" => self.image(attrs),
            // Structure without a look of its own, and tags that stand alone.
            "html" | "body" | "head" | "meta" | "link" | "tbody" | "thead" | "tfoot" | "tr"
            | "td" | "th" | "caption" | "table" => {}
            // Inside a table cell there are no boxes: a block is a new line.
            _ if in_cell && is_block(name) => {
                let needs_break = self
                    .target()
                    .is_some_and(|b| !b.text.is_empty() && !b.text.ends_with('\n'));
                if needs_break {
                    self.line_break();
                }
                if let Some(level) = heading_level(name) {
                    self.push(name, |o| heading_style(&mut o.style, level));
                } else if name == "pre" {
                    self.push(name, |o| {
                        o.style.mono = true;
                        o.pre = true;
                    });
                } else if name != "hr" {
                    self.push(name, |_| {});
                }
            }
            "hr" => {
                self.imply_end("p", &[]);
                self.flush(false);
                self.flow
                    .emit(InlineBuf::default(), Some(BlockKind::Rule), None);
            }
            "p" => {
                self.imply_end("p", &[]);
                let ctx = Ctx {
                    margin_top: 12.0,
                    margin_bottom: 12.0,
                    align: attrs.get("align").and_then(align),
                    ..Ctx::default()
                };
                self.open_box(name, ctx, |o| o.always = true);
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.imply_end("p", &[]);
                let level = heading_level(name).unwrap_or(1);
                let (margin_top, margin_bottom) = match level {
                    1 => (18.0, 12.0),
                    2 => (16.0, 12.0),
                    3 => (14.0, 12.0),
                    4 => (12.0, 12.0),
                    5 => (12.0, 4.0),
                    _ => (0.0, 0.0),
                };
                let ctx = Ctx {
                    margin_top,
                    margin_bottom,
                    align: attrs.get("align").and_then(align),
                    kind: Some(BlockKind::Heading(level)),
                    ..Ctx::default()
                };
                self.open_box(name, ctx, |o| {
                    o.always = true;
                    heading_style(&mut o.style, level);
                });
            }
            "pre" => {
                self.imply_end("p", &[]);
                let ctx = Ctx {
                    margin_top: 12.0,
                    margin_bottom: 12.0,
                    kind: Some(BlockKind::Code),
                    ..Ctx::default()
                };
                self.open_box(name, ctx, |o| {
                    o.always = true;
                    o.pre = true;
                    o.style.mono = true;
                });
            }
            "ul" | "ol" => {
                self.imply_end("p", &[]);
                let depth = self.open.iter().filter(|o| o.list.is_some()).count();
                let marker = list_marker(name == "ol", attrs.get("type"), depth);
                let ctx = Ctx {
                    margin_top: 12.0,
                    margin_bottom: 12.0,
                    indent: 40.0,
                    kind: Some(BlockKind::List),
                    ..Ctx::default()
                };
                self.open_box(name, ctx, |o| o.list = Some((marker, 0)));
            }
            "li" => {
                self.imply_end("li", &["p", "div"]);
                let marker = self
                    .open
                    .iter_mut()
                    .rev()
                    .find_map(|o| o.list.as_mut())
                    .map(|(marker, count)| {
                        *count = count.saturating_add(1);
                        Arc::from(marker_text(*marker, *count))
                    });
                let ctx = Ctx {
                    item: marker.is_some(),
                    marker,
                    ..Ctx::default()
                };
                self.open_box(name, ctx, |o| o.always = true);
            }
            "blockquote" => {
                self.imply_end("p", &[]);
                let ctx = Ctx {
                    margin_top: 12.0,
                    margin_bottom: 12.0,
                    indent: 40.0,
                    indent_right: 40.0,
                    kind: Some(BlockKind::Quote),
                    ..Ctx::default()
                };
                self.open_box(name, ctx, |_| {});
            }
            "dl" => {
                self.imply_end("p", &[]);
                let ctx = Ctx {
                    margin_top: 12.0,
                    margin_bottom: 12.0,
                    ..Ctx::default()
                };
                self.open_box(name, ctx, |_| {});
            }
            "dt" | "dd" => {
                self.imply_end("dt", &["p"]);
                self.imply_end("dd", &["p"]);
                let ctx = Ctx {
                    indent: if name == "dd" { 30.0 } else { 0.0 },
                    ..Ctx::default()
                };
                self.open_box(name, ctx, |o| o.always = true);
            }
            "div" | "center" => {
                self.imply_end("p", &[]);
                let ctx = Ctx {
                    align: if name == "center" {
                        Some(HAlign::Center)
                    } else {
                        attrs.get("align").and_then(align)
                    },
                    ..Ctx::default()
                };
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
                if let Some(t) = &mut self.table {
                    t.cell = false;
                    self.open.truncate(t.depth);
                }
            }
            "tr" | "tbody" | "thead" | "tfoot" | "html" | "body" | "head" | "br" | "hr" | "img" => {
            }
            _ => self.close(name),
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
            depth: self.open.len(),
        });
    }

    /// Tags that mean something else inside a table. False for the others.
    fn table_tag(&mut self, name: &str) -> bool {
        let Some(t) = &mut self.table else {
            return false;
        };
        match name {
            "tr" => {
                t.cell = false;
                t.rows.push(Vec::new());
                self.open.truncate(t.depth);
                true
            }
            "td" | "th" => {
                if t.rows.is_empty() {
                    t.rows.push(Vec::new());
                }
                let header = name == "th";
                if let Some(row) = t.rows.last_mut() {
                    row.push((InlineBuf::default(), header));
                }
                t.cell = true;
                self.open.truncate(t.depth);
                if header {
                    self.push(name, |o| o.style.bold = true);
                }
                true
            }
            // A table in a table is read as more rows of the outer one.
            "table" => true,
            _ => !t.cell && is_block(name),
        }
    }

    fn end_table(&mut self) {
        let Some(t) = self.table.take() else {
            return;
        };
        self.open.truncate(t.depth);
        let (buf, table) = table_block(t.rows, t.border, t.spacing, t.padding);
        if table.cells.is_empty() {
            return;
        }
        self.flow.emit(buf, Some(BlockKind::Table), Some(table));
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
            align: match attrs.get("align") {
                Some("top") => BoxAlign::Top,
                Some("middle") => BoxAlign::Middle,
                _ => BoxAlign::Baseline,
            },
            in_spaces: false,
        };
        if let Some(buf) = self.target() {
            buf.images.push(InlineImage {
                index: buf.text.len(),
                ..image
            });
        }
    }
}

/// What a `style` attribute says about colours and lines (see the module's
/// documentation), read the way Qt 6.11 does: declarations count in the
/// order they are written, and one of a property nobody knows or one that
/// is no `name: value` is passed over. Unless that one is the first: then
/// the whole style says nothing.
fn apply_style(open: &mut Open, css: &str) {
    let css = without_comments(css);
    for (i, declaration) in css.split(';').enumerate() {
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
            "font-family" => {
                let first = value.split(',').next().unwrap_or_default();
                let family = first.trim().trim_matches(['\'', '"']).trim();
                if !family.is_empty() {
                    style.family = Some(Arc::from(family));
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

/// The margins of a box (`margin-top` and its kin, in pixels), which an
/// editor writes for every paragraph. The `margin` shorthand is not read.
fn apply_box_style(ctx: &mut Ctx, css: &str) {
    let css = without_comments(css);
    for declaration in css.split(';') {
        let Some((name, value)) = declaration.split_once(':') else {
            continue;
        };
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

/// A length of `px` or `pt` (a point is four thirds of a pixel), within
/// sizes a font can have.
fn css_pixels(value: &str) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    let (number, factor) = if let Some(n) = value.strip_suffix("px") {
        (n, 1.0)
    } else {
        (value.strip_suffix("pt")?, 4.0 / 3.0)
    };
    let px = number.trim().parse::<f32>().ok()? * factor;
    (px.is_finite() && px > 0.0).then(|| px.min(1000.0))
}

/// `css` without its `/* comments */`. One that never ends takes the rest.
fn without_comments(css: &str) -> std::borrow::Cow<'_, str> {
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
    fn get(&self, name: &str) -> Option<&'a str> {
        let mut rest = self.0;
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
                if key.eq_ignore_ascii_case(name) {
                    return Some("");
                }
                rest = after;
                continue;
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
            if key.eq_ignore_ascii_case(name) {
                return Some(value);
            }
            rest = next;
        }
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
    fn a_background_is_of_text_and_not_of_a_block() {
        // A block's background would fill the block; that is not drawn, so
        // it is not kept as the background of the letters either.
        for tag in ["p", "div", "li", "h1", "td"] {
            let source = match tag {
                "td" => "<table><tr><td style=\"background-color: red; color: blue\">a</td></tr></table>".to_owned(),
                "li" => "<ul><li style=\"background-color: red; color: blue\">a</li></ul>".to_owned(),
                _ => format!("<{tag} style=\"background-color: red; color: blue\">a</{tag}>"),
            };
            for block in parse(&source) {
                for span in &block.spans {
                    assert_eq!(span.style.background, None, "{tag}");
                }
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
}
