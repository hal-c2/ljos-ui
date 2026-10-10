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
    forms: Vec<String>,
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

    /// How many messages the catalogue has, text ids included.
    #[must_use]
    pub fn len(&self) -> usize {
        let texts: usize = self
            .texts
            .values()
            .flat_map(HashMap::values)
            .map(Vec::len)
            .sum();
        texts + self.ids.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.texts.is_empty() && self.ids.is_empty()
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
    fn text_ids_are_a_namespace_of_their_own() {
        let mut c = Catalog::new("is");
        c.insert_id("app-title", Translation::Text("Ljós".into()));
        assert!(c.id("app-title").is_some());
        assert_eq!(c.text("", "app-title", ""), None);
        assert_eq!(c.ids().len(), 1);
    }
}
