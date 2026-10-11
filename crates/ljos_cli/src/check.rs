//! `ljos check`: compiles QML projects and says what the compiler says.
//! `--stats` adds the tier-1 coverage of their expressions: how many run in
//! the binding VM rather than in JavaScript, per file and in total, and why
//! the rest do not (ARCHITECTURE.md §5.4).
//!
//! Each project is linted as well (`lint`, ARCHITECTURE.md §14) unless
//! `--no-lint` says not to: a finding is a warning, never an error.
//!
//! A property or a signal handler of a name its type does not have is
//! reported too (`members`), as a warning: the compiler leaves it to the
//! run. The types are Qt's and Ljós's own (`Ljos`, `QtTest`) alike.

use std::{
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
}

impl Check {
    pub fn run(self) -> anyhow::Result<()> {
        let mut checked = Checked {
            lint: !self.no_lint,
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

impl Checked {
    /// Compiles the project in `dir` and adds it in. Its files are named
    /// under `label`.
    pub fn project(&mut self, dir: &Path, label: &str) -> anyhow::Result<()> {
        let mut keys = Vec::new();
        for file in compile_project(dir)? {
            self.files += 1;
            keys.push(file.key.clone());
            let name = if label.is_empty() {
                file.key.clone()
            } else {
                format!("{label}/{}", file.key)
            };
            let diagnostics = match &file.result {
                Ok(compiled) => {
                    self.coverage.add(&name, &compiled.file);
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
                self.diagnostics.push(format!("{name}.qml:{diagnostic}"));
            }
        }
        // What the compiler does not say and the run would: a name that is
        // no member of the type it is written on.
        let unknown = members::project(dir, label, &keys)?;
        self.warnings += unknown.len();
        self.diagnostics.extend(unknown);
        if self.lint {
            // After the compiler's own: a lint is never an error, and a
            // file that does not parse has none.
            let linted = lint::project(dir, label, &keys)?;
            self.warnings += linted.lines.len();
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
