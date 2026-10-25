//! Two fingers through `QuickScene::touch_event`: what a PinchArea and a
//! PinchHandler make of them, and what the first finger pressed. The
//! numbers are qml6's, driven with QtTest's touch events.

mod common;

use common::*;
use ljos_engine::Value;
use ljos_quick::touch::{TouchEvent, TouchPhase};
use ljos_quick::{PointerId, QuickScene};
use ljos_render::kurbo;
use ljos_runtime::Time;

fn touch(s: &mut QuickScene, e: &ljos_engine::Engine, id: u64, phase: TouchPhase, x: f64, y: f64) {
    s.touch_event(e, &[point(id, phase, x, y)]);
}

fn point(id: u64, phase: TouchPhase, x: f64, y: f64) -> TouchEvent {
    TouchEvent {
        id: PointerId(id),
        phase,
        pos: kurbo::Point::new(x, y),
        time: Time::ZERO,
    }
}

/// Both fingers move, in one event.
fn both(s: &mut QuickScene, e: &ljos_engine::Engine, a: (f64, f64), b: (f64, f64)) {
    s.touch_event(
        e,
        &[
            point(1, TouchPhase::Move, a.0, a.1),
            point(2, TouchPhase::Move, b.0, b.1),
        ],
    );
}

/// Two fingers `d0` apart about (150, 150), spreading `step` each per
/// move for `steps` moves, then lifted.
fn spread(s: &mut QuickScene, e: &ljos_engine::Engine, d0: f64, step: f64, steps: u32) {
    let h = d0 / 2.0;
    touch(s, e, 1, TouchPhase::Begin, 150.0 - h, 150.0);
    touch(s, e, 2, TouchPhase::Begin, 150.0 + h, 150.0);
    for k in 1..=steps {
        let o = h + step * f64::from(k);
        both(s, e, (150.0 - o, 150.0), (150.0 + o, 150.0));
    }
    touch(
        s,
        e,
        2,
        TouchPhase::End,
        150.0 + h + step * f64::from(steps),
        150.0,
    );
    touch(
        s,
        e,
        1,
        TouchPhase::End,
        150.0 - h - step * f64::from(steps),
        150.0,
    );
}

const AREA: &str = r#"
import QtQuick
Item {
    width: 300; height: 300
    property string log: ""
    property int updates: 0
    Rectangle { objectName: "box"; x: 100; y: 100; width: 100; height: 100 }
    PinchArea {
        objectName: "area"
        anchors.fill: parent
        pinch.target: parent.children[0]
        pinch.minimumScale: 0.1; pinch.maximumScale: 10
        property bool on: pinch.active
        onPinchStarted: (p) => parent.log += "S" + on + p.scale + " "
        onPinchUpdated: (p) => parent.updates++
        onPinchFinished: (p) => parent.log += "F" + p.pointCount + on
        MouseArea {
            x: 0; y: 0; width: 50; height: 50
            onPressed: parent.parent.log += "iP "
            onReleased: parent.parent.log += "iR "
            onCanceled: parent.parent.log += "iC "
        }
    }
}
"#;

#[test]
fn a_pinch_area_scales_its_target_from_where_the_pinch_started() {
    let e = engine();
    let root = load(&e, AREA);
    let box_ = find(&e, root, "box");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    spread(&mut s, &e, 100.0, 1.0, 50);
    let w = e.take_warnings();
    assert!(w.is_empty(), "{w:?}");
    // It starts once the fingers are 10 px further apart, at 110.
    assert_eq!(get(&e, root, "log"), Value::from("Sfalse1 F0true"));
    // One a move from there, and one as a finger is lifted.
    assert_eq!(num(&e, root, "updates"), 46.0);
    assert!((num(&e, box_, "scale") - 200.0 / 110.0).abs() < 1e-9);
}

#[test]
fn a_pinch_area_leaves_its_target_alone_by_default() {
    let e = engine();
    let root = load(
        &e,
        &AREA.replace("pinch.minimumScale: 0.1; pinch.maximumScale: 10", ""),
    );
    let box_ = find(&e, root, "box");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    spread(&mut s, &e, 100.0, 1.0, 50);
    assert_eq!(num(&e, root, "updates"), 46.0);
    assert_eq!(num(&e, box_, "scale"), 1.0);
}

#[test]
fn both_fingers_lifted_at_once_only_finish_a_pinch() {
    let e = engine();
    let root = load(&e, AREA);
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    touch(&mut s, &e, 1, TouchPhase::Begin, 100.0, 150.0);
    touch(&mut s, &e, 2, TouchPhase::Begin, 200.0, 150.0);
    for k in 1..=50 {
        let k = f64::from(k);
        both(&mut s, &e, (100.0 - k, 150.0), (200.0 + k, 150.0));
    }
    s.touch_event(
        &e,
        &[
            point(1, TouchPhase::End, 50.0, 150.0),
            point(2, TouchPhase::End, 250.0, 150.0),
        ],
    );
    assert_eq!(num(&e, root, "updates"), 45.0);
    assert_eq!(get(&e, root, "log"), Value::from("Sfalse1 F0true"));
}

#[test]
fn a_mouse_area_in_a_pinch_area_loses_its_press_to_a_second_finger() {
    let e = engine();
    let root = load(&e, AREA);
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    touch(&mut s, &e, 1, TouchPhase::Begin, 20.0, 20.0);
    touch(&mut s, &e, 1, TouchPhase::End, 20.0, 20.0);
    assert_eq!(get(&e, root, "log"), Value::from("iP iR "));
    set(&e, root, "log", Value::from(""));
    // A second finger beside it leaves it the first, and pinches nothing.
    touch(&mut s, &e, 1, TouchPhase::Begin, 20.0, 20.0);
    touch(&mut s, &e, 2, TouchPhase::Begin, 120.0, 20.0);
    for k in 1..=20 {
        let k = f64::from(k);
        both(&mut s, &e, (20.0 - k, 20.0), (120.0 + k, 20.0));
    }
    assert_eq!(get(&e, root, "log"), Value::from("iP "));
    touch(&mut s, &e, 2, TouchPhase::End, 140.0, 20.0);
    touch(&mut s, &e, 1, TouchPhase::End, 0.0, 20.0);
    // One on it too takes it away.
    set(&e, root, "log", Value::from(""));
    touch(&mut s, &e, 1, TouchPhase::Begin, 10.0, 20.0);
    touch(&mut s, &e, 2, TouchPhase::Begin, 40.0, 20.0);
    assert_eq!(get(&e, root, "log"), Value::from("iP iC "));
}

#[test]
fn a_finger_on_a_pinch_area_does_not_reach_the_mouse_area_under_it() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 300; height: 300
    property string log: ""
    MouseArea { anchors.fill: parent; onPressed: parent.log += "P" }
    PinchArea { anchors.fill: parent }
}
"#,
    );
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    touch(&mut s, &e, 1, TouchPhase::Begin, 20.0, 20.0);
    touch(&mut s, &e, 1, TouchPhase::End, 20.0, 20.0);
    assert_eq!(get(&e, root, "log"), Value::from(""));
}

const HANDLER: &str = r#"
import QtQuick
Item {
    width: 300; height: 300
    property string log: ""
    MouseArea {
        anchors.fill: parent
        onPressed: parent.log += "P "
        onCanceled: parent.log += "C "
    }
    Rectangle {
        objectName: "box"
        x: 50; y: 50; width: 200; height: 200
        PinchHandler { objectName: "pinch" }
    }
}
"#;

#[test]
fn two_fingers_pinch_a_pinch_handler_once_one_went_the_threshold() {
    let e = engine();
    let root = load(&e, HANDLER);
    let box_ = find(&e, root, "box");
    let h = find(&e, root, "pinch");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    // 4 px a finger a move, to 300 apart: it takes over at the third,
    // 124 apart.
    spread(&mut s, &e, 100.0, 4.0, 25);
    assert!((num(&e, h, "persistentScale") - 300.0 / 124.0).abs() < 1e-9);
    assert!((num(&e, box_, "scale") - 300.0 / 124.0).abs() < 1e-9);
    assert_eq!(get(&e, h, "active"), Value::Bool(false));
    assert_eq!(get(&e, root, "log"), Value::from("P C "));
}

#[test]
fn two_fingers_turn_a_pinch_handler_clockwise() {
    let e = engine();
    let root = load(&e, HANDLER);
    let h = find(&e, root, "pinch");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let at = |deg: f64| {
        let (sin, cos) = deg.to_radians().sin_cos();
        (50.0 * cos, 50.0 * sin)
    };
    touch(&mut s, &e, 1, TouchPhase::Begin, 100.0, 150.0);
    touch(&mut s, &e, 2, TouchPhase::Begin, 200.0, 150.0);
    for k in 1..=18 {
        let (dx, dy) = at(5.0 * f64::from(k));
        both(
            &mut s,
            &e,
            (150.0 - dx, 150.0 - dy),
            (150.0 + dx, 150.0 + dy),
        );
    }
    touch(&mut s, &e, 2, TouchPhase::End, 150.0, 200.0);
    touch(&mut s, &e, 1, TouchPhase::End, 150.0, 100.0);
    // A finger 50 px out goes 10 px down past 11.5°: from 15° to 90°.
    assert!((num(&e, h, "persistentRotation") - 75.0).abs() < 1e-9);
}
