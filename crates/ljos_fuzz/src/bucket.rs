//! The list of buckets already known: a difference in one of them is
//! reported but does not fail a run.

use std::{collections::BTreeMap, path::Path};

use crate::Error;

/// Signature to the reason it is known (and its issue, if any).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Known(pub BTreeMap<String, String>);

impl Known {
    /// Reads `path`: a signature, whitespace, and the reason, per line;
    /// `#` starts a comment line. A signature that ends in `*` knows every
    /// one that starts with what is before it. A missing file knows nothing.
    ///
    /// # Errors
    /// When the file cannot be read or a line has no reason.
    pub fn read(path: &Path) -> Result<Self, Error> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(Error::Io {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        Self::parse(&text).map_err(|line| Error::Known {
            path: path.to_owned(),
            line,
        })
    }

    /// # Errors
    /// The number of the first line without a reason.
    pub fn parse(text: &str) -> Result<Self, usize> {
        let mut map = BTreeMap::new();
        for (i, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (sig, reason) = line.split_once(char::is_whitespace).ok_or(i + 1)?;
            map.insert(sig.to_owned(), reason.trim().to_owned());
        }
        Ok(Self(map))
    }

    /// Why `signature` is known: its own line, or else the longest
    /// pattern that matches it.
    #[must_use]
    pub fn reason(&self, signature: &str) -> Option<&str> {
        if let Some(r) = self.0.get(signature) {
            return Some(r);
        }
        self.0
            .iter()
            .filter_map(|(k, r)| k.strip_suffix('*').map(|prefix| (prefix, r)))
            .filter(|(prefix, _)| signature.starts_with(prefix))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(_, r)| r.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_signatures_and_reasons() {
        let k = Known::parse("# known\nvalue:int:default  planted (#12)\n\n").expect("parses");
        assert_eq!(k.reason("value:int:default"), Some("planted (#12)"));
        assert_eq!(Known::parse("value:int:default"), Err(1));
    }

    #[test]
    fn a_pattern_knows_every_construct_of_its_kind() {
        let k = Known::parse("value:int:*  any int\nvalue:int:alias*  an alias\n").expect("parses");
        assert_eq!(k.reason("value:int:ternary"), Some("any int"));
        assert_eq!(k.reason("value:int:alias-to-member"), Some("an alias"));
        assert_eq!(k.reason("value:real:ternary"), None);
    }
}
