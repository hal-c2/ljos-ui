//! What a script of a file in Ljós's API may use that is not QML's: the
//! built-ins TYPES.md lists, and nothing else.
//!
//! DECISION: only the read-only array methods TYPES.md lists exist on a
//! readonly array (`forEach`, `findIndex` and the rest are not in the API);
//! a mutable one (`T[]`) adds the mutators. The callbacks of `filter`,
//! `find`, `some` and `every` return a boolean: the subset has no
//! truthiness. `String.prototype.arg` is Qt's, and takes a string, a
//! number or a boolean.

use super::ty::{Arena, Global, Kind, Method, Param, Sig, Ty};

/// The globals a script may name, by name.
pub(super) fn global(name: &str) -> Option<Global> {
    Some(match name {
        "Math" => Global::Math,
        "Number" => Global::Number,
        "String" => Global::String,
        "console" => Global::Console,
        "Qt" => Global::Qt,
        "qsTr" => Global::QsTr,
        "qsTrId" => Global::QsTrId,
        "Map" => Global::Map,
        "Array" => Global::Array,
        _ => return None,
    })
}

const MATH_CONSTANTS: &[&str] = &[
    "E", "LN10", "LN2", "LOG10E", "LOG2E", "PI", "SQRT1_2", "SQRT2",
];
/// `Math`'s functions of one number.
const MATH_UNARY: &[&str] = &[
    "abs", "acos", "acosh", "asin", "asinh", "atan", "atanh", "cbrt", "ceil", "cos", "cosh", "exp",
    "expm1", "floor", "fround", "log", "log10", "log1p", "log2", "round", "sign", "sin", "sinh",
    "sqrt", "tan", "tanh", "trunc",
];
const NUMBER_CONSTANTS: &[&str] = &[
    "EPSILON",
    "MAX_SAFE_INTEGER",
    "MAX_VALUE",
    "MIN_SAFE_INTEGER",
    "MIN_VALUE",
    "NaN",
    "NEGATIVE_INFINITY",
    "POSITIVE_INFINITY",
];
const CONSOLE: &[&str] = &["log", "warn", "error"];

/// What `global.name` is.
pub(super) fn global_member(arena: &mut Arena, global: Global, name: &str) -> Option<Ty> {
    let method = |arena: &mut Arena, name: &'static str| {
        arena.intern(Kind::Method(Ty::VOID, Method::Static(global, name)))
    };
    match global {
        Global::Math if MATH_CONSTANTS.contains(&name) => Some(Ty::NUMBER),
        Global::Math => {
            let found = MATH_UNARY
                .iter()
                .chain(["atan2", "pow", "max", "min", "hypot", "random", "imul"].iter())
                .find(|known| **known == name)?;
            Some(method(arena, found))
        }
        Global::Number if NUMBER_CONSTANTS.contains(&name) => Some(Ty::NUMBER),
        Global::Number => {
            let found = [
                "isFinite",
                "isInteger",
                "isNaN",
                "isSafeInteger",
                "parseFloat",
                "parseInt",
            ]
            .into_iter()
            .find(|known| *known == name)?;
            Some(method(arena, found))
        }
        Global::String => (name == "fromCharCode").then(|| method(arena, "fromCharCode")),
        Global::Console => {
            let found = CONSOLE.iter().find(|known| **known == name)?;
            Some(method(arena, found))
        }
        Global::Array => (name == "isArray").then(|| method(arena, "isArray")),
        Global::Qt | Global::QsTr | Global::QsTrId | Global::Map => None,
    }
}

fn param(name: &str, ty: Ty) -> Param {
    Param {
        name: name.into(),
        ty,
        optional: false,
        written: None,
    }
}

fn optional(name: &str, ty: Ty) -> Param {
    Param {
        optional: true,
        ..param(name, ty)
    }
}

fn sig(params: Vec<Param>, rest: Option<Ty>, ret: Ty) -> Sig {
    Sig {
        params: params.into_boxed_slice(),
        rest,
        ret,
        guard: None,
    }
}

/// What calling a global itself takes and gives: `Number(x)`, `qsTr(…)`.
pub(super) fn global_call(arena: &mut Arena, global: Global) -> Option<Sig> {
    let primitive = arena.union([Ty::STRING, Ty::NUMBER, Ty::BOOLEAN]);
    Some(match global {
        Global::Number => sig(vec![param("value", primitive)], None, Ty::NUMBER),
        Global::String => sig(vec![param("value", primitive)], None, Ty::STRING),
        Global::QsTr => sig(
            vec![
                param("text", Ty::STRING),
                optional("disambiguation", Ty::STRING),
                optional("n", Ty::NUMBER),
            ],
            None,
            Ty::STRING,
        ),
        Global::QsTrId => sig(
            vec![param("id", Ty::STRING), optional("n", Ty::NUMBER)],
            None,
            Ty::STRING,
        ),
        Global::Math | Global::Console | Global::Qt | Global::Map | Global::Array => return None,
    })
}

/// A primitive's kind, for its members.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Primitive {
    String,
    Number,
    Boolean,
}

const STRING_METHODS: &[&str] = &[
    "arg",
    "at",
    "charAt",
    "charCodeAt",
    "codePointAt",
    "concat",
    "endsWith",
    "includes",
    "indexOf",
    "lastIndexOf",
    "localeCompare",
    "padEnd",
    "padStart",
    "repeat",
    "replace",
    "replaceAll",
    "slice",
    "split",
    "startsWith",
    "substring",
    "toLowerCase",
    "toString",
    "toUpperCase",
    "trim",
    "trimEnd",
    "trimStart",
];
const NUMBER_METHODS: &[&str] = &["toFixed", "toPrecision", "toString"];

/// What `value.name` is on a string, a number or a boolean.
pub(super) fn primitive_member(
    arena: &mut Arena,
    receiver: Ty,
    primitive: Primitive,
    name: &str,
) -> Option<Ty> {
    let methods = match primitive {
        Primitive::String if name == "length" => return Some(Ty::NUMBER),
        Primitive::String => STRING_METHODS,
        Primitive::Number => NUMBER_METHODS,
        Primitive::Boolean => &["toString"][..],
    };
    let found = methods.iter().find(|known| **known == name)?;
    Some(arena.intern(Kind::Method(receiver, Method::Primitive(found))))
}

const ARRAY_READ: &[&str] = &[
    "concat", "every", "filter", "find", "includes", "indexOf", "join", "map", "reduce", "slice",
    "some",
];
/// What changes an array in place: only a mutable one has them.
pub(super) const ARRAY_WRITE: &[&str] = &[
    "copyWithin",
    "fill",
    "pop",
    "push",
    "reverse",
    "shift",
    "sort",
    "splice",
    "unshift",
];

/// What `array.name` is.
pub(super) fn array_member(arena: &mut Arena, receiver: Ty, name: &str) -> Option<Ty> {
    if name == "length" {
        return Some(Ty::NUMBER);
    }
    let Kind::Array { readonly, .. } = *arena.kind(receiver) else {
        return None;
    };
    let found = ARRAY_READ
        .iter()
        .chain(if readonly { &[][..] } else { ARRAY_WRITE }.iter())
        .find(|known| **known == name)?;
    Some(arena.intern(Kind::Method(receiver, Method::Array(found))))
}

const MAP_METHODS: &[&str] = &["clear", "delete", "get", "has", "set"];

pub(super) fn map_member(arena: &mut Arena, receiver: Ty, name: &str) -> Option<Ty> {
    if name == "size" {
        return Some(Ty::NUMBER);
    }
    let found = MAP_METHODS.iter().find(|known| **known == name)?;
    Some(arena.intern(Kind::Method(receiver, Method::Map(found))))
}

/// What calling a built-in method takes and gives, for those whose types
/// do not hang on a callback's: `map` and `reduce` are the checker's.
pub(super) fn method_sig(arena: &mut Arena, receiver: Ty, method: Method) -> Option<Sig> {
    let primitive = arena.union([Ty::STRING, Ty::NUMBER, Ty::BOOLEAN]);
    let maybe_string = arena.or(Ty::STRING, Ty::UNDEFINED);
    let maybe_number = arena.or(Ty::NUMBER, Ty::UNDEFINED);
    Some(match method {
        Method::Static(Global::Math, "random") => sig(vec![], None, Ty::NUMBER),
        Method::Static(Global::Math, "atan2" | "pow" | "imul") => sig(
            vec![param("a", Ty::NUMBER), param("b", Ty::NUMBER)],
            None,
            Ty::NUMBER,
        ),
        Method::Static(Global::Math, "max" | "min" | "hypot") => {
            sig(vec![], Some(Ty::NUMBER), Ty::NUMBER)
        }
        Method::Static(Global::Math, _) => sig(vec![param("x", Ty::NUMBER)], None, Ty::NUMBER),
        Method::Static(Global::Number, "parseFloat") => {
            sig(vec![param("text", Ty::STRING)], None, Ty::NUMBER)
        }
        Method::Static(Global::Number, "parseInt") => sig(
            vec![param("text", Ty::STRING), optional("radix", Ty::NUMBER)],
            None,
            Ty::NUMBER,
        ),
        Method::Static(Global::Number, _) => {
            sig(vec![param("value", Ty::NUMBER)], None, Ty::BOOLEAN)
        }
        Method::Static(Global::String, _) => sig(vec![], Some(Ty::NUMBER), Ty::STRING),
        Method::Static(Global::Console, _) => sig(vec![], Some(Ty::UNKNOWN), Ty::VOID),
        // What it narrows its argument to is the checker's (`facts`).
        Method::Static(Global::Array, "isArray") => {
            sig(vec![param("value", Ty::UNKNOWN)], None, Ty::BOOLEAN)
        }
        Method::Static(..) => return None,
        Method::Primitive(name) => {
            let string = |name: &str| param(name, Ty::STRING);
            let number = |name: &str| param(name, Ty::NUMBER);
            let maybe = |name: &str| optional(name, Ty::NUMBER);
            if arena.is_number(receiver) {
                return Some(match name {
                    "toString" => sig(vec![optional("radix", Ty::NUMBER)], None, Ty::STRING),
                    _ => sig(vec![optional("digits", Ty::NUMBER)], None, Ty::STRING),
                });
            }
            if arena.is_boolean(receiver) {
                return Some(sig(vec![], None, Ty::STRING));
            }
            match name {
                "arg" => sig(vec![param("value", primitive)], None, Ty::STRING),
                "at" => sig(vec![number("index")], None, maybe_string),
                "charAt" => sig(vec![number("index")], None, Ty::STRING),
                "charCodeAt" => sig(vec![number("index")], None, Ty::NUMBER),
                "codePointAt" => sig(vec![number("index")], None, maybe_number),
                "concat" => sig(vec![], Some(Ty::STRING), Ty::STRING),
                "endsWith" | "includes" | "startsWith" => {
                    sig(vec![string("search"), maybe("position")], None, Ty::BOOLEAN)
                }
                "indexOf" | "lastIndexOf" => {
                    sig(vec![string("search"), maybe("position")], None, Ty::NUMBER)
                }
                "localeCompare" => sig(vec![string("other")], None, Ty::NUMBER),
                "padEnd" | "padStart" => sig(
                    vec![number("length"), optional("fill", Ty::STRING)],
                    None,
                    Ty::STRING,
                ),
                "repeat" => sig(vec![number("count")], None, Ty::STRING),
                "replace" | "replaceAll" => sig(
                    vec![string("search"), string("replacement")],
                    None,
                    Ty::STRING,
                ),
                "slice" | "substring" => sig(vec![maybe("start"), maybe("end")], None, Ty::STRING),
                "split" => {
                    let parts = arena.array(Ty::STRING, true);
                    sig(vec![string("separator"), maybe("limit")], None, parts)
                }
                _ => sig(vec![], None, Ty::STRING),
            }
        }
        Method::Array(name) => {
            let Kind::Array { elem, .. } = *arena.kind(receiver) else {
                return None;
            };
            let index = param("index", Ty::NUMBER);
            let predicate = arena.function(sig(
                vec![param("value", elem), optional("index", Ty::NUMBER)],
                None,
                Ty::BOOLEAN,
            ));
            let readonly = arena.array(elem, true);
            let maybe_elem = arena.or(elem, Ty::UNDEFINED);
            match name {
                "every" | "some" => sig(vec![param("predicate", predicate)], None, Ty::BOOLEAN),
                "filter" => sig(vec![param("predicate", predicate)], None, readonly),
                "find" => sig(vec![param("predicate", predicate)], None, maybe_elem),
                "includes" => sig(vec![param("value", elem)], None, Ty::BOOLEAN),
                "indexOf" => sig(vec![param("value", elem)], None, Ty::NUMBER),
                "join" => sig(vec![optional("separator", Ty::STRING)], None, Ty::STRING),
                "slice" => sig(
                    vec![optional("start", Ty::NUMBER), optional("end", Ty::NUMBER)],
                    None,
                    readonly,
                ),
                "concat" => {
                    let item = arena.or(elem, readonly);
                    sig(vec![], Some(item), readonly)
                }
                "push" | "unshift" => sig(vec![], Some(elem), Ty::NUMBER),
                "pop" | "shift" => sig(vec![], None, maybe_elem),
                "reverse" => sig(vec![], None, receiver),
                "sort" => {
                    let compare = arena.function(sig(
                        vec![param("a", elem), param("b", elem)],
                        None,
                        Ty::NUMBER,
                    ));
                    sig(vec![optional("compare", compare)], None, receiver)
                }
                "fill" => sig(
                    vec![
                        param("value", elem),
                        optional("start", Ty::NUMBER),
                        optional("end", Ty::NUMBER),
                    ],
                    None,
                    receiver,
                ),
                "splice" => sig(
                    vec![param("start", Ty::NUMBER), optional("count", Ty::NUMBER)],
                    Some(elem),
                    readonly,
                ),
                "copyWithin" => sig(
                    vec![
                        index,
                        param("start", Ty::NUMBER),
                        optional("end", Ty::NUMBER),
                    ],
                    None,
                    receiver,
                ),
                _ => return None,
            }
        }
        Method::Map(name) => {
            let Kind::Map { key, value } = *arena.kind(receiver) else {
                return None;
            };
            let maybe_value = arena.or(value, Ty::UNDEFINED);
            match name {
                "get" => sig(vec![param("key", key)], None, maybe_value),
                "set" => sig(
                    vec![param("key", key), param("value", value)],
                    None,
                    receiver,
                ),
                "has" | "delete" => sig(vec![param("key", key)], None, Ty::BOOLEAN),
                "clear" => sig(vec![], None, Ty::VOID),
                _ => return None,
            }
        }
    })
}
