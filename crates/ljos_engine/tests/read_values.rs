//! [`Engine::read_values`]: several properties of one object read at
//! once, each as [`Engine::read`] reads it, and tracked the same way.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{PropIndex, Value};
use support::{Item, engine};

const ITEM: &str = r#"import QtQuick
Item {
    id: root
    property real w: 50
    Item { objectName: "child"; width: root.w; height: 20; x: 3 }
}"#;

#[test]
fn values_read_together_are_those_read_one_by_one() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(ITEM);
    let child = h.find(root, "child");
    let props = [
        Item::WIDTH.index,
        Item::HEIGHT.index,
        Item::Y.index,
        Item::X.index,
    ];
    let together = e.read_values(child, props).map(|v| v.to_number());
    let alone = props.map(|p| e.read(child, p).to_number());
    assert_eq!(together, alone);
    assert_eq!(together, [50.0, 20.0, 0.0, 3.0]);
    // A property the object does not have is what one read gives.
    let none = PropIndex(9_999);
    let [v] = e.read_values(child, [none]);
    assert_eq!(v, e.read(child, none));
}

#[test]
fn an_effect_reading_together_runs_again_for_each_of_them() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(ITEM);
    let child = h.find(root, "child");
    let runs = Rc::new(Cell::new(0));
    let count = runs.clone();
    e.effect(move |e| {
        let _ = e.read_values(child, [Item::WIDTH.index, Item::HEIGHT.index]);
        count.set(count.get() + 1);
    })
    .unwrap();
    assert_eq!(runs.get(), 1);
    // A field.
    h.set(child, "height", Value::Real(30.0));
    assert_eq!(runs.get(), 2);
    // What a binding gives.
    h.set(root, "w", Value::Real(60.0));
    assert_eq!(runs.get(), 3);
    // Not read: no run.
    h.set(child, "x", Value::Real(9.0));
    assert_eq!(runs.get(), 3);
}

/// The first tracked read of an object's fields is heard as the rest
/// are.
#[test]
fn an_effect_hears_of_the_first_field_it_read() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(ITEM);
    let child = h.find(root, "child");
    let runs = Rc::new(Cell::new(0));
    let count = runs.clone();
    e.effect(move |e| {
        let _ = e.read(child, Item::HEIGHT.index);
        count.set(count.get() + 1);
    })
    .unwrap();
    h.set(child, "height", Value::Real(30.0));
    assert_eq!(runs.get(), 2);
}
