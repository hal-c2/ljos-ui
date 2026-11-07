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
//!
//! A file is lowered with the files it names and that name it (`qml_compat::group`). When that
//! group holds a file that was not named, none of it is written: the named file is kept, saying
//! which file to name too, as the one would be renamed to match the other lowered while the other
//! stays Qt's.
//!
//! | stderr | `<file>: <error>` for a file that could not be read or written |
//!
//! | Status | Means |
//! |---|---|
//! | 0 | Every file was read (a kept file is not a failure: it runs as written). |
//! | 2 | A file could not be read or written, or a directory read as a project; the others were still handled. |

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use clap::Args;
use ljos::dev::EngineOracle;
use ljos_compiler::Project;
use qml_compat::Outcome;
use qml_front::project::ControlsStyle;

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
    /// The controls style the program starts with, which is what
    /// `import QtQuick.Controls` means: a file that imports it is lowered
    /// only where that is Basic, the style `Ljos.Controls` draws. Without
    /// it, the style a program started here would get
    /// (`QT_QUICK_CONTROLS_STYLE`, `QT_QUICK_CONTROLS_CONF`, the platform's).
    #[arg(long, value_name = "NAME")]
    style: Option<String>,
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
        // The roots a file was named in: the other files there are read too.
        let mut around: BTreeSet<usize> = BTreeSet::new();
        for path in &self.paths {
            let root = if path.is_dir() {
                path.clone()
            } else {
                // `Main.qml`'s parent is the empty path: the directory the
                // command runs in.
                path.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."))
                    .to_owned()
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
                around.insert(at);
                files.push((path.clone(), at));
            }
        }
        // A project loaded starts with the style a program started here
        // would get; `--style` chooses another.
        let chosen = self.style.clone().map(|name| ControlsStyle {
            name,
            fallback: None,
        });
        let projects: Vec<Option<Project<EngineOracle>>> = roots
            .iter()
            .map(|root| match Project::load(root, oracle_for_modules()) {
                Ok(mut project) => {
                    if chosen.is_some() {
                        project.set_controls_style(chosen.clone());
                    }
                    Some(project)
                }
                Err(e) => {
                    eprintln!("{}: {e}", root.display());
                    failed = true;
                    None
                }
            })
            .collect();
        // For a root that did not load: the same style, with nothing beside.
        let style = self.style.clone().unwrap_or_else(|| {
            ljos_compiler::controls_style(
                |name| std::env::var(name).ok(),
                |path| std::fs::read_to_string(path).ok(),
            )
            // No style chosen is Basic, as in Qt 6.
            .map_or_else(|| qml_compat::CONTROLS_STYLE.to_owned(), |style| style.name)
        });
        // What the pass makes of `file` alone, with the files of its uses
        // keyed by their paths.
        let alone_of = |file: &Path, at: usize, text: &str| {
            let root = &roots[at];
            let mut one = match &projects[at] {
                Some(project) => project.compat_alone(&key_in(root, file), text),
                None => qml_compat::Alone {
                    outcome: qml_compat::lower(
                        text,
                        &qml_compat::Styled {
                            locals: &|_: &str| None,
                            style: &style,
                        },
                    ),
                    references: qml_compat::references(text),
                    uses: Vec::new(),
                },
            };
            // The project's keys are from its root; these are the paths.
            for used in &mut one.uses {
                *used = key_of(&root.join(format!("{used}.qml")));
            }
            one
        };
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
            let key = key_of(file);
            alone.insert(key.clone(), (alone_of(file, *at, &text), *at));
            texts.insert(key, (file, text));
        }
        // Then the files a group may hold that were not named: the others
        // beside a file named, and every file a file uses, until no more
        // are found. Each is read, never written: one the pass would lower
        // keeps its group, so a named file is not lowered to match a file
        // that stays Qt's.
        let mut unnamed: Vec<(PathBuf, usize)> = Vec::new();
        for &at in &around {
            let mut below = Vec::new();
            // A file that cannot be read is said where it is named.
            let _ = files_below(&roots[at], is_qml, &mut below);
            unnamed.extend(below.into_iter().map(|file| (file, at)));
        }
        loop {
            let used: Vec<(PathBuf, usize)> = alone
                .values()
                .flat_map(|(one, at): &(qml_compat::Alone, usize)| {
                    one.uses
                        .iter()
                        .map(move |key| (PathBuf::from(format!("{key}.qml")), *at))
                })
                .filter(|(file, _)| !alone.contains_key(&key_of(file)))
                .collect();
            unnamed.extend(used);
            let mut found = false;
            for (file, at) in std::mem::take(&mut unnamed) {
                let key = key_of(&file);
                if alone.contains_key(&key) {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&file) else {
                    continue;
                };
                let mut one = alone_of(&file, at, &text);
                if let Outcome::Lowered(_) = one.outcome {
                    one.outcome = Outcome::Kept(qml_compat::Kept {
                        reason: "it is not named: name it too".into(),
                        at: 0,
                    });
                }
                alone.insert(key, (one, at));
                found = true;
            }
            if !found {
                break;
            }
        }
        let alone: BTreeMap<String, qml_compat::Alone> = alone
            .into_iter()
            .map(|(key, (one, _))| (key, one))
            .collect();
        for (key, outcome) in qml_compat::group(&alone) {
            let Some((file, text)) = texts.get(&key) else {
                continue;
            };
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
        .filter(|c| !matches!(c, std::path::Component::CurDir))
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
