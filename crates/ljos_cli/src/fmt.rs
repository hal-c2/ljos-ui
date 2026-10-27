//! `ljos fmt`: lay QML files out.
//!
//! The layout is `qml_fmt`'s, the same one the authoring server's
//! `qml_format` gives, so a file an agent formatted is one this command
//! leaves alone. What it prints is for a script as much as for a person:
//!
//! | To | What |
//! |---|---|
//! | stdout | `formatted <file>` (or `would format <file>`, or the diff) for each file that is not laid out, then `N files, M formatted` |
//! | stderr | `<file>:<line>:<column>: error: <message>` for a file that does not parse; such a file is never written |
//!
//! | Status | Means |
//! |---|---|
//! | 0 | Every file is laid out (or was, or `--diff` showed what would change). |
//! | 1 | `--check` found a file that is not laid out. |
//! | 2 | A file does not parse or could not be read; the others were still handled. |

use std::{
    io::Read as _,
    path::{Path, PathBuf},
};

use clap::Args;

#[derive(Args)]
pub struct Fmt {
    /// The files to lay out, or directories for every `.qml` file below
    /// them; `-` reads standard input and writes the result to standard
    /// output.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    /// Change nothing; name the files that are not laid out and exit with
    /// status 1 if there is one.
    #[arg(long)]
    check: bool,
    /// Change nothing; print what would change as a unified diff.
    #[arg(long)]
    diff: bool,
}

/// Directories a walk does not enter: tools' own, and build output that
/// holds copies of the sources. The same ones the compiler skips, so `ljos
/// fmt .` and `ljos check .` see one set of files.
pub(crate) fn skipped(name: &str) -> bool {
    name.starts_with('.')
        || matches!(name, "target" | "node_modules" | "build")
        || name.starts_with("build-")
        || name.starts_with("cmake-build")
}

/// Every file below `dir` that `wanted` takes, in the order of their
/// names, outside the [`skipped`] directories. Links to directories are
/// not entered: they can loop.
pub(crate) fn files_below(
    dir: &Path,
    wanted: fn(&Path) -> bool,
    out: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if !skipped(&entry.file_name().to_string_lossy()) {
                files_below(&path, wanted, out)?;
            }
        } else if wanted(&path) && path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn is_qml(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "qml")
}

/// A path as a report names it: as it was given, without the `./` a walk
/// from `.` puts in front.
pub(crate) fn shown(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix("./").unwrap_or(&text).to_owned()
}

#[derive(Default)]
struct Tally {
    files: usize,
    /// Files that are not laid out (and were written, unless nothing is).
    changed: usize,
    not_parsed: usize,
    /// A path that is not there, a file that cannot be read or written, or
    /// a fault of the formatter.
    failed: usize,
}

impl Fmt {
    /// Whether files are left as they are.
    fn dry(&self) -> bool {
        self.check || self.diff
    }

    /// Does the work and gives the exit status.
    pub fn run(&self) -> i32 {
        if self.paths.iter().any(|p| p.as_os_str() == "-") {
            if self.paths.len() > 1 {
                eprintln!("ljos: `-` (standard input) cannot be mixed with files");
                return 2;
            }
            return self.stdin();
        }
        let mut tally = Tally::default();
        for path in &self.paths {
            if path.is_dir() {
                let mut files = Vec::new();
                if let Err(e) = files_below(path, is_qml, &mut files) {
                    eprintln!("{}: error: {e}", shown(path));
                    tally.failed += 1;
                }
                for file in files {
                    self.file(&file, &mut tally);
                }
            } else {
                self.file(path, &mut tally);
            }
        }
        let verb = if self.dry() {
            "would be formatted"
        } else {
            "formatted"
        };
        let mut summary = format!("{} files, {} {verb}", tally.files, tally.changed);
        if tally.not_parsed > 0 {
            summary.push_str(&format!(", {} not parsed", tally.not_parsed));
        }
        println!("{summary}");
        if tally.not_parsed > 0 || tally.failed > 0 {
            2
        } else if self.check && tally.changed > 0 {
            1
        } else {
            0
        }
    }

    fn file(&self, path: &Path, tally: &mut Tally) {
        let name = shown(path);
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(e) => {
                eprintln!("{name}: error: {e}");
                tally.failed += 1;
                return;
            }
        };
        tally.files += 1;
        let formatted = match qml_fmt::format(&source) {
            Ok(formatted) => formatted,
            Err(qml_fmt::Error::Syntax(errors)) => {
                for e in errors {
                    eprintln!("{name}:{}:{}: error: {}", e.line, e.column, e.message);
                }
                tally.not_parsed += 1;
                return;
            }
            Err(e @ qml_fmt::Error::Unstable(_)) => {
                eprintln!("{name}: error: {e}");
                tally.failed += 1;
                return;
            }
        };
        if formatted == source {
            return;
        }
        if self.diff {
            print!("{}", qml_fmt::unified_diff(&name, &source, &formatted));
        } else if self.check {
            println!("would format {name}");
        } else if let Err(e) = std::fs::write(path, &formatted) {
            eprintln!("{name}: error: {e}");
            tally.failed += 1;
            return;
        } else {
            println!("formatted {name}");
        }
        tally.changed += 1;
    }

    /// Standard input to standard output, for an editor: the result and
    /// nothing else, or with `--check` nothing but the status.
    fn stdin(&self) -> i32 {
        const NAME: &str = "<stdin>";
        let mut source = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut source) {
            eprintln!("{NAME}: error: {e}");
            return 2;
        }
        let formatted = match qml_fmt::format(&source) {
            Ok(formatted) => formatted,
            Err(qml_fmt::Error::Syntax(errors)) => {
                for e in errors {
                    eprintln!("{NAME}:{}:{}: error: {}", e.line, e.column, e.message);
                }
                return 2;
            }
            Err(e @ qml_fmt::Error::Unstable(_)) => {
                eprintln!("{NAME}: error: {e}");
                return 2;
            }
        };
        if self.diff {
            print!("{}", qml_fmt::unified_diff(NAME, &source, &formatted));
        } else if !self.check {
            print!("{formatted}");
        }
        i32::from(self.check && formatted != source)
    }
}
