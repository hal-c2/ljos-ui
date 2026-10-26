//! Tier 1 against tier 2 (ARCHITECTURE.md §5.4): whichever tier runs an
//! expression, it gives the same value, depends on the same properties and
//! logs the same warnings.
//!
//! Each expression is the binding of a property of one object. It is run
//! with tier 1 on, with tier 1 off, and with the cross-check on, in the
//! same engine, so the nodes it read can be compared as they are.
//!
//! The timings are ignored by default:
//!
//! `cargo test --release -p ljos_engine --test tier1 -- --ignored --nocapture`

mod support;

use std::cell::Cell;
use std::time::Instant;

use ljos_engine::{NodeId, ObjectId, Value};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError, TestRunner};
use support::{Harness, engine};

/// What the expressions are written against: a property of every kind the
/// subset reads.
const MEMBERS: &str = r##"
    objectName: "root"
    width: 200
    height: 100
    radius: 2.5
    color: "#336699"
    border.width: 3
    border.color: "red"
    property int i: 7
    property int j: -3
    property int zero: 0
    property real r: 2.5
    property real big: 1e21
    property real tiny: 1e-7
    property bool yes: true
    property bool no: false
    property string s: "hello"
    property string empty: ""
    property string digits: " 12 "
    property var undef: undefined
    property var nothing: null
    property var list: [1, "two", 3.5, null]
    property point pt: Qt.point(3, 4)
    property size sz: Qt.size(10, 20)
    property rect rc: Qt.rect(1, 2, 30, 40)
    property url link: "a/b.png"
    property Item other: child
    property Item none: null
    function twice(v) { return v * 2 }
    function pick(c, a, b) { if (c) return a; return b }
    function area() { return width * height }
    function grade(v) {
        var limit = i * 2
        if (v > limit) return "high"
        else if (v < 0) return "low"
        return "mid"
    }
"##;

/// The properties a dependency may be named by in a failure.
const NAMED: &[&str] = &[
    "width", "height", "radius", "color", "i", "j", "zero", "r", "big", "tiny", "yes", "no", "s",
    "empty", "digits", "undef", "nothing", "list", "pt", "sz", "rc", "link", "other", "none",
];

/// The child's properties a dependency may be named by.
const CHILD: &[&str] = &["x", "y", "width", "height", "depth"];

/// One object with `MEMBERS`, a child, and the properties under test.
struct World {
    h: Harness,
    root: ObjectId,
    child: ObjectId,
}

/// What one evaluation showed.
#[derive(Debug, Clone, PartialEq)]
struct Seen {
    /// `Debug` of the value: it tells `-0` from `0` and a number from the
    /// string that prints the same.
    value: String,
    deps: Vec<NodeId>,
    warnings: Vec<String>,
}

impl World {
    /// `root`: more members of the root. `child`: members of its child.
    fn new(root: &str, child: &str) -> World {
        let source = format!(
            "import QtQuick\nRectangle {{\n    id: root\n{MEMBERS}\n{root}\n    Item {{\n        id: child\n        objectName: \"child\"\n        x: 5\n        y: 6\n        width: 50\n        height: 25\n        property int depth: 1\n{child}\n    }}\n}}\n"
        );
        let h = engine();
        // Whatever the environment says: these tests set the tier.
        h.engine.set_tier1(true);
        h.engine.set_tier1_check(false);
        let root = h.load(&source);
        let child = h.find(root, "child");
        // A dependency is a graph source only once the property has a
        // node; a read of one that never changed makes none.
        let e = &h.engine;
        for (obj, names) in [(root, NAMED), (child, CHILD)] {
            for n in names {
                if let Ok(p) = e.property_index(obj, n) {
                    e.property_node(obj, p).expect("a property");
                }
            }
        }
        World { h, root, child }
    }

    /// The root with `property var o<n>: <expression>` for each.
    fn with(expressions: &[impl AsRef<str>]) -> World {
        let mut members = String::new();
        for (n, e) in expressions.iter().enumerate() {
            members.push_str(&format!("    property var o{n}: {}\n", e.as_ref()));
        }
        World::new(&members, "")
    }

    /// The graph nodes a read of a property depends on: its own, and for
    /// an object the one that says the object is alive.
    fn nodes(&self, obj: ObjectId, prop: &str) -> Vec<NodeId> {
        let e = &self.h.engine;
        if let Ok(p) = e.property_index(obj, prop) {
            e.property_node(obj, p).expect("a property");
        }
        let name = prop.to_string();
        let effect = e
            .effect(move |e| {
                e.read_by_name(obj, &name);
            })
            .expect("an effect");
        let sources = e.graph().sources(effect);
        e.graph().dispose(effect).expect("the effect is alive");
        assert!(!sources.is_empty(), "{prop} is a node");
        sources
    }

    /// The graph node of a property with a binding.
    fn node(&self, obj: ObjectId, prop: &str) -> NodeId {
        let nodes = self.nodes(obj, prop);
        let graph = self.h.engine.graph();
        *nodes
            .iter()
            .find(|n| graph.is_bound(**n))
            .unwrap_or(&nodes[0])
    }

    /// Runs the binding of `prop` again and says what it did.
    fn eval(&self, obj: ObjectId, prop: &str) -> Seen {
        let e = &self.h.engine;
        let node = self.node(obj, prop);
        self.h.log();
        e.graph().refresh(node).expect("the property is alive");
        let value = format!("{:?}", self.h.get(obj, prop));
        let mut deps = e.graph().sources(node);
        deps.sort();
        Seen {
            value,
            deps,
            warnings: self.h.warnings(),
        }
    }

    /// The dependencies by name, for a failure a person can read.
    fn names(&self, deps: &[NodeId]) -> Vec<String> {
        let mut known: Vec<(NodeId, String)> = Vec::new();
        let e = &self.h.engine;
        let child = self.child;
        let effect = e
            .effect(move |e| {
                e.visual_parent(child);
            })
            .expect("an effect");
        for node in e.graph().sources(effect) {
            known.push((node, "child.parent".to_string()));
        }
        e.graph().dispose(effect).expect("the effect is alive");
        for n in CHILD {
            for node in self.nodes(self.child, n) {
                known.push((node, format!("child.{n}")));
            }
        }
        for n in NAMED {
            for node in self.nodes(self.root, n) {
                known.push((node, (*n).to_string()));
            }
        }
        let mut names: Vec<String> = deps
            .iter()
            .map(|d| {
                known
                    .iter()
                    .find(|(n, _)| n == d)
                    .map_or_else(|| format!("{d:?}"), |(_, name)| name.clone())
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// `prop` in tier 1, in tier 2 and cross-checked. `Ok(ran)`: the three
    /// agree, and `ran` says tier 1 completed it. `Err`: how they differ.
    fn compare(&self, obj: ObjectId, prop: &str) -> Result<bool, String> {
        let e = &self.h.engine;
        e.set_tier1(true);
        e.set_tier1_check(false);
        let before = e.tier1_stats();
        let one = self.eval(obj, prop);
        let after = e.tier1_stats();
        let ran = after.runs > before.runs && after.bails == before.bails;

        e.set_tier1(false);
        let two = self.eval(obj, prop);

        e.set_tier1(true);
        e.set_tier1_check(true);
        let before = e.tier1_stats();
        let checked = self.eval(obj, prop);
        let after = e.tier1_stats();
        e.set_tier1_check(false);

        if one != two {
            return Err(format!(
                "tier 1 gave {} reading {:?} with {:?}; tier 2 gave {} reading {:?} with {:?}",
                one.value,
                self.names(&one.deps),
                one.warnings,
                two.value,
                self.names(&two.deps),
                two.warnings
            ));
        }
        if after.mismatches != before.mismatches {
            return Err(format!("the cross-check: {:?}", checked.warnings));
        }
        if checked != two {
            return Err(format!(
                "cross-checked it gave {} reading {:?} with {:?}; tier 2 alone gave {} reading {:?} with {:?}",
                checked.value,
                self.names(&checked.deps),
                checked.warnings,
                two.value,
                self.names(&two.deps),
                two.warnings
            ));
        }
        Ok(ran)
    }
}

/// Every expression agrees between the tiers. Gives back which of them
/// tier 1 completed.
fn agree(expressions: &[&str]) -> Vec<bool> {
    let w = World::with(expressions);
    let mut failures = Vec::new();
    let mut ran = Vec::new();
    for (n, expression) in expressions.iter().enumerate() {
        match w.compare(w.root, &format!("o{n}")) {
            Ok(r) => ran.push(r),
            Err(why) => {
                ran.push(false);
                failures.push(format!("`{expression}`: {why}"));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    ran
}

/// Every expression agrees between the tiers and tier 1 runs it to the
/// end.
fn exact(expressions: &[&str]) {
    let ran = agree(expressions);
    let left: Vec<&str> = expressions
        .iter()
        .zip(ran)
        .filter(|(_, ran)| !ran)
        .map(|(e, _)| *e)
        .collect();
    assert!(left.is_empty(), "left to tier 2: {left:#?}");
}

/// The value of one expression, with tier 1 on, and what it read.
fn value_and_deps(expression: &str) -> (Value, Vec<String>) {
    let w = World::with(&[expression]);
    let ran = w
        .compare(w.root, "o0")
        .unwrap_or_else(|why| panic!("`{expression}`: {why}"));
    assert!(ran, "`{expression}` was left to tier 2");
    let seen = w.eval(w.root, "o0");
    (w.h.get(w.root, "o0"), w.names(&seen.deps))
}

#[test]
fn arithmetic_coerces_as_javascript_does() {
    exact(&[
        "i + j",
        "i - r",
        "i * r",
        "i / zero",
        "-i / zero",
        "zero / zero",
        "i % j",
        "j % i",
        "j % zero",
        "-zero",
        "zero * -1",
        "zero * j",
        "i ** 2",
        "2 ** -1",
        "(-8) ** (1 / 3)",
        "1 ** (zero / zero)",
        "(zero / zero) ** 0",
        "i + s",
        "s + i",
        "s + r",
        "s + yes",
        "s + nothing",
        "s + undef",
        "i + yes",
        "i + nothing",
        "i + undef",
        "yes + yes",
        "nothing + nothing",
        "digits * 2",
        "digits - 1",
        "digits + 1",
        "empty * 5",
        "s * 2",
        "+digits",
        "+s",
        "+empty",
        "-digits",
        "+yes",
        "+nothing",
        "+undef",
        "-nothing",
        "\"0x10\" * 1",
        "\"1e3\" * 1",
        "\"Infinity\" * 1",
        "\"-Infinity\" * 1",
        "\"infinity\" * 1",
        "\"0b101\" * 1",
        "\"0o17\" * 1",
        "\"1_000\" * 1",
        "\".5\" * 1",
        "\"5.\" * 1",
        "\"+5\" * 1",
        "\"-0\" * 1",
        "\"\\t7\\n\" * 1",
        "\"1 2\" * 1",
    ]);
}

#[test]
fn numbers_print_as_javascript_prints_them() {
    exact(&[
        "big + \"\"",
        "tiny + \"\"",
        "(0.1 + 0.2) + \"\"",
        "(big * 10) + \"\"",
        "(big / 10) + \"\"",
        "(i / 3) + \"\"",
        "(-tiny) + \"\"",
        "(tiny * 10) + \"\"",
        "(i / zero) + \"\"",
        "(-i / zero) + \"\"",
        "(zero / zero) + \"\"",
        "(-zero) + \"\"",
        "(r * 1e300 * 1e10) + \"\"",
        "(i * 123456789012345680000) + \"\"",
        "(i * 5e-324) + \"\"",
        "(i + 0.000001) + \"\"",
        "String(r)",
        "String(i / 7)",
        "`${i} and ${r}/${s}/${yes}/${nothing}/${undef}`",
        "`${big}${-zero}${zero / zero}`",
        "`${s}`",
        // Powers of two, whose lower neighbour is twice as close as the
        // upper: QuickJS wrote 2^378 as …6374e+113 before the fork's fix
        // (vendor/QUICKJS.md), and tier 1 wrote 2^-24 as …062e-8.
        "(2 ** 378) + \"\"",
        "`${Math.max(2 ** 378, i)}`",
        "String(2 ** -24)",
        "String(2 ** 89)",
        "String(-(2 ** -1017))",
    ]);
    agree(&["`plain`", "`a\\${b}`"]);
}

#[test]
fn comparisons_are_javascripts() {
    exact(&[
        "i < r",
        "i >= i",
        "s < \"world\"",
        "\"10\" < \"9\"",
        "\"10\" < 9",
        "digits == 12",
        "digits === 12",
        "digits != 12",
        "digits !== 12",
        "nothing == undef",
        "nothing === undef",
        "nothing == zero",
        "undef == zero",
        "empty == zero",
        "empty == no",
        "yes == 1",
        "\"1\" == yes",
        "\"true\" == yes",
        "(zero / zero) == (zero / zero)",
        "(zero / zero) != (zero / zero)",
        "(zero / zero) === (zero / zero)",
        "zero === -zero",
        "nothing >= zero",
        "undef >= zero",
        "nothing > zero",
        "s > i",
        "empty < i",
        "other == child",
        "other === child",
        "other != none",
        "other == root",
        "none == null",
        "none === null",
        "none == undefined",
        "none === undefined",
        "undef === undefined",
        "s == \"hello\"",
        "s === \"hell\" + \"o\"",
        "yes === true",
        "i === 7.0",
    ]);
}

#[test]
fn logic_gives_back_the_operand() {
    exact(&[
        "yes && i",
        "no && i",
        "no || s",
        "empty || \"fallback\"",
        "zero || nothing",
        "(zero / zero) || s",
        "nothing ?? i",
        "undef ?? s",
        "zero ?? i",
        "empty ?? i",
        "!s",
        "!empty",
        "!!list",
        "!nothing",
        "!undef",
        "!(zero / zero)",
        "!other",
        "!none",
        "!zero",
        "!-zero",
        "yes ? i : s",
        "no ? i : s",
        "empty ? 1 : 2",
        "other ? other.width : -1",
        "none ? none.width : -1",
        "(i, r)",
        "void i",
    ]);
}

#[test]
fn bit_operators_wrap_to_32_bits() {
    exact(&[
        "i & j",
        "i | j",
        "i ^ j",
        "~i",
        "~r",
        "i << 30",
        "i << 33",
        "i << -1",
        "j >> 1",
        "j >>> 1",
        "j >>> 0",
        "big | 0",
        "(i * 2147483648) | 0",
        "(i + 2147483641) | 0",
        "(j - 2147483646) | 0",
        "(zero / zero) | 0",
        "(i / zero) | 0",
        "(i * 4294967296.5) >>> 0",
        "\"7\" & 3",
        "s | 0",
        "nothing | 0",
        "yes << yes",
        "r | 0",
        "-r | 0",
    ]);
}

#[test]
fn typeof_names_what_javascript_names() {
    exact(&[
        "typeof i",
        "typeof r",
        "typeof s",
        "typeof yes",
        "typeof undef",
        "typeof (zero / zero)",
        "typeof (i + s)",
    ]);
    // Objects, lists and value types are objects of the JS engine: what
    // `typeof` says of them is its business.
    agree(&[
        "typeof nothing",
        "typeof list",
        "typeof other",
        "typeof none",
        "typeof pt",
        "typeof color",
    ]);
}

#[test]
fn members_of_objects_and_value_types() {
    exact(&[
        "pt.x + pt.y",
        "sz.width * sz.height",
        "rc.x + rc.width",
        "rc.y - rc.height",
        "color.r",
        "color.g + color.b",
        "color.a",
        "border.width + 1",
        "root.border.width * 2",
        "child.x + child.y",
        "other.width + 1",
        "other.depth + 1",
        "root.other.depth + child.depth",
        "child.parent.width + 1",
        "child.parent === root",
        "list.length + 1",
        "list[0] + 1",
        "list[1] + list[2]",
        "list[3] === null",
        "list[9] === undefined",
        "list[i - 7]",
        "s.length + 1",
        "s[1] + s[0]",
        "s[9] === undefined",
        "width + height",
        "root.width - root.height",
        "none?.width ?? -1",
        "other?.width + 1",
        "undef?.x === undefined",
        "nothing?.x === undefined",
        "[i, s, [r, yes], null, undefined]",
        "[pt.x, sz.height]",
    ]);
    // Reads tier 1 hands to JavaScript: a member the type does not
    // declare, a member of nothing.
    agree(&[
        "none.width",
        "undef.x",
        "other.nonesuch",
        "pt.nonesuch",
        "rc.left + rc.right + rc.top + rc.bottom",
        "list[\"length\"]",
        "list[1.5]",
        "list[-1]",
        "s[-1]",
        "link + \"\"",
        "pt == pt",
        "pt === pt",
        "color == color",
        "color == \"#336699\"",
        "color + \"\"",
        "pt + \"\"",
        "list + \"\"",
        "other + 1 === 1",
        "list.nonesuch",
        "i.nonesuch",
        "s.nonesuch",
    ]);
}

#[test]
fn the_child_reads_its_parent() {
    let w = World::new(
        "",
        "        property var c0: parent.width + depth\n        property var c1: parent.parent\n        property var c2: parent.i * x\n        property var c3: parent.border.width + root.radius\n        property var c4: parent.parent ? 1 : 2\n        property var c5: parent.twice(depth)\n",
    );
    for n in 0..6 {
        let prop = format!("c{n}");
        let ran = w
            .compare(w.child, &prop)
            .unwrap_or_else(|why| panic!("{prop}: {why}"));
        // `parent.parent` is a path, which tier 0 reads; a function of
        // another object is called, not inlined.
        if n != 1 && n != 5 {
            assert!(ran, "{prop} was left to tier 2");
        }
    }
    assert_eq!(w.h.num(w.child, "c0"), 201.0);
    assert_eq!(w.h.num(w.child, "c2"), 35.0);
}

#[test]
fn math_is_javascripts_math() {
    exact(&[
        "Math.max(i, r)",
        "Math.min(i, j, r)",
        "Math.max()",
        "Math.min()",
        "Math.max(i, \"9\")",
        "Math.max(i, undef)",
        "Math.max(i, nothing)",
        "Math.max(zero, -zero)",
        "Math.max(-zero, zero)",
        "Math.min(zero, -zero)",
        "Math.min(-zero, zero)",
        "Math.max(zero / zero, i)",
        "Math.min(i, zero / zero)",
        "Math.round(r)",
        "Math.round(-r)",
        "Math.round(-0.4)",
        "Math.round(0.49999999999999994)",
        "Math.round(i * 643371375338640.5)",
        "Math.round(-zero)",
        "Math.round(i / zero)",
        "Math.floor(-r)",
        "Math.floor(-zero)",
        "Math.ceil(-0.5)",
        "Math.ceil(r)",
        "Math.abs(j)",
        "Math.abs(-zero)",
        "Math.sqrt(i)",
        "Math.sqrt(j)",
        "Math.pow(i, 2)",
        "Math.pow(i, -zero)",
        "Math.pow(1, i / zero)",
        "Math.pow(-zero, j)",
        "Math.pow(j, 0.5)",
        "Math.sign(j)",
        "Math.sign(-zero)",
        "Math.sign(zero / zero)",
        "Math.trunc(-r)",
        "Math.trunc(-0.5)",
        "Math.PI * 2",
        "Math.E + Math.LN2 + Math.LN10 + Math.LOG2E + Math.LOG10E + Math.SQRT2 + Math.SQRT1_2",
        "Math.atan2(i, j)",
        "Math.atan2(zero, -zero)",
        "Math.sin(r)",
        "Math.cos(r)",
        "Math.tan(r)",
        "Math.asin(0.5)",
        "Math.acos(0.5)",
        "Math.atan(r)",
        "Math.asin(i)",
        "Math.log(i)",
        "Math.log(zero)",
        "Math.log(j)",
        "Math.exp(1)",
        "Math.exp(r)",
        "Math.cbrt(27)",
        "Math.cbrt(j)",
        "Math.log2(8)",
        "Math.log10(1000)",
        "Math.abs(s)",
        "Math.floor(digits)",
        "Math.floor(nothing)",
        "Math.floor(undef)",
        "Math.floor()",
        "Math.abs(i, j)",
        "Number.MAX_VALUE + Number.MIN_VALUE + Number.EPSILON",
        "Number.MAX_SAFE_INTEGER + Number.MIN_SAFE_INTEGER",
        "Number.POSITIVE_INFINITY + Number.NEGATIVE_INFINITY",
        "Number.NaN",
    ]);
}

#[test]
fn conversions_and_methods_of_primitives() {
    exact(&[
        "Number(digits)",
        "Number(empty)",
        "Number(nothing)",
        "Number(undef)",
        "Number(s)",
        "Number(yes)",
        "Number()",
        "String(i)",
        "String(nothing)",
        "String(undef)",
        "String(yes)",
        "String()",
        "Boolean(empty)",
        "Boolean(s)",
        "Boolean()",
        "Boolean(zero / zero)",
        "isNaN(s)",
        "isNaN(digits)",
        "isNaN(undef)",
        "isNaN(nothing)",
        "isFinite(i / zero)",
        "isFinite(digits)",
        "isFinite(nothing)",
        "r.toFixed(0)",
        "r.toFixed(1)",
        "r.toFixed()",
        "(i * 1.005 / 7).toFixed(2)",
        "(yes * 0.5).toFixed(0)",
        "(yes * 1.5).toFixed(0)",
        "(yes * 2.5).toFixed(0)",
        "(yes * -1.5).toFixed(0)",
        "(yes * -0.0001).toFixed(2)",
        "(-zero).toFixed(1)",
        "(yes * 1.45).toFixed(1)",
        "(yes * 8.345).toFixed(2)",
        "(yes * 0.000001).toFixed(7)",
        "(yes * 123.456).toFixed(10)",
        "(yes * 1e20).toFixed(2)",
        "big.toFixed(2)",
        "(zero / zero).toFixed(2)",
        "(i / zero).toFixed(2)",
        "i.toString()",
        "r.toString()",
        "s.toString()",
        "yes.toString()",
        "s.toUpperCase()",
        "\"MiXed 123\".toLowerCase() + s",
        "digits.trim()",
        "(\"\\t\\n x \\u00a0\\ufeff\" + s).trim()",
        "\"%1 of %2\".arg(i).arg(s)",
        "\"%1%\".arg(r)",
        "\"%2 %1\".arg(s).arg(yes)",
        "\"%1 %1\".arg(i)",
        "\"%1\".arg(big)",
        "\"%1\".arg(tiny)",
        "\"%1\".arg(i / 3)",
        "\"%1\".arg(nothing)",
        "\"%1\".arg(undef)",
        "\"%L1\".arg(i * 1000.5)",
    ]);
    // Receivers and arguments tier 1 does not take.
    agree(&[
        "r.toFixed(101)",
        "r.toFixed(-1)",
        "r.toFixed(1.7)",
        "r.toFixed(s)",
        "s.toFixed(1)",
        "i.toString(2)",
        "i.toUpperCase()",
        "nothing.toString()",
        "list.toString()",
        "other.toString()",
        "\"Straße ǆ\".toUpperCase() + s",
        "\"İ\".toLowerCase() + s",
        "\"%1\".arg(list)",
        "\"%1\".arg(other)",
        "\"%1\".arg(i, j)",
        "\"%1\".arg()",
        // No place for the argument: a warning, which is JavaScript's to
        // give.
        "s.arg(i)",
    ]);
}

/// `Qt.lighter` and `Qt.darker` cut and round in many places: tier 1 gives
/// the colour the script does over a spread of colours and factors.
#[test]
fn lighter_and_darker_agree_with_the_script() {
    let channels = ["0", "0.07", "0.33", "0.5", "0.81", "1"];
    let mut expressions = Vec::new();
    for (n, r) in channels.iter().enumerate() {
        for g in &channels {
            let b = channels[(n * 7 + g.len()) % channels.len()];
            for (f, by) in ["1.05", "1.1", "1.15", "1.5", "2", "3.7", "0.6", "0.57"]
                .iter()
                .enumerate()
            {
                let which = if (n + f) % 2 == 0 {
                    "lighter"
                } else {
                    "darker"
                };
                expressions.push(format!("Qt.{which}(Qt.rgba({r}, {g}, {b}, 0.6), {by})"));
            }
        }
    }
    let expressions: Vec<&str> = expressions.iter().map(String::as_str).collect();
    exact(&expressions);
}

#[test]
fn the_qt_object_makes_values() {
    exact(&[
        "Qt.rgba(1, 0.5, 0.25, 1)",
        "Qt.rgba(0.2, 0.4, 0.6, 0.5)",
        "Qt.rgba(r, -1, 0.3)",
        "Qt.rgba(0.1, 0.2, 0.3, zero / zero)",
        "Qt.hsla(0.5, 0.5, 0.5, 1)",
        "Qt.hsla(0.9, 1, 0.25, 0.5)",
        "Qt.hsla(0, 0, 0.5, 1)",
        "Qt.hsla(1.5, 0.3, 0.7)",
        "Qt.hsva(0.1, 0.7, 0.9, 1)",
        "Qt.hsva(0.66, 0.2, 0.4, 0.25)",
        "Qt.hsva(0, 0, 1)",
        "Qt.rgba(0.2, 0.4, 0.6, 1).g",
        "Qt.point(i, r)",
        "Qt.point(-zero, s)",
        "Qt.size(i, j)",
        "Qt.rect(1, 2, i, r)",
        "Qt.vector2d(1, r)",
        "Qt.vector3d(1, 2, i)",
        "Qt.vector4d(1, 2, 3, 0.1)",
        "Qt.quaternion(1, 0, 0.1, 0)",
        "Qt.point(i, r).x + Qt.size(i, j).height",
        "Qt.rect(1, 2, i, r).width",
        "Qt.vector3d(1, 2, i).z",
        "Qt.AlignLeft | Qt.AlignTop",
        "Qt.Key_A + Qt.Key_Return",
        "Qt.LeftButton | Qt.RightButton",
        "Qt.red",
        "Qt.Horizontal === 1",
    ]);
    agree(&[
        "Qt.point(i)",
        "Qt.point()",
        "Qt.size(1, 2, 3)",
        "Qt.rect(1, 2, 3)",
        "Qt.vector3d(1, 2)",
        "Qt.rgba()",
        "Qt.rgba(1, 2)",
        "Qt.rgba(s, 0, 0, 1)",
        "Qt.nonesuch",
        "Qt.platform.os",
        "Qt.point(other, 1)",
        "Qt.lighter(color)",
        "Qt.darker(s)",
        "Qt.darker(color, s)",
        "Qt.darker(color, 1, 2)",
        "Qt.lighter(color, 1 / 0)",
        "Qt.lighter(other)",
        "Qt.lighter(color, 1.2).hsvHue",
    ]);
    exact(&[
        "Qt.darker(color)",
        "Qt.darker(Qt.rgba(0.2, 0.4, 0.6, 1), 3)",
        "Qt.darker(Qt.rgba(0.5, 0.5, 0.5, 0.3), 1.25)",
        "Qt.darker(Qt.rgba(0.3, 0.6, 0.9, 1), 0)",
        "Qt.darker(Qt.rgba(1, 0, 0, 1), -1)",
        "Qt.darker(Qt.rgba(0.9, 0.1, 0.4, 1), 0.5)",
        "Qt.darker(Qt.rgba(0.1, 0.8, 0.3, 1), zero / zero)",
        "Qt.darker(Qt.rgba(0.7, 0.2, 0.9, 1), undef)",
        "Qt.lighter(Qt.rgba(0.9, 0.5, 0.1, 0.5))",
        "Qt.lighter(Qt.rgba(0.6, 0.7, 0.8, 1), 1.9)",
        "Qt.lighter(Qt.rgba(0, 0, 0, 1), 2)",
        "Qt.lighter(Qt.rgba(0.2, 0.3, 0.25, 1), 0.7)",
        "Qt.lighter(Qt.rgba(0.95, 0.94, 0.2, 1), 3)",
        "Qt.darker(nothing)",
        "Qt.lighter(i)",
        "Qt.darker(color).r + Qt.lighter(color, 1.2).g",
    ]);
}

#[test]
fn translation_functions_give_the_source_text_without_a_catalogue() {
    exact(&[
        "qsTr(\"Hello\") + s",
        "qsTr(\"Hello %1\").arg(s)",
        "qsTr(\"%n file(s)\", \"\", i)",
        "qsTr(\"Open\", \"the verb\") + s",
        "qsTranslate(\"Elsewhere\", \"Hi\") + s",
        "qsTranslate(\"Elsewhere\", \"%n thing(s)\", \"\", j)",
        "qsTrId(\"an-id\") + s",
        "QT_TR_NOOP(\"kept\") + s",
        "QT_TRID_NOOP(\"kept-id\") + s",
        "QT_TRANSLATE_NOOP(\"Elsewhere\", \"kept\") + s",
    ]);
    // A translation and nothing else is a binding of its own kind.
    agree(&[
        "qsTr(\"Hello\")",
        "qsTr(\"Open\", \"the verb\")",
        "qsTranslate(\"Elsewhere\", \"Hi\")",
        "qsTrId(\"an-id\")",
        "qsTr(s)",
        "qsTr()",
        "qsTr(i)",
        "qsTr(\"a\", \"b\", s)",
    ]);
}

#[test]
fn functions_of_the_file_are_inlined() {
    exact(&[
        "twice(i)",
        "twice(twice(r)) + 1",
        "twice(s)",
        "twice()",
        "twice(i, j)",
        "pick(yes, i, s)",
        "pick(no, i, s)",
        "pick(empty, i)",
        "area()",
        "root.twice(3) + area()",
        "grade(i * 3)",
        "grade(j)",
        "grade(i)",
        "grade(twice(i))",
    ]);
}

#[test]
fn a_binding_block_runs_its_statements() {
    exact(&[
        "{ if (yes) return i; return s }",
        "{ if (no) { return i } else { return s } }",
        "{ var t = i * 2; let u = t + r; const v = u - 1; return v }",
        "{ var t = i; if (t > 5) t = t * 10; return t }",
        "{ var t = s; t += i; t += \"!\"; return t }",
        "{ let n = zero; n++; ++n; n--; return n + 10 }",
        "{ if (i > 100) return 1 }",
        "{ var a; return a }",
        // An operand is not worked out in the register of a variable the
        // rest of the expression reads.
        "{ var t = i; t = (t * 2) + t; return t }",
        "{ var t = i; t = -(t + 1) * t - (t - 1) / t; return t }",
        "{ var t = s; t = yes ? (t + i) + t : t; return t }",
        "twice((i + 1) * i) + twice(i)",
    ]);
    agree(&[
        "{ var t = 0; for (var k = 0; k < i; k++) t += k; return t }",
        "{ switch (i) { case 7: return \"seven\"; default: return \"other\" } }",
        "{ try { return none.width } catch (e) { return -1 } }",
        "{ var f = function () { return i }; return f() }",
        "{ return { a: i } .a }",
    ]);
}

#[test]
fn a_branch_not_taken_is_not_a_dependency() {
    let deps = |e: &str| value_and_deps(e).1;
    assert_eq!(deps("yes ? i : r"), ["i", "yes"]);
    assert_eq!(deps("no ? i : r"), ["no", "r"]);
    assert_eq!(deps("no && i"), ["no"]);
    assert_eq!(deps("yes && i"), ["i", "yes"]);
    assert_eq!(deps("yes || i"), ["yes"]);
    assert_eq!(deps("no || i"), ["i", "no"]);
    assert_eq!(deps("i ?? r"), ["i"]);
    assert_eq!(deps("nothing ?? r"), ["nothing", "r"]);
    assert_eq!(deps("none?.width ?? j"), ["j", "none"]);
    assert_eq!(deps("other?.width + 1"), ["child.width", "other"]);
    assert_eq!(deps("{ if (no) return i; return s }"), ["no", "s"]);
    // JavaScript evaluates every argument before the call.
    assert_eq!(deps("pick(yes, i, r)"), ["i", "r", "yes"]);
    assert_eq!(deps("grade(j)"), ["i", "j"]);
    assert_eq!(
        deps("child.x + child.parent.width"),
        ["child.parent", "child.x", "width"]
    );
    assert_eq!(deps("pt.x + sz.width + rc.height"), ["pt", "rc", "sz"]);
    assert_eq!(deps("list[0] + list.length"), ["list"]);
    assert_eq!(deps("s.length + s[0]"), ["s"]);
    assert_eq!(deps("Math.max(i, j) + twice(r)"), ["i", "j", "r"]);
}

#[test]
fn a_dependency_changes_the_value_in_tier_1() {
    let w = World::with(&["yes ? i * 2 : s + \"!\""]);
    w.h.engine.set_tier1(true);
    w.h.engine.set_tier1_check(false);
    assert_eq!(w.h.num(w.root, "o0"), 14.0);
    w.h.set(w.root, "i", Value::Int(5));
    assert_eq!(w.h.num(w.root, "o0"), 10.0);
    w.h.set(w.root, "yes", Value::Bool(false));
    assert_eq!(w.h.get(w.root, "o0"), Value::string("hello!"));
    w.h.set(w.root, "s", Value::string("bye"));
    assert_eq!(w.h.get(w.root, "o0"), Value::string("bye!"));
    let stats = w.h.engine.tier1_stats();
    assert!(stats.runs >= 4, "{stats:?}");
    assert_eq!(stats.bails, 0, "{stats:?}");
}

#[test]
fn a_typed_property_converts_what_either_tier_gives() {
    let typed: &[(&str, &str)] = &[
        ("int", "r * 3"),
        ("int", "i / 2"),
        ("int", "digits * 2"),
        ("int", "s + 1"),
        ("real", "digits * 2"),
        ("real", "i / 3"),
        ("string", "i + 1"),
        ("string", "yes"),
        ("string", "r + s"),
        ("bool", "s"),
        ("bool", "empty + empty"),
        ("bool", "i - 7"),
        ("color", "Qt.rgba(1, 0.5, 0, 1)"),
        ("color", "yes ? \"red\" : \"blue\""),
        ("color", "s + s"),
        ("point", "Qt.point(i, r)"),
        ("size", "Qt.size(i, r)"),
        ("rect", "Qt.rect(i, r, j, 1)"),
        ("url", "s + \".png\""),
        ("Item", "yes ? child : null"),
        ("Item", "no ? child : null"),
        ("var", "yes ? child : null"),
        ("var", "[i, child]"),
        ("date", "i * 1000"),
    ];
    let mut members = String::new();
    for (n, (ty, e)) in typed.iter().enumerate() {
        members.push_str(&format!("    property {ty} t{n}: {e}\n"));
    }
    let w = World::new(&members, "");
    let mut failures = Vec::new();
    for (n, (ty, e)) in typed.iter().enumerate() {
        if let Err(why) = w.compare(w.root, &format!("t{n}")) {
            failures.push(format!("property {ty}: `{e}`: {why}"));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The state a script of handler calls leaves, step by step.
fn play(tier1: bool, check: bool) -> (Vec<String>, ljos_engine::Tier1Stats) {
    let h = engine();
    h.engine.set_tier1(tier1);
    h.engine.set_tier1_check(check);
    let root = h.load(
        r#"import QtQuick
Rectangle {
    id: root
    property int count: 0
    property int seen: 0
    property real total: 1.5
    property string label: "a"
    property var any
    readonly property int fixed: 1
    signal poke(int amount)
    onPoke: count += amount
    onCountChanged: seen = count * 2
    function bump() { count++ }
    function down() { --count }
    function reset() { count = 0 }
    function add(n) { total = total + n }
    function scale(n) { total *= n }
    function name(text) { label += text }
    function thick(w) { border.width = w }
    function move(to) { child.x = to }
    function far(to) { root.child2.y = to }
    function keep(v) { any = v }
    function spoil() { fixed = 2 }
    function wide(v) { width = v }
    function tint() { color = Qt.rgba(1, 0, 0, 1) }
    function either(c) { if (c) count = 10; else label = "no" }
    function next() { return count + 1 }
    function swap() { var t = count; count = t * 2; return t }
    function twoWrites() { count = 1; total = 2 }
    property Item child2: child
    Item { id: child; objectName: "child" }
}
"#,
    );
    let child = h.find(root, "child");
    let e = &h.engine;
    let mut steps = Vec::new();
    let mut note = |what: &str, result: String| {
        let state: Vec<String> = [
            "count", "seen", "total", "label", "any", "fixed", "width", "color",
        ]
        .iter()
        .map(|p| format!("{p}={:?}", h.get(root, p)))
        .collect();
        steps.push(format!(
            "{what} -> {result}: {} border={:?} child=({:?},{:?}) warnings={:?}",
            state.join(" "),
            h.get(root, "border.width"),
            h.get(child, "x"),
            h.get(child, "y"),
            h.warnings()
        ));
    };
    let calls: &[(&str, Vec<Value>)] = &[
        ("bump", vec![]),
        ("bump", vec![]),
        ("down", vec![]),
        ("add", vec![Value::Real(2.25)]),
        ("add", vec![Value::string("x")]),
        ("scale", vec![Value::Int(3)]),
        ("name", vec![Value::string("b")]),
        ("name", vec![Value::Int(4)]),
        ("thick", vec![Value::Int(4)]),
        ("move", vec![Value::Real(9.5)]),
        ("far", vec![Value::Int(-2)]),
        ("keep", vec![Value::string("x")]),
        ("keep", vec![Value::Int(3)]),
        ("keep", vec![Value::Null]),
        ("keep", vec![]),
        ("spoil", vec![]),
        ("wide", vec![Value::string("12")]),
        ("wide", vec![Value::string("wide")]),
        ("tint", vec![]),
        ("either", vec![Value::Bool(true)]),
        ("either", vec![Value::Bool(false)]),
        ("next", vec![]),
        ("swap", vec![]),
        ("twoWrites", vec![]),
        ("reset", vec![]),
    ];
    for (name, args) in calls {
        let result = e.invoke_by_name(root, name, args);
        note(&format!("{name}{args:?}"), format!("{result:?}"));
    }
    e.emit_by_name(root, "poke", &[Value::Int(4)])
        .expect("poke");
    note("poke(4)", String::new());
    (steps, e.tier1_stats())
}

#[test]
fn handlers_and_functions_write_the_same_in_both_tiers() {
    let (two, off) = play(false, false);
    let (one, on) = play(true, false);
    let (checked, check) = play(true, true);
    assert_eq!(off.runs, 0, "tier 1 off runs nothing: {off:?}");
    for (a, b) in one.iter().zip(&two) {
        assert_eq!(a, b, "tier 1 against tier 2");
    }
    for (a, b) in checked.iter().zip(&two) {
        assert_eq!(a, b, "cross-checked against tier 2");
    }
    assert_eq!(one.len(), two.len());
    assert!(on.runs >= 20, "tier 1 ran the handlers: {on:?}");
    assert_eq!(check.mismatches, 0, "{check:?}");
    assert_eq!(check.checked, check.runs, "{check:?}");
}

#[test]
fn a_function_a_derived_file_replaces_is_not_the_one_inlined() {
    for (tier1, check) in [(true, false), (false, false), (true, true)] {
        let h = engine();
        h.engine.set_tier1(tier1);
        h.engine.set_tier1_check(check);
        h.files(&[
            (
                "Base",
                "import QtQuick\nItem {\n    property int n: 1\n    property int v: f() + n\n    function f() { return 1 }\n}\n",
            ),
            (
                "Main",
                "import QtQuick\nItem {\n    Base { objectName: \"plain\" }\n    Base { objectName: \"over\"; function f() { return 10 } }\n}\n",
            ),
        ]);
        let root = h
            .engine
            .create_from_url("Main", None, Vec::new())
            .expect("Main.qml creates");
        let (plain, over) = (h.find(root, "plain"), h.find(root, "over"));
        assert_eq!(h.num(plain, "v"), 2.0, "tier 1 {tier1}, check {check}");
        assert_eq!(h.num(over, "v"), 11.0, "tier 1 {tier1}, check {check}");
        h.set(over, "n", Value::Int(5));
        h.set(plain, "n", Value::Int(5));
        assert_eq!(h.num(plain, "v"), 6.0);
        assert_eq!(h.num(over, "v"), 15.0);
        let stats = h.engine.tier1_stats();
        assert_eq!(stats.mismatches, 0, "{stats:?}");
        if tier1 {
            assert!(stats.runs >= 2, "the plain one ran in tier 1: {stats:?}");
            assert!(stats.bails >= 2, "the replaced one did not: {stats:?}");
        }
        assert!(h.warnings().is_empty());
    }
}

#[test]
fn the_switch_turns_tier_1_off() {
    let w = World::with(&["i * 2 + r"]);
    let e = &w.h.engine;
    e.set_tier1(false);
    assert!(!e.tier1());
    let before = e.tier1_stats();
    let off = w.eval(w.root, "o0");
    assert_eq!(e.tier1_stats(), before, "nothing runs in tier 1");
    e.set_tier1(true);
    assert!(e.tier1());
    let on = w.eval(w.root, "o0");
    assert_eq!(e.tier1_stats().runs, before.runs + 1);
    assert_eq!(on, off);
    assert_eq!(on.value, "16.5");
}

#[test]
fn the_cross_check_reports_a_difference_and_uses_tier_2() {
    let w = World::with(&["Math.max(i, 1) + 1"]);
    let e = &w.h.engine;
    assert_eq!(w.h.num(w.root, "o0"), 8.0);
    // What tier 1 takes for granted, broken on purpose.
    e.eval_js("Math.max = function () { return 41 }")
        .expect("the script runs");
    e.set_tier1(true);
    e.set_tier1_check(true);
    assert!(e.tier1_check());
    let before = e.tier1_stats();
    let seen = w.eval(w.root, "o0");
    let after = e.tier1_stats();
    assert_eq!(seen.value, "42.0", "tier 2's value is used");
    assert_eq!(after.mismatches, before.mismatches + 1);
    assert_eq!(after.checked, before.checked + 1);
    let [warning] = &seen.warnings[..] else {
        panic!("one warning: {:?}", seen.warnings);
    };
    assert!(
        warning.contains("Main.qml:42:22: tier 1 and tier 2 differ on")
            && warning.contains("Math.max($scope.i, 1) + 1"),
        "{warning}"
    );
    assert!(warning.contains("tier 1 gave 8.0"), "{warning}");
    assert!(warning.contains("tier 2 gave 42.0"), "{warning}");

    // Unchecked, tier 1 is believed.
    e.set_tier1_check(false);
    assert_eq!(w.eval(w.root, "o0").value, "8.0");
}

// ---------------------------------------------------------------- sweeps

/// Doubles of every kind, the same on every run: bit patterns at random
/// (so NaNs, infinities, subnormals and huge numbers), small numbers, and
/// the neighbours of the halves and of the powers of two where rounding
/// goes wrong.
fn doubles(count: usize) -> Vec<f64> {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut out = vec![
        0.0,
        1.0,
        -1.0,
        0.5,
        -0.5,
        1.5,
        2.5,
        -2.5,
        0.1,
        0.7,
        1.005,
        8.345,
        1.45,
        0.000001,
        0.0000001,
        123.456,
        1e21,
        1e20,
        -1e21,
        999999999999999900000.0,
        1e-7,
        1e-6,
        5e-324,
        2.2250738585072014e-308,
        f64::MAX,
        f64::MIN,
        f64::EPSILON,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        0.49999999999999994,
        -0.49999999999999994,
        4503599627370495.5,
        4503599627370496.5,
        4503599627370497.0,
        -4503599627370495.5,
        9007199254740991.0,
        9007199254740992.0,
        9007199254740993.0,
        2147483647.0,
        2147483648.0,
        -2147483648.0,
        -2147483649.0,
        4294967295.0,
        4294967296.0,
        4294967296.5,
        std::f64::consts::PI,
        std::f64::consts::E,
    ];
    while out.len() < count {
        let bits = next();
        let v = match bits % 6 {
            0 => f64::from_bits(next()),
            1 => (next() % 2001) as f64 / 8.0 - 125.0,
            2 => (next() % 100_000) as f64 / 1000.0 - 50.0,
            3 => ((next() % 41) as f64 - 20.0) * std::f64::consts::PI / 8.0,
            // A half, and the doubles on either side of it.
            4 => {
                let half = (next() % 2000) as f64 - 1000.0 + 0.5;
                f64::from_bits(half.to_bits().wrapping_add(next() % 3).wrapping_sub(1))
            }
            _ => {
                let scale = 10f64.powi((next() % 40) as i32 - 20);
                ((next() % 2_000_001) as f64 - 1_000_000.0) * scale
            }
        };
        out.push(v);
    }
    out
}

/// Runs every expression cross-checked for every input: tier 1 and tier 2
/// agree on all of them, and tier 1 ran at least `least` of each hundred
/// evaluations to the end.
fn sweep(
    members: &str,
    expressions: &[String],
    rounds: usize,
    least: u64,
    mut input: impl FnMut(&World, usize),
) {
    let mut all = String::from(members);
    for (n, e) in expressions.iter().enumerate() {
        all.push_str(&format!("    property var o{n}: {e}\n"));
    }
    let w = World::new(&all, "");
    let e = &w.h.engine;
    e.set_tier1(true);
    e.set_tier1_check(true);
    let props: Vec<_> = (0..expressions.len())
        .map(|n| e.property_index(w.root, &format!("o{n}")).expect("o<n>"))
        .collect();
    for round in 0..rounds {
        input(&w, round);
        for p in &props {
            e.read(w.root, *p);
        }
        let stats = e.tier1_stats();
        if stats.mismatches > 0 {
            let differences: Vec<String> =
                w.h.warnings()
                    .into_iter()
                    .filter(|m| m.contains("tier 1 and tier 2 differ"))
                    .collect();
            panic!("round {round}:\n{}", differences.join("\n"));
        }
        w.h.log();
    }
    let stats = e.tier1_stats();
    let evaluations = (rounds * expressions.len()) as u64;
    println!("{stats:?} of {evaluations} evaluations");
    assert!(
        stats.runs * 100 >= evaluations * least,
        "{stats:?} of {evaluations} evaluations"
    );
}

#[test]
fn numbers_of_every_kind_agree() {
    let mut expressions: Vec<String> = [
        "abs", "acos", "asin", "atan", "cbrt", "ceil", "cos", "exp", "floor", "log", "log10",
        "log2", "round", "sign", "sin", "sqrt", "tan", "trunc",
    ]
    .iter()
    .flat_map(|f| [format!("Math.{f}(a)"), format!("Math.{f}(-a)")])
    .collect();
    for f in ["atan2", "pow", "max", "min"] {
        expressions.push(format!("Math.{f}(a, b)"));
        expressions.push(format!("Math.{f}(-a, b)"));
        expressions.push(format!("Math.{f}(b, -a)"));
    }
    for op in [
        "+", "-", "*", "/", "%", "**", "&", "|", "^", "<<", ">>", ">>>", "<", "<=", "==", "===",
    ] {
        expressions.push(format!("a {op} b"));
        expressions.push(format!("(-a) {op} b"));
    }
    for digits in (0..=20).chain([21, 37, 50, 99, 100]) {
        expressions.push(format!("a.toFixed({digits})"));
        expressions.push(format!("(-a).toFixed({digits})"));
    }
    for e in [
        "a | 0",
        "a >>> 0",
        "~a",
        "-a",
        "+a",
        "!a",
        "a ? 1 : 2",
        "(-a) ? 1 : 2",
        "typeof a",
        "isNaN(a)",
        "isFinite(a)",
        "Boolean(a)",
        "a + \"\"",
        "(-a) + \"\"",
        "`${a}|${b}`",
        "String(a)",
        "a.toString()",
        "\"%1 %2\".arg(a).arg(-b)",
        "1 / -a",
        "Qt.rgba(a, b, a * b, 1)",
        "Qt.rgba(a / 255, b / 255, 0.5, a)",
        "Qt.hsla(a, b, 0.5, 1)",
        "Qt.hsla(a / 360, b / 100, a / 250, b)",
        "Qt.hsva(a, b, 0.5, b)",
        "Qt.hsva(a / 360, b / 100, a / 250, 1)",
        "Qt.rgba(a / 255, b / 255, 0.5, 1).r",
        "Qt.point(a, b)",
        "Qt.point(-a, b).x",
        "Qt.size(a, -b)",
        "Qt.rect(a, b, -a, -b)",
        "Qt.vector3d(a, b, -a)",
        "Qt.vector3d(a, b, -a).z",
        "Qt.quaternion(a, b, -a, -b)",
        "[a, -a, b]",
        "twice(-a)",
        "pick(-a, a, b)",
    ] {
        expressions.push(e.to_string());
    }
    let values = doubles(1500);
    sweep(
        "    property real a: 0\n    property real b: 0\n",
        &expressions,
        values.len(),
        90,
        |w, round| {
            w.h.set(w.root, "a", Value::Real(values[round]));
            // Another number each time, and now and then the same one.
            let other = values[(round * 7 + 3) % values.len()];
            w.h.set(
                w.root,
                "b",
                Value::Real(if round % 5 == 0 { values[round] } else { other }),
            );
        },
    );
}

#[test]
fn strings_of_every_kind_agree() {
    let mut texts: Vec<String> = [
        "",
        " ",
        "0",
        "-0",
        "+0",
        "12",
        " 12 ",
        "\t12\n",
        "\u{a0}12\u{feff}",
        "\u{2028}12\u{3000}",
        "\u{180e}12",
        "\u{85}12",
        "1 2",
        "12abc",
        "abc",
        ".",
        "+",
        "-",
        "e1",
        "1e",
        "1e+",
        "1e3",
        "1E3",
        "1e+400",
        "1e-400",
        "-1e400",
        "1.e2",
        ".5",
        "+.5",
        "-.5",
        "5.",
        "0x10",
        "0X1f",
        "-0x10",
        "+0x10",
        "0x",
        "0xg",
        "0x1.8",
        "0b101",
        "0B11",
        "0b2",
        "0o17",
        "0O7",
        "0o8",
        "017",
        "1_000",
        "1n",
        "1,5",
        "Infinity",
        "-Infinity",
        "+Infinity",
        "infinity",
        "INFINITY",
        "Infinityx",
        "inf",
        "NaN",
        "nan",
        "true",
        "null",
        "undefined",
        "١٢",
        "１２",
        "9007199254740993",
        "9007199254740993.00000000000000000001",
        "9007199254740992.99999999999999999999",
        "0.1000000000000000055511151231257827021181583404541015625",
        "0.30000000000000004",
        "123456789012345678901234567890",
        "0.000000000000000000000000000000000000001",
        "4.9406564584124654e-324",
        "2.4703282292062327e-324",
        "2.4703282292062328e-324",
        "1.7976931348623157e308",
        "1.7976931348623159e308",
        "0x1fffffffffffff",
        "0x20000000000001",
        "0xffffffffffffffffffff",
        "0b11111111111111111111111111111111111111111111111111111",
        "00012",
        "-00012.50",
        "1e0003",
        "%1",
        "%1 and %2",
        "%2 %1",
        "%L1",
        "%01",
        "%99 %1",
        "%100",
        "%0",
        "%%1",
        "%1%1",
        "100%",
        "é",
        "ß",
        "İ",
        "ǆ",
        "ﬁ",
        "😀",
        "a😀b",
        "Ünïcödé",
        "MiXeD cAsE 123",
        " \t\n\u{b}\u{c}\r x \u{1680}\u{2000}\u{200a}\u{202f}\u{205f} ",
        "\u{200b}x\u{200b}",
        "a\0b",
        "\"quoted\"",
        "line\nbreak",
    ]
    .iter()
    .map(|t| (*t).to_string())
    .collect();
    // Numbers as JavaScript and as people write them.
    for v in doubles(400) {
        texts.push(format!("{v}"));
        texts.push(format!("{v:e}"));
        texts.push(format!(" {v:.3} "));
        texts.push(format!("{v:+}"));
    }
    let expressions: Vec<String> = [
        "+t",
        "-t",
        "t * 1",
        "t - 0",
        "t / 2",
        "t | 0",
        "t >>> 0",
        "~t",
        "Number(t)",
        "t == 0",
        "t == n",
        "t === \"12\"",
        "t < 1",
        "t >= n",
        "t > \"1\"",
        "t < \"a\"",
        "isNaN(t)",
        "isFinite(t)",
        "Math.floor(t)",
        "Math.max(t, n)",
        "!t",
        "t ? 1 : 2",
        "t || \"empty\"",
        "t + n",
        "n + t",
        "t + t",
        "`<${t}>`",
        "String(t)",
        "t.toString()",
        "typeof t",
        "t.length",
        "t.trim()",
        "t.trim().length",
        "t.toUpperCase()",
        "t.toLowerCase()",
        "t[0]",
        "t[1]",
        "t[n]",
        "t[t.length - 1]",
        "\"%1|%2\".arg(t).arg(n)",
        "\"%1\".arg(t).arg(n)",
        "t.arg(n)",
        "t.arg(\"x\").arg(\"y\")",
        "t.arg(t)",
        "t.arg(n / 8)",
        "[t, n]",
        "twice(t)",
        "Qt.point(t, n)",
        "qsTr(\"Size: %1\").arg(t)",
    ]
    .iter()
    .map(|e| (*e).to_string())
    .collect();
    sweep(
        "    property string t: \"\"\n    property int n: 1\n",
        &expressions,
        texts.len(),
        60,
        |w, round| {
            w.h.set(w.root, "t", Value::string(texts[round].as_str()));
            w.h.set(w.root, "n", Value::Int((round % 7) as i32 - 2));
        },
    );
}

// ------------------------------------------------------------ properties

const NAMES: &[&str] = &[
    "i",
    "j",
    "zero",
    "r",
    "big",
    "tiny",
    "yes",
    "no",
    "s",
    "empty",
    "digits",
    "undef",
    "nothing",
    "pt.x",
    "sz.width",
    "rc.height",
    "child.x",
    "other.width",
    "list.length",
    "list[0]",
    "list[1]",
    "border.width",
    "color.r",
    "width",
    "s.length",
];

const NUMBERS: &[&str] = &[
    "0",
    "1",
    "2",
    "0.5",
    "0.1",
    "(-1)",
    "(-0)",
    "(0/0)",
    "(1/0)",
    "(-1/0)",
    "1e21",
    "1e-7",
    "2147483647",
    "2147483648",
    "4294967295",
    "9007199254740993",
    "1.005",
    "(-2.5)",
    "255",
    "31",
    "32",
    "5e-324",
    "1.7976931348623157e308",
];

const STRINGS: &[&str] = &[
    "\"\"",
    "\"a\"",
    "\"12\"",
    "\" 3 \"",
    "\"1e3\"",
    "\"0x10\"",
    "\"Infinity\"",
    "\"-0\"",
    "\"x y\"",
    "\"true\"",
    "\"null\"",
    "\"é\"",
    "\"1,2\"",
    "\"NaN\"",
    "\"%1 and %2\"",
];

const UNARY: &[&str] = &["-", "+", "!", "~", "typeof ", "void "];

const BINARY: &[&str] = &[
    "+", "+", "+", "-", "*", "/", "%", "**", "==", "!=", "===", "!==", "<", "<=", ">", ">=", "&",
    "|", "^", "<<", ">>", ">>>", "&&", "&&", "||", "||", "??", ",",
];

const CALLS: &[&str] = &[
    "Math.abs",
    "Math.acos",
    "Math.asin",
    "Math.atan",
    "Math.cbrt",
    "Math.ceil",
    "Math.cos",
    "Math.exp",
    "Math.floor",
    "Math.log",
    "Math.log10",
    "Math.log2",
    "Math.round",
    "Math.sign",
    "Math.sin",
    "Math.sqrt",
    "Math.tan",
    "Math.trunc",
    "Number",
    "String",
    "Boolean",
    "isNaN",
    "isFinite",
    "twice",
    "grade",
];

const CALLS_2: &[&str] = &[
    "Math.max",
    "Math.min",
    "Math.pow",
    "Math.atan2",
    "Qt.point",
    "Qt.size",
    "qsTranslate",
];

fn pick(of: &'static [&'static str]) -> impl Strategy<Value = String> {
    prop::sample::select(of).prop_map(str::to_string)
}

fn leaf() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => pick(NAMES),
        2 => pick(NUMBERS),
        2 => pick(STRINGS),
        1 => (-40i32..400).prop_map(|n| format!("({n})")),
        1 => pick(&["true", "false", "null", "undefined"]),
    ]
}

/// An expression of the subset, with now and then something outside it
/// (an array where a number is wanted, a method on the wrong receiver):
/// tier 1 must give those up, not get them wrong.
fn expression() -> impl Strategy<Value = String> {
    leaf().prop_recursive(5, 40, 3, |inner| {
        prop_oneof![
            2 => (pick(UNARY), inner.clone()).prop_map(|(op, a)| format!("({op}({a}))")),
            8 => (inner.clone(), pick(BINARY), inner.clone())
                .prop_map(|(a, op, b)| format!("(({a}) {op} ({b}))")),
            2 => (inner.clone(), inner.clone(), inner.clone())
                .prop_map(|(c, a, b)| format!("(({c}) ? ({a}) : ({b}))")),
            3 => (pick(CALLS), inner.clone()).prop_map(|(f, a)| format!("{f}({a})")),
            2 => (pick(CALLS_2), inner.clone(), inner.clone())
                .prop_map(|(f, a, b)| format!("{f}({a}, {b})")),
            1 => (inner.clone(), inner.clone(), inner.clone())
                .prop_map(|(c, a, b)| format!("pick({c}, {a}, {b})")),
            1 => (inner.clone(), inner.clone()).prop_map(|(a, b)| format!("`${{{a}}} and ${{{b}}}`")),
            1 => (inner.clone(), 0u8..5).prop_map(|(a, d)| format!("Number({a}).toFixed({d})")),
            1 => (inner.clone(), inner.clone()).prop_map(|(a, b)| format!("String({a}).arg({b})")),
            1 => (inner.clone(), inner.clone(), inner.clone())
                .prop_map(|(a, b, c)| format!("Qt.rgba({a}, {b}, {c}, 1)")),
            1 => (inner.clone(), inner.clone()).prop_map(|(a, b)| format!("[{a}, {b}]")),
            1 => inner.clone().prop_map(|a| format!("list[{a}]")),
            1 => inner.clone().prop_map(|a| format!("s[{a}]")),
            1 => inner.clone().prop_map(|a| format!("String({a}).trim().toUpperCase()")),
        ]
    })
}

#[test]
fn random_expressions_agree_in_value_and_dependencies() {
    let ran = Cell::new(0u32);
    let total = Cell::new(0u32);
    // `PROPTEST_CASES` sets how many worlds of eight expressions.
    let mut runner = TestRunner::new(Config {
        failure_persistence: None,
        ..Config::default()
    });
    let outcome = runner.run(&prop::collection::vec(expression(), 8), |expressions| {
        let w = World::with(&expressions);
        for (n, expression) in expressions.iter().enumerate() {
            total.set(total.get() + 1);
            match w.compare(w.root, &format!("o{n}")) {
                Ok(true) => ran.set(ran.get() + 1),
                Ok(false) => {}
                Err(why) => return Err(TestCaseError::fail(format!("`{expression}`: {why}"))),
            }
        }
        Ok(())
    });
    if let Err(e) = outcome {
        panic!("{e}");
    }
    println!("{} of {} ran in tier 1", ran.get(), total.get());
    // Not vacuous: most of what is generated runs in tier 1 to the end.
    assert!(
        ran.get() * 2 > total.get(),
        "only {} of {} ran in tier 1",
        ran.get(),
        total.get()
    );
}

// --------------------------------------------------------------- timings

/// `count` rectangles, five bindings each, all reading `root.t`.
fn bench_file(count: usize) -> String {
    let mut s = String::from(
        "import QtQuick\nItem {\n    id: root\n    width: 800\n    property real t: 0\n    property string unit: \"px\"\n",
    );
    for i in 0..count {
        s.push_str(&format!(
            "    Rectangle {{\n        x: root.t * 2 + {i}\n        y: root.t > 5 && root.t < 50 ? {i} : -{i}\n        width: Math.max(root.t, 10) + parent.width / 4\n        height: (root.t + {i}) % 7\n        radius: root.t * root.t - {i} / 3\n    }}\n"
        ));
    }
    s.push_str("}\n");
    s
}

/// Seconds per evaluation of `props` of every child of a file, `t`
/// changing between rounds.
fn time_bindings(source: &str, props: &[&str], tier1: bool, rounds: u32) -> (f64, u64) {
    let h = engine();
    h.engine.set_tier1(tier1);
    h.engine.set_tier1_check(false);
    let root = h.load(source);
    let e = &h.engine;
    let kids = e.visual_children(root);
    let t = e.property_index(root, "t").expect("t");
    let index: Vec<_> = props
        .iter()
        .map(|p| e.property_index(kids[0], p).expect("a property"))
        .collect();
    let round = |n: u32| {
        e.write(
            root,
            t,
            Value::Real(f64::from(n)),
            ljos_engine::WriteKind::Imperative,
        )
        .expect("write");
        for k in &kids {
            for p in &index {
                std::hint::black_box(e.read(*k, *p));
            }
        }
    };
    for n in 1..=3 {
        round(n);
    }
    let before = e.tier1_stats().runs;
    let start = Instant::now();
    for n in 0..rounds {
        round(n + 4);
    }
    let elapsed = start.elapsed().as_secs_f64();
    let evaluations = u64::from(rounds) * kids.len() as u64 * index.len() as u64;
    assert!(h.warnings().is_empty());
    (elapsed / evaluations as f64, e.tier1_stats().runs - before)
}

/// Seconds to create a file already compiled and read `props` of every
/// child once: what a binding costs the first time, the program built from
/// the IR and (in tier 2) the function compiled by the JS engine.
fn time_creation(source: &str, props: &[&str], tier1: bool) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..15 {
        // A new engine each time: nothing of the file is warm.
        let h = engine();
        h.engine.set_tier1(tier1);
        h.engine.set_tier1_check(false);
        h.files(&[("Main", source)]);
        let e = &h.engine;
        let start = Instant::now();
        let root = e
            .create_from_url("Main", None, Vec::new())
            .expect("Main.qml creates");
        for k in e.visual_children(root) {
            for p in props {
                std::hint::black_box(e.read_by_name(k, p));
            }
        }
        best = best.min(start.elapsed().as_secs_f64());
        assert!(h.warnings().is_empty());
    }
    best
}

fn report(what: &str, source: &str, props: &[&str], rounds: u32) {
    let (one, runs) = time_bindings(source, props, true, rounds);
    let (two, none) = time_bindings(source, props, false, rounds);
    assert_eq!(none, 0);
    println!(
        "{what}: tier 1 {:.0} ns ({:.2} M/s, {runs} runs), tier 2 {:.0} ns ({:.2} M/s), {:.1}x",
        one * 1e9,
        1e-6 / one,
        two * 1e9,
        1e-6 / two,
        two / one
    );
}

#[test]
#[ignore]
fn bench_binding_evaluations_per_second() {
    report(
        "five mixed bindings",
        &bench_file(200),
        &["x", "y", "width", "height", "radius"],
        200,
    );

    let props = ["x", "y", "width", "height", "radius"];
    for count in [200, 2000] {
        let file = bench_file(count);
        let one = time_creation(&file, &props, true);
        let two = time_creation(&file, &props, false);
        println!(
            "creating {count} rectangles of five bindings: tier 1 {:.2} ms, tier 2 {:.2} ms, {:.1}x",
            one * 1e3,
            two * 1e3,
            two / one
        );
    }

    // One operator against forty: what an operator costs beside what an
    // evaluation costs whatever it does.
    let chain = |ops: usize| {
        let mut e = String::from("root.t");
        for k in 0..ops {
            e = match k % 4 {
                0 => format!("({e} + {k})"),
                1 => format!("({e} * 1.01)"),
                2 => format!("({e} - {k})"),
                _ => format!("({e} / 1.01)"),
            };
        }
        let mut s = String::from("import QtQuick\nItem {\n    id: root\n    property real t: 0\n");
        for _ in 0..200 {
            s.push_str(&format!("    Item {{ x: {e} }}\n"));
        }
        s.push_str("}\n");
        s
    };
    // Tier 0 in both: what an evaluation costs before any operator.
    report("a bare path", &chain(0), &["x"], 500);
    report("1 operator", &chain(1), &["x"], 500);
    report("40 operators", &chain(40), &["x"], 500);

    let mut s = String::from(
        "import QtQuick\nItem {\n    id: root\n    property real t: 0\n    property string unit: \"px\"\n",
    );
    for i in 0..200 {
        s.push_str(&format!(
            "    Item {{\n        property string a: root.t + \" \" + root.unit\n        property string b: `${{root.t.toFixed(1)}}/{i}`\n        property string c: \"%1 of %2\".arg(root.t).arg({i})\n    }}\n"
        ));
    }
    s.push_str("}\n");
    report("three string bindings", &s, &["a", "b", "c"], 200);
}

#[test]
fn an_array_passed_to_a_function_holds_what_it_held() {
    // The function is given the caller's array, not one read back from a
    // property, so a `-0` in it is still one.
    exact(&[
        "[other.width, (-0)]",
        "pick(yes, [other.width, (-0)], 1)",
        "pick(no, 1, [[-0], -0])",
    ]);
}

#[test]
fn an_array_a_script_changed_is_read_as_it_is_now() {
    let w = World::new(
        "    property var xs: [1, 2]\n    property int count: xs.length\n    property var first: xs[0]\n    property var last: xs[xs.length - 1]\n    function grow() { xs.push(7); xs[0] = -0; xsChanged() }\n",
        "",
    );
    for p in ["count", "first", "last"] {
        assert_eq!(w.compare(w.root, p), Ok(true), "{p}");
    }
    w.h.engine.invoke_by_name(w.root, "grow", &[]).unwrap();
    assert_eq!(w.h.num(w.root, "count"), 3.0);
    assert_eq!(w.h.num(w.root, "last"), 7.0);
    // Once a script has the array, tier 1 reads what the script reads.
    for p in ["count", "first", "last"] {
        assert_eq!(w.compare(w.root, p), Ok(true), "{p}");
    }
    // An array inside the array is the script's to hand out.
    let w = World::new(
        "    property var xs: [[1], 2]\n    property var inner: xs[0]\n",
        "",
    );
    assert_eq!(w.compare(w.root, "inner"), Ok(false));
}

/// The interrupt that ends a runaway handler can come while tier 2 checks
/// a binding the handler reads: that binding did not throw.
#[test]
fn a_check_the_interrupt_cuts_off_is_not_a_difference() {
    let h = engine();
    h.engine.set_tier1_check(true);
    let root = h.load(
        r#"import QtQuick
Item {
    property int clicks: 0
    property int floor: Math.max(clicks, 1) + Math.max(clicks, 2) + Math.max(clicks, 3) + Math.max(clicks, 4)
        + Math.max(clicks, 5) + Math.max(clicks, 6) + Math.max(clicks, 7) + Math.max(clicks, 8)
    function spin() { var n = 0; while (true) { clicks += 1; n += floor } }
}
"#,
    );
    let _ = h.engine.invoke_by_name(root, "spin", &[]);
    let warnings = h.warnings();
    assert!(
        warnings.iter().any(|w| w.contains("interrupted")),
        "{warnings:?}"
    );
    assert!(
        !warnings
            .iter()
            .any(|w| w.contains("tier 1 and tier 2 differ")),
        "{warnings:?}"
    );
}
