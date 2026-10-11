//! The static lints of `ljos check` (ARCHITECTURE.md §14): what a file
//! says, before anything runs, that leaves an assistive technology or a
//! translator with nothing.
//!
//! The rules are the authoring server's (`qml_lint`), so a finding has the
//! same code here, there and in an editor. Of what that reports, this keeps
//! the rules a reader of the application pays for; the compiler says the
//! rest itself, and a text that is not translated is a matter of taste.
//!
//! | Code | Rule |
//! |---|---|
//! | `a11y-unnamed-interactive` | Something that handles a click or a tap and has no name. |
//! | `a11y-unnamed-icon-button` | A button with an icon and neither a text nor a name. |
//! | `a11y-unnamed-image` | An `Image` that has no name and is not `Accessible.ignored`. |
//! | `i18n-qstr-non-literal` | `qsTr` of something that is not written out, which no tool can extract. |

use std::{collections::HashMap, path::Path};

use ljos_mcp_author::{FrontEnd, QmlFrontEnd};

/// The rules `ljos check` reports.
pub const CODES: [&str; 4] = [
    "a11y-unnamed-interactive",
    "a11y-unnamed-icon-button",
    "a11y-unnamed-image",
    "i18n-qstr-non-literal",
];

/// What the front end says when it has no type table to tell a `Button`
/// from an `Item` with.
const NO_TYPES: &str = "types-unavailable";

/// What the lints found in one project.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Linted {
    /// One line per finding, in the form the compiler's warnings have.
    pub lines: Vec<String>,
    /// The rules could not be applied: there is no table of types.
    pub skipped: bool,
}

/// Lints the files of the project in `dir`. `keys` are its files as the
/// compiler names them (the path below `dir` without `.qml`), and `label`
/// what their names are put under in a report.
pub fn project(dir: &Path, label: &str, keys: &[String]) -> anyhow::Result<Linted> {
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
    let found = QmlFrontEnd::new(dir)
        .lint(&files)
        .map_err(|e| anyhow::anyhow!("{}: {e}", dir.display()))?;
    let mut linted = Linted::default();
    for d in found {
        if d.code == NO_TYPES {
            linted.skipped = true;
        }
        if !CODES.contains(&d.code.as_str()) {
            continue;
        }
        let name = names.get(&d.file).map_or(d.file.as_str(), String::as_str);
        linted.lines.push(format!(
            "{name}.qml:{}:{}: warning: {} [{}]",
            d.line, d.column, d.message, d.code
        ));
    }
    Ok(linted)
}
