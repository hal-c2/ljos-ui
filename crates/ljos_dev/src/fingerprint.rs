//! What each file said when it was last compiled, as a BLAKE3 hash.
//!
//! An editor saves without changing anything, touches a file to change its
//! time, or writes a backup and renames it back. None of that is a change
//! to the application, and a file event says only that something happened
//! to a path. The fingerprint says whether it matters.

use std::collections::BTreeMap;

/// File key to the hash of its text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fingerprints {
    seen: BTreeMap<String, blake3::Hash>,
}

impl Fingerprints {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `text` (None: the file is gone) is something else than
    /// what was recorded for `key`.
    #[must_use]
    pub fn differs(&self, key: &str, text: Option<&str>) -> bool {
        match (self.seen.get(key), text) {
            (None, None) => false,
            (Some(old), Some(text)) => *old != blake3::hash(text.as_bytes()),
            _ => true,
        }
    }

    /// Takes `text` as what `key` says from now on.
    pub fn record(&mut self, key: &str, text: Option<&str>) {
        match text {
            Some(text) => {
                self.seen
                    .insert(key.to_owned(), blake3::hash(text.as_bytes()));
            }
            None => {
                self.seen.remove(key);
            }
        }
    }

    /// Takes `files` as everything there is. True when that is something
    /// else than what was recorded: a file more, a file less, or another
    /// text.
    pub fn replace<'a>(&mut self, files: impl Iterator<Item = (&'a str, &'a str)>) -> bool {
        let now: BTreeMap<String, blake3::Hash> = files
            .map(|(key, text)| (key.to_owned(), blake3::hash(text.as_bytes())))
            .collect();
        let changed = now != self.seen;
        self.seen = now;
        changed
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_text_is_no_change() {
        let mut f = Fingerprints::new();
        assert!(f.differs("Main", Some("Item {}")));
        f.record("Main", Some("Item {}"));
        assert!(!f.differs("Main", Some("Item {}")));
        assert!(f.differs("Main", Some("Item { }")));
    }

    #[test]
    fn a_file_that_goes_or_comes_is_a_change() {
        let mut f = Fingerprints::new();
        assert!(!f.differs("Gone", None));
        f.record("Main", Some("Item {}"));
        assert!(f.differs("Main", None));
        f.record("Main", None);
        assert!(f.is_empty());
        assert!(!f.differs("Main", None));
    }

    #[test]
    fn replacing_everything_says_whether_anything_differs() {
        let mut f = Fingerprints::new();
        assert!(f.replace([("A", "1"), ("B", "2")].into_iter()));
        assert!(!f.replace([("B", "2"), ("A", "1")].into_iter()));
        assert!(f.replace([("A", "1")].into_iter()));
        assert_eq!(f.len(), 1);
        assert!(f.replace([("A", "one")].into_iter()));
    }
}
