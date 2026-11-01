//! The QML modules Design Studio's output imports, built in: `import
//! QtQuick.Studio.Components` and `import QtQuick.Studio.DesignEffects` give
//! an application what is here. The other types of those modules stand in as
//! empty items (`qt_types`), and an application that carries Studio's modules
//! in its own import path uses those.
//!
//! The build script compiles `qml/` to IR; nothing parses QML at run time
//! (ARCHITECTURE.md principle 5).

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;

use ljos_engine::{ComponentSource, Engine, LogLevel, Version};
use ljos_ir::CompiledFile;

/// What `build.rs` packed: a count, then per file its key and its IR, each
/// behind a little-endian `u32` length.
static PACK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/studio.bin"));

/// Where the files say they are, in messages.
const URL_PREFIX: &str = "ljos-studio:/";

/// What the modules' messages are logged under.
const CATEGORY: &str = "ljos.studio";

/// Each module and the directory of its files below `qml/`.
const MODULES: &[(&str, &str)] = &[
    ("QtQuick.Studio.Components", "QtQuick/Studio/Components/"),
    (
        "QtQuick.Studio.DesignEffects",
        "QtQuick/Studio/DesignEffects/",
    ),
];

/// The embedded files, decoded once.
#[derive(Debug)]
struct StudioLibrary {
    files: HashMap<String, Arc<CompiledFile>>,
}

impl StudioLibrary {
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
        (StudioLibrary { files }, bad)
    }

    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.files.keys().cloned().collect();
        keys.sort_unstable();
        keys
    }
}

impl ComponentSource for StudioLibrary {
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

#[derive(Default)]
struct Registered(Cell<bool>);

/// Registers the modules at 1.0. Calling it again does nothing.
pub fn register(engine: &Engine) {
    if engine.extension::<Registered>().0.replace(true) {
        return;
    }
    let (library, undecoded) = StudioLibrary::embedded();
    let keys = library.keys();
    engine.add_component_library(library);
    for bad in undecoded {
        engine.log(LogLevel::Warning, CATEGORY, format!("Design Studio: {bad}"));
    }
    for key in &keys {
        let Some((uri, _)) = MODULES.iter().find(|(_, dir)| key.starts_with(dir)) else {
            continue;
        };
        if let Err(e) = engine.register_qml_type(uri, Version::new(1, 0), key) {
            engine.log(LogLevel::Warning, CATEGORY, format!("{uri}: {e}"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_is_packed_and_decodes() {
        let (lib, bad) = StudioLibrary::embedded();
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(
            lib.keys(),
            vec![
                "QtQuick/Studio/Components/SvgPathItem",
                "QtQuick/Studio/DesignEffects/DesignEffect",
            ]
        );
    }
}
