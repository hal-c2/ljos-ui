//! JavaScript's built-ins: `Math`, `Number`, `String`, `Array`, `Object`,
//! `JSON`, `Date` in UTC, regular expressions and the URI functions, more
//! of them and more often than bindings uses, in bindings, functions and
//! the steps that call them. What is compared is what they return.
//!
//! Only what Qt's engine has is called: `trimStart`, `trimEnd`,
//! `replaceAll`, `at` and `flat` are Ljós's alone
//! (features/language/newer-builtins.feature).

use crate::{
    bindings::{
        ast::{Decl, Kind, Obj, ObjType, PType, Program},
        generate::{Arg, Gen, Op},
    },
    rng::Rng,
};

/// Programs whose expressions are mostly built-in calls.
#[derive(Debug, Default)]
pub struct Builtins;

crate::program_area!(Builtins, "builtins", program);

use Arg::Of;
use Kind::{Any, Arr, Bool, Num, Obj as KObj, Str};

/// The built-ins besides those every program uses.
const OPS: &[Op] = &[
    (Num, "Math.exp", "Math.exp($0)", &[Of(Num)]),
    (Num, "Math.cbrt", "Math.cbrt($0)", &[Of(Num)]),
    (Num, "Math.log2", "Math.log2($0)", &[Of(Num)]),
    (Num, "Math.log10", "Math.log10($0)", &[Of(Num)]),
    (Num, "Math.expm1", "Math.expm1($0)", &[Of(Num)]),
    (Num, "Math.asinh", "Math.asinh($0)", &[Of(Num)]),
    (Num, "Math.fround", "Math.fround($0)", &[Of(Num)]),
    (Num, "Math.clz32", "Math.clz32($0)", &[Of(Num)]),
    (Num, "Math.imul", "Math.imul($0, $1)", &[Of(Num), Of(Num)]),
    (
        Num,
        "Math.max-array",
        "Math.max.apply(null, $0)",
        &[Of(Arr)],
    ),
    (
        Num,
        "Number.parseFloat",
        "Number.parseFloat($0)",
        &[Of(Str)],
    ),
    (Num, "parseInt36", "parseInt($0, 36)", &[Of(Str)]),
    (Num, "Number-any", "Number($0)", &[Of(Any)]),
    (
        Num,
        "lastIndexOf",
        "$0.lastIndexOf($1)",
        &[Of(Str), Of(Str)],
    ),
    (Num, "search", "$0.search(/[0-9]/)", &[Of(Str)]),
    (Num, "codePointAt", "$0.codePointAt(0)", &[Of(Str)]),
    (
        Num,
        "localeCompare",
        "$0.localeCompare($1)",
        &[Of(Str), Of(Str)],
    ),
    (
        Num,
        "findIndex",
        "$0.findIndex(function (x) { return x > 1; })",
        &[Of(Arr)],
    ),
    (Num, "Date.UTC", "Date.UTC(2020, $0, 29)", &[Of(Num)]),
    (
        Num,
        "Date.getUTCDay",
        "new Date(Date.UTC(2024, 1, $0)).getUTCDay()",
        &[Of(Num)],
    ),
    (Num, "Date.parse", "Date.parse($0)", &[Of(Str)]),
    (Num, "exponent", "(($0) ** $1)", &[Of(Num), Of(Num)]),
    (Num, "Number.EPSILON", "(Number.EPSILON * $0)", &[Of(Num)]),
    (Bool, "Number.isInteger", "Number.isInteger($0)", &[Of(Num)]),
    (
        Bool,
        "Number.isSafeInteger",
        "Number.isSafeInteger($0)",
        &[Of(Num)],
    ),
    (Bool, "Number.isNaN", "Number.isNaN($0)", &[Of(Any)]),
    (Bool, "Object.is", "Object.is($0, $1)", &[Of(Num), Of(Num)]),
    (Bool, "startsWith", "$0.startsWith($1)", &[Of(Str), Of(Str)]),
    (Bool, "endsWith", "$0.endsWith($1)", &[Of(Str), Of(Str)]),
    (
        Bool,
        "array-includes",
        "$0.includes($1)",
        &[Of(Arr), Of(Num)],
    ),
    (
        Bool,
        "every",
        "$0.every(function (x) { return x == x; })",
        &[Of(Arr)],
    ),
    (
        Bool,
        "some",
        "$0.some(function (x) { return typeof x === \"string\"; })",
        &[Of(Arr)],
    ),
    (Bool, "regexp-test", "/^[a-z]+$/i.test($0)", &[Of(Str)]),
    (
        Bool,
        "hasOwnProperty",
        "$0.hasOwnProperty(\"k1\")",
        &[Of(KObj)],
    ),
    (Str, "padEnd", "$0.padEnd(4, \"-\")", &[Of(Str)]),
    (
        Str,
        "replace-regexp",
        "$0.replace(/(\\w)(\\w)/g, \"[$&]\")",
        &[Of(Str)],
    ),
    (
        Str,
        "replace-function",
        "$0.replace(/./g, function (c) { return c.charCodeAt(0) % 10; })",
        &[Of(Str)],
    ),
    (
        Str,
        "normalize",
        "$0.normalize(\"NFD\").length.toString()",
        &[Of(Str)],
    ),
    (
        Str,
        "String.fromCharCode",
        "String.fromCharCode(65 + Math.abs($0 | 0) % 26)",
        &[Of(Num)],
    ),
    (Str, "toString36", "Math.trunc($0).toString(36)", &[Of(Num)]),
    (Str, "toFixed-big", "$0.toFixed(5)", &[Of(Num)]),
    (Str, "toPrecision-1", "$0.toPrecision(1)", &[Of(Num)]),
    (Str, "toExponential-none", "$0.toExponential()", &[Of(Num)]),
    (Str, "JSON.stringify-any", "JSON.stringify($0)", &[Of(Any)]),
    (
        Str,
        "JSON.stringify-indent",
        "JSON.stringify($0, null, 2)",
        &[Of(KObj)],
    ),
    (
        Str,
        "encodeURIComponent",
        "encodeURIComponent($0)",
        &[Of(Str)],
    ),
    (Str, "encodeURI", "encodeURI($0)", &[Of(Str)]),
    (Str, "escape", "escape($0)", &[Of(Str)]),
    (Str, "array-join", "$0.join(\"|\")", &[Of(Arr)]),
    (Str, "array-toString", "String($0)", &[Of(Arr)]),
    (Str, "sort-join", "$0.slice().sort().join()", &[Of(Arr)]),
    (
        Str,
        "match-global",
        "String($0.match(/[a-z0-9]/g))",
        &[Of(Str)],
    ),
    (Str, "exec", "String(/(a)(b)?/.exec($0))", &[Of(Str)]),
    (
        Str,
        "split-regexp",
        "$0.split(/[,\\s]/).join(\"+\")",
        &[Of(Str)],
    ),
    (
        Str,
        "Object.entries",
        "String(Object.entries($0))",
        &[Of(KObj)],
    ),
    (Str, "typeof", "typeof $0", &[Of(Any)]),
    (
        Str,
        "Date.toISOString",
        "new Date(Date.UTC(2001, 0, $0)).toISOString()",
        &[Of(Num)],
    ),
    (Str, "Date.toUTCString", "new Date(0).toUTCString()", &[]),
    (
        Arr,
        "map",
        "$0.map(function (x) { return x * 2; })",
        &[Of(Arr)],
    ),
    (
        Arr,
        "filter",
        "$0.filter(function (x, i) { return i % 2 === 0; })",
        &[Of(Arr)],
    ),
    (
        Arr,
        "sort-numbers",
        "$0.slice().sort(function (a, b) { return a - b; })",
        &[Of(Arr)],
    ),
    (Arr, "reverse", "$0.slice().reverse()", &[Of(Arr)]),
    (Arr, "concat", "$0.concat($1)", &[Of(Arr), Of(Arr)]),
    (Arr, "Array.of", "Array.of($0, $1)", &[Of(Num), Of(Str)]),
    (Arr, "Array.from-string", "Array.from($0)", &[Of(Str)]),
    (
        Arr,
        "Array-length",
        "Array.from({length: Math.abs($0 | 0) % 4})",
        &[Of(Num)],
    ),
    (Arr, "fill", "new Array(3).fill($0)", &[Of(Num)]),
    (
        Arr,
        "splice",
        "(function (a) { a.splice(1, 1); return a; })($0.slice())",
        &[Of(Arr)],
    ),
    (Arr, "Object.keys", "Object.keys($0)", &[Of(KObj)]),
    (Arr, "Object.values", "Object.values($0)", &[Of(KObj)]),
    (Arr, "split-limit", "$0.split(\"\", 2)", &[Of(Str)]),
    (
        KObj,
        "JSON.parse",
        "JSON.parse(JSON.stringify($0))",
        &[Of(KObj)],
    ),
    (
        KObj,
        "Object.assign",
        "Object.assign({k9: 1}, $0)",
        &[Of(KObj)],
    ),
    (Any, "JSON.parse-text", "JSON.parse($0)", &[Of(Str)]),
];

/// A root with properties bound to built-in calls, functions of them and
/// steps that call those.
pub fn program(rng: &mut Rng) -> Program {
    let mut g = Gen::new(rng);
    g.extra = OPS;
    let mut root = Obj::new("root".into(), ObjType::Item);
    g.ids.push("root".into());
    if g.rng.chance(50) {
        g.funcs(&mut root);
    }
    for _ in 0..2 + g.rng.below(4) {
        let ty = g
            .rng
            .pick(&[
                PType::Real,
                PType::Int,
                PType::String,
                PType::Bool,
                PType::Var,
            ])
            .clone();
        let kind = if ty == PType::Var {
            *g.rng.pick(&[Arr, KObj, Any, Str])
        } else {
            ty.kind()
        };
        let e = g.expr(kind, 3, &[]);
        let e = g.fit(&ty, e);
        let name = g.name("p");
        g.add(
            &mut root,
            name,
            Decl::New {
                ty: ty.clone(),
                readonly: false,
            },
            Some(e),
            ty,
            true,
        );
    }
    if g.rng.chance(20) {
        g.after(&mut root);
    }
    let n = g.rng.below(5);
    let steps = (0..n).filter_map(|_| g.step(&root)).collect();
    Program {
        root,
        enums: Vec::new(),
        steps,
    }
}
