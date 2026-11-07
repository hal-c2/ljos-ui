//! `ljos migrate`: rewrites files written in Qt's QML in Ljós's own API
//! (`Ljos.Ui`), with the edits `--compat=lower` makes as a file loads
//! (docs/DIALECT.md, docs/adr/0004-compat-lowering-before-the-ir.md). The
//! two are one pass, so a migrated file is the file a lowered run compiled.
//! Its import line says `pragma Scripts: JavaScript`, so its scripts are not
//! held to the checker's rules until a person takes the pragma out.
//!
//! | To | What |
//! |---|---|
//! | stdout | `lowered <file> (N edits)` (or `would lower`) for each file the pass lowers, then `wrote <module>` (or `would write`) for each module beside it the lowered file imports, `kept <file>:<line>:<column>: <why>` for each it cannot lower whole, then `N files, M lowered, K kept` |
//! | stderr | `<file>: <error>` for a file that could not be read or written, or a module that would overwrite another file |
//!
//! A file's constant `ListModel`s are written as typed lists, with their
//! row types in `<File>.rows.ts` beside it (docs/DIALECT.md §4.8). A file
//! of that name that is not that module is left as it is, and so is the
//! file that would import it.
//!
//! A file is lowered with the files it names and that name it (`qml_compat::group`). When that
//! group holds a file that was not named, none of it is written: the named file is kept, saying
//! which file to name too, as the one would be renamed to match the other lowered while the other
//! stays Qt's. A file is read with the project of its directory, or, in a module's directory, of
//! the directory the module is imported from. The files of a group that were not named are looked
//! for below those projects, each read with its project, and below the directory the command
//! runs in, where only a file that imports a named file's directory (by its path or its module) is
//! read: a file that imports one named from anywhere else is not seen.
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
        // for: below each root of a file named, each read with that root's
        // project, as a module on its import path is a file to every file
        // below it.
        let named = roots.len();
        let mut unnamed: Vec<(PathBuf, usize)> = Vec::new();
        let mut seen = BTreeSet::new();
        for (at, root) in roots.iter().enumerate() {
            let mut below = Vec::new();
            // A file that cannot be read is said where it is named.
            let _ = files_below(root, is_qml, &mut below);
            for file in below {
                if seen.insert(absolute(&file)) {
                    unnamed.push((file, at));
                }
            }
        }
        // And below the directory the command runs in, where the files that
        // import a directory by its path are, unless a root already holds
        // it. Only a file whose imports name a named file's directory is
        // read, all with the one project of that directory.
        if let Some(here) = std::env::current_dir().ok().map(|dir| absolute(&dir))
            && !roots.iter().any(|root| here.starts_with(root))
        {
            let mut below = Vec::new();
            let _ = files_below(&here, is_qml, &mut below);
            let importers = importers(
                &files,
                &here,
                below.into_iter().filter(|f| !seen.contains(&absolute(f))),
            );
            if !importers.is_empty() {
                let at = at_of(root_of(&here), &mut roots);
                unnamed.extend(importers.into_iter().map(|file| (file, at)));
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
            .enumerate()
            .map(
                |(at, root)| match Project::load(root, oracle_for_modules()) {
                    Ok(mut project) => {
                        if chosen.is_some() {
                            project.set_controls_style(chosen.clone());
                        }
                        Some(project)
                    }
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
                Some(project) => project.compat_alone(&key_in(root, &absolute(file)), text),
                None => {
                    let locals = qml_compat::Styled {
                        locals: &|_: &str| None,
                        style: &style,
                    };
                    let outcome = match file.file_stem() {
                        Some(stem) => qml_compat::lower_as(text, &stem.to_string_lossy(), &locals),
                        None => qml_compat::lower(text, &locals),
                    };
                    qml_compat::Alone {
                        outcome,
                        references: qml_compat::references(text),
                        uses: Vec::new(),
                    }
                }
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
                    let dir = file.parent().unwrap_or(Path::new("."));
                    let modules: Vec<(PathBuf, &String)> = done
                        .modules
                        .iter()
                        .map(|(name, text)| (dir.join(name), text))
                        .collect();
                    // Only a module that is not there is free to write: one
                    // that cannot be read is someone's too.
                    if let Some((path, _)) =
                        modules
                            .iter()
                            .find(|(path, text)| match std::fs::read(path) {
                                Ok(there) => there != text.as_bytes(),
                                Err(e) => e.kind() != std::io::ErrorKind::NotFound,
                            })
                    {
                        eprintln!(
                            "{}: is there already, and is not the module {} would import; neither is written",
                            path.display(),
                            file.display()
                        );
                        failed = true;
                        continue;
                    }
                    // One there already is the module to the byte: it stays
                    // as it is, untouched.
                    let modules: Vec<(PathBuf, &String)> = modules
                        .into_iter()
                        .filter(|(path, _)| !path.exists())
                        .collect();
                    lowered += 1;
                    let edits = done.changes.len();
                    if self.write {
                        // The modules first: one that fails to be written
                        // leaves the file as it was, never a file that
                        // imports a module that is not there.
                        let mut wrote = Vec::new();
                        for (path, text) in &modules {
                            if let Err(e) = std::fs::write(path, text) {
                                eprintln!("{}: {e}", path.display());
                                failed = true;
                                break;
                            }
                            wrote.push(path);
                        }
                        if wrote.len() < modules.len() {
                            continue;
                        }
                        if let Err(e) = std::fs::write(file, &done.text) {
                            eprintln!("{}: {e}", file.display());
                            failed = true;
                            continue;
                        }
                        outln!("lowered {} ({edits} edits)", file.display());
                        for path in wrote {
                            outln!("wrote {}", path.display());
                        }
                    } else {
                        outln!("would lower {} ({edits} edits)", file.display());
                        for (path, _) in &modules {
                            outln!("would write {}", path.display());
                        }
                    }
                }
            }
        }
        outln!("{} files, {lowered} lowered, {kept} kept", files.len());
        if failed { 2 } else { 0 }
    }
}

/// The files of `candidates` whose imports name the directory of a file of
/// `named`: by its path (`import "parts"`), or by the module its `qmldir`
/// or build says it is (`import Gauges`). The others are not read further, so a
/// tree around the files named costs a parse of each file, not a project.
fn importers(
    named: &[(PathBuf, usize)],
    here: &Path,
    candidates: impl Iterator<Item = PathBuf>,
) -> Vec<PathBuf> {
    let dirs: BTreeSet<PathBuf> = named
        .iter()
        .filter_map(|(file, _)| absolute(file).parent().map(Path::to_owned))
        .collect();
    // A module the file is of: one described in its directory, or one a
    // directory above it (up to `here`) describes with the file among its
    // types, as a build may list a file from a subdirectory.
    let mut modules = BTreeSet::new();
    // Named files share their directories and those above them: each is
    // read once.
    let mut described: BTreeMap<PathBuf, Vec<qml_front::discover::Module>> = BTreeMap::new();
    for (file, _) in named {
        let file = absolute(file);
        let Some(own) = file.parent() else { continue };
        for dir in own.ancestors() {
            if dir != own && !dir.starts_with(here) {
                break;
            }
            let found = described
                .entry(dir.to_owned())
                .or_insert_with(|| ljos_compiler::described_in(dir));
            for module in found.iter() {
                let holds = dir == own
                    || module
                        .types
                        .iter()
                        .any(|ty| absolute(&dir.join(&ty.path)) == file);
                if holds {
                    modules.insert(module.uri.clone());
                }
            }
        }
    }
    candidates
        .filter(|file| {
            let Ok(text) = std::fs::read_to_string(file) else {
                return false;
            };
            let found = qml_compat::references(&text);
            let beside = absolute(file);
            let beside = beside.parent().unwrap_or(Path::new("/"));
            found
                .dirs
                .iter()
                .any(|dir| dirs.contains(&absolute(&beside.join(dir))))
                || found.modules.iter().any(|module| modules.contains(module))
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Below the directory the command runs in, only a file that imports a
    /// named file's directory is read: by its path or by its module.
    #[test]
    fn only_a_file_that_imports_a_named_directory_is_an_importer() {
        let dir = tempfile::tempdir().unwrap();
        let at = |name: &str, text: &str| {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        };
        at("lib/Gauges/qmldir", "module Gauges\nDial 1.0 Dial.qml\n");
        let dial = at("lib/Gauges/Dial.qml", "import QtQuick\nRectangle {}\n");
        let by_path = at(
            "a/Main.qml",
            "import QtQuick\nimport \"../lib/Gauges\"\nItem {}\n",
        );
        let by_module = at("b/Main.qml", "import QtQuick\nimport Gauges\nItem {}\n");
        let other = at("c/Main.qml", "import QtQuick\nimport \"parts\"\nItem {}\n");
        let found = importers(
            &[(dial, 0)],
            &absolute(dir.path()),
            [by_path.clone(), by_module.clone(), other].into_iter(),
        );
        assert_eq!(found, [by_path, by_module]);
    }
}
