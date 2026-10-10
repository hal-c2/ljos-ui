//! Compiles the Basic style (`qml/`) to IR and packs it into one file that
//! `src/style.rs` embeds, so the crate ships no QML text and never links the
//! compiler (ARCHITECTURE.md principle 5).
//!
//! The compiler asks Qt's type table what a name is, and that table is
//! generated locally from a Qt install (`mise run types`), never committed.
//! Without it the style cannot be compiled; the crate still builds, with an
//! empty style, and says so here and when the module is registered.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    let qml = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it")).join("qml");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets it")).join("style.bin");
    println!("cargo:rerun-if-changed=build.rs");
    rerun_if_changed(&qml);

    if !qt_types::is_available() {
        println!(
            "cargo:warning=ljos_controls: Qt's type table is missing (mise run types), so the \
             Basic style is not built and QtQuick.Controls will have no types"
        );
        fs::write(&out, pack(&[])).expect("OUT_DIR is writable");
        return;
    }

    let results =
        ljos_compiler::compile_project(&qml).unwrap_or_else(|e| panic!("{}: {e}", qml.display()));
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut failed = false;
    for r in results {
        match r.result {
            Ok(c) => {
                for d in &c.diagnostics {
                    println!("cargo:warning={}.qml: {}", r.key, d.message);
                }
                failed |= c.has_errors();
                let bytes = c
                    .file
                    .to_bytes()
                    .unwrap_or_else(|e| panic!("{}: {e}", r.key));
                files.push((r.key, bytes));
            }
            Err(ds) => {
                for d in ds {
                    println!("cargo:warning={}.qml: {}", r.key, d.message);
                }
                failed = true;
            }
        }
    }
    assert!(
        !failed,
        "the Basic style does not compile (see the warnings)"
    );
    files.sort();
    fs::write(&out, pack(&files)).expect("OUT_DIR is writable");
}

/// Count, then per file: key length, key, IR length, IR. Lengths are
/// little-endian `u32`s. `src/style.rs` reads it back.
fn pack(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let len = |n: usize| {
        u32::try_from(n)
            .expect("a style file under 4 GiB")
            .to_le_bytes()
    };
    let mut out = Vec::new();
    out.extend_from_slice(&len(files.len()));
    for (key, bytes) in files {
        out.extend_from_slice(&len(key.len()));
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&len(bytes.len()));
        out.extend_from_slice(bytes);
    }
    out
}

fn rerun_if_changed(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rerun_if_changed(&p);
        } else {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }
}
