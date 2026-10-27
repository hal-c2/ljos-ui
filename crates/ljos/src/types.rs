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

use std::path::Path;

use ljos_engine::Engine;
pub use ljos_engine::{TypeDump, TypeDumpError};

/// The environment variable that makes [`check_file`] write the file:
/// `LJOS_TYPES=write`.
pub const WRITE_VAR: &str = "LJOS_TYPES";

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
pub fn check_file(path: &Path, register: impl Fn(&Engine)) -> Result<(), String> {
    let dump = dump(register);
    if writing() {
        return std::fs::write(path, dump.to_json() + "\n")
            .map_err(|e| format!("{}: {e}", path.display()));
    }
    let again = format!("write it with `{WRITE_VAR}=write cargo test --no-fail-fast`");
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("{}: {e}; {again}", path.display()))?;
    let written = TypeDump::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if written != dump {
        return Err(format!(
            "{} does not describe the types the application registers; {again}",
            path.display()
        ));
    }
    Ok(())
}
