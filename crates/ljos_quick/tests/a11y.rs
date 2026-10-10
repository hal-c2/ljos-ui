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
    Rectangle { objectName: "text"; Accessible.role: Accessible.StaticText }
    Rectangle {
        objectName: "forced"
        Accessible.role: Accessible.StaticText; Accessible.focusable: true
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
    assert!(!focusable("text"));
    assert!(focusable("forced"));
    assert!(!focusable("off"));

    // The default follows a role change; a set value does not.
    set_a11y(&e, find(&e, root, "text"), "role", role(Role::CheckBox));
    set_a11y(&e, find(&e, root, "off"), "role", role(Role::CheckBox));
    assert!(focusable("text"));
    assert!(!focusable("off"));

    let sc = active(&e, root);
    let node = |name: &str| {
        let key = sc.accessible_node(find(&e, root, name)).unwrap();
        sc.accessibility_tree().get(key).unwrap().focusable
    };
    assert!(node("button"));
    assert!(node("forced"));
    assert!(!node("off"));
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
