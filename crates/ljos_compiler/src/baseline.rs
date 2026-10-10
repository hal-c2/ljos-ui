//! How much of a body of QML compiles: the measure the compiler is held to.
//!
//! Each project directory is compiled on its own, the way a build would, and
//! every file is counted as compiled without errors, compiled with errors
//! (partly), or not compiled at all.

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};

use ljos_ir::Resolved;

use qml_front::TypeOracle;

use crate::{Project, Severity, compile_file};

#[derive(Debug, Default, Clone)]
pub struct Baseline {
    pub projects: usize,
    pub files: usize,
    /// Compiled without errors.
    pub full: usize,
    /// Compiled, with errors: what did not parse or resolve is left out.
    pub partial: usize,
    /// The parser gave up.
    pub failed: usize,
    /// Property bindings, handlers and functions.
    pub bindings: usize,
    /// Those whose every free name the compiler resolved.
    pub resolved: usize,
    pub literals: usize,
    pub warnings: usize,
    /// How often each name was not resolved.
    pub unknown: BTreeMap<String, usize>,
    /// The first error of each file that has one, by file.
    pub errors: Vec<(String, String)>,
}

impl Baseline {
    pub fn resolved_percent(&self) -> f64 {
        if self.bindings == 0 {
            return 100.0;
        }
        self.resolved as f64 * 100.0 / self.bindings as f64
    }

    /// The names most often not resolved, most first.
    pub fn top_unknown(&self, n: usize) -> Vec<(&str, usize)> {
        let mut names: Vec<(&str, usize)> = self
            .unknown
            .iter()
            .map(|(name, count)| (name.as_str(), *count))
            .collect();
        names.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        names.truncate(n);
        names
    }

    /// Compiles one project and adds it in.
    pub fn add<O: TypeOracle>(&mut self, project: &Project<O>, label: &str) {
        self.projects += 1;
        for key in project.files() {
            self.files += 1;
            match compile_file(project, key) {
                Err(diagnostics) => {
                    self.failed += 1;
                    if let Some(first) = diagnostics.first() {
                        self.errors
                            .push((format!("{label}/{key}"), first.to_string()));
                    }
                }
                Ok(compiled) => {
                    let errors: Vec<_> = compiled
                        .diagnostics
                        .iter()
                        .filter(|d| d.severity == Severity::Error)
                        .collect();
                    if let Some(first) = errors.first() {
                        self.partial += 1;
                        self.errors
                            .push((format!("{label}/{key}"), first.to_string()));
                    } else {
                        self.full += 1;
                    }
                    self.warnings += compiled.diagnostics.len() - errors.len();
                    for expr in &compiled.file.exprs {
                        self.bindings += 1;
                        let mut known = true;
                        for r in &expr.refs {
                            if r.resolved == Resolved::Unknown {
                                known = false;
                                *self.unknown.entry(r.name.clone()).or_insert(0) += 1;
                            }
                        }
                        if known {
                            self.resolved += 1;
                        }
                    }
                    self.literals += compiled
                        .file
                        .components
                        .iter()
                        .flat_map(|c| &c.objects)
                        .map(|o| {
                            o.members.assignments.len()
                                + o.grouped
                                    .iter()
                                    .map(|g| g.members.assignments.len())
                                    .sum::<usize>()
                                + o.attached
                                    .iter()
                                    .map(|a| a.members.assignments.len())
                                    .sum::<usize>()
                        })
                        .sum::<usize>();
                }
            }
        }
    }
}

impl fmt::Display for Baseline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let percent = |n: usize| {
            if self.files == 0 {
                0.0
            } else {
                n as f64 * 100.0 / self.files as f64
            }
        };
        writeln!(f, "projects  {}", self.projects)?;
        writeln!(f, "files     {}", self.files)?;
        writeln!(f, "  full    {} ({:.1}%)", self.full, percent(self.full))?;
        writeln!(
            f,
            "  partial {} ({:.1}%)",
            self.partial,
            percent(self.partial)
        )?;
        writeln!(
            f,
            "  failed  {} ({:.1}%)",
            self.failed,
            percent(self.failed)
        )?;
        writeln!(f, "literals  {}", self.literals)?;
        writeln!(f, "bindings  {}", self.bindings)?;
        writeln!(
            f,
            "  no Unknown ref {} ({:.1}%)",
            self.resolved,
            self.resolved_percent()
        )?;
        writeln!(f, "warnings  {}", self.warnings)?;
        let top = self.top_unknown(15);
        if !top.is_empty() {
            let names: Vec<String> = top
                .iter()
                .map(|(name, count)| format!("{name} ({count})"))
                .collect();
            writeln!(f, "unknown   {}", names.join(", "))?;
        }
        Ok(())
    }
}

/// The project directories of a tree of examples: each directory two below
/// the root (`demos/clocks`), or the root itself when it has QML files of
/// its own and nothing deeper.
pub fn project_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(groups) = std::fs::read_dir(root) else {
        return out;
    };
    let mut groups: Vec<PathBuf> = groups
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    groups.sort();
    for group in groups {
        let Ok(entries) = std::fs::read_dir(&group) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect();
        dirs.sort();
        out.extend(dirs);
    }
    if out.is_empty() {
        out.push(root.to_owned());
    }
    out
}

/// Compiles every project under `root` against the types `oracle` makes.
pub fn measure<O: TypeOracle>(root: &Path, oracle: impl Fn() -> O) -> Baseline {
    let mut baseline = Baseline::default();
    for dir in project_dirs(root) {
        let Ok(project) = Project::load(&dir, oracle()) else {
            continue;
        };
        if project.files().next().is_none() {
            continue;
        }
        let label = dir
            .strip_prefix(root)
            .unwrap_or(&dir)
            .to_string_lossy()
            .into_owned();
        baseline.add(&project, &label);
    }
    baseline
}
