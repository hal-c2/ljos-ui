//! Records and typed collections between Rust and QML: a Rust type
//! gives QML a list of records and reads a script's nested object back,
//! with no JS source made in between.

use std::collections::HashMap;

use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Bridge")]
pub struct Bridge {
    #[qml(notify)]
    pub tags: Vec<String>,
    #[qml(notify)]
    pub title: Option<String>,
    #[qml(notify)]
    pub settings: HashMap<String, Value>,
    #[qml(skip)]
    pub seen: Vec<String>,
}

#[qml_impl]
impl Bridge {
    /// A payload of any shape, read all the way down.
    #[qml(invokable)]
    fn dispatch(&mut self, action: String, payload: serde_json::Value) {
        self.seen.push(format!("{action} {payload}"));
    }

    /// A list of records.
    #[qml(invokable)]
    fn rows(&self) -> Vec<HashMap<String, Value>> {
        [("/a", 3), ("/b", 5)]
            .into_iter()
            .map(|(path, size)| {
                HashMap::from([
                    ("path".to_owned(), Value::from(path)),
                    ("size".to_owned(), Value::Int(size)),
                ])
            })
            .collect()
    }

    #[qml(invokable)]
    fn json(&self) -> serde_json::Value {
        serde_json::json!({ "name": "x", "files": [{ "size": 2 }, { "size": 7 }] })
    }

    #[qml(invokable)]
    fn longest(&self, words: Vec<String>) -> Option<String> {
        words.into_iter().max_by_key(String::len)
    }
}

const MAIN: &str = r#"
import QtQuick
import Example
Item {
    property alias bridge: b
    Bridge { id: b }
    property var rows: b.rows()
    property string summary: rows.length + " " + rows[1].path + " " + rows[1].size
    property int total: b.json().files[0].size + b.json().files[1].size
    property string longest: b.longest(["a", "abc", "ab"])
    property bool none: b.longest([]) === null
    property string tagText: b.tags.join(",")
    property string titleText: b.title === null ? "none" : b.title
    property int depth: b.settings.nested ? b.settings.nested.level : -1
    function send() { b.dispatch("withMap", { a: 1, nested: { b: [1, 2.5], c: null } }) }
    function setAll() {
        b.tags = ["x", "y"]
        b.title = "hello"
        b.settings = { nested: { level: 3 } }
    }
    function clearTitle() { b.title = null }
}
"#;

fn load() -> (Engine, ObjectId, ObjectId) {
    let engine = ljos::engine();
    engine.register_module(ljos::module!("Example" 1.0 => [Bridge]));
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let source = ljos::dev::compile_dir_with(dir.path(), &engine).unwrap();
    let root = ljos::create_root(&engine, source, "Main").unwrap();
    let bridge = engine.read_by_name(root, "bridge").as_object().unwrap();
    (engine, root, bridge)
}

fn text(engine: &Engine, obj: ObjectId, name: &str) -> String {
    engine.read_by_name(obj, name).to_js_string().to_string()
}

#[test]
fn rust_gives_qml_records_and_lists() {
    let (engine, root, _) = load();
    assert_eq!(text(&engine, root, "summary"), "2 /b 5");
    assert_eq!(text(&engine, root, "total"), "9");
    assert_eq!(text(&engine, root, "longest"), "abc");
    assert_eq!(engine.read_by_name(root, "none"), Value::Bool(true));
}

#[test]
fn a_nested_payload_reaches_rust_whole() {
    let (engine, root, bridge) = load();
    engine.invoke_by_name(root, "send", &[]).unwrap();
    let seen = engine
        .with::<Bridge, _>(bridge, |b| b.seen.clone())
        .unwrap();
    assert_eq!(seen, [r#"withMap {"a":1,"nested":{"b":[1,2.5],"c":null}}"#]);
}

#[test]
fn typed_properties_take_what_qml_writes() {
    let (engine, root, bridge) = load();
    assert_eq!(text(&engine, root, "tagText"), "");
    assert_eq!(text(&engine, root, "titleText"), "none");
    assert_eq!(engine.read_by_name(root, "depth"), Value::Int(-1));

    engine.invoke_by_name(root, "setAll", &[]).unwrap();
    assert_eq!(engine.get(bridge, Bridge::TAGS).unwrap(), ["x", "y"]);
    assert_eq!(
        engine.get(bridge, Bridge::TITLE).unwrap().as_deref(),
        Some("hello")
    );
    let settings = engine.get(bridge, Bridge::SETTINGS).unwrap();
    assert_eq!(
        settings["nested"].get("level").map(Value::to_number),
        Some(3.0)
    );
    assert_eq!(text(&engine, root, "tagText"), "x,y");
    assert_eq!(text(&engine, root, "titleText"), "hello");
    assert_eq!(engine.read_by_name(root, "depth").to_number(), 3.0);

    engine.invoke_by_name(root, "clearTitle", &[]).unwrap();
    assert_eq!(engine.get(bridge, Bridge::TITLE).unwrap(), None);
    assert_eq!(text(&engine, root, "titleText"), "none");
}

#[test]
fn rust_sets_a_record_qml_reads() {
    let (engine, root, bridge) = load();
    let level = Value::map([("level", Value::Int(4))]);
    engine
        .set(
            bridge,
            Bridge::SETTINGS,
            HashMap::from([("nested".to_owned(), level)]),
        )
        .unwrap();
    assert_eq!(engine.read_by_name(root, "depth").to_number(), 4.0);
}

#[test]
fn a_var_payload_is_read_deep_on_request() {
    let (engine, _, _) = load();
    let payload = engine
        .eval_js("({ a: 1, nested: { b: [1, { c: 'd' }] } })")
        .unwrap();
    let copy = engine.deep_copy(&payload);
    let nested = copy.get("nested").unwrap();
    let b = nested.get("b").unwrap().as_list().unwrap();
    assert_eq!(b[1].get("c"), Some(&Value::from("d")));
}

#[test]
fn rust_calling_with_a_scripts_object_hands_over_data() {
    let (engine, _, bridge) = load();
    let payload = engine.eval_js("({ ids: [4, 5] })").unwrap();
    engine
        .invoke_by_name(bridge, "dispatch", &[Value::from("open"), payload])
        .unwrap();
    let seen = engine
        .with::<Bridge, _>(bridge, |b| b.seen.clone())
        .unwrap();
    assert_eq!(seen, [r#"open {"ids":[4,5]}"#]);
}
