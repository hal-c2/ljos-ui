//! The names `ljos check` reports that the type they are written on does
//! not have: `colr` on a `Rectangle`, `onClikced` on a `MouseArea`.
//!
//! The compiler leaves such a name to the run, where the file then fails
//! to load. The authoring server's front end finds it before that
//! (`qml_check`), from the same table of types the compiler reads, so a
//! finding has the same code here, there and in an editor. That table has
//! Ljós's own modules (`Ljos`, `QtTest`) beside Qt's.
//!
//! A finding is a warning: the front end says only what it is sure of, but
//! a type a style writes in QML is known by what it inherits alone, and
//! whether a project compiles is the compiler's to say.
//!
//! | Code | What is reported |
//! |---|---|
//! | `unknown-property` | A binding of a property the object's type does not have. |
//! | `unknown-signal` | A handler (`onFoo`) of a signal the object's type does not have. |

use std::{collections::HashMap, path::Path};

use ljos_mcp_author::QmlFrontEnd;

/// What of the front end's findings is reported.
pub const CODES: [&str; 2] = ["unknown-property", "unknown-signal"];

/// The names in the project in `dir` that are no member of what they are
/// written on, one line each in the form the compiler's warnings have.
/// `keys` are the project's files as the compiler names them (the path
/// below `dir` without `.qml`), and `label` what their names are put under
/// in a report. Without a table of types there is nothing to say.
pub fn project(dir: &Path, label: &str, keys: &[String]) -> anyhow::Result<Vec<String>> {
    let mut names = HashMap::new();
    let files: Vec<_> = keys
        .iter()
        .map(|key| {
            let path = dir.join(format!("{key}.qml"));
            let name = if label.is_empty() {
                key.clone()
            } else {
                format!("{label}/{key}")
            };
            names.insert(path.to_string_lossy().into_owned(), name);
            path
        })
        .collect();
    // The compiler's findings are `ljos check`'s own already.
    let found = QmlFrontEnd::new(dir).front_findings(&files, false);
    Ok(found
        .into_iter()
        .filter(|d| CODES.contains(&d.code.as_str()))
        .map(|d| {
            let name = names.get(&d.file).map_or(d.file.as_str(), String::as_str);
            format!(
                "{name}.qml:{}:{}: warning: {} [{}]",
                d.line, d.column, d.message, d.code
            )
        })
        .collect())
}
