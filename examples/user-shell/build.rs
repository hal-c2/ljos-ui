//! Compiles `qml/` ahead of time into the pack the program embeds, with
//! the sources a user's file is compiled against.

use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets it"));
    if let Err(e) = ljos::build::Options::new().with_sources().compile_dirs(
        &[&dir.join("qml")],
        |_| {},
        &out.join("qml.pack"),
    ) {
        panic!("{e}");
    }
}
