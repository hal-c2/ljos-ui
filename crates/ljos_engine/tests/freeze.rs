//! Freezing (ARCHITECTURE.md §8.3): a frozen subtree does no work and
//! wakes nobody, and on thaw it catches up on what it missed.

mod support;

use support::engine;

const PAGE: &str = r#"import QtQuick
Item {
  property int ticks: 0
  property int once: 0
  Item {
    objectName: "page"
    Timer { interval: 100; running: true; repeat: true; onTriggered: ticks++ }
    Timer { objectName: "late"; interval: 50; onTriggered: once++ }
  }
}
"#;

#[test]
fn a_frozen_timer_wakes_nobody_and_fires_once_on_thaw() {
    let h = engine();
    let root = h.load(PAGE);
    let page = h.find(root, "page");
    h.pass_ms(100);
    assert_eq!(h.num(root, "ticks"), 1.0);

    h.engine.set_frozen(page, true).unwrap();
    assert!(h.engine.is_frozen(page));
    assert!(h.engine.is_frozen(h.find(root, "late")), "what it owns");
    assert!(!h.engine.is_frozen(root));
    assert_eq!(h.engine.next_timer_deadline(), None, "the host sleeps");
    h.pass_ms(1000);
    assert_eq!(h.num(root, "ticks"), 1.0);

    h.engine.set_frozen(page, false).unwrap();
    h.pass_ms(0);
    assert_eq!(h.num(root, "ticks"), 2.0, "it triggered, once");
    h.pass_ms(100);
    assert_eq!(h.num(root, "ticks"), 3.0, "and goes on from there");
}

#[test]
fn a_timer_started_while_frozen_waits_for_the_thaw() {
    let h = engine();
    let root = h.load(PAGE);
    let page = h.find(root, "page");
    let late = h.find(root, "late");
    h.engine.set_frozen(page, true).unwrap();
    h.set(late, "running", ljos_engine::Value::Bool(true));
    h.pass_ms(20);
    h.engine.set_frozen(page, false).unwrap();
    // The start itself was parked with the page: the timer counts from
    // the thaw.
    h.pass_ms(49);
    assert_eq!(h.num(root, "once"), 0.0, "not due yet");
    h.pass_ms(1);
    assert_eq!(h.num(root, "once"), 1.0);
}

#[test]
fn freezing_twice_is_undone_by_one_thaw() {
    let h = engine();
    let root = h.load(PAGE);
    let page = h.find(root, "page");
    let late = h.find(root, "late");
    h.engine.set_frozen(page, true).unwrap();
    h.engine.set_frozen(page, true).unwrap();
    h.engine.set_frozen(late, true).unwrap();
    h.engine.set_frozen(page, false).unwrap();
    assert!(!h.engine.is_frozen(page));
    assert!(h.engine.is_frozen(late), "frozen on its own");
    h.engine.set_frozen(late, false).unwrap();
    h.engine.set_frozen(late, false).unwrap();
    assert!(!h.engine.is_frozen(late));
    h.engine.set_frozen(late, true).unwrap();
    assert!(h.engine.is_frozen(late), "and counted again");
}
