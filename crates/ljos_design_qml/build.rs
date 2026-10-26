//! Compiles the module `Ljos.Design` (`qml/`) to IR and packs it into one
//! file that `src/lib.rs` embeds, so the crate ships no QML text and never
//! links the compiler (ARCHITECTURE.md principle 5).
//!
//! As for the Basic style (`ljos_controls/build.rs`), the compiler reads
//! the files against Qt's table of types where one was generated on this
//! machine and against Ljós's committed table (`qt_types/src/ljos.txt`)
//! otherwise; the files use only QtQuick and QtQuick.Controls.
//!
//! The files are written by `ljos_design::module`; `tests/tokens.rs` fails
//! when they are not what it writes.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// Set to `write` while Ljós's table of its own types is being written.
const WRITE: &str = "LJOS_OWN_TYPES";

fn main() {
    let qml = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it")).join("qml");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets it")).join("design.bin");
    println!("cargo:rerun-if-changed=build.rs");
    rerun_if_changed(&qml);

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
    if failed {
        // A member the files use that the committed table does not have
        // yet: the table is being written, and the next run has them.
        let own_table = !qt_types::has_qt_table();
        let writing = own_table && env::var_os(WRITE).is_some_and(|value| value == "write");
        assert!(writing, "Ljos.Design does not compile (see the warnings)");
        println!("cargo:rerun-if-env-changed={WRITE}");
        println!(
            "cargo:warning=ljos_design_qml: Ljos.Design does not compile against the table \
             being written: this build has none. Run the same command once more."
        );
        files.clear();
    }
    files.sort();
    fs::write(&out, pack(&files)).expect("OUT_DIR is writable");
}

/// Count, then per file: key length, key, IR length, IR. Lengths are
/// little-endian `u32`s. `src/library.rs` reads it back.
fn pack(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let len = |n: usize| {
        u32::try_from(n)
            .expect("a QML file under 4 GiB")
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
