//! What `AutoText` takes for markup.
//!
//! The rule is Qt 6.11's, measured on `Text` as a black box (a `Text` with
//! `AutoText` and one with `PlainText` side by side, compared by width):
//!
//! * white space at the start is skipped, then only the first line counts;
//! * on that line the first `<` decides, unless the text `&lt;` comes before
//!   it, which makes the text markup on its own;
//! * the `<` makes it markup when a `>` follows somewhere (also on a later
//!   line) and what stands between them is a tag Qt knows: optional white
//!   space, an optional `!`, a name of letters and digits, and then either
//!   the `>`, white space (whatever follows it), or a `/` right before the
//!   `>`.
//!
//! So `a < b` is plain, `a < b > c` is markup (`b` is a tag), `</b>` and
//! `<!-- x -->` are plain, and Markdown is never guessed.

/// The tags that make a text markup. Measured one by one; `em`, `strong`
/// and the like are here although `StyledText` then ignores some of them.
const TAGS: &[&str] = &[
    "a",
    "address",
    "b",
    "big",
    "blockquote",
    "body",
    "br",
    "caption",
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
    "link",
    "meta",
    "nobr",
    "ol",
    "p",
    "pre",
    "qt",
    "s",
    "samp",
    "script",
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

/// Whether `AutoText` reads `text` as markup.
pub fn might_be_rich(text: &str) -> bool {
    let text = text.trim_start();
    let line_end = text.find('\n').unwrap_or(text.len());
    let line = &text[..line_end];
    let mut at = 0;
    while let Some(i) = line[at..].find(['<', '&']) {
        let i = at + i;
        if line.as_bytes()[i] == b'<' {
            // The tag may close on a later line, so look in the whole text.
            return is_known_tag(&text[i + 1..]);
        }
        if line[i..].starts_with("&lt;") {
            return true;
        }
        at = i + 1;
    }
    false
}

/// `rest` is what follows a `<`.
fn is_known_tag(rest: &str) -> bool {
    let Some(close) = rest.find('>') else {
        return false;
    };
    let inner = rest[..close].trim_start();
    let (inner, bang) = match inner.strip_prefix('!') {
        Some(after) => (after, true),
        None => (inner, false),
    };
    let name_len = inner
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(inner.len());
    let (name, after) = inner.split_at(name_len);
    let ends_well = after.is_empty() || after == "/" || after.starts_with(char::is_whitespace);
    if !ends_well {
        return false;
    }
    let name = name.to_ascii_lowercase();
    TAGS.binary_search(&name.as_str()).is_ok() || (bang && name == "doctype")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_sorted_for_the_search() {
        assert!(TAGS.windows(2).all(|w| w[0] < w[1]));
    }

    /// Every line is one measurement on Qt 6.11.2.
    #[test]
    fn the_rule_is_the_one_measured_on_qt() {
        let rich = [
            "<b>a</b>",
            "a &lt; b",
            "< b>",
            "<b/>",
            "<b / >",
            "<!b>",
            "<a /x>",
            "<b>>",
            "a < b > c",
            "a < b <b>x</b>",
            "&lt; <foo>",
            "&&lt;",
            "x<b\ny>z",
            "<b\t/>",
            "\n\nx<b>",
            "<!DOCTYPE html>",
            "<B>a</B>",
            "  <i>a</i>",
            "<font color=\"red\">a</font>",
            "<br>",
            "<h1>a</h1>",
        ];
        for text in rich {
            assert!(might_be_rich(text), "{text:?} is markup");
        }
        let plain = [
            "",
            "abc",
            "a < b",
            "<foo>",
            "x\n<b>y</b>",
            "&amp;",
            "&lt",
            "&LT;",
            "<b//>",
            "<b/ />",
            "</b>",
            "<!-- c -->",
            "<a.b>",
            "<a-b>",
            "<a1>",
            "<bb>",
            "<a/b>",
            "<<b>",
            "<b<b>",
            "< >",
            "<b x",
            "a <x> <b>",
            "<foo> &lt;",
            "<b/\n>",
            "<b=1>",
            "<?b>",
            "< / b>",
            "**a**",
            "# a",
            "<",
            "&",
        ];
        for text in plain {
            assert!(!might_be_rich(text), "{text:?} is plain");
        }
    }
}
