//! A property's `given` hook: told when an author gives the property a
//! value, a binding or an override, and not when a binding, an animation
//! or the toolkit writes it as it runs.

use std::cell::RefCell;

use ljos_engine::{
    Cx, Engine, ObjectId, OverrideContent, PropIndex, QmlObject, RestoreMode, Value, WriteKind,
};

thread_local! {
    static TOLD: RefCell<Vec<(ObjectId, PropIndex, Value)>> = const { RefCell::new(Vec::new()) };
}

/// Notes what the property holds as it is told, which says the hook runs
/// once the value is in place.
fn noted(cx: &Cx<'_>, p: PropIndex) {
    let v = cx.engine.read(cx.this, p);
    TOLD.with(|t| t.borrow_mut().push((cx.this, p, v)));
}

fn told() -> Vec<(ObjectId, PropIndex, Value)> {
    TOLD.with(|t| std::mem::take(&mut *t.borrow_mut()))
}

#[derive(Debug, Default, QmlObject)]
#[qml(name = "PinGroup", uncreatable)]
pub struct Pin {
    #[qml(notify, given = noted)]
    pub at: f64,
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Test", name = "Pinned")]
pub struct Pinned {
    #[qml(notify, given = noted)]
    pub line: f64,
    #[qml(notify)]
    pub source: f64,
    #[qml(group)]
    pub pin: Pin,
}

fn pinned() -> (Engine, ObjectId) {
    let e = Engine::new();
    e.register_module(ljos_engine::module!("Test" 1.0 => [Pinned]));
    let obj = e.create_by_name("Test", "Pinned", None).unwrap();
    told();
    (e, obj)
}

#[test]
fn a_value_written_is_told_once_it_is_held() {
    let (e, obj) = pinned();
    e.set(obj, Pinned::LINE, 4.0).unwrap();
    assert_eq!(told(), [(obj, Pinned::LINE.index, Value::Real(4.0))]);
}

#[test]
fn a_binding_is_told_when_it_is_put_on_and_not_as_it_runs() {
    let (e, obj) = pinned();
    e.bind(obj, Pinned::LINE.index, move |e| {
        e.read(obj, Pinned::SOURCE.index)
    })
    .unwrap();
    assert_eq!(told().len(), 1);
    e.set(obj, Pinned::SOURCE, 7.0).unwrap();
    assert_eq!(e.get(obj, Pinned::LINE).unwrap(), 7.0);
    assert!(told().is_empty());
}

#[test]
fn an_override_is_told_and_its_removal_is_not() {
    let (e, obj) = pinned();
    let id = e
        .push_override(
            obj,
            Pinned::LINE.index,
            OverrideContent::Value(Value::Real(2.0)),
        )
        .unwrap();
    assert_eq!(told(), [(obj, Pinned::LINE.index, Value::Real(2.0))]);
    e.remove_override(obj, Pinned::LINE.index, id, RestoreMode::BindingOrValue)
        .unwrap();
    assert!(told().is_empty());
}

#[test]
fn what_an_animation_or_the_toolkit_writes_is_not_told() {
    let (e, obj) = pinned();
    e.write(
        obj,
        Pinned::LINE.index,
        Value::Real(1.0),
        WriteKind::Animation,
    )
    .unwrap();
    e.write(obj, Pinned::LINE.index, Value::Real(2.0), WriteKind::Host)
        .unwrap();
    e.write(obj, Pinned::LINE.index, Value::Real(3.0), WriteKind::Layout)
        .unwrap();
    assert!(told().is_empty());
}

#[test]
fn a_member_of_a_group_is_told_on_the_object_that_holds_it() {
    let (e, obj) = pinned();
    e.write_by_name(obj, "pin.at", Value::Real(5.0)).unwrap();
    let at = e.property_index(obj, "pin.at").unwrap();
    assert_eq!(told(), [(obj, at, Value::Real(5.0))]);
}

#[test]
fn a_property_without_the_hook_is_not_told() {
    let (e, obj) = pinned();
    e.set(obj, Pinned::SOURCE, 1.0).unwrap();
    assert!(told().is_empty());
}
