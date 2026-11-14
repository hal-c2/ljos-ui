//! [`Engine::make_on_read_by`]: a value made when it is read.

mod support;

use std::cell::Cell;

use ljos_engine::{Engine, ObjectId, PropIndex, Value};
use support::{Item, engine};

thread_local! {
    static MADE: Cell<i32> = const { Cell::new(0) };
}

/// Gives itself again, so each read makes the value anew.
fn counting(e: &Engine, me: ObjectId, prop: PropIndex) -> Value {
    let _ = e.make_on_read_by(me, prop, counting);
    MADE.with(|m| {
        m.set(m.get() + 1);
        Value::Real(f64::from(m.get()))
    })
}

#[test]
fn a_maker_that_gives_itself_again_makes_the_value_on_every_read() {
    let h = engine();
    let e = &h.engine;
    let item = h.load("import QtQuick\nItem {}");
    MADE.with(|m| m.set(0));
    e.make_on_read_by(item, Item::X.index, counting).unwrap();
    assert_eq!(e.read(item, Item::X.index).to_number(), 1.0);
    assert_eq!(e.read(item, Item::X.index).to_number(), 2.0);
    assert!(e.is_made_on_read(item, Item::X.index));
    // A write gives the property its value, and the maker goes.
    h.set(item, "x", Value::Real(7.0));
    assert!(!e.is_made_on_read(item, Item::X.index));
    assert_eq!(e.read(item, Item::X.index).to_number(), 7.0);
    assert_eq!(MADE.with(Cell::get), 2);
}
