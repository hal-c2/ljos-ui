//! The model as QML sees it, through the facade alone: an Engine from
//! `ljos::engine()`, this crate's `register`, and the platform's
//! answer delivery driven by hand (`finish`), so Loading is observed
//! before Ready without sleeping.

use ljos::prelude::{Engine, ObjectId, Platform};

fn engine() -> Engine {
    let e = ljos::engine();
    ljos_xml::register(&e);
    e.take_log();
    e
}

/// Compiles `Main` and creates it.
fn load(e: &Engine, qml: &str) -> ObjectId {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(ljos::dev::EngineOracle::from_engine(e));
    project.add("Main", qml);
    let compiled = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!compiled.has_errors(), "{:?}", compiled.diagnostics);
    let mut source = ljos::MapSource::new();
    source.insert("Main", compiled.file);
    e.set_component_source(source);
    e.create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"))
}

fn find(e: &Engine, root: ObjectId, name: &str) -> ObjectId {
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if e.read_by_name(o, "objectName").to_js_string().as_str() == name {
            return o;
        }
        stack.extend(e.children(o));
    }
    panic!("no object named {name}")
}

fn int(e: &Engine, o: ObjectId, prop: &str) -> i64 {
    e.read_by_name(o, prop).to_number() as i64
}

fn text(e: &Engine, o: ObjectId, prop: &str) -> String {
    e.read_by_name(o, prop).to_js_string().to_string()
}

const FEED: &[u8] = b"<rss><channel>\
    <item id=\"a1\"><title>First</title></item>\
    <item id=\"a2\"><title>Second</title></item>\
    </channel></rss>";

const MAIN: &str = r#"
import QtQuick
import QtQml.XmlListModel
Item {
    id: root
    objectName: "o"
    property string seen: ""
    property string err: "?"
    XmlListModel {
        id: m
        objectName: "m"
        source: "feed.xml"
        query: "/rss/channel/item"
        XmlListModelRole { name: "title"; elementName: "title" }
        XmlListModelRole { name: "itemId"; elementName: ""; attributeName: "id" }
        onStatusChanged: {
            root.seen += status + ";"
            if (status === XmlListModel.Error) root.err = m.errorString()
        }
    }
}
"#;

#[test]
fn a_document_loads_in_the_background_then_shows_its_rows() {
    let e = engine();
    Platform::of(&e).register_resource("file:///feed.xml", FEED.to_vec());
    let root = load(&e, MAIN);
    let m = find(&e, root, "m");
    // The answer has not been delivered: still loading, no rows.
    assert_eq!(int(&e, m, "status"), 2);
    assert_eq!(int(&e, m, "count"), 0);
    Platform::of(&e).finish(&e);
    assert_eq!(int(&e, m, "status"), 1);
    assert_eq!(int(&e, m, "count"), 2);
    assert_eq!(text(&e, root, "seen"), "2;1;");
    // The rows a delegate would read, by role.
    let model = e.object_model(m).expect("the object is a model");
    let rows: Vec<String> = (0..model.row_count())
        .map(|r| {
            format!(
                "{}|{}",
                model.data(&e, r, "title").to_js_string(),
                model.data(&e, r, "itemId").to_js_string()
            )
        })
        .collect();
    assert_eq!(rows, vec!["First|a1", "Second|a2"]);
}

#[test]
fn a_fetch_that_fails_is_an_error_that_says_why() {
    let e = engine();
    let main = MAIN.replace("feed.xml", "http://127.0.0.1:9/feed.xml");
    let root = load(&e, &main);
    let m = find(&e, root, "m");
    Platform::of(&e).finish(&e);
    assert_eq!(int(&e, m, "status"), 3);
    assert_eq!(int(&e, m, "count"), 0);
    let err = text(&e, root, "err");
    assert!(!err.is_empty() && err != "?", "errorString was {err:?}");
}

#[test]
fn no_source_is_null_and_reload_rereads() {
    let e = engine();
    Platform::of(&e).register_resource("file:///feed.xml", FEED.to_vec());
    let main = MAIN.replace("source: \"feed.xml\"", "");
    let root = load(&e, &main);
    let m = find(&e, root, "m");
    Platform::of(&e).finish(&e);
    assert_eq!(int(&e, m, "status"), 0);
    e.write_by_name(m, "source", "feed.xml".into()).unwrap();
    Platform::of(&e).finish(&e);
    assert_eq!(int(&e, m, "status"), 1);
    assert_eq!(int(&e, m, "count"), 2);
    e.invoke_by_name(m, "reload", &[]).unwrap();
    assert_eq!(int(&e, m, "status"), 2);
    Platform::of(&e).finish(&e);
    assert_eq!(text(&e, root, "seen"), "2;0;2;1;2;1;");
}
