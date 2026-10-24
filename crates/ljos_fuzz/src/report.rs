//! A bucket's repro on disk: what a person needs to see the difference and
//! to turn it into a scenario.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::json;

use crate::{Error, area::Area, compare::Diff};

/// A directory name for a signature.
#[must_use]
pub fn dir_name(signature: &str) -> String {
    signature
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Writes the repro of `case` into `out/<signature>/` and returns that
/// directory: `Case.qml` (what both sides ran), `program.qml` (the same
/// without the observation helpers), `steps.txt`, `observations.json` and a
/// draft `scenario.feature`.
///
/// # Errors
/// When the directory or a file cannot be written.
pub fn write<A: Area>(
    area: &A,
    out: &Path,
    signature: &str,
    case: &A::Case,
    diff: &Diff,
) -> Result<PathBuf, Error> {
    let dir = out.join(dir_name(signature));
    let io = |path: &Path| {
        let path = path.to_owned();
        move |source| Error::Io { path, source }
    };
    fs::create_dir_all(&dir).map_err(io(&dir))?;
    let files = [
        ("Case.qml", area.render(case).qml),
        ("program.qml", area.program(case)),
        (
            "steps.txt",
            area.steps(case).iter().map(|s| format!("{s}\n")).collect(),
        ),
        (
            "observations.json",
            serde_json::to_string_pretty(&json!({
                "signature": signature,
                "kind": diff.kind.label(),
                "phase": diff.phase,
                "key": diff.key,
                "qt": diff.qt,
                "ljos": diff.ljos,
            }))
            .unwrap_or_default()
                + "\n",
        ),
        ("scenario.feature", area.scenario(case, diff, signature)),
    ];
    for (name, text) in files {
        let path = dir.join(name);
        fs::write(&path, text).map_err(io(&path))?;
    }
    Ok(dir)
}
