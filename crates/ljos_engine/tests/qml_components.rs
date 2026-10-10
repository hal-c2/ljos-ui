//! Components: file components, inline components, required properties,
//! completion order, `Component.createObject` and incremental creation.

mod support;

use ljos_engine::{CreationJob, EngineError, ObjectFlags, Value};
use ljos_runtime::{Lane, ProcId};
use support::engine;

#[test]
fn on_completed_runs_in_qts_order() {
    let h = engine();
    h.files(&[
        (
            "Foo",
            r#"import QtQuick
Item { Component.onCompleted: console.log("foo root")
  Item { Component.onCompleted: console.log("foo child") } }
"#,
        ),
        (
            "Main",
            r#"import QtQuick
Item {
    Component.onCompleted: console.log("root")
    Item { Component.onCompleted: console.log("a")
        Item { Component.onCompleted: console.log("a1") }
        Item { Component.onCompleted: console.log("a2") }
    }
    Item { Component.onCompleted: console.log("b") }
    Foo { Component.onCompleted: console.log("foo use") }
}
"#,
        ),
    ]);
    h.engine.create_from_url("Main", None, vec![]).unwrap();
    // Checked with qml6: each object before its children, the last child
    // first, and a component's own handler before its user's.
    assert_eq!(
        h.log(),
        vec![
            "root",
            "foo root",
            "foo use",
            "foo child",
            "b",
            "a",
            "a2",
            "a1"
        ]
    );
}

#[test]
fn a_file_component_is_a_type_with_its_own_properties() {
    let h = engine();
    h.files(&[
        (
            "Button",
            r#"import QtQuick
Item {
  id: me
  property string label: "?"
  property int clicks: 0
  signal clicked()
  onClicked: clicks++
  width: label.length * 10
}
"#,
        ),
        (
            "Main",
            r#"import QtQuick
Item {
  Button { id: b; objectName: "b"; label: "four" }
  property int total: b.clicks
}
"#,
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    let b = h.find(root, "b");
    assert_eq!(h.num(b, "width"), 40.0);
    h.engine.emit_by_name(b, "clicked", &[]).unwrap();
    h.engine.emit_by_name(b, "clicked", &[]).unwrap();
    assert_eq!(h.num(root, "total"), 2.0);
    assert_eq!(h.engine.meta_of(b).unwrap().name.as_str(), "Button");
}

#[test]
fn ids_of_a_file_component_are_private_to_it() {
    let h = engine();
    h.files(&[
        (
            "Inner",
            "import QtQuick\nItem { id: box; property int v: 1; property int w: box.v * 2 }\n",
        ),
        (
            "Main",
            "import QtQuick\nItem { id: box; property int v: 10\n  Inner { objectName: \"i\" } }\n",
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    let i = h.find(root, "i");
    assert_eq!(h.num(i, "w"), 2.0);
}

#[test]
fn inline_components_and_required_properties() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  component Cell: Item { required property int n; width: n * 2 }
  Cell { objectName: "c"; n: 4 }
}
"#,
    );
    let c = h.find(root, "c");
    assert_eq!(h.num(c, "width"), 8.0);
}

#[test]
fn a_missing_required_property_is_an_error() {
    let h = engine();
    h.files(&[(
        "Main",
        "import QtQuick\nItem {\n  required property int n\n}\n",
    )]);
    let before = h.engine.object_count();
    let err = h.engine.create_from_url("Main", None, vec![]).unwrap_err();
    assert!(
        matches!(&err, EngineError::RequiredNotSet(m)
            if m == "file:///Main.qml:2:1: Required property n was not initialized"),
        "{err:?}"
    );
    assert_eq!(h.engine.object_count(), before);
    // Given as an initial property, it is set.
    let root = h
        .engine
        .create_from_url("Main", None, vec![("n".into(), Value::Int(3))])
        .unwrap();
    assert_eq!(h.num(root, "n"), 3.0);
}

#[test]
fn objects_are_completed_once_created() {
    let h = engine();
    let root = h.load("import QtQuick\nItem { Item { objectName: \"k\" } }\n");
    let k = h.find(root, "k");
    assert!(
        h.engine
            .flags(root)
            .unwrap()
            .contains(ObjectFlags::COMPLETED)
    );
    assert!(h.engine.flags(k).unwrap().contains(ObjectFlags::COMPLETED));
}

#[test]
fn component_create_object_makes_instances_with_properties() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  id: root
  property int base: 5
  property var made: []
  Component {
    id: comp
    Item { property int n; width: n + root.base }
  }
  function make(n) {
    var o = comp.createObject(root, { n: n })
    made = made.concat([o])
    return o.width
  }
  property int status: comp.status
}
"#,
    );
    assert_eq!(h.num(root, "status"), 1.0);
    let w = h
        .engine
        .invoke_by_name(root, "make", &[Value::Int(3)])
        .unwrap();
    assert_eq!(w.to_number(), 8.0);
    let kids = h.engine.visual_children(root);
    assert_eq!(kids.len(), 1);
    h.set(root, "base", Value::Int(10));
    assert_eq!(h.num(kids[0], "width"), 13.0);
    assert!(h.warnings().is_empty());
}

#[test]
fn a_component_from_a_url_is_created_with_initial_properties() {
    let h = engine();
    h.files(&[
        (
            "Label",
            "import QtQuick\nItem { property string text; width: text.length }\n",
        ),
        ("Main", "import QtQuick\nItem {}\n"),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    let l = h
        .engine
        .create_from_url(
            "Label",
            Some(root),
            vec![("text".into(), Value::from("hello"))],
        )
        .unwrap();
    assert_eq!(h.num(l, "width"), 5.0);
    assert_eq!(h.engine.visual_parent(l), Some(root));
    assert_eq!(h.engine.owner(l), Some(root));
}

#[test]
fn destroy_from_js_removes_the_object() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  Item { id: kid; objectName: "kid" }
  property bool gone: false
  function drop() { kid.destroy() }
}
"#,
    );
    let kid = h.find(root, "kid");
    h.engine.invoke_by_name(root, "drop", &[]).unwrap();
    // Deferred, as in Qt: gone once control returns to the event loop.
    h.pass_ms(0);
    assert!(!h.engine.is_alive(kid));
}

#[test]
fn incremental_creation_yields_and_finishes() {
    let h = engine();
    let mut src = String::from("import QtQuick\nItem {\n  property int sum: 0\n");
    for i in 0..50 {
        src.push_str(&format!("  Item {{ width: {i} }}\n"));
    }
    src.push_str("  Component.onCompleted: console.log(\"done\")\n}\n");
    h.files(&[("Main", &src)]);
    let (mut job, handle) = CreationJob::new(
        &h.engine,
        "Main",
        None,
        vec![],
        Lane::Visible,
        ProcId::new(0, 0),
    );
    let mut steps = 0;
    loop {
        // A slice that is spent after every object.
        match job.step(&|| true) {
            ljos_runtime::JobPoll::Done => break,
            ljos_runtime::JobPoll::Yield => steps += 1,
            ljos_runtime::JobPoll::Failed(f) => panic!("{f:?}"),
        }
        assert!(h.log().is_empty(), "completion runs at the end");
    }
    assert!(steps > 10, "{steps}");
    let root = handle.result().unwrap().unwrap();
    assert_eq!(h.engine.visual_children(root).len(), 50);
    assert_eq!(h.log(), vec!["done"]);
}

#[test]
fn a_file_that_uses_a_missing_module_is_rejected() {
    let h = engine();
    h.files(&[("Main", "import QtQuick\nItem {}\n")]);
    // Loading succeeds when the module exists.
    assert!(h.engine.create_from_url("Main", None, vec![]).is_ok());
    let err = h.engine.create_from_url("Nope", None, vec![]).unwrap_err();
    assert!(err.to_string().contains("No such file"), "{err}");
}
