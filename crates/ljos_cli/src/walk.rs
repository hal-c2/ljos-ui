//! The one walk of a tree of files that every command takes, so that `ljos
//! fmt .`, `ljos check .`, `ljos shaders .` and `ljos i18n extract .` see
//! one set of files.

use std::path::{Path, PathBuf};

use ljos_compiler::skipped_dir;

/// Every file below `dir` that `wanted` takes, in the order of their
/// names, outside the directories the compiler skips ([`skipped_dir`]).
/// Links to directories are not entered: they can loop.
pub fn files_below(
    dir: &Path,
    wanted: fn(&Path) -> bool,
    out: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if !skipped_dir(&entry.file_name().to_string_lossy()) {
                files_below(&path, wanted, out)?;
            }
        } else if wanted(&path) && path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

pub fn is_qml(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "qml")
}
