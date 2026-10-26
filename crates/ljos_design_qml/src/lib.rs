//! The QML module `Ljos.Design` built in: `import Ljos.Design` gives an
//! application the palette (`Colors`), motion (`Motion`), the spacing and
//! type scales (`Spacing`, `Typography`), and transitions and behaviours
//! that use them, with no files of its own. The files are the ones
//! `ljos design export` writes, so an application that runs on Qt as well
//! exports them and imports the same module from a directory.
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
static PACK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/design.bin"));

/// Where the files say they are, in messages.
const URL_PREFIX: &str = "ljos-design:/";

/// What the module's messages are logged under.
pub const CATEGORY: &str = "ljos.design";

/// The module's URI and the directory of its files below `qml/`.
pub const URI: &str = "Ljos.Design";
const DIRECTORY: &str = "Ljos/Design";

/// The embedded files, decoded once.
#[derive(Debug)]
struct DesignLibrary {
    files: HashMap<String, Arc<CompiledFile>>,
}

impl DesignLibrary {
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
        (DesignLibrary { files }, bad)
    }

    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.files.keys().cloned().collect();
        keys.sort_unstable();
        keys
    }
}

impl ComponentSource for DesignLibrary {
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

/// Registers `Ljos.Design` 1.0. Its transitions are for QtQuick.Controls'
/// popups, so a host registers those first. Calling it again does nothing.
pub fn register(engine: &Engine) {
    if engine.extension::<Registered>().0.replace(true) {
        return;
    }
    let (library, undecoded) = DesignLibrary::embedded();
    let keys = library.keys();
    engine.add_component_library(library);
    for bad in undecoded {
        engine.log(LogLevel::Warning, CATEGORY, format!("Ljos.Design: {bad}"));
    }
    if keys.is_empty() {
        engine.log(
            LogLevel::Warning,
            CATEGORY,
            "Ljos.Design has no types: it was not compiled when ljos_design_qml was built (its \
             build script says why)",
        );
    }
    for key in keys.iter().filter(|k| k.starts_with(DIRECTORY)) {
        if let Err(e) = engine.register_qml_type(URI, Version::new(1, 0), key) {
            engine.log(LogLevel::Warning, CATEGORY, format!("Ljos.Design: {e}"));
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
    fn every_file_of_the_module_is_packed_and_decodes() {
        let (lib, bad) = DesignLibrary::embedded();
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(
            lib.keys(),
            vec![
                "Ljos/Design/Colors",
                "Ljos/Design/EnterTransition",
                "Ljos/Design/ExitTransition",
                "Ljos/Design/Motion",
                "Ljos/Design/MoveBehavior",
                "Ljos/Design/PressBehavior",
                "Ljos/Design/Spacing",
                "Ljos/Design/Typography",
            ]
        );
        let url = lib.url("Ljos/Design/Colors");
        assert_eq!(lib.key(&url), "Ljos/Design/Colors");
    }
}
