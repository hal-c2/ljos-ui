//! The static lints of `ljos check` (ARCHITECTURE.md §14): what a file
//! says, before anything runs, that leaves an assistive technology or a
//! translator with nothing, or that breaks a design rule of `ljos_design`
//! (motion, layout, text, focus, colour).
//!
//! A finding keeps the rule's severity: an error is certain and fails the
//! check, unless `--lint=warn` makes it a warning (for code that is not
//! yours); a warning is kept with `// ljos-allow(<code>): <reason>`; a
//! hint is advice and left to `qml_lint`.
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

use std::path::Path;

use ljos_mcp_author::{QmlFrontEnd, Severity, lint};

/// Whether `ljos check` reports the rule with this code (hints are
/// `qml_lint`'s alone). The table is the
/// authoring server's, where `qml_lint` and `ljos://lints` read it too.
fn reported(code: &str) -> bool {
    lint(code).is_some_and(|lint| lint.in_check)
}

/// What the lints found in one project.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Linted {
    /// One line per finding, in the form the compiler's warnings have.
    pub lines: Vec<String>,
    pub errors: usize,
    pub warnings: usize,
}

/// Lints the files of the project in `dir`. `keys` are its files as the
/// compiler names them (the path below `dir` without `.qml`), and `label`
/// what their names are put under in a report.
pub fn project(
    dir: &Path,
    label: &str,
    keys: &[String],
    errors_as_warnings: bool,
) -> anyhow::Result<Linted> {
    let (files, names) = super::project_files(dir, label, keys);
    // The compiler's findings are `ljos check`'s own already.
    let found = QmlFrontEnd::new(dir).front_findings(&files, true);
    let mut linted = Linted::default();
    for d in found {
        if !reported(&d.code) {
            continue;
        }
        let name = names.get(&d.file).map_or(d.file.as_str(), String::as_str);
        let error = d.severity == Severity::Error && !errors_as_warnings;
        if error {
            linted.errors += 1;
        } else {
            linted.warnings += 1;
        }
        let guide = d.guide.map(|g| format!(" ({g})")).unwrap_or_default();
        linted.lines.push(format!(
            "{name}.qml:{}:{}: {}: {}{guide} [{}]",
            d.line,
            d.column,
            if error { "error" } else { "warning" },
            d.message,
            d.code
        ));
    }
    Ok(linted)
}
