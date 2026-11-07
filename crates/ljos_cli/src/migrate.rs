//! `ljos migrate`: rewrites files written in Qt's QML in Ljós's own API
//! (`Ljos.Ui`), with the edits `--compat=lower` makes as a file loads
//! (docs/DIALECT.md, docs/adr/0004-compat-lowering-before-the-ir.md). The
//! two are one pass, so a migrated file is the file a lowered run compiled.
//! Its import line says `pragma Scripts: JavaScript`, so its scripts are not
//! held to the checker's rules until a person takes the pragma out.
//!
//! | To | What |
//! |---|---|
//! | stdout | `lowered <file> (N edits)` (or `would lower`) for each file the pass lowers, `kept <file>:<line>:<column>: <why>` for each it cannot lower whole, then `N files, M lowered, K kept` |
//! | stderr | `<file>: <error>` for a file that could not be read or written |
//!
//! | Status | Means |
//! |---|---|
//! | 0 | Every file was read (a kept file is not a failure: it runs as written). |
//! | 2 | A file could not be read or written, or a directory read as a project; the others were still handled. |

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use clap::Args;
use ljos::dev::EngineOracle;
use ljos_compiler::Project;
use qml_compat::Outcome;

use crate::render::oracle_for_modules;
use crate::stdout::outln;
use crate::walk::{files_below, is_qml};

#[derive(Args)]
pub struct Migrate {
    /// The files to migrate, or directories for every `.qml` file below
    /// them.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    /// Write each lowered file in place. Without it nothing is changed and
    /// the command says what it would do.
    #[arg(long)]
    write: bool,
}

impl Migrate {
    /// The exit status.
    pub fn run(self) -> u8 {
        let (mut lowered, mut kept, mut failed) = (0, 0, false);
        // Each file with the project it is of: the directory named, or the
        // directory of a file named, read as a lowered run reads it, so
        // the modules and directories a file imports are files to it.
        let mut files: Vec<(PathBuf, usize)> = Vec::new();
        let mut roots: Vec<PathBuf> = Vec::new();
        for path in &self.paths {
            let root = if path.is_dir() {
                path.clone()
            } else {
                path.parent().unwrap_or(Path::new(".")).to_owned()
            };
            let at = roots.iter().position(|r| *r == root).unwrap_or_else(|| {
                roots.push(root);
                roots.len() - 1
            });
            if path.is_dir() {
                let mut below = Vec::new();
                if let Err(e) = files_below(path, is_qml, &mut below) {
                    eprintln!("{}: {e}", path.display());
                    failed = true;
                }
                files.extend(below.into_iter().map(|file| (file, at)));
            } else {
                files.push((path.clone(), at));
            }
        }
        let projects: Vec<Option<Project<EngineOracle>>> = roots
            .iter()
            .map(|root| match Project::load(root, oracle_for_modules()) {
                Ok(project) => Some(project),
                Err(e) => {
                    eprintln!("{}: {e}", root.display());
                    failed = true;
                    None
                }
            })
            .collect();
        // Every file first: files that name one another are lowered
        // together, as a lowered run lowers them (`qml_compat::group`).
        let mut alone = BTreeMap::new();
        let mut texts = BTreeMap::new();
        for (file, at) in &files {
            let text = match std::fs::read_to_string(file) {
                Ok(text) => text,
                Err(e) => {
                    eprintln!("{}: {e}", file.display());
                    failed = true;
                    continue;
                }
            };
            let root = &roots[*at];
            let mut one = match &projects[*at] {
                Some(project) => project.compat_alone(&key_in(root, file), &text),
                None => qml_compat::Alone {
                    outcome: qml_compat::lower(&text, &|_: &str| None),
                    references: qml_compat::references(&text),
                    uses: Vec::new(),
                },
            };
            // The project's keys are from its root; these are the paths.
            for used in &mut one.uses {
                *used = key_of(&root.join(format!("{used}.qml")));
            }
            let key = key_of(file);
            alone.insert(key.clone(), one);
            texts.insert(key, (file, text));
        }
        for (key, outcome) in qml_compat::group(&alone) {
            let (file, text) = &texts[&key];
            match outcome {
                Outcome::Unchanged => {}
                Outcome::Kept(why) => {
                    kept += 1;
                    let (line, column) = line_column(text, why.at);
                    outln!("kept {}:{line}:{column}: {}", file.display(), why.reason);
                }
                Outcome::Lowered(done) => {
                    lowered += 1;
                    let edits = done.changes.len();
                    if self.write {
                        if let Err(e) = std::fs::write(file, &done.text) {
                            eprintln!("{}: {e}", file.display());
                            failed = true;
                            continue;
                        }
                        outln!("lowered {} ({edits} edits)", file.display());
                    } else {
                        outln!("would lower {} ({edits} edits)", file.display());
                    }
                }
            }
        }
        outln!("{} files, {lowered} lowered, {kept} kept", files.len());
        if failed { 2 } else { 0 }
    }
}

/// A file's key for [`qml_compat::group`]: its path without `.qml`, with
/// `/` between the parts on every system.
fn key_of(file: &Path) -> String {
    let path = file.with_extension("");
    let parts: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let joined = parts.join("/");
    // A root `/` is a part of its own.
    joined
        .strip_prefix("//")
        .map_or(joined.clone(), |rest| format!("/{rest}"))
}

/// The key of `file` in the project under `root`: its path from there
/// without `.qml`, with `/` between the parts.
fn key_in(root: &Path, file: &Path) -> String {
    let path = file.strip_prefix(root).unwrap_or(file).with_extension("");
    let parts: Vec<String> = path
        .components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.join("/")
}

/// The 1-based line and column (in characters) of byte `at` of `text`.
fn line_column(text: &str, at: usize) -> (usize, usize) {
    let before = &text[..at.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[start..].chars().count() + 1)
}
