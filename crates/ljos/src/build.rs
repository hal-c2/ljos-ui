//! Ahead-of-time QML compilation for an application's build script
//! (the non-default `build` feature, for `[build-dependencies]`).
//!
//! The build script compiles the application's QML — imports of its
//! own native modules included — into a pack in `OUT_DIR`; the
//! program embeds the pack with [`include_qml!`](crate::include_qml)
//! and links no parser (ARCHITECTURE.md principle 5). The compile
//! knows the native types from a throwaway Engine the script's
//! `register` function fills, so those types live in a crate of their
//! own that both the build script and the program depend on: a build
//! script cannot use the types of the crate it builds.
//!
//! ```no_run
//! // build.rs, with `ljos = { features = ["build"], default-features = false }`
//! // and the natives crate under [build-dependencies]
//! # mod natives { pub fn register(_: &ljos::Engine) {} }
//! let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
//! ljos::build::compile_dir("qml".as_ref(), natives::register, &out.join("qml.pack")).unwrap();
//! ```
//!
//! ```ignore
//! // main.rs
//! ljos::App::new("Hello")
//!     .register(natives::register)
//!     .source(ljos::include_qml!("qml.pack"))
//!     .root("Main")
//!     .run()?;
//! ```

use std::path::Path;

use ljos_engine::Engine;

pub use crate::compile::{EngineOracle, EngineType};
use crate::compile::{OnError, compile_dirs as compile_to_source};

/// Compiles every `.qml` file under `dir` into the pack `out`, against
/// the table of types plus what `register` registers; see
/// [`compile_dirs`].
pub fn compile_dir(dir: &Path, register: impl Fn(&Engine), out: &Path) -> Result<(), String> {
    compile_dirs(&[dir], register, out)
}

/// Compiles every `.qml` file under each of `dirs` (keyed from its own
/// root, as [`App::qml_dirs`](crate::App::qml_dirs) reads them) into
/// the pack `out`. The types known are the table's and those
/// `register` puts on a throwaway Engine that also holds the standard
/// modules, as the program's Engine will. Any file with an error fails
/// the compile with every error, one a line: what ships is clean.
/// Prints `cargo:rerun-if-changed` for every directory and file read,
/// so the script runs again when the QML changes.
pub fn compile_dirs(dirs: &[&Path], register: impl Fn(&Engine), out: &Path) -> Result<(), String> {
    for dir in dirs {
        rerun_if_changed(dir);
    }
    let engine = crate::engine();
    register(&engine);
    let source = compile_to_source(dirs, Some(&engine), OnError::Fail)?;
    let pack = source.to_pack().map_err(|e| e.to_string())?;
    std::fs::write(out, pack).map_err(|e| format!("{}: {e}", out.display()))
}

fn rerun_if_changed(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(entries) = std::fs::read_dir(dir) else {
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
