//! What a script makes while the program runs: `Qt.createComponent`,
//! `Qt.createQmlObject` and the initial properties of `createObject`.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use ljos_compiler::{Project, Severity, compile_text};
use ljos_engine::{QmlText, QmlTextCompiler, QmlTextError, Value};
use qml_front::QtTypeOracle;
use support::{Harness, engine};

const TAG: &str = r#"import QtQuick
Item {
    objectName: "tag"
    property int n: 1
    property string label: "own"
    property Item other: null
}
"#;

const MAKER: &str = r#"import QtQuick
Item {
    id: root
    property int base: 5
    property var comp: null
    property var kid: null
    property string caught: ""
    property var errors: []
    function load(url) { comp = Qt.createComponent(url) }
    function make(props) { kid = comp.createObject(root, props) }
    function follow() { kid = comp.createObject(root, { n: Qt.binding(() => root.base * 2) }) }
    function text(t, where) {
        try {
            kid = where === undefined ? Qt.createQmlObject(t, root) : Qt.createQmlObject(t, root, where)
        } catch (e) {
            caught = e.name + ": " + e.message
            errors = e.qmlErrors || []
        }
    }
    function resolved(url) { return Qt.resolvedUrl(url) }
}
"#;

const FILES: &[(&str, &str)] = &[
    ("Tag", TAG),
    ("sub/Tag", "import QtQuick\nItem { property int n: 2 }\n"),
    ("Main", MAKER),
];

fn made() -> (Harness, ljos_engine::ObjectId) {
    let h = engine();
    h.files(FILES);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    (h, root)
}

/// What a development host installs: the compiler, over the same files.
fn compiler() -> QmlTextCompiler {
    let mut project = Project::new(QtTypeOracle);
    for (key, source) in FILES {
        project.add(key, *source);
    }
    let project = RefCell::new(project);
    Rc::new(move |text: QmlText<'_>| {
        let errors = |d: &[ljos_compiler::Diagnostic]| {
            d.iter()
                .filter(|d| d.severity == Severity::Error)
                .map(|d| QmlTextError {
                    line: d.span.line,
                    column: d.span.column,
                    message: d.message.clone(),
                })
                .collect::<Vec<_>>()
        };
        match compile_text(&mut project.borrow_mut(), text.key, text.text) {
            Ok(c) if !c.has_errors() => Ok(c.file),
            Ok(c) => Err(errors(&c.diagnostics)),
            Err(d) => Err(errors(&d)),
        }
    })
}

fn string(h: &Harness, obj: ljos_engine::ObjectId, prop: &str) -> String {
    h.get(obj, prop).to_js_string().to_string()
}

fn object(h: &Harness, obj: ljos_engine::ObjectId, prop: &str) -> ljos_engine::ObjectId {
    h.get(obj, prop)
        .as_object()
        .unwrap_or_else(|| panic!("{prop} holds no object"))
}

fn call(h: &Harness, obj: ljos_engine::ObjectId, method: &str, args: &[&str]) -> Value {
    let args: Vec<Value> = args.iter().map(|a| Value::String((*a).into())).collect();
    h.engine.invoke_by_name(obj, method, &args).unwrap()
}

#[test]
fn a_component_made_from_a_file_is_ready_and_makes_objects() {
    let (h, root) = made();
    call(&h, root, "load", &["Tag.qml"]);
    let comp = object(&h, root, "comp");
    assert_eq!(h.num(comp, "status"), 1.0);
    assert_eq!(string(&h, comp, "url"), "file:///Tag.qml");
    assert_eq!(
        h.engine.invoke_by_name(comp, "errorString", &[]).unwrap(),
        Value::String("".into())
    );
    h.engine.invoke_by_name(root, "make", &[]).unwrap();
    let kid = object(&h, root, "kid");
    assert_eq!(h.num(kid, "n"), 1.0);
    assert_eq!(h.engine.children(root), vec![kid]);
    assert!(h.warnings().is_empty());
}

#[test]
fn a_file_that_is_not_there_is_an_error_component() {
    let (h, root) = made();
    call(&h, root, "load", &["Nope.qml"]);
    let comp = object(&h, root, "comp");
    assert_eq!(h.num(comp, "status"), 3.0);
    assert_eq!(
        h.engine.invoke_by_name(comp, "errorString", &[]).unwrap(),
        Value::String("file:///Nope.qml: No such file or directory\n".into())
    );
    h.engine.invoke_by_name(root, "make", &[]).unwrap();
    assert_eq!(h.get(root, "kid"), Value::Null);
    assert_eq!(h.warnings(), vec!["QQmlComponent: Component is not ready"]);
}

#[test]
fn an_address_is_read_from_the_file_whose_script_asks() {
    let h = engine();
    h.files(&[
        ("Tag", TAG),
        ("sub/Tag", "import QtQuick\nItem { property int n: 2 }\n"),
        ("sub/Maker", MAKER),
        ("Main", "import QtQuick\nimport \"sub\"\nMaker { }\n"),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    call(&h, root, "load", &["Tag.qml"]);
    h.engine.invoke_by_name(root, "make", &[]).unwrap();
    assert_eq!(
        h.num(object(&h, root, "kid"), "n"),
        2.0,
        "the Tag beside Maker"
    );
    assert_eq!(
        call(&h, root, "resolved", &["../Tag.qml"])
            .to_js_string()
            .as_str(),
        "file:///Tag.qml"
    );
}

#[test]
fn initial_properties_that_cannot_be_set_are_reported_one_by_one() {
    let (h, root) = made();
    call(&h, root, "load", &["Tag.qml"]);
    let props = h
        .engine
        .eval_js("({ label: 'a', nope: 1, n: 'many', other: 5 })")
        .unwrap();
    h.engine.invoke_by_name(root, "make", &[props]).unwrap();
    let kid = object(&h, root, "kid");
    assert_eq!(string(&h, kid, "label"), "a");
    assert_eq!(h.num(kid, "n"), 1.0, "a text that is no number is not 0");
    let mut warnings = h.warnings();
    warnings.sort();
    assert_eq!(
        warnings,
        vec![
            "file:///Tag.qml: Could not set initial property n",
            "file:///Tag.qml: Could not set initial property other",
            "file:///Tag.qml: Setting initial properties failed: Tag does not have a property called nope",
        ]
    );
}

#[test]
fn an_initial_property_given_as_a_binding_follows() {
    let (h, root) = made();
    call(&h, root, "load", &["Tag.qml"]);
    h.engine.invoke_by_name(root, "follow", &[]).unwrap();
    let kid = object(&h, root, "kid");
    assert_eq!(h.num(kid, "n"), 10.0);
    h.set(root, "base", Value::Int(7));
    assert_eq!(h.num(kid, "n"), 14.0);
}

#[test]
fn text_without_a_compiler_is_an_error_the_script_catches() {
    let (h, root) = made();
    call(&h, root, "text", &["import QtQuick; Item { }"]);
    assert_eq!(
        string(&h, root, "caught"),
        "Error: Qt.createQmlObject(): this program runs compiled QML and has no compiler to make an object from text"
    );
    assert_eq!(h.get(root, "kid"), Value::Null);
}

#[test]
fn text_is_made_into_an_object_that_reads_the_names_of_the_asking_file() {
    let (h, root) = made();
    h.engine.set_qml_text_compiler(Some(compiler()));
    call(
        &h,
        root,
        "text",
        &["import QtQuick\nItem { width: root.base * 2; height: base }"],
    );
    assert_eq!(string(&h, root, "caught"), "");
    let kid = object(&h, root, "kid");
    assert_eq!(h.engine.children(root), vec![kid]);
    h.set(root, "base", Value::Int(7));
    assert_eq!(h.num(kid, "width"), 14.0);
    assert_eq!(h.num(kid, "height"), 7.0);
}

#[test]
fn the_place_named_for_a_text_is_where_its_types_are_looked_for() {
    let (h, root) = made();
    h.engine.set_qml_text_compiler(Some(compiler()));
    call(&h, root, "text", &["import QtQuick\nTag { }"]);
    assert_eq!(h.num(object(&h, root, "kid"), "n"), 1.0);
    call(
        &h,
        root,
        "text",
        &["import QtQuick\nTag { }", "sub/snippet"],
    );
    assert_eq!(h.num(object(&h, root, "kid"), "n"), 2.0);
}

#[test]
fn what_is_wrong_with_a_text_is_an_error_that_says_where() {
    let (h, root) = made();
    h.engine.set_qml_text_compiler(Some(compiler()));
    call(
        &h,
        root,
        "text",
        &["import QtQuick\nItem { nothing: 3 }", "sub/bad"],
    );
    let caught = string(&h, root, "caught");
    assert!(
        caught.starts_with(
            "Error: Qt.createQmlObject(): failed to create object: \n    file:///sub/bad:2:"
        ),
        "{caught}"
    );
    let errors = h.get(root, "errors");
    let first = h.engine.js_object_entries(&match &errors {
        Value::List(l) => l[0].clone(),
        other => panic!("qmlErrors is {other:?}"),
    });
    let field = |name: &str| {
        first
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.to_js_string().to_string())
            .unwrap_or_default()
    };
    assert_eq!(field("lineNumber"), "2");
    assert_eq!(field("fileName"), "file:///sub/bad");
    assert!(field("message").contains("nothing"));
    assert_eq!(h.get(root, "kid"), Value::Null);
    assert!(h.engine.children(root).is_empty());
}

#[test]
fn every_text_is_a_module_of_its_own() {
    let (h, root) = made();
    h.engine.set_qml_text_compiler(Some(compiler()));
    call(
        &h,
        root,
        "text",
        &["import QtQuick\nItem { width: base + 1 }", "at"],
    );
    let first = object(&h, root, "kid");
    call(
        &h,
        root,
        "text",
        &["import QtQuick\nItem { width: base + 2 }", "at"],
    );
    let second = object(&h, root, "kid");
    assert_eq!(h.num(first, "width"), 6.0);
    assert_eq!(h.num(second, "width"), 7.0);
}
