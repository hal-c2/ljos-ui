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

pub use oracle::{Property, QtType, QtTypeOracle, Special, TypeOracle};
pub use qml_syntax::Span;

/// Something wrong with what a file says, at the place it says it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct Error {
    pub message: String,
    pub span: Span,
}

impl Error {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

impl From<qml_syntax::Diagnostic> for Error {
    fn from(diagnostic: qml_syntax::Diagnostic) -> Self {
        Self::new(diagnostic.message, diagnostic.span)
    }
}
