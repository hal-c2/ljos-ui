//! A lowered file as `ljos migrate` writes it (docs/DIALECT.md §8).
//!
//! The pass keeps every line where it was, so that what a lowered run says
//! of a file points at the file as written: its imports share the line of
//! `pragma Scripts: JavaScript`, and a Qt import whose module another's line
//! has is left an empty line. Below the imports it is the same: a binding it
//! takes out (`target:` of a `PropertyChanges`, a table's `id:`) leaves its
//! line empty, and one it writes out (`spacing: 5` of a `ColumnLayout`, a
//! view's `model: rows`) goes on the line of the object's `{` or the list's
//! `]`. A migrated file is the one a person reads and changes from then on,
//! so it is laid out as a file written in Ljós's API is, and says its
//! scripts are JavaScript only where they are.

use crate::JAVASCRIPT;

/// `lowered`, the text the pass made of `written`, with each import of the
/// pragma's line on a line of its own and the lines the pass emptied of a
/// Qt import taken out. Without the pragma where `typed`: the file's scripts
/// are held to the checker's rules. Below the imports, a line the pass
/// emptied is taken out and each binding it wrote after a `{` or a `]` is
/// put on a line of its own.
///
/// A line is taken out only where it was not empty as written, so an empty
/// line a person left is kept. Below the imports a line is read beside the
/// line it was only while the two files have as many lines, which the pass
/// keeps; where they do not, the rest is left as the pass made it.
#[must_use]
pub fn migrated(written: &str, lowered: &str, typed: bool) -> String {
    let mut out = String::with_capacity(lowered.len());
    let mut as_written = written.split_inclusive('\n');
    let mut lines = lowered.split_inclusive('\n');
    // Inside a `/* */` comment that began on a line above: its lines are
    // the header's too, whatever they start with.
    let mut in_comment = false;
    // The first line below the imports, and the line it was.
    let mut body = None;
    for line in lines.by_ref() {
        let was = as_written.next().unwrap_or("");
        let content = line.trim_end_matches(['\n', '\r']);
        let end = &line[content.len()..];
        // A file indented as a whole is QML too; each line it is laid out
        // to keeps the indent of the line it came from.
        let rest = content.trim_start();
        let indent = &content[..content.len() - rest.len()];
        if in_comment {
            out.push_str(line);
            match header(rest, true) {
                Some(open) => in_comment = open,
                None => break,
            }
            continue;
        }
        if let Some(imports) = rest.strip_prefix(JAVASCRIPT) {
            if !typed {
                out.push_str(indent);
                out.push_str(JAVASCRIPT);
                out.push_str(end_or_newline(end));
            }
            // A comment the import of Qt's had stays at the end of the
            // last.
            let (statements, comment, open) = statements(imports);
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
            in_comment = open;
            continue;
        }
        let trimmed = content.trim();
        if trimmed.is_empty() && is_import(was) {
            continue;
        }
        match header(rest, false) {
            Some(open) => {
                out.push_str(line);
                in_comment = open;
            }
            None => {
                body = Some((line, was));
                break;
            }
        }
    }
    // A line that ends a comment and goes on with code is the header's,
    // so the lines below it are read from the next.
    let (first, was) = body.unzip();
    let lowered: Vec<&str> = first.into_iter().chain(lines).collect();
    let written: Vec<&str> = was.into_iter().chain(as_written).collect();
    if lowered.len() != written.len() {
        for line in lowered {
            out.push_str(line);
        }
        return out;
    }
    for (at, (line, was)) in lowered.iter().zip(&written).enumerate() {
        laid_out(&mut out, line, was, &lowered[at + 1..]);
    }
    out
}

/// Whether a line, read from its start (inside a `/* */` comment where
/// `in_comment`), is the header's: empty, a comment, an import or a
/// pragma, with whatever follows a comment's `*/` read as well. `Some`
/// says whether it leaves a `/* */` comment open; `None` is code, where
/// the header ends.
fn header(mut text: &str, in_comment: bool) -> Option<bool> {
    // What follows the `*/` that ends the comment the line is in or opens,
    // or `None` while the comment goes on.
    fn past(text: &str) -> Option<&str> {
        text.find("*/").map(|close| &text[close + 2..])
    }
    if in_comment {
        let Some(after) = past(text) else {
            return Some(true);
        };
        text = after;
    }
    loop {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            return Some(false);
        }
        if let Some(opened) = trimmed.strip_prefix("/*") {
            let Some(after) = past(opened) else {
                return Some(true);
            };
            text = after;
            continue;
        }
        if is_import(trimmed) || trimmed.starts_with("pragma ") {
            return Some(statements(trimmed).2);
        }
        return None;
    }
}

/// One line below the imports, `line` as the pass made it of `was`, with
/// `after` the lines the pass made below it.
fn laid_out(out: &mut String, line: &str, was: &str, after: &[&str]) {
    let content = line.trim_end_matches(['\n', '\r']);
    let end = &line[content.len()..];
    let was = was.trim_end_matches(['\n', '\r']);
    if content.trim().is_empty() && !was.trim().is_empty() {
        return;
    }
    let rest = content.trim_start();
    let indent = &content[..content.len() - rest.len()];
    let Some((head, bindings, inside)) = written_out(was, content) else {
        out.push_str(line);
        return;
    };
    // A binding written after a `{` is a member of the object it opens, so
    // it is indented as the object's next line is, or a level in where that
    // line closes it.
    let members = if inside {
        after
            .iter()
            .map(|next| next.trim_end_matches(['\n', '\r']))
            .find(|next| !next.trim().is_empty())
            .map(|next| &next[..next.len() - next.trim_start().len()])
            .filter(|deeper| deeper.len() > indent.len() && deeper.starts_with(indent))
            .map_or_else(|| format!("{indent}    "), str::to_owned)
    } else {
        indent.to_owned()
    };
    out.push_str(head.trim_end());
    out.push_str(end_or_newline(end));
    for (at, binding) in bindings.iter().enumerate() {
        out.push_str(&members);
        out.push_str(binding);
        out.push_str(if at + 1 == bindings.len() {
            end
        } else {
            end_or_newline(end)
        });
    }
}

/// The bindings the pass wrote at the end of `line`, which it made of
/// `was`: after the `{` that ends `was` (each ending in `;`, the third of
/// the result `true`), or after a `]` or `}` that ends a line of `was` with
/// no `;` (each after a `;`). `None` where it wrote none, or wrote what is
/// not a plain `name: value` binding.
fn written_out<'a>(was: &str, line: &'a str) -> Option<(&'a str, Vec<&'a str>, bool)> {
    let was = was.trim_end();
    let line = line.trim_end();
    if was.contains(['"', '\'', ';', '/']) {
        return None;
    }
    let (head, tail, inside) = if was.ends_with('{') {
        let braces = was.matches('{').count();
        let (at, _) = line.match_indices('{').nth(braces - 1)?;
        let tail = line[at + 1..].trim();
        (&line[..=at], tail.strip_suffix(';')?, true)
    } else {
        let (head, tail) = line.split_once(';')?;
        if !head.trim_end().ends_with([']', '}']) {
            return None;
        }
        (head, tail, false)
    };
    let bindings: Vec<&str> = tail.split(';').map(str::trim).collect();
    bindings
        .iter()
        .all(|binding| is_binding(binding))
        .then_some((head, bindings, inside))
}

/// `name: value`, the name dotted or not, the value one with no string,
/// list, object or call in it.
fn is_binding(text: &str) -> bool {
    let Some((name, value)) = text.split_once(':') else {
        return false;
    };
    let name = name.trim_end();
    let value = value.trim();
    !name.is_empty()
        && name.split('.').all(|part| {
            part.chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        && !value.is_empty()
        && !value.contains(['"', '\'', '`', '{', '}', '[', ']', '(', ')', ':', ';'])
}

/// The statements of the pragma's line, split at each `;`, the `//`
/// comment that ends it, and whether a `/* */` comment it opens goes on to
/// a later line. A `;` or `//` in a quoted import (`import "http://…"`) or
/// in a `/* */` comment is neither: such a comment stays with the statement
/// it is in.
fn statements<'a>(line: &'a str) -> (Vec<&'a str>, Option<&'a str>, bool) {
    let bytes = line.as_bytes();
    let mut statements = Vec::new();
    let mut push = |text: &'a str| {
        let text = text.trim();
        if !text.is_empty() {
            statements.push(text);
        }
    };
    let (mut start, mut at) = (0, 0);
    let mut quote = None;
    // Every byte compared is ASCII, so each index sliced at is a character's
    // start, whatever the strings between hold.
    while at < bytes.len() {
        let byte = bytes[at];
        if let Some(open) = quote {
            if byte == b'\\' {
                at += 1;
            } else if byte == open {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if bytes[at..].starts_with(b"//") {
            push(&line[start..at]);
            return (statements, Some(line[at..].trim_end()), false);
        } else if bytes[at..].starts_with(b"/*") {
            let Some(close) = line[at + 2..].find("*/") else {
                push(&line[start..]);
                return (statements, None, true);
            };
            at += 2 + close + 2;
            continue;
        } else if byte == b';' {
            push(&line[start..at]);
            start = at + 1;
        }
        at += 1;
    }
    push(&line[start..]);
    (statements, None, false)
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
        let written =
            "import QtQuick\nimport QtQuick.Layouts\nimport QtQuick.Controls.Basic\n\nRow {}\n";
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
        let lowered =
            "pragma Scripts: JavaScript; import Ljos.Ui\nItem {\nproperty string s: \"\n\"\n}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "import Ljos.Ui\nItem {\nproperty string s: \"\n\"\n}\n"
        );
    }

    #[test]
    fn a_comment_after_the_imports_stays_with_the_last() {
        let written = "// A dial.\nimport QtQuick // items; all\nimport QtQuick.Layouts\nItem {}";
        let lowered = "// A dial.\npragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Layout // items; all\n\nItem {}";
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
        let lowered =
            "  pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Layout\n\n  Row {}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "  import Ljos.Ui\n  import Ljos.Layout\n  Row {}\n"
        );
        assert_eq!(
            migrated(written, lowered, false),
            "  pragma Scripts: JavaScript\n  import Ljos.Ui\n  import Ljos.Layout\n  Row {}\n"
        );
    }

    /// A licence header of several lines, as older Qt examples have, is
    /// read past to the imports under it.
    #[test]
    fn a_block_comment_of_several_lines_is_read_past() {
        let written = "/*\n * A dial.\n */\nimport QtQuick\nimport QtQuick.Layouts\nRow {}\n";
        let lowered = "/*\n * A dial.\n */\npragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Layout\n\nRow {}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "/*\n * A dial.\n */\nimport Ljos.Ui\nimport Ljos.Layout\nRow {}\n"
        );
    }

    /// A `//` or a `;` inside a quoted import or a `/* */` comment is not
    /// where a comment starts or an import ends.
    #[test]
    fn a_quoted_url_and_an_inline_comment_are_whole() {
        let written = "import QtQuick\nimport \"http://example.com/parts\"\nItem {}\n";
        let lowered = "pragma Scripts: JavaScript; import Ljos.Ui; import \"http://example.com/parts\" /* a; b */ // end\n\nItem {}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "import Ljos.Ui\nimport \"http://example.com/parts\" /* a; b */ // end\nItem {}\n"
        );
    }

    /// A `/* */` comment the first import opened and a later line closes
    /// stays with the last import, and the import lines under it the pass
    /// emptied still go.
    #[test]
    fn a_block_comment_the_first_import_opens_is_read_past() {
        let written = "import QtQuick /* items,\n   all */\nimport QtQuick.Layouts\nRow {}\n";
        let lowered = "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Layout /* items,\n   all */\n\nRow {}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "import Ljos.Ui\nimport Ljos.Layout /* items,\n   all */\nRow {}\n"
        );
    }

    /// What follows a comment's `*/` on its line is read too: code there
    /// ends the header, so a string below that holds the pragma's text is
    /// left as it is.
    #[test]
    fn code_after_a_comment_on_its_line_ends_the_header() {
        for text in [
            "/* a */ Item { property string s: \"\npragma Scripts: JavaScript; import A\n\" }\n",
            "/* a\n */ Item { property string s: \"\npragma Scripts: JavaScript; import A\n\" }\n",
        ] {
            assert_eq!(migrated(text, text, true), text);
        }
    }

    /// An import above the pragma's line that opens a `/* */` comment a
    /// later line closes: the comment is the header's, and the pragma's
    /// line under it is still laid out.
    #[test]
    fn an_import_that_opens_a_comment_keeps_the_header_going() {
        let written =
            "/* c */ import A /* x\n y */\nimport QtQuick\nimport QtQuick.Layouts\nItem {}\n";
        let lowered = "/* c */ import A /* x\n y */\npragma Scripts: JavaScript; import B; import C\n\nItem {}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "/* c */ import A /* x\n y */\nimport B\nimport C\nItem {}\n"
        );
    }

    /// What the pass writes out after an object's `{` goes on lines of its
    /// own, indented as the object's members are.
    #[test]
    fn a_binding_written_after_a_brace_goes_on_a_line_of_its_own() {
        let written =
            "import QtQuick.Layouts\nColumnLayout {\n  RowLayout {\n      Text {}\n  }\n}\n";
        let lowered = "pragma Scripts: JavaScript; import Ljos.Layout\nColumn { spacing: 5;\n  Row { spacing: 5; Layout.fillWidth: false;\n      Text {}\n  }\n}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "import Ljos.Layout\nColumn {\n  spacing: 5\n  Row {\n      spacing: 5\n      Layout.fillWidth: false\n      Text {}\n  }\n}\n"
        );
    }

    /// An object whose next line closes it has its members a level in.
    #[test]
    fn a_member_of_an_object_with_nothing_else_is_a_level_in() {
        let written = "import QtQuick\nItem {\n    Grid {\n    }\n}\n";
        let lowered = "pragma Scripts: JavaScript; import Ljos.Ui\nItem {\n    Grid { columns: 4;\n    }\n}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "import Ljos.Ui\nItem {\n    Grid {\n        columns: 4\n    }\n}\n"
        );
    }

    #[test]
    fn a_binding_written_after_a_list_goes_on_a_line_of_its_own() {
        let written = "import QtQuick\nListView {\n    model: ListModel {\n        ListElement { a: 1 }\n    }\n}\n";
        let lowered = "pragma Scripts: JavaScript; import Ljos.Ui\nListView {\n    readonly property list<Rows.Row> rows: [\n        { a: 1 }\n    ]; model: rows\n}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "import Ljos.Ui\nListView {\n    readonly property list<Rows.Row> rows: [\n        { a: 1 }\n    ]\n    model: rows\n}\n"
        );
    }

    /// A line the pass emptied goes; an empty line as written stays.
    #[test]
    fn a_line_the_pass_emptied_goes() {
        let written = "import QtQuick\nItem {\n\n    PropertyChanges {\n        target: box\n        width: 3\n    }\n}\n";
        let lowered = "pragma Scripts: JavaScript; import Ljos.Ui\nItem {\n\n    PropertyChanges {\n\n        box.width: 3\n    }\n}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "import Ljos.Ui\nItem {\n\n    PropertyChanges {\n        box.width: 3\n    }\n}\n"
        );
    }

    /// What is not a plain binding the pass wrote is left where it is: a
    /// line that had a `;`, a string or a comment as written, and anything
    /// after a `{` that is not `name: value;`.
    #[test]
    fn what_a_person_wrote_on_one_line_stays_there() {
        let written =
            "import QtQuick\nItem { // a\n    Row { x: 1;\n    }\n    Column {\n    }\n}\n";
        let lowered = "pragma Scripts: JavaScript; import Ljos.Ui\nItem { // a\n    Row { x: 1; spacing: 5;\n    }\n    Column { f(); \n    }\n}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "import Ljos.Ui\nItem { // a\n    Row { x: 1; spacing: 5;\n    }\n    Column { f(); \n    }\n}\n"
        );
    }

    /// Where the two files do not have as many lines, no line below the
    /// imports is read beside another.
    #[test]
    fn a_file_of_another_length_is_left_below_its_imports() {
        let written = "import QtQuick\nItem {\n    x: 1\n}\n";
        let lowered = "pragma Scripts: JavaScript; import Ljos.Ui\nItem {\n\n    Row { spacing: 5;\n    }\n}\n";
        assert_eq!(
            migrated(written, lowered, true),
            "import Ljos.Ui\nItem {\n\n    Row { spacing: 5;\n    }\n}\n"
        );
    }

    #[test]
    fn carriage_returns_are_kept() {
        let written = "import QtQuick\r\nimport QtQuick.Layouts\r\nRow {}\r\n";
        let lowered =
            "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Layout\r\n\r\nRow {}\r\n";
        assert_eq!(
            migrated(written, lowered, false),
            "pragma Scripts: JavaScript\r\nimport Ljos.Ui\r\nimport Ljos.Layout\r\nRow {}\r\n"
        );
    }
}
