//! The type dump file: what an application's native types are, written
//! where a build script can read it without running the application.
//!
//! The application's own test writes the file from its `register`
//! function and fails when the file no longer says what `register`
//! registers; its build script compiles the QML against the file with
//! [`build::compile_dir_with_types`](crate::build) (the `build`
//! feature). The native types can then live in the application's own
//! crate: the build script needs the file, not the types.
//!
//! ```no_run
//! // tests/types.rs
//! # mod my_app { pub fn register(_: &ljos::Engine) {} }
//! #[test]
//! fn the_type_dump_is_current() {
//!     ljos::types::check_file("ljostypes.json".as_ref(), my_app::register).unwrap();
//! }
//! ```
//!
//! `LJOS_TYPES=write cargo test --no-fail-fast` writes the file instead
//! of checking it. The build script, seeing the same variable, lets a
//! QML file that no longer compiles through as a recorded failure, so
//! that the test can run; tests that make that component fail until the
//! file is written, which is why the run goes on past them.

use std::path::{Path, PathBuf};

use ljos_engine::Engine;
pub use ljos_engine::{BlockEntry, TypeDump, TypeDumpError};

/// The environment variable that makes [`check_file`] write the file:
/// `LJOS_TYPES=write`.
pub const WRITE_VAR: &str = "LJOS_TYPES";

/// Why [`check_file`] failed: what to run, when the file is missing or
/// stale, is in the text.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TypesError {
    /// With `LJOS_TYPES=write`, the file could not be written.
    #[error("{}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The file could not be read: it is not there yet, say.
    #[error("{}: {source}; write it with `{WRITE_VAR}=write cargo test --no-fail-fast`", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The file is not a type dump this version reads.
    #[error("{}: {source}", path.display())]
    Dump {
        path: PathBuf,
        #[source]
        source: TypeDumpError,
    },
    /// The file describes other types than `register` registers.
    #[error("{} does not describe the types the application registers; write it with `{WRITE_VAR}=write cargo test --no-fail-fast`", path.display())]
    Stale { path: PathBuf },
}

/// Whether this run writes type dumps rather than checking them.
pub fn writing() -> bool {
    std::env::var(WRITE_VAR).is_ok_and(|v| v == "write")
}

/// The types `register` registers on an Engine that holds the standard
/// modules, and the group and attached types they reach: the toolkit's
/// own types are left out, as the compiler knows them already.
pub fn dump(register: impl Fn(&Engine)) -> TypeDump {
    let standard = crate::engine().type_dump();
    let engine = crate::engine();
    register(&engine);
    let mut dump = engine.type_dump();
    let known = |uri: &str, name: &str| {
        standard
            .modules
            .iter()
            .any(|m| m.uri == uri && m.types.iter().any(|t| t.name == name))
    };
    for module in &mut dump.modules {
        let uri = module.uri.clone();
        module.types.retain(|t| !known(&uri, &t.name));
    }
    dump.modules.retain(|m| !m.types.is_empty());
    dump.unnamed
        .retain(|t| !standard.unnamed.iter().any(|s| s.name == t.name));
    dump
}

/// Checks that the file at `path` holds [`dump`]`(register)`; with
/// `LJOS_TYPES=write` writes it instead. `Err` says what to run when the
/// file is missing or stale.
pub fn check_file(path: &Path, register: impl Fn(&Engine)) -> Result<(), TypesError> {
    let dump = dump(register);
    let path_buf = || path.to_path_buf();
    if writing() {
        return std::fs::write(path, dump.to_json() + "\n").map_err(|source| TypesError::Write {
            path: path_buf(),
            source,
        });
    }
    let text = std::fs::read_to_string(path).map_err(|source| TypesError::Read {
        path: path_buf(),
        source,
    })?;
    let written = TypeDump::from_json(&text).map_err(|source| TypesError::Dump {
        path: path_buf(),
        source,
    })?;
    if written != dump {
        return Err(TypesError::Stale { path: path_buf() });
    }
    Ok(())
}
