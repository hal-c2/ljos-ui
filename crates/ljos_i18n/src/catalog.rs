//! The translations of one language (ARCHITECTURE.md §15).
//!
//! A message is found by `(context, source, disambiguation)`, as in Qt
//! (<https://doc.qt.io/qt-6/i18n-source-translation.html>), or by its id
//! when the application uses text ids. What is found is one text, or one
//! text per numerus form.
//!
//! [`Messages`] is the question the runtime asks; [`Catalog`] answers it
//! from memory and [`crate::Ljl`] from the bytes of a `.ljl` file.

use std::collections::HashMap;

/// Where the forms of one message are in its catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The message has one text per numerus form of the language.
    pub numerus: bool,
    pub(crate) first: u32,
    pub(crate) count: u32,
}

impl Entry {
    /// How many forms the message has; one unless it is `numerus`.
    #[must_use]
    pub const fn forms(self) -> usize {
        self.count as usize
    }
}

/// A source of translations for one language.
pub trait Messages {
    /// The language the translations are in, as the catalogue names it (`is`, `pt_BR`).
    fn language(&self) -> &str;
    /// The message with exactly this context, source text and
    /// disambiguation (`""` for none).
    fn text(&self, context: &str, source: &str, comment: &str) -> Option<Entry>;
    /// The message with this text id.
    fn id(&self, id: &str) -> Option<Entry>;
    /// One form of a message this catalogue returned.
    fn form(&self, entry: Entry, index: usize) -> Option<&str>;
    /// The translation of an ICU message (`t`), keyed by its context (`""`
    /// for none) and its source message or explicit id. A separate table
    /// from [`text`](Self::text): a `t` message is never looked for among
    /// `qsTr`'s, nor the other way round.
    fn message(&self, context: &str, key: &str) -> Option<Entry> {
        let _ = (context, key);
        None
    }
    /// Where a message this catalogue returned was read from (`is.po:12`),
    /// for a warning about it.
    fn origin(&self, entry: Entry) -> Option<String> {
        let _ = entry;
        None
    }
}

/// What a message is translated to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Translation {
    Text(String),
    /// One text per numerus form, in `.ts` order.
    Numerus(Vec<String>),
}

type Comments = Vec<(Box<str>, Entry)>;

/// A catalogue held in memory: what a `.ts` file turns into, and what a
/// `.ljl` file is written from.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    language: String,
    // Nested so a lookup by three borrowed strings allocates nothing.
    texts: HashMap<Box<str>, HashMap<Box<str>, Comments>>,
    ids: HashMap<Box<str>, Entry>,
    messages: HashMap<Box<str>, HashMap<Box<str>, Entry>>,
    forms: Vec<String>,
    /// The file the catalogue was read from, and the line each message
    /// starts on, by its first form.
    file: Option<String>,
    lines: HashMap<u32, u32>,
}

/// One message of a [`Catalog`], for writing it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextKey<'a> {
    pub context: &'a str,
    pub source: &'a str,
    pub comment: &'a str,
}

impl Catalog {
    #[must_use]
    pub fn new(language: &str) -> Self {
        Self {
            language: language.to_owned(),
            ..Self::default()
        }
    }

    fn store(&mut self, translation: Translation) -> Entry {
        let first = self.forms.len();
        let numerus = match translation {
            Translation::Text(text) => {
                self.forms.push(text);
                false
            }
            Translation::Numerus(forms) => {
                self.forms.extend(forms);
                true
            }
        };
        // More than four thousand million forms do not fit in memory first.
        Entry {
            numerus,
            first: u32::try_from(first).unwrap_or(u32::MAX),
            count: u32::try_from(self.forms.len() - first).unwrap_or(u32::MAX),
        }
    }

    /// Adds a message. The first translation of a message wins, as it does
    /// when Qt releases a `.ts` file that has the same message twice;
    /// `false` says this one was ignored.
    pub fn insert(
        &mut self,
        context: &str,
        source: &str,
        comment: &str,
        translation: Translation,
    ) -> bool {
        if self.text(context, source, comment).is_some() {
            return false;
        }
        let entry = self.store(translation);
        self.texts
            .entry(context.into())
            .or_default()
            .entry(source.into())
            .or_default()
            .push((comment.into(), entry));
        true
    }

    /// Adds a message found by its text id; the first one wins.
    pub fn insert_id(&mut self, id: &str, translation: Translation) -> bool {
        if self.ids.contains_key(id) {
            return false;
        }
        let entry = self.store(translation);
        self.ids.insert(id.into(), entry);
        true
    }

    /// Adds the translation of an ICU message; the first one wins.
    pub fn insert_message(&mut self, context: &str, key: &str, text: &str) -> bool {
        if self.message(context, key).is_some() {
            return false;
        }
        let entry = self.store(Translation::Text(text.to_owned()));
        self.messages
            .entry(context.into())
            .or_default()
            .insert(key.into(), entry);
        true
    }

    /// [`insert_message`](Self::insert_message), remembering the line the
    /// message starts on in the catalogue's file.
    pub fn insert_message_at(&mut self, context: &str, key: &str, text: &str, line: u32) -> bool {
        let first = u32::try_from(self.forms.len()).unwrap_or(u32::MAX);
        let added = self.insert_message(context, key, text);
        if added {
            self.lines.insert(first, line);
        }
        added
    }

    /// Names the file the catalogue was read from, for warnings.
    pub fn set_file(&mut self, file: &str) {
        self.file = Some(file.to_owned());
    }

    /// Every ICU message, in `(context, key)` order.
    #[must_use]
    pub fn messages(&self) -> Vec<(&str, &str, Entry)> {
        let mut out: Vec<_> = self
            .messages
            .iter()
            .flat_map(|(context, keys)| {
                keys.iter()
                    .map(move |(key, entry)| (&**context, &**key, *entry))
            })
            .collect();
        out.sort_by_key(|(context, key, _)| (*context, *key));
        out
    }

    /// How many messages the catalogue has, text ids and ICU messages
    /// included.
    #[must_use]
    pub fn len(&self) -> usize {
        let texts: usize = self
            .texts
            .values()
            .flat_map(HashMap::values)
            .map(Vec::len)
            .sum();
        let messages: usize = self.messages.values().map(HashMap::len).sum();
        texts + self.ids.len() + messages
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every message found by its text, in key order so that what is
    /// written from a catalogue does not depend on hashing.
    #[must_use]
    pub fn texts(&self) -> Vec<(TextKey<'_>, Entry)> {
        let mut out: Vec<_> = self
            .texts
            .iter()
            .flat_map(|(context, sources)| {
                sources.iter().flat_map(move |(source, comments)| {
                    comments.iter().map(move |(comment, entry)| {
                        (
                            TextKey {
                                context,
                                source,
                                comment,
                            },
                            *entry,
                        )
                    })
                })
            })
            .collect();
        out.sort_by_key(|(key, _)| (key.context, key.source, key.comment));
        out
    }

    /// Every message found by its id, in id order.
    #[must_use]
    pub fn ids(&self) -> Vec<(&str, Entry)> {
        let mut out: Vec<_> = self.ids.iter().map(|(id, e)| (&**id, *e)).collect();
        out.sort_by_key(|(id, _)| *id);
        out
    }
}

impl Messages for Catalog {
    fn language(&self) -> &str {
        &self.language
    }

    fn text(&self, context: &str, source: &str, comment: &str) -> Option<Entry> {
        self.texts
            .get(context)?
            .get(source)?
            .iter()
            .find(|(c, _)| &**c == comment)
            .map(|(_, entry)| *entry)
    }

    fn id(&self, id: &str) -> Option<Entry> {
        self.ids.get(id).copied()
    }

    fn form(&self, entry: Entry, index: usize) -> Option<&str> {
        if index >= entry.forms() {
            return None;
        }
        self.forms
            .get(entry.first as usize + index)
            .map(String::as_str)
    }

    fn message(&self, context: &str, key: &str) -> Option<Entry> {
        self.messages.get(context)?.get(key).copied()
    }

    fn origin(&self, entry: Entry) -> Option<String> {
        let file = self.file.as_deref()?;
        Some(match self.lines.get(&entry.first) {
            Some(line) => format!("{file}:{line}"),
            None => file.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_is_found_by_all_three_parts_of_its_key() {
        let mut c = Catalog::new("is");
        assert!(c.insert("Main", "Open", "", Translation::Text("Opna".into())));
        assert!(c.insert(
            "Main",
            "Open",
            "adjective",
            Translation::Text("Opinn".into())
        ));
        let plain = c.text("Main", "Open", "").unwrap();
        assert_eq!(c.form(plain, 0), Some("Opna"));
        let adjective = c.text("Main", "Open", "adjective").unwrap();
        assert_eq!(c.form(adjective, 0), Some("Opinn"));
        assert_eq!(c.text("Main", "Open", "noun"), None);
        assert_eq!(c.text("Other", "Open", ""), None);
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn the_first_translation_of_a_message_wins() {
        let mut c = Catalog::new("is");
        assert!(c.insert("Main", "Yes", "", Translation::Text("Já".into())));
        assert!(!c.insert("Main", "Yes", "", Translation::Text("Jú".into())));
        let e = c.text("Main", "Yes", "").unwrap();
        assert_eq!(c.form(e, 0), Some("Já"));
    }

    #[test]
    fn a_numerus_message_has_one_text_per_form() {
        let mut c = Catalog::new("is");
        c.insert(
            "Main",
            "%n file(s)",
            "",
            Translation::Numerus(vec!["%n skrá".into(), "%n skrár".into()]),
        );
        let e = c.text("Main", "%n file(s)", "").unwrap();
        assert!(e.numerus);
        assert_eq!(e.forms(), 2);
        assert_eq!(c.form(e, 1), Some("%n skrár"));
        assert_eq!(c.form(e, 2), None);
    }

    #[test]
    fn icu_messages_are_a_table_of_their_own() {
        let mut c = Catalog::new("is");
        c.set_file("is.po");
        c.insert("", "Open", "", Translation::Text("Opna".into()));
        assert!(c.insert_message_at("", "Open", "Opna skrá", 12));
        assert!(c.insert_message("menu", "Open", "Opna valmynd"));
        assert!(!c.insert_message("menu", "Open", "Opna"));
        let e = c.message("", "Open").unwrap();
        assert_eq!(c.form(e, 0), Some("Opna skrá"));
        assert_eq!(c.origin(e).as_deref(), Some("is.po:12"));
        let menu = c.message("menu", "Open").unwrap();
        assert_eq!(c.origin(menu).as_deref(), Some("is.po"));
        assert_eq!(c.message("", "Close"), None);
        assert_eq!(c.form(c.text("", "Open", "").unwrap(), 0), Some("Opna"));
        assert_eq!(c.len(), 3);
        assert_eq!(c.messages().len(), 2);
    }

    #[test]
    fn text_ids_are_a_namespace_of_their_own() {
        let mut c = Catalog::new("is");
        c.insert_id("app-title", Translation::Text("Ljós".into()));
        assert!(c.id("app-title").is_some());
        assert_eq!(c.text("", "app-title", ""), None);
        assert_eq!(c.ids().len(), 1);
    }
}
