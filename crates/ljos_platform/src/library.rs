//! The QML files of this crate (`qml/`): the dialogs the application draws.
//! The build script compiles them to IR and packs them; nothing parses QML
//! at run time (ARCHITECTURE.md principle 5).

use std::collections::HashMap;
use std::sync::Arc;

use ljos_engine::ComponentSource;
use ljos_ir::CompiledFile;

/// What `build.rs` packed: a count, then per file its key and its IR, each
/// behind a little-endian `u32` length.
static PACK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dialogs.bin"));

/// Where the files say they are, in messages.
const URL_PREFIX: &str = "ljos-platform:/";

/// The embedded files, decoded once.
#[derive(Debug)]
pub(crate) struct DialogLibrary {
    files: HashMap<String, Arc<CompiledFile>>,
}

impl DialogLibrary {
    /// Reads the pack. A file that does not decode (the IR version moved
    /// on without a rebuild) is left out and named in the result.
    pub(crate) fn embedded() -> (Self, Vec<String>) {
        let mut files = HashMap::new();
        let mut bad = Vec::new();
        for (key, bytes) in entries(PACK) {
            match CompiledFile::from_bytes(bytes) {
                Ok(f) => {
                    files.insert(key.to_owned(), Arc::new(f));
                }
                Err(e) => bad.push(format!("{key}: {e}")),
            }
        }
        (DialogLibrary { files }, bad)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn keys(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.files.keys().map(String::as_str).collect();
        keys.sort_unstable();
        keys
    }
}

impl ComponentSource for DialogLibrary {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>> {
        self.files.get(key).cloned()
    }

    fn url(&self, key: &str) -> String {
        format!("{URL_PREFIX}{key}.qml")
    }

    fn key(&self, url: &str) -> String {
        let s = url.strip_prefix(URL_PREFIX).unwrap_or(url);
        s.strip_suffix(".qml").unwrap_or(s).to_owned()
    }
}

/// The `(key, IR)` pairs of a pack; stops at the first entry that does not
/// fit, which only a truncated file has.
fn entries(pack: &[u8]) -> Vec<(&str, &[u8])> {
    fn take<'a>(rest: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
        if rest.len() < n {
            return None;
        }
        let (head, tail) = rest.split_at(n);
        *rest = tail;
        Some(head)
    }
    fn len(rest: &mut &[u8]) -> Option<usize> {
        let b = take(rest, 4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    }
    let mut rest = pack;
    let mut out = Vec::new();
    let Some(count) = len(&mut rest) else {
        return out;
    };
    for _ in 0..count {
        let entry = (|| {
            let n = len(&mut rest)?;
            let key = std::str::from_utf8(take(&mut rest, n)?).ok()?;
            let n = len(&mut rest)?;
            Some((key, take(&mut rest, n)?))
        })();
        match entry {
            Some(e) => out.push(e),
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_drawn_dialog_is_packed_and_decodes() {
        let (lib, bad) = DialogLibrary::embedded();
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(
            lib.keys(),
            vec![
                "QtQuick/Dialogs/Ljos/ColorDialog",
                "QtQuick/Dialogs/Ljos/FileDialog",
                "QtQuick/Dialogs/Ljos/FontDialog",
                "QtQuick/Dialogs/Ljos/MessageDialog",
            ]
        );
        let url = lib.url("QtQuick/Dialogs/Ljos/ColorDialog");
        assert_eq!(lib.key(&url), "QtQuick/Dialogs/Ljos/ColorDialog");
    }
}
