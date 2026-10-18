//! The QtQml module: `Connections`, `Timer`, `Qt.callLater`, `QtObject`.

mod support;

use ljos_engine::Value;
use support::engine;

#[test]
fn connections_run_handlers_for_the_targets_signals() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  id: root
  signal ping(int n)
  property int got: 0
  Connections {
    target: root
    function onPing(n) { root.got = n }
  }
}
"#,
    );
    h.engine
        .emit_by_name(root, "ping", &[Value::Int(7)])
        .unwrap();
    assert_eq!(h.num(root, "got"), 7.0);
    assert!(h.warnings().is_empty());
}

#[test]
fn connections_follow_a_new_target_and_can_be_disabled() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  id: root
  property int hits: 0
  property bool on: true
  property Item which: a
  Item { id: a; objectName: "a"; signal ping() }
  Item { id: b; objectName: "b"; signal ping() }
  Connections {
    target: root.which
    enabled: root.on
    function onPing() { root.hits++ }
  }
}
"#,
    );
    let (a, b) = (h.find(root, "a"), h.find(root, "b"));
    h.engine.emit_by_name(a, "ping", &[]).unwrap();
    h.engine.emit_by_name(b, "ping", &[]).unwrap();
    assert_eq!(h.num(root, "hits"), 1.0);
    h.set(root, "which", Value::Object(b));
    h.engine.emit_by_name(a, "ping", &[]).unwrap();
    h.engine.emit_by_name(b, "ping", &[]).unwrap();
    assert_eq!(h.num(root, "hits"), 2.0);
    h.set(root, "on", Value::Bool(false));
    h.engine.emit_by_name(b, "ping", &[]).unwrap();
    assert_eq!(h.num(root, "hits"), 2.0);
}

#[test]
fn connections_warn_about_an_unknown_signal_unless_told_not_to() {
    let h = engine();
    h.load(
        r#"import QtQuick
Item {
  id: root
  Connections { target: root; function onNothing() {} }
  Connections { target: root; ignoreUnknownSignals: true; function onAlsoNothing() {} }
}
"#,
    );
    let w = h.warnings();
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(
        w[0].contains("Detected function \"onNothing\" in Connections element"),
        "{w:?}"
    );
}

#[test]
fn a_repeating_timer_triggers_every_interval() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  property int ticks: 0
  Timer { interval: 100; running: true; repeat: true; onTriggered: ticks++ }
}
"#,
    );
    h.pass_ms(99);
    assert_eq!(h.num(root, "ticks"), 0.0);
    h.pass_ms(1);
    assert_eq!(h.num(root, "ticks"), 1.0);
    h.pass_ms(450);
    assert_eq!(h.num(root, "ticks"), 5.0);
}

#[test]
fn a_single_shot_timer_stops_after_triggering() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  property int ticks: 0
  property alias active: t.running
  Timer { id: t; interval: 50; running: true; onTriggered: ticks++ }
}
"#,
    );
    h.pass_ms(500);
    assert_eq!(h.num(root, "ticks"), 1.0);
    assert_eq!(h.get(root, "active"), Value::Bool(false));
    // Nothing is scheduled any more: the engine sleeps.
    assert_eq!(h.engine.next_timer_deadline(), None);
}

#[test]
fn timer_start_stop_restart_and_triggered_on_start() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  property int ticks: 0
  Timer { id: t; interval: 100; repeat: true; triggeredOnStart: true; onTriggered: ticks++ }
  function go() { t.start() }
  function halt() { t.stop() }
  function again() { t.restart() }
}
"#,
    );
    h.pass_ms(1000);
    assert_eq!(h.num(root, "ticks"), 0.0);
    h.engine.invoke_by_name(root, "go", &[]).unwrap();
    h.engine.invoke_by_name(root, "go", &[]).unwrap();
    // Posted, as in Qt: it fires on the next turn, once.
    assert_eq!(h.num(root, "ticks"), 0.0);
    h.pass_ms(0);
    assert_eq!(h.num(root, "ticks"), 1.0, "triggered on start");
    h.pass_ms(250);
    assert_eq!(h.num(root, "ticks"), 3.0);
    h.engine.invoke_by_name(root, "halt", &[]).unwrap();
    h.pass_ms(500);
    assert_eq!(h.num(root, "ticks"), 3.0);
    h.engine.invoke_by_name(root, "again", &[]).unwrap();
    h.pass_ms(0);
    assert_eq!(h.num(root, "ticks"), 4.0);
}

#[test]
fn a_destroyed_timer_never_fires() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  property int ticks: 0
  Timer { id: t; objectName: "t"; interval: 10; running: true; repeat: true; onTriggered: ticks++ }
}
"#,
    );
    let t = h.find(root, "t");
    h.engine.destroy_now(t);
    h.pass_ms(100);
    assert_eq!(h.num(root, "ticks"), 0.0);
    assert_eq!(h.engine.next_timer_deadline(), None);
}

#[test]
fn call_later_runs_once_after_the_current_work() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  property int runs: 0
  function bump() { runs++ }
  Component.onCompleted: { Qt.callLater(bump); Qt.callLater(bump); console.log("now", runs) }
}
"#,
    );
    assert_eq!(h.num(root, "runs"), 0.0);
    assert!(h.engine.has_pending_calls());
    // Due at once, and nothing is once it ran: the host sleeps again.
    assert!(h.engine.next_timer_deadline().is_some());
    h.engine.run_pending_calls();
    assert_eq!(h.num(root, "runs"), 1.0);
    assert_eq!(h.log(), vec!["now 0"]);
    assert_eq!(h.engine.next_timer_deadline(), None);
}

#[test]
fn qt_object_holds_properties_and_object_name() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  property QtObject store: QtObject { objectName: "store"; property int count: 2 }
  property int seen: store.count * 3
  property string name: store.objectName
}
"#,
    );
    assert_eq!(h.num(root, "seen"), 6.0);
    assert_eq!(h.get(root, "name").to_js_string().as_str(), "store");
}

#[test]
fn a_deleted_object_reads_as_null_in_js() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  property Item kid: Item { objectName: "kid" }
  function isNull() { return kid === null }
}
"#,
    );
    let kid = h.get(root, "kid").as_object().unwrap();
    h.engine.destroy_now(kid);
    let r = h.engine.invoke_by_name(root, "isNull", &[]).unwrap();
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn a_timers_parent_is_the_object_it_is_declared_in() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  id: root
  Item {
    objectName: "panel"
    property int ticks: 0
    property int step: 3
    Timer {
      id: timer
      objectName: "timer"
      property int twice: parent.step * 2
      running: true; repeat: true; interval: 50
      onTriggered: parent.ticks += parent.step
    }
  }
  property bool has: "parent" in timer
  property string name: timer.parent.objectName
}
"#,
    );
    let (panel, timer) = (h.find(root, "panel"), h.find(root, "timer"));
    assert_eq!(h.get(timer, "parent"), Value::Object(panel));
    assert_eq!(h.num(timer, "twice"), 6.0);
    assert_eq!(h.get(root, "has"), Value::Bool(true));
    assert_eq!(h.get(root, "name"), Value::from("panel"));
    h.pass_ms(120);
    assert_eq!(h.num(panel, "ticks"), 6.0);
    assert!(h.warnings().is_empty());
}

#[test]
fn a_timers_parent_follows_its_owner() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  Item { objectName: "a" }
  Item {
    objectName: "b"
    Timer { id: timer; objectName: "timer" }
  }
  property string name: timer.parent ? timer.parent.objectName : "none"
}
"#,
    );
    let (a, timer) = (h.find(root, "a"), h.find(root, "timer"));
    assert_eq!(h.get(root, "name"), Value::from("b"));
    h.engine.set_owner(timer, Some(a)).unwrap();
    assert_eq!(h.get(root, "name"), Value::from("a"));
    h.engine.set_owner(timer, None).unwrap();
    assert_eq!(h.get(root, "name"), Value::from("none"));
}

#[test]
fn parent_inside_a_plain_object_is_the_component_roots_parent() {
    // A `QtObject` has no `parent` property in Qt, so the name is looked
    // up further out and found on the component's root object.
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  id: root
  Item {
    id: inner
    objectName: "inner"
    Item {
      objectName: "panel"
      QtObject {
        objectName: "plain"
        property bool rootsParent: parent === root.parent
        property bool panel: parent !== null && parent.objectName === "panel"
      }
    }
  }
}
"#,
    );
    let plain = h.find(root, "plain");
    assert_eq!(h.get(plain, "rootsParent"), Value::Bool(true));
    assert_eq!(h.get(plain, "panel"), Value::Bool(false));
    assert!(h.warnings().is_empty());
}

const WALL_CLOCK_QML: &str = r#"import QtQuick
Item {
  property double now: Date.now()
  property double made: new Date().getTime()
  property double fixed: new Date(2020, 0, 2).getTime()
  property double parsed: Date.parse("2020-01-02T00:00:00Z")
  property bool isDate: new Date() instanceof Date
}
"#;

#[test]
fn a_pinned_wall_clock_is_what_date_reads_and_moves_with_the_clock() {
    let h = engine();
    h.engine.set_wall_clock_origin(Some(1_791_741_429_000.0));
    assert_eq!(h.engine.wall_clock_ms(), Some(1_791_741_429_000.0));
    let root = h.load(WALL_CLOCK_QML);
    assert_eq!(h.num(root, "now"), 1_791_741_429_000.0);
    assert_eq!(h.num(root, "made"), 1_791_741_429_000.0);
    assert_eq!(h.get(root, "isDate"), Value::Bool(true));
    h.pass_ms(1500);
    assert_eq!(h.engine.wall_clock_ms(), Some(1_791_741_430_500.0));
    // Dates made from arguments are not the clock's business.
    let unpinned = engine();
    let plain = unpinned.load(WALL_CLOCK_QML);
    assert_eq!(h.num(root, "fixed"), unpinned.num(plain, "fixed"));
    assert_eq!(h.num(root, "parsed"), 1_577_923_200_000.0);
}

#[test]
fn an_unpinned_wall_clock_is_the_systems() {
    let h = engine();
    assert_eq!(h.engine.wall_clock_ms(), None);
    let root = h.load(WALL_CLOCK_QML);
    let system = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        * 1000.0;
    assert!((h.num(root, "now") - system).abs() < 60_000.0);
    h.engine.set_wall_clock_origin(Some(0.0));
    h.engine.set_wall_clock_origin(None);
    assert_eq!(h.engine.wall_clock_ms(), None);
}

/// Only a pinned clock puts a function in front of the engine's `Date`:
/// an application on the machine's clock pays nothing for `new Date`.
#[test]
fn an_unpinned_wall_clock_puts_nothing_in_front_of_date() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  function source() { return Date.toString() }
  function pinned() { return new Date().getTime() }
}
"#,
    );
    let native = |h: &support::Harness| match h.engine.invoke_by_name(root, "source", &[]).unwrap()
    {
        Value::String(s) => s.contains("[native code]"),
        v => panic!("{v:?} {:?}", h.warnings()),
    };
    assert!(native(&h));
    h.engine.set_wall_clock_origin(Some(1_791_741_429_000.0));
    assert!(!native(&h));
    assert_eq!(
        h.engine.invoke_by_name(root, "pinned", &[]).unwrap(),
        Value::from(1_791_741_429_000.0)
    );
    h.engine.set_wall_clock_origin(None);
    assert!(native(&h));
}
