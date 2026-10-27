//! What code compiled ahead of time from a shipped file is run with
//! (ARCHITECTURE.md §5.4, the amendment for shipped files).
//!
//! An application's build turns the bindings and handlers of its QML into
//! Rust (`ljos_aot`), and registers the table that code is in with
//! [`Engine::register_compiled`]. The engine runs a compiled function in
//! place of the dynamic tiers for the expressions of a file whose content
//! key ([`ljos_ir::aot::content_key`]) is the one the code was made for: a
//! file changed since (hot reload, another build of a dependency's types)
//! runs dynamically, as does every expression the generator refused.
//!
//! The code is generated, and only generated code is meant to call what
//! is here: a [`Frame`] is how it reads, writes and emits, and every name
//! it uses is looked up through the frame before it has any effect, so
//! that where the running program is not what the build saw it stops
//! ([`Stop::Fallback`]) and the dynamic tiers run the expression whole.
//!
//! `LJOS_AOT=0` (or [`Engine::set_aot`]) turns compiled code off. With the
//! cross-check on (debug builds, or `LJOS_AOT_CHECK=1`), each compiled run
//! is compared with the dynamic one: a binding's value, and the writes and
//! emits a handler makes, in their order.

pub use crate::atom::Atom;
pub use crate::incubator::aot::{AotStats, Frame, Obj, Prop, Sig};
pub use crate::value::Value;

/// One compiled function: an expression of the file, by
/// [`ljos_ir::ExprId`].
pub type Code = fn(&Frame<'_>) -> Result<Value, Stop>;

/// Why compiled code did not finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// Something the code needs is not what the build saw, and it has done
    /// nothing yet: the dynamic tiers run the expression.
    Fallback,
    /// It failed after a write or an emit, which cannot be undone: as a
    /// script that throws there, the rest does not run.
    Thrown,
    /// A dry run of the cross-check read what a write it held back would
    /// have changed: the run is not compared.
    Inconclusive,
}

/// A shipped file's compiled code.
#[derive(Debug)]
pub struct File {
    /// The file, as the build named it: for reports.
    pub name: &'static str,
    /// [`ljos_ir::aot::content_key`] of the file the code was made from.
    pub content: [u8; 32],
    /// The context of the file's `qsTr`.
    pub context: &'static str,
    /// The texts `qsTr` translates, by [`Frame::tr`]'s index.
    pub texts: &'static [&'static str],
    pub props: &'static [PropSite],
    /// Signals the code emits, by name: declared by the file.
    pub signals: &'static [&'static str],
    /// The singleton types whose objects the code reads, by
    /// [`Frame::singleton`]'s index.
    pub singletons: &'static [Single],
    /// Functions of the file the code has inlined.
    pub methods: &'static [MethodSite],
    /// By expression: `None` for one that runs dynamically.
    pub exprs: &'static [Option<Code>],
}

/// A property the code reads or writes, by name: what it is on an object
/// is looked up when the code first meets the object's type.
#[derive(Debug, Clone, Copy)]
pub struct PropSite {
    pub name: &'static str,
    pub kind: Kind,
    pub write: bool,
}

/// What the code takes a property to hold. A property found to be of
/// another type stops the code before it does anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Number,
    Bool,
    String,
}

/// A singleton type the code reads the object of.
#[derive(Debug, Clone, Copy)]
pub enum Single {
    /// Written in QML: the file of that key (`Theme`).
    File { file: &'static str },
    /// Of a module: native, or a style's file.
    Native {
        module: &'static str,
        name: &'static str,
    },
}

/// A function of the file whose body is inlined: the object must still
/// have it, as the file's own expression `expr`.
#[derive(Debug, Clone, Copy)]
pub struct MethodSite {
    pub name: &'static str,
    pub expr: u32,
}

/// JavaScript's operators and builtins on the values the code has, to the
/// bit.
pub mod js {
    use super::Atom;
    use crate::number::{number_to_string, to_int32};

    pub fn truthy(x: f64) -> bool {
        !(x == 0.0 || x.is_nan())
    }

    /// `s.length`: UTF-16 code units.
    pub fn length(s: &Atom) -> f64 {
        crate::incubator::tier1::utf16_len(s) as f64
    }

    pub fn concat(a: &Atom, b: &Atom) -> Atom {
        if a.is_empty() {
            return b.clone();
        }
        if b.is_empty() {
            return a.clone();
        }
        let mut s = String::with_capacity(a.len() + b.len());
        s.push_str(a);
        s.push_str(b);
        Atom::from(s)
    }

    /// A number as `String(n)` writes it.
    pub fn number_text(n: f64) -> Atom {
        Atom::from(number_to_string(n))
    }

    /// The order of two strings by UTF-16 code unit: -1, 0 or 1.
    pub fn order(a: &Atom, b: &Atom) -> i32 {
        crate::incubator::tier1::compare(a, b) as i32
    }

    /// `Math.min` of two: NaN if either is, and `-0` below `+0`.
    pub fn min(a: f64, b: f64) -> f64 {
        if a.is_nan() || b.is_nan() {
            f64::NAN
        } else if a == b {
            if a.is_sign_negative() { a } else { b }
        } else {
            a.min(b)
        }
    }

    pub fn max(a: f64, b: f64) -> f64 {
        if a.is_nan() || b.is_nan() {
            f64::NAN
        } else if a == b {
            if a.is_sign_positive() { a } else { b }
        } else {
            a.max(b)
        }
    }

    pub fn pow(a: f64, b: f64) -> f64 {
        crate::incubator::tier1::pow(a, b)
    }

    pub fn abs(x: f64) -> f64 {
        x.abs()
    }

    pub fn floor(x: f64) -> f64 {
        x.floor()
    }

    pub fn ceil(x: f64) -> f64 {
        x.ceil()
    }

    pub fn round(x: f64) -> f64 {
        crate::incubator::tier1::round(x)
    }

    pub fn sqrt(x: f64) -> f64 {
        x.sqrt()
    }

    pub fn trunc(x: f64) -> f64 {
        x.trunc()
    }

    /// A zero of either sign, or NaN, is itself.
    pub fn sign(x: f64) -> f64 {
        if x > 0.0 {
            1.0
        } else if x < 0.0 {
            -1.0
        } else {
            x
        }
    }

    pub fn bit_and(a: f64, b: f64) -> f64 {
        f64::from(to_int32(a) & to_int32(b))
    }

    pub fn bit_or(a: f64, b: f64) -> f64 {
        f64::from(to_int32(a) | to_int32(b))
    }

    pub fn bit_xor(a: f64, b: f64) -> f64 {
        f64::from(to_int32(a) ^ to_int32(b))
    }

    pub fn bit_not(a: f64) -> f64 {
        f64::from(!to_int32(a))
    }

    fn by(b: f64) -> u32 {
        (to_int32(b) as u32) & 31
    }

    pub fn shl(a: f64, b: f64) -> f64 {
        f64::from(to_int32(a).wrapping_shl(by(b)))
    }

    pub fn shr(a: f64, b: f64) -> f64 {
        f64::from(to_int32(a) >> by(b))
    }

    pub fn ushr(a: f64, b: f64) -> f64 {
        f64::from((to_int32(a) as u32) >> by(b))
    }
}
