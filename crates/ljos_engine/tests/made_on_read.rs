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

const CHAIN: &str = "import QtQuick\nItem {\n  property real t: 0\n  property real a: t * 2\n  property real b: a + 1\n  property real alone: 5\n}";

fn index(h: &support::Harness, item: ObjectId, name: &str) -> PropIndex {
    h.engine.property_index(item, name).unwrap()
}

#[test]
fn the_lazy_readers_of_a_property_are_the_bindings_read_by_nothing_else() {
    let h = engine();
    let item = h.load(CHAIN);
    let (t, a, b) = (
        index(&h, item, "t"),
        index(&h, item, "a"),
        index(&h, item, "b"),
    );
    assert_eq!(h.num(item, "b"), 1.0);
    let mut readers = h.engine.lazy_readers(item, t).unwrap();
    readers.sort();
    assert_eq!(readers, [(item, a), (item, b)]);
    // Read by nothing at all.
    let alone = index(&h, item, "alone");
    assert_eq!(h.engine.lazy_readers(item, alone), Some(Vec::new()));
}

#[test]
fn a_watch_at_the_end_of_the_bindings_sees_what_they_read() {
    let h = engine();
    let item = h.load(CHAIN);
    let (t, b) = (index(&h, item, "t"), index(&h, item, "b"));
    assert_eq!(h.num(item, "b"), 1.0);
    h.engine.watch(item, b, |_, _| {}).unwrap();
    assert_eq!(h.engine.lazy_readers(item, t), None);
}

#[test]
fn an_effect_that_reads_the_bindings_sees_what_they_read() {
    let h = engine();
    let item = h.load(CHAIN);
    let (t, a) = (index(&h, item, "t"), index(&h, item, "a"));
    h.engine
        .effect_in(item, move |e| {
            let _ = e.read(item, a);
        })
        .unwrap();
    assert_eq!(h.engine.lazy_readers(item, t), None);
}

thread_local! {
    static NOW: Cell<f64> = const { Cell::new(0.0) };
}

/// Gives the time of now on every read, as a clock nobody observes does.
fn clock(e: &Engine, me: ObjectId, prop: PropIndex) -> Value {
    let _ = e.make_on_read_by(me, prop, clock);
    Value::Real(NOW.with(Cell::get))
}

#[test]
fn a_binding_worked_out_again_reads_a_value_made_on_read_anew() {
    let h = engine();
    let item = h.load(CHAIN);
    let (t, a) = (index(&h, item, "t"), index(&h, item, "a"));
    h.set(item, "t", Value::Real(10.0));
    assert_eq!(h.num(item, "a"), 20.0);
    // From now on `t` is made when it is read, and nothing is told.
    NOW.with(|n| n.set(30.0));
    h.engine.make_on_read_by(item, t, clock).unwrap();
    assert_eq!(h.num(item, "a"), 20.0);
    assert!(h.engine.work_out_again(item, a));
    assert_eq!(h.num(item, "a"), 60.0);
    // A property with no binding has nothing to work out.
    assert!(!h.engine.work_out_again(item, index(&h, item, "alone")));
}
