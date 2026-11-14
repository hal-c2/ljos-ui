//! A native setter that takes a `Cx` is handed what a binding gives though
//! nothing reads the property, in the engine an application runs, which
//! works bindings out when they are read: the object may act on the value
//! (follow the object it names, start work), as a Qt setter does when its
//! binding runs. The scenario engine is eager and hides this
//! (`features/language/property-setter-writes-others.feature`).

use std::cell::RefCell;
use std::path::Path;

use ljos::prelude::*;

thread_local! {
    static HANDED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn handed() -> Vec<String> {
    HANDED.with(|t| std::mem::take(&mut *t.borrow_mut()))
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Follower")]
pub struct Follower {
    #[qml(notify, write = Self::follow)]
    pub path: String,
}

impl Follower {
    fn follow(_cx: &Cx, path: String) -> Option<String> {
        HANDED.with(|t| t.borrow_mut().push(path.clone()));
        Some(path)
    }
}

fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Follower]));
}

fn app(files: &[(&str, &str)]) -> (tempfile::TempDir, Engine, ObjectId) {
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in files {
        std::fs::write(dir.path().join(Path::new(name)), text).unwrap();
    }
    let (engine, root) = ljos::App::new("Test")
        .register(register)
        .qml_dir(dir.path(), "Main")
        .unwrap()
        .build()
        .unwrap();
    (dir, engine, root)
}

fn set(engine: &Engine, obj: ObjectId, name: &str, v: &str) {
    engine.write_by_name(obj, name, Value::from(v)).unwrap();
}

#[test]
fn a_binding_nothing_reads_hands_the_setter_its_value_when_made_and_on_change() {
    let (_dir, engine, root) = app(&[(
        "Main.qml",
        r#"import QtQuick
import Example
Item {
    id: root
    property string wanted: "/a"
    Follower { path: root.wanted }
}
"#,
    )]);
    assert_eq!(handed(), ["/a"]);
    set(&engine, root, "wanted", "/b");
    assert_eq!(handed(), ["/b"]);
}

/// The shape of a panel a loader makes with an initial property bound to
/// what its user shows: the panel's child follows it with nothing reading.
#[test]
fn an_initial_property_reaches_the_setter_of_a_loaded_files_child() {
    let (_dir, engine, root) = app(&[
        (
            "Panel.qml",
            r#"import QtQuick
import Example
Item {
    id: panel
    property string source
    Follower { path: panel.source }
}
"#,
        ),
        (
            "Main.qml",
            r#"import QtQuick
Item {
    id: root
    property string shown: "/a"
    Loader {
        Component.onCompleted: setSource("Panel.qml", { source: Qt.binding(() => root.shown) })
    }
}
"#,
        ),
    ]);
    assert_eq!(handed(), ["/a"]);
    set(&engine, root, "shown", "/b");
    assert_eq!(handed(), ["/b"]);
}

/// A state's binding that nothing reads through hands the setter each
/// value too.
#[test]
fn a_states_binding_nothing_reads_hands_the_setter_each_value() {
    let (_dir, engine, root) = app(&[(
        "Main.qml",
        r#"import QtQuick
import Example
Item {
    id: root
    property string wanted: "/a"
    Follower { id: follower }
    states: State {
        name: "following"
        when: true
        PropertyChanges { follower.path: root.wanted }
    }
}
"#,
    )]);
    assert_eq!(handed(), ["/a"]);
    set(&engine, root, "wanted", "/b");
    assert_eq!(handed(), ["/b"]);
}
