//! Tier-1 bytecode: a binding as a register program (ARCHITECTURE.md §5.4).
//!
//! It sits beside the binding's JavaScript function and means the same. The
//! compiler writes it for an expression that is wholly inside the subset the
//! engine's VM runs (literals, resolved reads and writes, value-type fields,
//! operators with JavaScript's coercions, template strings, pure builtins and
//! inlined user functions); an expression outside it has none and runs as
//! JavaScript, whole.
//!
//! The program is flat and owned like the rest of the IR: registers are
//! numbered slots, constants and names live in pools on the program, and a
//! jump names the index of the instruction it goes to. Numbers are stored as
//! their bits, so `-0` and every `NaN` survive and the type stays `Eq`.
//!
//! A register holds a value in the form JavaScript would see it (a number is
//! a double, a URL is a string). What the VM cannot do exactly as the JS
//! function would (an operand that is not a primitive where a coercion is
//! needed, a name the object does not have), it does not do: it gives up and
//! the JS function runs instead. Programs are written so that this is safe:
//! everything before the one write a program may end with is free of effects.

use serde::{Deserialize, Serialize};

use crate::{Arg, CompIdx, ExprId};

/// A register of the program's frame.
pub type Reg = u16;

/// One binding as a register program.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tier1 {
    /// The size of the frame.
    pub registers: u16,
    pub consts: Vec<Const>,
    /// Property, key and method names, by index.
    pub names: Vec<String>,
    /// The parameters of inlined functions. [`Op::Param`] numbers them after
    /// the binding's own.
    pub args: Vec<InlineArg>,
    pub code: Vec<Op>,
}

impl Tier1 {
    /// Whether a function of the file is inlined in the program.
    pub fn has_inlined_calls(&self) -> bool {
        self.code.iter().any(|op| matches!(op, Op::Guard { .. }))
    }

    /// Whether the program ends with a write to a property.
    pub fn writes(&self) -> bool {
        self.code.iter().any(|op| matches!(op, Op::Set { .. }))
    }
}

/// A parameter of an inlined function: what the function's own JS form would
/// have been passed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineArg {
    /// The component the function is declared in: `arg` is resolved from
    /// that component's instance, as it is when the function is called.
    pub component: CompIdx,
    pub arg: Arg,
}

/// A constant of a program.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Const {
    Undefined,
    Null,
    Bool(bool),
    /// The bits of the double.
    Number(u64),
    String(String),
}

impl Const {
    pub fn number(n: f64) -> Self {
        Const::Number(n.to_bits())
    }
}

/// One instruction. Registers are written `dst` first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    /// `dst = consts[k]`.
    Const {
        dst: Reg,
        k: u16,
    },
    Move {
        dst: Reg,
        src: Reg,
    },
    /// The value in `reg` as it is once it has gone through the engine to
    /// another JS function: an argument of an inlined call, or what the
    /// call gives back. A zero loses its sign there.
    Pass {
        reg: Reg,
    },
    /// A parameter of the binding's function, as the JS function would be
    /// passed it: `params[param]` of the binding, or past those
    /// [`Tier1::args`]. Reading one can have effects the JS call has too (a
    /// context property is a tracked read), so the program loads them in the
    /// order the JS call evaluates them.
    Param {
        dst: Reg,
        param: u16,
    },
    /// An argument of the call (a signal's, a method's); `undefined` when
    /// the caller passed fewer.
    Local {
        dst: Reg,
        index: u16,
    },
    /// `this`: the scope object.
    This {
        dst: Reg,
    },
    /// `dst = obj.a.b.c`: the path is the `len` names of [`Tier1::names`]
    /// from index `path` on. A grouped property and its member
    /// (`border.width`) are read as the one property they are.
    Get {
        dst: Reg,
        obj: Reg,
        path: u16,
        len: u8,
    },
    /// `dst = obj[index]`.
    Index {
        dst: Reg,
        obj: Reg,
        index: Reg,
    },
    /// `dst = Type.Key`: the key of an enum of the type the parameter
    /// `param` names (an [`Arg::Type`] or [`Arg::Singleton`]); `undefined`
    /// when the type has no such key.
    EnumKey {
        dst: Reg,
        param: u16,
        key: u16,
    },
    /// `dst = Qt.Key`, an enum key of the `Qt` object.
    QtKey {
        dst: Reg,
        key: u16,
    },
    Binary {
        op: BinaryOp,
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    Unary {
        op: UnaryOp,
        dst: Reg,
        src: Reg,
    },
    Jump {
        to: u32,
    },
    /// Jumps when `cond` is truthy.
    JumpIf {
        cond: Reg,
        to: u32,
    },
    /// Jumps when `cond` is falsy.
    JumpIfNot {
        cond: Reg,
        to: u32,
    },
    /// Jumps when `src` is `null` or `undefined`.
    JumpIfNullish {
        src: Reg,
        to: u32,
    },
    /// A template string: the registers `base..base + count`, each
    /// converted to a string, joined.
    Concat {
        dst: Reg,
        base: Reg,
        count: u16,
    },
    /// An array of the registers `base..base + count`.
    Array {
        dst: Reg,
        base: Reg,
        count: u16,
    },
    /// A builtin, called with the registers `base..base + count`. For a
    /// method (`String#arg`) the first of them is the receiver.
    Call {
        dst: Reg,
        f: Builtin,
        base: Reg,
        count: u8,
    },
    /// Before the arguments of a method call are evaluated: `src` must be a
    /// value the builtin `f` is the method of. JavaScript looks the method
    /// up first, and fails there without having read what the arguments
    /// read.
    Check {
        src: Reg,
        f: Builtin,
    },
    /// Before an inlined call of `obj.name(...)`: the function `obj` has by
    /// that name must be `expr` of this file, the one whose body follows.
    /// A file that derives from this one may have declared another, and a
    /// hot reload may have replaced it.
    Guard {
        obj: Reg,
        name: u16,
        expr: ExprId,
    },
    /// The object a write goes to, for `obj.name.last = v`: `obj` itself
    /// when `name` is a grouped property of it (then `grouped` is true and
    /// nothing is read), else what `obj.name` holds.
    Target {
        dst: Reg,
        grouped: Reg,
        obj: Reg,
        name: u16,
    },
    /// `obj.name = val`. With `group` (a register [`Op::Target`] set, and
    /// the name it looked at): the write is to the member `name` of that
    /// grouped property when the register says so.
    Set {
        obj: Reg,
        name: u16,
        val: Reg,
        group: Option<(Reg, u16)>,
    },
    Return {
        src: Reg,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Exp,
    /// `==`.
    Eq,
    /// `!=`.
    Ne,
    /// `===`.
    StrictEq,
    /// `!==`.
    StrictNe,
    Lt,
    Le,
    Gt,
    Ge,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    /// `>>`.
    Shr,
    /// `>>>`.
    UShr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    /// `+x`: the number `x` converts to.
    Plus,
    Not,
    BitNot,
    Typeof,
}

/// A function the VM has in Rust. Each gives what the JS function of the
/// same name gives for the arguments it accepts, and gives up on the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Builtin {
    MathAbs,
    MathAcos,
    MathAsin,
    MathAtan,
    MathAtan2,
    MathCbrt,
    MathCeil,
    MathCos,
    MathExp,
    MathFloor,
    MathLog,
    MathLog10,
    MathLog2,
    MathMax,
    MathMin,
    MathPow,
    MathRound,
    MathSign,
    MathSin,
    MathSqrt,
    MathTan,
    MathTrunc,
    /// `Number(x)`.
    Number,
    /// `String(x)`.
    String,
    /// `Boolean(x)`.
    Boolean,
    IsNaN,
    IsFinite,
    QtRgba,
    QtHsla,
    QtHsva,
    QtPoint,
    QtSize,
    QtRect,
    QtVector2d,
    QtVector3d,
    QtVector4d,
    QtQuaternion,
    /// `qsTr`: the first register is the translation context of the file.
    QsTr,
    QsTranslate,
    QsTrId,
    /// `QT_TR_NOOP`, `QT_TRID_NOOP`: the first argument.
    TrNoop,
    /// `QT_TRANSLATE_NOOP`: the second argument.
    TranslateNoop,
    /// `String.prototype.arg`.
    Arg,
    /// `Number.prototype.toFixed`.
    ToFixed,
    /// `toString()` of a primitive.
    ToString,
    /// `String.prototype.toUpperCase`.
    ToUpperCase,
    /// `String.prototype.toLowerCase`.
    ToLowerCase,
    /// `String.prototype.trim`.
    Trim,
}
