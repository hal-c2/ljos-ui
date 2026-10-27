//! What compiling a shipped file ahead of time reads (ARCHITECTURE.md §5.4,
//! the amendment for shipped files).
//!
//! The functions of a file's module are JavaScript text, which the runtime
//! never parses itself. A build that compiles a file to Rust needs their
//! code as a tree, and that tree is here, beside the rest of the IR, so that
//! the generator (`ljos_aot`) reads the IR and nothing else: the compiler
//! lowers a function to a [`Body`] or says why it cannot, and the generator
//! types it against the file's declarations.
//!
//! The tree is the part of JavaScript a typed QML script is written in:
//! literals, names, members, calls, the operators, `if`, `switch`, `return`
//! and local `const`/`let`. Anything else never reaches it.
//!
//! [`content_key`] is how compiled code is matched to a file: the runtime
//! uses code compiled for a file only when the file it loaded is the very one
//! the code was made from.

use serde::{Deserialize, Serialize};

pub use crate::tier1::{BinaryOp, UnaryOp};
use crate::{CompiledFile, IR_VERSION};

/// A file's module as the generator reads it: by [`crate::ExprId`], each
/// expression's function as a tree, or why it is not one.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Module {
    pub functions: Vec<Result<Body, Refused>>,
    /// The context of the file's own `qsTr`, when the module binds one.
    pub context: Option<String>,
}

/// Why an expression stays with the dynamic tiers: what a report groups by
/// and which one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Refused {
    pub kind: String,
    pub detail: String,
}

impl Refused {
    pub fn new(kind: impl Into<String>, detail: impl Into<String>) -> Self {
        Refused {
            kind: kind.into(),
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.detail.is_empty() {
            f.write_str(&self.kind)
        } else {
            write!(f, "{}: {}", self.kind, self.detail)
        }
    }
}

/// One function of the module, as the generator reads it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Body {
    /// The names the call's arguments are given: a signal's parameters, a
    /// function's.
    pub formals: Vec<String>,
    pub code: Code,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Code {
    Statements(Vec<Stmt>),
    /// An arrow function written without braces: its value is the result.
    Expression(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stmt {
    Expr(Expr),
    /// `const` or `let`, one name.
    Let {
        name: String,
        constant: bool,
        init: Option<Expr>,
    },
    If {
        test: Expr,
        then: Vec<Stmt>,
        otherwise: Vec<Stmt>,
    },
    Return(Option<Expr>),
    /// Cases are compared with `===`; `None` is `default`. A case without
    /// a `break` or `return` at its end falls into the next.
    Switch {
        on: Expr,
        cases: Vec<(Option<Expr>, Vec<Stmt>)>,
    },
    /// Out of the `switch` around it.
    Break,
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Number(f64),
    String(String),
    Bool(bool),
    Null,
    Undefined,
    /// A parameter, a local or a global.
    Name(String),
    Member(Box<Expr>, String),
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    /// `&&` when true, `||` when false.
    Logical {
        and: bool,
        a: Box<Expr>,
        b: Box<Expr>,
    },
    Conditional {
        test: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    /// `target = value`, or `target op= value` with `op`.
    Assign {
        op: Option<BinaryOp>,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    /// `++` or `--`, before or after.
    Update {
        increment: bool,
        prefix: bool,
        target: Box<Expr>,
    },
}

/// What a file's compiled code is matched by: the whole of the compiled
/// file, so that code made from other text, or from the same text compiled
/// against other types, is never run for it. A file a development build
/// changes (hot reload) has another key and runs in the dynamic tiers.
pub fn content_key(file: &CompiledFile) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&IR_VERSION.to_le_bytes());
    // The IR serialises: a file that did not would have no key worth
    // having, and gets one nothing matches.
    match postcard::to_allocvec(file) {
        Ok(bytes) => {
            hasher.update(&bytes);
        }
        Err(_) => {
            hasher.update(b"unserialisable");
            hasher.update(file.file.as_bytes());
            hasher.update(&file.source_hash);
        }
    }
    *hasher.finalize().as_bytes()
}
