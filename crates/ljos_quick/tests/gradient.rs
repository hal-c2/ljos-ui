//! `Gradient` and `GradientStop` on a Rectangle: what the stops written in
//! QML come to when the rectangle is painted. What Qt does was observed on
//! Qt 6.11.2 (qml6); the pictures are in features/items/gradient.feature
//! and crates/ljos_render/tests/gradient.rs.

mod common;

use common::*;
use ljos_engine::{Color, Engine, ObjectId, Value};
use ljos_quick::{GradientPaint, RectanglePaint};
use ljos_render::kurbo::{Point, Rect};
use ljos_render::{Paint, Rgba};

const RED: Color = Color::from_rgba8(255, 0, 0, 255);
const BLUE: Color = Color::from_rgba8(0, 0, 255, 255);
const GREEN: Color = Color::from_rgba8(0, 128, 0, 255);

fn gradient_of(e: &Engine, rect: ObjectId) -> Option<GradientPaint> {
    RectanglePaint::read(e, rect).gradient
}

fn colour(c: Color) -> Value {
    Value::Color(c)
}

/// A rectangle named "box" whose gradient is `gradient`.
fn rectangle(e: &Engine, gradient: &str) -> ObjectId {
    load(
        e,
        &format!(
            "import QtQuick\nRectangle {{\n  objectName: \"box\"\n  width: 40; height: 100\n  \
             color: \"green\"\n  gradient: {gradient}\n}}\n"
        ),
    )
}

#[test]
fn the_types_are_registered_with_qts_defaults() {
    let e = engine();
    let stop = e.create_by_name("QtQuick", "GradientStop", None).unwrap();
    assert_eq!(num(&e, stop, "position"), 0.0);
    assert_eq!(get(&e, stop, "color"), colour(Color::BLACK));
    let g = e.create_by_name("QtQuick", "Gradient", None).unwrap();
    assert_eq!(get(&e, g, "orientation"), Value::Int(2));
    assert!(list(&e, g, "stops").is_empty());
    let r = e.create_by_name("QtQuick", "Rectangle", None).unwrap();
    assert_eq!(get(&e, r, "gradient"), Value::Undefined);
    assert_eq!(gradient_of(&e, r), None);
}

#[test]
fn stops_are_painted_in_the_order_of_their_positions() {
    let e = engine();
    let r = rectangle(
        &e,
        r#"Gradient {
            GradientStop { position: 1; color: "blue" }
            GradientStop { position: 0.5; color: "green" }
            GradientStop { position: 0; color: "red" }
        }"#,
    );
    let g = gradient_of(&e, r).unwrap();
    assert!(!g.horizontal);
    assert_eq!(g.stops, vec![(0.0, RED), (0.5, GREEN), (1.0, BLUE)]);
    assert!(e.take_warnings().is_empty());
}

#[test]
fn of_stops_at_one_position_the_first_written_is_painted() {
    let e = engine();
    let r = rectangle(
        &e,
        r#"Gradient {
            GradientStop { position: 0; color: "red" }
            GradientStop { position: 1; color: "blue" }
            GradientStop { position: 0; color: "green" }
            GradientStop { position: 1; color: "green" }
        }"#,
    );
    assert_eq!(
        gradient_of(&e, r).unwrap().stops,
        vec![(0.0, RED), (1.0, BLUE)]
    );
}

#[test]
fn without_stops_the_colour_is_painted_and_one_stop_is_one_colour() {
    let e = engine();
    let r = rectangle(&e, "Gradient {}");
    assert_eq!(gradient_of(&e, r), None);
    assert!(get(&e, r, "gradient").as_object().is_some());

    let r = rectangle(
        &e,
        r#"Gradient { GradientStop { position: 0.3; color: "blue" } }"#,
    );
    let g = gradient_of(&e, r).unwrap();
    assert_eq!(
        g.paint(Rect::new(0.0, 0.0, 40.0, 100.0)),
        Paint::Solid(Rgba::rgb(0, 0, 255))
    );
}

#[test]
fn the_line_runs_from_the_first_stop_to_the_last() {
    let e = engine();
    let r = rectangle(
        &e,
        r#"Gradient {
            GradientStop { position: -1; color: "red" }
            GradientStop { position: 0.5; color: "green" }
            GradientStop { position: 2; color: "blue" }
        }"#,
    );
    let g = gradient_of(&e, r).unwrap();
    let Paint::LinearGradient(line) = g.paint(Rect::new(10.0, 20.0, 50.0, 120.0)) else {
        panic!("several stops are a gradient")
    };
    // Off the rectangle at both ends, and down its left edge.
    assert_eq!(
        (line.start, line.end),
        (Point::new(10.0, -80.0), Point::new(10.0, 220.0))
    );
    let offsets: Vec<f32> = line.stops.iter().map(|s| s.offset).collect();
    assert_eq!(offsets, vec![0.0, 0.5, 1.0]);
    assert_eq!(line.stops[1].color, Rgba::rgb(0, 128, 0));

    let stops = list(&e, get(&e, r, "gradient").as_object().unwrap(), "stops");
    set(
        &e,
        get(&e, r, "gradient").as_object().unwrap(),
        "orientation",
        Value::Int(1),
    );
    set(&e, stops[0], "position", Value::Real(0.25));
    set(&e, stops[2], "position", Value::Real(0.75));
    let g = gradient_of(&e, r).unwrap();
    assert!(g.horizontal);
    let Paint::LinearGradient(line) = g.paint(Rect::new(10.0, 20.0, 50.0, 120.0)) else {
        panic!("several stops are a gradient")
    };
    assert_eq!(
        (line.start, line.end),
        (Point::new(20.0, 20.0), Point::new(40.0, 20.0))
    );
}

#[test]
fn a_gradient_tells_when_what_it_paints_has_changed() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Rectangle {
    id: root
    objectName: "box"
    property int updates: 0
    property int turns: 0
    gradient: Gradient {
        objectName: "gradient"
        onUpdated: root.updates += 1
        onOrientationChanged: root.turns += 1
        GradientStop { objectName: "top"; position: 0; color: "red" }
        GradientStop { objectName: "bottom"; position: 1; color: "blue" }
    }
}
"#,
    );
    let (g, top) = (find(&e, root, "gradient"), find(&e, root, "top"));
    let counts = || (num(&e, root, "updates"), num(&e, root, "turns"));
    assert_eq!(counts(), (0.0, 0.0), "making a gradient is not an update");

    set(&e, top, "color", colour(GREEN));
    assert_eq!(counts(), (1.0, 0.0));
    assert_eq!(gradient_of(&e, root).unwrap().stops[0], (0.0, GREEN));

    set(&e, top, "position", Value::Real(0.5));
    assert_eq!(counts(), (2.0, 0.0));

    set(&e, g, "orientation", Value::Int(1));
    assert_eq!(counts(), (3.0, 1.0));

    // Where Ljós differs from Qt, which counts every assignment.
    set(&e, top, "color", colour(GREEN));
    assert_eq!(counts(), (3.0, 1.0));

    // The rectangle's own colour is none of the gradient's business.
    set(&e, root, "color", colour(BLUE));
    assert_eq!(counts(), (3.0, 1.0));
    assert!(e.take_warnings().is_empty());
}

#[test]
fn a_rectangle_painted_with_a_gradient_is_painted_again_when_a_stop_changes() {
    let e = engine();
    let root = rectangle(
        &e,
        r#"Gradient {
            GradientStop { objectName: "top"; position: 0; color: "red" }
            GradientStop { position: 1; color: "blue" }
        }"#,
    );
    let mut s = scene(&e, root, 40.0, 100.0);
    frame(&mut s, &e);
    let node = s.node_of(root).unwrap();
    let before = s.render_tree().node(node).unwrap().content().cloned();
    assert!(before.is_some());

    // Nothing changed: the same picture, and nothing to wake up for.
    let idle = frame(&mut s, &e);
    assert_eq!(idle.next_wake, None);
    let same = s.render_tree().node(node).unwrap().content().cloned();
    assert_eq!(before.as_ref().map(|l| l.id()), same.map(|l| l.id()));

    set(&e, find(&e, root, "top"), "color", colour(GREEN));
    frame(&mut s, &e);
    let after = s.render_tree().node(node).unwrap().content().cloned();
    assert_ne!(before.map(|l| l.id()), after.map(|l| l.id()));
}

#[test]
fn nothing_and_null_take_the_gradient_away() {
    let e = engine();
    let r = rectangle(
        &e,
        r#"Gradient { GradientStop { color: "red" } GradientStop { position: 1; color: "blue" } }"#,
    );
    assert!(gradient_of(&e, r).is_some());
    set(&e, r, "gradient", Value::Null);
    assert_eq!(get(&e, r, "gradient"), Value::Null);
    assert_eq!(gradient_of(&e, r), None);
    set(&e, r, "gradient", Value::Undefined);
    assert_eq!(get(&e, r, "gradient"), Value::Undefined);
    assert_eq!(gradient_of(&e, r), None);
    assert!(e.take_warnings().is_empty());
}

#[test]
fn what_is_not_a_gradient_is_refused_with_qts_warning() {
    let e = engine();
    let cases = [
        (
            "Item {}",
            "QML Rectangle: Can't assign Item to gradient property",
        ),
        ("5000", "QML Rectangle: No such gradient preset '5000'"),
        ("\"nope\"", "QML Rectangle: No such gradient preset 'nope'"),
        (
            "true",
            "QML Rectangle: Unknown gradient type. Expected int, string, or Gradient",
        ),
    ];
    for (written, warning) in cases {
        let r = rectangle(&e, written);
        let warnings = e.take_warnings();
        assert!(
            warnings.iter().any(|w| w.contains(warning)),
            "{written}: {warnings:?}"
        );
        assert_eq!(get(&e, r, "gradient"), Value::Undefined, "{written}");
        assert_eq!(gradient_of(&e, r), None, "{written}");
    }
    // The same when it is assigned later.
    let r = rectangle(&e, "undefined");
    assert!(e.take_warnings().is_empty());
    set(&e, r, "gradient", Value::Int(7));
    assert_eq!(
        e.take_warnings(),
        vec!["QML Rectangle: No such gradient preset '7'".to_owned()]
    );
    assert_eq!(get(&e, r, "gradient"), Value::Undefined);
}

#[test]
fn only_stops_go_into_a_gradient() {
    // Qt refuses the file. Ljós warns and leaves the object out; refusing
    // the load is the engine's to do (a backlog scenario in
    // features/items/gradient.feature).
    let e = engine();
    let r = rectangle(
        &e,
        r#"Gradient { GradientStop { color: "red" } Item {} GradientStop { position: 1 } }"#,
    );
    assert!(!e.take_warnings().is_empty());
    let g = get(&e, r, "gradient").as_object().unwrap();
    assert_eq!(list(&e, g, "stops").len(), 2);
    assert_eq!(
        gradient_of(&e, r).unwrap().stops,
        vec![(0.0, RED), (1.0, Color::BLACK)]
    );
}
