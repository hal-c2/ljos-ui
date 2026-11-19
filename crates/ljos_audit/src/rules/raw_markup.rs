//! `text-raw-markup`: a text shows markup as characters: backticks or
//! `**` that plain text does not read as Markdown, or a tag that the
//! format it is shown in prints or drops.
//!
//! The format decides what counts, as Qt draws it. Rich text (and
//! automatic text whose first tag is one rich text knows) drops a tag it
//! does not know. Plain text (and automatic text that is not taken as
//! rich) prints every tag: one rich text knows (`<b>`) was meant to be
//! read, and one with attributes or a closing tag (`<task-result id="7">`)
//! is a program's, and so is one that is the whole text (a row reading
//! `<default>` shows the program's stand-in, not a value); a `<date>`
//! within a sentence is a placeholder in prose, shown as it was written. Markdown text is left out; it reads all of
//! it. A field one types into is not looked at.

use super::controls::shown_texts;
use crate::cx::Cx;

/// `Text.RichText`, `Text.AutoText`, `Text.MarkdownText`
/// and `Text.StyledText`, as Qt documents the enumeration.
const RICH: i64 = 1;
const AUTO: i64 = 2;
const MARKDOWN: i64 = 3;
const STYLED: i64 = 4;

/// The tags rich text knows, as Qt documents its supported HTML subset.
const HTML: &[&str] = &[
    "a",
    "address",
    "b",
    "big",
    "blockquote",
    "body",
    "br",
    "center",
    "cite",
    "code",
    "dd",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "font",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "hr",
    "html",
    "i",
    "img",
    "kbd",
    "li",
    "meta",
    "nobr",
    "ol",
    "p",
    "pre",
    "qt",
    "s",
    "samp",
    "small",
    "span",
    "strong",
    "style",
    "sub",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "tt",
    "u",
    "ul",
    "var",
];

/// A tag in a text: `<name …>`, `</name>` or `<name/>`, where it starts a
/// word.
struct Tag<'a> {
    name: &'a str,
    /// Its attributes, its closing tag, its own `/>`, or its being the
    /// whole text say it is markup, not a word in angle brackets.
    markup: bool,
}

impl Tag<'_> {
    fn html(&self) -> bool {
        HTML.contains(&self.name.to_ascii_lowercase().as_str())
    }
}

/// The tags in `text`, in order.
fn tags(text: &str) -> impl Iterator<Item = Tag<'_>> {
    let bytes = text.as_bytes();
    text.match_indices('<').filter_map(move |(at, _)| {
        let starts_word = at == 0 || !bytes[at - 1].is_ascii_alphanumeric();
        if !starts_word {
            return None;
        }
        let rest = &text[at + 1..];
        let (rest, closing) = match rest.strip_prefix('/') {
            Some(r) => (r, true),
            None => (rest, false),
        };
        let name_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..name_len];
        if name.is_empty() || !name.as_bytes()[0].is_ascii_alphabetic() {
            return None;
        }
        let after = &rest[name_len..];
        let selfclosing = after.starts_with("/>");
        let closes = after.starts_with('>') || selfclosing;
        let attrs = after.starts_with(' ') && after.contains('>') && after.contains('=');
        (closes || attrs).then(|| Tag {
            name,
            markup: closing
                || selfclosing
                || attrs
                || text.contains(&format!("</{name}>"))
                || text.trim() == format!("<{name}>"),
        })
    })
}

/// Inline code or strong emphasis written in Markdown: a pair of
/// backticks or of `**` around words.
fn markdown_mark(text: &str) -> Option<&'static str> {
    let paired = |mark: &str| {
        let mut parts = text.split(mark);
        parts.next();
        let inner = parts.next()?;
        // The mark closes, and holds a word that does not start or end
        // with a space (`a ** b` is arithmetic).
        parts.next()?;
        (!inner.is_empty() && !inner.starts_with(' ') && !inner.ends_with(' ')).then_some(())
    };
    if paired("`").is_some() {
        return Some("backticks");
    }
    if paired("**").is_some() {
        return Some("`**`");
    }
    None
}

/// What markup `text` shows as characters in `format`, if any.
pub(crate) fn shown_markup(text: &str, format: i64) -> Option<String> {
    if format == MARKDOWN {
        return None;
    }
    // Qt takes automatic text as rich when its first tag is one rich text
    // knows (qml6: "<b>a</b> <date>" drops `<date>`, "<date> <b>a</b>"
    // prints both).
    let rich = format == RICH
        || format == STYLED
        || (format == AUTO && tags(text).next().is_some_and(|t| t.html()));
    let shown = tags(text).find(|t| {
        if rich {
            !t.html()
        } else {
            t.html() || t.markup
        }
    });
    if let Some(tag) = shown {
        return Some(format!("the tag `<{}>`", tag.name));
    }
    markdown_mark(text).map(str::to_string)
}

pub(crate) fn check(cx: &mut Cx) {
    for (item, text) in shown_texts(cx) {
        let format = cx
            .engine
            .untrack(|| cx.engine.read_by_name(item, "textFormat"))
            .to_number() as i64;
        let Some(what) = shown_markup(&text, format) else {
            continue;
        };
        cx.report(
            "text-raw-markup",
            Some(item),
            format!(
                "the text {:?} shows {what} as characters (or drops it), which only the program \
                 reads; set `textFormat` to the markup it is written in, or take the markup out \
                 before showing it",
                text.trim()
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::shown_markup;

    use super::{AUTO, RICH};

    #[test]
    fn markup_shown_as_characters_is_found() {
        assert!(shown_markup("<default>", RICH).is_some());
        assert!(shown_markup("<default>", AUTO).is_some());
        assert!(shown_markup(" <default> ", 0).is_some());
        assert!(shown_markup("<b>Done</b> for <date>", AUTO).is_some());
        assert!(shown_markup("<task-result id=\"7\">Done</task-result>", 0).is_some());
        assert!(shown_markup("<task-result>Done</task-result>", AUTO).is_some());
        assert!(shown_markup("see <foo/> here", AUTO).is_some());
        // Automatic text that does not start with a tag rich text knows is
        // plain, so the `<b>` after a placeholder shows as characters.
        assert!(shown_markup("<date> then <b>bold</b>", AUTO).is_some());
        assert!(shown_markup("Run `make build` first", 0).is_some());
        assert!(shown_markup("before **continuing**", AUTO).is_some());
        assert!(shown_markup("<b>Bold</b>", 0).is_some());
    }

    #[test]
    fn what_is_read_or_is_no_markup_is_not() {
        assert_eq!(shown_markup("<b>Bold</b>", AUTO), None);
        assert_eq!(shown_markup("<p>Hi</p>", RICH), None);
        assert_eq!(shown_markup("Run `make build` first", 3), None);
        for t in [
            "a < b and c > d",
            "Vec<String>",
            "Mail <someone@example.org>",
            "2 ** 8 = 256",
            "It's a `single backtick",
            "<- back",
            "Fix works on a maintenance/<date> branch",
        ] {
            assert_eq!(shown_markup(t, AUTO), None, "{t}");
            assert_eq!(shown_markup(t, 0), None, "{t}");
        }
    }
}
