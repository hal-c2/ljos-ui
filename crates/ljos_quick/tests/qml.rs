//! Compiled QML on the real QtQuick types: the visual tree that nested
//! objects build, `parent` assignments, and the scene driving QML timers.

mod common;

use std::rc::Rc;
use std::sync::Arc;

use common::*;
use ljos_engine::Value;
use ljos_quick::{Item, QuickScene};
use ljos_render::kurbo;
use ljos_runtime::{Time, VirtualClock};

#[test]
fn nested_items_become_children_and_other_objects_resources() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    objectName: "root"
    Item { objectName: "a"; Rectangle { objectName: "a1" } }
    QtObject { objectName: "res" }
    Rectangle { objectName: "b" }
}
"#,
    );
    let (a, a1, res, b) = (
        find(&e, root, "a"),
        find(&e, root, "a1"),
        find(&e, root, "res"),
        find(&e, root, "b"),
    );
    assert_eq!(list(&e, root, "children"), vec![a, b]);
    assert_eq!(list(&e, root, "resources"), vec![res]);
    assert_eq!(list(&e, root, "data"), vec![a, res, b]);
    assert_eq!(list(&e, a, "children"), vec![a1]);
    assert_eq!(e.visual_parent(a1), Some(a));
    assert_eq!(get(&e, a1, "parent").as_object(), Some(a));
    assert_eq!(get(&e, res, "parent").as_object(), None);
    assert!(e.take_warnings().is_empty());

    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    let root_node = s.node_of(root).unwrap();
    let kids: Vec<_> = s.render_tree().node(root_node).unwrap().children().to_vec();
    assert_eq!(kids, vec![s.node_of(a).unwrap(), s.node_of(b).unwrap()]);
    assert!(s.node_of(res).is_none());
}

#[test]
fn assigning_parent_from_javascript_moves_the_item() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    id: root
    Item { id: a; objectName: "a" }
    Item { id: b; objectName: "b"; x: 10 }
    function move() { a.parent = b }
}
"#,
    );
    let (a, b) = (find(&e, root, "a"), find(&e, root, "b"));
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    e.invoke_by_name(root, "move", &[]).unwrap();
    assert_eq!(e.visual_parent(a), Some(b));
    assert_eq!(list(&e, root, "children"), vec![b]);
    assert_eq!(list(&e, b, "children"), vec![a]);
    frame(&mut s, &e);
    let b_node = s.node_of(b).unwrap();
    assert_eq!(
        s.render_tree().node(b_node).unwrap().children(),
        &[s.node_of(a).unwrap()]
    );
    assert!(e.take_warnings().is_empty());
}

#[test]
fn a_parent_binding_places_the_item() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    property bool second: false
    Item { id: a; objectName: "a" }
    Item { id: b; objectName: "b" }
    Item { objectName: "k"; parent: second ? b : a }
}
"#,
    );
    let (a, b, k) = (
        find(&e, root, "a"),
        find(&e, root, "b"),
        find(&e, root, "k"),
    );
    assert_eq!(e.visual_parent(k), Some(a));
    e.write_by_name(root, "second", Value::Bool(true)).unwrap();
    assert_eq!(e.visual_parent(k), Some(b));
    assert_eq!(list(&e, a, "children"), vec![]);
    assert_eq!(list(&e, b, "children"), vec![k]);
}

/// Within the scheduler's slack after `ms`.
fn near(t: Time, ms: u64) -> bool {
    t >= Time::from_millis(ms) && t < Time::from_millis(ms + 20)
}

#[test]
fn the_frame_runs_qml_timers_and_reports_their_deadline() {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    property int ticks: 0
    Timer { interval: 100; running: true; repeat: true; onTriggered: ticks++ }
}
"#,
    );
    let mut s = QuickScene::new(kurbo::Size::new(10.0, 10.0));
    s.set_root(&e, root).unwrap();
    // The host's clock is offset from the engine's: deadlines are
    // translated, not copied.
    let host = Time::from_millis(5000);
    let out = s.frame(&e, host);
    // Timers may be woken a little late (coalescing slack), never early.
    let wake = out.next_wake.unwrap();
    assert!(near(wake, 5100), "{wake:?}");
    clock.advance(wake - host);
    let out = s.frame(&e, wake);
    assert_eq!(num(&e, root, "ticks"), 1.0);
    let wake = out.next_wake.unwrap();
    assert!(near(wake, 5200), "{wake:?}");
}

#[test]
fn starting_a_timer_outside_a_frame_wakes_the_host() {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    property alias running: t.running
    Timer { id: t; interval: 250 }
}
"#,
    );
    let mut s = QuickScene::new(kurbo::Size::new(10.0, 10.0));
    s.set_root(&e, root).unwrap();
    let out = s.frame(&e, Time::ZERO);
    assert_eq!(out.next_wake, None);
    let woke = Rc::new(std::cell::Cell::new(0));
    let w = Rc::clone(&woke);
    s.set_waker(move || w.set(w.get() + 1));
    assert!(!s.needs_frame());
    e.write_by_name(root, "running", Value::Bool(true)).unwrap();
    assert!(s.needs_frame());
    assert_eq!(woke.get(), 1);
    let out = s.frame(&e, Time::ZERO);
    assert!(out.next_wake.is_some_and(|w| near(w, 250)), "{out:?}");
    // Re-arming inside the frame does not wake the host again.
    assert_eq!(woke.get(), 1);
}

#[test]
fn a_new_scale_factor_asks_for_a_frame() {
    let e = engine();
    let root = make::<ljos_quick::Rectangle>(&e, None);
    geom(&e, root, 0.0, 0.0, 10.0, 10.0);
    let mut s = scene(&e, root, 10.0, 10.0);
    frame(&mut s, &e);
    assert!(!s.needs_frame());
    s.set_scale_factor(1.0);
    assert!(!s.needs_frame(), "the same factor is no change");
    s.set_scale_factor(2.0);
    assert!(s.needs_frame());
    let out = frame(&mut s, &e);
    assert!(out.changed, "display lists are rebuilt at the new scale");
}

#[test]
fn scene_warnings_go_to_the_engine_log() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Item { id: a; Item { id: inner } }
    Item { anchors.left: inner.right }
}
"#,
    );
    let _s = scene(&e, root, 100.0, 100.0);
    let log = e.take_log();
    assert!(
        log.iter()
            .any(|l| l.category == ljos_quick::ANCHORS_CATEGORY
                && l.message.ends_with("isn't a parent or sibling.")),
        "{log:?}"
    );
    let _ = Item::X;
}
