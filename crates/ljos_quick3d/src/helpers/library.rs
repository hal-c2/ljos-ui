//! The module's QML types, compiled to IR when the crate is built and
//! registered from the pack `build.rs` writes.

use std::collections::HashMap;
use std::sync::Arc;

use ljos::ljos_engine::{ComponentSource, Engine, LogLevel, Version};
use ljos_ir::CompiledFile;

/// What `build.rs` packed: a count, then per file its key and its IR, each
/// behind a little-endian `u32` length.
static PACK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/helpers.bin"));

/// Where the files say they are, in messages.
const URL_PREFIX: &str = "ljos-quick3d-helpers:/";

const CATEGORY: &str = "ljos.quick3d";
const URI: &str = "QtQuick3D.Helpers";
const DIRECTORY: &str = "QtQuick3D/Helpers";

#[derive(Debug)]
struct Library {
    files: HashMap<String, Arc<CompiledFile>>,
}

impl Library {
    /// Reads the pack. A file that does not decode (the IR version moved
    /// on without a rebuild) is left out and named in the result.
    fn embedded() -> (Self, Vec<String>) {
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
        (Library { files }, bad)
    }

    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.files.keys().cloned().collect();
        keys.sort_unstable();
        keys
    }
}

impl ComponentSource for Library {
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

/// Registers the QML-written types of `QtQuick3D.Helpers` 6.0.
pub(super) fn register(engine: &Engine) {
    let (library, undecoded) = Library::embedded();
    let keys = library.keys();
    engine.add_component_library(library);
    for bad in undecoded {
        engine.log(LogLevel::Warning, CATEGORY, format!("{URI}: {bad}"));
    }
    if keys.is_empty() {
        engine.log(
            LogLevel::Warning,
            CATEGORY,
            format!(
                "{URI} has no QML types: they were not compiled when ljos_quick3d was built \
                 (its build script says why)"
            ),
        );
    }
    for key in keys.iter().filter(|k| k.starts_with(DIRECTORY)) {
        if let Err(e) = engine.register_qml_type(URI, Version::new(6, 0), key) {
            engine.log(LogLevel::Warning, CATEGORY, format!("{URI}: {e}"));
        }
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
