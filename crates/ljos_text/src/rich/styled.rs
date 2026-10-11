//! Qt's `StyledText`: a strict, small set of tags over one flowing text.
//!
//! The result is one block. Paragraphs, headings and list items are line
//! breaks in its text, and a list item's indent and bullet are characters,
//! which is why eliding and `maximumLineCount` work on styled text as they
//! do on plain text.
//!
//! Every rule here was measured on Qt 6.11 as a black box (widths, heights
//! and line counts of a `Text` against plain ones):
//!
//! * `b strong i u s del font a h1`..`h6 pre` open a style, and the end tag
//!   of any of them closes the style opened last, whatever its name;
//! * `p`, a heading, `pre` and `li` start on a new line unless the last
//!   thing was the end of one; their end tags (not `li`'s) break the line;
//! * `br` breaks the line; `ul` and `ol` indent their items by six spaces a
//!   level, and the outermost one breaks the line where it ends;
//! * a tag is known only as documented: `<b x="1">` is not `b`, attribute
//!   values need quotes, an unknown tag is dropped and its content kept (a
//!   comment is an unknown tag that ends at the first `>`);
//! * white space collapses to one space and is dropped at the start of a
//!   line; `pre` keeps it;
//! * `&name;` is a character; an unknown name is nothing; an `&` with a
//!   space before the next `;` is itself; an `&` or a `<` that never ends
//!   swallows the rest of the text;
//! * `img` stands in the line in a room of whole spaces of the text's font
//!   (the fewest that are wider than it), after a space of its own unless a
//!   space or another image is before it; a space written after it is
//!   dropped, and `width` and `height` count only when both are given.

use std::sync::Arc;

use super::entity;
use super::{Block, BlockKind, Inline, InlineBuf, InlineImage};
use crate::style::BoxAlign;

/// Font sizes of `<font size="1">` to `"7"`, as multiples of the text's.
pub(crate) const FONT_SCALE: [f32; 7] = [0.7, 0.8, 1.0, 1.2, 1.5, 2.0, 2.4];
/// Font sizes of `<h1>` to `<h6>`.
pub(crate) const HEADING_SCALE: [f32; 6] = [2.0, 1.5, 1.2, 1.0, 0.8, 0.7];

/// Lists deeper than this are not indented further: the text of a document
/// cannot grow faster than its source.
const MAX_LIST_DEPTH: usize = 16;
/// Styles nested deeper than this are not remembered. Every open tag would
/// otherwise keep a copy of the style alive.
const MAX_STYLE_DEPTH: usize = 256;

struct Frame {
    style: Inline,
    /// White space is kept.
    pre: bool,
}

struct List {
    marker: Marker,
    count: u32,
}

#[derive(Clone, Copy)]
pub(crate) enum Marker {
    Disc,
    Circle,
    Square,
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

struct Parser<'a> {
    src: &'a str,
    at: usize,
    out: InlineBuf,
    frames: Vec<Frame>,
    lists: Vec<List>,
    /// The last thing was the end of a paragraph, heading or `pre`: the
    /// next one starts where it stands.
    fresh: bool,
}

pub(crate) fn parse(source: &str) -> Vec<Block> {
    // White space the source ends in is not text (measured: ` abc ` is as
    // wide as `abc`, while the space of `a < b` counts).
    let source = source.trim_end_matches(|c: char| c.is_ascii_whitespace());
    let mut p = Parser {
        src: source,
        at: 0,
        out: InlineBuf::default(),
        frames: vec![Frame {
            style: Inline::default(),
            pre: false,
        }],
        lists: Vec::new(),
        fresh: true,
    };
    p.run();
    vec![p.out.into_block(Block::new(BlockKind::Paragraph))]
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
                if !self.tag() {
                    break;
                }
            } else if !self.reference() {
                break;
            }
        }
    }

    fn style(&self) -> &Inline {
        // The stack never loses its first frame: `pop` leaves it.
        &self.frames[self.frames.len() - 1].style
    }

    fn pre(&self) -> bool {
        self.frames.last().is_some_and(|f| f.pre)
    }

    fn text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        self.fresh = false;
        let style = self.style().clone();
        if self.pre() {
            self.out.push(s, &style);
            return;
        }
        let mut word = 0;
        for (i, c) in s.char_indices() {
            if !c.is_ascii_whitespace() {
                continue;
            }
            self.out.push(&s[word..i], &style);
            word = i + 1;
            if !self.out.text.is_empty()
                && !self.out.text.ends_with([' ', '\n'])
                && !self.after_image()
            {
                self.out.push(" ", &style);
            }
        }
        self.out.push(&s[word..], &style);
    }

    /// A line break in `style`. A space a line ends in does not count as
    /// part of it.
    fn newline(&mut self, style: &Inline) {
        // The space before an image is the image's, not the line's end.
        if !self.pre() && self.out.text.ends_with(' ') && !self.after_image() {
            self.out.truncate(self.out.text.len() - 1);
        }
        self.out.push("\n", style);
    }

    fn push(&mut self, change: impl FnOnce(&mut Inline), pre: bool) {
        if self.frames.len() >= MAX_STYLE_DEPTH {
            return;
        }
        let mut style = self.style().clone();
        change(&mut style);
        let pre = pre || self.pre();
        self.frames.push(Frame { style, pre });
    }

    fn pop(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    /// At an `&`. False when it swallows the rest of the text.
    fn reference(&mut self) -> bool {
        let rest = &self.src[self.at + 1..];
        let Some(end) = rest.find([';', ' ']) else {
            return false;
        };
        if rest.as_bytes()[end] == b' ' {
            self.text("&");
            self.at += 1;
            return true;
        }
        if let Some(c) = entity::decode(&rest[..end]) {
            let style = self.style().clone();
            self.fresh = false;
            self.out.push(c.encode_utf8(&mut [0; 4]), &style);
        }
        self.at += end + 2;
        true
    }

    /// At a `<`. False when it swallows the rest of the text.
    fn tag(&mut self) -> bool {
        let rest = &self.src[self.at + 1..];
        let Some(end) = rest.find('>') else {
            return false;
        };
        let inner = &rest[..end];
        self.at += end + 2;
        match inner.strip_prefix('/') {
            Some(name) => self.end_tag(name.trim()),
            None => self.start_tag(inner.trim_start()),
        }
        true
    }

    fn start_tag(&mut self, inner: &str) {
        let name_len = inner
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(inner.len());
        let (name, attrs) = inner.split_at(name_len);
        let name = name.to_ascii_lowercase();
        // `<b/>` and `<b >` are `<b>`; anything else after the name makes a
        // tag that takes no attributes another, unknown one.
        let bare = attrs.trim().trim_end_matches('/').trim().is_empty();
        let attrs = Attrs(attrs);
        match name.as_str() {
            "b" | "strong" if bare => self.push(|s| s.bold = true, false),
            "i" if bare => self.push(|s| s.italic = true, false),
            "u" if bare => self.push(|s| s.underline = true, false),
            "s" | "del" if bare => self.push(|s| s.strike = true, false),
            "br" if bare => {
                let style = self.style().clone();
                self.newline(&style);
            }
            "p" if bare => self.break_before(),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" if bare => {
                let level = usize::from(name.as_bytes()[1] - b'1');
                self.push(
                    |s| {
                        s.bold = true;
                        s.scale = HEADING_SCALE[level];
                    },
                    false,
                );
                self.break_before();
            }
            "pre" if bare => {
                self.push(|s| s.mono = true, true);
                self.break_before();
            }
            "font" => {
                let size = attrs.get("size").and_then(font_size);
                let color = attrs.get("color").filter(|c| !c.is_empty());
                self.push(
                    |s| {
                        if let Some(scale) = size {
                            s.scale = scale;
                        }
                        if let Some(color) = color {
                            s.color = Some(Arc::from(color));
                        }
                    },
                    false,
                );
            }
            "a" => {
                let href = attrs.get("href");
                self.push(|s| s.link = href.map(Arc::from), false);
            }
            "img" => self.image(&attrs),
            "ul" | "ol" => self.open_list(name == "ol", &attrs),
            "li" if bare => self.item(),
            _ => {}
        }
    }

    fn end_tag(&mut self, name: &str) {
        match name.to_ascii_lowercase().as_str() {
            "b" | "strong" | "i" | "u" | "s" | "del" | "font" | "a" => self.pop(),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "pre" => {
                self.pop();
                self.break_after();
            }
            "p" => self.break_after(),
            "ul" | "ol" if self.lists.pop().is_some() && self.lists.is_empty() => {
                let style = self.style().clone();
                self.newline(&style);
            }
            _ => {}
        }
    }

    /// What a paragraph does where it starts. The break is already in the
    /// paragraph's style, so a heading's makes the line before it tall.
    fn break_before(&mut self) {
        if !self.fresh {
            let style = self.style().clone();
            self.newline(&style);
        }
    }

    fn break_after(&mut self) {
        let style = self.style().clone();
        self.newline(&style);
        self.fresh = true;
    }

    fn open_list(&mut self, ordered: bool, attrs: &Attrs<'_>) {
        let marker = match (ordered, attrs.get("type")) {
            (false, Some("disc")) => Marker::Disc,
            (false, Some("circle")) => Marker::Circle,
            (false, Some("square")) => Marker::Square,
            (false, _) => match self.lists.len() % 3 {
                0 => Marker::Disc,
                1 => Marker::Circle,
                _ => Marker::Square,
            },
            (true, Some("a")) => Marker::LowerAlpha,
            (true, Some("A")) => Marker::UpperAlpha,
            (true, Some("i")) => Marker::LowerRoman,
            (true, Some("I")) => Marker::UpperRoman,
            (true, _) => Marker::Decimal,
        };
        self.lists.push(List { marker, count: 0 });
    }

    fn item(&mut self) {
        self.break_before();
        let depth = self.lists.len().min(MAX_LIST_DEPTH);
        let Some(list) = self.lists.last_mut() else {
            return;
        };
        list.count = list.count.saturating_add(1);
        let marker = marker_text(list.marker, list.count);
        let style = self.style().clone();
        let mut prefix = " ".repeat(6 * depth);
        prefix.push_str(&marker);
        prefix.push_str("  ");
        self.out.push(&prefix, &style);
        self.fresh = false;
    }

    fn image(&mut self, attrs: &Attrs<'_>) {
        let Some(source) = attrs.get("src") else {
            return;
        };
        let size = |name| {
            attrs
                .get(name)
                .and_then(|v| v.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
        };
        // Measured on Qt 6.11.2: an image stands a space away from a letter
        // before it (and from the start of the text), and not from a space
        // or another image.
        if !self.after_image() && !self.out.text.ends_with(' ') {
            let style = self.style().clone();
            self.out.push(" ", &style);
        }
        self.out.images.push(InlineImage {
            index: self.out.text.len(),
            source: Arc::from(source),
            alt: Arc::from(attrs.get("alt").unwrap_or_default()),
            width: size("width"),
            height: size("height"),
            align: match attrs.get("align") {
                Some("top") => BoxAlign::Top,
                Some("middle") => BoxAlign::Middle,
                _ => BoxAlign::Bottom,
            },
            in_spaces: true,
        });
        self.fresh = false;
    }

    /// Nothing was written since the last image. The room of an image is
    /// spaces, so a space after it collapses as one after a space does.
    fn after_image(&self) -> bool {
        self.out
            .images
            .last()
            .is_some_and(|image| image.index == self.out.text.len())
    }
}

/// `size="4"`, `"+1"` (one more than the normal 3) or `"-1"`.
fn font_size(value: &str) -> Option<f32> {
    let value = value.trim();
    let size = if value.starts_with(['+', '-']) {
        value.parse::<i32>().ok()?.checked_add(3)?
    } else {
        value.parse::<i32>().ok()?
    };
    let index = usize::try_from(size.checked_sub(1)?).ok()?;
    FONT_SCALE.get(index).copied()
}

pub(crate) fn marker_text(marker: Marker, count: u32) -> String {
    match marker {
        Marker::Disc => "\u{2022}".to_owned(),
        Marker::Circle => "\u{25E6}".to_owned(),
        Marker::Square => "\u{25A0}".to_owned(),
        Marker::Decimal => format!("{count}."),
        Marker::LowerAlpha => format!("{}.", alpha(count, b'a')),
        Marker::UpperAlpha => format!("{}.", alpha(count, b'A')),
        Marker::LowerRoman => format!("{}.", roman(count).to_ascii_lowercase()),
        Marker::UpperRoman => format!("{}.", roman(count)),
    }
}

/// 1 is a, 26 is z, 27 is aa.
fn alpha(mut n: u32, first: u8) -> String {
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(first + (n % 26) as u8);
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn roman(n: u32) -> String {
    // Beyond what the numerals cover, the number is itself.
    if n == 0 || n > 3999 {
        return n.to_string();
    }
    const PARTS: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut n = n;
    let mut out = String::new();
    for (value, numeral) in PARTS {
        while n >= value {
            out.push_str(numeral);
            n -= value;
        }
    }
    out
}

/// The attributes of a tag: `name="value"` or `name='value'`.
struct Attrs<'a>(&'a str);

impl<'a> Attrs<'a> {
    /// The quoted value of `name`. An attribute without quotes is not one.
    fn get(&self, name: &str) -> Option<&'a str> {
        let mut rest = self.0;
        loop {
            rest = rest.trim_start();
            let eq = rest.find('=')?;
            let key = rest[..eq].trim();
            let after = rest[eq + 1..].trim_start();
            let quote = after.chars().next().filter(|c| matches!(c, '"' | '\''))?;
            let value_end = after[1..].find(quote)?;
            let value = &after[1..1 + value_end];
            if key.eq_ignore_ascii_case(name) {
                return Some(value);
            }
            rest = &after[value_end + 2..];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(source: &str) -> String {
        parse(source).remove(0).text
    }

    fn block(source: &str) -> Block {
        parse(source).remove(0)
    }

    /// The text each source becomes; every pair was a measurement on Qt
    /// 6.11.2 (line counts and widths of a `Text`).
    #[test]
    fn the_text_is_what_qt_makes_of_the_tags() {
        let b = "      \u{2022}  ";
        let c = "            \u{25E6}  ";
        let cases: &[(&str, &str)] = &[
            ("abc", "abc"),
            ("<p>abc</p>", "abc\n"),
            ("<p>abc</p><p>abc</p>", "abc\nabc\n"),
            ("abc<p>abc</p>", "abc\nabc\n"),
            ("<p>abc</p>abc", "abc\nabc"),
            ("<p>abc<p>abc", "abc\nabc"),
            ("<p></p>", "\n"),
            ("<p>abc</p> <p>abc</p>", "abc\n\nabc\n"),
            ("abc<p>", "abc\n"),
            ("abc<p><p>abc", "abc\n\nabc"),
            ("abc</p>abc", "abc\nabc"),
            ("</p>abc", "\nabc"),
            ("<h1>abc</h1>", "abc\n"),
            ("<h1>abc</h1>abc", "abc\nabc"),
            ("abc<h1>abc</h1>abc", "abc\nabc\nabc"),
            ("<h1>abc</h1> abc", "abc\nabc"),
            ("<h1>abc</h1>\nabc", "abc\nabc"),
            ("abc<br>", "abc\n"),
            ("<br>", "\n"),
            ("<br/>", "\n"),
            ("<br />", "\n"),
            ("<br><br>", "\n\n"),
            ("abc<br><p>abc</p>", "abc\n\nabc\n"),
            ("abc <br> abc", "abc\nabc"),
            ("abc\n<br>\nabc", "abc\nabc"),
            ("a\nb", "a b"),
            ("a\n\nb", "a b"),
            ("  abc", "abc"),
            ("a  \t b", "a b"),
            ("a <b> b</b>", "a b"),
            ("<p> abc</p>", "abc\n"),
            (" <p>abc</p>", "\nabc\n"),
            ("abc<li>abc", "abc\nabc"),
            ("<ul>abc</ul>", "abc\n"),
            ("<ul></ul>", "\n"),
            ("<b>x</ul>y", "xy"),
            ("<b>x</li>y", "xy"),
            ("<b>x</p>y", "x\ny"),
            ("<pre>a  b</pre>", "a  b\n"),
            ("<pre>a\nb</pre>", "a\nb\n"),
            ("<pre> a</pre>", " a\n"),
            ("abc<pre>abc</pre>abc", "abc\nabc\nabc"),
            ("<foo>abc</foo>", "abc"),
            ("<em>abc</em>", "abc"),
            ("<code>abc</code>", "abc"),
            ("a<!-- x > y -->b", "a y -->b"),
            ("a<!x>b", "ab"),
            ("<b", ""),
            ("abc<", "abc"),
            ("a < b", "a "),
            ("a > b", "a > b"),
            ("<>", ""),
            ("&lt;&gt;&amp;&quot;&apos;", "<>&\"'"),
            ("a&nbsp;b", "a\u{A0}b"),
            ("&#65;&#x41;", "AA"),
            ("&copy;&LT;", "\u{A9}<"),
            ("a&foo;b", "ab"),
            ("a &b c", "a &b c"),
            ("a&b", "a"),
            ("&amp", ""),
            ("a&b<b>c</b>", "a"),
            ("a&b <b>c</b>", "a&b c"),
            ("x &amp y", "x &amp y"),
        ];
        for (source, expected) in cases {
            assert_eq!(text(source), *expected, "{source:?}");
        }
        assert_eq!(text("<ul><li>abc</li></ul>"), format!("{b}abc\n"));
        assert_eq!(
            text("<ul><li>abc</li><li>abc</li></ul>"),
            format!("{b}abc\n{b}abc\n")
        );
        assert_eq!(
            text("<ul><li>abc</li></ul><p>abc</p>"),
            format!("{b}abc\n\nabc\n")
        );
        assert_eq!(
            text("<p>abc</p><ul><li>abc</li></ul>"),
            format!("abc\n{b}abc\n")
        );
        assert_eq!(
            text("<ul><li>abc<ul><li>abc</li></ul></li></ul>"),
            format!("{b}abc\n{c}abc\n")
        );
        assert_eq!(
            text("<ul>\n<li>abc</li>\n<li>abc</li>\n</ul>"),
            format!("\n{b}abc\n{b}abc\n")
        );
        assert_eq!(text("<ul><li>abc</ul>abc"), format!("{b}abc\nabc"));
        assert_eq!(
            text("<ol><li>a<li>b<li>c</ol>"),
            "      1.  a\n      2.  b\n      3.  c\n"
        );
        assert_eq!(text("<ol type=\"a\"><li>x</ol>"), "      a.  x\n");
        assert_eq!(text("<ol type=\"A\"><li>x</ol>"), "      A.  x\n");
        assert_eq!(text("<ol type=\"i\"><li>x</ol>"), "      i.  x\n");
        assert_eq!(
            text("<ul type=\"square\"><li>x</ul>"),
            "      \u{25A0}  x\n"
        );
    }

    fn styles(source: &str) -> Vec<(String, Inline)> {
        let b = block(source);
        b.spans
            .iter()
            .map(|s| (b.text[s.range.clone()].to_owned(), s.style.clone()))
            .collect()
    }

    #[test]
    fn tags_set_the_style_of_their_text() {
        let s = styles("a<b>b</b><i>c</i><u>d</u><s>e</s><del>f</del><strong>g</strong>");
        // `s` and `del` set the same style, so their text is one span.
        assert_eq!(s.len(), 5);
        assert!(s[0].1.bold && s[0].0 == "b");
        assert!(s[1].1.italic && !s[1].1.bold);
        assert!(s[2].1.underline);
        assert!(s[3].1.strike && s[3].0 == "ef");
        assert!(s[4].1.bold && s[4].0 == "g");
    }

    #[test]
    fn an_end_tag_closes_the_style_opened_last_whatever_its_name() {
        // Measured: `<b>a<i>b</b>c</i>d` is bold, bold italic, bold, plain.
        let s = styles("<b>a<i>b</b>c</i>d");
        assert_eq!(s.len(), 3);
        assert!(s[0].1.bold && !s[0].1.italic && s[0].0 == "a");
        assert!(s[1].1.bold && s[1].1.italic && s[1].0 == "b");
        assert!(s[2].1.bold && !s[2].1.italic && s[2].0 == "c");
        // `</b>` ends a heading's style without breaking the line.
        assert_eq!(text("<h1>abc</b>abc"), "abcabc");
        assert_eq!(styles("<h1>abc</b>abc").len(), 1);
        // And a heading's end tag ends bold, and breaks the line.
        assert_eq!(text("<b>abc</h1>abc"), "abc\nabc");
    }

    #[test]
    fn a_tag_with_what_it_does_not_take_is_unknown() {
        assert!(styles("<b x=\"y\">abc</b>").is_empty());
        assert!(styles("<h1 align=\"center\">abc</h1>").is_empty());
        assert_eq!(styles("< b>abc</b>").len(), 1);
        assert_eq!(styles("<b >abc</b>").len(), 1);
        assert_eq!(styles("<b/>abc").len(), 1);
        assert_eq!(styles("<B>abc</B>").len(), 1);
        // `</b x>` and `< /b>` end nothing.
        assert_eq!(styles("<b>abc</b x>abc")[0].0, "abcabc");
        assert_eq!(styles("<b>abc< /b>abc")[0].0, "abcabc");
        assert_eq!(styles("<b>abc</ b>abc")[0].0, "abc");
    }

    #[test]
    fn headings_are_bold_and_scaled_and_their_break_is_in_the_outer_style() {
        let b = block("<h1>abc</h1>");
        assert_eq!(b.spans.len(), 1);
        assert_eq!(b.spans[0].range, 0..3);
        assert!(b.spans[0].style.bold);
        assert_eq!(b.spans[0].style.scale, 2.0);
        // The break before a heading is the heading's.
        let b = block("abc<h2>abc</h2>abc");
        assert_eq!(b.spans[0].range, 3..7);
        assert_eq!(b.spans[0].style.scale, 1.5);
    }

    #[test]
    fn font_sizes_are_the_seven_steps_and_nest() {
        for (size, scale) in [("1", 0.7), ("3", 1.0), ("7", 2.4), ("+1", 1.2), ("-1", 0.8)] {
            let s = styles(&format!("<font size=\"{size}\">abc</font>"));
            let got = s.first().map_or(1.0, |s| s.1.scale);
            assert_eq!(got, scale, "size {size}");
        }
        for ignored in ["0", "8", "x", ""] {
            assert!(styles(&format!("<font size=\"{ignored}\">abc</font>")).is_empty());
        }
        // Without quotes the attribute is not read; single quotes do.
        assert!(styles("<font size=5>abc</font>").is_empty());
        assert_eq!(styles("<font size='5'>abc</font>")[0].1.scale, 1.5);
        let s = styles("<font size=\"5\">a<font color=\"red\">b</font></font>");
        assert_eq!(s[1].1.scale, 1.5);
        assert_eq!(s[1].1.color.as_deref(), Some("red"));
        // A size inside a heading is its own, the weight stays.
        let s = styles("<h1>a<font size=\"3\">b</font></h1>");
        assert!(s[1].1.bold);
        assert_eq!(s[1].1.scale, 1.0);
    }

    #[test]
    fn links_and_images_are_recorded_and_nothing_else_of_them() {
        let b = block(
            "see <a href=\"http://x/\">this</a> <img src=\"a.png\" width=\"30\" height=\"40\" align=\"middle\">",
        );
        assert_eq!(b.text, "see this ");
        assert_eq!(b.spans[0].style.link.as_deref(), Some("http://x/"));
        assert_eq!(b.images.len(), 1);
        let image = &b.images[0];
        assert_eq!(&*image.source, "a.png");
        assert_eq!((image.width, image.height), (Some(30.0), Some(40.0)));
        assert_eq!(image.align, BoxAlign::Middle);
        assert_eq!(image.index, b.text.len());
    }

    #[test]
    fn pre_is_fixed_width_and_keeps_its_spaces() {
        let b = block("<pre>a  b</pre>");
        assert!(b.spans[0].style.mono);
        assert_eq!(b.spans[0].range, 0..4);
    }

    #[test]
    fn numbering() {
        assert_eq!(alpha(1, b'a'), "a");
        assert_eq!(alpha(26, b'a'), "z");
        assert_eq!(alpha(27, b'a'), "aa");
        assert_eq!(roman(4), "IV");
        assert_eq!(roman(1994), "MCMXCIV");
        assert_eq!(roman(4000), "4000");
    }

    #[test]
    fn nesting_is_bounded() {
        let deep = "<ul>".repeat(1000) + "<li>x";
        let t = text(&deep);
        assert!(t.len() < 200, "{}", t.len());
        let styled = "<b>".repeat(100_000) + "x";
        assert_eq!(text(&styled), "x");
    }
}
