//! Embeds the Qt type table when one has been generated.
//!
//! `src/types.txt` is derived from a local Qt install and is never committed
//! (ARCHITECTURE.md D1, CLEAN_ROOM.md), so a fresh checkout and CI have none.
//! The crate must still build there: without a table it compiles an empty one
//! and `qt_types::is_available()` says so.
//!
//! `LJOS_QT_TYPES=/path/to/types.txt` takes a table from elsewhere.

use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo::rerun-if-env-changed=LJOS_QT_TYPES");
    println!("cargo::rustc-check-cfg=cfg(qt_types_table)");

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let path = match env::var_os("LJOS_QT_TYPES") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => manifest.join("src/types.txt"),
    };
    // Watching a missing file makes cargo rerun this script each build, which
    // is what notices a table generated after the first build.
    println!("cargo::rerun-if-changed={}", path.display());

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("set by cargo")).join("types.txt");
    let text = match fs::read_to_string(&path) {
        Ok(text) => {
            println!("cargo::rustc-cfg=qt_types_table");
            text
        }
        Err(_) => {
            println!(
                "cargo::warning=no Qt type table at {}; qt_types is empty. Run `mise run types` against a local Qt.",
                path.display()
            );
            String::new()
        }
    };
    fs::write(&out, text).expect("OUT_DIR is writable");
}
