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
//! | 2 | A file could not be read or written; the others were still handled. |

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use clap::Args;
use qml_compat::Outcome;

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
        let mut files = Vec::new();
        for path in &self.paths {
            if path.is_dir() {
                if let Err(e) = files_below(path, is_qml, &mut files) {
                    eprintln!("{}: {e}", path.display());
                    failed = true;
                }
            } else {
                files.push(path.clone());
            }
        }
        let style = self.style.clone().unwrap_or_else(|| {
            ljos_compiler::controls_style(
                |name| std::env::var(name).ok(),
                |path| std::fs::read_to_string(path).ok(),
            )
            // No style chosen is Basic, as in Qt 6.
            .map_or_else(|| qml_compat::CONTROLS_STYLE.to_owned(), |style| style.name)
        });
        // Every file first: files that name one another are lowered
        // together, as a lowered run lowers them (`qml_compat::group`).
        let mut alone = BTreeMap::new();
        let mut texts = BTreeMap::new();
        for file in &files {
            let text = match std::fs::read_to_string(file) {
                Ok(text) => text,
                Err(e) => {
                    eprintln!("{}: {e}", file.display());
                    failed = true;
                    continue;
                }
            };
            // A type the file names that a file beside it is: what that
            // file's root object.
            let dir = file.parent().unwrap_or(Path::new("."));
            let locals = |name: &str| {
                let beside = std::fs::read_to_string(dir.join(format!("{name}.qml"))).ok()?;
                qml_compat::root(&beside)
            };
            let key = key_of(file);
            alone.insert(
                key.clone(),
                qml_compat::Alone {
                    outcome: qml_compat::lower(
                        &text,
                        &qml_compat::Styled {
                            locals: &locals,
                            style: &style,
                        },
                    ),
                    references: qml_compat::references(&text),
                },
            );
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

/// The 1-based line and column (in characters) of byte `at` of `text`.
fn line_column(text: &str, at: usize) -> (usize, usize) {
    let before = &text[..at.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[start..].chars().count() + 1)
}
