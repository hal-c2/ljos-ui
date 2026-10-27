//! Ahead-of-time QML compilation for an application's build script
//! (the non-default `build` feature, for `[build-dependencies]`).
//!
//! The build script compiles the application's QML — imports of its
//! own native modules included — into a pack in `OUT_DIR`; the
//! program embeds the pack with [`include_qml!`](crate::include_qml)
//! and links no parser (ARCHITECTURE.md principle 5). A file that names
//! a property, signal, method or enum key its type does not have fails
//! the build.
//!
//! The compile learns the native types one of two ways:
//!
//! - From the application's type dump file ([`compile_dir_with_types`]),
//!   which the application's own test writes and keeps current
//!   ([`crate::types::check_file`]). The build script runs none of the
//!   application's code, so the native types live in the application's
//!   own crate and the build script depends on `ljos` alone.
//! - From a throwaway Engine the script's `register` function fills
//!   ([`compile_dir`]). A build script cannot use the types of the crate
//!   it builds, so those types then live in a crate of their own that
//!   both the build script and the program depend on.
//!
//! ```no_run
//! // build.rs, with `ljos = { features = ["build"], default-features = false }`
//! // under [build-dependencies]
//! let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
//! ljos::build::compile_dir_with_types(
//!     "qml".as_ref(),
//!     "ljostypes.json".as_ref(),
//!     &out.join("qml.pack"),
//! )
//! .unwrap();
//! ```
//!
//! ```ignore
//! // main.rs
//! ljos::App::new("Hello")
//!     .register(my_app::register)
//!     .source(ljos::include_qml!("qml.pack"))
//!     .root("Main")
//!     .run()?;
//! ```

use std::path::{Path, PathBuf};

use ljos_engine::{Engine, EngineError, MapSource, TypeDump, TypeDumpError};

pub use crate::compile::{CompileError, EngineOracle, EngineType, NativeTypeError};
use crate::compile::{OnError, compile_dirs as compile_to_source, compile_with_oracle};
use crate::types::WRITE_VAR;

/// Why a build script's compile wrote no pack.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildError {
    /// The QML did not compile, or a directory could not be read.
    #[error(transparent)]
    Compile(#[from] CompileError),
    /// The type dump file could not be read: the text says what writes it.
    #[error("{}: {source}; the application's test writes it with `{WRITE_VAR}=write cargo test --no-fail-fast`", path.display())]
    TypesUnread {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The type dump file is not a dump this version reads.
    #[error("{}: {source}; the application's test writes it with `{WRITE_VAR}=write cargo test --no-fail-fast`", path.display())]
    TypesUnreadable {
        path: PathBuf,
        #[source]
        source: TypeDumpError,
    },
    /// The type dump describes a type the compiler cannot be told of.
    #[error("{}: {source}", path.display())]
    NativeTypes {
        path: PathBuf,
        #[source]
        source: NativeTypeError,
    },
    /// The compiled files do not fit in a pack.
    #[error(transparent)]
    Pack(EngineError),
    /// The pack could not be written.
    #[error("{}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Compiles every `.qml` file under `dir` into the pack `out`, against
/// the table of types plus what `register` registers; see
/// [`compile_dirs`].
pub fn compile_dir(dir: &Path, register: impl Fn(&Engine), out: &Path) -> Result<(), BuildError> {
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
pub fn compile_dirs(
    dirs: &[&Path],
    register: impl Fn(&Engine),
    out: &Path,
) -> Result<(), BuildError> {
    for dir in dirs {
        rerun_if_changed(dir);
    }
    let engine = crate::engine();
    register(&engine);
    let source = compile_to_source(dirs, Some(&engine), OnError::Fail)?;
    write_pack(&source, out)
}

/// Compiles every `.qml` file under `dir` into the pack `out`, against
/// the table of types plus the native types the type dump file `types`
/// describes; see [`compile_dirs_with_types`].
pub fn compile_dir_with_types(dir: &Path, types: &Path, out: &Path) -> Result<(), BuildError> {
    compile_dirs_with_types(&[dir], types, out)
}

/// [`compile_dirs`] against a type dump file in place of a `register`
/// function: the native types are those `types` describes (written by
/// [`crate::types::check_file`]), over the toolkit's own. Runs none of
/// the application's code. Any file with an error fails the compile,
/// unless `LJOS_TYPES=write` is set: then the file is being written
/// again, and a QML file that does not compile against the old one is
/// packed as a failure (it fails where it is made) and reported as a
/// warning, so that the application builds and its test can write the
/// new file.
pub fn compile_dirs_with_types(dirs: &[&Path], types: &Path, out: &Path) -> Result<(), BuildError> {
    for dir in dirs {
        rerun_if_changed(dir);
    }
    println!("cargo:rerun-if-changed={}", types.display());
    println!("cargo:rerun-if-env-changed={}", crate::types::WRITE_VAR);
    let writing = crate::types::writing();
    let path = || types.to_path_buf();
    let read = std::fs::read_to_string(types)
        .map_err(|source| BuildError::TypesUnread {
            path: path(),
            source,
        })
        .and_then(|text| {
            TypeDump::from_json(&text).map_err(|source| BuildError::TypesUnreadable {
                path: path(),
                source,
            })
        });
    let dump = match read {
        Ok(dump) => dump,
        Err(_) if writing => TypeDump {
            format: TypeDump::FORMAT,
            modules: Vec::new(),
            unnamed: Vec::new(),
        },
        Err(e) => return Err(e),
    };
    let engine = crate::engine();
    let oracle =
        EngineOracle::from_dump(&dump, &engine).map_err(|source| BuildError::NativeTypes {
            path: path(),
            source,
        })?;
    let source = match compile_with_oracle(dirs, oracle.clone(), OnError::Fail) {
        Ok(source) => source,
        Err(e) if writing => {
            for line in e.to_string().lines() {
                println!("cargo:warning={line}");
            }
            println!(
                "cargo:warning=packed as failing while {} is written; build again once it is",
                types.display()
            );
            compile_with_oracle(dirs, oracle, OnError::Record)?
        }
        Err(e) => return Err(e.into()),
    };
    write_pack(&source, out)
}

fn write_pack(source: &MapSource, out: &Path) -> Result<(), BuildError> {
    let pack = source.to_pack().map_err(BuildError::Pack)?;
    std::fs::write(out, pack).map_err(|source| BuildError::Write {
        path: out.to_path_buf(),
        source,
    })
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
