//! `content-raw-identifier`: a text shows what the program calls a thing,
//! not what a person does: a UUID, an absolute path, a hash or an
//! address, an object printed as it is in memory, or a request's URL.
//!
//! Each is told by its shape alone, so a word, a number, a date or a
//! file name never matches. A field one types into is the user's own
//! words and is not looked at; only texts the application shows are.

use ljos_design::limits::HASH_MIN_CHARS;

use super::controls::shown_texts;
use crate::cx::Cx;

/// What the identifier in `text` is, and the identifier itself.
pub(crate) fn raw_identifier(text: &str) -> Option<(&'static str, String)> {
    for dump in ["[object Object]", "QObject(0x", "_QMLTYPE_", "QQuick"] {
        if let Some(at) = text.find(dump)
            && (dump != "QQuick" || text[at..].contains("(0x"))
        {
            return Some(("an object dump", dump.to_string()));
        }
    }
    for token in text.split(|c: char| c.is_whitespace() || "\"'()[]{},;".contains(c)) {
        let token = token.trim_end_matches(['.', ':', '!', '?']);
        if is_uuid(token) {
            return Some(("a UUID", token.to_string()));
        }
        if token.contains("://") && token.contains('?') {
            return Some(("a request URL", token.to_string()));
        }
        if is_absolute_path(token) {
            return Some(("an absolute path", token.to_string()));
        }
        if is_hash(token) {
            return Some(("a hash", token.to_string()));
        }
    }
    None
}

fn is_uuid(t: &str) -> bool {
    let parts: Vec<&str> = t.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

fn is_absolute_path(t: &str) -> bool {
    if t.contains("://") {
        return false;
    }
    let unix = t
        .strip_prefix('/')
        .or_else(|| t.strip_prefix("~/"))
        .map(|rest| rest.split('/').filter(|s| !s.is_empty()).count() >= 2);
    let windows = t.len() > 3
        && t.as_bytes()[0].is_ascii_alphabetic()
        && t.get(1..3) == Some(":\\")
        && t[3..].split('\\').filter(|s| !s.is_empty()).count() >= 2;
    unix.unwrap_or(false) || windows
}

fn is_hash(t: &str) -> bool {
    let t = t.strip_prefix("0x").unwrap_or(t);
    t.len() >= HASH_MIN_CHARS
        && t.chars().all(|c| c.is_ascii_hexdigit())
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

pub(crate) fn check(cx: &mut Cx) {
    for (item, text) in shown_texts(cx) {
        let Some((what, id)) = raw_identifier(&text) else {
            continue;
        };
        cx.report(
            "content-raw-identifier",
            Some(item),
            format!(
                "the text shows {what} (`{id}`), which names the thing as the program does; \
                 show its human name (a title, a file name), and offer the identifier through \
                 a copy action or a tooltip"
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::raw_identifier;

    #[test]
    fn identifiers_are_told_by_their_shape() {
        let what = |t: &str| raw_identifier(t).map(|(w, _)| w);
        assert_eq!(
            what("Closed 3f2c8a1e-9b7d-4c1e-8f2a-5d6e7f8091a2"),
            Some("a UUID")
        );
        assert_eq!(what("/home/user/projects/app"), Some("an absolute path"));
        assert_eq!(what("C:\\Users\\me\\notes.txt"), Some("an absolute path"));
        assert_eq!(what("commit 9fceb02d0ae598e9"), Some("a hash"));
        assert_eq!(
            what("HTTP 502 https://api.example.test/v1/items?page=2"),
            Some("a request URL")
        );
        assert_eq!(what("Item(0x55d0) [object Object]"), Some("an object dump"));
    }

    #[test]
    fn words_numbers_and_names_are_not() {
        for t in [
            "Save the file",
            "1234567890123",
            "Notes/2026",
            "/help",
            "report.pdf",
            "Visit https://example.org/docs",
            "deadbeef",
            "and/or",
            "12:30 – 14:00",
        ] {
            assert_eq!(raw_identifier(t), None, "{t}");
        }
    }
}
