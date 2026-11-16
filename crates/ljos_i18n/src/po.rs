//! `.po` catalogues of ICU messages, read and written (feature `po`).
//!
//! The layout is gettext's
//! (<https://www.gnu.org/software/gettext/manual/html_node/PO-Files.html>)
//! as Lingui writes it: `msgid` is the source message in ICU MessageFormat,
//! or the message's explicit id, `msgstr` is its translation in the same
//! syntax, and plurals live inside the message rather than in gettext's
//! `msgid_plural`. A message with an explicit id carries
//! `#. js-lingui-explicit-id` and `#. Default: <message>`, so a translator
//! sees the text and Lingui's tools read the file the same way.
//!
//! The writer is deterministic: a fixed header without dates, entries in
//! bytewise `(msgid, msgctxt)` order and obsolete (`#~`) ones last, so that
//! extracting twice from the same sources gives the same bytes.

use std::fmt::Write as _;
use std::path::Path;

use crate::Catalog;

const EXPLICIT_ID: &str = "js-lingui-explicit-id";
const DEFAULT: &str = "Default: ";

/// Why text is not a `.po` catalogue.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {reason}")]
pub struct PoError {
    pub line: u32,
    pub reason: String,
}

/// One message of a `.po` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PoEntry {
    pub context: Option<String>,
    /// The source message, or the explicit id.
    pub id: String,
    /// Empty when nobody translated it yet.
    pub translation: String,
    /// `# ` lines: what translators wrote.
    pub translator_comments: Vec<String>,
    /// `#.` lines other than the explicit id's two: the comment the
    /// developer gave the message.
    pub comments: Vec<String>,
    /// `id` is an explicit id; `default` is then the source message.
    pub explicit_id: bool,
    pub default: Option<String>,
    /// `#:` references, `file:line` each.
    pub origins: Vec<String>,
    /// `#,` flags (`fuzzy`).
    pub flags: Vec<String>,
    /// `#~`: kept for translators, not used.
    pub obsolete: bool,
    /// The line `msgid` is on, counting from 1; 0 for an entry not read
    /// from a file.
    pub line: u32,
}

impl PoEntry {
    #[must_use]
    pub fn is_fuzzy(&self) -> bool {
        self.flags.iter().any(|f| f == "fuzzy")
    }
}

/// A `.po` file: its language and its messages, the header left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PoFile {
    /// The header's `Language:`.
    pub language: Option<String>,
    pub entries: Vec<PoEntry>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Context,
    Id,
    Plural,
    Str,
    /// `msgstr[1]` and on: gettext's plural forms, which a message of
    /// Ljós's does not have and which are read past.
    Other,
}

fn unquote(text: &str, line: u32) -> Result<String, PoError> {
    let fail = |reason: &str| PoError {
        line,
        reason: reason.to_owned(),
    };
    let inner = text
        .trim()
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .ok_or_else(|| fail("expected a string in double quotes"))?;
    // C's escapes, as gettext writes and reads them; an octal or a hex
    // escape is a byte, so the bytes of one character may be spelt out.
    let mut out: Vec<u8> = Vec::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
            continue;
        }
        let byte = match chars.next() {
            Some('n') => b'\n',
            Some('t') => b'\t',
            Some('r') => b'\r',
            Some('a') => 0x07,
            Some('b') => 0x08,
            Some('f') => 0x0c,
            Some('v') => 0x0b,
            Some('"') => b'"',
            Some('\'') => b'\'',
            Some('?') => b'?',
            Some('\\') => b'\\',
            Some(first @ '0'..='7') => {
                let mut value = first.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    match chars.peek().and_then(|c| c.to_digit(8)) {
                        Some(digit) => {
                            value = value * 8 + digit;
                            chars.next();
                        }
                        None => break,
                    }
                }
                u8::try_from(value)
                    .map_err(|_| fail(&format!("`\\{value:o}` is more than a byte")))?
            }
            Some('x') => {
                let mut value = None;
                for _ in 0..2 {
                    match chars.peek().and_then(|c| c.to_digit(16)) {
                        Some(digit) => {
                            value = Some(value.unwrap_or(0) * 16 + digit);
                            chars.next();
                        }
                        None => break,
                    }
                }
                let value = value.ok_or_else(|| fail("`\\x` has no hex digits"))?;
                u8::try_from(value).unwrap_or(u8::MAX)
            }
            Some(other) => return Err(fail(&format!("`\\{other}` is not an escape"))),
            None => return Err(fail("the string ends in `\\`")),
        };
        out.push(byte);
    }
    String::from_utf8(out).map_err(|_| fail("the escaped bytes are not UTF-8"))
}

impl PoFile {
    /// Reads a `.po` file.
    ///
    /// # Errors
    /// A line that is neither a comment, a keyword with a quoted string,
    /// nor a quoted string that continues one.
    pub fn parse(text: &str) -> Result<Self, PoError> {
        let mut file = Self::default();
        let mut entry = PoEntry::default();
        let mut field: Option<Field> = None;
        let mut started = false;
        let finish = |entry: &mut PoEntry, started: &mut bool, file: &mut Self| {
            if *started {
                let done = std::mem::take(entry);
                if done.id.is_empty() && done.context.is_none() && !done.obsolete {
                    file.language = header_language(&done.translation);
                } else {
                    file.entries.push(done);
                }
            }
            *started = false;
        };
        for (number, raw) in text.lines().enumerate() {
            let line = u32::try_from(number + 1).unwrap_or(u32::MAX);
            let raw = raw.trim_end_matches('\r');
            let raw = raw.trim_start();
            let (body, obsolete) = match raw.strip_prefix("#~") {
                Some(rest) => (rest.trim_start(), true),
                None => (raw, false),
            };
            if body.is_empty() {
                if !obsolete {
                    finish(&mut entry, &mut started, &mut file);
                    field = None;
                }
                continue;
            }
            if let Some(comment) = body.strip_prefix('#') {
                // A comment after the keywords starts the next entry.
                if field.is_some() {
                    finish(&mut entry, &mut started, &mut file);
                    field = None;
                }
                started = true;
                let (kind, rest) =
                    comment.split_at(comment.chars().next().map_or(0, char::len_utf8));
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                match kind {
                    "." if rest == EXPLICIT_ID => entry.explicit_id = true,
                    "." => match rest.strip_prefix(DEFAULT) {
                        // A default of many lines is a comment for each.
                        Some(default) if entry.explicit_id => match &mut entry.default {
                            Some(before) => {
                                before.push('\n');
                                before.push_str(default);
                            }
                            None => entry.default = Some(default.to_owned()),
                        },
                        _ => entry.comments.push(rest.to_owned()),
                    },
                    ":" => entry
                        .origins
                        .extend(rest.split_whitespace().map(str::to_owned)),
                    "," => entry.flags.extend(
                        rest.split(',')
                            .map(str::trim)
                            .filter(|f| !f.is_empty())
                            .map(str::to_owned),
                    ),
                    // Previous messages (`#|`) are msgmerge's, and go.
                    "|" => {}
                    _ => entry
                        .translator_comments
                        .push(comment.strip_prefix(' ').unwrap_or(comment).to_owned()),
                }
                continue;
            }
            if body.starts_with('"') {
                let more = unquote(body, line)?;
                match field {
                    Some(Field::Context) => {
                        entry.context.get_or_insert_default().push_str(&more);
                    }
                    Some(Field::Id) => entry.id.push_str(&more),
                    Some(Field::Str) => entry.translation.push_str(&more),
                    Some(Field::Plural | Field::Other) => {}
                    None => {
                        return Err(PoError {
                            line,
                            reason: "a string that continues no keyword".to_owned(),
                        });
                    }
                }
                continue;
            }
            let (keyword, value) = body.split_once(char::is_whitespace).ok_or(PoError {
                line,
                reason: format!("`{body}` is not a keyword and a string"),
            })?;
            let next = match keyword {
                "msgctxt" => Field::Context,
                "msgid" => Field::Id,
                "msgid_plural" => Field::Plural,
                "msgstr" | "msgstr[0]" => Field::Str,
                k if k.starts_with("msgstr[") => Field::Other,
                k => {
                    return Err(PoError {
                        line,
                        reason: format!("`{k}` is not a keyword of a .po file"),
                    });
                }
            };
            // `msgctxt` or `msgid` after the translation starts the next entry.
            if matches!(next, Field::Context | Field::Id)
                && matches!(field, Some(Field::Str | Field::Other | Field::Plural))
            {
                finish(&mut entry, &mut started, &mut file);
            }
            started = true;
            entry.obsolete |= obsolete;
            let value = unquote(value, line)?;
            match next {
                Field::Context => entry.context = Some(value),
                Field::Id => {
                    entry.id = value;
                    entry.line = line;
                }
                Field::Str => entry.translation = value,
                Field::Plural | Field::Other => {}
            }
            field = Some(next);
        }
        finish(&mut entry, &mut started, &mut file);
        Ok(file)
    }

    /// The translations to use, as a [`Catalog`] of ICU messages. Its
    /// language is the header's `Language:`, or, when there is none, the
    /// file's name without `.po` (`is.po` is Icelandic). Obsolete, fuzzy
    /// and untranslated entries are left out: a fuzzy one is a guess
    /// nobody checked yet, which gettext does not use either.
    #[must_use]
    pub fn catalog(&self, file: &str) -> Catalog {
        let name = Path::new(file)
            .file_name()
            .map_or_else(|| file.to_owned(), |n| n.to_string_lossy().into_owned());
        let language = self
            .language
            .clone()
            .unwrap_or_else(|| name.strip_suffix(".po").unwrap_or(&name).to_owned());
        let mut catalog = Catalog::new(&language);
        catalog.set_file(&name);
        for entry in &self.entries {
            if entry.obsolete || entry.is_fuzzy() || entry.translation.is_empty() {
                continue;
            }
            catalog.insert_message_at(
                entry.context.as_deref().unwrap_or(""),
                &entry.id,
                &entry.translation,
                entry.line,
            );
        }
        catalog
    }

    /// Writes the file: the fixed header, then the entries sorted by
    /// `msgid` and `msgctxt`, obsolete ones last. Writing what was read
    /// gives the same file back, up to that order and header. A template
    /// (`.pot`) has no language, and its header says none.
    #[must_use]
    pub fn write(&self, language: Option<&str>) -> String {
        let mut out = String::new();
        out.push_str("msgid \"\"\nmsgstr \"\"\n");
        let language = language.map(|language| format!("Language: {language}"));
        for line in [
            Some("MIME-Version: 1.0".to_owned()),
            Some("Content-Type: text/plain; charset=UTF-8".to_owned()),
            Some("Content-Transfer-Encoding: 8bit".to_owned()),
            language,
            Some("X-Generator: ljos i18n extract".to_owned()),
        ]
        .into_iter()
        .flatten()
        {
            let _ = writeln!(out, "\"{}\\n\"", escape(&line));
        }
        let mut entries: Vec<&PoEntry> = self.entries.iter().collect();
        entries.sort_by(|a, b| {
            (
                a.obsolete,
                a.id.as_bytes(),
                a.context.as_deref().map(str::as_bytes),
            )
                .cmp(&(
                    b.obsolete,
                    b.id.as_bytes(),
                    b.context.as_deref().map(str::as_bytes),
                ))
        });
        for entry in entries {
            out.push('\n');
            write_entry(&mut out, entry);
        }
        out
    }
}

/// What [`PoFile::merge`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PoMerge {
    /// The messages the sources have, each once.
    pub messages: usize,
    /// Of those, the ones the file did not have.
    pub new: usize,
    /// Of those, the ones with a translation.
    pub translated: usize,
    /// The entries kept for translators though the sources lost them,
    /// before and now.
    pub obsolete: usize,
    /// The messages the file had that the sources lost, translated or not.
    pub gone: usize,
}

impl PoFile {
    /// Brings the file up to date with the messages of the sources, given
    /// in the order found; one message found in several places is one
    /// entry with every place. A message keeps its translation, its
    /// translators' comments and its flags; what the sources say of it
    /// (the comment, the places, the default of an id) is the sources'.
    /// A translated message the sources lost is kept as obsolete (`#~`),
    /// and comes back with its translation when the sources have it again;
    /// an untranslated one goes.
    pub fn merge(&mut self, found: impl IntoIterator<Item = PoEntry>) -> PoMerge {
        let key = |entry: &PoEntry| (entry.context.clone(), entry.id.clone());
        let mut sources: Vec<PoEntry> = Vec::new();
        let mut at = std::collections::HashMap::new();
        for entry in found {
            match at.get(&key(&entry)) {
                Some(&index) => {
                    let into: &mut PoEntry = &mut sources[index];
                    for origin in entry.origins {
                        if !into.origins.contains(&origin) {
                            into.origins.push(origin);
                        }
                    }
                    for comment in entry.comments {
                        if !into.comments.contains(&comment) {
                            into.comments.push(comment);
                        }
                    }
                    into.explicit_id |= entry.explicit_id;
                    if into.default.is_none() {
                        into.default = entry.default;
                    }
                }
                None => {
                    at.insert(key(&entry), sources.len());
                    sources.push(entry);
                }
            }
        }

        let mut before: std::collections::HashMap<_, PoEntry> = std::collections::HashMap::new();
        let mut was_live = std::collections::HashSet::new();
        for entry in std::mem::take(&mut self.entries) {
            let k = key(&entry);
            if !entry.obsolete {
                was_live.insert(k.clone());
            }
            // A live entry wins over an obsolete one of the same key.
            match before.get(&k) {
                Some(kept) if !kept.obsolete => {}
                _ => {
                    before.insert(k, entry);
                }
            }
        }

        let mut report = PoMerge {
            messages: sources.len(),
            ..PoMerge::default()
        };
        for mut entry in sources {
            let k = key(&entry);
            match before.remove(&k) {
                Some(old) => {
                    entry.translation = old.translation;
                    entry.translator_comments = old.translator_comments;
                    entry.flags = old.flags;
                    if !was_live.contains(&k) {
                        report.new += 1;
                    }
                }
                None => report.new += 1,
            }
            entry.obsolete = false;
            entry.line = 0;
            if !entry.translation.is_empty() {
                report.translated += 1;
            }
            self.entries.push(entry);
        }
        let mut gone: Vec<PoEntry> = before.into_values().collect();
        gone.sort_by_key(key);
        for mut entry in gone {
            if !entry.obsolete {
                report.gone += 1;
            }
            if entry.translation.is_empty() {
                continue;
            }
            // Where it was, and what its author said of it, left with it.
            entry.obsolete = true;
            entry.origins.clear();
            entry.comments.clear();
            entry.line = 0;
            report.obsolete += 1;
            self.entries.push(entry);
        }
        report
    }

    /// The file without where its messages are used: a line that moved is
    /// not a reason for a translator to look again.
    #[must_use]
    pub fn without_origins(&self) -> Self {
        let mut file = self.clone();
        for entry in &mut file.entries {
            entry.origins.clear();
            entry.line = 0;
        }
        file
    }
}

fn header_language(header: &str) -> Option<String> {
    header.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        let value = value.trim();
        (key.trim().eq_ignore_ascii_case("Language") && !value.is_empty()).then(|| value.to_owned())
    })
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

/// `keyword "text"`, or over several lines where the text has line
/// breaks inside it, as gettext writes them.
fn keyword(out: &mut String, prefix: &str, name: &str, text: &str) {
    let inner = text.strip_suffix('\n').unwrap_or(text);
    if !inner.contains('\n') {
        let _ = writeln!(out, "{prefix}{name} \"{}\"", escape(text));
        return;
    }
    let _ = writeln!(out, "{prefix}{name} \"\"");
    for piece in text.split_inclusive('\n') {
        let _ = writeln!(out, "{prefix}\"{}\"", escape(piece));
    }
}

fn write_entry(out: &mut String, entry: &PoEntry) {
    // A comment cannot span lines, so each line of one is a line of its own.
    let lines = |text: &str| text.lines().map(str::to_owned).collect::<Vec<_>>();
    for comment in entry.translator_comments.iter().flat_map(|c| lines(c)) {
        let _ = writeln!(out, "# {comment}");
    }
    for comment in entry.comments.iter().flat_map(|c| lines(c)) {
        let _ = writeln!(out, "#. {comment}");
    }
    if entry.explicit_id {
        let _ = writeln!(out, "#. {EXPLICIT_ID}");
        if let Some(default) = &entry.default {
            // Every line, an empty last one too, so reading it back gives
            // the whole message.
            for line in default.split('\n') {
                let _ = writeln!(out, "#. {DEFAULT}{line}");
            }
        }
    }
    if !entry.origins.is_empty() {
        let _ = writeln!(out, "#: {}", entry.origins.join(" "));
    }
    if !entry.flags.is_empty() {
        let _ = writeln!(out, "#, {}", entry.flags.join(", "));
    }
    let prefix = if entry.obsolete { "#~ " } else { "" };
    if let Some(context) = &entry.context {
        keyword(out, prefix, "msgctxt", context);
    }
    keyword(out, prefix, "msgid", &entry.id);
    keyword(out, prefix, "msgstr", &entry.translation);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Messages;

    const IS: &str = r#"msgid ""
msgstr ""
"Language: is\n"

# Checked by Jón
#. Shown on the greeting card
#: src/Main.qml:4 src/Card.qml:9
msgid "Hello {name}"
msgstr "Halló {name}"

msgctxt "menu"
msgid "Open"
msgstr "Opna valmynd"

#. js-lingui-explicit-id
#. Default: Home
msgid "nav.home"
msgstr "Heim"

#, fuzzy
msgid "Close"
msgstr "Loka"

msgid "Save"
msgstr ""

msgid ""
"Two\n"
"lines"
msgstr "Tvær\nlínur"

#~ msgid "Gone"
#~ msgstr "Farið"
"#;

    #[test]
    fn it_reads_every_part_of_an_entry() {
        let po = PoFile::parse(IS).unwrap();
        assert_eq!(po.language.as_deref(), Some("is"));
        let hello = &po.entries[0];
        assert_eq!(hello.id, "Hello {name}");
        assert_eq!(hello.translation, "Halló {name}");
        assert_eq!(hello.translator_comments, ["Checked by Jón"]);
        assert_eq!(hello.comments, ["Shown on the greeting card"]);
        assert_eq!(hello.origins, ["src/Main.qml:4", "src/Card.qml:9"]);
        assert_eq!(hello.line, 8);
        assert_eq!(po.entries[1].context.as_deref(), Some("menu"));
        let home = &po.entries[2];
        assert!(home.explicit_id);
        assert_eq!(home.default.as_deref(), Some("Home"));
        assert!(home.comments.is_empty());
        assert!(po.entries[3].is_fuzzy());
        assert_eq!(po.entries[5].id, "Two\nlines");
        let gone = po.entries.last().unwrap();
        assert!(gone.obsolete);
        assert_eq!(gone.id, "Gone");
        assert_eq!(gone.translation, "Farið");
    }

    #[test]
    fn the_catalogue_has_what_is_translated_and_says_where_it_came_from() {
        let catalog = PoFile::parse(IS).unwrap().catalog("translations/is.po");
        assert_eq!(catalog.language(), "is");
        let hello = catalog.message("", "Hello {name}").unwrap();
        assert_eq!(catalog.form(hello, 0), Some("Halló {name}"));
        assert_eq!(catalog.origin(hello).as_deref(), Some("is.po:8"));
        assert!(catalog.message("menu", "Open").is_some());
        assert!(catalog.message("", "Open").is_none());
        assert!(catalog.message("", "nav.home").is_some());
        assert!(catalog.message("", "Close").is_none(), "fuzzy");
        assert!(catalog.message("", "Save").is_none(), "untranslated");
        assert!(catalog.message("", "Gone").is_none(), "obsolete");
        assert_eq!(catalog.text("", "Hello {name}", ""), None);
    }

    #[test]
    fn without_a_language_header_the_file_name_is_the_language() {
        let po = PoFile::parse("msgid \"Open\"\nmsgstr \"Opna\"\n").unwrap();
        assert_eq!(po.catalog("de_CH.po").language(), "de_CH");
    }

    #[test]
    fn the_writer_is_deterministic_and_reads_back() {
        let po = PoFile::parse(IS).unwrap();
        let written = po.write(Some("is"));
        assert!(written.starts_with(
            "msgid \"\"\nmsgstr \"\"\n\"MIME-Version: 1.0\\n\"\n\
             \"Content-Type: text/plain; charset=UTF-8\\n\"\n\
             \"Content-Transfer-Encoding: 8bit\\n\"\n\"Language: is\\n\"\n\
             \"X-Generator: ljos i18n extract\\n\"\n"
        ));
        let mut shuffled = po.clone();
        shuffled.entries.reverse();
        assert_eq!(shuffled.write(Some("is")), written);
        let again = PoFile::parse(&written).unwrap();
        assert_eq!(again.write(Some("is")), written);
        let ids: Vec<_> = again.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "Close",
                "Hello {name}",
                "Open",
                "Save",
                "Two\nlines",
                "nav.home",
                "Gone"
            ]
        );
        assert!(
            written.contains("#. js-lingui-explicit-id\n#. Default: Home\nmsgid \"nav.home\"\n")
        );
        assert!(written.contains("#~ msgid \"Gone\"\n#~ msgstr \"Farið\"\n"));
        assert!(written.contains("msgid \"\"\n\"Two\\n\"\n\"lines\"\n"));
    }

    #[test]
    fn an_indented_obsolete_entry_is_obsolete_and_keeps_the_header() {
        let text = "msgid \"\"\nmsgstr \"\"\n\"Language: is\\n\"\n\n  #~ msgid \"Gone\"\n  #~ msgstr \"Farið\"\n";
        let po = PoFile::parse(text).unwrap();
        assert_eq!(po.language.as_deref(), Some("is"));
        assert_eq!(po.catalog("translations.po").language(), "is");
        let [gone] = po.entries.as_slice() else {
            panic!("{:?}", po.entries);
        };
        assert!(gone.obsolete);
        assert_eq!(gone.id, "Gone");
        assert_eq!(gone.translation, "Farið");
    }

    #[test]
    fn a_default_of_many_lines_reads_back_whole() {
        for default in ["First\nSecond", "First\n\nThird", "Last line\n"] {
            let po = PoFile {
                language: None,
                entries: vec![PoEntry {
                    id: "greeting".into(),
                    explicit_id: true,
                    default: Some(default.into()),
                    ..PoEntry::default()
                }],
            };
            let read = PoFile::parse(&po.write(Some("is"))).unwrap();
            assert_eq!(read.entries[0].default.as_deref(), Some(default));
        }
    }

    #[test]
    fn same_msgid_sorts_by_context() {
        let entry = |context: Option<&str>| PoEntry {
            context: context.map(str::to_owned),
            id: "Open".into(),
            ..PoEntry::default()
        };
        let po = PoFile {
            language: None,
            entries: vec![entry(Some("menu")), entry(None), entry(Some("file"))],
        };
        let written = po.write(Some("en"));
        let file = written.find("\"file\"").unwrap();
        let menu = written.find("\"menu\"").unwrap();
        assert!(file < menu);
        assert!(written.find("\n\nmsgid \"Open\"").unwrap() < file);
    }

    #[test]
    fn c_escapes_are_read_as_gettext_writes_them() {
        let file = PoFile::parse("msgid \"Open\"\nmsgstr \"\\117pna \\303\\251 \\x41\\a\\'\\?\"\n")
            .unwrap();
        assert_eq!(file.entries[0].translation, "Opna é A\u{7}'?");
        let error = PoFile::parse("msgid \"a\"\nmsgstr \"\\377\"\n").unwrap_err();
        assert_eq!(error.line, 2);
    }

    #[test]
    fn broken_lines_are_reported_with_their_number() {
        let error = PoFile::parse("msgid \"a\"\nmsgstr oops\n").unwrap_err();
        assert_eq!(error.line, 2);
        let error = PoFile::parse("\"stray\"\n").unwrap_err();
        assert_eq!(error.line, 1);
    }

    fn found(id: &str, context: Option<&str>, origin: &str) -> PoEntry {
        PoEntry {
            id: id.to_owned(),
            context: context.map(str::to_owned),
            origins: vec![origin.to_owned()],
            ..PoEntry::default()
        }
    }

    #[test]
    fn merging_keeps_translations_and_translated_gone_ones() {
        let mut po = PoFile::parse(IS).unwrap();
        let report = po.merge([
            found("Hello {name}", None, "Main.qml:5"),
            found("Open", Some("menu"), "Main.qml:6"),
            found("Hello {name}", None, "Card.qml:2"),
            found("Gone", None, "Main.qml:9"),
            found("New", None, "Main.qml:7"),
        ]);
        assert_eq!(
            report,
            PoMerge {
                messages: 4,
                // `New`, and `Gone`, which was obsolete: it comes back to
                // the live catalogue with its translation, and counts as
                // new there.
                new: 2,
                translated: 3,
                // `nav.home`, `Close` and the two lines; `Save` had no
                // translation and goes.
                obsolete: 3,
                gone: 4,
            }
        );
        let hello = po.entries.iter().find(|e| e.id == "Hello {name}").unwrap();
        assert_eq!(hello.translation, "Halló {name}");
        assert_eq!(hello.translator_comments, ["Checked by Jón"]);
        assert_eq!(hello.origins, ["Main.qml:5", "Card.qml:2"]);
        assert!(hello.comments.is_empty());
        let gone = po.entries.iter().find(|e| e.id == "Gone").unwrap();
        assert!(!gone.obsolete);
        assert_eq!(gone.translation, "Farið");
        let close = po.entries.iter().find(|e| e.id == "Close").unwrap();
        assert!(close.obsolete && close.is_fuzzy());
        assert!(po.entries.iter().all(|e| e.id != "Save"));
    }

    #[test]
    fn merging_twice_changes_nothing() {
        let sources = || {
            [
                found("A", None, "Main.qml:1"),
                found("B", Some("x"), "Main.qml:2"),
            ]
        };
        let mut po = PoFile::default();
        po.merge(sources());
        let once = po.write(Some("is"));
        let mut again = PoFile::parse(&once).unwrap();
        let report = again.merge(sources());
        assert_eq!(report.new, 0);
        assert_eq!(report.gone, 0);
        assert_eq!(again.write(Some("is")), once);
    }

    #[test]
    fn a_template_has_no_language() {
        let written = PoFile::default().write(None);
        assert!(!written.contains("Language"));
        assert!(written.contains("X-Generator: ljos i18n extract"));
    }
}
