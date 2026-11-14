//! [`Engine::make_on_read_by`]: a value made when it is read.

mod support;

use std::cell::Cell;

use ljos_engine::{Engine, Kept, ObjectId, PropIndex, Value, WriteKind};
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

thread_local! {
    static TOLD: std::cell::RefCell<Vec<Kept>> = const { std::cell::RefCell::new(Vec::new()) };
    static NEXT: Cell<f64> = const { Cell::new(0.0) };
}

/// Gives `NEXT` on each read, as an animation brought to now would, and
/// lets go when something comes to observe the property.
fn keeping(e: &Engine, me: ObjectId, prop: PropIndex, kept: Kept) {
    TOLD.with(|t| t.borrow_mut().push(kept));
    match kept {
        Kept::Read => assert!(e.give_unobserved(me, prop, Value::Real(NEXT.with(Cell::get)))),
        Kept::Observed => e.stop_keeping(me, prop),
        Kept::Given => {}
    }
}

fn told() -> Vec<Kept> {
    TOLD.with(|t| std::mem::take(&mut *t.borrow_mut()))
}

const KEPT: &str = "import QtQuick\nItem {\n  property real k: 0\n  property bool on: false\n  property real seen: on ? k : -1\n}";

fn kept(h: &support::Harness) -> (ObjectId, PropIndex) {
    let item = h.load(KEPT);
    let k = h.engine.property_index(item, "k").unwrap();
    told();
    NEXT.with(|n| n.set(1.0));
    h.engine.keep_on_read(item, k, keeping).unwrap();
    (item, k)
}

#[test]
fn a_kept_property_is_given_its_value_on_each_read_and_stays_kept() {
    let h = engine();
    let (item, k) = kept(&h);
    assert_eq!(h.num(item, "k"), 1.0);
    NEXT.with(|n| n.set(2.0));
    assert_eq!(h.num(item, "k"), 2.0);
    assert!(h.engine.is_made_on_read(item, k));
    assert_eq!(told(), [Kept::Read, Kept::Read]);
}

#[test]
fn a_binding_that_reads_a_kept_property_tells_its_keeper_it_is_observed() {
    let h = engine();
    let (item, k) = kept(&h);
    NEXT.with(|n| n.set(3.0));
    h.set(item, "on", Value::Bool(true));
    assert_eq!(h.num(item, "seen"), 3.0);
    assert_eq!(told(), [Kept::Read, Kept::Observed]);
    assert!(!h.engine.is_made_on_read(item, k));
    // From now on the keeper writes, and the binding follows.
    h.engine
        .write(item, k, Value::Real(4.0), WriteKind::Animation)
        .unwrap();
    assert_eq!(h.num(item, "seen"), 4.0);
}

#[test]
fn a_watch_on_a_kept_property_tells_its_keeper_it_is_observed() {
    let h = engine();
    let (item, k) = kept(&h);
    let heard = std::rc::Rc::new(Cell::new(0.0));
    let ear = heard.clone();
    h.engine
        .watch(item, k, move |_, v| ear.set(v.to_number()))
        .unwrap();
    assert_eq!(told(), [Kept::Observed]);
    h.engine
        .write(item, k, Value::Real(5.0), WriteKind::Animation)
        .unwrap();
    assert_eq!(heard.get(), 5.0);
}

#[test]
fn a_write_to_a_kept_property_tells_its_keeper_and_takes_it_away() {
    let h = engine();
    let (item, k) = kept(&h);
    h.set(item, "k", Value::Real(9.0));
    assert_eq!(told(), [Kept::Given]);
    assert!(!h.engine.is_made_on_read(item, k));
    assert_eq!(h.num(item, "k"), 9.0);
    assert!(told().is_empty());
}

#[test]
fn a_value_given_unobserved_is_refused_while_something_observes_the_property() {
    let h = engine();
    let item = h.load(KEPT);
    let k = h.engine.property_index(item, "k").unwrap();
    assert!(h.engine.give_unobserved(item, k, Value::Real(1.0)));
    assert_eq!(h.num(item, "k"), 1.0);
    h.set(item, "on", Value::Bool(true));
    assert_eq!(h.num(item, "seen"), 1.0);
    assert!(!h.engine.give_unobserved(item, k, Value::Real(2.0)));
    assert_eq!(h.num(item, "seen"), 1.0);
}
