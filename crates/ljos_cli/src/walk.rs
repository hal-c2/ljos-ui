//! The one walk of a tree of files that every command takes, so that `ljos
//! fmt .`, `ljos check .`, `ljos shaders .` and `ljos i18n extract .` see
//! one set of files.

use std::path::Path;

pub use ljos_compiler::files_below;

pub fn is_qml(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "qml")
}
