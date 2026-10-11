//! `Transition`: its animations run on the object it is run on, and the
//! one who ran it hears when the last of them has ended.

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::*;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{QuickScene, run_transition, running_animation_count, stop_transition};
use ljos_runtime::Time;

const QML: &str = r#"
import QtQuick
Item {
    width: 200; height: 200
    Rectangle { objectName: "box"; width: 10; height: 10 }
    Rectangle { objectName: "other"; width: 10; height: 10 }
    Transition {
        objectName: "t"
        NumberAnimation { property: "x"; to: 100; duration: 100 }
        // Names nothing: it takes its time and moves nothing.
        NumberAnimation { to: 50; duration: 200 }
        NumberAnimation { target: other; property: "y"; to: 30; duration: 50 }
    }
    Transition { objectName: "empty" }
    Transition {
        objectName: "off"; enabled: false
        NumberAnimation { property: "x"; to: 7; duration: 100 }
    }
    property Item other: null
}
"#;

fn rig() -> (Engine, QuickScene, ObjectId) {
    let e = engine();
    // `target: other` wants an id; the property stands in for it here.
    let root = load(&e, &QML.replace("target: other; ", ""));
    let s = scene(&e, root, 200.0, 200.0);
    (e, s, root)
}

fn frames(s: &mut QuickScene, e: &Engine, ms: u64) {
    for _ in 0..4 {
        s.frame(e, Time::from_millis(ms));
    }
}

#[test]
fn a_transition_moves_what_it_is_run_on_and_says_when_it_is_over() {
    let (e, mut s, root) = rig();
    let (t, b) = (find(&e, root, "t"), find(&e, root, "box"));
    let ended = Rc::new(Cell::new(0));
    let n = ended.clone();
    run_transition(&e, t, b, move |_| n.set(n.get() + 1));
    assert!(get(&e, t, "running").to_boolean());
    assert_eq!(running_animation_count(&e), 3);

    frames(&mut s, &e, 0);
    frames(&mut s, &e, 150);
    assert_eq!(num(&e, b, "x"), 100.0);
    assert_eq!(num(&e, b, "y"), 30.0);
    // The longest of them is still under way.
    assert_eq!(ended.get(), 0);
    assert!(get(&e, t, "running").to_boolean());

    frames(&mut s, &e, 250);
    assert_eq!(ended.get(), 1);
    assert!(!get(&e, t, "running").to_boolean());
    assert_eq!(running_animation_count(&e), 0);
    assert_eq!(num(&e, b, "width"), 10.0);
}

#[test]
fn a_stopped_transition_leaves_things_where_they_are_and_says_nothing() {
    let (e, mut s, root) = rig();
    let (t, b) = (find(&e, root, "t"), find(&e, root, "box"));
    let ended = Rc::new(Cell::new(0));
    let n = ended.clone();
    run_transition(&e, t, b, move |_| n.set(n.get() + 1));
    frames(&mut s, &e, 0);
    frames(&mut s, &e, 50);
    stop_transition(&e, t);
    assert_eq!(num(&e, b, "x"), 50.0);
    assert!(!get(&e, t, "running").to_boolean());
    assert_eq!(running_animation_count(&e), 0);
    frames(&mut s, &e, 400);
    assert_eq!(num(&e, b, "x"), 50.0);
    assert_eq!(ended.get(), 0);
}

#[test]
fn a_transition_with_nothing_to_run_is_over_at_once() {
    let (e, _s, root) = rig();
    let b = find(&e, root, "box");
    for name in ["empty", "off"] {
        let ended = Rc::new(Cell::new(0));
        let n = ended.clone();
        run_transition(&e, find(&e, root, name), b, move |_| n.set(n.get() + 1));
        assert_eq!(ended.get(), 1, "{name}");
        assert_eq!(running_animation_count(&e), 0, "{name}");
    }
    assert_eq!(num(&e, b, "x"), 0.0);
}
