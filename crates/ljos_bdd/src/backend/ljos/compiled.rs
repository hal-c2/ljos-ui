//! A scenario whose files are compiled ahead of time: the code is in
//! `aot/`, written by running the scenario with `LJOS_AOT_FIXTURES=write`
//! from the files exactly as the engine loads them.
//!
//! A write run is of every scenario that compiles ahead of time: what it
//! leaves in `aot/` is the code of the files it was given, as `cargo fmt`
//! writes it, and nothing else.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use super::{Live, failed, key_of, qmldir_of};
use crate::backend::Result;

/// The variable that has a run write the code of the files it compiles
/// ahead of time.
const WRITE_VAR: &str = "LJOS_AOT_FIXTURES";

const RUNTIME: &str = "ljos_engine::aot";

pub(super) use super::aot::COMPILED;

fn writing() -> bool {
    std::env::var(WRITE_VAR).is_ok_and(|v| v == "write")
}

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend/ljos/aot")
}

fn hex(key: &[u8; 32]) -> String {
    key[..12].iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

impl Live {
    /// After the files `names` are given: each has its compiled code, or
    /// with `LJOS_AOT_FIXTURES=write` has it written.
    pub(super) fn compiled_ahead(&self, names: &[&str]) -> Result<()> {
        let source = self
            .engine
            .component_source()
            .ok_or_else(|| failed("no files are given"))?;
        let mut missing = Vec::new();
        let files: Vec<(&str, _)> = names
            .iter()
            .filter(|name| qmldir_of(name).is_none() && name.ends_with(".qml"))
            .filter_map(|name| Some((*name, source.load(key_of(name))?)))
            .collect();
        // What a file reads of the others types its code.
        let project: Vec<ljos_ir::CompiledFile> =
            files.iter().map(|(_, f)| (**f).clone()).collect();
        for (name, file) in &files {
            let key = ljos_ir::aot::content_key(file);
            if writing() {
                write(file, &project, key, &super::oracle())?;
            } else if !COMPILED.iter().any(|f| f.content == key) {
                missing.push(*name);
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        Err(failed(format!(
            "no code was compiled ahead of time from {} as this scenario gives it: \
             run the scenarios that compile ahead of time with {WRITE_VAR}=write \
             to write it, then again",
            missing.join(", ")
        )))
    }
}

/// The modules this run has written. Scenarios run on several threads:
/// one writes its file and the index at a time.
static WRITTEN: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

fn write(
    file: &ljos_ir::CompiledFile,
    project: &[ljos_ir::CompiledFile],
    key: [u8; 32],
    oracle: &impl qml_front::TypeOracle,
) -> Result<()> {
    let generated = ljos_compiler::aot::generate_in(file, project, oracle, RUNTIME);
    let dir = dir();
    let module = format!("k{}", hex(&key));
    let mut written = WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    let path = dir.join(format!("{module}.rs"));
    std::fs::write(&path, &generated.rust)
        .map_err(|e| failed(format!("cannot write {module}.rs: {e}")))?;
    format(&path)?;
    written.insert(module);
    index(&dir, &written)
}

/// Lays `path` out as `cargo fmt` would, by the workspace's rustfmt.
fn format(path: &Path) -> Result<()> {
    let rustfmt = std::env::var_os("RUSTFMT").unwrap_or_else(|| "rustfmt".into());
    let out = Command::new(rustfmt)
        .args(["--edition", "2024"])
        .arg(path)
        .output()
        .map_err(|e| failed(format!("cannot run rustfmt: {e}")))?;
    if out.status.success() {
        return Ok(());
    }
    Err(failed(format!(
        "rustfmt refused {}: {}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    )))
}

/// Writes `mod.rs` again for the modules this run wrote, and removes the
/// code of files no scenario it ran gave.
fn index(dir: &Path, modules: &BTreeSet<String>) -> Result<()> {
    let stale: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| failed(format!("cannot read {}: {e}", dir.display())))?
        .flatten()
        .filter(|e| {
            e.file_name().to_str().is_some_and(|name| {
                name.strip_suffix(".rs")
                    .is_some_and(|m| m.starts_with('k') && !modules.contains(m))
            })
        })
        .map(|e| e.path())
        .collect();
    for path in stale {
        std::fs::remove_file(&path)
            .map_err(|e| failed(format!("cannot remove {}: {e}", path.display())))?;
    }
    let mut out = String::from(
        "//! Code compiled ahead of time from the QML of the scenarios that say\n\
         //! \"Given the files are compiled ahead of time\". Running those with\n\
         //! `LJOS_AOT_FIXTURES=write` writes it; it is matched to a file by the\n\
         //! file's content, so code no scenario's file is any more is never run.\n\
         //! Generated: do not edit.\n\n",
    );
    for m in modules {
        let _ = writeln!(
            out,
            "#[allow(\n    unused_mut,\n    unused_variables,\n    unused_assignments,\n    \
             unreachable_code,\n    unused_labels,\n    unused_parens,\n    unused_braces,\n    \
             clippy::all,\n    clippy::pedantic\n)]\nmod {m};"
        );
    }
    out.push_str("\npub(in crate::backend) static COMPILED: &[&ljos_engine::aot::File] = &[");
    if !modules.is_empty() {
        out.push('\n');
    }
    for m in modules {
        let _ = writeln!(out, "    &{m}::FILE,");
    }
    out.push_str("];\n");
    let index = dir.join("mod.rs");
    std::fs::write(&index, out).map_err(|e| failed(format!("cannot write the index: {e}")))?;
    format(&index)
}
