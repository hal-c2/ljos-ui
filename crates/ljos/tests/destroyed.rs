//! Rust hears an object's destruction through its `destroyed` signal,
//! which every object has, as every `QObject` does.

use std::cell::RefCell;
use std::rc::Rc;

use ljos::prelude::*;

fn item(engine: &Engine, parent: Option<ObjectId>) -> ObjectId {
    engine.create_by_name("QtQuick", "Item", parent).unwrap()
}

#[test]
fn destroyed_runs_once_with_the_object_still_there() {
    let engine = ljos::engine();
    let obj = item(&engine, None);
    engine.write_by_name(obj, "width", Value::from(7)).unwrap();
    let heard = Rc::new(RefCell::new(Vec::new()));
    let h = Rc::clone(&heard);
    engine
        .connect_by_name(obj, "destroyed", move |e, args| {
            let Some(Value::Object(o)) = args.first() else {
                panic!("no object: {args:?}");
            };
            h.borrow_mut()
                .push((*o, e.is_alive(*o), e.read_by_name(*o, "width").to_number()));
        })
        .unwrap();

    assert!(engine.destroy_now(obj));
    assert!(!engine.destroy_now(obj));

    assert_eq!(*heard.borrow(), [(obj, true, 7.0)]);
    assert!(!engine.is_alive(obj));
}

#[test]
fn an_owner_is_told_before_what_it_owns() {
    let engine = ljos::engine();
    let parent = item(&engine, None);
    let child = item(&engine, Some(parent));
    let order = Rc::new(RefCell::new(Vec::new()));
    for (obj, name) in [(child, "child"), (parent, "parent")] {
        let o = Rc::clone(&order);
        engine
            .connect_by_name(obj, "destroyed", move |_, _| o.borrow_mut().push(name))
            .unwrap();
    }
    engine.destroy(parent);
    engine.process_deferred_deletes();
    assert_eq!(*order.borrow(), ["parent", "child"]);
}

#[test]
fn a_disconnected_handler_does_not_run() {
    let engine = ljos::engine();
    let obj = item(&engine, None);
    let heard = Rc::new(RefCell::new(0));
    let h = Rc::clone(&heard);
    let id = engine
        .connect(obj, SignalIndex::DESTROYED, move |_, _| {
            *h.borrow_mut() += 1
        })
        .unwrap();
    assert!(engine.disconnect(id));
    engine.destroy_now(obj);
    assert_eq!(*heard.borrow(), 0);
}

#[test]
fn a_connection_to_a_receiver_ends_with_the_receiver() {
    let engine = ljos::engine();
    let watched = item(&engine, None);
    let receiver = item(&engine, None);
    let heard = Rc::new(RefCell::new(0));
    let h = Rc::clone(&heard);
    engine
        .connect_to(watched, SignalIndex::DESTROYED, receiver, move |_, _| {
            *h.borrow_mut() += 1
        })
        .unwrap();
    engine.destroy_now(receiver);
    engine.destroy_now(watched);
    assert_eq!(*heard.borrow(), 0);
}
