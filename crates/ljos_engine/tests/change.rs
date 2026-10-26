//! A property's `change` hook: told of every change however it comes, a
//! value written by anyone or a change that reaches the property's
//! binding, once the change has spread and before the binding runs again.

use std::cell::RefCell;

use ljos_engine::{Cx, Engine, ObjectId, PropIndex, QmlObject, Value, WriteKind};

thread_local! {
    static TOLD: RefCell<Vec<(ObjectId, PropIndex)>> = const { RefCell::new(Vec::new()) };
}

fn noted(cx: &Cx<'_>, p: PropIndex) {
    TOLD.with(|t| t.borrow_mut().push((cx.this, p)));
}

fn told() -> Vec<(ObjectId, PropIndex)> {
    TOLD.with(|t| std::mem::take(&mut *t.borrow_mut()))
}

#[derive(Debug, Default, QmlObject)]
#[qml(name = "PlaceGroup", uncreatable)]
pub struct Place {
    #[qml(notify, change = noted)]
    pub at: f64,
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Test", name = "Placed")]
pub struct Placed {
    #[qml(notify, change = noted)]
    pub x: f64,
    #[qml(notify)]
    pub source: f64,
    #[qml(group)]
    pub place: Place,
}

fn placed() -> (Engine, ObjectId) {
    let e = Engine::new();
    e.register_module(ljos_engine::module!("Test" 1.0 => [Placed]));
    let obj = e.create_by_name("Test", "Placed", None).unwrap();
    told();
    (e, obj)
}

#[test]
fn a_value_written_by_anyone_is_told() {
    let (e, obj) = placed();
    e.set(obj, Placed::X, 4.0).unwrap();
    assert_eq!(told(), [(obj, Placed::X.index)]);
    for (v, how) in [
        (1.0, WriteKind::Animation),
        (2.0, WriteKind::Host),
        (3.0, WriteKind::Layout),
    ] {
        e.write(obj, Placed::X.index, Value::Real(v), how).unwrap();
        assert_eq!(told(), [(obj, Placed::X.index)], "{how:?}");
    }
    e.write_derived(obj, Placed::X.index, Value::Real(5.0))
        .unwrap();
    assert_eq!(told(), [(obj, Placed::X.index)]);
}

#[test]
fn a_value_to_a_property_with_a_node_is_told_once_it_changes() {
    let (e, obj) = placed();
    // As a reader would give it one.
    let _node = e.property_node(obj, Placed::X.index).unwrap();
    e.set(obj, Placed::X, 4.0).unwrap();
    assert_eq!(told(), [(obj, Placed::X.index)]);
    e.set(obj, Placed::X, 4.0).unwrap();
    assert!(told().is_empty(), "the value it had is no change");
}

#[test]
fn a_binding_is_told_when_its_source_changes_and_read_when_wanted() {
    let (e, obj) = placed();
    e.bind(obj, Placed::X.index, move |e| {
        Value::Real(e.read(obj, Placed::SOURCE.index).to_number() * 2.0)
    })
    .unwrap();
    assert_eq!(told(), [(obj, Placed::X.index)]);
    // Until it is read, it is not told again: what was told of it stands.
    e.set(obj, Placed::SOURCE, 6.0).unwrap();
    assert!(told().is_empty());
    assert_eq!(e.get(obj, Placed::X).unwrap(), 12.0);
    e.set(obj, Placed::SOURCE, 7.0).unwrap();
    e.set(obj, Placed::SOURCE, 8.0).unwrap();
    // Told once: nothing read it between the two changes.
    assert_eq!(told(), [(obj, Placed::X.index)]);
    assert_eq!(e.get(obj, Placed::X).unwrap(), 16.0);
    e.set(obj, Placed::SOURCE, 9.0).unwrap();
    assert_eq!(told(), [(obj, Placed::X.index)]);
    assert_eq!(e.get(obj, Placed::X).unwrap(), 18.0);
    assert!(told().is_empty(), "its own result is not told again");
}

#[test]
fn a_member_of_a_group_is_told_on_the_object_that_holds_it() {
    let (e, obj) = placed();
    e.write_by_name(obj, "place.at", Value::Real(5.0)).unwrap();
    let at = e.property_index(obj, "place.at").unwrap();
    assert_eq!(told(), [(obj, at)]);
}

#[test]
fn a_property_without_the_hook_is_not_told() {
    let (e, obj) = placed();
    e.set(obj, Placed::SOURCE, 1.0).unwrap();
    assert!(told().is_empty());
}
