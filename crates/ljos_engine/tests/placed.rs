//! Values that come from native code and not from the author: what a
//! layout writes ([`WriteKind::Layout`]) and what a binding native code
//! installs gives ([`Engine::bind_computed`]). Neither is an assignment in
//! Qt: the first leaves a binding where it is, and no `Behavior` sees
//! either.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{Interception, ObjectId, Provenance, QmlType, Value, WriteKind};
use support::{Harness, Item, engine};

const BOUND: &str = r#"import QtQuick
Item {
    id: root
    property real w: 50
    Item { objectName: "child"; width: root.w }
}"#;

/// Makes every value on its way into `width` be taken, and noted.
fn intercept(h: &Harness, obj: ObjectId) -> Rc<RefCell<Vec<(f64, Interception)>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    h.engine.set_interceptor(
        obj,
        Item::WIDTH.index,
        Some(Rc::new(move |_, v, how| {
            log.borrow_mut().push((v.to_number(), how));
            true
        })),
    );
    seen
}

#[test]
fn a_layout_write_leaves_the_binding_and_fills_the_field() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(BOUND);
    let child = h.find(root, "child");
    let p = Item::WIDTH.index;
    assert_eq!(h.num(child, "width"), 50.0);

    e.write_internal(child, p, Value::Real(80.0), WriteKind::Layout)
        .unwrap();
    assert_eq!(h.num(child, "width"), 80.0);
    assert_eq!(e.with::<Item, _>(child, |i| i.width).unwrap(), 80.0);
    assert!(e.is_bound(child, p));
    assert_eq!(e.provenance(child, p), Some(Provenance::Bound));

    h.set(root, "w", Value::Real(60.0));
    assert_eq!(h.num(child, "width"), 60.0);
    assert_eq!(e.with::<Item, _>(child, |i| i.width).unwrap(), 60.0);

    // An assignment still removes the binding.
    h.set(child, "width", Value::Real(33.0));
    h.set(root, "w", Value::Real(90.0));
    assert_eq!(h.num(child, "width"), 33.0);
    assert!(!e.is_bound(child, p));
}

#[test]
fn a_layout_write_without_a_binding_is_an_assignment() {
    let h = engine();
    let e = &h.engine;
    let p = Item::WIDTH.index;
    // With and without a node for the property.
    for watched in [false, true] {
        let item = e.create(Item::static_meta(), None).unwrap();
        if watched {
            e.effect(move |e| {
                e.read(item, p);
            })
            .unwrap();
        }
        assert_eq!(e.provenance(item, p), Some(Provenance::Default));
        e.write_internal(item, p, Value::Real(7.0), WriteKind::Layout)
            .unwrap();
        assert_eq!(e.read(item, p), Value::Real(7.0), "watched {watched}");
        assert_eq!(
            e.provenance(item, p),
            Some(Provenance::Assigned),
            "watched {watched}"
        );
    }
}

#[test]
fn no_interceptor_sees_a_layout_write() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(BOUND);
    let child = h.find(root, "child");
    let p = Item::WIDTH.index;
    assert_eq!(h.num(child, "width"), 50.0);
    let seen = intercept(&h, child);

    e.write_internal(child, p, Value::Real(80.0), WriteKind::Layout)
        .unwrap();
    assert_eq!(h.num(child, "width"), 80.0);
    assert!(seen.borrow().is_empty());

    // What the author's binding gives next is seen, and taken here.
    h.set(root, "w", Value::Real(60.0));
    assert_eq!(h.num(child, "width"), 80.0);
    assert_eq!(*seen.borrow(), [(60.0, Interception::Binding)]);
}

#[test]
fn no_interceptor_sees_what_a_computed_binding_gives() {
    let h = engine();
    let e = &h.engine;
    let p = Item::WIDTH.index;
    let item = e.create(Item::static_meta(), None).unwrap();
    let source = e.create(Item::static_meta(), None).unwrap();
    e.write(source, p, Value::Real(10.0), WriteKind::Imperative)
        .unwrap();
    let seen = intercept(&h, item);

    e.bind_computed(item, p, move |e| e.read(source, p))
        .unwrap();
    assert_eq!(e.read(item, p), Value::Real(10.0));
    e.write(source, p, Value::Real(50.0), WriteKind::Imperative)
        .unwrap();
    assert_eq!(e.read(item, p), Value::Real(50.0));
    assert!(seen.borrow().is_empty());

    // The same binding installed as the author's is seen every time
    // after it replaced a value.
    e.bind(item, p, move |e| e.read(source, p)).unwrap();
    e.write(source, p, Value::Real(70.0), WriteKind::Imperative)
        .unwrap();
    assert_eq!(e.read(item, p), Value::Real(50.0));
    assert!(!seen.borrow().is_empty());
}

/// What an interceptor puts off with [`Engine::after_binding`] is done
/// when the binding has given its value: before the read (or, with eager
/// bindings, the write) that ran the binding returns, and it may write
/// the property the binding is on.
#[test]
fn work_an_interceptor_puts_off_runs_when_the_binding_has_its_value() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(BOUND);
    let child = h.find(root, "child");
    let p = Item::WIDTH.index;
    assert_eq!(h.num(child, "width"), 50.0);

    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = log.clone();
    e.set_interceptor(
        child,
        p,
        Some(Rc::new(move |e, v, how| {
            assert_eq!(how, Interception::Binding);
            let (to, log) = (v.to_number(), seen.clone());
            e.after_binding(move |e| {
                log.borrow_mut().push(to);
                // Half the way, as an animation's first step would be.
                let half = (e.peek(child, p).to_number() + to) / 2.0;
                e.write_internal(child, p, Value::Real(half), WriteKind::Animation)
                    .unwrap();
            });
            true
        })),
    );
    h.set(root, "w", Value::Real(70.0));
    assert_eq!(h.num(child, "width"), 60.0);
    assert_eq!(*log.borrow(), [70.0]);
    assert!(e.is_bound(child, p));

    // Outside a binding there is nothing to wait for.
    let ran = Rc::new(RefCell::new(false));
    let r = ran.clone();
    e.after_binding(move |_| *r.borrow_mut() = true);
    assert!(*ran.borrow());
}
