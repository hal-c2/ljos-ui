//! `MyFocusScopeWidget`, the widget of the qtdoc focus snippet
//! (`corpus/qtdoc/doc/src/snippets/declarative/focus`).
//!
//! The snippet's `qmldir` names the type and points it at
//! `myfocusscopewidget.qml`, a file the corpus does not carry; the
//! snippet only uses `focus` and `color` on it. The widget here is
//! Ljós's own, giving the snippet just those and showing where the
//! focus is. Its QML is compiled to IR by this crate's build script
//! and registered as a QML-defined type, the way the Basic style's
//! types are, so the application that registers it links no
//! parser.
//!
//! ```ignore
//! let engine = ljos::engine();
//! ljos_example_focusscopewidget::register(&engine);
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use ljos::ComponentSource;
use ljos::prelude::Version;
use ljos_ir::CompiledFile;

/// What `build.rs` packed.
static PACK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/widget.bin"));

/// The module the type is registered in. The snippet's directory is
/// not a module (its `qmldir` has no `module` line), and the file
/// does not import one for it, so the type lives in the module the
/// file already imports for every item type.
const MODULE: &str = "QtQuick";

const URL_PREFIX: &str = "focusscopewidget:/";

#[derive(Debug)]
struct Library {
    files: HashMap<String, Arc<CompiledFile>>,
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

/// The `(key, IR)` pairs of the pack; stops at the first entry that
/// does not fit, which only a truncated file has.
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

/// The one seam a host calls: registers `MyFocusScopeWidget` on
/// `engine`. It needs the standard modules (`ljos::engine()` has
/// them) because the widget is made of `FocusScope` and friends.
pub fn register(engine: &ljos::Engine) {
    let mut files = HashMap::new();
    for (key, bytes) in entries(PACK) {
        match CompiledFile::from_bytes(bytes) {
            Ok(f) => {
                files.insert(key.to_owned(), Arc::new(f));
            }
            Err(e) => engine.log(
                ljos::prelude::LogLevel::Warning,
                "focusscopewidget",
                format!("{key}: {e}"),
            ),
        }
    }
    let keys: Vec<String> = files.keys().cloned().collect();
    engine.add_component_library(Library { files });
    for key in keys {
        if let Err(err) = engine.register_qml_type(MODULE, Version::new(2, 15), &key) {
            engine.log(
                ljos::prelude::LogLevel::Warning,
                "focusscopewidget",
                format!("{key}: {err}"),
            );
        }
    }
}
