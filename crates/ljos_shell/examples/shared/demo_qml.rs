//! Compiles a directory of QML for the `demo_qml` example. Shared by
//! `examples/demo_qml.rs` and `tests/demos.rs` through `#[path]`.
//!
//! Compiling at start-up is for the example only; a shipped app embeds the
//! compiled IR and never links the compiler (ARCHITECTURE principle 5).

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use ljos_engine::MapSource;

/// This example's `qml/` directory.
pub fn default_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/qml")
}

/// Compiles every file under `dir` into a component source, printing the
/// diagnostics and failing if any file has an error.
pub fn compile(dir: &Path) -> anyhow::Result<MapSource> {
    let mut source = MapSource::new();
    let mut failed = false;
    for r in ljos_compiler::compile_project(dir).with_context(|| format!("{}", dir.display()))? {
        match r.result {
            Ok(c) => {
                for d in &c.diagnostics {
                    eprintln!("{}: {:?}: {}", r.key, d.severity, d.message);
                }
                failed |= c.has_errors();
                source.insert(r.key, c.file);
            }
            Err(ds) => {
                for d in ds {
                    eprintln!("{}: {}", r.key, d.message);
                }
                failed = true;
            }
        }
    }
    if failed {
        bail!("{} did not compile", dir.display());
    }
    Ok(source)
}
