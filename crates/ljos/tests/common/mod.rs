//! Shared helpers: compile one `Main.qml` and make its root.

#![allow(dead_code)]

use ljos::prelude::*;

/// `main` compiled as `Main.qml` alone in a directory, against the
/// types registered on `engine`. The directory is gone when this
/// returns, so the file must not read anything beside it later.
pub fn compile_main(engine: &Engine, main: &str) -> ljos::MapSource {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), main).unwrap();
    ljos::dev::compile_dir_with(dir.path(), engine).unwrap()
}

/// The root of [`compile_main`]'s `Main`, made on `engine`.
pub fn create_main(engine: &Engine, main: &str) -> ObjectId {
    ljos::create_root(engine, compile_main(engine, main), "Main").unwrap()
}
