//! A lowered file as `ljos migrate` writes it (docs/DIALECT.md §8).
//!
//! The pass keeps every line where it was, so that what a lowered run says
//! of a file points at the file as written: its imports share the line of
//! `pragma Scripts: JavaScript`, and a Qt import whose module another's line
//! has is left an empty line. A migrated file is the one a person reads and
//! changes from then on, so it is laid out as a file written in Ljós's API
//! is, and says its scripts are JavaScript only where they are.

use crate::JAVASCRIPT;

/// `lowered`, the text the pass made of `written`, with each import of the
/// pragma's line on a line of its own and the lines the pass emptied of a
/// Qt import taken out. Without the pragma where `typed`: the file's scripts
/// are held to the checker's rules.
///
/// Only the lines before the first that is not an import, a pragma, a
/// comment or empty are read, and a line is taken out only where it was an
/// import as written, so an empty line a person left is kept.
#[must_use]
pub fn migrated(written: &str, lowered: &str, typed: bool) -> String {
    let mut out = String::with_capacity(lowered.len());
    let mut as_written = written.split_inclusive('\n');
    let mut lines = lowered.split_inclusive('\n');
    for line in lines.by_ref() {
        let was = as_written.next().unwrap_or("");
        let content = line.trim_end_matches(['\n', '\r']);
        let end = &line[content.len()..];
        // A file indented as a whole is QML too; each line it is laid out
        // to keeps the indent of the line it came from.
        let rest = content.trim_start();
        let indent = &content[..content.len() - rest.len()];
        if let Some(imports) = rest.strip_prefix(JAVASCRIPT) {
            if !typed {
                out.push_str(indent);
                out.push_str(JAVASCRIPT);
                out.push_str(end_or_newline(end));
            }
            // A comment the import of Qt's had stays at the end of the
            // last, and a `;` in it is not between imports.
            let (imports, comment) = match imports.find("//") {
                Some(at) => (&imports[..at], Some(imports[at..].trim_end())),
                None => (imports, None),
            };
            let statements: Vec<&str> = imports
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            for (at, statement) in statements.iter().enumerate() {
                out.push_str(indent);
                out.push_str(statement);
                if let Some(comment) = comment.filter(|_| at + 1 == statements.len()) {
                    out.push(' ');
                    out.push_str(comment);
                }
                // The last keeps the line's own ending, so a file that
                // ends on its imports ends as it did.
                out.push_str(if at + 1 == statements.len() {
                    end
                } else {
                    end_or_newline(end)
                });
            }
            continue;
        }
        let trimmed = content.trim();
        if trimmed.is_empty() && is_import(was) {
            continue;
        }
        out.push_str(line);
        if !(trimmed.is_empty()
            || is_import(trimmed)
            || trimmed.starts_with("pragma ")
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*"))
        {
            break;
        }
    }
    for line in lines {
        out.push_str(line);
    }
    out
}

fn is_import(line: &str) -> bool {
    line.trim_start().starts_with("import ")
}

/// A line's ending, or `\n` for the last line of a file without one.
fn end_or_newline(end: &str) -> &str {
    if end.is_empty() { "\n" } else { end }
}

#[cfg(test)]
mod tests {
    use super::migrated;

    #[test]
    fn imports_go_on_lines_of_their_own_and_emptied_lines_go() {
        let written = "import QtQuick\nimport QtQuick.Layouts\nimport QtQuick.Controls.Basic\n\nRow {}\n";
        let lowered = "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Layout\n\nimport Ljos.Controls.Basic\n\nRow {}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "import Ljos.Ui\nimport Ljos.Layout\nimport Ljos.Controls.Basic\n\nRow {}\n"
        );
        assert_eq!(
            migrated(written, lowered, false),
            "pragma Scripts: JavaScript\nimport Ljos.Ui\nimport Ljos.Layout\nimport Ljos.Controls.Basic\n\nRow {}\n"
        );
    }

    /// Below the imports nothing is read: an empty line where the file as
    /// written had an import-like line inside a string stays.
    #[test]
    fn only_the_imports_are_laid_out() {
        let written = "import QtQuick\nItem {\nproperty string s: \"\n\"\n}\n";
        let lowered = "pragma Scripts: JavaScript; import Ljos.Ui\nItem {\nproperty string s: \"\n\"\n}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "import Ljos.Ui\nItem {\nproperty string s: \"\n\"\n}\n"
        );
    }

    #[test]
    fn a_comment_after_the_imports_stays_with_the_last() {
        let written = "// A dial.\nimport QtQuick // items; all\nimport QtQuick.Layouts\nItem {}";
        let lowered =
            "// A dial.\npragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Layout // items; all\n\nItem {}";
        assert_eq!(
            migrated(written, lowered, true),
            "// A dial.\nimport Ljos.Ui\nimport Ljos.Layout // items; all\nItem {}"
        );
    }

    /// A file indented as a whole, as one pasted from a document is: the
    /// pragma is still found, and every line keeps the indent.
    #[test]
    fn an_indented_file_keeps_its_indent() {
        let written = "  import QtQuick\n  import QtQuick.Layouts\n  Row {}\n";
        let lowered = "  pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Layout\n\n  Row {}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "  import Ljos.Ui\n  import Ljos.Layout\n  Row {}\n"
        );
        assert_eq!(
            migrated(written, lowered, false),
            "  pragma Scripts: JavaScript\n  import Ljos.Ui\n  import Ljos.Layout\n  Row {}\n"
        );
    }

    #[test]
    fn carriage_returns_are_kept() {
        let written = "import QtQuick\r\nimport QtQuick.Layouts\r\nRow {}\r\n";
        let lowered = "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Layout\r\n\r\nRow {}\r\n";
        assert_eq!(
            migrated(written, lowered, false),
            "pragma Scripts: JavaScript\r\nimport Ljos.Ui\r\nimport Ljos.Layout\r\nRow {}\r\n"
        );
    }
}
