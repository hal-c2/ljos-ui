//! `Shape`: what a change costs.
//!
//! What a Shape paints is in `features/shapes/`; here is what a scenario
//! cannot see. A render backend keeps the triangles or the pixels it made
//! for an outline for as long as it is handed the same outline, so the
//! outline of a path has to stay the very same one until something it was
//! made from changes, and a Shape nothing happened to has to keep its
//! display list.

mod common;

use std::sync::Arc;

use common::*;
use ljos_engine::{Color, Engine, ObjectId, Value};
use ljos_quick::QuickScene;
use ljos_render::kurbo::BezPath;
use ljos_render::{Brush, DisplayList, DrawOp, Rgba, ShapeStyle};

const TWO_SHAPES: &str = r#"
import QtQuick
import QtQuick.Shapes
Item {
  width: 200; height: 200
  Shape {
    objectName: "first"
    ShapePath {
      objectName: "outline"
      strokeColor: "red"; strokeWidth: 4; fillColor: "transparent"
      startX: 10; startY: 10
      PathLine { objectName: "corner"; x: 90; y: 10 }
      PathLine { x: 90; y: 90 }
    }
    ShapePath {
      objectName: "patch"
      strokeColor: "transparent"; fillColor: "blue"
      startX: 20; startY: 20
      PathLine { x: 60; y: 20 }
      PathLine { x: 40; y: 60 }
    }
  }
  Shape {
    objectName: "second"
    x: 100
    ShapePath {
      strokeColor: "green"; strokeWidth: 2; fillColor: "transparent"
      PathSvg { path: "M 10 10 C 40 0 60 80 90 90" }
    }
  }
}
"#;

struct Rig {
    e: Engine,
    s: QuickScene,
    root: ObjectId,
}

impl Rig {
    fn new(qml: &str) -> Rig {
        let e = engine();
        let root = load(&e, qml);
        let mut s = scene(&e, root, 200.0, 200.0);
        frame(&mut s, &e);
        assert!(e.take_warnings().is_empty());
        Rig { e, s, root }
    }

    fn obj(&self, name: &str) -> ObjectId {
        find(&self.e, self.root, name)
    }

    /// What the Shape named `name` hands to the backends.
    fn content(&self, name: &str) -> Arc<DisplayList> {
        let node = self
            .s
            .node_of(self.obj(name))
            .expect("the Shape has a node");
        let tree = self.s.render_tree();
        tree.node(node)
            .and_then(|n| n.content())
            .unwrap_or_else(|| panic!("{name} paints nothing"))
            .clone()
    }

    fn frame(&mut self) -> bool {
        frame(&mut self.s, &self.e).changed
    }
}

/// The outlines of a display list in the order they are painted, each with
/// whether it is a stroke and its colour.
fn shapes(list: &DisplayList) -> Vec<(Arc<BezPath>, bool, Rgba)> {
    list.ops()
        .iter()
        .map(|op| match op {
            DrawOp::Shape { path, style, brush } => {
                let Brush::Solid(colour) = brush else {
                    panic!("a plain colour was expected: {brush:?}");
                };
                (
                    path.clone(),
                    matches!(style, ShapeStyle::Stroke(_)),
                    *colour,
                )
            }
            other => panic!("a Shape paints shapes, not {other:?}"),
        })
        .collect()
}

#[test]
fn each_path_is_handed_over_once_for_what_is_painted_of_it() {
    let rig = Rig::new(TWO_SHAPES);
    let first = shapes(&rig.content("first"));
    // The outline has no fill and the patch no stroke.
    assert_eq!(first.len(), 2);
    assert!(first[0].1, "the outline is a stroke");
    assert!(!first[1].1, "the patch is a fill");
    assert!(!Arc::ptr_eq(&first[0].0, &first[1].0));
    assert_eq!(shapes(&rig.content("second")).len(), 1);
}

#[test]
fn a_shape_nothing_happened_to_costs_nothing() {
    let mut rig = Rig::new(TWO_SHAPES);
    let (first, second) = (rig.content("first"), rig.content("second"));
    let rebuilds = rig.s.render_tree().content_rebuilds();
    assert!(!rig.s.needs_frame(), "nothing asks for a frame");
    assert!(
        !rig.frame(),
        "a frame nobody asked for shows the same picture"
    );
    assert!(Arc::ptr_eq(&first, &rig.content("first")));
    assert!(Arc::ptr_eq(&second, &rig.content("second")));
    assert_eq!(rig.s.render_tree().content_rebuilds(), rebuilds);
}

#[test]
fn a_new_colour_keeps_the_outline_and_leaves_the_other_shape_alone() {
    let mut rig = Rig::new(TWO_SHAPES);
    let (first, second) = (rig.content("first"), rig.content("second"));
    let before = shapes(&first);
    let rebuilds = rig.s.render_tree().content_rebuilds();

    let lime = Color::from_rgba8(0, 255, 0, 255);
    set(
        &rig.e,
        rig.obj("outline"),
        "strokeColor",
        Value::Color(lime),
    );
    assert!(rig.s.needs_frame());
    assert!(rig.frame());

    let after = shapes(&rig.content("first"));
    assert_eq!(after.len(), 2);
    assert_ne!(after[0].2, before[0].2, "the new colour is painted");
    // The backend's triangles for both outlines are still good.
    assert!(Arc::ptr_eq(&after[0].0, &before[0].0));
    assert!(Arc::ptr_eq(&after[1].0, &before[1].0));
    assert!(Arc::ptr_eq(&second, &rig.content("second")));
    assert_eq!(rig.s.render_tree().content_rebuilds(), rebuilds + 1);
    assert!(!rig.s.needs_frame(), "and then it is still again");
}

#[test]
fn a_moved_point_makes_one_outline_anew() {
    let mut rig = Rig::new(TWO_SHAPES);
    let second = rig.content("second");
    let before = shapes(&rig.content("first"));
    let rebuilds = rig.s.render_tree().content_rebuilds();

    set(&rig.e, rig.obj("corner"), "x", Value::Real(70.0));
    assert!(rig.frame());

    let after = shapes(&rig.content("first"));
    assert!(!Arc::ptr_eq(&after[0].0, &before[0].0), "the outline moved");
    assert_ne!(after[0].0.elements(), before[0].0.elements());
    assert!(
        Arc::ptr_eq(&after[1].0, &before[1].0),
        "the patch next to it did not"
    );
    assert!(Arc::ptr_eq(&second, &rig.content("second")));
    assert_eq!(rig.s.render_tree().content_rebuilds(), rebuilds + 1);
}

#[test]
fn writing_the_value_a_point_has_makes_nothing_anew() {
    let rig = Rig::new(TWO_SHAPES);
    let first = rig.content("first");
    set(&rig.e, rig.obj("corner"), "x", Value::Real(90.0));
    assert!(!rig.s.needs_frame());
    assert!(Arc::ptr_eq(&first, &rig.content("first")));
}

#[test]
fn a_path_left_of_and_above_its_shape_is_painted() {
    // The Shape has no size and nothing of it lies at or after its origin;
    // the path is in the window all the same.
    let rig = Rig::new(
        r#"
import QtQuick
import QtQuick.Shapes
Item {
  width: 200; height: 200
  Shape {
    objectName: "back"
    x: 150; y: 150
    ShapePath {
      strokeColor: "transparent"; fillColor: "red"
      startX: -100; startY: -100
      PathLine { x: -20; y: -100 }
      PathLine { x: -20; y: -20 }
    }
  }
}
"#,
    );
    let node = rig.s.node_of(rig.obj("back")).unwrap();
    let snapshot = rig.s.snapshot().expect("a frame was made");
    let item = snapshot
        .items
        .iter()
        .find(|i| i.node == node)
        .expect("the Shape is in the frame");
    let list = item.list.as_ref().expect("and has something to paint");
    assert_eq!(shapes(list).len(), 1);
}

/// A Shape at 300, 300 in a window of 200 by 200 with one triangle whose
/// corners are `points` (in the Shape's coordinates).
fn far_shape(points: [(i32, i32); 3]) -> Rig {
    let [a, b, c] = points;
    Rig::new(&format!(
        r#"
import QtQuick
import QtQuick.Shapes
Item {{
  width: 200; height: 200
  Shape {{
    objectName: "far"
    x: 300; y: 300
    ShapePath {{
      strokeColor: "transparent"; fillColor: "red"
      startX: {}; startY: {}
      PathLine {{ x: {}; y: {} }}
      PathLine {{ x: {}; y: {} }}
    }}
  }}
}}
"#,
        a.0, a.1, b.0, b.1, c.0, c.1
    ))
}

fn in_frame(rig: &Rig, name: &str) -> bool {
    let node = rig.s.node_of(rig.obj(name)).unwrap();
    let snapshot = rig.s.snapshot().expect("a frame was made");
    snapshot
        .items
        .iter()
        .any(|i| i.node == node && i.list.is_some())
}

#[test]
fn a_shape_outside_the_window_is_painted_where_its_path_is_inside() {
    let rig = far_shape([(-250, -250), (-150, -250), (-150, -150)]);
    assert!(in_frame(&rig, "far"));
}

#[test]
fn a_shape_whose_path_is_outside_the_window_is_left_out_of_the_frame() {
    let mut rig = far_shape([(10, 10), (90, 10), (90, 90)]);
    assert!(!in_frame(&rig, "far"));
    // It still has its outline for when it comes into view, and holding
    // it asks for no frame.
    assert_eq!(shapes(&rig.content("far")).len(), 1);
    assert!(!rig.s.needs_frame());
    set(&rig.e, rig.obj("far"), "x", Value::Real(100.0));
    set(&rig.e, rig.obj("far"), "y", Value::Real(100.0));
    assert!(rig.frame());
    assert!(in_frame(&rig, "far"));
}
