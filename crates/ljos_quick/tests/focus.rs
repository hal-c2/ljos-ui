//! Focus: who is told when active focus moves, the Tab order being kept
//! until the tree changes, window activation, and an idle scene staying
//! idle. What focus looks like from QML is in `features/input/`.

mod common;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::focus::{FocusReason, force_active_focus, tab_order_builds};
use ljos_quick::keys::key;
use ljos_quick::{KeyEvent, QuickScene};

/// Two scopes and a bystander; every item counts its `activeFocus`
/// notifications in `n`.
const SCOPES: &str = r#"
import QtQuick
Item {
    objectName: "root"
    property int n: 0
    onActiveFocusChanged: n++
    FocusScope {
        objectName: "a"
        focus: true
        property int n: 0
        onActiveFocusChanged: n++
        Item { objectName: "a1"; focus: true; property int n: 0; onActiveFocusChanged: n++ }
        Item { objectName: "a2"; property int n: 0; onActiveFocusChanged: n++ }
    }
    FocusScope {
        objectName: "b"
        property int n: 0
        onActiveFocusChanged: n++
        Item { objectName: "b1"; focus: true; property int n: 0; onActiveFocusChanged: n++ }
        Item { objectName: "b2"; property int n: 0; onActiveFocusChanged: n++ }
    }
    Item { objectName: "c"; property int n: 0; onActiveFocusChanged: n++ }
}
"#;

const NAMES: [&str; 8] = ["root", "a", "a1", "a2", "b", "b1", "b2", "c"];

fn ready(e: &Engine, source: &str) -> (ObjectId, QuickScene) {
    let root = load(e, source);
    let mut s = scene(e, root, 200.0, 200.0);
    frame(&mut s, e);
    (root, s)
}

/// How often each of [`NAMES`] heard `activeFocusChanged`.
fn counts(e: &Engine, root: ObjectId) -> [i64; 8] {
    NAMES.map(|n| num(e, find(e, root, n), "n") as i64)
}

fn active(e: &Engine, root: ObjectId) -> Vec<&'static str> {
    NAMES
        .into_iter()
        .filter(|n| get(e, find(e, root, n), "activeFocus").to_boolean())
        .collect()
}

fn name(e: &Engine, o: Option<ObjectId>) -> String {
    o.map_or_else(
        || "none".to_owned(),
        |o| get(e, o, "objectName").to_js_string().to_string(),
    )
}

#[test]
fn active_focus_is_told_only_along_the_old_and_the_new_chain() {
    let e = engine();
    let (root, mut s) = ready(&e, SCOPES);
    assert_eq!(active(&e, root), ["a", "a1"]);
    //                          root a a1 a2 b b1 b2 c
    assert_eq!(counts(&e, root), [0, 1, 1, 0, 0, 0, 0, 0]);

    // Inside one scope: the scope keeps active focus and hears nothing.
    set(&e, find(&e, root, "a2"), "focus", Value::Bool(true));
    frame(&mut s, &e);
    assert_eq!(active(&e, root), ["a", "a2"]);
    assert_eq!(counts(&e, root), [0, 1, 2, 1, 0, 0, 0, 0]);

    // To the other scope: each item of both chains hears once, the
    // bystanders (`a1`, `b2`, `c`, the root) not at all.
    set(&e, find(&e, root, "b"), "focus", Value::Bool(true));
    frame(&mut s, &e);
    assert_eq!(active(&e, root), ["b", "b1"]);
    assert_eq!(counts(&e, root), [0, 2, 2, 2, 1, 1, 0, 0]);

    // Asking for what is already so tells nobody.
    force_active_focus(&e, find(&e, root, "b1"), FocusReason::Other);
    frame(&mut s, &e);
    assert_eq!(counts(&e, root), [0, 2, 2, 2, 1, 1, 0, 0]);
    assert!(e.take_warnings().is_empty());
}

#[test]
fn focus_inside_a_scope_without_focus_changes_no_active_focus() {
    let e = engine();
    let (root, mut s) = ready(&e, SCOPES);
    let before = counts(&e, root);
    set(&e, find(&e, root, "b2"), "focus", Value::Bool(true));
    frame(&mut s, &e);
    assert_eq!(active(&e, root), ["a", "a1"]);
    assert_eq!(counts(&e, root), before);
    // The scope remembers it for when it gets focus.
    assert!(!get(&e, find(&e, root, "b1"), "focus").to_boolean());
    set(&e, find(&e, root, "b"), "focus", Value::Bool(true));
    frame(&mut s, &e);
    assert_eq!(active(&e, root), ["b", "b2"]);
}

#[test]
fn active_focus_follows_the_window() {
    let e = engine();
    let (root, mut s) = ready(&e, SCOPES);
    assert!(s.is_window_active(&e));
    assert_eq!(name(&e, s.active_focus_item(&e)), "a1");

    s.set_window_active(&e, false);
    assert!(active(&e, root).is_empty());
    assert_eq!(name(&e, s.active_focus_item(&e)), "none");
    // `focus` is what the item asked for and stays.
    assert!(get(&e, find(&e, root, "a1"), "focus").to_boolean());
    assert_eq!(counts(&e, root), [0, 2, 2, 0, 0, 0, 0, 0]);
    // No keys for a window that does not have the keyboard.
    assert!(!s.key_event(&e, &KeyEvent::press(key::TAB)));

    s.set_window_active(&e, true);
    assert_eq!(active(&e, root), ["a", "a1"]);
    assert_eq!(counts(&e, root), [0, 3, 3, 0, 0, 0, 0, 0]);
    // Saying it twice is not a change.
    s.set_window_active(&e, true);
    assert_eq!(counts(&e, root), [0, 3, 3, 0, 0, 0, 0, 0]);
}

#[test]
fn a_destroyed_focus_item_leaves_no_active_focus_behind() {
    let e = engine();
    let (root, mut s) = ready(&e, SCOPES);
    let a1 = find(&e, root, "a1");
    assert_eq!(s.active_focus_item(&e), Some(a1));
    e.destroy_now(a1);
    frame(&mut s, &e);
    assert_ne!(s.active_focus_item(&e), Some(a1));
    // Keys still have somewhere to go.
    s.key_event(&e, &KeyEvent::press(key::A).with_text("a"));
    s.key_event(&e, &KeyEvent::release(key::A).with_text("a"));
    assert!(get(&e, find(&e, root, "a"), "activeFocus").to_boolean());
}

const TABS: &str = r#"
import QtQuick
Item {
    objectName: "root"
    Item { objectName: "t1"; activeFocusOnTab: true; focus: true }
    Item {
        objectName: "group"
        Item { objectName: "t2"; activeFocusOnTab: true }
        Item { objectName: "plain" }
    }
    Item { objectName: "t3"; activeFocusOnTab: true }
}
"#;

fn tab(s: &mut QuickScene, e: &Engine) -> String {
    assert!(s.key_event(e, &KeyEvent::press(key::TAB).with_text("\t")));
    s.key_event(e, &KeyEvent::release(key::TAB).with_text("\t"));
    name(e, s.active_focus_item(e))
}

#[test]
fn the_tab_order_is_made_once_and_again_only_after_the_tree_changed() {
    let e = engine();
    let (root, mut s) = ready(&e, TABS);
    // Nobody pressed Tab yet.
    assert_eq!(tab_order_builds(&e, root), 0);
    assert_eq!(tab(&mut s, &e), "t2");
    assert_eq!(tab(&mut s, &e), "t3");
    assert_eq!(tab(&mut s, &e), "t1");
    assert_eq!(tab_order_builds(&e, root), 1);

    // What the order does not depend on leaves it alone.
    set(&e, find(&e, root, "t2"), "x", Value::Real(40.0));
    set(&e, find(&e, root, "plain"), "width", Value::Real(10.0));
    frame(&mut s, &e);
    assert_eq!(tab(&mut s, &e), "t2");
    assert_eq!(tab_order_builds(&e, root), 1);

    // Hiding a stop marks the order stale; the next Tab makes it again.
    set(&e, find(&e, root, "t3"), "visible", Value::Bool(false));
    frame(&mut s, &e);
    assert_eq!(tab_order_builds(&e, root), 1);
    assert_eq!(tab(&mut s, &e), "t1");
    assert_eq!(tab_order_builds(&e, root), 2);

    // So does a new stop.
    set(
        &e,
        find(&e, root, "plain"),
        "activeFocusOnTab",
        Value::Bool(true),
    );
    assert_eq!(tab(&mut s, &e), "t2");
    assert_eq!(tab(&mut s, &e), "plain");
    assert_eq!(tab_order_builds(&e, root), 3);

    // And a parent that goes away with its stops.
    e.destroy_now(find(&e, root, "group"));
    frame(&mut s, &e);
    assert_eq!(tab(&mut s, &e), "t1");
    assert_eq!(tab(&mut s, &e), "t1");
    assert_eq!(tab_order_builds(&e, root), 4);
    assert!(e.take_warnings().is_empty());
}

#[test]
fn a_scene_with_focus_is_idle_between_events() {
    let e = engine();
    let (_root, mut s) = ready(&e, TABS);
    let out = frame(&mut s, &e);
    assert!(!out.changed);
    assert_eq!(out.next_wake, None);

    tab(&mut s, &e);
    frame(&mut s, &e);
    let out = frame(&mut s, &e);
    assert!(!out.changed);
    assert_eq!(out.next_wake, None);

    s.set_window_active(&e, false);
    frame(&mut s, &e);
    assert_eq!(frame(&mut s, &e).next_wake, None);
}
