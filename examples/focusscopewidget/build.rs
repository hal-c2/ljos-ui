//! Compiles the widget (`qml/`) to IR and packs it into one file that
//! `src/lib.rs` embeds, so the crate ships no QML text and never links
//! the compiler (ARCHITECTURE.md principle 5). The pack is a count, then
//! per file: key length, key, IR length, IR, little-endian `u32`s.

use std::{env, fs, path::PathBuf};

fn main() {
    let qml = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it")).join("qml");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets it")).join("widget.bin");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", qml.display());

    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for r in
        ljos_compiler::compile_project(&qml).unwrap_or_else(|e| panic!("{}: {e}", qml.display()))
    {
        match r.result {
            Ok(c) => {
                assert!(
                    !c.has_errors(),
                    "{}.qml does not compile: {:?}",
                    r.key,
                    c.diagnostics
                );
                let bytes = c
                    .file
                    .to_bytes()
                    .unwrap_or_else(|e| panic!("{}: {e}", r.key));
                files.push((r.key, bytes));
            }
            Err(ds) => panic!("{}.qml does not compile: {ds:?}", r.key),
        }
    }
    files.sort();
    let len = |n: usize| u32::try_from(n).expect("under 4 GiB").to_le_bytes();
    let mut pack = Vec::new();
    pack.extend_from_slice(&len(files.len()));
    for (key, bytes) in &files {
        pack.extend_from_slice(&len(key.len()));
        pack.extend_from_slice(key.as_bytes());
        pack.extend_from_slice(&len(bytes.len()));
        pack.extend_from_slice(bytes);
    }
    fs::write(&out, pack).expect("OUT_DIR is writable");
}
