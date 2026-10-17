//! What the compiler says of the files of a call, beside what the front end
//! says: `qml_check` reports what `ljos check` does. The compiler finds
//! what only compiling does (a member of an id an expression names, an
//! enumeration a binding cannot hold, a script that does not parse).
//!
//! Where both say something at one place, the front end's finding stands
//! for both, as in `ljos check`: it has a code and may name what was meant.
//! It is an error when the compiler's was one. Where the front end held
//! back (a type some C++ registers, or a file importing a module it does not
//! know, which may provide any name), the compiler's findings are held back
//! too: it cannot know either. What only the compiler says has the code
//! `compiler`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use ljos_compiler::{Project, compile_file};
use qml_front::{Code, QtTypeOracle};

use crate::{Diagnostic, LINTS, Severity};

/// The code of what only the compiler says.
pub(super) const CODE: &str = "compiler";

/// `found` (the front end's findings for `files`, in their order) with the
/// compiler's added in, each file's sorted by place. `held` are the places
/// the front end held back on.
pub(super) fn merge(
    root: &Path,
    files: &[PathBuf],
    found: Vec<Diagnostic>,
    held: &[Diagnostic],
) -> Vec<Diagnostic> {
    let compiled = compile(root, files);
    let mut by_file: HashMap<String, Vec<Diagnostic>> = HashMap::new();
    for d in found {
        by_file.entry(d.file.clone()).or_default().push(d);
    }
    let mut out = Vec::new();
    for (path, said) in files.iter().zip(compiled) {
        let name = path.to_string_lossy().into_owned();
        let mut mine = by_file.remove(&name).unwrap_or_default();
        let front = mine.len();
        let unverifiable = mine.iter().any(|f| f.code == Code::UnknownModule.as_str());
        for d in said.into_iter().filter(|_| !unverifiable) {
            if held
                .iter()
                .any(|h| (h.file.as_str(), h.line, h.column) == (name.as_str(), d.line, d.column))
            {
                continue;
            }
            // A lint at the place is about something else.
            match mine[..front].iter_mut().find(|f| {
                (f.line, f.column) == (d.line, d.column)
                    && !LINTS.iter().any(|lint| lint.code == f.code)
            }) {
                Some(f) => {
                    if d.severity == Severity::Error {
                        f.severity = Severity::Error;
                    }
                }
                None => mine.push(d),
            }
        }
        mine.sort_by(|a, b| {
            (a.line, a.column, &a.code, &a.message).cmp(&(b.line, b.column, &b.code, &b.message))
        });
        mine.dedup();
        out.extend(mine);
    }
    out
}

/// What the compiler says of each of `files`, compiled as the project under
/// `root` compiles it. A file it cannot read was the front end's to report.
fn compile(root: &Path, files: &[PathBuf]) -> Vec<Vec<Diagnostic>> {
    let Ok(mut project) = Project::load(root, QtTypeOracle) else {
        return vec![Vec::new(); files.len()];
    };
    let keys: Vec<Option<String>> = files
        .iter()
        .map(|path| {
            let key = key(root, path)?;
            let text = std::fs::read_to_string(path).ok()?;
            // A walk of the project leaves build output and hidden
            // directories out; a file that is named is checked wherever
            // it is.
            if project.source(&key) != Some(text.as_str()) {
                project.add(&key, text);
            }
            Some(key)
        })
        .collect();
    files
        .iter()
        .zip(keys)
        .map(|(path, key)| {
            let Some(key) = key else {
                return Vec::new();
            };
            let (failed, diagnostics) = match compile_file(&project, &key) {
                Ok(compiled) => (false, compiled.diagnostics),
                Err(diagnostics) => (true, diagnostics),
            };
            let file = path.to_string_lossy();
            diagnostics
                .into_iter()
                .map(|d| Diagnostic {
                    file: file.clone().into_owned(),
                    line: d.span.line,
                    column: d.span.column,
                    // A file the parser gave up on has errors, whatever
                    // they are marked as.
                    severity: if failed || d.severity == ljos_compiler::Severity::Error {
                        Severity::Error
                    } else {
                        Severity::Warning
                    },
                    code: CODE.to_owned(),
                    message: d.message,
                })
                .collect()
        })
        .collect()
}

/// The compiler's name of the file at `path`: its path below `root`
/// without `.qml`.
fn key(root: &Path, path: &Path) -> Option<String> {
    let below = path.strip_prefix(root).ok()?.with_extension("");
    let parts: Vec<_> = below
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect::<Option<_>>()?;
    Some(parts.join("/"))
}
