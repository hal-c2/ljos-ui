//! `QtQuick.Layouts` and the positioners: what the scenarios in
//! `features/layouts/` cannot see. How often a layout runs, and the values
//! native code reads from the attached objects.

mod common;

use common::*;
use ljos_engine::{Engine, MetaRef, ObjectId, QmlType, Value};
use ljos_quick::{Layout, Positioner, layout_runs};

fn layout_attached(e: &Engine, o: ObjectId) -> ObjectId {
    e.attached_object(o, &MetaRef::Static(Layout::static_meta()))
        .unwrap()
}

fn positioner_attached(e: &Engine, o: ObjectId) -> ObjectId {
    e.attached_object(o, &MetaRef::Static(Positioner::static_meta()))
        .unwrap()
}

const NESTED: &str = r#"
import QtQuick
import QtQuick.Layouts
Item {
    width: 400; height: 300
    ColumnLayout {
        objectName: "outer"; width: 400; height: 300
        RowLayout {
            objectName: "still"
            Layout.fillWidth: false
            Item { implicitWidth: 30; implicitHeight: 10 }
        }
        RowLayout {
            objectName: "inner"
            Item { objectName: "kid"; implicitWidth: 30; implicitHeight: 10; Layout.preferredWidth: 30 }
            Item { objectName: "rest"; implicitWidth: 30; implicitHeight: 10; Layout.fillWidth: true }
        }
    }
    RowLayout {
        objectName: "apart"
        Item { implicitWidth: 30; implicitHeight: 10 }
    }
}
"#;

fn runs(e: &Engine, root: ObjectId) -> [u64; 4] {
    ["outer", "still", "inner", "apart"].map(|n| layout_runs(e, find(e, root, n)))
}

#[test]
fn every_layout_runs_once_for_a_new_scene() {
    let e = engine();
    let root = load(&e, NESTED);
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(runs(&e, root), [1, 1, 1, 1]);
    assert_eq!(num(&e, find(&e, root, "rest"), "width"), 365.0);
}

#[test]
fn an_idle_scene_runs_no_layout() {
    let e = engine();
    let root = load(&e, NESTED);
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    let before = runs(&e, root);
    for _ in 0..3 {
        frame(&mut s, &e);
    }
    assert_eq!(runs(&e, root), before);
}

#[test]
fn a_new_preferred_size_runs_the_layout_and_its_ancestors_only() {
    let e = engine();
    let root = load(&e, NESTED);
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    let before = runs(&e, root);

    let kid = find(&e, root, "kid");
    set(
        &e,
        layout_attached(&e, kid),
        "preferredWidth",
        Value::Real(60.0),
    );
    frame(&mut s, &e);

    assert_eq!(num(&e, kid, "width"), 60.0);
    assert_eq!(num(&e, find(&e, root, "rest"), "x"), 65.0);
    assert_eq!(num(&e, find(&e, root, "rest"), "width"), 335.0);
    let after = runs(&e, root);
    // The layout the child is in and the one around it: once each. The
    // sibling layout kept its cell and the one elsewhere saw nothing.
    assert_eq!(
        [
            after[0] - before[0],
            after[1] - before[1],
            after[2] - before[2],
            after[3] - before[3],
        ],
        [1, 0, 1, 0]
    );

    frame(&mut s, &e);
    assert_eq!(runs(&e, root), after);
}

#[test]
fn a_new_size_runs_the_layout_but_measures_nothing_twice() {
    let e = engine();
    let root = load(&e, NESTED);
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    let before = runs(&e, root);

    set(&e, find(&e, root, "outer"), "width", Value::Real(500.0));
    frame(&mut s, &e);

    assert_eq!(num(&e, find(&e, root, "rest"), "width"), 465.0);
    let after = runs(&e, root);
    // "still" does not fill, so it kept its size.
    assert_eq!(after[0] - before[0], 1);
    assert_eq!(after[1] - before[1], 0);
    assert_eq!(after[2] - before[2], 1);
    assert_eq!(after[3] - before[3], 0);
}

#[test]
fn a_layout_keeps_a_child_at_the_size_it_gave() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Layouts
RowLayout {
    Item { objectName: "a"; implicitWidth: 30; height: 10; Layout.preferredWidth: 30 }
    Item { objectName: "b"; width: 20; height: 10 }
}
"#,
    );
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    let a = find(&e, root, "a");
    assert_eq!(num(&e, a, "width"), 30.0);

    // The item's width follows its implicit width until the layout says
    // otherwise, which it does: the preferred width is still 30 (Qt 6.11).
    set(&e, a, "implicitWidth", Value::Real(50.0));
    frame(&mut s, &e);
    assert_eq!(num(&e, a, "width"), 30.0);
    assert_eq!(num(&e, find(&e, root, "b"), "x"), 35.0);
}

#[test]
fn a_hidden_layout_still_lays_out_and_remembers_who_was_hidden() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Layouts
Item {
    RowLayout {
        objectName: "row"
        Item { objectName: "a"; width: 30; height: 10 }
        Item { objectName: "gone"; width: 30; height: 10; visible: false }
        Item { objectName: "b"; width: 20; height: 10 }
    }
}
"#,
    );
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    let row = find(&e, root, "row");
    assert_eq!(num(&e, row, "implicitWidth"), 55.0);

    // Hidden, every child reads invisible; the two that were shown keep
    // their cells.
    set(&e, row, "visible", Value::Bool(false));
    set(&e, row, "spacing", Value::Real(1.0));
    frame(&mut s, &e);
    assert_eq!(num(&e, row, "implicitWidth"), 51.0);
    assert_eq!(num(&e, find(&e, root, "b"), "x"), 31.0);
}

#[test]
fn a_stack_hidden_from_the_start_shows_its_page_when_it_appears() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Layouts
Item {
    StackLayout {
        objectName: "stack"; visible: false; width: 100; height: 50; currentIndex: 1
        Item { objectName: "a" }
        Item { objectName: "b" }
    }
}
"#,
    );
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    let stack = find(&e, root, "stack");
    assert_eq!(get(&e, stack, "count"), Value::Int(2));

    set(&e, stack, "visible", Value::Bool(true));
    frame(&mut s, &e);
    let (a, b) = (find(&e, root, "a"), find(&e, root, "b"));
    assert!(!get(&e, a, "visible").to_boolean());
    assert!(get(&e, b, "visible").to_boolean());
    assert_eq!(rect(&e, b), (0.0, 0.0, 100.0, 50.0));
}

const ROW: &str = r#"
import QtQuick
Row {
    objectName: "row"
    Item { objectName: "a"; width: 10; height: 10 }
    Item { objectName: "hidden"; width: 10; height: 10; visible: false }
    Item { objectName: "empty"; height: 10 }
    Item { objectName: "b"; width: 10; height: 10 }
    Item { objectName: "c"; width: 10; height: 10 }
}
"#;

fn place(e: &Engine, o: ObjectId) -> (i64, bool, bool) {
    let a = positioner_attached(e, o);
    (
        get(e, a, "index").to_number() as i64,
        get(e, a, "isFirstItem").to_boolean(),
        get(e, a, "isLastItem").to_boolean(),
    )
}

#[test]
fn a_positioner_tells_each_child_its_place() {
    let e = engine();
    let root = load(&e, ROW);
    let item = |n: &str| find(&e, root, n);
    // Asked before the first layout: no place yet.
    for n in ["a", "hidden", "empty", "b"] {
        assert_eq!(place(&e, item(n)), (-1, false, false));
    }
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);

    assert_eq!(place(&e, item("a")), (0, true, false));
    assert_eq!(place(&e, item("b")), (1, false, false));
    // Children that are not positioned have no place.
    assert_eq!(place(&e, item("hidden")), (-1, false, false));
    assert_eq!(place(&e, item("empty")), (-1, false, false));

    // "c" asks only now: the positioner tells it in the next frame.
    positioner_attached(&e, item("c"));
    frame(&mut s, &e);
    assert_eq!(place(&e, item("c")), (2, false, true));
}

#[test]
fn a_child_that_appears_moves_the_ones_after_it() {
    let e = engine();
    let root = load(&e, ROW);
    let item = |n: &str| find(&e, root, n);
    for n in ["a", "hidden", "b", "c"] {
        positioner_attached(&e, item(n));
    }
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(place(&e, item("c")), (2, false, true));

    set(&e, item("hidden"), "visible", Value::Bool(true));
    frame(&mut s, &e);
    assert_eq!(place(&e, item("hidden")), (1, false, false));
    assert_eq!(place(&e, item("b")), (2, false, false));
    assert_eq!(place(&e, item("c")), (3, false, true));

    set(&e, item("a"), "visible", Value::Bool(false));
    frame(&mut s, &e);
    assert_eq!(place(&e, item("a")), (-1, false, false));
    assert_eq!(place(&e, item("hidden")), (0, true, false));
}

#[test]
fn the_grid_and_flow_tell_their_children_too() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Grid {
        columns: 2
        Item { objectName: "g1"; width: 10; height: 10 }
        Item { objectName: "g2"; width: 10; height: 10 }
        Item { objectName: "g3"; width: 10; height: 10 }
    }
    Flow {
        width: 15
        Item { objectName: "f1"; width: 10; height: 10 }
        Item { objectName: "f2"; width: 10; height: 10 }
    }
}
"#,
    );
    let item = |n: &str| find(&e, root, n);
    for n in ["g1", "g2", "g3", "f1", "f2"] {
        positioner_attached(&e, item(n));
    }
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(place(&e, item("g1")), (0, true, false));
    assert_eq!(place(&e, item("g3")), (2, false, true));
    assert_eq!(place(&e, item("f1")), (0, true, false));
    assert_eq!(place(&e, item("f2")), (1, false, true));
    assert_eq!(rect(&e, item("f2")), (0.0, 10.0, 10.0, 10.0));
    assert_eq!(rect(&e, item("g3")), (0.0, 10.0, 10.0, 10.0));
}
