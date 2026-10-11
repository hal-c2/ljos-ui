//! Embeds the table of Qt's types when one has been generated.
//!
//! `src/types.txt` is derived from a local Qt install and is never committed
//! (ARCHITECTURE.md D1, CLEAN_ROOM.md), so a fresh checkout and CI have none.
//! That is a configuration like any other, not a broken one: the crate then
//! reads QML against Ljós's own description of its types (`src/ljos.txt`,
//! committed) alone, and `qt_types::has_qt_table()` says so.
//!
//! - `LJOS_QT_TYPES=/path/to/types.txt` takes Qt's table from elsewhere.
//! - `LJOS_QT_TYPES=none` builds without one though `src/types.txt` is
//!   there: how a machine that has Qt checks what a machine without it gets.
//!
//! A file that is missing, or that has no module in it (`/dev/null`), is no
//! table either.

use std::{env, fs, path::PathBuf};

/// The value of `LJOS_QT_TYPES` that asks for a build without Qt's table.
const NONE: &str = "none";

fn main() {
    println!("cargo::rerun-if-env-changed=LJOS_QT_TYPES");
    println!("cargo::rustc-check-cfg=cfg(qt_types_table)");

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let path = match env::var_os("LJOS_QT_TYPES") {
        Some(path) if path == NONE => None,
        Some(path) if !path.is_empty() => Some(PathBuf::from(path)),
        _ => Some(manifest.join("src/types.txt")),
    };
    let text = path
        .and_then(|path| {
            // Watching a missing file makes cargo rerun this script each
            // build, which is what notices a table generated after the
            // first build.
            println!("cargo::rerun-if-changed={}", path.display());
            fs::read_to_string(path).ok()
        })
        .filter(|text| text.lines().any(|line| line.starts_with("module ")))
        .unwrap_or_default();
    if !text.is_empty() {
        println!("cargo::rustc-cfg=qt_types_table");
    }

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("set by cargo")).join("types.txt");
    fs::write(&out, text).expect("OUT_DIR is writable");
}
