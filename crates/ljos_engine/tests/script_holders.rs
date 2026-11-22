//! What holds the script heap is told by the types whose properties hold
//! it, the most first, so the part of an application that fills the heap
//! can be named.

mod support;

use std::sync::Arc;

use ljos_engine::{QmlObject, QmlType, QtObject, Value, fault_kind};
use support::engine;

const PAGE: &str = r#"import QtQuick
Item {
  property var small: ({ n: 1 })
  QtObject {
    property var store: null
    Component.onCompleted: {
      var rows = []
      for (var i = 0; i < 20000; i++) rows.push({ i: i, s: "row " + i })
      store = { rows: rows }
    }
  }
}
"#;

#[test]
fn the_type_that_holds_a_big_array_is_named_first() {
    let h = engine();
    let e = &h.engine;
    h.load(PAGE);

    let held = e.script_heap_holders(usize::MAX).expect("scripts ran");
    assert!(!held.truncated);
    let first = &held.holders[0];
    assert_eq!(first.name, "QtObject", "{held:?}");
    assert_eq!(first.objects, 1);
    assert!(first.bytes > 20_000 * 64, "{held:?}");
    // The root, which holds a small object, comes after.
    assert!(held.holders.len() >= 2, "{held:?}");
    assert!(held.holders[1].bytes < first.bytes / 100, "{held:?}");

    let cut = e.script_heap_holders(10).unwrap();
    assert!(cut.truncated);
}

const DEEP: &str = r#"import QtQuick
Item {
  property var held: ({ n: 1 })
  QtObject { objectName: "a"; property list<var> list }
}
"#;

fn nested(depth: usize) -> Value {
    let mut v = Value::Undefined;
    for _ in 0..depth {
        v = Value::List(Arc::new(vec![v]));
    }
    v
}

/// Taken apart a level at a time: dropping a deep nesting whole recurses
/// as deep as it goes.
fn take_apart(mut v: Value) {
    while let Value::List(items) = v {
        v = match Arc::try_unwrap(items) {
            Ok(mut items) => items.pop().unwrap_or(Value::Undefined),
            Err(_) => return,
        };
    }
}

/// A list nested deeper than the thread's stack goes is walked without
/// running out of it, and only as far as the budget.
#[test]
fn a_deep_nesting_is_walked_within_the_budget() {
    std::thread::Builder::new()
        .stack_size(4 << 20)
        .spawn(|| {
            let h = engine();
            let e = &h.engine;
            let root = h.load(DEEP);
            let deep = nested(500_000);
            let a = h.find(root, "a");
            e.write_by_name(a, "list", deep.clone()).unwrap();
            let held = e.script_heap_holders(1000);
            // Taken apart before anything can fail: unwinding through the
            // nesting would overflow the stack and hide what failed.
            let cleared = e.write_by_name(a, "list", Value::list(Vec::new()));
            take_apart(deep);
            cleared.unwrap();
            let held = held.expect("scripts ran");
            assert!(held.truncated, "{held:?}");
        })
        .unwrap()
        .join()
        .unwrap();
}

const SHARED: &str = r#"import QtQuick
Item {
  property var row: ({ s: "row" })
  property list<var> rows
  Item { property list<var> rows }
  Item { property list<var> rows }
  Item { property list<var> rows }
  QtObject { property var tail: ({ s: "tail" }) }
}
"#;

/// A list many objects share is charged to the budget once, not once per
/// object, so a model every delegate holds leaves room to name the types
/// after them, and each of them is still counted as holding it.
#[test]
fn a_list_many_objects_share_is_charged_once() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(SHARED);
    let row = h.get(root, "row");
    assert!(matches!(row, Value::Js(_)), "{row:?}");
    let rows = Value::list(vec![row; 10_000]);
    let mut holders = 0;
    for obj in std::iter::once(root).chain(e.children(root)) {
        if e.write_by_name(obj, "rows", rows.clone()).is_ok() {
            holders += 1;
            // The one list in each, or there is nothing shared to charge once.
            match (e.read_by_name(obj, "rows"), &rows) {
                (Value::List(kept), Value::List(given)) => {
                    assert!(Arc::ptr_eq(&kept, given), "a copy was stored");
                }
                (kept, _) => panic!("the list is stored as {}", kept.to_js_string()),
            }
        }
    }
    assert_eq!(holders, 4);
    // Charged once per object, the four would take 40,000.
    let held = e.script_heap_holders(20_000).expect("scripts ran");
    assert!(!held.truncated, "{held:?}");
    let named = |name: &str| {
        held.holders
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.objects)
    };
    assert_eq!(named("QtObject"), Some(1), "{held:?}");
    // The root and its three Items, whatever the root's type is named.
    let holding: usize = held.holders.iter().map(|t| t.objects).sum();
    assert_eq!(holding, 5, "{held:?}");
}

/// A script value that is a property's whole value costs the budget one,
/// so objects that each hold one are not all gone through past it.
#[test]
fn a_value_each_object_holds_by_itself_is_charged_to_the_budget() {
    let h = engine();
    let objects = "  QtObject { property var v: ({}) }\n".repeat(40);
    h.load(&format!("import QtQuick\nItem {{\n{objects}}}\n"));

    let held = h.engine.script_heap_holders(10).expect("scripts ran");
    assert!(held.truncated, "{held:?}");
    let found: usize = held.holders.iter().map(|t| t.objects).sum();
    assert!(found <= 10, "{found} objects found on a budget of 10");
}

/// A native type that keeps a script's value in a field of its own.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Test", name = "Keeper", extends = QtObject)]
pub struct Keeper {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub held: Value,
}

#[test]
fn a_script_value_in_a_native_field_is_found() {
    let h = engine();
    let e = &h.engine;
    let root =
        h.load("import QtQuick\nItem { property var made: ({ text: \"x\".repeat(100000) }) }\n");
    let keeper = e.create(Keeper::static_meta(), Some(root)).unwrap();
    let made = e.read_by_name(root, "made");
    assert!(matches!(made, Value::Js(_)), "{made:?}");
    e.write_by_name(keeper, "held", made).unwrap();
    e.write_by_name(root, "made", Value::Null).unwrap();

    let held = e.script_heap_holders(usize::MAX).expect("scripts ran");
    let first = held.holders.first().expect("a holder");
    assert_eq!(first.name, "Keeper", "{held:?}");
    assert_eq!(first.objects, 1);
    assert!(first.bytes >= 100_000, "{held:?}");
}

/// The walk makes nothing, so it needs no limit lifted: a heap over its
/// limit is walked whole, and the limit stands.
#[test]
fn a_heap_over_its_limit_is_walked_whole_under_it() {
    let h = engine();
    let e = &h.engine;
    h.load(PAGE);
    let all = e.script_heap_holders(usize::MAX).expect("scripts ran");
    let limit = e.script_heap_bytes().unwrap() / 2;
    assert!(e.set_script_heap_limit(Some(limit)));

    let held = e.script_heap_holders(usize::MAX).expect("scripts ran");
    assert_eq!(held, all);
    assert_eq!(e.script_memory_limit(), Some(limit));
    assert!(e.faults().is_empty(), "{:?}", e.faults());
}

/// Entering for the walk minds the heap as any entry does, against the
/// limit there is: a warning about a heap with room again is over.
#[test]
fn looking_for_holders_minds_the_heap_against_its_limit() {
    let h = engine();
    let e = &h.engine;
    h.load(PAGE);
    e.collect_garbage();
    let bytes = e.script_heap_bytes().unwrap();
    assert!(e.set_script_heap_limit(Some(bytes + bytes / 10)));
    e.collect_garbage();
    assert!(e.script_heap_pressure().unwrap().warned, "not nearly full");

    assert!(e.set_script_heap_limit(Some(bytes * 4)));
    e.script_heap_holders(usize::MAX).expect("scripts ran");
    assert!(!e.script_heap_pressure().unwrap().warned);
}

const LEAK: &str = r#"import QtQuick
Item {
  property int ticks: 0
  QtObject { id: leaker; property var store: ({ rows: [] }) }
  Timer {
    interval: 10; running: true; repeat: true
    onTriggered: {
      for (var i = 0; i < 2000; i++) leaker.store.rows.push({ i: i, s: "row " + i })
      ticks++
    }
  }
}
"#;

/// A leak spread over many dispatches, each well under its share of the
/// room, is not stopped by the share (`JsConfig::dispatch_heap_share`
/// bounds one dispatch). Measured here: it runs for many dispatches, until
/// it runs out of room near the heap's limit with an out-of-memory fault
/// (the last refusal can come from the share, which shrinks with the room
/// left, rather than from the limit itself), never past the limit, and
/// `script_heap_holders` then names the type that keeps it first, with
/// most of the heap, so the part that leaks can be found.
#[test]
fn a_leak_over_many_dispatches_runs_into_the_limit_and_is_named() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(LEAK);
    let limit = 16 << 20;
    assert!(e.set_script_heap_limit(Some(limit)));
    h.pass_ms(10 * 1000);

    let ticks = h.num(root, "ticks");
    let faults = e.faults();
    assert!(
        faults
            .iter()
            .any(|f| fault_kind(&f.fault) == "JsOutOfMemory"),
        "{faults:?}"
    );
    // Many dispatches ran before the limit: the share stopped none of them.
    assert!(ticks > 10.0 && ticks < 1000.0, "{ticks} ticks");
    let bytes = e.script_heap_bytes().unwrap();
    assert!(bytes < limit + (1 << 20), "{bytes} of {limit}");

    let held = e.script_heap_holders(usize::MAX).expect("scripts ran");
    assert!(!held.truncated, "{held:?}");
    let first = &held.holders[0];
    assert_eq!(first.name, "QtObject", "{held:?}");
    assert!(first.bytes > limit / 2, "{held:?}");
}
