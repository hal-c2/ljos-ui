//! A data parameter (`serde_json::Value`, `Vec<T>`, a map) reads what
//! `JSON.stringify` would write of what a script hands it: a function or
//! `undefined` field is left out, and what cannot be data is refused
//! with the method, the parameter and the path to it.

use std::collections::HashMap;

use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Sink")]
pub struct Sink {
    #[qml(skip)]
    pub seen: Vec<String>,
}

#[qml_impl]
impl Sink {
    #[qml(invokable)]
    fn json(&mut self, payload: serde_json::Value) {
        self.seen.push(format!("json {payload}"));
    }
    #[qml(invokable)]
    fn names(&mut self, names: Vec<String>) {
        self.seen.push(format!("names {names:?}"));
    }
    #[qml(invokable)]
    fn sizes(&mut self, sizes: HashMap<String, i32>) {
        let mut sizes: Vec<_> = sizes.into_iter().collect();
        sizes.sort();
        self.seen.push(format!("sizes {sizes:?}"));
    }
    #[qml(invokable)]
    fn maybe(&mut self, names: Option<Vec<String>>) {
        self.seen.push(format!("maybe {names:?}"));
    }
}

const MAIN: &str = r#"
import QtQuick
import Example
Item {
    id: root
    property alias sink: s
    Sink { id: s }
    function attempt(f) { try { f(); return "ok" } catch (e) { return "threw: " + e.message } }
    function withItem() { return attempt(function () { s.json({ item: root, n: 1 }) }) }
    function deepItem() { return attempt(function () { s.json({ rows: [{ a: 1 }, { "the item": root }] }) }) }
    function withFunction() { return attempt(function () { s.json({ f: function () {}, n: 1 }) }) }
    function functionInList() { return attempt(function () { s.json([1, function () {}, undefined]) }) }
    function withUndefinedField() { return attempt(function () { s.json({ u: undefined, n: 1 }) }) }
    function missing() { return attempt(function () { s.json() }) }
    function namesUndefined() { return attempt(function () { s.names(undefined) }) }
    function namesMissing() { return attempt(function () { s.names() }) }
    function namesNumbers() { return attempt(function () { s.names([1, 2]) }) }
    function namesOne() { return attempt(function () { s.names("a") }) }
    function sizesNotAMap() { return attempt(function () { s.sizes("big") }) }
    function sizes() { return attempt(function () { s.sizes({ a: 1, b: "2" }) }) }
    function maybeMissing() { return attempt(function () { s.maybe() }) }
    function maybeEmpty() { return attempt(function () { s.maybe([]) }) }
}
"#;

#[test]
fn a_data_parameter_takes_what_json_would_and_names_what_it_refuses() {
    let engine = ljos::engine();
    engine.register_module(ljos::module!("Example" 1.0 => [Sink]));
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let source = ljos::dev::compile_dir_with(dir.path(), &engine).unwrap();
    let root = ljos::create_root(&engine, source, "Main").unwrap();
    let sink = engine.read_by_name(root, "sink").as_object().unwrap();
    let run = |f: &str| {
        let r = engine.invoke_by_name(root, f, &[]).unwrap();
        let seen = engine.with_mut::<Sink, _>(sink, |s| s.seen.pop()).unwrap();
        (r.to_js_string().to_string(), seen.unwrap_or_default())
    };
    let ok = |s: &str| ("ok".to_owned(), s.to_owned());
    let threw = |s: &str| (format!("threw: {s}"), String::new());

    assert_eq!(
        run("withItem"),
        threw("Sink.json: payload.item is a QML object, not JSON")
    );
    assert_eq!(
        run("deepItem"),
        threw(r#"Sink.json: payload.rows[1]["the item"] is a QML object, not JSON"#)
    );
    assert_eq!(run("withFunction"), ok(r#"json {"n":1}"#));
    assert_eq!(run("functionInList"), ok("json [1,null,null]"));
    assert_eq!(run("withUndefinedField"), ok(r#"json {"n":1}"#));
    assert_eq!(run("missing"), ok("json null"));
    assert_eq!(run("namesUndefined"), ok("names []"));
    assert_eq!(run("namesMissing"), ok("names []"));
    assert_eq!(run("namesNumbers"), ok(r#"names ["1", "2"]"#));
    assert_eq!(run("namesOne"), ok(r#"names ["a"]"#));
    assert_eq!(
        run("sizesNotAMap"),
        threw("Sink.sizes: sizes is a string, not a map")
    );
    assert_eq!(run("sizes"), ok(r#"sizes [("a", 1), ("b", 2)]"#));
    assert_eq!(run("maybeMissing"), ok("maybe None"));
    assert_eq!(run("maybeEmpty"), ok("maybe Some([])"));
}
