//! Singletons: a `pragma Singleton` file and a native type registered as
//! one are each one object of the engine, made when first read.

mod support;

use std::time::Duration;

use ljos_compiler::{Project, compile_file};
use ljos_engine::{Engine, MapSource, QmlObject, Value, Version, qml_impl, qml_singleton};
use qml_front::QtTypeOracle;
use support::engine;

const THEME: &str = r#"pragma Singleton
import QtQuick
QtObject {
    property int base: 4
    readonly property int twice: base * 2
    function scaled(n) { return n * base }
    function grow() { base = base + 1 }
    Component.onCompleted: console.log("theme made")
}
"#;

#[test]
fn it_is_made_when_first_read_and_once() {
    let h = engine();
    h.files(&[
        ("Theme", THEME),
        (
            "Main",
            r#"import QtQuick
Item {
    property bool on: false
    property int a: on ? Theme.base : 0
    property int b: on ? Theme.twice : 0
}
"#,
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    assert!(h.log().is_empty(), "nothing has read it yet");
    h.set(root, "on", Value::Bool(true));
    assert_eq!(h.num(root, "a"), 4.0);
    assert_eq!(h.num(root, "b"), 8.0);
    assert_eq!(h.log(), vec!["theme made"]);
    let second = h.engine.create_from_url("Main", None, vec![]).unwrap();
    h.set(second, "on", Value::Bool(true));
    assert_eq!(h.num(second, "b"), 8.0);
    assert!(h.log().is_empty(), "the second root reads the same object");
}

#[test]
fn every_file_and_the_host_see_the_one_object() {
    let h = engine();
    h.files(&[
        ("Theme", THEME),
        (
            "Reader",
            "import QtQuick\nItem { property int seen: Theme.base }\n",
        ),
        (
            "Main",
            r#"import QtQuick
Item {
    property int base: Theme.base
    property int scaled: Theme.scaled(3)
    Reader { objectName: "reader" }
    function grow() { Theme.grow() }
}
"#,
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    let reader = h.find(root, "reader");
    h.engine.invoke_by_name(root, "grow", &[]).unwrap();
    assert_eq!(h.num(root, "base"), 5.0);
    assert_eq!(h.num(root, "scaled"), 15.0);
    assert_eq!(h.num(reader, "seen"), 5.0);
    let theme = h.engine.singleton_by_name("", "Theme").unwrap();
    assert_eq!(h.num(theme, "base"), 5.0);
}

/// Reads of a singleton's properties are tier 0 or tier 1, and so is a
/// call of a function of it that writes nothing: no JavaScript runs.
#[test]
fn reads_and_calls_of_it_run_without_javascript() {
    let h = engine();
    h.engine.set_tier1_check(false);
    h.files(&[
        ("Theme", THEME),
        (
            "Main",
            r#"import QtQuick
Item {
    property int n: 3
    property int plain: Theme.base
    property int sum: Theme.base + Theme.twice
    property int scaled: Theme.scaled(n) + 1
}
"#,
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    assert_eq!(h.num(root, "plain"), 4.0);
    assert_eq!(h.num(root, "sum"), 12.0);
    assert_eq!(h.num(root, "scaled"), 13.0);
    let before = h.engine.tier1_stats();
    assert_eq!(before.bails, 0, "{before:?}");
    h.set(root, "n", Value::Int(5));
    assert_eq!(h.num(root, "scaled"), 21.0);
    let after = h.engine.tier1_stats();
    assert_eq!(after.bails, 0, "{after:?}");
    assert!(after.runs > before.runs, "{after:?}");
    assert!(h.warnings().is_empty());
}

/// A function that writes is JavaScript's to call: the program that calls
/// it is never made.
#[test]
fn a_call_of_a_function_that_writes_is_left_to_javascript() {
    let h = engine();
    h.files(&[
        ("Theme", THEME),
        (
            "Main",
            "import QtQuick\nItem { function grow() { Theme.grow(); return Theme.base } }\n",
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    let got = h.engine.invoke_by_name(root, "grow", &[]).unwrap();
    assert_eq!(got.to_number(), 5.0);
    assert!(h.warnings().is_empty());
}

#[test]
fn the_cross_check_agrees_on_calls_into_it() {
    let h = engine();
    h.engine.set_tier1_check(true);
    h.files(&[
        ("Theme", THEME),
        (
            "Main",
            r#"import QtQuick
Item {
    property int n: 2
    property int scaled: Theme.scaled(n) * Theme.scaled(n + 1)
    property string text: "x" + Theme.scaled(-0)
}
"#,
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    assert_eq!(h.num(root, "scaled"), 96.0);
    h.set(root, "n", Value::Int(4));
    assert_eq!(h.num(root, "scaled"), 320.0);
    let stats = h.engine.tier1_stats();
    assert_eq!(stats.mismatches, 0, "{stats:?}");
    assert!(h.warnings().is_empty());
}

/// Checked with qml6: its own bindings read the object it is becoming.
#[test]
fn a_singleton_reads_itself_while_it_is_made() {
    let h = engine();
    h.files(&[
        (
            "Loop",
            r#"pragma Singleton
import QtQuick
QtObject {
    id: self
    property int base: 2
    property bool early: Loop === undefined
    property bool same: Loop === self
    property int twice: Loop.base * 2
}
"#,
        ),
        (
            "Main",
            r#"import QtQuick
Item {
    property int twice: Loop.twice
    property bool early: Loop.early
    property bool same: Loop.same
}
"#,
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    assert_eq!(h.num(root, "twice"), 4.0);
    assert_eq!(h.get(root, "early"), Value::Bool(false));
    assert_eq!(h.get(root, "same"), Value::Bool(true));
    assert!(h.warnings().is_empty());
}

#[test]
fn a_singleton_that_cannot_be_made_is_reported_once() {
    let h = engine();
    // The file is gone between the compilation and the first read.
    let mut project = Project::new(QtTypeOracle);
    project.add(
        "Broken",
        "pragma Singleton\nimport QtQuick\nQtObject { property int base: 1 }\n",
    );
    project.add(
        "Main",
        "import QtQuick\nItem { property var a: Broken.base; property var b: Broken.base }\n",
    );
    let mut only_main = MapSource::new();
    only_main.insert("Main", compile_file(&project, "Main").unwrap().file);
    h.engine.set_component_source(only_main);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    assert_eq!(h.get(root, "a"), Value::Undefined);
    let warnings = h.warnings();
    let about: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("the singleton Broken is not available"))
        .collect();
    assert_eq!(about.len(), 1, "{warnings:?}");
}

#[test]
fn it_is_made_again_when_its_file_is_another() {
    let h = engine();
    let main = "import QtQuick\nItem { property int base: Theme.base }\n";
    h.files(&[("Theme", THEME), ("Main", main)]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    assert_eq!(h.num(root, "base"), 4.0);
    let first = h.engine.singleton_by_name("", "Theme").unwrap();

    // The same files again: the object stays.
    h.files(&[("Theme", THEME), ("Main", main)]);
    assert_eq!(h.engine.singleton_by_name("", "Theme"), Some(first));

    let other = THEME.replace("base: 4", "base: 9");
    h.files(&[("Theme", &other), ("Main", main)]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    assert_eq!(h.num(root, "base"), 9.0);
    let second = h.engine.singleton_by_name("", "Theme").unwrap();
    assert_ne!(first, second);
    // The old one goes as `destroy()` in QML does, when the turn is over.
    h.engine.process_deferred_deletes();
    assert!(!h.engine.is_alive(first));
    assert!(h.engine.is_alive(second));
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Settings")]
#[qml_singleton]
pub struct Settings {
    #[qml(notify)]
    pub volume: i32,
}

#[qml_impl]
impl Settings {
    #[qml(invokable)]
    fn louder(&mut self, by: i32) -> i32 {
        self.volume + by
    }
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Other")]
#[qml_singleton(scope = "engine")]
pub struct Other {
    #[qml(notify)]
    pub n: i32,
}

fn with_settings() -> support::Harness {
    let h = engine();
    h.engine
        .register_singleton::<Settings>("Example", Version::new(1, 0));
    h
}

#[test]
fn a_native_singleton_is_one_object_of_the_engine() {
    let h = with_settings();
    let settings = h.engine.singleton::<Settings>().unwrap();
    assert_eq!(h.engine.singleton::<Settings>().unwrap(), settings);
    assert_eq!(
        h.engine.singleton_by_name("Example", "Settings"),
        Some(settings)
    );
    h.engine.set(settings, Settings::VOLUME, 7).unwrap();

    // Another engine has its own.
    let other = Engine::new();
    other.register_singleton::<Settings>("Example", Version::new(1, 0));
    let theirs = other.singleton::<Settings>().unwrap();
    assert_eq!(other.read_by_name(theirs, "volume").to_number(), 0.0);
}

#[test]
fn a_native_singleton_that_is_not_registered_is_an_error() {
    let h = engine();
    assert!(h.engine.singleton::<Other>().is_err());
}

#[test]
fn scripts_read_write_and_call_a_native_singleton() {
    let h = with_settings();
    let root = h.load(
        r#"import QtQuick
import Example
Item {
    property int volume: Settings.volume
    property int louder: Settings.louder(2)
    function turnUp() { Settings.volume = 11 }
}
"#,
    );
    assert_eq!(h.num(root, "volume"), 0.0);
    h.engine.invoke_by_name(root, "turnUp", &[]).unwrap();
    assert_eq!(h.num(root, "volume"), 11.0);
    assert_eq!(h.num(root, "louder"), 13.0);
    let settings = h.engine.singleton::<Settings>().unwrap();
    assert_eq!(h.num(settings, "volume"), 11.0);
    h.engine.set(settings, Settings::VOLUME, 3).unwrap();
    assert_eq!(h.num(root, "volume"), 3.0);
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Store", uncreatable)]
#[qml_singleton]
pub struct Store {
    #[qml(constant, readonly)]
    pub n: i32,
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Plain")]
#[qml_singleton]
pub struct Plain {
    #[qml(constant, readonly)]
    pub n: i32,
}

#[test]
fn an_uncreatable_singleton_is_still_made_by_the_engine() {
    let h = engine();
    h.engine
        .register_singleton::<Store>("Example", Version::new(1, 0));
    h.engine
        .register_singleton::<Plain>("Example", Version::new(1, 0));
    assert!(h.engine.singleton::<Plain>().is_ok());
    let store = h.engine.singleton::<Store>().unwrap();
    assert_eq!(h.engine.singleton::<Store>().unwrap(), store);

    let root = h.load(
        r#"import QtQuick
import Example
Item {
    property int n: Store.n + 1
}
"#,
    );
    assert_eq!(h.num(root, "n"), 1.0);
}

#[derive(Debug, QmlObject)]
#[qml(module = "Example", name = "Wired", uncreatable)]
#[qml_singleton]
pub struct Wired {
    #[qml(constant, readonly)]
    pub limit: i32,
}

#[test]
fn a_singleton_is_made_by_the_constructor_it_was_registered_with() {
    let h = engine();
    let made = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = made.clone();
    h.engine
        .register_singleton_with("Example", Version::new(1, 0), move |_| {
            count.set(count.get() + 1);
            Wired { limit: 40 }
        });
    assert_eq!(made.get(), 0, "made when first read, not when registered");
    let root = h.load(
        r#"import QtQuick
import Example
Item {
    property int limit: Wired.limit
}
"#,
    );
    assert_eq!(h.num(root, "limit"), 40.0);
    let wired = h.engine.singleton::<Wired>().unwrap();
    assert_eq!(h.engine.with::<Wired, _>(wired, |w| w.limit).ok(), Some(40));
    assert_eq!(made.get(), 1);
}

/// A tool's script names a singleton as the application's code does,
/// though no compiler saw it: in an object and in the global scope, a
/// file one no code has read yet, and a native one.
#[test]
fn a_tool_script_names_singletons_by_their_type() {
    let h = with_settings();
    h.files(&[
        ("ui/Theme", THEME),
        (
            "ui/Tints",
            "pragma Singleton\nimport QtQuick\nQtObject { property int deep: 9 }\n",
        ),
        (
            "ui/Main",
            "import QtQuick\nItem { property int base: Theme.base }\n",
        ),
    ]);
    let root = h.engine.create_from_url("ui/Main", None, vec![]).unwrap();
    let eval = |this, src| {
        h.engine
            .eval_js_in(this, src, Duration::from_secs(1), [4, 16, 64, 64, 1024])
            .unwrap()
    };
    assert!(eval(Some(root), "Theme.twice + base").contains("\"value\":12"));
    assert!(eval(None, "Theme.base").contains("\"value\":4"));
    assert!(eval(None, "Tints.deep").contains("\"value\":9"));
    assert!(eval(None, "Settings.louder(2)").contains("\"value\":2"));
    assert!(eval(None, "Nowhere").contains("ReferenceError"));
    // Not a type's name, so not a singleton's either.
    assert!(eval(None, "typeof base").contains("\"value\":\"undefined\""));
}
