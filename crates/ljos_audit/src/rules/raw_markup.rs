//! `text-raw-markup`: a text shows markup as characters: backticks or
//! `**` that plain text does not read as Markdown, or a tag (`<default>`,
//! `<task-result id="7">`) that plain text prints and rich text drops.
//!
//! The format the text is shown in decides what counts: plain text shows
//! every tag, and automatic or rich text shows those that are not HTML as
//! nothing, or as the characters they are made of. Markdown text is left
//! out; it reads all of it. A field one types into is not looked at.

use super::controls::shown_texts;
use crate::cx::Cx;

/// `Text.PlainText`, `Text.RichText`, `Text.AutoText`, `Text.MarkdownText`
/// and `Text.StyledText`, as Qt documents the enumeration.
const PLAIN: i64 = 0;
const MARKDOWN: i64 = 3;

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

/// The first tag in `text`: `<name …>`, `</name>` or `<name/>`, where it
/// starts a word.
fn first_tag(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    for (at, _) in text.match_indices('<') {
        let starts_word = at == 0 || !bytes[at - 1].is_ascii_alphanumeric();
        if !starts_word {
            continue;
        }
        let rest = &text[at + 1..];
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        let name_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..name_len];
        if name.is_empty() || !name.as_bytes()[0].is_ascii_alphabetic() {
            continue;
        }
        let after = &rest[name_len..];
        let closes = after.starts_with('>') || after.starts_with("/>");
        let attrs = after.starts_with(' ') && after.contains('>') && after.contains('=');
        if closes || attrs {
            return Some(name);
        }
    }
    None
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
    if let Some(name) = first_tag(text) {
        let html = HTML.contains(&name.to_ascii_lowercase().as_str());
        if format == PLAIN || !html {
            return Some(format!("the tag `<{name}>`"));
        }
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

    const AUTO: i64 = 2;
    const RICH: i64 = 1;

    #[test]
    fn markup_shown_as_characters_is_found() {
        assert!(shown_markup("<default>", AUTO).is_some());
        assert!(shown_markup("<task-result id=\"7\">Done</task-result>", 0).is_some());
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
        ] {
            assert_eq!(shown_markup(t, AUTO), None, "{t}");
            assert_eq!(shown_markup(t, 0), None, "{t}");
        }
    }
}
