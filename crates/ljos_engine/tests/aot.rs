//! The cross-check of compiled code (ARCHITECTURE.md §5.4, the amendment
//! for shipped files) against code that is wrong on purpose: it has to see
//! every way compiled code can differ from the dynamic tiers, and nothing
//! where the two agree.
//!
//! The code here is written by hand, as `ljos_aot` would generate it, and
//! registered for the content of the file the harness compiles.

mod support;

use std::cell::Cell;

use ljos_engine::aot::{Code, File, Frame, Kind, PropSite, Stop, Value};
use ljos_engine::{Engine, ObjectId};
use support::{Harness, engine};

const MAIN: &str = r#"import QtQuick
Item {
    id: root
    property int count: 0
    property real r: 0
    property int a: 0
    property int b: 0
    property string order: ""
    property real z: count * 0
    property real sum: a + b * 0
    property real twice: count * 2
    signal ping(int n)
    onPing: (n) => { order = order + "p" + n }
    onRChanged: ping(r)
    onCountChanged: { a = count; b = count }
    function go() { count = 5; count = 6; return "made by no createObject" }
}
"#;

/// The properties the code names, by site.
const COUNT: usize = 0;
const A: usize = 1;
const A_SET: usize = 2;
const R: usize = 3;
const COUNT_SET: usize = 4;
const B: usize = 5;

static PROPS: &[PropSite] = &[
    PropSite {
        name: "count",
        kind: Kind::Number,
        write: false,
    },
    PropSite {
        name: "a",
        kind: Kind::Number,
        write: false,
    },
    PropSite {
        name: "a",
        kind: Kind::Number,
        write: true,
    },
    PropSite {
        name: "r",
        kind: Kind::Number,
        write: false,
    },
    PropSite {
        name: "count",
        kind: Kind::Number,
        write: true,
    },
    PropSite {
        name: "b",
        kind: Kind::Number,
        write: false,
    },
];

/// The code of the expression on `line` of the file `key`, the rest
/// running dynamically, registered for the file as the harness compiles
/// it.
fn register(h: &Harness, key: &str, code: &[(u32, Code)]) -> Result<(), ljos_engine::EngineError> {
    register_sites(h, key, PROPS, code)
}

fn register_sites(
    h: &Harness,
    key: &str,
    props: &'static [PropSite],
    code: &[(u32, Code)],
) -> Result<(), ljos_engine::EngineError> {
    let file = h.engine.component_source().unwrap().load(key).unwrap();
    let mut exprs: Vec<Option<Code>> = vec![None; file.exprs.len()];
    for (line, c) in code {
        let at = file
            .exprs
            .iter()
            .position(|b| b.span.line == *line)
            .unwrap_or_else(|| panic!("no expression on line {line}"));
        exprs[at] = Some(*c);
    }
    let compiled: &'static File = Box::leak(Box::new(File {
        name: "Main.qml",
        content: ljos_ir::aot::content_key(&file),
        context: "Main",
        texts: &[],
        props,
        signals: &["ping"],
        singletons: &[],
        methods: &[],
        exprs: Box::leak(exprs.into_boxed_slice()),
    }));
    h.engine.register_compiled(Box::leak(Box::new([compiled])))
}

fn made(h: &Harness) -> ObjectId {
    h.engine
        .create_from_url("Main", None, Vec::new())
        .expect("Main.qml creates")
}

fn world(code: &[(u32, Code)]) -> (Harness, ObjectId) {
    let h = engine();
    h.engine.set_aot_check(true);
    h.files(&[("Main", MAIN)]);
    register(&h, "Main", code).unwrap();
    let root = made(&h);
    (h, root)
}

fn differ(e: &Engine, warnings: &[String]) -> usize {
    let said = warnings
        .iter()
        .filter(|w| w.contains("compiled code and the dynamic tiers differ"))
        .count();
    assert_eq!(said as u64, e.aot_stats().mismatches, "{warnings:?}");
    said
}

/// `onCountChanged`, without the write of `b`.
fn drops_a_write(f: &Frame<'_>) -> Result<Value, Stop> {
    let o = f.object(0, 0)?;
    let count = f.prop(o, COUNT)?;
    let a = f.prop(o, A_SET)?;
    let n = f.num(o, count)?;
    f.set(o, a, Value::Real(n))?;
    Ok(Value::Undefined)
}

#[test]
fn a_handler_that_leaves_out_a_write_is_a_mismatch() {
    let (h, root) = world(&[(15, drops_a_write)]);
    h.set(root, "count", Value::Int(1));
    assert_eq!(differ(&h.engine, &h.warnings()), 1);
    // The dynamic tiers made the effects.
    assert_eq!(h.num(root, "b"), 1.0);
}

/// `sum: a + b * 0`, without the read of `b`: the same value, and no
/// dependency on `b`.
fn misses_a_read(f: &Frame<'_>) -> Result<Value, Stop> {
    let o = f.object(0, 0)?;
    let a = f.prop(o, A)?;
    Ok(Value::Real(f.num(o, a)?))
}

#[test]
fn a_binding_that_does_not_read_what_it_depends_on_is_a_mismatch() {
    let (h, root) = world(&[(10, misses_a_read)]);
    // Bindings run when read.
    assert_eq!(h.num(root, "sum"), 0.0);
    assert_eq!(differ(&h.engine, &h.warnings()), 1);
}

/// `sum: a + b * 0`, as it should be.
fn reads_both(f: &Frame<'_>) -> Result<Value, Stop> {
    let o = f.object(0, 0)?;
    let (a, b) = (f.prop(o, A)?, f.prop(o, B)?);
    let a = f.num(o, a)?;
    let b = f.num(o, b)?;
    Ok(Value::Real(a + b * 0.0))
}

#[test]
fn a_binding_that_reads_what_the_script_reads_agrees() {
    let (h, root) = world(&[(10, reads_both)]);
    assert_eq!(h.num(root, "sum"), 0.0);
    h.set(root, "a", Value::Int(3));
    assert_eq!(h.num(root, "sum"), 3.0);
    assert_eq!(differ(&h.engine, &h.warnings()), 0);
    assert!(
        h.engine.aot_stats().checked >= 2,
        "{:?}",
        h.engine.aot_stats()
    );
}

/// `z: count * 0`, giving `-0`.
fn negative_zero(f: &Frame<'_>) -> Result<Value, Stop> {
    let o = f.object(0, 0)?;
    let count = f.prop(o, COUNT)?;
    Ok(Value::Real(-(f.num(o, count)? * 0.0)))
}

#[test]
fn minus_zero_for_zero_is_a_mismatch() {
    let (h, root) = world(&[(9, negative_zero)]);
    assert_eq!(h.num(root, "z"), 0.0);
    assert_eq!(differ(&h.engine, &h.warnings()), 1);
}

/// `onRChanged: ping(r)`, emitting the number as it is: the engine makes
/// it the `int` the signal declares.
fn emits_a_real(f: &Frame<'_>) -> Result<Value, Stop> {
    let o = f.object(0, 0)?;
    let r = f.prop(o, R)?;
    let ping = f.signal(o, 0)?;
    let n = f.num(o, r)?;
    f.emit(o, ping, &[Value::Real(n)])?;
    Ok(Value::Undefined)
}

#[test]
fn a_signal_argument_is_compared_as_the_signal_declares_it() {
    let (h, root) = world(&[(14, emits_a_real)]);
    h.set(root, "r", Value::Real(2.5));
    assert_eq!(differ(&h.engine, &h.warnings()), 0);
    assert_eq!(h.get(root, "order"), Value::from("p2"));
    assert_eq!(h.engine.aot_stats().checked, 1);
}

/// `go()`, failing after its first write. A script calls a function that
/// names a way to make objects through the engine, which runs its compiled
/// code: here the name is in a text.
fn throws_after_a_write(f: &Frame<'_>) -> Result<Value, Stop> {
    let o = f.object(0, 0)?;
    let count = f.prop(o, COUNT_SET)?;
    f.set(o, count, Value::Real(5.0))?;
    Err(Stop::Thrown)
}

const CALLER: &str = r#"import QtQuick
Item {
    property Item target
    function tryGo() { try { target.go() } catch (e) { target.order = "caught" } }
}
"#;

#[test]
fn a_compiled_function_that_throws_throws_to_the_script_that_called_it() {
    let h = engine();
    h.engine.set_aot_check(false);
    h.files(&[("Main", MAIN), ("Caller", CALLER)]);
    register(&h, "Main", &[(16, throws_after_a_write)]).unwrap();
    let root = made(&h);
    let caller = h
        .engine
        .create_from_url("Caller", None, Vec::new())
        .unwrap();
    h.set(caller, "target", Value::Object(root));
    h.engine.invoke_by_name(caller, "tryGo", &[]).unwrap();
    assert_eq!(h.engine.aot_stats().thrown, 1);
    assert_eq!(h.num(root, "count"), 5.0);
    assert_eq!(h.get(root, "order"), Value::from("caught"));
}

thread_local! {
    static RAN: Cell<u32> = const { Cell::new(0) };
}

/// `twice: count * 2`, counting its runs.
fn counted(f: &Frame<'_>) -> Result<Value, Stop> {
    RAN.with(|r| r.set(r.get() + 1));
    let o = f.object(0, 0)?;
    let count = f.prop(o, COUNT)?;
    Ok(Value::Real(f.num(o, count)? * 2.0))
}

#[test]
fn with_both_checks_on_compiled_code_runs_once_an_evaluation() {
    let h = engine();
    h.engine.set_aot_check(true);
    h.engine.set_tier1_check(true);
    h.files(&[("Main", MAIN)]);
    register(&h, "Main", &[(11, counted)]).unwrap();
    RAN.with(|r| r.set(0));
    let root = made(&h);
    assert_eq!(h.num(root, "twice"), 0.0);
    assert_eq!(RAN.with(Cell::get), 1);
    let stats = h.engine.aot_stats();
    assert_eq!((stats.runs, stats.checked), (1, 1), "{stats:?}");
    assert_eq!(differ(&h.engine, &h.warnings()), 0);
}

#[test]
fn code_for_a_file_that_already_ran_is_refused_and_says_why() {
    let h = engine();
    h.files(&[("Main", MAIN)]);
    register(&h, "Main", &[(11, counted)]).unwrap();
    let root = made(&h);
    assert_eq!(h.num(root, "twice"), 0.0);
    let err = register(&h, "Main", &[(9, negative_zero)]).unwrap_err();
    assert!(err.to_string().contains("Main.qml"), "{err}");
    assert!(err.to_string().contains("already"), "{err}");
}

const HOLDER: &str = r#"import QtQuick
Item {
    id: root
    property int count: 0
    property alias av: inner.v
    Item { id: inner; objectName: "inner"; property int v: 0 }
    onCountChanged: av = count
}
"#;

static ALIAS_PROPS: &[PropSite] = &[
    PropSite {
        name: "count",
        kind: Kind::Number,
        write: false,
    },
    PropSite {
        name: "v",
        kind: Kind::Number,
        write: true,
    },
];

/// `onCountChanged: av = count`, writing what the alias stands for: the
/// script writes the alias.
fn writes_through_an_alias(f: &Frame<'_>) -> Result<Value, Stop> {
    let root = f.object(0, 0)?;
    let inner = f.object(0, 1)?;
    let count = f.prop(root, 0)?;
    let v = f.prop(inner, 1)?;
    let n = f.num(root, count)?;
    f.set(inner, v, Value::Real(n))?;
    Ok(Value::Undefined)
}

#[test]
fn a_write_through_an_alias_is_compared_as_the_script_makes_it() {
    let h = engine();
    h.engine.set_aot_check(true);
    h.files(&[("Main", HOLDER)]);
    let file = h.engine.component_source().unwrap().load("Main").unwrap();
    let inner = file.components[0]
        .objects
        .iter()
        .position(|o| o.id.as_deref() == Some("inner"));
    assert_eq!(inner, Some(1));
    let mut exprs: Vec<Option<Code>> = vec![None; file.exprs.len()];
    let at = file.exprs.iter().position(|b| b.span.line == 7).unwrap();
    exprs[at] = Some(writes_through_an_alias);
    let compiled: &'static File = Box::leak(Box::new(File {
        name: "Main.qml",
        content: ljos_ir::aot::content_key(&file),
        context: "Main",
        texts: &[],
        props: ALIAS_PROPS,
        signals: &[],
        singletons: &[],
        methods: &[],
        exprs: Box::leak(exprs.into_boxed_slice()),
    }));
    h.engine
        .register_compiled(Box::leak(Box::new([compiled])))
        .unwrap();
    let root = made(&h);
    h.set(root, "count", Value::Int(4));
    let inner = h.find(root, "inner");
    assert_eq!(h.num(inner, "v"), 4.0);
    let stats = h.engine.aot_stats();
    assert_eq!(stats.checked, 1, "{stats:?}");
    assert_eq!(differ(&h.engine, &h.warnings()), 0);
}

const SHUT: &str = r#"import QtQuick
Item {
    id: root
    property int count: 0
    property int a: 0
    Item { id: inner; objectName: "inner"; property int v: 0 }
    onAChanged: inner.destroy()
    onCountChanged: { a = count; inner.v = count }
}
"#;

static SHUT_PROPS: &[PropSite] = &[
    PropSite {
        name: "count",
        kind: Kind::Number,
        write: false,
    },
    PropSite {
        name: "a",
        kind: Kind::Number,
        write: true,
    },
    PropSite {
        name: "v",
        kind: Kind::Number,
        write: true,
    },
];

/// `onCountChanged: { a = count; inner.v = count }`, `inner` looked up
/// before the write of `a` has its handler destroy it.
fn writes_after_a_destroy(f: &Frame<'_>) -> Result<Value, Stop> {
    let root = f.object(0, 0)?;
    let inner = f.object(0, 1)?;
    let count = f.prop(root, 0)?;
    let a = f.prop(root, 1)?;
    let v = f.prop(inner, 2)?;
    let n = f.num(root, count)?;
    f.set(root, a, Value::Real(n))?;
    f.set(inner, v, Value::Real(n))?;
    Ok(Value::Undefined)
}

// What compiled code looks up before its first effect stays what a
// script would find after it: a script destroys an object later, never
// in the middle of a handler.
#[test]
fn an_object_looked_up_before_an_effect_that_destroys_it_is_still_there() {
    for check in [true, false] {
        let h = engine();
        h.engine.set_aot_check(check);
        h.files(&[("Main", SHUT)]);
        register_sites(&h, "Main", SHUT_PROPS, &[(8, writes_after_a_destroy)]).unwrap();
        let root = made(&h);
        let inner = h.find(root, "inner");
        h.set(root, "count", Value::Int(3));
        assert_eq!(h.num(inner, "v"), 3.0, "check {check}");
        let stats = h.engine.aot_stats();
        assert_eq!(stats.runs, 1, "check {check}: {stats:?}");
        assert_eq!(stats.checked, u64::from(check), "{stats:?}");
        assert_eq!(differ(&h.engine, &h.warnings()), 0);
        assert!(h.engine.is_alive(inner));
        h.engine.process_deferred_deletes();
        assert!(!h.engine.is_alive(inner));
    }
}
