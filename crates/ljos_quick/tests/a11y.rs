//! The `Accessible` attached type and the scene's accessibility tree:
//! what is exposed, where, and how little work a change costs.

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::*;
use ljos_a11y::accesskit::Role as AkRole;
use ljos_a11y::{Bounds, NodeKey, Role};
use ljos_engine::{Engine, MetaRef, ObjectId, QmlType, Value};
use ljos_quick::{Accessible, Item, QuickScene, Rectangle, set_parent_item};
use ljos_runtime::Time;

/// The item's `Accessible` object, made on first use as QML does.
fn accessible(e: &Engine, obj: ObjectId) -> ObjectId {
    e.attached_object(obj, &MetaRef::Static(Accessible::static_meta()))
        .unwrap()
}

fn set_a11y(e: &Engine, obj: ObjectId, name: &str, v: Value) {
    set(e, accessible(e, obj), name, v);
}

fn role(r: Role) -> Value {
    Value::Int(r.qt_value())
}

/// A scene with accessibility on and its tree built.
fn active(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = scene(e, root, 400.0, 300.0);
    s.set_accessibility_active(e, true);
    frame(&mut s, e);
    s
}

/// `(role, name, parent's name or "")` in tree order.
fn outline(s: &QuickScene) -> Vec<(AkRole, String, String)> {
    let tree = s.accessibility_tree();
    let mut out = Vec::new();
    tree.walk(|_, parent, data| {
        let above = tree.get(parent).map(|p| p.name.clone()).unwrap_or_default();
        out.push((data.role, data.name.clone(), above));
    });
    out
}

fn names(s: &QuickScene) -> Vec<String> {
    outline(s).into_iter().map(|(_, n, _)| n).collect()
}

const SAVE: &str = r#"
import QtQuick
Item {
    objectName: "root"
    width: 200; height: 100
    Item {
        objectName: "bar"
        x: 10; y: 20
        Rectangle {
            objectName: "save"
            x: 5; y: 5; width: 80; height: 30
            Accessible.role: Accessible.Button
            Accessible.name: "Save"
            Accessible.description: "Writes the file"
        }
    }
}
"#;

#[test]
fn compiled_qml_exposes_a_named_button() {
    let e = engine();
    let root = load(&e, SAVE);
    assert!(e.take_warnings().is_empty());
    let s = active(&e, root);
    let save = find(&e, root, "save");

    let tree = s.accessibility_tree();
    assert_eq!(tree.len(), 1, "only the item with a role is exposed");
    let key = tree.find(AkRole::Button, "Save").expect("the button");
    assert_eq!(s.accessible_item(key), Some(save));
    assert_eq!(s.accessible_node(save), Some(key));
    assert_eq!(s.accessible_node(root), None);
    let node = tree.get(key).unwrap();
    assert_eq!(node.qt_role, Role::Button);
    assert_eq!(node.description, "Writes the file");
    assert_eq!(node.author_id, "save");
    assert!(node.press);
    assert!(!node.disabled);
    // Scene coordinates: the bar's offset and the button's own.
    assert_eq!(node.bounds, Bounds::new(15.0, 25.0, 95.0, 55.0));
    assert_eq!(tree.parent(key), Some(NodeKey::ROOT));
}

#[test]
fn role_keys_have_qts_numbers() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    property int button: Accessible.Button
    property int push: Accessible.PushButton
    property int check: Accessible.CheckBox
    property int text: Accessible.StaticText
    property int quote: Accessible.BlockQuote
}
"#,
    );
    assert_eq!(get(&e, root, "button"), Value::Int(0x2B));
    assert_eq!(get(&e, root, "push"), Value::Int(0x2B));
    assert_eq!(get(&e, root, "check"), Value::Int(0x2C));
    assert_eq!(get(&e, root, "text"), Value::Int(0x29));
    assert_eq!(get(&e, root, "quote"), Value::Int(0x431));
}

#[test]
fn without_an_assistive_technology_the_scene_does_nothing() {
    let e = engine();
    let root = load(&e, SAVE);
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    assert!(!s.accessibility_active());
    assert!(s.accessibility_tree().is_empty());

    // Nothing watches the name, so a rename is not even a frame.
    let save = find(&e, root, "save");
    set_a11y(&e, save, "name", Value::string("Store"));
    assert!(!s.needs_frame());

    // Turning it on asks for one frame, which builds the tree.
    s.set_accessibility_active(&e, true);
    assert!(s.needs_frame());
    frame(&mut s, &e);
    assert_eq!(names(&s), ["Store"]);
    assert!(!s.needs_frame());

    // Off again: the tree is gone and so is the watching.
    s.set_accessibility_active(&e, false);
    assert!(s.accessibility_tree().is_empty());
    // One frame for what follows `screenReaderActive`, which went false.
    frame(&mut s, &e);
    assert!(!s.needs_frame());
    set_a11y(&e, save, "name", Value::string("Keep"));
    assert!(!s.needs_frame());
}

#[test]
fn a_rename_updates_one_node_and_then_the_scene_is_idle() {
    let e = engine();
    let root = load(&e, SAVE);
    let mut s = active(&e, root);
    let save = find(&e, root, "save");
    let key = s.accessible_node(save).unwrap();
    let _ = s.accessibility_tree_mut().take_changes();
    let before = s.accessibility_tree().stats();

    let woke = Rc::new(Cell::new(0));
    let w = woke.clone();
    s.set_waker(move || w.set(w.get() + 1));
    set_a11y(&e, save, "name", Value::string("Store"));
    assert_eq!(woke.get(), 1, "the rename asks for a frame");
    let out = s.frame(&e, Time::from_millis(16));
    assert!(!out.changed, "nothing to paint");
    assert_eq!(out.next_wake, None);

    let after = s.accessibility_tree().stats();
    assert_eq!(after.node_updates, before.node_updates + 1);
    assert_eq!(after.child_updates, before.child_updates);
    assert_eq!(after.removals, before.removals);
    let changes = s.accessibility_tree_mut().take_changes();
    assert_eq!(changes.changed, vec![key]);
    assert!(changes.removed.is_empty());
    assert_eq!(names(&s), ["Store"]);
    // The node kept its place.
    assert_eq!(
        s.accessibility_tree().get(key).unwrap().bounds,
        Bounds::new(15.0, 25.0, 95.0, 55.0)
    );

    // Idle: no frame wanted, and a frame anyway touches nothing.
    assert!(!s.needs_frame());
    assert_eq!(woke.get(), 1);
    s.frame(&e, Time::from_millis(32));
    assert_eq!(s.accessibility_tree().stats(), after);
    assert!(!s.accessibility_tree().has_changes());
}

#[test]
fn what_the_tree_does_not_show_does_not_touch_it() {
    let e = engine();
    let root = load(&e, SAVE);
    let mut s = active(&e, root);
    let save = find(&e, root, "save");
    let before = s.accessibility_tree().stats();
    set(&e, save, "color", Value::string("red"));
    set(&e, save, "opacity", Value::Real(0.5));
    frame(&mut s, &e);
    assert_eq!(s.accessibility_tree().stats(), before);
}

#[test]
fn a_role_change_updates_the_same_node() {
    let e = engine();
    let root = load(&e, SAVE);
    let mut s = active(&e, root);
    let save = find(&e, root, "save");
    let key = s.accessible_node(save).unwrap();
    let before = s.accessibility_tree().stats();

    set_a11y(&e, save, "role", role(Role::CheckBox));
    frame(&mut s, &e);
    let node = s.accessibility_tree().get(key).unwrap();
    assert_eq!(node.role, AkRole::CheckBox);
    assert_eq!(node.name, "Save");
    let after = s.accessibility_tree().stats();
    assert_eq!(after.node_updates, before.node_updates + 1);
    assert_eq!(after.child_updates, before.child_updates);

    // No role and a name: still there, as a group.
    set_a11y(&e, save, "role", role(Role::NoRole));
    frame(&mut s, &e);
    assert_eq!(
        s.accessibility_tree().get(key).map(|n| n.role),
        Some(AkRole::Group)
    );
    // No role and no name: gone.
    set_a11y(&e, save, "name", Value::string(""));
    frame(&mut s, &e);
    assert!(s.accessibility_tree().is_empty());
    assert_eq!(s.accessible_node(save), None);
}

const NESTED: &str = r#"
import QtQuick
Item {
    objectName: "root"
    width: 300; height: 200
    Rectangle {
        objectName: "tools"
        x: 10; y: 10; width: 200; height: 40
        Accessible.role: Accessible.ToolBar
        Accessible.name: "Tools"
        Item {
            objectName: "spacer"
            x: 4; y: 4; width: 190; height: 30
            Rectangle {
                objectName: "open"
                width: 60; height: 30
                Accessible.role: Accessible.Button
                Accessible.name: "Open"
            }
            Rectangle {
                objectName: "close"
                x: 70; width: 60; height: 30
                Accessible.role: Accessible.Button
                Accessible.name: "Close"
            }
        }
    }
    Rectangle {
        objectName: "status"
        y: 150; width: 300; height: 20
        Accessible.role: Accessible.StatusBar
        Accessible.name: "Status"
    }
}
"#;

fn s(v: &str) -> String {
    v.to_owned()
}

#[test]
fn the_tree_follows_the_visual_tree() {
    let e = engine();
    let root = load(&e, NESTED);
    let sc = active(&e, root);
    // The spacer has no role and no name: its buttons are the toolbar's.
    assert_eq!(
        outline(&sc),
        [
            (AkRole::Toolbar, s("Tools"), s("")),
            (AkRole::Button, s("Open"), s("Tools")),
            (AkRole::Button, s("Close"), s("Tools")),
            (AkRole::Status, s("Status"), s("")),
        ]
    );
    let close = sc.accessible_node(find(&e, root, "close")).unwrap();
    assert_eq!(
        sc.accessibility_tree().get(close).unwrap().bounds,
        Bounds::new(84.0, 14.0, 144.0, 44.0)
    );
}

#[test]
fn an_ignored_item_leaves_its_children_in_its_place() {
    let e = engine();
    let root = load(&e, NESTED);
    let mut sc = active(&e, root);
    let tools = find(&e, root, "tools");
    let before = sc.accessibility_tree().stats();

    set_a11y(&e, tools, "ignored", Value::Bool(true));
    frame(&mut sc, &e);
    assert_eq!(
        outline(&sc),
        [
            (AkRole::Button, s("Open"), s("")),
            (AkRole::Button, s("Close"), s("")),
            (AkRole::Status, s("Status"), s("")),
        ]
    );
    // One node went and one list changed; the buttons were not rebuilt.
    let after = sc.accessibility_tree().stats();
    assert_eq!(after.removals, before.removals + 1);
    assert_eq!(after.node_updates, before.node_updates);

    set_a11y(&e, tools, "ignored", Value::Bool(false));
    frame(&mut sc, &e);
    assert_eq!(
        outline(&sc),
        [
            (AkRole::Toolbar, s("Tools"), s("")),
            (AkRole::Button, s("Open"), s("Tools")),
            (AkRole::Button, s("Close"), s("Tools")),
            (AkRole::Status, s("Status"), s("")),
        ]
    );
}

#[test]
fn invisible_items_are_left_out_with_everything_inside() {
    let e = engine();
    let root = load(&e, NESTED);
    let mut sc = active(&e, root);

    set(&e, find(&e, root, "open"), "visible", Value::Bool(false));
    frame(&mut sc, &e);
    assert_eq!(names(&sc), ["Tools", "Close", "Status"]);

    // The spacer is not exposed itself, but hiding it hides its buttons.
    let spacer = find(&e, root, "spacer");
    set(&e, spacer, "visible", Value::Bool(false));
    frame(&mut sc, &e);
    assert_eq!(names(&sc), ["Tools", "Status"]);

    set(&e, spacer, "visible", Value::Bool(true));
    frame(&mut sc, &e);
    // "open" is still hidden by its own `visible`.
    assert_eq!(names(&sc), ["Tools", "Close", "Status"]);

    set(&e, find(&e, root, "open"), "visible", Value::Bool(true));
    frame(&mut sc, &e);
    assert_eq!(names(&sc), ["Tools", "Open", "Close", "Status"]);
}

#[test]
fn a_disabled_item_stays_in_the_tree_as_disabled() {
    let e = engine();
    let root = load(&e, NESTED);
    let mut sc = active(&e, root);
    let open = find(&e, root, "open");
    set(&e, find(&e, root, "tools"), "enabled", Value::Bool(false));
    frame(&mut sc, &e);
    let key = sc.accessible_node(open).expect("still exposed");
    assert!(sc.accessibility_tree().get(key).unwrap().disabled);
}

#[test]
fn moving_a_container_moves_the_nodes_inside() {
    let e = engine();
    let root = load(&e, NESTED);
    let mut sc = active(&e, root);
    let open = sc.accessible_node(find(&e, root, "open")).unwrap();
    let status = sc.accessible_node(find(&e, root, "status")).unwrap();
    let _ = sc.accessibility_tree_mut().take_changes();

    set(&e, find(&e, root, "spacer"), "x", Value::Real(24.0));
    frame(&mut sc, &e);
    assert_eq!(
        sc.accessibility_tree().get(open).unwrap().bounds,
        Bounds::new(34.0, 14.0, 94.0, 44.0)
    );
    // The two buttons moved; the toolbar and the status bar did not.
    let changes = sc.accessibility_tree_mut().take_changes();
    assert_eq!(changes.changed.len(), 2);
    assert!(!changes.changed.contains(&status));
}

#[test]
fn a_container_moved_again_and_again_before_a_frame_places_once() {
    let e = engine();
    let root = load(&e, NESTED);
    let mut sc = active(&e, root);
    let open = sc.accessible_node(find(&e, root, "open")).unwrap();
    let before = sc.accessibility_tree().stats();

    let spacer = find(&e, root, "spacer");
    for x in [24.0, 48.0, 6.0] {
        set(&e, spacer, "x", Value::Real(x));
    }
    frame(&mut sc, &e);
    assert_eq!(
        sc.accessibility_tree().get(open).unwrap().bounds,
        Bounds::new(16.0, 14.0, 76.0, 44.0)
    );
    // Each button is placed once, where it ended up.
    let after = sc.accessibility_tree().stats();
    assert_eq!(after.node_updates, before.node_updates + 2);

    // Told of the next move as well.
    set(&e, spacer, "x", Value::Real(24.0));
    frame(&mut sc, &e);
    assert_eq!(
        sc.accessibility_tree().get(open).unwrap().bounds,
        Bounds::new(34.0, 14.0, 94.0, 44.0)
    );
}

#[test]
fn resizing_a_container_places_nothing_again() {
    let e = engine();
    let root = load(&e, NESTED);
    let mut sc = active(&e, root);
    let before = sc.accessibility_tree().stats();
    set(&e, root, "width", Value::Real(640.0));
    set(&e, find(&e, root, "spacer"), "height", Value::Real(12.0));
    frame(&mut sc, &e);
    assert_eq!(sc.accessibility_tree().stats(), before);
}

#[test]
fn rotation_gives_the_bounding_box() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 200.0, 200.0);
    let r = make::<Rectangle>(&e, Some(root));
    geom(&e, r, 50.0, 50.0, 40.0, 20.0);
    set(&e, r, "rotation", Value::Real(90.0));
    set_a11y(&e, r, "role", role(Role::Button));
    let sc = active(&e, root);
    let b = sc
        .accessibility_tree()
        .get(sc.accessible_node(r).unwrap())
        .unwrap()
        .bounds;
    // 40x20 about its centre (70, 60) becomes 20x40.
    for (got, want) in [(b.x0, 60.0), (b.y0, 40.0), (b.x1, 80.0), (b.y1, 80.0)] {
        assert!((got - want).abs() < 1e-9, "{b:?}");
    }
}

#[test]
fn text_names_its_node_only_when_it_has_a_role() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Text { objectName: "plain"; text: "Plain" }
    Text { objectName: "label"; text: "Total"; Accessible.role: Accessible.StaticText }
    Text {
        objectName: "named"; text: "42"
        Accessible.role: Accessible.StaticText; Accessible.name: "Answer"
    }
    Text { objectName: "nameonly"; text: "x"; Accessible.name: "Close" }
    Rectangle { objectName: "group"; Accessible.name: "Options" }
}
"#,
    );
    let mut sc = active(&e, root);
    assert_eq!(
        outline(&sc),
        [
            (AkRole::Label, s("Total"), s("")),
            (AkRole::Label, s("Answer"), s("")),
            (AkRole::Label, s("Close"), s("")),
            (AkRole::Group, s("Options"), s("")),
        ]
    );
    assert_eq!(sc.accessible_node(find(&e, root, "plain")), None);

    // The text is the name, so it follows the text. Only that node is
    // touched (its name, and its width with the new text).
    let label = find(&e, root, "label");
    let _ = sc.accessibility_tree_mut().take_changes();
    set(&e, label, "text", Value::string("Sum"));
    frame(&mut sc, &e);
    assert_eq!(names(&sc)[0], "Sum");
    assert_eq!(
        sc.accessibility_tree_mut().take_changes().changed,
        vec![sc.accessible_node(label).unwrap()]
    );

    // A Text with a name of its own does not watch its text.
    let before = sc.accessibility_tree().stats();
    set(&e, find(&e, root, "named"), "text", Value::string("43"));
    frame(&mut sc, &e);
    assert_eq!(sc.accessibility_tree().stats(), before);
}

#[test]
fn an_accessible_object_made_later_is_found() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 200.0, 100.0);
    let r = make::<Rectangle>(&e, Some(root));
    geom(&e, r, 10.0, 10.0, 50.0, 20.0);
    let mut sc = active(&e, root);
    assert!(sc.accessibility_tree().is_empty());

    // The first use of `Accessible` on the item, after its effect looked.
    set_a11y(&e, r, "role", role(Role::Button));
    assert!(sc.needs_frame());
    frame(&mut sc, &e);
    assert_eq!(outline(&sc), [(AkRole::Button, s(""), s(""))]);

    // And the new object is watched like any other.
    set_a11y(&e, r, "name", Value::string("Go"));
    frame(&mut sc, &e);
    assert_eq!(names(&sc), ["Go"]);
}

#[test]
fn focusable_follows_the_role_until_it_is_set() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Rectangle { objectName: "button"; Accessible.role: Accessible.Button }
    Rectangle { objectName: "slider"; Accessible.role: Accessible.Slider }
    Rectangle {
        objectName: "forced"
        Accessible.role: Accessible.Slider; Accessible.focusable: true
    }
    Rectangle {
        objectName: "off"
        Accessible.role: Accessible.Button; Accessible.focusable: false
    }
}
"#,
    );
    let focusable =
        |name: &str| get(&e, accessible(&e, find(&e, root, name)), "focusable").to_boolean();
    assert!(focusable("button"));
    assert!(!focusable("slider"));
    assert!(focusable("forced"));
    assert!(!focusable("off"));

    // The default follows a role change; a set value does not.
    set_a11y(&e, find(&e, root, "slider"), "role", role(Role::CheckBox));
    set_a11y(&e, find(&e, root, "off"), "role", role(Role::CheckBox));
    assert!(focusable("slider"));
    assert!(!focusable("off"));
    // Once on, the default stays on, as in Qt.
    set_a11y(&e, find(&e, root, "slider"), "role", role(Role::Slider));
    assert!(focusable("slider"));

    let sc = active(&e, root);
    let node = |name: &str| {
        let key = sc.accessible_node(find(&e, root, name)).unwrap();
        sc.accessibility_tree().get(key).unwrap().focusable
    };
    assert!(node("button"));
    assert!(node("forced"));
    assert!(!node("off"));
}

const FOCUS: &str = r#"
import QtQuick
Item {
    objectName: "root"
    width: 200; height: 100
    FocusScope {
        objectName: "bar"
        focus: true
        Accessible.role: Accessible.ToolBar; Accessible.name: "Tools"
        Rectangle {
            objectName: "save"; focus: true
            Accessible.role: Accessible.Button; Accessible.name: "Save"
        }
        Rectangle {
            objectName: "close"
            Accessible.role: Accessible.Button; Accessible.name: "Close"
        }
        Item { objectName: "bare" }
    }
    Rectangle {
        objectName: "gauge"
        Accessible.role: Accessible.Slider; Accessible.name: "Gauge"
    }
}
"#;

/// The name of the node that has the keyboard; `"window"` when none has.
fn focus_name(sc: &QuickScene) -> String {
    sc.accessible_focus_node().map_or_else(
        || "window".to_owned(),
        |k| sc.accessibility_tree().get(k).unwrap().name.clone(),
    )
}

#[test]
fn the_focus_node_follows_active_focus_and_two_nodes_hear_of_it() {
    let e = engine();
    let root = load(&e, FOCUS);
    let mut sc = active(&e, root);
    assert_eq!(focus_name(&sc), "Save");
    let update = sc.accessibility_tree_mut().take_update().unwrap();
    let save = sc.accessible_node(find(&e, root, "save")).unwrap();
    assert_eq!(update.focus, save.node_id());

    // Inside one scope: the old and the new focus item, not the scope.
    let before = sc.accessibility_tree().stats();
    set(&e, find(&e, root, "close"), "focus", Value::Bool(true));
    frame(&mut sc, &e);
    assert_eq!(focus_name(&sc), "Close");
    let after = sc.accessibility_tree().stats();
    assert_eq!(after.node_updates, before.node_updates + 2);
    assert_eq!(after.child_updates, before.child_updates);
    let close = sc.accessible_node(find(&e, root, "close")).unwrap();
    let update = sc.accessibility_tree_mut().take_update().unwrap();
    assert_eq!(update.focus, close.node_id());
    assert_eq!(update.nodes.len(), 2);

    // An item nobody is told about: the scope around it has the keyboard.
    set(&e, find(&e, root, "bare"), "focus", Value::Bool(true));
    frame(&mut sc, &e);
    assert_eq!(focus_name(&sc), "Tools");

    // The window loses the keyboard, and gets it back.
    sc.set_window_active(&e, false);
    frame(&mut sc, &e);
    assert_eq!(focus_name(&sc), "window");
    let update = sc.accessibility_tree_mut().take_update().unwrap();
    assert_eq!(update.focus, NodeKey::ROOT.node_id());
    sc.set_window_active(&e, true);
    frame(&mut sc, &e);
    assert_eq!(focus_name(&sc), "Tools");

    // Then nothing runs.
    assert!(!sc.needs_frame());
    let idle = sc.accessibility_tree().stats();
    frame(&mut sc, &e);
    assert_eq!(sc.accessibility_tree().stats(), idle);
}

#[test]
fn the_focus_action_gives_the_item_active_focus() {
    let e = engine();
    let root = load(&e, FOCUS);
    let mut sc = active(&e, root);
    let key = |sc: &QuickScene, name: &str| sc.accessible_node(find(&e, root, name)).unwrap();
    let has = |name: &str| get(&e, find(&e, root, name), "activeFocus").to_boolean();

    let close = key(&sc, "close");
    assert!(sc.accessible_focus(&e, close));
    frame(&mut sc, &e);
    assert!(has("close") && has("bar") && !has("save"));
    assert_eq!(focus_name(&sc), "Close");
    assert_eq!(sc.active_focus_item(&e), Some(find(&e, root, "close")));

    // A slider is not focusable until its author says so.
    let gauge = key(&sc, "gauge");
    assert!(!sc.accessible_focus(&e, gauge));
    assert!(has("close"));
    set_a11y(&e, find(&e, root, "gauge"), "focusable", Value::Bool(true));
    frame(&mut sc, &e);
    assert!(sc.accessible_focus(&e, gauge));
    frame(&mut sc, &e);
    assert!(has("gauge") && !has("bar") && !has("close"));
    assert_eq!(focus_name(&sc), "Gauge");

    // A disabled item does not take it.
    set(&e, find(&e, root, "save"), "enabled", Value::Bool(false));
    frame(&mut sc, &e);
    let save = key(&sc, "save");
    assert!(!sc.accessible_focus(&e, save));
    assert!(has("gauge"));

    // Nor does the window's own node, or one that is not there.
    assert!(!sc.accessible_focus(&e, NodeKey::ROOT));
    assert!(!sc.accessible_focus(&e, NodeKey(987_654)));
    assert!(e.take_warnings().is_empty());
}

#[test]
fn an_item_can_say_it_is_focused_itself() {
    let e = engine();
    let root = load(&e, FOCUS);
    let mut sc = active(&e, root);
    let gauge = find(&e, root, "gauge");
    // What the property reads is what was assigned, never active focus.
    let save = accessible(&e, find(&e, root, "save"));
    assert_eq!(get(&e, save, "focused"), Value::Bool(false));

    set(&e, find(&e, root, "bar"), "focus", Value::Bool(false));
    frame(&mut sc, &e);
    assert_eq!(focus_name(&sc), "window");
    set_a11y(&e, gauge, "focused", Value::Bool(true));
    frame(&mut sc, &e);
    assert_eq!(focus_name(&sc), "Gauge");
    assert!(!get(&e, gauge, "activeFocus").to_boolean());
}

#[test]
fn items_that_come_and_go_join_and_leave_the_tree() {
    let e = engine();
    let root = load(&e, NESTED);
    let mut sc = active(&e, root);
    let (tools, spacer, status) = (
        find(&e, root, "tools"),
        find(&e, root, "spacer"),
        find(&e, root, "status"),
    );

    // A new item between two exposed ones lands between them.
    let extra = make::<Rectangle>(&e, Some(spacer));
    set_a11y(&e, extra, "role", role(Role::Button));
    set_a11y(&e, extra, "name", Value::string("Extra"));
    frame(&mut sc, &e);
    assert_eq!(names(&sc), ["Tools", "Open", "Close", "Extra", "Status"]);
    assert_eq!(
        sc.accessibility_tree()
            .parent(sc.accessible_node(extra).unwrap()),
        sc.accessible_node(tools)
    );

    // Reparented: it moves to the node above its new place.
    set_parent_item(&e, extra, Some(status)).unwrap();
    frame(&mut sc, &e);
    assert_eq!(names(&sc), ["Tools", "Open", "Close", "Status", "Extra"]);
    assert_eq!(
        sc.accessibility_tree()
            .parent(sc.accessible_node(extra).unwrap()),
        sc.accessible_node(status)
    );

    // Destroyed: gone, with what was inside it and what it owned (the
    // extra button was made under the spacer).
    e.destroy_now(tools);
    frame(&mut sc, &e);
    assert_eq!(names(&sc), ["Status"]);
    assert_eq!(sc.accessibility_tree().len(), 1);
}

#[test]
fn a_node_that_gains_a_role_takes_over_the_nodes_inside_it() {
    let e = engine();
    let root = load(&e, NESTED);
    let mut sc = active(&e, root);
    let spacer = find(&e, root, "spacer");
    set_a11y(&e, spacer, "role", role(Role::Grouping));
    set_a11y(&e, spacer, "name", Value::string("Files"));
    frame(&mut sc, &e);
    assert_eq!(
        outline(&sc),
        [
            (AkRole::Toolbar, s("Tools"), s("")),
            (AkRole::Group, s("Files"), s("Tools")),
            (AkRole::Button, s("Open"), s("Files")),
            (AkRole::Button, s("Close"), s("Files")),
            (AkRole::Status, s("Status"), s("")),
        ]
    );
}

const CLICKABLE: &str = r#"
import QtQuick
Item {
    objectName: "root"
    width: 300; height: 100
    property int clicks: 0
    property int taps: 0
    Rectangle {
        objectName: "area"
        x: 10; y: 10; width: 80; height: 30
        Accessible.role: Accessible.Button
        Accessible.name: "Area"
        MouseArea { anchors.fill: parent; hoverEnabled: true; onClicked: clicks += 1 }
    }
    Rectangle {
        objectName: "tap"
        x: 110; y: 10; width: 80; height: 30
        Accessible.role: Accessible.Button
        Accessible.name: "Tap"
        TapHandler { onTapped: taps += 1 }
    }
    Rectangle {
        objectName: "label"
        x: 210; y: 10; width: 80; height: 30
        Accessible.role: Accessible.StaticText
        Accessible.name: "Label"
        MouseArea { anchors.fill: parent; onClicked: clicks += 100 }
    }
}
"#;

#[test]
fn the_press_action_clicks_what_is_under_the_node() {
    let e = engine();
    let root = load(&e, CLICKABLE);
    let mut sc = active(&e, root);
    let key = |sc: &QuickScene, name: &str| sc.accessible_node(find(&e, root, name)).unwrap();

    let area = key(&sc, "area");
    assert!(sc.accessible_press(&e, area, Time::from_millis(10)));
    assert_eq!(get(&e, root, "clicks"), Value::Int(1));
    // No pointer is there afterwards.
    let ma = e.children(find(&e, root, "area"))[0];
    assert_eq!(get(&e, ma, "containsMouse"), Value::Bool(false));
    assert_eq!(get(&e, ma, "pressed"), Value::Bool(false));

    let tap = key(&sc, "tap");
    assert!(sc.accessible_press(&e, tap, Time::from_millis(20)));
    assert_eq!(get(&e, root, "taps"), Value::Int(1));

    // A label is not pressed, whatever is under it.
    let label = key(&sc, "label");
    assert!(!sc.accessible_press(&e, label, Time::from_millis(30)));
    assert_eq!(get(&e, root, "clicks"), Value::Int(1));

    // Nor is a disabled button.
    set(&e, find(&e, root, "area"), "enabled", Value::Bool(false));
    frame(&mut sc, &e);
    assert!(!sc.accessible_press(&e, area, Time::from_millis(40)));
    assert_eq!(get(&e, root, "clicks"), Value::Int(1));

    assert!(!sc.accessible_press(&e, NodeKey(987_654), Time::from_millis(50)));
}

#[test]
fn a_new_root_replaces_the_tree() {
    let e = engine();
    let first = load(&e, SAVE);
    let mut sc = active(&e, first);
    assert_eq!(names(&sc), ["Save"]);
    let second = make::<Item>(&e, None);
    let r = make::<Rectangle>(&e, Some(second));
    set_a11y(&e, r, "role", role(Role::CheckBox));
    set_a11y(&e, r, "name", Value::string("Wrap"));
    sc.set_root(&e, second).unwrap();
    frame(&mut sc, &e);
    assert_eq!(outline(&sc), [(AkRole::CheckBox, s("Wrap"), s(""))]);
}

const UNHANDLED: &str = r#"
import QtQuick
Item {
    id: root; objectName: "root"
    width: 300; height: 200
    property string log: ""
    Rectangle {
        objectName: "it"
        width: 80; height: 30
        Accessible.role: Accessible.CheckBox; Accessible.name: "It"
        MouseArea { anchors.fill: parent; onClicked: root.log += "clicked " }
    }
}
"#;

#[test]
fn a_press_nobody_handles_is_a_click() {
    let e = engine();
    let root = load(&e, UNHANDLED);
    let mut sc = active(&e, root);
    let it = sc.accessible_node(find(&e, root, "it")).unwrap();
    assert!(sc.accessible_press(&e, it, Time::from_millis(10)));
    assert_eq!(get(&e, root, "log"), Value::string("clicked "));
}

const ROWS: &str = r#"
import QtQuick
Item {
    id: root
    width: 300; height: 300
    property int rows: 3
    Column {
        Repeater {
            model: root.rows
            Rectangle {
                width: 100; height: 30
                Accessible.role: Accessible.ListItem
                Accessible.name: "Row " + (index + 1)
            }
        }
        Repeater {
            model: 2
            Rectangle {
                width: 100; height: 30
                Accessible.role: Accessible.Button
                Accessible.name: "Button " + (index + 1)
            }
        }
    }
}
"#;

/// `(place, of)` of the node with this name.
fn set_of(s: &QuickScene, role: AkRole, name: &str) -> Option<(usize, usize)> {
    let tree = s.accessibility_tree();
    let node = tree.get(tree.find(role, name).expect(name)).unwrap();
    node.position_in_set.zip(node.size_of_set)
}

#[test]
fn a_row_a_repeater_makes_says_its_place() {
    let e = engine();
    let root = load(&e, ROWS);
    let mut s = active(&e, root);
    assert_eq!(names(&s).len(), 5, "{:?}", outline(&s));
    assert_eq!(set_of(&s, AkRole::ListItem, "Row 1"), Some((1, 3)));
    assert_eq!(set_of(&s, AkRole::ListItem, "Row 3"), Some((3, 3)));
    // A button is not one of a set, whoever made it.
    assert_eq!(set_of(&s, AkRole::Button, "Button 2"), None);

    set(&e, root, "rows", Value::Int(4));
    frame(&mut s, &e);
    assert_eq!(set_of(&s, AkRole::ListItem, "Row 1"), Some((1, 4)));
    assert_eq!(set_of(&s, AkRole::ListItem, "Row 4"), Some((4, 4)));
}

/// What an audit walks the Tab order with: it is told where Tab would go
/// and nothing has the keyboard afterwards that did not have it before.
#[test]
fn asking_where_tab_goes_moves_nothing() {
    use ljos_quick::keys::tab_target;
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 100; height: 100
    Item { objectName: "a"; activeFocusOnTab: true; KeyNavigation.tab: c }
    Item { objectName: "b"; activeFocusOnTab: true }
    Item { id: c; objectName: "c"; activeFocusOnTab: true }
}
"#,
    );
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    let (a, b, c) = (
        find(&e, root, "a"),
        find(&e, root, "b"),
        find(&e, root, "c"),
    );
    assert_eq!(tab_target(&e, root, None, true), Some(a));
    // `KeyNavigation.tab` first, then the order of the document.
    assert_eq!(tab_target(&e, root, Some(a), true), Some(c));
    assert_eq!(tab_target(&e, root, Some(b), true), Some(c));
    assert_eq!(tab_target(&e, root, Some(c), true), Some(a));
    // The way back is the way there turned round, unless it is written.
    assert_eq!(tab_target(&e, root, Some(c), false), Some(a));
    assert_eq!(tab_target(&e, root, Some(b), false), Some(a));
    assert_eq!(tab_target(&e, root, None, false), Some(c));
    assert_eq!(s.active_focus_item(&e), None);
    for item in [a, b, c] {
        assert_eq!(get(&e, item, "activeFocus"), Value::Bool(false));
    }
}
