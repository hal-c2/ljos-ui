//! `ljos check`: compiles QML projects and says what the compiler says.
//! `--stats` adds the tier-1 coverage of their expressions: how many run in
//! the binding VM rather than in JavaScript, per file and in total, and why
//! the rest do not (ARCHITECTURE.md §5.4).
//!
//! Each project is linted as well (`lint`, ARCHITECTURE.md §14) unless
//! `--no-lint` says not to: a finding has its rule's severity, and an
//! error fails the check unless `--lint=warn` says it is a warning.
//!
//! A property or a signal handler of a name its type does not have is
//! reported too (`members`), as a warning: the compiler leaves it to the
//! run. The types are Qt's and Ljós's own (`Ljos`, `QtTest`) alike.

use std::{
    collections::HashMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use anyhow::bail;
use clap::Args;
use ljos_compiler::{
    Severity, baseline, compile_project,
    tier1::{Counts, Coverage, Fell, Tally},
};

mod lint;
mod members;
mod port;

#[derive(Args)]
pub struct Check {
    /// Project directories: every `.qml` file below one is compiled as one
    /// project.
    #[arg(required = true)]
    dirs: Vec<PathBuf>,
    /// Each directory is a tree of projects laid out as a set of examples
    /// is (`group/project/...`): every directory two below it is a project
    /// of its own.
    #[arg(long)]
    tree: bool,
    /// Report tier-1 coverage, and the reasons expressions are tier 2.
    #[arg(long)]
    stats: bool,
    /// With `--stats`: how many reasons to list.
    #[arg(long, default_value_t = 25)]
    top: usize,
    /// With `--stats`: leave the per-file table out.
    #[arg(long)]
    summary: bool,
    /// Do not print the compiler's warnings.
    #[arg(long)]
    quiet: bool,
    /// Leave the static lints out: what has no name for an assistive
    /// technology, and `qsTr` of what is not written out.
    #[arg(long)]
    no_lint: bool,
    /// What a lint error is: `error` fails the check; `warn` reports it as
    /// a warning, for a tree of code that is not yours.
    #[arg(long, value_enum, default_value_t = LintMode::Error, value_name = "MODE")]
    lint: LintMode,
    /// Instead of compiling: report the Qt Quick types and members the
    /// projects use that Ljós lacks, by module, most used first.
    #[arg(long)]
    port: bool,
    /// With `--port`: print JSON rather than text.
    #[arg(long, requires = "port")]
    json: bool,
    /// With `--port`: exit with an error if anything is missing.
    #[arg(long, requires = "port")]
    deny_missing: bool,
    /// With `--port`: weigh against this API ledger's rows first (the tables
    /// of types answer where it has none). Without it the tables alone do.
    #[arg(long, requires = "port", value_name = "FILE")]
    ledger: Option<PathBuf>,
    /// With `--port`: list at most this many missing items per module.
    #[arg(long, requires = "port", value_name = "N")]
    limit: Option<usize>,
}

/// How a lint that is certain counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum LintMode {
    Error,
    Warn,
}

/// What a run found.
#[derive(Default)]
pub struct Checked {
    pub files: usize,
    pub errors: usize,
    pub warnings: usize,
    /// One line per diagnostic.
    pub diagnostics: Vec<String>,
    pub coverage: Coverage,
    /// Whether the static lints are applied to each project as well.
    pub lint: bool,
    /// Whether a lint that is certain counts as a warning.
    pub lint_warn: bool,
}

impl Check {
    pub fn run(self) -> anyhow::Result<()> {
        if self.port {
            return port::run(&self);
        }
        let mut checked = Checked {
            lint: !self.no_lint,
            lint_warn: self.lint == LintMode::Warn,
            ..Checked::default()
        };
        for dir in &self.dirs {
            if !dir.is_dir() {
                bail!("{} is not a directory", dir.display());
            }
            if self.tree {
                for project in baseline::project_dirs(dir) {
                    let label = project.strip_prefix(dir).unwrap_or(&project);
                    checked.project(&project, &label.to_string_lossy())?;
                }
            } else {
                checked.project(dir, "")?;
            }
        }
        for line in &checked.diagnostics {
            if !self.quiet || line.contains(": error: ") {
                eprintln!("{line}");
            }
        }
        if self.stats {
            print!("{}", checked.stats(self.top, !self.summary));
        } else {
            println!(
                "{} files, {} errors, {} warnings",
                checked.files, checked.errors, checked.warnings
            );
        }
        if checked.errors > 0 {
            bail!("{} errors", checked.errors);
        }
        Ok(())
    }
}

/// The name a report gives the file `key` of a project put under `label`.
fn shown_name(label: &str, key: &str) -> String {
    if label.is_empty() {
        key.to_owned()
    } else {
        format!("{label}/{key}")
    }
}

/// The paths of the project files `keys` in `dir`, and by each path the
/// name a report gives it.
fn project_files(
    dir: &Path,
    label: &str,
    keys: &[String],
) -> (Vec<PathBuf>, HashMap<String, String>) {
    let mut names = HashMap::new();
    let files = keys
        .iter()
        .map(|key| {
            let path = dir.join(format!("{key}.qml"));
            names.insert(path.to_string_lossy().into_owned(), shown_name(label, key));
            path
        })
        .collect();
    (files, names)
}

impl Checked {
    /// Compiles the project in `dir` and adds it in. Its files are named
    /// under `label`.
    pub fn project(&mut self, dir: &Path, label: &str) -> anyhow::Result<()> {
        let mut keys = Vec::new();
        // This project's lines, the stand-in notes among them, which no
        // finding is the same as.
        let start = self.diagnostics.len();
        for file in compile_project(dir)? {
            self.files += 1;
            keys.push(file.key.clone());
            let name = shown_name(label, &file.key);
            let diagnostics = match &file.result {
                Ok(compiled) => {
                    self.coverage.add(&name, &compiled.file);
                    let source = std::fs::read_to_string(dir.join(format!("{}.qml", file.key)));
                    for uri in stood_in(&compiled.file.imports) {
                        let (line, column) = source
                            .as_deref()
                            .map_or((1, 1), |source| import_at(source, uri));
                        self.warnings += 1;
                        self.diagnostics.push(format!(
                            "{name}.qml:{line}:{column}: warning: module \"{uri}\" is not part of \
                             Ljós: its types stand in as empty Items, and what only they have is \
                             left out [stand-in]"
                        ));
                    }
                    &compiled.diagnostics
                }
                Err(diagnostics) => diagnostics,
            };
            // A file the parser gave up on has errors, whatever they are
            // marked as.
            if file.result.is_err() && diagnostics.is_empty() {
                self.errors += 1;
            }
            for diagnostic in diagnostics {
                if diagnostic.severity == Severity::Error || file.result.is_err() {
                    self.errors += 1;
                } else {
                    self.warnings += 1;
                }
                self.diagnostics
                    .push(format!("{name}.qml:{diagnostic} [{}]", diagnostic.code));
            }
        }
        // What the compiler does not say and the run would: a name that is
        // no member of the type it is written on. Where the compiler said
        // it too, the finding, which may name what was meant, stands for
        // both, as an error when the compiler's was one.
        for finding in members::project(dir, label, &keys)? {
            let said = self.diagnostics[start..]
                .iter()
                .position(|line| same_member(line, &finding));
            let Some(said) = said.map(|i| start + i) else {
                self.warnings += 1;
                self.diagnostics.push(finding);
                continue;
            };
            // Counted once already, as the compiler's.
            self.diagnostics[said] = if self.diagnostics[said].contains(": error: ") {
                finding.replacen(": warning: ", ": error: ", 1)
            } else {
                finding
            };
        }
        if self.lint {
            // After the compiler's own: a file that does not parse has no
            // lints.
            let linted = lint::project(dir, label, &keys, self.lint_warn)?;
            self.errors += linted.errors;
            self.warnings += linted.warnings;
            self.diagnostics.extend(linted.lines);
        }
        Ok(())
    }

    /// The coverage report.
    pub fn stats(&self, top: usize, per_file: bool) -> String {
        let mut out = String::new();
        let coverage = &self.coverage;
        let width = coverage
            .files
            .iter()
            .map(|(name, _)| name.chars().count())
            .max()
            .unwrap_or(0)
            .max("total".len());
        let row = |out: &mut String, name: &str, counts: &Counts| {
            let cell = |t: Tally| format!("{}/{}", t.tier1, t.total);
            let all = counts.all();
            let _ = writeln!(
                out,
                "{name:<width$}  {:>11}  {:>11}  {:>11}  {:>11}  {:>6.1}%",
                cell(counts.bindings),
                cell(counts.handlers),
                cell(counts.functions),
                cell(all),
                all.percent(),
            );
        };
        let _ = writeln!(
            out,
            "{:<width$}  {:>11}  {:>11}  {:>11}  {:>11}  {:>7}",
            "tier 1 / all", "bindings", "handlers", "functions", "all", "tier 1"
        );
        if per_file {
            for (name, counts) in &coverage.files {
                row(&mut out, name, counts);
            }
        }
        row(&mut out, "total", &coverage.total);
        let total = &coverage.total;
        let _ = writeln!(out);
        let _ = writeln!(out, "files      {}", coverage.files.len());
        for (what, tally) in [
            ("bindings ", total.bindings),
            ("handlers ", total.handlers),
            ("functions", total.functions),
            ("all      ", total.all()),
        ] {
            let _ = writeln!(
                out,
                "{what}  {} of {} tier 1 ({:.1}%)",
                tally.tier1,
                tally.total,
                tally.percent()
            );
        }
        let kinds = coverage.reasons_by_kind();
        if kinds.is_empty() {
            return out;
        }
        // Bindings are what re-run when a property changes: the columns
        // say which reasons cost there and which only in a handler.
        let table =
            |out: &mut String, title: &str, rows: &mut dyn Iterator<Item = (&str, Fell)>| {
                let _ = writeln!(out);
                let _ = writeln!(out, "{title}");
                let _ = writeln!(
                    out,
                    "{:>7}  {:>8}  {:>8}  {:>9}",
                    "all", "bindings", "handlers", "functions"
                );
                for (reason, fell) in rows.take(top) {
                    let _ = writeln!(
                        out,
                        "{:>7}  {:>8}  {:>8}  {:>9}  {reason}",
                        fell.all(),
                        fell.bindings,
                        fell.handlers,
                        fell.functions
                    );
                }
            };
        table(
            &mut out,
            "tier 2, by construct",
            &mut kinds.iter().map(|(kind, fell)| (*kind, *fell)),
        );
        let details = coverage.details_by_kind();
        table(
            &mut out,
            "tier 2, in full",
            &mut details
                .iter()
                .map(|(reason, fell)| (reason.as_str(), *fell)),
        );
        out
    }
}

/// Whether the compiler's `line` and the finding `finding` are about one
/// name: they are at one place, and the compiler's names what the finding
/// names first.
fn same_member(line: &str, finding: &str) -> bool {
    let place = |l: &str| {
        l.find(": warning: ")
            .or_else(|| l.find(": error: "))
            .map(|at| l[..at].to_owned())
    };
    let named = finding.split('`').nth(1).filter(|name| !name.is_empty());
    match (place(line), place(finding), named) {
        (Some(a), Some(b), Some(name)) => a == b && line.contains(&format!("`{name}`")),
        _ => false,
    }
}

/// The modules among `imports` whose types stand in as empty Items. The
/// check says so where the compiler does not: the run says it once, in its
/// log, and a file that checks clean would otherwise hide what it loses.
fn stood_in(imports: &[ljos_ir::Import]) -> impl Iterator<Item = &str> {
    imports
        .iter()
        .filter(|import| import.stands_in)
        .filter_map(|import| match &import.source {
            ljos_ir::ImportSource::Module(uri) => Some(uri.as_str()),
            ljos_ir::ImportSource::Path(_) => None,
        })
}

/// The line and column, from 1, of the import of `uri` in `source`. An
/// import keeps no place of its own in the IR; 1:1 if none is found.
fn import_at(source: &str, uri: &str) -> (usize, usize) {
    source
        .lines()
        .enumerate()
        .find_map(|(i, line)| {
            let text = line.trim_start();
            let rest = text
                .strip_prefix("import")?
                .trim_start()
                .strip_prefix(uri)?;
            rest.chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || c == ';')
                .then(|| (i + 1, line.chars().count() - text.chars().count() + 1))
        })
        .unwrap_or((1, 1))
}
