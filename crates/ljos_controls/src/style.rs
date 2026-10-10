//! The Basic style: QML files of this crate (`qml/`), compiled to IR by
//! the build script and embedded here. Nothing parses QML at run time.

use std::collections::HashMap;
use std::sync::Arc;

use ljos_engine::ComponentSource;
use ljos_ir::CompiledFile;

/// What `build.rs` packed: a count, then per file its key and its IR, each
/// behind a little-endian `u32` length.
static PACK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/style.bin"));

/// Where the style's files say they are, in messages and to
/// [`is_style_url`].
const URL_PREFIX: &str = "ljos-controls:/";

/// The directory of the style in the pack.
pub(crate) const BASIC: &str = "QtQuick/Controls/Basic";

/// Whether an object declared at `location` (as
/// [`ljos_engine::Engine::declared_at`] gives it) was declared by a style
/// file, not by an application.
pub(crate) fn is_style_url(location: &str) -> bool {
    location.starts_with(URL_PREFIX)
}

/// The embedded files. A file's IR is decoded when a type made of it is
/// first registered, and shared from then on.
#[derive(Debug)]
pub(crate) struct StyleLibrary {
    files: HashMap<String, Arc<CompiledFile>>,
}

impl StyleLibrary {
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
        (StyleLibrary { files }, bad)
    }

    /// The type names the style has files for, sorted.
    pub(crate) fn type_names(&self) -> Vec<String> {
        let prefix = format!("{BASIC}/");
        let mut names: Vec<String> = self
            .files
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix))
            .filter(|n| !n.contains('/'))
            .map(str::to_owned)
            .collect();
        names.sort();
        names
    }
}

impl ComponentSource for StyleLibrary {
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
    fn a_truncated_pack_gives_what_fits() {
        let mut pack = Vec::new();
        pack.extend_from_slice(&2u32.to_le_bytes());
        pack.extend_from_slice(&1u32.to_le_bytes());
        pack.push(b'a');
        pack.extend_from_slice(&2u32.to_le_bytes());
        pack.extend_from_slice(&[7, 8]);
        pack.extend_from_slice(&9u32.to_le_bytes());
        assert_eq!(entries(&pack), vec![("a", &[7u8, 8][..])]);
        assert!(entries(&[]).is_empty());
    }

    #[test]
    fn style_urls_are_told_from_application_urls() {
        let (lib, _) = StyleLibrary::embedded();
        let url = lib.url("QtQuick/Controls/Basic/Button");
        assert!(is_style_url(&format!("{url}:3:5: ")));
        assert!(!is_style_url("file:///Main.qml:3:5: "));
        assert_eq!(lib.key(&url), "QtQuick/Controls/Basic/Button");
    }
}
