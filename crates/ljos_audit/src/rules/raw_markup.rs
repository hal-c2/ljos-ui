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
    "del",
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

/// A tag in a text: `<name …>`, `<name/>` or `</name>`.
struct Tag<'a> {
    name: &'a str,
    closing: bool,
    /// Its own `/>`, its attributes, or its being the whole text say it is
    /// markup, not a word in angle brackets.
    marked: bool,
}

impl Tag<'_> {
    fn html(&self) -> bool {
        HTML.contains(&self.name.to_ascii_lowercase().as_str())
    }
}

/// Where the `>` that ends a tag is in `after` (what follows its name),
/// passing over quoted attribute values, so a `>` or a `<` in
/// `title="<date>"` is the value's.
fn tag_end(after: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in after.char_indices() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i),
            (None, '<') => return None,
            _ => {}
        }
    }
    None
}

/// The tags in `text`, in order. Scanning resumes after each tag's `>`,
/// so what is inside its attributes is never taken for a tag. In `prose`
/// an opening tag must start a word: `Vec<String>` in plain text is
/// words, while rich text parses `e<date>` as a tag all the same, and
/// passes over comments.
fn tags(text: &str, prose: bool) -> impl Iterator<Item = Tag<'_>> {
    let bytes = text.as_bytes();
    let mut from = 0;
    std::iter::from_fn(move || {
        while let Some(found) = text[from..].find('<') {
            let at = from + found;
            from = at + 1;
            let rest = &text[at + 1..];
            if !prose && let Some(comment) = rest.strip_prefix("!--") {
                from = match comment.find("-->") {
                    Some(close) => text.len() - comment.len() + close + 3,
                    None => text.len(),
                };
                continue;
            }
            let (rest, closing) = match rest.strip_prefix('/') {
                Some(r) => (r, true),
                None => (rest, false),
            };
            // A closing tag ends the word before it (`Done</b>`); an
            // opening one starts a word (`Vec<String>` is no tag).
            let starts_word = at == 0 || !bytes[at - 1].is_ascii_alphanumeric();
            if prose && !starts_word && !closing {
                continue;
            }
            let name_len = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                .unwrap_or(rest.len());
            let name = &rest[..name_len];
            if name.is_empty() || !name.as_bytes()[0].is_ascii_alphabetic() {
                continue;
            }
            let after = &rest[name_len..];
            // Any whitespace may part the name from attributes or `/>`
            // (`<foo />`, `<foo\tid="7">`).
            let spaced = after.starts_with(|c: char| c.is_ascii_whitespace());
            let suffix = after.trim_start_matches(|c: char| c.is_ascii_whitespace());
            let selfclosing = suffix.starts_with("/>");
            let closes = suffix.starts_with('>') || selfclosing;
            let end = tag_end(after);
            let valued = spaced && end.is_some_and(|end| after[..end].contains('='));
            // `<input disabled>`: an attribute of no value.
            let valueless =
                spaced && end.is_some() && suffix.starts_with(|c: char| c.is_ascii_alphabetic());
            // A tag with no `>` (`<foo` at the end) is no tag.
            let Some(stop) = end.map(|end| text.len() - after.len() + end + 1) else {
                continue;
            };
            let whole = text[..at].trim().is_empty() && text[stop..].trim().is_empty();
            // Prose keeps `<press any key>` amid words as words; on its own,
            // or in rich text, it is a tag.
            if !(closes || valued || (valueless && (whole || !prose))) {
                continue;
            }
            from = stop;
            return Some(Tag {
                name,
                closing,
                marked: selfclosing || valued || valueless || whole,
            });
        }
        None
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
    // Qt takes automatic text as rich when the first tag on its first
    // line is one rich text knows (qml6: "<b>a</b> <date>" drops `<date>`,
    // "<date> <b>a</b>" prints both, and so does "Summary\n<b>Done</b>").
    let first_line = text.split('\n').next().unwrap_or("");
    let rich = format == RICH
        || format == STYLED
        || (format == AUTO && tags(first_line, false).next().is_some_and(|t| t.html()));
    let shown = if rich {
        tags(text, false).find(|t| !t.html())
    } else {
        // A tag closed somewhere in the text is markup; the closed names
        // are gathered once, and only plain text asks.
        let closed: std::collections::HashSet<&str> = tags(text, true)
            .filter(|t| t.closing)
            .map(|t| t.name)
            .collect();
        tags(text, true).find(|t| t.html() || t.closing || t.marked || closed.contains(&t.name))
    };
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
        // Only the first line decides whether automatic text is rich.
        assert!(shown_markup("Summary\n<b>Done</b>", AUTO).is_some());
        // A tag after one with a quoted attribute is still seen.
        assert!(shown_markup("<a title=\"x > y\" href=\"h\">Help</a> <date>", RICH).is_some());
        assert!(shown_markup("<b>Current</b><date>Previous</date>", RICH).is_some());
        // Rich text parses a tag right after a letter; only prose spares it.
        assert!(shown_markup("<b>Value<date></b>", RICH).is_some());
        // Whitespace of any kind parts a name from `/>` or attributes.
        assert!(shown_markup("see <foo /> here", 0).is_some());
        assert!(shown_markup("<foo\tid=\"7\">", 0).is_some());
        assert!(shown_markup("<foo\n/> here", AUTO).is_some());
        // An attribute of no value, the whole text, or in rich text.
        assert!(shown_markup("<input disabled>", 0).is_some());
        assert!(shown_markup("<b>Go</b> <foo bar>", RICH).is_some());
        // A tag after a comment is still seen.
        assert!(shown_markup("<b>Ready</b><!-- note --><date>", RICH).is_some());
    }

    #[test]
    fn tags_inside_attribute_values_are_the_values() {
        assert_eq!(
            shown_markup("<a title=\"<date>\" href=\"help.html\">Help</a>", RICH),
            None
        );
        assert_eq!(
            shown_markup("<a title='<date>' href='help.html'>Help</a>", AUTO),
            None
        );
    }

    #[test]
    fn comments_in_rich_text_are_no_tags() {
        assert_eq!(shown_markup("<b>Ready</b><!-- <date> -->", RICH), None);
        assert_eq!(shown_markup("<b>Ready</b><!-- <date>", RICH), None);
        // Words in angle brackets amid prose stay words.
        assert_eq!(shown_markup("Then <press any key> to go on", 0), None);
    }

    #[test]
    fn an_unclosed_tag_is_no_tag() {
        assert_eq!(shown_markup("<foo", 0), None);
        assert_eq!(shown_markup("a <foo bar", 0), None);
        assert_eq!(shown_markup("<b>x</b> <foo", RICH), None);
        assert_eq!(shown_markup("<é", 0), None);
    }

    #[test]
    fn del_is_rich_text() {
        assert_eq!(
            shown_markup("<b>Current</b><del>Previous</del>", RICH),
            None
        );
        assert_eq!(shown_markup("<del>Previous</del>", AUTO), None);
    }

    #[test]
    fn many_tags_are_scanned_once() {
        // Rich text with many tags does not search the whole text per tag.
        let text = "<br>".repeat(20_000);
        let start = std::time::Instant::now();
        assert_eq!(shown_markup(&text, RICH), None);
        let plain = format!("{}</zz>", " <date>".repeat(20_000));
        assert!(shown_markup(&plain, 0).is_some());
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
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
