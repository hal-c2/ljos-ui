//! The QML front end: what a project's QML files mean, short of running them.
//!
//! [`qml_syntax`] gives the tree of one file. This crate reads what the files
//! say about each other and about the types they name:
//!
//! - [`discover`]: which modules a project has, from its `qmldir` files and
//!   its `CMakeLists.txt`.
//! - [`project`]: the files of a project, each summarised: its imports, and
//!   what each of its components declares.
//! - [`types`]: what a type name is in a file, and what an object of that
//!   type has. Types that are not QML files come from a [`TypeOracle`]: Qt's
//!   description of its own ([`QtTypeOracle`]), or a runtime's registry.
//! - [`scope`]: the objects of a file, the components (contexts) they are in,
//!   and what each object declares.
//! - [`names`]: what a free name in a script is, in QML's lookup order.
//!
//! Nothing here emits code or decides how a binding is evaluated: it says
//! *what* a name is, and a back end decides what to do with it. The front end
//! is shared by Ljós and qml-solid (ARCHITECTURE.md §4, §5.2).

pub mod discover;
pub mod names;
pub mod oracle;
pub mod project;
pub mod scope;
pub mod types;

pub use oracle::{Property, QtType, QtTypeOracle, Special, TypeOracle, unknown_type};
pub use qml_syntax::Span;

/// What kind of problem an [`Error`] is, for a caller that acts on the kind
/// and not on the wording of the message.
///
/// The names from [`as_str`](Code::as_str) are stable: tools show them to
/// people and match on them. New kinds may be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Code {
    /// The parser rejected part of the file.
    Syntax,
    /// An import names a module nothing knows.
    UnknownModule,
    /// An object type, or the type an attached property is written on, is
    /// not provided by anything the file imports.
    UnknownType,
    /// A binding names a property the type does not have.
    UnknownProperty,
    /// A handler is of a signal the type does not have.
    UnknownSignal,
    /// Two objects of one component share an `id`.
    DuplicateId,
    /// A `property alias` points at an id the component does not have.
    UnknownAliasTarget,
    /// A free name in a script that nothing in reach has.
    UnresolvedName,
    /// Anything else.
    Other,
}

impl Code {
    /// The stable, hyphenated name: `unknown-type`.
    pub fn as_str(self) -> &'static str {
        match self {
            Code::Syntax => "syntax",
            Code::UnknownModule => "unknown-module",
            Code::UnknownType => "unknown-type",
            Code::UnknownProperty => "unknown-property",
            Code::UnknownSignal => "unknown-signal",
            Code::DuplicateId => "duplicate-id",
            Code::UnknownAliasTarget => "unknown-alias-target",
            Code::UnresolvedName => "unresolved-name",
            Code::Other => "semantic",
        }
    }
}

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Something wrong with what a file says, at the place it says it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct Error {
    pub code: Code,
    pub message: String,
    pub span: Span,
}

impl Error {
    /// An error of no particular kind ([`Code::Other`]).
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self::with_code(Code::Other, message, span)
    }

    pub fn with_code(code: Code, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span,
        }
    }
}

impl From<qml_syntax::Diagnostic> for Error {
    fn from(diagnostic: qml_syntax::Diagnostic) -> Self {
        Self::with_code(Code::Syntax, diagnostic.message, diagnostic.span)
    }
}
