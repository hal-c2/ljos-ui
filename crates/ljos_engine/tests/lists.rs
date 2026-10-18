//! Object lists changed one item at a time: `Engine::list_push`,
//! `list_insert_before` and `list_remove`. What a scenario cannot see is
//! that the list is changed where it is (a parent with n children does
//! not copy n items for the next one), and that this stays true to what
//! readers hold.

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{ObjectId, QmlType, Value, WriteKind};
use support::{Harness, Item, engine};

fn item(h: &Harness) -> ObjectId {
    h.engine.create(Item::static_meta(), None).expect("an Item")
}

fn field(h: &Harness, parent: ObjectId) -> Vec<ObjectId> {
    h.engine
        .with::<Item, _>(parent, |i| i.children.objects())
        .expect("an Item")
}

fn read(h: &Harness, parent: ObjectId) -> Vec<ObjectId> {
    match h.engine.read(parent, Item::CHILDREN.index) {
        Value::List(l) => l.iter().filter_map(Value::as_object).collect(),
        other => panic!("children is {other:?}"),
    }
}

/// Where the list read from `parent` is kept.
fn allocation(h: &Harness, parent: ObjectId) -> *const Vec<Value> {
    match h.engine.read(parent, Item::CHILDREN.index) {
        Value::List(l) => Arc::as_ptr(&l),
        other => panic!("children is {other:?}"),
    }
}

#[test]
fn a_list_nobody_watches_is_changed_in_the_field() {
    let h = engine();
    let e = &h.engine;
    let parent = item(&h);
    let p = Item::CHILDREN.index;
    let [a, b, c, d] = [item(&h), item(&h), item(&h), item(&h)];

    e.list_push(parent, p, a).unwrap();
    e.list_push(parent, p, c).unwrap();
    assert_eq!(field(&h, parent), [a, c]);

    assert!(e.list_insert_before(parent, p, b, c).unwrap());
    assert_eq!(field(&h, parent), [a, b, c]);
    // An anchor that is not in the list changes nothing.
    assert!(!e.list_insert_before(parent, p, d, d).unwrap());
    assert_eq!(field(&h, parent), [a, b, c]);

    assert!(e.list_remove(parent, p, b).unwrap());
    assert!(!e.list_remove(parent, p, b).unwrap());
    assert_eq!(field(&h, parent), [a, c]);
    assert_eq!(read(&h, parent), [a, c]);
}

#[test]
fn a_watched_list_tells_its_readers_and_fills_the_field() {
    let h = engine();
    let e = &h.engine;
    let parent = item(&h);
    let p = Item::CHILDREN.index;
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    e.effect(move |e| log.borrow_mut().push(e.list_count(parent, p)))
        .unwrap();

    let [a, b, c] = [item(&h), item(&h), item(&h)];
    e.list_push(parent, p, a).unwrap();
    e.list_push(parent, p, c).unwrap();
    assert!(e.list_insert_before(parent, p, b, c).unwrap());
    assert_eq!(*seen.borrow(), [0, 1, 2, 3]);
    assert_eq!(read(&h, parent), [a, b, c]);
    assert_eq!(field(&h, parent), [a, b, c]);

    assert!(e.list_remove(parent, p, a).unwrap());
    assert_eq!(*seen.borrow(), [0, 1, 2, 3, 2]);
    assert_eq!(field(&h, parent), [b, c]);

    // One change for a batch of them.
    let d = item(&h);
    e.batch(|| {
        e.list_push(parent, p, d).unwrap();
        e.list_remove(parent, p, b).unwrap();
    });
    assert_eq!(*seen.borrow(), [0, 1, 2, 3, 2, 2]);
    assert_eq!(field(&h, parent), [c, d]);
}

#[test]
fn the_next_item_does_not_copy_the_list() {
    let h = engine();
    let e = &h.engine;
    let p = Item::CHILDREN.index;
    for watched in [false, true] {
        let parent = item(&h);
        if watched {
            e.effect(move |e| {
                e.list_count(parent, p);
            })
            .unwrap();
        }
        // Room for more than is pushed, so that a move would be a copy.
        let first: Vec<ObjectId> = (0..64).map(|_| item(&h)).collect();
        for o in &first {
            e.list_push(parent, p, *o).unwrap();
        }
        let before = allocation(&h, parent);
        let mut moved = 0;
        for _ in 0..1000 {
            e.list_push(parent, p, item(&h)).unwrap();
            let now = allocation(&h, parent);
            if now != before {
                moved += 1;
            }
        }
        // The list grows by doubling: from 64 to 1064 items that is a
        // handful of moves, where a copy for every push is a thousand.
        assert!(moved < 1000, "watched {watched}: every push copied");
        let mut last = allocation(&h, parent);
        let mut copies = 0;
        for _ in 0..100 {
            e.list_push(parent, p, item(&h)).unwrap();
            let now = allocation(&h, parent);
            copies += usize::from(now != last);
            last = now;
        }
        assert!(copies <= 1, "watched {watched}: {copies} copies in 100");
        assert_eq!(read(&h, parent).len(), 1164);
        assert_eq!(field(&h, parent).len(), 1164);
    }
}

#[test]
fn a_reader_that_kept_the_list_keeps_what_it_read() {
    let h = engine();
    let e = &h.engine;
    let p = Item::CHILDREN.index;
    for watched in [false, true] {
        let parent = item(&h);
        if watched {
            e.effect(move |e| {
                e.list_count(parent, p);
            })
            .unwrap();
        }
        let [a, b] = [item(&h), item(&h)];
        e.list_push(parent, p, a).unwrap();
        let kept = e.read(parent, p);
        e.list_push(parent, p, b).unwrap();
        let Value::List(kept) = kept else {
            panic!("children is not a list");
        };
        assert_eq!(kept.len(), 1, "watched {watched}");
        assert_eq!(read(&h, parent), [a, b], "watched {watched}");
        assert_eq!(field(&h, parent), [a, b], "watched {watched}");
    }
}

#[test]
fn a_list_declared_in_qml_is_changed_the_same_way() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(
        r#"import QtQuick
Item {
    property list<Item> things
    property int count: things.length
}"#,
    );
    let p = e.property_index(root, "things").unwrap();
    let [a, b, c] = [item(&h), item(&h), item(&h)];
    e.list_push(root, p, a).unwrap();
    assert_eq!(h.num(root, "count"), 1.0);
    e.list_push(root, p, c).unwrap();
    assert!(e.list_insert_before(root, p, b, c).unwrap());
    assert_eq!(h.num(root, "count"), 3.0);
    assert!(e.list_remove(root, p, a).unwrap());
    assert_eq!(h.num(root, "count"), 2.0);
    match e.read(root, p) {
        Value::List(l) => {
            let got: Vec<ObjectId> = l.iter().filter_map(Value::as_object).collect();
            assert_eq!(got, [b, c]);
        }
        other => panic!("things is {other:?}"),
    }
}

#[test]
fn a_bound_list_takes_the_item_too() {
    let h = engine();
    let e = &h.engine;
    let parent = item(&h);
    let p = Item::CHILDREN.index;
    let [a, b] = [item(&h), item(&h)];
    e.bind(parent, p, move |_| Value::list(vec![Value::Object(a)]))
        .unwrap();
    e.list_push(parent, p, b).unwrap();
    assert_eq!(read(&h, parent), [a, b]);
    assert_eq!(field(&h, parent), [a, b]);
}

#[test]
fn only_objects_are_kept_in_a_native_list() {
    let h = engine();
    let e = &h.engine;
    let parent = item(&h);
    let p = Item::CHILDREN.index;
    let a = item(&h);
    let mixed = Value::list(vec![Value::Int(1), Value::Object(a), Value::Null]);
    e.write(parent, p, mixed, WriteKind::Imperative).unwrap();
    assert_eq!(field(&h, parent), [a]);
}

#[test]
fn a_script_s_loop_over_a_native_list_makes_one_array_of_it() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(
        r#"import QtQuick
Item {
    id: grid
    Item {} Item {} Item {} Item {} Item {} Item {} Item {} Item {}
    property int shown: { let n = 0; for (let i = 0; i < grid.children.length; i++) n++; return n }
    function byId() {
        let n = 0
        for (let i = 0; i < grid.children.length; i++) n += grid.children[i] ? 1 : 0
        return n
    }
    function names() {
        let n = 0
        for (let i = 0; i < children.length; i++) if (children[i].parent === this) n++
        return n
    }
    function own() {
        const a = children, b = children
        a.push(null)
        return (a !== b) + "," + b.length + "," + children.length
    }
}"#,
    );
    let before = e.native_list_arrays();
    assert_eq!(e.invoke_by_name(root, "names", &[]).unwrap(), Value::Int(8));
    assert_eq!(e.invoke_by_name(root, "names", &[]).unwrap(), Value::Int(8));
    assert_eq!(e.invoke_by_name(root, "byId", &[]).unwrap(), Value::Int(8));
    assert_eq!(h.num(root, "shown"), 8.0);
    // Seventeen reads of the list in each loop, by its own name and by the
    // object's, and the list never changed.
    assert_eq!(e.native_list_arrays() - before, 1);

    // Each read is still an array of the script's own.
    assert_eq!(
        e.invoke_by_name(root, "own", &[]).unwrap(),
        Value::from("true,8,8")
    );

    // A changed list is made again, once.
    e.list_push(root, Item::CHILDREN.index, item(&h)).unwrap();
    assert_eq!(e.invoke_by_name(root, "names", &[]).unwrap(), Value::Int(8));
    assert_eq!(e.invoke_by_name(root, "byId", &[]).unwrap(), Value::Int(9));
    assert_eq!(h.num(root, "shown"), 9.0);
    assert_eq!(e.native_list_arrays() - before, 2);
}
