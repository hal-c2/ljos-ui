//! `RATCHET`: the scenarios of an external suite that must keep passing.
//!
//! One scenario id per line, `#` starts a comment. The file only grows by
//! `--raise`, which adds what passes now and refuses to write anything while
//! a listed scenario fails, so no run lowers the bar by itself. Taking a
//! line out is a decision a person makes and shows in a diff.

use std::{collections::BTreeSet, fs, path::Path};

#[derive(Debug, thiserror::Error)]
pub enum RatchetError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}

/// The ids, and the comment block the file starts with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ratchet {
    pub ids: BTreeSet<String>,
    header: String,
}

impl Ratchet {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut ratchet = Self::default();
        let mut in_header = true;
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(comment) = line.strip_prefix('#') {
                if in_header {
                    ratchet.header.push_str(&format!("#{comment}\n"));
                }
                continue;
            }
            in_header = false;
            ratchet.ids.insert(line.to_owned());
        }
        ratchet
    }

    /// A missing file is an empty ratchet: a suite starts with nothing held.
    ///
    /// # Errors
    /// The file exists and cannot be read.
    pub fn read(file: &Path) -> Result<Self, RatchetError> {
        match fs::read_to_string(file) {
            Ok(text) => Ok(Self::parse(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(RatchetError::Io {
                path: file.display().to_string(),
                source,
            }),
        }
    }

    #[must_use]
    pub fn render(&self) -> String {
        let mut out = self.header.clone();
        if !out.is_empty() {
            out.push('\n');
        }
        for id in &self.ids {
            out.push_str(id);
            out.push('\n');
        }
        out
    }

    /// # Errors
    /// The file cannot be written.
    pub fn write(&self, file: &Path) -> Result<(), RatchetError> {
        fs::write(file, self.render()).map_err(|source| RatchetError::Io {
            path: file.display().to_string(),
            source,
        })
    }

    /// Adds `passing` and returns the ids that were new. The caller has
    /// already checked that nothing held is failing.
    pub fn raise(&mut self, passing: impl IntoIterator<Item = String>) -> Vec<String> {
        passing
            .into_iter()
            .filter(|id| self.ids.insert(id.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_and_ids_survive_a_round_trip_sorted() {
        let text =
            "# Held scenarios.\n# Raised by --raise.\n\nb.feature :: B\n# note\na.feature :: A\n";
        let r = Ratchet::parse(text);
        assert_eq!(r.ids.len(), 2);
        assert_eq!(
            r.render(),
            "# Held scenarios.\n# Raised by --raise.\n\na.feature :: A\nb.feature :: B\n"
        );
    }

    #[test]
    fn raise_only_adds() {
        let mut r = Ratchet::parse("a\n");
        let added = r.raise(["a".to_owned(), "b".to_owned()]);
        assert_eq!(added, ["b"]);
        assert_eq!(r.ids.len(), 2);
        assert!(
            Ratchet::read(Path::new("/nonexistent/RATCHET"))
                .unwrap()
                .ids
                .is_empty()
        );
    }
}
