//! `ljos migrate`: rewrites files written in Qt's QML in Ljós's own API
//! (`Ljos.Ui`), with the edits `--compat=lower` makes as a file loads
//! (docs/DIALECT.md, docs/adr/0004-compat-lowering-before-the-ir.md). The
//! two are one pass, so a migrated file compiles to the program a lowered
//! run compiled. It is written as a file in Ljós's API is: each import on a
//! line of its own, and typed where its scripts already keep the checker's
//! rules. Where they do not, it says `pragma Scripts: JavaScript`, so they
//! are not held to the rules until a person types them and takes the pragma
//! out; `ljos check` without it says what to change.
//!
//! | To | What |
//! |---|---|
//! | stdout | `renamed <module> to <name>.lts, and the imports that name it in <files>; files outside <directory> were not looked in` (or `would rename`) first, for each `.ts` module that uses Ljós's `match` or `is` (`ts-lts-syntax`), or `kept <module>: it would be renamed to <name>.lts, but <name>.lts is in the way`; then `lowered <file> (N edits)` (or `would lower`) for each file the pass lowers, then `wrote <module>` (or `would write`) for each module beside it the lowered file imports, and `scripts of <file> stay JavaScript: typed, ljos check finds N` where its scripts break the checker's rules, `kept <file>:<line>:<column>: <why>` for each it cannot lower whole, then `N files, M lowered, K kept` |
//! | stderr | `<file>: <error>` for a file that could not be read or written, or a module that would overwrite another file, or a description of the program's types that could not be read (one found beside the files is named with its directory) |
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
//! | 2 | A file could not be read or written, or a directory read as a project; the others were still handled. Or the program's types could not be read, and nothing was. |

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use clap::Args;
use ljos::dev::EngineOracle;
use ljos_compiler::{
    Project, Roots,
    roots::{absolute, file_key},
};
use qml_compat::Outcome;
use qml_front::{TypeOracle, project::ControlsStyle};

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
    /// only where that is Basic, the style `Ljos.Controls.Basic` draws. Without
    /// it, the style a program started here would get
    /// (`QT_QUICK_CONTROLS_STYLE`, `QT_QUICK_CONTROLS_CONF`, the platform's).
    #[arg(long, value_name = "NAME")]
    style: Option<String>,
    /// The program's own types, as `ljos run --types` reads them: a file
    /// that imports a module of them is lowered against them. Without it,
    /// the `ljostypes.json` of the first path given (of a file, its
    /// directory), if it has one; the other paths are not looked in.
    #[arg(long, value_name = "FILE")]
    types: Option<PathBuf>,
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
        // The `.ts` modules that use Ljós's syntax are renamed to `.lts`,
        // with the root of the project they are of.
        let mut modules: Vec<(PathBuf, usize)> = Vec::new();
        for path in &self.paths {
            if path.is_dir() {
                let at = at_of(root_of(&absolute(path)), &mut roots);
                let mut below = Vec::new();
                // A directory that cannot be read is said below.
                let _ = files_below(path, is_ts_module, &mut below);
                modules.extend(below.into_iter().map(|file| (file, at)));
            } else if is_ts_module(path) {
                let file = absolute(path);
                let dir = file.parent().unwrap_or(&file).to_owned();
                modules.push((path.clone(), at_of(root_of(&dir), &mut roots)));
                continue;
            }
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
        let here = std::env::current_dir().ok().map(|dir| absolute(&dir));
        for (module, at) in &modules {
            failed |= !rename_lts(&roots[*at], here.as_deref(), module, self.write);
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
        if let Some(here) = here
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
        // The program's own types, where they are described: each project
        // is checked against them, and a file that imports a module of
        // them is lowered against them.
        let oracle = match self.oracle() {
            Ok(oracle) => oracle,
            Err(e) => {
                eprintln!("{e:#}");
                return 2;
            }
        };
        let mut around = Roots::new();
        for (at, root) in roots.iter().enumerate() {
            let oracle = || oracle.clone().unwrap_or_else(oracle_for_modules);
            let project = match Project::load(root, oracle()) {
                Ok(mut project) => {
                    if chosen.is_some() {
                        project.set_controls_style(chosen.clone());
                    }
                    project
                }
                Err(e) => {
                    // A project only searched for importers is not the
                    // command's to fail on.
                    if at < named {
                        eprintln!("{}: {e}", root.display());
                        failed = true;
                    }
                    // A root that did not load is a project with nothing
                    // in it: its files have nothing beside them, and the
                    // style a loaded one would have.
                    let mut project = Project::new(oracle());
                    project.set_controls_style(chosen.clone().or_else(|| {
                        ljos_compiler::controls_style(
                            |name| std::env::var(name).ok(),
                            |path| std::fs::read_to_string(path).ok(),
                        )
                    }));
                    project
                }
            };
            around.push(root.clone(), project);
        }
        // Every file first: files that name one another are lowered
        // together, as a lowered run lowers them (`qml_compat::Lowering`).
        let mut lowering = qml_compat::Lowering::new();
        let mut shown = BTreeMap::new();
        for (file, at) in &files {
            let text = match std::fs::read_to_string(file) {
                Ok(text) => text,
                Err(e) => {
                    eprintln!("{}: {e}", file.display());
                    failed = true;
                    continue;
                }
            };
            let key = around.place(file, *at);
            lowering.put(&key, text);
            shown.insert(key, file);
        }
        // Then the files found around them, and every file a file uses,
        // until no more are found. Each is read, never written: one the
        // pass would lower keeps its group, so a named file is not lowered
        // to match a file that stays Qt's.
        loop {
            lowering.lower(&around);
            let used: Vec<(PathBuf, usize)> = lowering
                .uses()
                .filter_map(|(user, used)| {
                    Some((PathBuf::from(format!("{used}.qml")), around.root_of(user)?))
                })
                .filter(|(file, _)| lowering.written(&file_key(file)).is_none())
                .collect();
            unnamed.extend(used);
            let mut found = false;
            for (file, at) in std::mem::take(&mut unnamed) {
                if lowering.written(&file_key(&file)).is_some() {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&file) else {
                    continue;
                };
                let key = around.place(&file, at);
                lowering.put_read_only(&key, text);
                found = true;
            }
            if !found {
                break;
            }
        }
        // The modules the lowered files import are written below, to the
        // disk, by rules of the command's own.
        let _ = lowering.settle(&around);
        let typed = findings_typed(&lowering, &mut around, shown.keys());
        for (key, file) in shown {
            let (Some(outcome), Some(text)) = (lowering.lowering(&key), lowering.written(&key))
            else {
                continue;
            };
            match outcome {
                Outcome::Unchanged => {}
                Outcome::Kept(why) => {
                    kept += 1;
                    let (line, column) = line_column(text, why.at);
                    outln!("kept {}:{line}:{column}: {}", file.display(), why);
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
                    // A file nothing compiled typed is not known to keep
                    // the checker's rules: it stays JavaScript.
                    let findings = typed.get(&key).copied();
                    let migrated = qml_compat::migrated(text, &done.text, findings == Some(0));
                    let javascript = || {
                        if let Some(findings @ 1..) = findings {
                            outln!(
                                "scripts of {} stay JavaScript: typed, ljos check finds {findings}",
                                file.display()
                            );
                        }
                    };
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
                        if let Err(e) = std::fs::write(file, &migrated) {
                            eprintln!("{}: {e}", file.display());
                            failed = true;
                            continue;
                        }
                        outln!("lowered {} ({edits} edits)", file.display());
                        for path in wrote {
                            outln!("wrote {}", path.display());
                        }
                        javascript();
                    } else {
                        outln!("would lower {} ({edits} edits)", file.display());
                        for (path, _) in &modules {
                            outln!("would write {}", path.display());
                        }
                        javascript();
                    }
                }
            }
        }
        outln!("{} files, {lowered} lowered, {kept} kept", files.len());
        if failed { 2 } else { 0 }
    }
}

/// Per lowered file of `keys`, what the checker finds in it typed, without
/// `pragma Scripts: JavaScript`, that it does not find in it with the
/// pragma: the findings of its scripts that typing it would add. Where that
/// is none the file is written typed, as it compiles to the same program
/// either way (the pragma only turns the checker's rules off).
///
/// Each file is compiled as it will be written: every lowered file is put in
/// its project first, as JavaScript, with the modules the pass writes beside
/// it, so a file is checked against the files and rows it will be read with.
/// A finding is told from another by its severity, code and message, not
/// where it is, as the pragma's line moves the lines below it.
fn findings_typed<O: TypeOracle>(
    lowering: &qml_compat::Lowering,
    around: &mut Roots<O>,
    keys: impl Iterator<Item = impl AsRef<str>>,
) -> BTreeMap<String, usize> {
    let lowered: Vec<(String, &str, &qml_compat::Lowered)> = keys
        .filter_map(|key| {
            let key = key.as_ref();
            match lowering.lowering(key)? {
                Outcome::Lowered(done) => Some((key.to_owned(), lowering.written(key)?, done)),
                _ => None,
            }
        })
        .collect();
    for (key, written, done) in &lowered {
        let Some((project, in_project)) = around.project_mut(key) else {
            continue;
        };
        let in_project = in_project.to_owned();
        for (name, text) in &done.modules {
            let path = match in_project.rsplit_once('/') {
                Some((dir, _)) => format!("{dir}/{name}"),
                None => name.clone(),
            };
            project.add_script(&path, text.clone());
        }
        project.add(
            &in_project,
            qml_compat::migrated(written, &done.text, false),
        );
    }
    let findings = |compiled: Result<ljos_compiler::Compiled, Vec<ljos_compiler::Diagnostic>>| {
        let mut found: BTreeMap<String, usize> = BTreeMap::new();
        let mut count = |d: &ljos_compiler::Diagnostic| {
            *found
                .entry(format!("{:?} {} {}", d.severity, d.code, d.message))
                .or_default() += 1;
        };
        match &compiled {
            Ok(compiled) => compiled.all_diagnostics().for_each(|(_, d)| count(d)),
            Err(diagnostics) => diagnostics.iter().for_each(count),
        }
        found
    };
    lowered
        .iter()
        .filter_map(|(key, written, done)| {
            let (project, in_project) = around.project_mut(key)?;
            let in_project = in_project.to_owned();
            let javascript = findings(ljos_compiler::compile_file(project, &in_project));
            let typed = qml_compat::migrated(written, &done.text, true);
            let typed = findings(ljos_compiler::compile_text(project, &in_project, &typed));
            let more = typed
                .iter()
                .map(|(finding, n)| n.saturating_sub(javascript.get(finding).copied().unwrap_or(0)))
                .sum();
            Some((key.clone(), more))
        })
        .collect()
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

impl Migrate {
    /// What the checker is told of the program's own types, where they
    /// are described: the file `--types` names, else the `ljostypes.json`
    /// of the first path given (of a file, its directory). The other paths
    /// are not looked in, so which description a file is lowered against
    /// does not depend on what the paths after the first hold.
    fn oracle(&self) -> anyhow::Result<Option<EngineOracle>> {
        let file = match self.types.as_deref() {
            Some(named) => crate::run::types_file(Some(named), Path::new(".")),
            None => self.paths.first().and_then(|path| {
                let dir = if path.is_dir() {
                    path.as_path()
                } else {
                    path.parent().unwrap_or(Path::new("."))
                };
                let dir = if dir.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    dir
                };
                // Named with its directory, as it was not named by the
                // person running the command.
                crate::run::types_file(None, dir)
                    .map(|(found, _)| (found.clone(), found.display().to_string()))
            }),
        };
        crate::run::read_types(file)?
            .map(|types| crate::run::oracle_for_types(&types))
            .transpose()
    }
}

/// The 1-based line and column (in characters) of byte `at` of `text`.
fn line_column(text: &str, at: usize) -> (usize, usize) {
    let before = &text[..at.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[start..].chars().count() + 1)
}

/// A `.ts` module, not a declaration.
fn is_ts_module(path: &Path) -> bool {
    let name = path.to_string_lossy();
    name.ends_with(".ts") && !name.ends_with(".d.ts")
}

/// A file that may import a module: a QML file, a script, a module or a
/// `qmldir`.
fn imports_modules(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "qmldir")
        || path
            .extension()
            .is_some_and(|e| ["qml", "js", "mjs", "ts", "lts"].iter().any(|x| e == *x))
}

/// The key of `file` from `root`, `/`-separated.
fn key_below(root: &Path, file: &Path) -> Option<String> {
    let rest = absolute(file);
    let rest = rest.strip_prefix(root).ok()?;
    Some(
        rest.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// Renames `module`, a `.ts` module of the project at `root`, to `.lts`
/// when it uses Ljós's syntax (`ts-lts-syntax`), and has each file that
/// imports it import the new name. Importers are looked for below the
/// directory the command runs in, `here`, when `root` is in it, as a file
/// beside the directory named may import the module; else below `root`.
/// The output says which, as a file outside it is not changed. The module
/// is renamed first, then its importers are written in the order of their
/// paths; when one cannot be written, those written get their text back and
/// the module its name, so a failure leaves the files as they were. Whether
/// nothing failed.
///
/// DECISION: a module whose `.lts` name is taken is not renamed and is
/// said as kept, as `module-clash` and `ts-lts-syntax` say it in the
/// check; it is not counted with the QML files kept.
fn rename_lts(root: &Path, here: Option<&Path>, module: &Path, write: bool) -> bool {
    let Ok(text) = std::fs::read_to_string(module) else {
        eprintln!("{}: cannot be read", module.display());
        return false;
    };
    if ljos_compiler::rename::lts_syntax(&text).is_none() {
        return true;
    }
    let base = here.filter(|here| root.starts_with(here)).unwrap_or(root);
    let Some(from) = key_below(base, module) else {
        return true;
    };
    let to = ljos_compiler::rename::lts_key(&from);
    let target = module.with_extension("lts");
    let to_name = to.rsplit('/').next().unwrap_or(&to);
    if target.exists() {
        outln!(
            "kept {}: it would be renamed to {to_name}, but {to_name} is in the way",
            module.display()
        );
        return true;
    }
    let mut below = Vec::new();
    if let Err(e) = files_below(base, imports_modules, &mut below) {
        eprintln!("{}: {e}", base.display());
        return false;
    }
    // Each importer's key, path, text and text with the new name.
    let mut importers: Vec<(String, PathBuf, String, String)> = Vec::new();
    for file in below {
        let Some(key) = key_below(base, &file) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        if let Some(renamed) = ljos_compiler::rename::imports(&key, &text, &from, &to) {
            importers.push((key, file, text, renamed));
        }
    }
    importers.sort_by(|a, b| a.0.cmp(&b.0));
    if write {
        if let Err(e) = std::fs::rename(module, &target) {
            eprintln!("{}: {e}", module.display());
            return false;
        }
        for (at, (_, file, _, renamed)) in importers.iter().enumerate() {
            if let Err(e) = std::fs::write(file, renamed) {
                eprintln!("{}: {e}", file.display());
                for (_, file, text, _) in &importers[..at] {
                    if let Err(e) = std::fs::write(file, text) {
                        eprintln!("{}: {e}", file.display());
                    }
                }
                if let Err(e) = std::fs::rename(&target, module) {
                    eprintln!("{}: {e}", target.display());
                }
                return false;
            }
        }
    }
    let verb = if write { "renamed" } else { "would rename" };
    // Each importer, and where they were looked for, is shown as the
    // module is, from the directory the command was given, whatever
    // `base` was made absolute as.
    let written = module.display().to_string();
    let given = written.strip_suffix(from.as_str());
    let shown: Vec<String> = importers
        .iter()
        .map(|(key, file, _, _)| match given {
            Some(given) => format!("{given}{key}"),
            None => file.display().to_string(),
        })
        .collect();
    let looked = match given.map(|given| given.trim_end_matches('/')) {
        Some("") => ".".to_string(),
        Some(given) => given.to_string(),
        None => base.display().to_string(),
    };
    let importers = if shown.is_empty() {
        String::new()
    } else {
        format!(", and the imports that name it in {}", shown.join(", "))
    };
    outln!(
        "{verb} {} to {to_name}{importers}; files outside {looked} were not looked in",
        module.display()
    );
    true
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
