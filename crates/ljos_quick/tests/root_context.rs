//! What the application gives the root context is seen from every context
//! below it: a delegate's, a Loader's, an object a Component makes. The
//! values were observed on Qt 6.12.0 with a QQmlPropertyMap given to the
//! root context as the documentation's `owner`, and the same tree in QML.

mod common;

use common::*;
use ljos_engine::{Engine, ObjectId, PropertyMap, Value};

const MAIN: &str = r#"import QtQuick
Item {
    id: root
    width: 400; height: 400
    Item { objectName: "direct"; property real g: owner.gap }
    Repeater {
        model: 1
        Item {
            objectName: "repeater"; property real g: owner.gap
            property bool has: typeof owner !== "undefined"
        }
    }
    Column { Repeater { model: 1; Item { objectName: "column"; property real g: owner.gap } } }
    ListView {
        width: 100; height: 100; model: 1
        delegate: Item { objectName: "listView"; width: 10; height: 10; property real g: owner.gap }
    }
    Loader { sourceComponent: Item { objectName: "loader"; property real g: owner.gap } }
    Loader { source: "Panel.qml" }
    Component { id: made; Item { objectName: "createObject"; property real g: owner.gap } }
    Component.onCompleted: made.createObject(root)
}
"#;

const PANEL: &str = r#"import QtQuick
Item { objectName: "file"; property real g: owner.gap }
"#;

const READERS: [&str; 7] = [
    "direct",
    "repeater",
    "column",
    "listView",
    "loader",
    "file",
    "createObject",
];

/// Loads `MAIN` with `give` handing the root context the map, and checks
/// every reader sees its key and follows it.
fn every_context_sees(give: impl Fn(&Engine, ObjectId)) {
    let e = engine();
    let owner = PropertyMap::new(&e, None).expect("map creates");
    owner.insert(&e, "gap", Value::Real(3.0));
    give(&e, owner.object());
    compile(&e, &[("Main", MAIN), ("Panel", PANEL)]);
    let root = e
        .create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"));
    let mut s = scene(&e, root, 400.0, 400.0);
    frame(&mut s, &e);
    let seen = |e: &Engine| -> Vec<(&str, f64)> {
        READERS
            .iter()
            .map(|n| (*n, num(e, find(e, root, n), "g")))
            .collect()
    };
    assert_eq!(seen(&e), READERS.map(|n| (n, 3.0)));
    assert_eq!(
        get(&e, find(&e, root, "repeater"), "has"),
        Value::Bool(true)
    );
    owner.insert(&e, "gap", Value::Real(5.0));
    frame(&mut s, &e);
    assert_eq!(seen(&e), READERS.map(|n| (n, 5.0)));
}

/// `owner` declared as an id of the root context, the way an application
/// names an object it gives every file (#680).
#[test]
fn an_id_of_the_root_context_is_seen_from_every_context_below_it() {
    every_context_sees(|e, owner| e.set_context_id(e.root_context(), "owner", owner));
}

/// `owner` given as a context property of the root context
/// (`setContextProperty`).
#[test]
fn a_property_of_the_root_context_is_seen_from_every_context_below_it() {
    every_context_sees(|e, owner| {
        e.set_context_property(e.root_context(), "owner", Value::Object(owner))
    });
}

/// A binding that read an id of the root context reads again when its
/// object is destroyed, from a delegate as from the file's items: Qt 6.12.0
/// gives `false`, then `true` in both once the application deletes the map.
#[test]
fn a_root_context_id_destroyed_re_runs_what_read_it() {
    let e = engine();
    let owner = PropertyMap::new(&e, None).expect("map creates");
    e.set_context_id(e.root_context(), "owner", owner.object());
    let root = load(
        &e,
        r#"import QtQuick
Item {
    Item { objectName: "direct"; property bool gone: owner === null }
    Repeater { model: 1; Item { objectName: "repeater"; property bool gone: owner === null } }
}
"#,
    );
    let gone = |e: &Engine| ["direct", "repeater"].map(|n| (n, get(e, find(e, root, n), "gone")));
    assert_eq!(
        gone(&e),
        ["direct", "repeater"].map(|n| (n, Value::Bool(false)))
    );
    e.destroy_now(owner.object());
    assert_eq!(
        gone(&e),
        ["direct", "repeater"].map(|n| (n, Value::Bool(true)))
    );
}

/// A delegate writes a key through an id of the root context, and the map
/// and every reader see it, as a write in a file's item does.
#[test]
fn a_delegate_writes_through_a_root_context_id() {
    let e = engine();
    let owner = PropertyMap::new(&e, None).expect("map creates");
    owner.insert(&e, "gap", Value::Real(3.0));
    e.set_context_id(e.root_context(), "owner", owner.object());
    let root = load(
        &e,
        r#"import QtQuick
Item {
    width: 400; height: 400
    Item { objectName: "direct"; property real g: owner.gap }
    Repeater {
        model: 1
        Item { objectName: "repeater"; function write(v) { owner.gap = v } }
    }
    ListView {
        width: 100; height: 100; model: 1
        delegate: Item {
            objectName: "listView"; width: 10; height: 10
            function write(v) { owner.gap = v }
        }
    }
}
"#,
    );
    let mut s = scene(&e, root, 400.0, 400.0);
    frame(&mut s, &e);
    for (writer, v) in [("repeater", 4.0), ("listView", 6.0)] {
        e.invoke_by_name(find(&e, root, writer), "write", &[Value::Real(v)])
            .unwrap_or_else(|err| panic!("{writer}: {err}"));
        frame(&mut s, &e);
        assert_eq!(
            owner.value(&e, "gap"),
            Value::Real(v),
            "{writer} wrote the map"
        );
        assert_eq!(
            num(&e, find(&e, root, "direct"), "g"),
            v,
            "{writer}'s write is read"
        );
    }
}
