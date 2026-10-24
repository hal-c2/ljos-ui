//! Making random programs that Qt accepts.
//!
//! Every binding refers only to properties made before it, so the
//! dependency graph has no cycle unless one is planted on purpose (and then
//! its values are not compared). Handlers of `on<Prop>Changed` write only
//! properties made after the one they watch, for the same reason. A
//! literal is always of its property's type, because Qt rejects a file
//! whose literal is not; a value of another type comes only from an
//! expression, which Qt converts or warns about at run time.

use std::sync::LazyLock;

use super::ast::{
    Changed, Decl, Expr, Func, Kind, Obj, ObjType, PType, Program, Prop, Signal, Stmt,
};
use crate::rng::Rng;

/// What an argument of an operation is.
#[derive(Clone, Copy)]
enum Arg {
    Of(Kind),
    /// A literal between 0 and 1, as a colour channel is.
    Unit,
    /// A literal pixel size.
    Pixels,
}

use Arg::{Of, Pixels, Unit};
use Kind::{Any, Arr, Bool, Color, Font, Num, Obj as KObj, Point, Rect, Size, Str, Url};

/// The operations that make a value of each kind.
const OPS: &[(Kind, &str, &str, &[Arg])] = &[
    (Num, "add", "($0 + $1)", &[Of(Num), Of(Num)]),
    (Num, "sub", "($0 - $1)", &[Of(Num), Of(Num)]),
    (Num, "mul", "($0 * $1)", &[Of(Num), Of(Num)]),
    (Num, "div", "($0 / $1)", &[Of(Num), Of(Num)]),
    (Num, "mod", "($0 % $1)", &[Of(Num), Of(Num)]),
    (Num, "neg", "(-$0)", &[Of(Num)]),
    (Num, "Math.abs", "Math.abs($0)", &[Of(Num)]),
    (Num, "Math.floor", "Math.floor($0)", &[Of(Num)]),
    (Num, "Math.ceil", "Math.ceil($0)", &[Of(Num)]),
    (Num, "Math.round", "Math.round($0)", &[Of(Num)]),
    (Num, "Math.trunc", "Math.trunc($0)", &[Of(Num)]),
    (Num, "Math.sign", "Math.sign($0)", &[Of(Num)]),
    (Num, "Math.sqrt", "Math.sqrt($0)", &[Of(Num)]),
    (Num, "Math.pow", "Math.pow($0, $1)", &[Of(Num), Of(Num)]),
    (Num, "Math.max", "Math.max($0, $1)", &[Of(Num), Of(Num)]),
    (Num, "Math.min", "Math.min($0, $1)", &[Of(Num), Of(Num)]),
    (Num, "Math.hypot", "Math.hypot($0, $1)", &[Of(Num), Of(Num)]),
    (Num, "Math.sin", "Math.sin($0)", &[Of(Num)]),
    (Num, "Math.log", "Math.log($0)", &[Of(Num)]),
    (Num, "Math.atan2", "Math.atan2($0, $1)", &[Of(Num), Of(Num)]),
    (Num, "Number", "Number($0)", &[Of(Str)]),
    (Num, "parseInt", "parseInt($0)", &[Of(Str)]),
    (Num, "parseInt16", "parseInt($0, 16)", &[Of(Str)]),
    (Num, "parseFloat", "parseFloat($0)", &[Of(Str)]),
    (Num, "string-length", "$0.length", &[Of(Str)]),
    (Num, "array-length", "$0.length", &[Of(Arr)]),
    (Num, "string-indexOf", "$0.indexOf($1)", &[Of(Str), Of(Str)]),
    (Num, "array-indexOf", "$0.indexOf($1)", &[Of(Arr), Of(Num)]),
    (Num, "charCodeAt", "$0.charCodeAt(0)", &[Of(Str)]),
    (
        Num,
        "ternary",
        "($0 ? $1 : $2)",
        &[Of(Bool), Of(Num), Of(Num)],
    ),
    (Num, "bool-to-number", "(+$0)", &[Of(Bool)]),
    (Num, "string-times-one", "($0 * 1)", &[Of(Str)]),
    (Num, "bit-or", "($0 | 0)", &[Of(Num)]),
    (Num, "shift-left", "($0 << 2)", &[Of(Num)]),
    (Num, "unsigned-shift", "($0 >>> 1)", &[Of(Num)]),
    (Num, "plus-null", "($0 + null)", &[Of(Num)]),
    (Num, "toFixed-number", "Number($0.toFixed(2))", &[Of(Num)]),
    (
        Num,
        "reduce",
        "$0.reduce(function (a, b) { return a + b; }, 0)",
        &[Of(Arr)],
    ),
    (
        Num,
        "object-keys-length",
        "Object.keys($0).length",
        &[Of(KObj)],
    ),
    (Num, "color.r", "$0.r", &[Of(Color)]),
    (Num, "color.a", "$0.a", &[Of(Color)]),
    (Num, "color.hslLightness", "$0.hslLightness", &[Of(Color)]),
    (Num, "point.x", "$0.x", &[Of(Point)]),
    (Num, "size.width", "$0.width", &[Of(Size)]),
    (Num, "rect.height", "$0.height", &[Of(Rect)]),
    (Num, "font.pixelSize", "$0.pixelSize", &[Of(Font)]),
    (Str, "concat", "($0 + $1)", &[Of(Str), Of(Str)]),
    (Str, "string-plus-number", "($0 + $1)", &[Of(Str), Of(Num)]),
    (Str, "number-plus-string", "($0 + $1)", &[Of(Num), Of(Str)]),
    (Str, "template", "`${$0}:${$1}`", &[Of(Any), Of(Num)]),
    (Str, "toUpperCase", "$0.toUpperCase()", &[Of(Str)]),
    (Str, "toLowerCase", "$0.toLowerCase()", &[Of(Str)]),
    (Str, "slice", "$0.slice(1, 3)", &[Of(Str)]),
    (Str, "slice-negative", "$0.slice(-2)", &[Of(Str)]),
    (Str, "substring", "$0.substring(1)", &[Of(Str)]),
    (Str, "trim", "$0.trim()", &[Of(Str)]),
    (Str, "repeat", "$0.repeat(2)", &[Of(Str)]),
    (Str, "padStart", "$0.padStart(5, \"*\")", &[Of(Str)]),
    (Str, "charAt", "$0.charAt(1)", &[Of(Str)]),
    (Str, "replace", "$0.replace(\"a\", \"b\")", &[Of(Str)]),
    (Str, "split-join", "$0.split(\",\").join(\"-\")", &[Of(Str)]),
    (Str, "toFixed", "$0.toFixed(2)", &[Of(Num)]),
    (Str, "toFixed0", "$0.toFixed(0)", &[Of(Num)]),
    (Str, "toPrecision", "$0.toPrecision(3)", &[Of(Num)]),
    (Str, "toExponential", "$0.toExponential(2)", &[Of(Num)]),
    (Str, "number-toString", "$0.toString()", &[Of(Num)]),
    (
        Str,
        "toString-radix",
        "Math.floor($0).toString(2)",
        &[Of(Num)],
    ),
    (Str, "String", "String($0)", &[Of(Any)]),
    (Str, "stringify-array", "JSON.stringify($0)", &[Of(Arr)]),
    (Str, "stringify-object", "JSON.stringify($0)", &[Of(KObj)]),
    (Str, "join", "$0.join(\",\")", &[Of(Arr)]),
    (Str, "typeof", "typeof $0", &[Of(Any)]),
    (
        Str,
        "ternary",
        "($0 ? $1 : $2)",
        &[Of(Bool), Of(Str), Of(Str)],
    ),
    (Str, "color-string", "String($0)", &[Of(Color)]),
    (Str, "url-string", "String($0)", &[Of(Url)]),
    (Str, "arg", "\"%1-%2\".arg($0).arg($1)", &[Of(Str), Of(Num)]),
    (Bool, "lt", "($0 < $1)", &[Of(Num), Of(Num)]),
    (Bool, "le", "($0 <= $1)", &[Of(Num), Of(Num)]),
    (Bool, "loose-eq", "($0 == $1)", &[Of(Num), Of(Num)]),
    (Bool, "strict-eq", "($0 === $1)", &[Of(Num), Of(Num)]),
    (Bool, "strict-ne", "($0 !== $1)", &[Of(Num), Of(Num)]),
    (Bool, "string-eq", "($0 === $1)", &[Of(Str), Of(Str)]),
    (Bool, "string-lt", "($0 < $1)", &[Of(Str), Of(Str)]),
    (
        Bool,
        "string-loose-eq-number",
        "($0 == $1)",
        &[Of(Str), Of(Num)],
    ),
    (Bool, "not", "(!$0)", &[Of(Bool)]),
    (Bool, "and", "($0 && $1)", &[Of(Bool), Of(Bool)]),
    (Bool, "or", "($0 || $1)", &[Of(Bool), Of(Bool)]),
    (Bool, "Array.isArray", "Array.isArray($0)", &[Of(Arr)]),
    (
        Bool,
        "array-includes",
        "$0.includes($1)",
        &[Of(Arr), Of(Num)],
    ),
    (Bool, "startsWith", "$0.startsWith($1)", &[Of(Str), Of(Str)]),
    (
        Bool,
        "string-includes",
        "$0.includes($1)",
        &[Of(Str), Of(Str)],
    ),
    (Bool, "isNaN", "isNaN($0)", &[Of(Num)]),
    (Bool, "Number.isInteger", "Number.isInteger($0)", &[Of(Num)]),
    (Bool, "Boolean", "Boolean($0)", &[Of(Any)]),
    (Bool, "is-null", "($0 === null)", &[Of(Any)]),
    (Bool, "is-undefined", "($0 === undefined)", &[Of(Any)]),
    (Bool, "not-not", "(!!$0)", &[Of(Str)]),
    (
        Bool,
        "ternary",
        "($0 ? $1 : $2)",
        &[Of(Bool), Of(Bool), Of(Bool)],
    ),
    (
        Bool,
        "Qt.colorEqual",
        "Qt.colorEqual($0, $1)",
        &[Of(Color), Of(Color)],
    ),
    (
        Arr,
        "map",
        "$0.map(function (v) { return v * 2; })",
        &[Of(Arr)],
    ),
    (
        Arr,
        "filter",
        "$0.filter(function (v) { return v > 1; })",
        &[Of(Arr)],
    ),
    (Arr, "array-slice", "$0.slice(1)", &[Of(Arr)]),
    (Arr, "array-concat", "$0.concat($1)", &[Of(Arr), Of(Arr)]),
    (Arr, "reverse", "$0.slice().reverse()", &[Of(Arr)]),
    (Arr, "sort", "$0.slice().sort()", &[Of(Arr)]),
    (
        Arr,
        "sort-numbers",
        "$0.slice().sort(function (a, b) { return a - b; })",
        &[Of(Arr)],
    ),
    (Arr, "split", "$0.split(\",\")", &[Of(Str)]),
    (Arr, "Object.keys", "Object.keys($0)", &[Of(KObj)]),
    (Arr, "pair", "[$0, $1]", &[Of(Num), Of(Num)]),
    (Arr, "string-pair", "[$0, $1]", &[Of(Str), Of(Str)]),
    (Arr, "fill", "new Array(3).fill($0)", &[Of(Num)]),
    (Arr, "Array.from", "Array.from($0)", &[Of(Str)]),
    (
        Arr,
        "json-round-trip",
        "JSON.parse(JSON.stringify($0))",
        &[Of(Arr)],
    ),
    (
        Arr,
        "ternary",
        "($0 ? $1 : $2)",
        &[Of(Bool), Of(Arr), Of(Arr)],
    ),
    (KObj, "object-1", "({k0: $0})", &[Of(Any)]),
    (KObj, "object-2", "({k1: $0, k0: $1})", &[Of(Num), Of(Str)]),
    (
        KObj,
        "Object.assign",
        "Object.assign({}, $0, {k2: $1})",
        &[Of(KObj), Of(Num)],
    ),
    (
        KObj,
        "json-round-trip",
        "JSON.parse(JSON.stringify($0))",
        &[Of(KObj)],
    ),
    (
        Color,
        "Qt.rgba",
        "Qt.rgba($0, $1, $2, $3)",
        &[Unit, Unit, Unit, Unit],
    ),
    (
        Color,
        "Qt.rgba-computed",
        "Qt.rgba($0, 0.5, 0.25, 1)",
        &[Of(Num)],
    ),
    (
        Color,
        "Qt.hsla",
        "Qt.hsla($0, $1, $2, $3)",
        &[Unit, Unit, Unit, Unit],
    ),
    (Color, "Qt.darker", "Qt.darker($0, 1.5)", &[Of(Color)]),
    (Color, "Qt.lighter", "Qt.lighter($0)", &[Of(Color)]),
    (Color, "Qt.tint", "Qt.tint($0, $1)", &[Of(Color), Of(Color)]),
    (
        Color,
        "ternary",
        "($0 ? $1 : $2)",
        &[Of(Bool), Of(Color), Of(Color)],
    ),
    (Point, "Qt.point", "Qt.point($0, $1)", &[Of(Num), Of(Num)]),
    (Size, "Qt.size", "Qt.size($0, $1)", &[Of(Num), Of(Num)]),
    (
        Rect,
        "Qt.rect",
        "Qt.rect($0, $1, $2, $3)",
        &[Of(Num), Of(Num), Of(Num), Of(Num)],
    ),
    (
        Font,
        "Qt.font",
        "Qt.font({pixelSize: $0, bold: $1})",
        &[Pixels, Of(Bool)],
    ),
    (
        Font,
        "Qt.font-italic",
        "Qt.font({pixelSize: $0, italic: $1, underline: $2})",
        &[Pixels, Of(Bool), Of(Bool)],
    ),
    (Url, "string-as-url", "$0", &[Of(Str)]),
    (Url, "Qt.resolvedUrl", "Qt.resolvedUrl(\"b.png\")", &[]),
    (Url, "url-plus-string", "($0 + \".png\")", &[Of(Str)]),
];

const NUMS: &[&str] = &[
    "0",
    "1",
    "2",
    "3",
    "-1",
    "7",
    "10",
    "0.5",
    "1.25",
    "-2.5",
    "100",
    "0.1",
    "3.14159",
    "1e21",
    "255",
    "42",
    "-0.75",
    "2147483648",
];
const STRS: &[&str] = &[
    "\"\"",
    "\"a\"",
    "\"Hello\"",
    "\"10\"",
    "\" x \"",
    "\"3.5\"",
    "\"a,b,c\"",
    "\"Ljós\"",
    "\"0x1F\"",
    "\"-0\"",
    "\"true\"",
    "\"ab\"",
];
const ARRS: &[&str] = &[
    "[1, 2, 3]",
    "[]",
    "[\"a\", \"b\"]",
    "[3, 1, 2]",
    "[0.5, -1]",
    "[10, 9, 1]",
];
const OBJS: &[&str] = &[
    "({k0: 1, k1: \"a\"})",
    "({k0: [1, 2]})",
    "({})",
    "({k1: true, k0: null})",
];
const COLORS: &[&str] = &[
    "\"#ff0000\"",
    "\"red\"",
    "\"steelblue\"",
    "\"#80ff0000\"",
    "\"#123\"",
    "\"transparent\"",
    "\"#00ff00\"",
    "\"#336699\"",
];
// Relative URLs only go down: `Qt.resolvedUrl` of one going up would name
// the directory a case happens to run in, and the case file's own directory
// is the only one the observations hide. No absolute `file:` URL either, for
// the same reason: Ljós's case file is at the root.
const URLS: &[&str] = &[
    "\"a.png\"",
    "\"http://example.com/x.png\"",
    "\"sub/b.png\"",
    "\"\"",
    "\"https://example.org/c.png?q=1#f\"",
];
const UNITS: &[&str] = &["0", "1", "0.5", "0.25", "0.75", "0.1"];

/// The properties of an `Item` a program may bind, from the table of types:
/// those of a number or a truth value that can be written and that hold
/// their value whatever else the item does.
static ITEM_BUILTINS: LazyLock<Vec<(&'static str, PType)>> = LazyLock::new(|| {
    // These depend on the window, focus or the scene and not only on the
    // program, or are not a value at all.
    const LEAVE_OUT: &[&str] = &[
        "objectName",
        "state",
        "focus",
        "activeFocusOnTab",
        "antialiasing",
        "smooth",
        "baselineOffset",
        "transformOrigin",
        "focusPolicy",
    ];
    let mut out = Vec::new();
    let mut ty = qt_types::module("QtQuick").and_then(|m| m.type_named("Item"));
    while let Some(t) = ty {
        for p in &t.properties {
            let pt = match p.type_name {
                "double" | "qreal" | "QtQml.real" | "QtQml.double" => PType::Real,
                "int" | "QtQml.int" => PType::Int,
                "bool" | "QtQml.bool" => PType::Bool,
                _ => continue,
            };
            if p.is_readonly || p.is_list || p.is_pointer || LEAVE_OUT.contains(&p.name) {
                continue;
            }
            if !out.iter().any(|(n, _)| *n == p.name) {
                out.push((p.name, pt));
            }
        }
        ty = t.prototype();
    }
    out.sort_unstable();
    out
});

/// The keys of an enum of a type in the table, written as QML writes them.
fn enum_keys(module: &str, ty: &str, name: &str) -> Vec<String> {
    qt_types::module(module)
        .and_then(|m| m.type_named(ty))
        .and_then(|t| t.enums.iter().find(|e| e.name == name))
        .map(|e| e.keys.iter().map(|(k, _)| format!("{ty}.{k}")).collect())
        .unwrap_or_default()
}

static ALIGNMENTS: LazyLock<Vec<String>> =
    LazyLock::new(|| enum_keys("QtQuick", "Text", "HAlignment"));
static WRAP_MODES: LazyLock<Vec<String>> =
    LazyLock::new(|| enum_keys("QtQuick", "Text", "WrapMode"));

/// The Item properties the generator binds, for the tests.
#[must_use]
pub fn item_builtins() -> &'static [(&'static str, PType)] {
    &ITEM_BUILTINS
}

/// A property that bindings made later may read.
#[derive(Clone, Debug)]
struct Avail {
    obj: String,
    prop: String,
    ty: PType,
    /// Its place in the order things were made.
    order: usize,
    /// An alias, a member of a group or a readonly property is never the
    /// target of a write made to keep the graph acyclic.
    writable: bool,
}

/// A function a binding may call: it reads only its parameters.
#[derive(Clone, Debug)]
struct Callable {
    obj: String,
    name: String,
    params: Vec<Kind>,
    ret: Kind,
}

struct Gen<'r> {
    rng: &'r mut Rng,
    scope: Vec<Avail>,
    callables: Vec<Callable>,
    names: usize,
    enums: Vec<String>,
    /// Ids of the objects made so far, which a `var` may hold.
    ids: Vec<String>,
}

/// A random program.
pub fn program(rng: &mut Rng) -> Program {
    let mut g = Gen {
        rng,
        scope: Vec::new(),
        callables: Vec::new(),
        names: 0,
        enums: Vec::new(),
        ids: Vec::new(),
    };
    let mut enums = Vec::new();
    if g.rng.chance(15) {
        let keys = vec![
            ("One".to_owned(), None),
            ("Two".to_owned(), g.rng.chance(50).then_some(5)),
            ("Three".to_owned(), None),
        ];
        g.enums = keys.iter().map(|(k, _)| format!("Case.{k}")).collect();
        enums.push(("Shade".to_owned(), keys));
    }
    let mut root = Obj::new("root".into(), ObjType::Item);
    g.ids.push("root".into());
    let mut count = 1;
    let target = 1 + g.rng.below(4);
    g.fill(&mut root, 0, &mut count, target);
    g.after(&mut root);
    let n = g.rng.below(6);
    let steps = (0..n).filter_map(|_| g.step(&root)).collect();
    Program { root, enums, steps }
}

impl Gen<'_> {
    fn name(&mut self, prefix: &str) -> String {
        self.names += 1;
        format!("{prefix}{}", self.names - 1)
    }

    /// Gives `o` its properties and children, depth first.
    fn fill(&mut self, o: &mut Obj, depth: usize, count: &mut usize, target: usize) {
        if self.rng.chance(40) {
            self.funcs(o);
        }
        let n = 1 + self.rng.below(4);
        for _ in 0..n {
            self.prop(o);
        }
        if self.rng.chance(30) {
            self.alias(o);
        }
        if o.ty != ObjType::QtObject {
            for _ in 0..self.rng.below(3) {
                self.builtin(o);
            }
        }
        if depth == 0 && self.rng.chance(4) {
            self.binding_loop(o);
        }
        // A QtObject has no default property to hold children.
        if o.ty == ObjType::QtObject {
            return;
        }
        while *count < target && depth < 2 && self.rng.chance(70) {
            *count += 1;
            let ty = *self
                .rng
                .pick(&[ObjType::QtObject, ObjType::Item, ObjType::Text]);
            let id = format!("o{}", *count - 1);
            let mut child = Obj::new(id.clone(), ty);
            self.ids.push(id);
            self.fill(&mut child, depth + 1, count, target);
            o.children.push(child);
        }
    }

    fn prop(&mut self, o: &mut Obj) {
        let ty = self.rng.pick(&PType::ALL).clone();
        let readonly = self.rng.chance(10);
        let init = if readonly || self.rng.chance(90) {
            Some(self.init(&ty))
        } else {
            None
        };
        let name = self.name("p");
        self.add(
            o,
            name,
            Decl::New {
                ty: ty.clone(),
                readonly,
            },
            init,
            ty,
            !readonly,
        );
    }

    fn add(
        &mut self,
        o: &mut Obj,
        name: String,
        decl: Decl,
        init: Option<Expr>,
        ty: PType,
        writable: bool,
    ) {
        let order = self.scope.len();
        self.scope.push(Avail {
            obj: o.id.clone(),
            prop: name.clone(),
            ty,
            order,
            writable,
        });
        o.props.push(Prop {
            name,
            decl,
            init,
            in_loop: false,
        });
    }

    /// What a property of type `ty` is first bound to.
    fn init(&mut self, ty: &PType) -> Expr {
        // The string forms of value types Qt reads when it loads the file.
        let literal = match ty {
            PType::Point => Some("\"1,2\""),
            PType::Size => Some("\"3x4\""),
            PType::Rect => Some("\"1,2,3x4\""),
            _ => None,
        };
        if let Some(l) = literal
            && self.rng.chance(15)
        {
            return Expr::Lit(l.into(), ty.kind());
        }
        // Now and then a value of another kind, which Qt converts or warns
        // about: never as a literal, which Qt would reject.
        if self.rng.chance(6) {
            let other = *self.rng.pick(&[Num, Str, Bool, Arr, Any]);
            if other != ty.kind() {
                let e = self.expr(other, 2, &[]);
                if !matches!(e, Expr::Lit(..)) {
                    return e;
                }
            }
        }
        let e = self.expr(ty.kind(), 3, &[]);
        match ty {
            // A list's literal must be a list of its own element type.
            PType::ListInt | PType::ListReal | PType::ListString if matches!(e, Expr::Lit(..)) => {
                let text = match ty {
                    PType::ListString => "[\"a\", \"b\"]",
                    PType::ListReal => "[0.5, 2]",
                    _ => "[1, 2, 3]",
                };
                Expr::Lit(text.into(), Arr)
            }
            _ => self.fit(ty, e),
        }
    }

    /// Qt rejects a literal for an `int` that is not a 32-bit whole number.
    fn fit(&mut self, ty: &PType, e: Expr) -> Expr {
        let whole = |t: &str| {
            t.parse::<f64>()
                .is_ok_and(|n| n.fract() == 0.0 && n.abs() < 2_147_483_648.0)
        };
        match &e {
            Expr::Lit(t, Num) if *ty == PType::Int && t.parse::<f64>().is_ok() && !whole(t) => {
                Expr::Lit(self.rng.range(-3, 40).to_string(), Num)
            }
            _ => e,
        }
    }

    fn alias(&mut self, o: &mut Obj) {
        let Some(t) = self.pick_scope(|_| true) else {
            return;
        };
        let members = t.ty.members();
        let (target, ty) = if !members.is_empty() && self.rng.chance(30) {
            let (m, k) = *self.rng.pick(members);
            let ty = if k == Bool { PType::Bool } else { PType::Real };
            (format!("{}.{}.{m}", t.obj, t.prop), ty)
        } else {
            (format!("{}.{}", t.obj, t.prop), t.ty.clone())
        };
        let name = self.name("a");
        self.add(
            o,
            name,
            Decl::Alias {
                target,
                ty: ty.clone(),
            },
            None,
            ty,
            false,
        );
    }

    fn builtin(&mut self, o: &mut Obj) {
        let text = o.ty == ObjType::Text;
        let choice = if text { self.rng.below(5) } else { 0 };
        let (name, ty, init, writable) = match choice {
            1 => (
                "text".to_owned(),
                PType::String,
                self.expr(Str, 3, &[]),
                true,
            ),
            2 => (
                "color".to_owned(),
                PType::Color,
                self.expr(Color, 2, &[]),
                true,
            ),
            3 if !ALIGNMENTS.is_empty() => {
                let which = if self.rng.chance(50) {
                    &*ALIGNMENTS
                } else {
                    &*WRAP_MODES
                };
                let key = self.rng.pick(which).clone();
                let prop = if key.contains("Wrap") {
                    "wrapMode"
                } else {
                    "horizontalAlignment"
                };
                (prop.to_owned(), PType::Int, Expr::Lit(key, Num), true)
            }
            4 => {
                let (m, ty, k) = self
                    .rng
                    .pick(&[
                        ("pixelSize", PType::Int, Num),
                        ("bold", PType::Bool, Bool),
                        ("italic", PType::Bool, Bool),
                    ])
                    .clone();
                let e = if k == Num {
                    Expr::Lit(self.rng.range(8, 30).to_string(), Num)
                } else {
                    self.expr(k, 2, &[])
                };
                (format!("font.{m}"), ty, e, false)
            }
            _ => {
                // A text's implicit size is its text's: Qt does not let it be set.
                let (n, ty) = loop {
                    let b = self.rng.pick(&ITEM_BUILTINS).clone();
                    if !(text && b.0.starts_with("implicit")) {
                        break b;
                    }
                };
                let e = self.expr(ty.kind(), 3, &[]);
                let e = self.fit(&ty, e);
                (n.to_owned(), ty, e, true)
            }
        };
        if o.props.iter().any(|p| p.name == name) {
            return;
        }
        let decl = if matches!(init, Expr::Lit(ref t, _) if t.contains('.') && t.starts_with("Text."))
        {
            Decl::Enum
        } else {
            Decl::Builtin { ty: ty.clone() }
        };
        self.add(o, name, decl, Some(init), ty, writable);
    }

    /// Two properties bound to each other: both sides must warn.
    fn binding_loop(&mut self, o: &mut Obj) {
        let (a, b) = (self.name("l"), self.name("l"));
        let r = |p: &str| Expr::Ref {
            obj: o.id.clone(),
            prop: p.to_owned(),
            kind: Num,
        };
        let one = Expr::Lit("1".into(), Num);
        for (name, other) in [(&a, &b), (&b, &a)] {
            o.props.push(Prop {
                name: name.clone(),
                decl: Decl::New {
                    ty: PType::Int,
                    readonly: false,
                },
                init: Some(Expr::op(
                    "add",
                    "($0 + $1)",
                    Num,
                    vec![r(other), one.clone()],
                )),
                in_loop: true,
            });
        }
    }

    /// Functions of the object; those that read only their parameters can be
    /// called from bindings made after them.
    fn funcs(&mut self, o: &mut Obj) {
        for _ in 0..1 + self.rng.below(2) {
            let n = self.rng.below(3);
            let params: Vec<(String, Kind, Option<&'static str>)> = (0..n)
                .map(|i| {
                    let k = *self.rng.pick(&[Num, Str, Bool]);
                    let ann = self.rng.chance(30).then(|| match k {
                        Num => *self.rng.pick(&["int", "real", "double"]),
                        Str => "string",
                        _ => "bool",
                    });
                    (["a", "b", "c"][i].to_owned(), k, ann)
                })
                .collect();
            let ps: Vec<(String, Kind)> = params.iter().map(|(n, k, _)| (n.clone(), *k)).collect();
            let pure = self.rng.chance(60);
            let ret_kind = *self.rng.pick(&[Num, Str, Bool, Arr, Any]);
            let saved = if pure {
                Some(std::mem::take(&mut self.scope))
            } else {
                None
            };
            let ret = self.expr(ret_kind, 3, &ps);
            let throws = self.rng.chance(25).then(|| {
                let guard = self.expr(Bool, 2, &ps);
                (guard, *self.rng.pick(&["Error", "RangeError", "TypeError"]))
            });
            if let Some(s) = saved {
                self.scope = s;
            }
            let name = self.name("f");
            if pure {
                self.callables.push(Callable {
                    obj: o.id.clone(),
                    name: name.clone(),
                    params: params.iter().map(|(_, k, _)| *k).collect(),
                    ret: ret_kind,
                });
            }
            o.funcs.push(Func {
                name,
                params,
                throws,
                ret,
            });
        }
    }

    /// Signals, handlers and `Component.onCompleted`, once every property
    /// exists.
    fn after(&mut self, root: &mut Obj) {
        let mut objs = Vec::new();
        root.each_mut(&mut |o| objs.push(o.id.clone()));
        for id in objs {
            let mut changed = Vec::new();
            let mut signals = Vec::new();
            let mut completed = None;
            if self.rng.chance(30) {
                let n = self.rng.below(3);
                let params: Vec<(String, PType)> = (0..n)
                    .map(|i| {
                        let t = self
                            .rng
                            .pick(&[
                                PType::Int,
                                PType::Real,
                                PType::String,
                                PType::Bool,
                                PType::Var,
                            ])
                            .clone();
                        (["x", "y"][i].to_owned(), t)
                    })
                    .collect();
                let ps: Vec<(String, Kind)> =
                    params.iter().map(|(n, t)| (n.clone(), t.kind())).collect();
                let name = self.name("s");
                let mut body = vec![Stmt::Log {
                    text: name.clone(),
                    value: ps.first().map(|(n, k)| Expr::Param(n.clone(), *k)),
                }];
                if let Some(t) = self.pick_scope(|a| a.writable) {
                    let value = self.expr(t.ty.kind(), 2, &ps);
                    body.push(assign(&t, value));
                }
                signals.push(Signal {
                    name,
                    params,
                    handler: Some(body),
                });
            }
            let own: Vec<Avail> = self
                .scope
                .iter()
                .filter(|a| a.obj == id && !a.prop.contains('.'))
                .cloned()
                .collect();
            if !own.is_empty() && self.rng.chance(30) {
                let watched = self.rng.pick(&own).clone();
                let mut body = vec![Stmt::Log {
                    text: watched.prop.clone(),
                    value: Some(Expr::Ref {
                        obj: watched.obj.clone(),
                        prop: watched.prop.clone(),
                        kind: watched.ty.kind(),
                    }),
                }];
                if let Some(t) = self.pick_scope(|a| a.writable && a.order > watched.order) {
                    let value = self.expr(t.ty.kind(), 2, &[]);
                    body.push(assign(&t, value));
                }
                changed.push(Changed {
                    prop: watched.prop,
                    body,
                });
            }
            if self.rng.chance(30) {
                let mut body = vec![Stmt::Log {
                    text: format!("c:{id}"),
                    value: None,
                }];
                if self.rng.chance(40)
                    && let Some(t) = self.pick_scope(|a| a.writable)
                {
                    let value = self.expr(t.ty.kind(), 2, &[]);
                    body.push(assign(&t, value));
                }
                completed = Some(body);
            }
            root.each_mut(&mut |o| {
                if o.id == id {
                    o.signals.append(&mut signals);
                    o.changed.append(&mut changed);
                    if completed.is_some() {
                        o.completed = completed.take();
                    }
                }
            });
        }
    }

    fn step(&mut self, root: &Obj) -> Option<Stmt> {
        let objs = root.all();
        match self.rng.below(100) {
            0..35 => {
                // Now and then a readonly property, which must throw.
                let t = if self.rng.chance(8) {
                    self.readonly(root)
                        .or_else(|| self.pick_scope(|a| !a.prop.contains('.')))
                } else {
                    self.pick_scope(|a| !a.prop.contains('.'))
                }?;
                let value = self.expr(t.ty.kind(), 2, &[]);
                Some(assign(&t, value))
            }
            35..50 => {
                let t = self.pick_scope(|a| a.writable && !a.prop.contains('.'))?;
                let order = t.order;
                let saved = self.scope.clone();
                self.scope.retain(|a| a.order < order);
                let value = self.expr(t.ty.kind(), 2, &[]);
                self.scope = saved;
                Some(Stmt::Bind {
                    obj: t.obj,
                    prop: t.prop,
                    value,
                })
            }
            50..65 => {
                let fs: Vec<(String, &Func)> = objs
                    .iter()
                    .flat_map(|o| o.funcs.iter().map(|f| (o.id.clone(), f)))
                    .collect();
                if fs.is_empty() {
                    return None;
                }
                let (obj, f) = &fs[self.rng.below(fs.len())];
                let args = f
                    .params
                    .iter()
                    .map(|(_, k, _)| self.expr(*k, 1, &[]))
                    .collect();
                Some(Stmt::Call {
                    obj: obj.clone(),
                    fun: f.name.clone(),
                    args,
                    ret: f.ret.kind(),
                })
            }
            65..77 => {
                let ss: Vec<(String, &Signal)> = objs
                    .iter()
                    .flat_map(|o| o.signals.iter().map(|s| (o.id.clone(), s)))
                    .collect();
                if ss.is_empty() {
                    return None;
                }
                let (obj, s) = &ss[self.rng.below(ss.len())];
                let args = s
                    .params
                    .iter()
                    .map(|(_, t)| self.expr(t.kind(), 1, &[]))
                    .collect();
                Some(Stmt::Emit {
                    obj: obj.clone(),
                    sig: s.name.clone(),
                    args,
                })
            }
            77..89 => {
                let t = self.pick_scope(|a| !a.ty.members().is_empty() && !a.prop.contains('.'))?;
                let (m, k) = *self.rng.pick(t.ty.members());
                let value = if k == Bool {
                    self.expr(Bool, 1, &[])
                } else {
                    let unit = matches!(t.ty, PType::Color);
                    if unit {
                        self.lit_from(UNITS, Num)
                    } else {
                        self.expr(Num, 1, &[])
                    }
                };
                Some(Stmt::Assign {
                    obj: t.obj,
                    prop: t.prop,
                    member: Some(m.to_owned()),
                    value,
                })
            }
            _ => {
                let t = self.pick_scope(|a| {
                    matches!(
                        a.ty,
                        PType::ListInt | PType::ListReal | PType::ListString | PType::Var
                    ) && !a.prop.contains('.')
                })?;
                let k = if t.ty == PType::ListString { Str } else { Num };
                let value = self.expr(k, 1, &[]);
                Some(Stmt::Push {
                    obj: t.obj,
                    prop: t.prop,
                    value,
                })
            }
        }
    }

    fn readonly(&mut self, root: &Obj) -> Option<Avail> {
        let ro: Vec<Avail> = self
            .scope
            .iter()
            .filter(|a| {
                root.all().iter().any(|o| {
                    o.id == a.obj && o.props.iter().any(|p| p.name == a.prop && p.readonly())
                })
            })
            .cloned()
            .collect();
        (!ro.is_empty()).then(|| self.rng.pick(&ro).clone())
    }

    fn pick_scope(&mut self, keep: impl Fn(&Avail) -> bool) -> Option<Avail> {
        let c: Vec<&Avail> = self.scope.iter().filter(|a| keep(a)).collect();
        (!c.is_empty()).then(|| c[self.rng.below(c.len())].clone())
    }

    fn lit_from(&mut self, from: &[&str], k: Kind) -> Expr {
        Expr::Lit((*self.rng.pick(from)).to_owned(), k)
    }

    fn literal(&mut self, k: Kind) -> Expr {
        match k {
            Num => {
                if !self.enums.is_empty() && self.rng.chance(10) {
                    let e = self.rng.pick(&self.enums).clone();
                    return Expr::Lit(e, Num);
                }
                if self.rng.chance(5) && !ALIGNMENTS.is_empty() {
                    let e = self.rng.pick(&ALIGNMENTS).clone();
                    return Expr::Lit(e, Num);
                }
                self.lit_from(NUMS, Num)
            }
            Str => self.lit_from(STRS, Str),
            Bool => self.lit_from(&["true", "false"], Bool),
            Arr => self.lit_from(ARRS, Arr),
            KObj => self.lit_from(OBJS, KObj),
            Color => self.lit_from(COLORS, Color),
            Url => self.lit_from(URLS, Url),
            Point => Expr::Lit(
                format!(
                    "Qt.point({}, {})",
                    self.rng.range(-5, 20),
                    self.rng.range(-5, 20)
                ),
                Point,
            ),
            Size => Expr::Lit(
                format!(
                    "Qt.size({}, {})",
                    self.rng.range(0, 20),
                    self.rng.range(0, 20)
                ),
                Size,
            ),
            Rect => Expr::Lit(
                format!(
                    "Qt.rect({}, {}, {}, {})",
                    self.rng.range(-5, 20),
                    self.rng.range(-5, 20),
                    self.rng.range(0, 20),
                    self.rng.range(0, 20)
                ),
                Rect,
            ),
            Font => Expr::Lit(
                format!("Qt.font({{pixelSize: {}}})", self.rng.range(8, 30)),
                Font,
            ),
            Any => match self.rng.below(10) {
                0 => Expr::Lit("null".into(), Any),
                1 => Expr::Lit("undefined".into(), Any),
                2 => {
                    let id = self.rng.pick(&self.ids).clone();
                    Expr::Ref {
                        obj: id,
                        prop: String::new(),
                        kind: Any,
                    }
                }
                _ => {
                    let k = *self.rng.pick(&[Num, Str, Bool, Arr, KObj]);
                    let mut e = self.literal(k);
                    if let Expr::Lit(_, kind) = &mut e {
                        *kind = Any;
                    }
                    e
                }
            },
        }
    }

    /// An expression of kind `k`, at most `depth` operations deep, that may
    /// read `params` and the properties in scope.
    fn expr(&mut self, k: Kind, depth: u32, params: &[(String, Kind)]) -> Expr {
        if depth == 0 || self.rng.chance(30) {
            return self.leaf(k, params);
        }
        if k == Any {
            let concrete = *self.rng.pick(&[Num, Str, Bool, Arr, KObj, Color, Point]);
            let mut e = self.expr(concrete, depth, params);
            // A `var` holds what it is given: the node keeps its own kind so
            // the shrinker replaces it with a literal of that kind.
            if let Expr::Lit(_, kind) = &mut e {
                *kind = Any;
            }
            return e;
        }
        if self.rng.chance(8) && !self.callables.is_empty() {
            let fitting: Vec<Callable> = self
                .callables
                .iter()
                .filter(|c| c.ret == k)
                .cloned()
                .collect();
            if !fitting.is_empty() {
                let c = &fitting[self.rng.below(fitting.len())];
                let args: Vec<Expr> = c
                    .params
                    .iter()
                    .map(|pk| self.expr(*pk, depth - 1, params))
                    .collect();
                let holes: Vec<String> = (0..args.len()).map(|i| format!("${i}")).collect();
                return Expr::op(
                    "function-call",
                    format!("{}.{}({})", c.obj, c.name, holes.join(", ")),
                    k,
                    args,
                );
            }
        }
        let ops: Vec<&(Kind, &str, &str, &[Arg])> =
            OPS.iter().filter(|(ok, ..)| *ok == k).collect();
        let (_, label, tpl, args) = *ops[self.rng.below(ops.len())];
        let mut args: Vec<Expr> = args
            .iter()
            .map(|a| match a {
                Of(ak) => self.expr(*ak, depth - 1, params),
                Unit => self.lit_from(UNITS, Num),
                Pixels => Expr::Lit(self.rng.range(8, 30).to_string(), Num),
            })
            .collect();
        // What an object or a resolved URL reads as text is its address or
        // the directory a case runs in, which differ on every run.
        for a in &mut args {
            let unstable = match a {
                Expr::Ref { prop, .. } => prop.is_empty() && matches!(label, "String" | "template"),
                Expr::Op { label: inner, .. } => {
                    *inner == "Qt.resolvedUrl" && label == "url-string"
                }
                _ => false,
            };
            if unstable {
                *a = a.kind().literal();
            }
        }
        Expr::op(label, tpl, k, args)
    }

    fn leaf(&mut self, k: Kind, params: &[(String, Kind)]) -> Expr {
        let ps: Vec<&(String, Kind)> = params
            .iter()
            .filter(|(_, pk)| *pk == k || k == Any)
            .collect();
        if !ps.is_empty() && self.rng.chance(50) {
            let (n, pk) = ps[self.rng.below(ps.len())];
            return Expr::Param(n.clone(), *pk);
        }
        if self.rng.chance(50)
            && let Some(a) = self.pick_scope(|a| a.ty.kind() == k || k == Any)
        {
            return Expr::Ref {
                obj: a.obj,
                prop: a.prop,
                kind: a.ty.kind(),
            };
        }
        self.literal(k)
    }
}

fn assign(t: &Avail, value: Expr) -> Stmt {
    Stmt::Assign {
        obj: t.obj.clone(),
        prop: t.prop.clone(),
        member: None,
        value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::print;

    #[test]
    fn the_item_properties_come_from_the_type_table() {
        let names: Vec<&str> = item_builtins().iter().map(|(n, _)| *n).collect();
        for wanted in ["x", "width", "opacity", "visible", "z"] {
            assert!(
                names.contains(&wanted),
                "{wanted} is missing from {names:?}"
            );
        }
        assert!(!names.contains(&"state"));
        assert!(!ALIGNMENTS.is_empty(), "Text's alignments are in the table");
    }

    #[test]
    fn a_seed_makes_the_same_program() {
        let a = program(&mut Rng::for_case(9, 4));
        let b = program(&mut Rng::for_case(9, 4));
        assert_eq!(a, b);
    }

    #[test]
    fn bindings_read_only_what_was_made_before_them() {
        for n in 0..300 {
            let p = program(&mut Rng::for_case(1, n));
            let mut order = Vec::new();
            for o in p.root.all() {
                for prop in &o.props {
                    order.push(format!("{}.{}", o.id, prop.name));
                }
            }
            for o in p.root.all() {
                for prop in o.props.iter().filter(|p| !p.in_loop) {
                    let me = order
                        .iter()
                        .position(|k| *k == format!("{}.{}", o.id, prop.name))
                        .unwrap();
                    if let Some(e) = &prop.init {
                        e.visit(&mut |e| {
                            if let Expr::Ref { obj, prop: r, .. } = e
                                && !r.is_empty()
                            {
                                let at = order.iter().position(|k| *k == format!("{obj}.{r}"));
                                assert!(
                                    at.is_some_and(|at| at < me),
                                    "case {n}: {}",
                                    print::clean(&p)
                                );
                            }
                        });
                    }
                }
            }
        }
    }
}
