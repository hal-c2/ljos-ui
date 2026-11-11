//! The text written out as Qt writes it: [`RichText::html`] as its HTML
//! document, [`RichText::markdown`] as its Markdown.

use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

use super::super::css;
use super::super::doc::HAlign;
use super::super::html::scale_keyword;
use super::*;

impl RichText {
    /// Qt's HTML document of the text, set in `font`.
    pub fn html(&self, font: &DocFont) -> String {
        let mut out = String::new();
        write_head(&mut out);
        let mut family = String::new();
        css::write_families(&mut family, [font.family.as_str()]);
        let _ = writeln!(
            out,
            "<body style=\" font-family:{}; font-size:{}px; font-weight:{}; font-style:{};\">",
            escape_attr(&family),
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
        let mut written = false;
        let mut index = 0;
        while index < count {
            let block = &self.blocks[index];
            let range = self.block_range(index);
            if self.beside_table(index) {
                index += 1;
                continue;
            }
            let in_list = block.kind == BlockKind::List;
            let ordered = block.ordered();
            // Measured: a paragraph out of a list stays in it while items of
            // the list follow.
            let between = !in_list
                && list.is_some()
                && block.list.as_ref().is_some_and(|l| {
                    self.blocks[index + 1..].iter().any(|b| {
                        b.kind == BlockKind::List
                            && b.list.as_ref().is_some_and(|o| Arc::ptr_eq(o, l))
                    })
                });
            if !between {
                end_list(out, &mut list, in_list.then_some(ordered));
            }
            if written {
                out.push('\n');
            }
            written = true;
            if let Some(row) = &block.table {
                let end = (index..count)
                    .find(|i| {
                        self.blocks[*i]
                            .table
                            .as_ref()
                            .is_none_or(|r| !r.same_table(row))
                    })
                    .unwrap_or(count);
                self.write_table(out, &row.table, index..end);
                index = end;
                continue;
            }
            index += 1;
            let index = index - 1;
            if in_list {
                start_list(out, &mut list, ordered);
            }
            if block.kind == BlockKind::Rule {
                out.push_str("<hr />");
                continue;
            }
            // Measured: a fragment's marks are in paragraphs, not in cells.
            let marks = (fragment && index == 0, fragment && index + 1 == count);
            self.write_paragraph(out, block, range, marks);
        }
        end_list(out, &mut list, None);
    }

    /// Paragraph `index` is the empty one Qt has after every table, or
    /// before one that starts the document, which it does not write.
    fn beside_table(&self, index: usize) -> bool {
        let block = &self.blocks[index];
        if !self.block_range(index).is_empty() || **block != BlockFormat::default() {
            return false;
        }
        let table = |i: Option<usize>| {
            i.and_then(|i| self.blocks.get(i))
                .is_some_and(|b| b.table.is_some())
        };
        let (before, after) = (table(index.checked_sub(1)), table(Some(index + 1)));
        before || (index == 0 && after)
    }

    /// The rows `rows` of a table as Qt writes them.
    fn write_table(&self, out: &mut String, table: &TableFormat, rows: Range<usize>) {
        let _ = write!(
            out,
            "<table border=\"{}\" style=\" margin-top:0px; margin-bottom:0px; \
             margin-left:0px; margin-right:0px;\"",
            num(table.border)
        );
        if let Some(width) = &table.width {
            let _ = write!(out, " width=\"{}\"", escape_attr(width));
        }
        let _ = write!(
            out,
            " cellspacing=\"{}\" cellpadding=\"{}\"",
            num(table.spacing),
            num(table.padding)
        );
        if let Some(bg) = &table.background {
            let _ = write!(out, " bgcolor=\"{bg}\"");
        }
        out.push('>');
        for row in rows {
            out.push_str("\n<tr>");
            let range = self.block_range(row);
            let cells = self.blocks[row].table.as_ref().map(|r| &r.cells);
            let mut at = range.start;
            for (c, text) in self.text[range].split(CELL).enumerate() {
                let cell = at..at + text.len();
                at = cell.end + 1;
                let format = cells.and_then(|cells| cells.get(c));
                out.push_str("\n<td");
                if let Some(bg) = format.and_then(|f| f.background.as_deref()) {
                    let _ = write!(out, " bgcolor=\"{bg}\"");
                }
                out.push('>');
                // A cell's items are in a list of their own, as out of a
                // table.
                let mut list = None;
                let mut start = cell.start;
                for (k, text) in self.text[cell].split(CELL_PARAGRAPH).enumerate() {
                    let paragraph = start..start + text.len();
                    start = paragraph.end + CELL_PARAGRAPH.len_utf8();
                    // Measured: an empty first paragraph is not written, an
                    // empty cell is none.
                    if k == 0 && paragraph.is_empty() {
                        continue;
                    }
                    // The paragraph's own record; one past the records'
                    // end is a default paragraph.
                    let mut block = format.and_then(|f| f.paragraphs.get(k)).map_or_else(
                        || Cow::Owned(BlockFormat::default()),
                        |p| Cow::Borrowed(&**p),
                    );
                    // Measured: a th is written as a td in the middle.
                    if format.is_some_and(|f| f.header) && block.align.is_none() {
                        block.to_mut().align = Some(HAlign::Center);
                    }
                    let in_list = block.kind == BlockKind::List;
                    end_list(out, &mut list, in_list.then(|| block.ordered()));
                    out.push('\n');
                    if in_list {
                        start_list(out, &mut list, block.ordered());
                    }
                    self.write_paragraph(out, &block, paragraph, (false, false));
                }
                end_list(out, &mut list, None);
                out.push_str("</td>");
            }
            out.push_str("</tr>");
        }
        out.push_str("</table>");
    }

    /// Paragraph `range`, set as `block` says, as Qt writes it out of a
    /// table or in a cell; `marks` says whether the fragment starts or ends
    /// in it.
    fn write_paragraph(
        &self,
        out: &mut String,
        block: &BlockFormat,
        range: Range<usize>,
        (starts, ends): (bool, bool),
    ) {
        // A block is written with its own tag, a code block as a pre, and
        // says whether it wraps, so that it reads back as it was
        // (features/text/rich-text-white-space.feature).
        // Qt 6.11: a paragraph that does not wrap is a pre, whatever it is,
        // in an li left open in a list, and a code block that wraps, or a
        // fenced one of Markdown, is a p; read back, every pre of the
        // editor's HTML is monospace and wraps (6.12 the same).
        let tag = match block.kind {
            BlockKind::Code => "pre".to_owned(),
            BlockKind::Heading(n) => format!("h{}", n.clamp(1, 6)),
            BlockKind::List => "li".to_owned(),
            _ => "p".to_owned(),
        };
        // Qt 6.11: a paragraph with no text says so in its style, and holds
        // a line break (6.12 the same).
        let empty = range.is_empty();
        out.push('<');
        out.push_str(&tag);
        if let Some(align) = block.align.and_then(align_name) {
            let _ = write!(out, " align=\"{align}\"");
        }
        let _ = write!(
            out,
            " style=\"{}margin-top:{}px; margin-bottom:{}px; margin-left:{}px; \
             margin-right:{}px; -qt-block-indent:0; text-indent:0px;{}{}\">",
            if empty {
                "-qt-paragraph-type:empty; "
            } else {
                " "
            },
            num(block.margin_top),
            num(block.margin_bottom),
            num(if block.kind == BlockKind::List {
                0.0
            } else {
                block.indent
            }),
            num(block.indent_right),
            block
                .background
                .as_deref()
                .map(|c| format!(" background-color:{c};"))
                .unwrap_or_default(),
            white_space(block.no_wrap, block.kind == BlockKind::Code),
        );
        if starts {
            out.push_str("<!--StartFragment-->");
        }
        if empty {
            out.push_str("<br />");
        } else {
            self.write_inline(out, range);
        }
        if ends {
            out.push_str("<!--EndFragment-->");
        }
        let _ = write!(out, "</{tag}>");
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
    /// items of a list follow each other, and a nested item, an item's
    /// later paragraph or its code is indented to the text of the item it
    /// is in, so that it reads back in that item.
    pub fn markdown(&self) -> String {
        let mut out = String::new();
        let count = self.blocks.len();
        // The items the block written last is in, outermost first.
        let mut items: Vec<OpenItem> = Vec::new();
        // The marks of the fence open, which the block closes with.
        let mut fence = String::new();
        // Whether the block written last was in a list: an item, or a
        // paragraph or code of one.
        let mut in_list = false;
        for (index, block) in self.blocks.iter().enumerate() {
            let range = self.block_range(index);
            let next = self.blocks.get(index + 1);
            let after_list = std::mem::replace(&mut in_list, false);
            // A code block indented as a list is a block of the item it
            // follows.
            let in_item = |b: &BlockFormat| b.kind == BlockKind::Code && b.indent > 0.0;
            let next_in_list = next.is_some_and(|b| b.kind == BlockKind::List || in_item(b));
            match block.kind {
                // Each line of a code block is a paragraph, and lines of one
                // fence run on under it: the editor keeps no edge between two
                // blocks of one fence. Lines of another fence start a block of
                // their own. A fence closing, or indented lines a fenced block
                // follows, has no empty line after it.
                // Qt 6.11: fenced blocks run on whatever their fences, under
                // the first one's (6.12 the same).
                BlockKind::Code => {
                    let run = code_run(block);
                    let indented = run == Some(CodeRun::Indented);
                    let joins_before = index > 0 && same_run(&self.blocks[index - 1], block);
                    let joins_after = next.is_some_and(|n| same_run(block, n));
                    let text = &self.text[range];
                    let column = if in_item(block) && !items.is_empty() {
                        close_items(&mut items, block.indent, false);
                        items.last().map_or(0, |i| i.column)
                    } else {
                        items.clear();
                        0
                    };
                    in_list = column > 0;
                    // A block of an item is apart from the item's text.
                    if column > 0 && !joins_before && !out.ends_with("\n\n") {
                        out.push('\n');
                    }
                    if indented {
                        // Indented lines right after a list would be a
                        // paragraph of its last item; an empty comment ends
                        // the list, as CommonMark has it, and reads as nothing.
                        // Qt 6.11: indented lines after a list item are
                        // written right after it, and read back they are a
                        // paragraph of the item (6.12 the same).
                        if column == 0 && !joins_before && after_list {
                            out.push_str("<!-- -->\n\n");
                        }
                        // Measured: an empty line of an indented block keeps
                        // its indent.
                        out.extend(std::iter::repeat_n(' ', column + 4));
                        out.push_str(text);
                        out.push('\n');
                    } else {
                        if !joins_before {
                            let (tildes, info) = match run {
                                Some(CodeRun::Fenced { tildes, info }) => (tildes, info),
                                _ => (false, ""),
                            };
                            // Longer than any run of its mark in the lines,
                            // so that none of them closes it.
                            // Qt 6.11: the fence is three marks whatever the
                            // lines hold, and a line of three closes it (6.12
                            // the same).
                            let mark = if tildes { '~' } else { '`' };
                            let marks = (index..count)
                                .take_while(|&j| same_run(block, &self.blocks[j]))
                                .map(|j| longest_run(&self.text[self.block_range(j)], mark))
                                .max()
                                .unwrap_or(0);
                            fence.clear();
                            fence.extend(std::iter::repeat_n(mark, (marks + 1).max(3)));
                            out.extend(std::iter::repeat_n(' ', column));
                            out.push_str(&fence);
                            out.push_str(info);
                            out.push('\n');
                        }
                        push_line(&mut out, column, text);
                        if !joins_after {
                            // Qt 6.11: the fence that closes an item's code
                            // is indented twice over, and in a nested item it
                            // reads back as a line of the code (6.12 the
                            // same).
                            out.extend(std::iter::repeat_n(' ', column));
                            out.push_str(&fence);
                            out.push('\n');
                        }
                    }
                    let leaves_list = column > 0 && !joins_after && !next_in_list;
                    let blank = match next {
                        None => true,
                        Some(n) => {
                            leaves_list || (!joins_after && indented && n.kind != BlockKind::Code)
                        }
                    };
                    if leaves_list {
                        items.clear();
                    }
                    if blank {
                        out.push('\n');
                    }
                    continue;
                }
                BlockKind::List => {
                    in_list = true;
                    // A paragraph of an item after its first has no marker.
                    // It stays in the item: indented to the item's text, an
                    // empty line on each side.
                    if block.marker.is_none() && !items.is_empty() {
                        close_items(&mut items, block.indent, false);
                        let column = items.last().map_or(0, |i| i.column);
                        if !out.ends_with("\n\n") {
                            out.push('\n');
                        }
                        out.extend(std::iter::repeat_n(' ', column));
                        self.write_markdown_inline(&mut out, range);
                        out.push_str("\n\n");
                        if !next_in_list {
                            items.clear();
                        }
                        continue;
                    }
                    // The item before at this depth, which an item of the
                    // same kind goes on counting from.
                    let before = close_items(&mut items, block.indent, true);
                    let ordered = block.ordered();
                    let number = match before {
                        Some(b) if b.ordered == ordered => b.number + 1,
                        _ => 1,
                    };
                    // A nested item starts at the text of the item it is in.
                    // Qt 6.11: a nested item is indented by its depth times
                    // the width of its own marker, and an item of one kind
                    // in an item of the other reads back at another depth
                    // (6.12 the same).
                    let start = items.last().map_or(0, |i| i.column);
                    out.extend(std::iter::repeat_n(' ', start));
                    let marker = out.len();
                    if ordered {
                        let _ = write!(out, "{number}.  ");
                    } else {
                        out.push_str("- ");
                    }
                    items.push(OpenItem {
                        indent: block.indent,
                        column: start + out.len() - marker,
                        ordered,
                        number,
                    });
                    self.write_markdown_inline(&mut out, range);
                    out.push('\n');
                    if !next_in_list {
                        items.clear();
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
                BlockKind::Rule => out.push_str("- - -"),
                _ => self.write_markdown_inline(&mut out, range),
            }
            // Any other block ends the list.
            items.clear();
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
}

/// An item of a list that the Markdown written so far is in.
struct OpenItem {
    /// The item's indent in the document.
    indent: f32,
    /// The column its text starts at, which what is in it is indented to.
    column: usize,
    ordered: bool,
    number: usize,
}

/// Close the items a block at `indent` is not in: those deeper than it,
/// and with `same` the one at its depth too, which is returned.
fn close_items(items: &mut Vec<OpenItem>, indent: f32, same: bool) -> Option<OpenItem> {
    // Indents are whole points; half of one is room for rounding.
    while let Some(top) = items.last() {
        let deeper = top.indent > indent + 0.5;
        let at = (top.indent - indent).abs() <= 0.5;
        if deeper {
            items.pop();
        } else if at && same {
            return items.pop();
        } else {
            break;
        }
    }
    None
}

/// Whether code block `b` runs on under code block `a`: the same run, at
/// the same indent, so in the same item or in none.
fn same_run(a: &BlockFormat, b: &BlockFormat) -> bool {
    code_run(a) == code_run(b) && (a.indent - b.indent).abs() <= 0.5
}

/// A line of fenced code, indented to `column` unless it is empty.
fn push_line(out: &mut String, column: usize, text: &str) {
    if !text.is_empty() {
        out.extend(std::iter::repeat_n(' ', column));
        out.push_str(text);
    }
    out.push('\n');
}

/// The most `mark`s in a row in `line`.
fn longest_run(line: &str, mark: char) -> usize {
    line.split(|c| c != mark)
        .map(|r| r.len() / mark.len_utf8())
        .max()
        .unwrap_or(0)
}

/// How a code paragraph's Markdown is written, which the lines written
/// the same way run on under.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CodeRun<'a> {
    Indented,
    Fenced { tildes: bool, info: &'a str },
}

/// The code run of a paragraph; `None` for one that is not code. A `pre`'s
/// lines are written as a fence of backticks with no info string, and run
/// on with that fence.
fn code_run(block: &BlockFormat) -> Option<CodeRun<'_>> {
    (block.kind == BlockKind::Code).then(|| match &block.fence {
        Some(Fence::Indented) => CodeRun::Indented,
        Some(Fence::Fenced { tildes, info }) => CodeRun::Fenced {
            tildes: *tildes,
            info,
        },
        None => CodeRun::Fenced {
            tildes: false,
            info: "",
        },
    })
}

/// The `white-space` a block's style says: `pre` where it does not wrap.
/// A `pre` says it either way, as one that does not say wraps where the
/// editor's mark is (see [`super::html`]); a paragraph that wraps need not,
/// as the head says `pre-wrap` for it.
/// Close the list open, `Some(ordered)` when one is, unless the item
/// `next` (`Some(ordered)` when it is one) is of it.
fn end_list(out: &mut String, list: &mut Option<bool>, next: Option<bool>) {
    if let Some(was) = *list
        && next != Some(was)
    {
        out.push_str(if was { "</ol>" } else { "</ul>" });
        *list = None;
    }
}

/// Open a list for an item, numbered when `ordered`, unless one is open.
fn start_list(out: &mut String, list: &mut Option<bool>, ordered: bool) {
    if list.is_some() {
        return;
    }
    let _ = write!(
        out,
        "<{} style=\"margin-top: 0px; margin-bottom: 0px; margin-left: 0px; \
         margin-right: 0px; -qt-list-indent: 1;\">",
        if ordered { "ol" } else { "ul" }
    );
    // Measured on Qt 6.12: every item is on a line of its own.
    out.push('\n');
    *list = Some(ordered);
}

fn white_space(no_wrap: bool, pre: bool) -> &'static str {
    match (no_wrap, pre) {
        (true, _) => " white-space:pre;",
        (false, true) => " white-space:pre-wrap;",
        (false, false) => "",
    }
}

/// The head of the document: what the text is, and the style sheet that
/// makes paragraphs keep their spaces and draws a task's box.
// Qt 6.11: an HTML 4.0 doctype, a qrichtext meta and this style sheet head
// every document, whatever it holds (6.12 the same).
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
        let rest = i.fallback_families.iter().flat_map(|r| r.iter());
        let mut families = String::new();
        css::write_families(
            &mut families,
            std::iter::once(family).chain(rest).map(|n| &**n),
        );
        let _ = write!(s, " font-family:{};", escape_attr(&families));
    } else if i.mono {
        s.push_str(" font-family:'monospace';");
    }
    if let Some(px) = i.pixel_size {
        if i.in_points {
            // Back from pixels without the error of the way there.
            let pt = (ljos_runtime::pixels_to_points(f64::from(px * i.scale)) * 1e4).round() / 1e4;
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

pub(super) fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
