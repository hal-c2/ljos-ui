//! A `TestWindow` turns its accessibility tree on and reads it as an
//! assistive technology would: each node's role, computed name, states
//! and children, and which item a node stands for.

use ljos::App;
use ljos::prelude::{Engine, ObjectId, Value};
use ljos::testing::{AccessibleNode, TestWindow};

const MAIN: &str = r#"
import QtQuick
import QtQuick.Controls

Column {
    property Item wrap: wrap
    property Item tab: tab
    Switch { id: wrap; text: "Wrap lines"; checked: true }
    TextField { id: tab; Accessible.name: "Tab width"; enabled: false }
}
"#;

fn open() -> (Engine, TestWindow, ObjectId) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let source = ljos::dev::compile(dir.path(), None, ljos::dev::OnError::Fail).unwrap();
    let engine = App::new("accessibility").source(source).build_engine();
    let mut window = TestWindow::new(&engine, 200.0, 100.0);
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();
    window.show(root).unwrap();
    (engine, window, root)
}

fn item(engine: &Engine, root: ObjectId, name: &str) -> ObjectId {
    match engine.read_by_name(root, name) {
        Value::Object(o) => o,
        other => panic!("{name} is not an item: {other:?}"),
    }
}

fn words(node: &AccessibleNode) -> Vec<&str> {
    node.states.iter().map(String::as_str).collect()
}

#[test]
fn the_tree_is_off_until_turned_on() {
    let (_, window, _) = open();
    assert!(window.accessibility_tree().children.is_empty());
}

#[test]
fn nodes_carry_role_computed_name_and_states() {
    let (_, mut window, _) = open();
    window.set_accessibility_active(true).unwrap();
    let tree = window.accessibility_tree();
    assert_eq!(tree.role, "window");
    // A switch is named by its text, not by `Accessible.name` read off it.
    let wrap = tree.find("switch", "Wrap lines").expect("the switch");
    assert_eq!(words(wrap), ["checkable", "checked"]);
    let tab = tree.find("textinput", "Tab width").expect("the field");
    assert!(words(tab).contains(&"disabled"), "{:?}", tab.states);
    assert!(!tab.focused);
}

#[test]
fn a_node_names_its_item_and_an_item_its_node() {
    let (engine, mut window, root) = open();
    window.set_accessibility_active(true).unwrap();
    let tree = window.accessibility_tree();
    let wrap = item(&engine, root, "wrap");
    let node = tree.find("switch", "Wrap lines").unwrap().key.unwrap();
    assert_eq!(window.accessible_node(wrap), Some(node));
    assert_eq!(window.accessible_item(node), Some(wrap));
    let tab = item(&engine, root, "tab");
    let key = window.accessible_node(tab).expect("the field's node");
    assert_ne!(key, node);
}

#[test]
fn turned_off_the_tree_is_empty_again() {
    let (_, mut window, _) = open();
    window.set_accessibility_active(true).unwrap();
    window.set_accessibility_active(false).unwrap();
    assert!(window.accessibility_tree().children.is_empty());
}
