//! What holds the script heap is told by the types whose properties hold
//! it, the most first, so the part of an application that fills the heap
//! can be named.

mod support;

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
