//! Compiles the built-in styles (`qml/`) to IR and packs them into one file that
//! `src/style.rs` embeds, so the crate ships no QML text and never links the
//! compiler (ARCHITECTURE.md principle 5).
//!
//! The compiler asks a table of types what a name is (`qt_types`): Qt's own
//! description where one was generated on this machine (`mise run types`),
//! and otherwise Ljós's description of the types it implements, which is
//! committed (`crates/qt_types/src/ljos.txt`). So the style is built with no
//! Qt anywhere.
//!
//! That committed text is written by a test that links this crate
//! (`LJOS_OWN_TYPES=write cargo test -p ljos_cli --test own_types`), and
//! this script reads it: not a circle, because the text as committed is
//! what is read. It only shows when a style file uses a native member added
//! since the text was written, in a build without Qt's table: the style
//! cannot compile until the text is written again, and writing it needs
//! this crate built. So while it is being written (`LJOS_OWN_TYPES=write`)
//! a style that does not compile is left out with a warning, where any
//! other build fails; the same command run once more then has the style.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// Set to `write` while Ljós's table of its own types is being written.
const WRITE: &str = "LJOS_OWN_TYPES";

/// What a style that does not compile most likely means in a build that
/// reads it against Ljós's own table.
const STALE: &str = ". This build reads QML against crates/qt_types/src/ljos.txt: if a native \
                     member was added since that was written, write it again with \
                     `LJOS_OWN_TYPES=write cargo test -p ljos_cli --test own_types`";

fn main() {
    let qml = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it")).join("qml");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets it")).join("style.bin");
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
        let own_table = !qt_types::has_qt_table();
        let writing = own_table && env::var_os(WRITE).is_some_and(|value| value == "write");
        assert!(
            writing,
            "the built-in styles do not compile (see the warnings){}",
            if own_table { STALE } else { "" }
        );
        // Asked for only here, so that no other build is made again for it.
        println!("cargo:rerun-if-env-changed={WRITE}");
        println!(
            "cargo:warning=ljos_controls: the built-in styles do not compile against Ljós's table \
             of types as committed, which is being written again: this build has no style. Run \
             the same command once more."
        );
        files.clear();
    }
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
