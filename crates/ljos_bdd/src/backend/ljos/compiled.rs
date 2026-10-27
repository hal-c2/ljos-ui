//! A scenario whose files are compiled ahead of time: the code is in
//! `aot/`, written by running the scenario with `LJOS_AOT_FIXTURES=write`
//! from the files exactly as the engine loads them.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

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
        for name in names {
            if qmldir_of(name).is_some() || !name.ends_with(".qml") {
                continue;
            }
            let Some(file) = source.load(key_of(name)) else {
                continue;
            };
            let key = ljos_ir::aot::content_key(&file);
            if writing() {
                write(&file, key, &super::oracle())?;
            } else if !COMPILED.iter().any(|f| f.content == key) {
                missing.push(*name);
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        Err(failed(format!(
            "no code was compiled ahead of time from {} as this scenario gives it: \
             run the scenario with {WRITE_VAR}=write to write it, then again",
            missing.join(", ")
        )))
    }
}

fn write(
    file: &ljos_ir::CompiledFile,
    key: [u8; 32],
    oracle: &impl qml_front::TypeOracle,
) -> Result<()> {
    let generated = ljos_compiler::aot::generate(file, oracle, RUNTIME);
    let dir = dir();
    let name = format!("k{}.rs", hex(&key));
    std::fs::write(dir.join(&name), &generated.rust)
        .map_err(|e| failed(format!("cannot write {name}: {e}")))?;
    index(&dir)
}

/// Writes `mod.rs` again for the files the directory has: scenarios run
/// on several threads, and each writes the whole of it.
fn index(dir: &Path) -> Result<()> {
    let mut modules: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| failed(format!("cannot read {}: {e}", dir.display())))?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let module = name.strip_suffix(".rs")?;
            (module.starts_with('k')).then(|| module.to_owned())
        })
        .collect();
    modules.sort();
    let mut out = String::from(
        "//! Code compiled ahead of time from the QML of the scenarios that say\n\
         //! \"Given the files are compiled ahead of time\". Running those with\n\
         //! `LJOS_AOT_FIXTURES=write` writes it; it is matched to a file by the\n\
         //! file's content, so code no scenario's file is any more is never run.\n\
         //! Generated: do not edit.\n\n",
    );
    for m in &modules {
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
    for m in &modules {
        let _ = writeln!(out, "    &{m}::FILE,");
    }
    out.push_str("];\n");
    let tmp = dir.join(format!("mod.rs.{}", std::process::id()));
    std::fs::write(&tmp, out).map_err(|e| failed(format!("cannot write the index: {e}")))?;
    std::fs::rename(&tmp, dir.join("mod.rs"))
        .map_err(|e| failed(format!("cannot write the index: {e}")))
}
