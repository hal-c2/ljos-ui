//! A QML formatter: the layout behind `ljos fmt` and the `qml_format`
//! authoring tool.
//!
//! # What it changes
//!
//! Only the QML structure is laid out: one member a line, four spaces a
//! level, `name: value` with one space, `Type {` on the line of the type,
//! `Type {}` for an object with nothing in it, a line for each object of an
//! object list, and no `;` after a member. Pragmas and imports are written
//! with single spaces and keep their order, and one empty line separates
//! them from what follows.
//!
//! # What it keeps
//!
//! - **JavaScript.** A binding's expression, a handler's block, a function
//!   and a signal's or an enum's declaration are copied as they were
//!   written. Lines after the first move with the member (they keep their
//!   distance to it), and nothing inside is rewritten or wrapped.
//! - **Text over several lines.** A line that starts inside a template
//!   literal or a continued string is not touched at all: its indentation
//!   is part of the text.
//! - **Comments**, each where the author put it: on a line of its own, or
//!   after what it trails.
//! - **Groups of lines.** One empty line is kept wherever the author left
//!   one or more between two members; none is kept after `{` or before `}`.
//! - **Order.** Nothing is sorted: not the imports, not the members.
//!
//! # What it promises
//!
//! [`format`] is deterministic (no configuration, no environment) and
//! idempotent (`format(format(x)) == format(x)`). Before it answers, it
//! parses its own output and compares it with the input: the QML structure
//! node by node, every JavaScript node by content, and every comment. A
//! difference is an [`Error::Unstable`] and no text is returned, so a fault
//! in the formatter cannot change what a file means. A file that does not
//! parse is refused with its diagnostics ([`Error::Syntax`]): a layout
//! built on a damaged tree would lose what the parser skipped.
//!
//! # Where it lives
//!
//! Beside the front end, on `qml_syntax` alone. The CLI and the authoring
//! server share it, and no runtime crate may depend on it (the parser is
//! not shipped in applications, ARCHITECTURE.md §5).

mod diff;
mod layout;
mod same;

pub use diff::unified_diff;

/// One problem the parser reported, at a 1-based line and column (the
/// column counts characters).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The text is not valid QML. Never empty.
    #[error("the text does not parse as QML ({} problem(s); first: {})", .0.len(), .0.first().map_or_else(String::new, ToString::to_string))]
    Syntax(Vec<SyntaxError>),
    /// The formatter's own check failed: its output would not mean what
    /// the input means. Nothing was changed.
    #[error(
        "formatting would change what the text means ({0}); nothing was changed. This is a fault in the formatter, not in the text"
    )]
    Unstable(String),
}

/// Line ends as `\n` and no byte order mark: what the layout is worked out
/// on, and what [`same_tree`] compares.
fn normalise(source: &str) -> String {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    source.replace("\r\n", "\n")
}

/// Lays `source` out. See the crate documentation for the rules.
///
/// # Errors
/// [`Error::Syntax`] when `source` does not parse, [`Error::Unstable`] when
/// the result failed the formatter's own comparison with the input.
pub fn format(source: &str) -> Result<String, Error> {
    let source = normalise(source);
    let allocator = qml_syntax::Allocator::default();
    let parsed = qml_syntax::parse(&allocator, &source);
    if !parsed.is_ok() {
        return Err(Error::Syntax(syntax_errors(&parsed, &source)));
    }
    let out = layout::print(&source, &parsed);
    let again = qml_syntax::parse(&allocator, &out);
    if !again.is_ok() {
        let first = syntax_errors(&again, &out)
            .first()
            .map_or_else(String::new, ToString::to_string);
        return Err(Error::Unstable(format!(
            "the result does not parse: {first}"
        )));
    }
    same::documents(&source, &parsed, &out, &again).map_err(Error::Unstable)?;
    Ok(out)
}

fn syntax_errors(parsed: &qml_syntax::Parsed<'_>, source: &str) -> Vec<SyntaxError> {
    let mut errors: Vec<SyntaxError> = parsed
        .diagnostics
        .iter()
        .map(|d| {
            let (line, column) = d.position(source);
            SyntaxError {
                line: line as u32,
                column: column as u32,
                message: d.message.clone(),
            }
        })
        .collect();
    if errors.is_empty() {
        // `panicked` without a diagnostic: still say something.
        errors.push(SyntaxError {
            line: 1,
            column: 1,
            message: "the file has no root object".to_owned(),
        });
    }
    errors
}

/// Whether two QML texts are the same document: the same pragmas and
/// imports in the same order, the same tree of objects and members, the
/// same JavaScript (compared node by node, so spacing does not count) and
/// the same comments. `Err` says where the first difference is.
///
/// # Errors
/// A description of the first difference, or of why a text did not parse.
pub fn same_tree(a: &str, b: &str) -> Result<(), String> {
    let (a, b) = (normalise(a), normalise(b));
    let allocator = qml_syntax::Allocator::default();
    let pa = qml_syntax::parse(&allocator, &a);
    let pb = qml_syntax::parse(&allocator, &b);
    for (parsed, which) in [(&pa, "first"), (&pb, "second")] {
        if !parsed.is_ok() {
            return Err(format!("the {which} text does not parse"));
        }
    }
    same::documents(&a, &pa, &b, &pb)
}
