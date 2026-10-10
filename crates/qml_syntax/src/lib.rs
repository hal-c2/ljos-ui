//! QML syntax for Ljós and qml-solid.
//!
//! The grammar lives in the vendored fork of `oxc_parser` (`vendor/oxc_parser`,
//! see `vendor/FORK.md`): QML mode needs the parser's crate-private cursor and
//! lexer, so it cannot sit on oxc's public API. This crate is the stable face
//! of that fork. Callers name `qml_syntax::ast::QmlDocument`, not the fork's
//! module path, so a rebase or a rename of the fork touches only this crate.
//!
//! # Error recovery
//!
//! One typo does not fail the whole file. [`parse`] always returns a
//! [`Parsed`] whose `document` holds everything that did parse, and whose
//! `diagnostics` hold one entry per problem, sorted by position.
//!
//! - The unit of recovery is an object member (binding, property, signal,
//!   handler, function, enum, inline component, nested object) and, in the
//!   header, an import or a pragma. A unit that fails is **omitted** from the
//!   tree, with no placeholder node, so consumers need not know about errors.
//!   Errors inside an embedded JavaScript expression or block belong to the
//!   member that contains them.
//! - After a failure the parser skips to a synchronisation point: past a `;`,
//!   or up to a `}` that closes the enclosing object, or up to a name on a new
//!   line that starts a member (`name:`, `Type {`, `a.b`, `property x`,
//!   `function`, `enum`). Brackets opened by the broken member are skipped as
//!   a group. If they are never closed, a member starting no deeper than the
//!   broken one on a later line ends the skip. An import or pragma is skipped
//!   to the end of its line.
//! - Every recovery consumes at least one token, so parsing terminates.
//! - A diagnostic is placed where the parser noticed the problem, which for an
//!   unclosed bracket can be a later line than the cause.
//! - An object that reaches the end of the file unclosed keeps its members and
//!   reports the missing `}` once per place.
//! - A root object that lost its `{` keeps its members. A second root object
//!   is an error but the first is kept.
//! - [`Parsed::panicked`] is set only when no root object could be started
//!   (its type name is missing). The document then has the header and an
//!   empty root whose type name is `""`.
//!
//! [`Parsed::is_ok`] is still the test for a clean file. Code that must not
//! act on a damaged file (the compiler) keeps checking it; code that wants the
//! best partial tree (hot reload, an editor) reads `document` and shows the
//! diagnostics.
//!
//! # Trees and arenas
//!
//! Every binding value, handler and function in the tree is an ordinary Oxc
//! JavaScript node allocated in the caller's [`Allocator`], with spans that are
//! byte offsets into the QML source. A document cannot outlive its arena; the
//! front end lowers it to an owned IR and drops the arena (ARCHITECTURE.md §5).

use std::fmt;

pub use oxc_allocator::{Allocator, ArenaBox, ArenaVec};
pub use oxc_ast::ast::Comment;
pub use oxc_span::{GetSpan, SourceType, Span};

/// Oxc's JavaScript AST, which every embedded expression, statement and
/// function is.
pub use oxc_ast::ast as js;
/// The QML tree: pragmas, imports and objects.
pub use oxc_parser::qml::ast;

use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::Parser;

/// What [`parse`] gives back.
#[derive(Debug)]
pub struct Parsed<'a> {
    /// Everything that parsed, also when `diagnostics` is not empty: members
    /// that failed are left out (see the crate docs). Only when
    /// [`panicked`](Self::panicked) is set is the root object missing.
    pub document: ast::QmlDocument<'a>,
    pub comments: ArenaVec<'a, Comment>,
    /// Empty when the file is valid QML.
    pub diagnostics: Vec<Diagnostic>,
    /// The parser could not recover and stopped early: the file has no root
    /// object with a type name. Ordinary syntax errors do not set this.
    pub panicked: bool,
}

impl Parsed<'_> {
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty() && !self.panicked
    }
}

/// Parses a `.qml` document.
///
/// Embedded JavaScript is parsed as TypeScript so that QML's type annotations
/// on function parameters and return types are accepted.
pub fn parse<'a>(allocator: &'a Allocator, source: &'a str) -> Parsed<'a> {
    let parsed = Parser::new(allocator, source, SourceType::ts()).parse_qml();
    Parsed {
        document: parsed.document,
        comments: parsed.comments,
        diagnostics: parsed
            .diagnostics
            .into_iter()
            .map(Diagnostic::from)
            .collect(),
        panicked: parsed.panicked,
    }
}

/// A syntax error, detached from oxc's diagnostic type so that callers need
/// no `miette` or `oxc_diagnostics` dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    /// Offsets into the QML source; empty at 0 when oxc gave no label.
    pub span: Span,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    /// One-based line and column of the error in `source`, the column in
    /// characters.
    pub fn position(&self, source: &str) -> (usize, usize) {
        position(source, self.span.start)
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Diagnostic {}

impl From<OxcDiagnostic> for Diagnostic {
    fn from(diagnostic: OxcDiagnostic) -> Self {
        let span = diagnostic.labels.first().map_or(Span::default(), |label| {
            let start = label.offset();
            Span::new(start, start + label.len())
        });
        Self::new(diagnostic.message.to_string(), span)
    }
}

/// One-based line and column of byte `offset` in `source`.
pub fn position(source: &str, offset: u32) -> (usize, usize) {
    let mut end = (offset as usize).min(source.len());
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let before = &source[..end];
    let line = before.matches('\n').count() + 1;
    let column = before.chars().rev().take_while(|c| *c != '\n').count() + 1;
    (line, column)
}

/// The source text a span covers.
pub fn text(source: &str, span: Span) -> &str {
    &source[span.start as usize..span.end as usize]
}
