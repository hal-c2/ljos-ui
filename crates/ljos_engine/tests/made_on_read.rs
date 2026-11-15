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

thread_local! {
    static AGAIN: Cell<Option<(ObjectId, PropIndex)>> = const { Cell::new(None) };
    static ANSWERS: std::cell::RefCell<Vec<bool>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Asks, in the middle of the read, that the binding in `AGAIN` be worked
/// out again, and notes the answer.
fn asks_again(e: &Engine, _: ObjectId, _: PropIndex) -> Value {
    if let Some((o, p)) = AGAIN.with(Cell::get) {
        ANSWERS.with(|a| a.borrow_mut().push(e.work_out_again(o, p)));
    }
    Value::Real(1.0)
}

const READ_IN_A_RUN: &str = "import QtQuick\nItem {\n  property real t: 0\n  property real u: 0\n  property real a: t * 2\n  property real c: a + u\n}";

#[test]
fn a_binding_that_is_running_is_not_worked_out_again() {
    let h = engine();
    let item = h.load(READ_IN_A_RUN);
    let (t, a) = (index(&h, item, "t"), index(&h, item, "a"));
    AGAIN.with(|g| g.set(Some((item, a))));
    ANSWERS.with(|x| x.borrow_mut().clear());
    // `a` reads `t` as it runs: its maker asks for `a` again.
    h.engine.make_on_read_by(item, t, asks_again).unwrap();
    assert_eq!(h.num(item, "a"), 2.0);
    assert_eq!(ANSWERS.with(|x| x.take()), [false]);
}

#[test]
fn a_binding_a_running_binding_has_read_is_not_worked_out_again() {
    let h = engine();
    let item = h.load(READ_IN_A_RUN);
    let (u, a) = (index(&h, item, "u"), index(&h, item, "a"));
    AGAIN.with(|g| g.set(Some((item, a))));
    ANSWERS.with(|x| x.borrow_mut().clear());
    // `c` has read `a` when it reads `u`: worked out again, `a` would
    // come back to `c` as a loop.
    h.engine.make_on_read_by(item, u, asks_again).unwrap();
    assert_eq!(h.num(item, "c"), 1.0);
    assert_eq!(ANSWERS.with(|x| x.take()), [false]);
    // Once the run is over, it is.
    assert!(h.engine.work_out_again(item, a));
}

/// A keeper that is not `keeping`.
fn another(_: &Engine, _: ObjectId, _: PropIndex, _: Kept) {}

#[test]
fn a_keeper_lets_go_only_of_what_it_keeps_itself() {
    let h = engine();
    let (item, k) = kept(&h);
    assert!(h.engine.is_kept_by(item, k, keeping));
    assert!(!h.engine.is_kept_by(item, k, another));
    // Given a value, it is no longer kept; another comes to keep it.
    h.set(item, "k", Value::Real(3.0));
    h.engine.keep_on_read(item, k, another).unwrap();
    h.engine.stop_keeping_by(item, k, keeping);
    assert!(
        h.engine.is_kept_by(item, k, another),
        "it stays the other's"
    );
    h.engine.stop_keeping_by(item, k, another);
    assert!(!h.engine.is_made_on_read(item, k));
}

thread_local! {
    static STILL_MINE: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Hands the property it keeps to `another` as it is read, then lets go of
/// it, as a keeper that no longer wants it would.
fn handing_over(e: &Engine, me: ObjectId, prop: PropIndex, kept: Kept) {
    if kept == Kept::Read {
        e.keep_on_read(me, prop, another).unwrap();
        STILL_MINE.with(|s| s.set(Some(e.is_kept_by(me, prop, handing_over))));
        e.stop_keeping_by(me, prop, handing_over);
    }
}

#[test]
fn a_keeper_another_takes_the_place_of_as_it_is_read_keeps_nothing() {
    let h = engine();
    let item = h.load(KEPT);
    let k = index(&h, item, "k");
    STILL_MINE.with(|s| s.set(None));
    h.engine.keep_on_read(item, k, handing_over).unwrap();
    let _ = h.num(item, "k");
    assert_eq!(STILL_MINE.with(Cell::get), Some(false));
    assert!(
        h.engine.is_kept_by(item, k, another),
        "it stays the other's"
    );
    assert!(!h.engine.is_kept_by(item, k, handing_over));
}

/// Hands the property it keeps to `another` as it is read, and the other
/// lets go of it at once.
fn handing_over_to_none(e: &Engine, me: ObjectId, prop: PropIndex, kept: Kept) {
    if kept == Kept::Read {
        e.keep_on_read(me, prop, another).unwrap();
        e.stop_keeping(me, prop);
    }
}

#[test]
fn a_keeper_handed_over_as_it_is_read_is_not_put_back_when_the_other_lets_go() {
    let h = engine();
    let item = h.load(KEPT);
    let k = index(&h, item, "k");
    h.engine
        .keep_on_read(item, k, handing_over_to_none)
        .unwrap();
    let _ = h.num(item, "k");
    assert!(!h.engine.is_made_on_read(item, k));
}

/// Hands the property it keeps to `keeping` as it is read.
fn handing_over_to_keeping(e: &Engine, me: ObjectId, prop: PropIndex, kept: Kept) {
    if kept == Kept::Read {
        e.keep_on_read(me, prop, keeping).unwrap();
    }
}

#[test]
fn a_keeper_handed_a_property_as_a_binding_reads_it_is_told_it_is_observed() {
    let h = engine();
    let item = h.load(KEPT);
    let k = index(&h, item, "k");
    h.engine
        .keep_on_read(item, k, handing_over_to_keeping)
        .unwrap();
    told();
    h.set(item, "on", Value::Bool(true));
    assert_eq!(h.num(item, "seen"), 0.0);
    assert_eq!(told(), [Kept::Observed]);
    assert!(!h.engine.is_made_on_read(item, k));
}
