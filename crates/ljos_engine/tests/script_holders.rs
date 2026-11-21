//! What holds the script heap is told by the types whose properties hold
//! it, the most first, so the part of an application that fills the heap
//! can be named.

mod support;

use std::sync::Arc;

use ljos_engine::Value;
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
            let held = e.script_heap_holders(1000).expect("scripts ran");
            assert!(held.truncated, "{held:?}");
            e.write_by_name(a, "list", Value::list(Vec::new())).unwrap();
            take_apart(deep);
        })
        .unwrap()
        .join()
        .unwrap();
}
