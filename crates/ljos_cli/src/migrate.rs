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
//! A file is lowered with the files it names and that name it (`qml_compat::group`). When that
//! group holds a file that was not named, none of it is written: the named file is kept, saying
//! which file to name too, as the one would be renamed to match the other lowered while the other
//! stays Qt's. A file is read with the project of its directory, or, in a module's directory, of
//! the directory the module is imported from. The files of a group that were not named are looked
//! for below those projects and below the directory the command runs in: a file that imports one
//! named from anywhere else is not seen.
//!
//! | Status | Means |
//! |---|---|
//! | 0 | Every file was read (a kept file is not a failure: it runs as written). |
//! | 2 | A file could not be read or written, or a directory read as a project; the others were still handled. |

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
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
        // directory of a file named, or the directory its module is
        // imported from (`root_of`), read as a lowered run reads it, so the
        // modules and directories a file imports are files to it. Roots
        // and keys are from absolute paths, so a file is the same file
        // however it is named.
        let mut files: Vec<(PathBuf, usize)> = Vec::new();
        let mut roots: Vec<PathBuf> = Vec::new();
        let at_of = |root: PathBuf, roots: &mut Vec<PathBuf>| {
            roots.iter().position(|r| *r == root).unwrap_or_else(|| {
                roots.push(root);
                roots.len() - 1
            })
        };
        for path in &self.paths {
            if path.is_dir() {
                let at = at_of(root_of(&absolute(path)), &mut roots);
                let mut below = Vec::new();
                if let Err(e) = files_below(path, is_qml, &mut below) {
                    eprintln!("{}: {e}", path.display());
                    failed = true;
                }
                files.extend(below.into_iter().map(|file| (file, at)));
            } else {
                let file = absolute(path);
                let dir = file.parent().unwrap_or(&file).to_owned();
                files.push((path.clone(), at_of(root_of(&dir), &mut roots)));
            }
        }
        // Where the files a group may hold that were not named are looked
        // for: below each root of a file named, and below the directory
        // the command runs in, where the files that import a directory by
        // its path are. Each is read with the project it is of.
        let named = roots.len();
        let mut unnamed: Vec<(PathBuf, usize)> = Vec::new();
        let mut around: Vec<PathBuf> = roots.clone();
        around.extend(std::env::current_dir().ok().map(|dir| absolute(&dir)));
        let mut seen = BTreeSet::new();
        for dir in around {
            let mut below = Vec::new();
            // A file that cannot be read is said where it is named.
            let _ = files_below(&dir, is_qml, &mut below);
            for file in below {
                if seen.insert(file.clone()) {
                    let root = root_of(file.parent().unwrap_or(&file));
                    unnamed.push((file, at_of(root, &mut roots)));
                }
            }
        }
        let projects: Vec<Option<Project<EngineOracle>>> = roots
            .iter()
            .enumerate()
            .map(
                |(at, root)| match Project::load(root, oracle_for_modules()) {
                    Ok(project) => Some(project),
                    // A project only searched for importers is not the
                    // command's to fail on.
                    Err(_) if at >= named => None,
                    Err(e) => {
                        eprintln!("{}: {e}", root.display());
                        failed = true;
                        None
                    }
                },
            )
            .collect();
        // What the pass makes of `file` alone, with the files of its uses
        // keyed by their paths.
        let alone_of = |file: &Path, at: usize, text: &str| {
            let root = &roots[at];
            let mut one = match &projects[at] {
                Some(project) => project.compat_alone(&key_in(root, &absolute(file)), text),
                None => qml_compat::Alone {
                    outcome: qml_compat::lower(text, &|_: &str| None),
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
        // Then the files found around them, and every file a file uses,
        // until no more are found. Each is read, never written: one the
        // pass would lower keeps its group, so a named file is not lowered
        // to match a file that stays Qt's.
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

/// The project `dir`'s files are of: the directory a module in `dir` is
/// imported from, when its `qmldir` names one whose parts are the
/// directories it is in (`lib/Ui/Gauges` holding `module Ui.Gauges` is
/// imported from `lib`, with the files that import it), and `dir` itself
/// otherwise.
fn root_of(dir: &Path) -> PathBuf {
    let import_root = || {
        let text = std::fs::read_to_string(dir.join("qmldir")).ok()?;
        let uri = qml_front::discover::qmldir(&text)?.uri;
        let mut root = dir;
        for part in uri.rsplit('.') {
            if root.file_name()? != part {
                return None;
            }
            root = root.parent()?;
        }
        Some(root)
    };
    import_root().unwrap_or(dir).to_owned()
}

/// `path` from the root of the file system, with `.` and `..` taken out
/// by its words alone, as the path a person wrote names it.
fn absolute(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            part => out.push(part),
        }
    }
    out
}

/// A file's key for [`qml_compat::group`]: its absolute path without
/// `.qml`, with `/` between the parts on every system.
fn key_of(file: &Path) -> String {
    let path = absolute(file).with_extension("");
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
        .filter(|c| !matches!(c, Component::CurDir))
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
