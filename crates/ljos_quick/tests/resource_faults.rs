//! What a full script heap, an interrupted script or a panic in native
//! work stops comes back on its own once the fault is over: a Repeater's
//! row, a view's delegate, a singleton.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, ObjectId, QmlObject, QtObject, Value, qml_impl};
use ljos_quick::QuickScene;
use ljos_runtime::{Clock, VirtualClock};

fn timed() -> (Engine, Arc<VirtualClock>) {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    (e, clock)
}

/// A file whose item needs its script while it is made, and the script
/// takes room to load: tens of kilobytes, against the few left free.
fn row() -> String {
    let mut s = String::from("import QtQuick\nItem {\n    width: f0(1, 2).length; height: 10\n");
    for i in 0..40 {
        s.push_str(&format!(
            "    function f{i}(a, b) {{ const t = [a, b, \"row {i}\"]; return t.map(x => String(x) + {i}).join(\"-\") }}\n"
        ));
    }
    s.push_str("}\n");
    s
}

const REPEATER: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property bool on: false
    Column {
        objectName: "column"
        Repeater { objectName: "repeater"; model: root.on ? 3 : 0; delegate: Line {} }
    }
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

/// Starts the script engine and leaves the heap `room` bytes from full.
fn fill(e: &Engine, root: ObjectId, room: usize) {
    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + room)));
}

fn widths(e: &Engine, repeater: ObjectId) -> Vec<f64> {
    (0..3)
        .map(|i| {
            e.invoke_by_name(repeater, "itemAt", &[Value::from(f64::from(i))])
                .ok()
                .and_then(|v| v.as_object())
                .map_or(-1.0, |o| num(e, o, "width"))
        })
        .collect()
}

#[test]
fn repeater_rows_tried_again_on_a_full_heap_collect_once_a_round() {
    let (e, clock) = timed();
    // Its script runs while it is made: the Repeater makes the row again
    // in its own timer, which runs out again before the next row's timer.
    let line = row().replace(
        "    width: f0(1, 2).length; height: 10\n",
        "    height: 10\n    Component.onCompleted: width = f0(1, 2).length\n",
    );
    compile(&e, &[("Main", REPEATER), ("Line", &line)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let repeater = find(&e, root, "repeater");
    fill(&e, root, 4 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    assert_eq!(widths(&e, repeater), [-1.0, -1.0, -1.0], "no row is made");
    // Every row is tried again together, and each try runs out again:
    // the first one's collection covers the rest, whose faults came
    // before it, round after round.
    for _ in 0..2 {
        let collections = e.script_memory().unwrap().collections;
        let due = e.next_timer_deadline().expect("the rows are tried again");
        clock.set(due);
        e.advance_timers(clock.now());
        frame(&mut s, &e);
        assert_eq!(widths(&e, repeater), [-1.0, -1.0, -1.0], "no row is made");
        assert_eq!(
            e.script_memory().unwrap().collections,
            collections + 1,
            "{:?}",
            e.take_warnings()
        );
    }
    // No retry went without the collection it needed: with room, they all
    // come back on the next round.
    assert!(e.set_script_heap_limit(None));
    let due = e.next_timer_deadline().expect("the rows are tried again");
    clock.set(due);
    e.advance_timers(clock.now());
    frame(&mut s, &e);
    assert_eq!(widths(&e, repeater), [12.0, 12.0, 12.0]);
}

#[test]
fn a_repeater_row_whose_script_ran_out_of_heap_comes_back_once_there_is_room() {
    let (e, clock) = timed();
    compile(&e, &[("Main", REPEATER), ("Line", &row())]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let repeater = find(&e, root, "repeater");
    fill(&e, root, 4 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    // The rows are made, but their script did not load.
    assert_eq!(widths(&e, repeater), [0.0, 0.0, 0.0]);
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w.contains("JS heap limit exceeded")),
        "{warned:?}"
    );
    // While the heap stays full, each try runs out again, at a bounded rate.
    let due = e.next_timer_deadline().expect("the rows are tried again");
    clock.set(due);
    e.advance_timers(clock.now());
    frame(&mut s, &e);
    assert_eq!(widths(&e, repeater), [0.0, 0.0, 0.0]);
    let due = e.next_timer_deadline().expect("and tried again after that");
    let wait = due.saturating_since(clock.now());
    assert!(
        wait >= Duration::from_secs(1) && wait <= Duration::from_secs(60),
        "{wait:?}"
    );
    // Whatever filled the heap lets go: the rows load their script, with
    // nobody changing the model.
    assert!(e.set_script_heap_limit(None));
    clock.set(due);
    e.advance_timers(clock.now());
    frame(&mut s, &e);
    assert_eq!(widths(&e, repeater), [12.0, 12.0, 12.0]);
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

const LIST_VIEW: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property bool on: false
    ListView {
        objectName: "view"
        width: 100; height: 100
        model: root.on ? 3 : 0
        delegate: Line {}
    }
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

fn delegate_widths(e: &Engine, view: ObjectId) -> Vec<f64> {
    (0..3)
        .map(|i| {
            e.invoke_by_name(view, "itemAtIndex", &[Value::from(f64::from(i))])
                .ok()
                .and_then(|v| v.as_object())
                .map_or(-1.0, |o| num(e, o, "width"))
        })
        .collect()
}

/// Lets the timers due by the next deadline run, and draws a frame.
fn wait_out(e: &Engine, clock: &VirtualClock, s: &mut QuickScene) -> Duration {
    let due = e.next_timer_deadline().expect("it is tried again");
    let wait = due.saturating_since(clock.now());
    clock.set(due);
    e.advance_timers(clock.now());
    frame(s, e);
    wait
}

#[test]
fn a_list_view_delegate_whose_script_ran_out_of_heap_comes_back_once_there_is_room() {
    let (e, clock) = timed();
    compile(&e, &[("Main", LIST_VIEW), ("Line", &row())]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let view = find(&e, root, "view");
    fill(&e, root, 4 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let before = delegate_widths(&e, view);
    assert!(before.iter().all(|w| *w != 12.0), "{before:?}");
    // Still full: tried again, at a bounded rate.
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert!(e.set_script_heap_limit(None));
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert_eq!(delegate_widths(&e, view), [12.0, 12.0, 12.0]);
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

const READS_THEME: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property bool on: false
    property int w: root.on ? Theme.w : -1
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

#[test]
fn a_singleton_whose_script_ran_out_of_heap_comes_back_once_there_is_room() {
    let (e, clock) = timed();
    let theme = format!(
        "pragma Singleton\n{}",
        row()
            .replace("Item {", "QtObject {\n    property int w: f0(1, 2).length")
            .replace("    width: f0(1, 2).length; height: 10\n", "")
    );
    compile(&e, &[("Main", READS_THEME), ("Theme", &theme)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    fill(&e, root, 4 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    assert_ne!(num(&e, root, "w"), 12.0);
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w.contains("JS heap limit exceeded")),
        "{warned:?}"
    );
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert_ne!(num(&e, root, "w"), 12.0);
    assert!(e.set_script_heap_limit(None));
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert_eq!(num(&e, root, "w"), 12.0, "the singleton is there again");
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

#[test]
fn a_singleton_that_ran_out_of_heap_while_it_was_made_is_made_again() {
    let (e, clock) = timed();
    // Its script runs while it is made, not when a binding is first read.
    let theme = format!(
        "pragma Singleton\n{}",
        row()
            .replace(
                "Item {",
                "QtObject {\n    property int w: 0\n    Component.onCompleted: w = f0(1, 2).length",
            )
            .replace("    width: f0(1, 2).length; height: 10\n", "")
    );
    compile(&e, &[("Main", READS_THEME), ("Theme", &theme)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    fill(&e, root, 4 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    assert_ne!(num(&e, root, "w"), 12.0);
    let warned = e.take_warnings();
    assert!(
        warned
            .iter()
            .any(|w| w.contains("the singleton Theme is not available")
                && w.contains("making it again")),
        "{warned:?}"
    );
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert_ne!(num(&e, root, "w"), 12.0);
    assert!(e.set_script_heap_limit(None));
    while e.next_timer_deadline().is_some() {
        let wait = wait_out(&e, &clock, &mut s);
        assert!(wait <= Duration::from_secs(60), "{wait:?}");
        if num(&e, root, "w") == 12.0 {
            break;
        }
    }
    assert_eq!(num(&e, root, "w"), 12.0, "the singleton is there again");
    frame(&mut s, &e);
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

#[test]
fn a_list_view_delegate_that_ran_out_of_heap_while_it_was_made_is_made_again() {
    let (e, clock) = timed();
    // Its script runs while it is made, not when a binding is first read.
    let line = row().replace(
        "    width: f0(1, 2).length; height: 10\n",
        "    height: 10\n    Component.onCompleted: width = f0(1, 2).length\n",
    );
    compile(&e, &[("Main", LIST_VIEW), ("Line", &line)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let view = find(&e, root, "view");
    fill(&e, root, 4 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let before = delegate_widths(&e, view);
    assert!(before.iter().all(|w| *w != 12.0), "{before:?}");
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert!(e.set_script_heap_limit(None));
    while e.next_timer_deadline().is_some() && delegate_widths(&e, view) != [12.0; 3] {
        let wait = wait_out(&e, &clock, &mut s);
        assert!(wait <= Duration::from_secs(60), "{wait:?}");
    }
    assert_eq!(delegate_widths(&e, view), [12.0, 12.0, 12.0]);
    frame(&mut s, &e);
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

const SLOW_BINDING: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property bool on: false
    property int made: 0
    readonly property var tries: ({ binding: 0, handler: 0 })
    // Runs over its time the first three times it is called for `what`.
    function slow(what) {
        if (tries[what]++ < 3) { for (;;) {} }
        return 5
    }
    ListView {
        objectName: "view"
        width: 100; height: 100
        model: root.on ? 3 : 0
        delegate: Item {
            width: 12; height: 10
            property int v: root.slow("binding")
            // Runs over its time while the delegate is made.
            Component.onCompleted: { root.made++; root.slow("handler") }
        }
    }
}
"#;

#[test]
fn a_list_view_delegate_whose_scripts_ran_over_their_time_is_shown_and_its_binding_runs_again() {
    let (e, clock) = timed();
    compile(&e, &[("Main", SLOW_BINDING)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let view = find(&e, root, "view");
    e.set_js_dispatch_budget(Duration::from_millis(10), Duration::from_millis(50))
        .unwrap();
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    // Made once and shown, though a script was stopped while it was made
    // and its binding was stopped after.
    assert_eq!(num(&e, root, "made"), 3.0);
    assert_eq!(delegate_widths(&e, view), [12.0, 12.0, 12.0]);
    let v = |e: &Engine| -> Vec<f64> {
        (0..3)
            .map(|i| {
                e.invoke_by_name(view, "itemAtIndex", &[Value::from(f64::from(i))])
                    .ok()
                    .and_then(|v| v.as_object())
                    .map_or(-1.0, |o| num(e, o, "v"))
            })
            .collect()
    };
    assert_eq!(v(&e), [0.0, 0.0, 0.0]);
    // The binding runs again after a wait; the delegate is not made again.
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert_eq!(v(&e), [5.0, 5.0, 5.0]);
    assert_eq!(num(&e, root, "made"), 3.0);
    frame(&mut s, &e);
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

/// A delegate that holds a boundary of its own, whose content fills the
/// heap as it is made: that boundary takes the fault, so the delegate is
/// whole, is shown at once and is not made again (ljos-ui#712).
const GUARDED_LINE: &str = r#"
import QtQuick
import Ljos
Item {
    width: 12; height: 10
    ErrorBoundary {
        objectName: "inner"
        Item {
            property var lots: []
            Component.onCompleted: {
                for (let i = 0; i < 20000; i++) lots.push({ i: i, s: "row " + i });
            }
        }
    }
}
"#;

#[test]
fn a_list_view_delegate_whose_own_boundary_took_a_full_heap_is_shown_at_once() {
    let (e, _clock) = timed();
    compile(&e, &[("Main", LIST_VIEW), ("Line", GUARDED_LINE)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let view = find(&e, root, "view");
    fill(&e, root, 64 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    assert_eq!(delegate_widths(&e, view), [12.0, 12.0, 12.0]);
    let first: Vec<_> = (0..3)
        .map(|i| e.invoke_by_name(view, "itemAtIndex", &[Value::from(f64::from(i))]))
        .map(|v| v.ok().and_then(|v| v.as_object()))
        .collect();
    for d in first.iter().flatten() {
        let inner = find(&e, *d, "inner");
        assert!(
            get(&e, inner, "failed").to_boolean(),
            "the inner one took it"
        );
    }
    frame(&mut s, &e);
    let again: Vec<_> = (0..3)
        .map(|i| e.invoke_by_name(view, "itemAtIndex", &[Value::from(f64::from(i))]))
        .map(|v| v.ok().and_then(|v| v.as_object()))
        .collect();
    assert_eq!(first, again, "the delegates are not made again");
    assert!(e.application_fault().is_none());
}

/// What fills the heap, and frees it when it is cut short: `FILL` below.
const FILLS: &str =
    r#"{ const a = []; for (let i = 0; i < 20000; i++) a.push({ i: i, s: "row " + i }) }"#;

/// A root that is whole as it is: what fills the heap is content that a
/// boundary inside it discards.
const CONTENT_FAULTS: [(&str, &str); 4] = [
    (
        "an ErrorBoundary's content",
        r#"ErrorBoundary { Item { id: c; Component.onCompleted: { FILL c.objectName = "back" } } }"#,
    ),
    (
        "a function of an ErrorBoundary's content",
        r#"ErrorBoundary { Item { id: c; function work() FILL Component.onCompleted: { c.work(); c.objectName = "back" } } }"#,
    ),
    (
        "a Loader's item",
        r#"Loader { sourceComponent: Component { Item { id: c; Component.onCompleted: { FILL c.objectName = "back" } } } }"#,
    ),
    (
        "a ShellLoader's shell",
        r#"ShellLoader { source: "Shell" }"#,
    ),
];

thread_local! {
    /// The panics left in [`Flaky`]: they outlast what is made again.
    static PANICS_LEFT: Cell<u32> = const { Cell::new(0) };
}

/// A native type whose method panics while there are panics left, then
/// works. Under the name of a type that is not `ljos_quick`'s to
/// implement, so the compiler knows it whichever table of types it reads.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "ButtonGroup", extends = QtObject)]
struct Flaky {
    #[qml(base)]
    base: QtObject,
}

#[qml_impl]
impl Flaky {
    #[qml(invokable)]
    fn work(&self) {
        let left = PANICS_LEFT.get();
        if left > 0 {
            PANICS_LEFT.set(left - 1);
            panic!("not yet");
        }
    }
}

/// A boundary that has given up by the time native work inside it no
/// longer panics, or cannot undo what the panic cut short outside it, with
/// how many panics that takes. `WORK` is the content's.
const PANIC_FAULTS: [(&str, u32, &str); 4] = [
    (
        "a Loader's item",
        1,
        "Loader { sourceComponent: Component { Item { WORK } } }",
    ),
    (
        "an ErrorBoundary that restarts nothing",
        1,
        "ErrorBoundary { maxRestarts: 0; Item { WORK } }",
    ),
    (
        "an ErrorBoundary past its restarts",
        4,
        "ErrorBoundary { Item { WORK } }",
    ),
    (
        "an outer object's function that called what panicked",
        1,
        r#"Item { id: h; function run(g) { if (d.started) return; d.started = true; g["work"](); d.ok = true } }
    ErrorBoundary { Item { ButtonGroup { id: p; Component.onCompleted: h.run(p) } } }"#,
    ),
];

/// Calls what panics from inside the content, then says it is whole.
const WORKS: &str = r#"ButtonGroup { id: f; Component.onCompleted: { f["work"](); d.ok = true } }"#;

/// A root left half made by a fault in an object of a boundary's own, one
/// that boundary never makes again.
const OWN_CHILD_FAULTS: [(&str, &str); 4] = [
    (
        "an ErrorBoundary's property object",
        "ErrorBoundary { property QtObject helper: QtObject { Component.onCompleted: { FILL d.ok = true } } Item {} }",
    ),
    (
        "a Loader's property object",
        "Loader { property QtObject helper: QtObject { Component.onCompleted: { FILL d.ok = true } } }",
    ),
    (
        "a Loader's declared child",
        "Loader { Item { Component.onCompleted: { FILL d.ok = true } } }",
    ),
    (
        "a ShellLoader's declared child",
        "ShellLoader { Item { Component.onCompleted: { FILL d.ok = true } } }",
    ),
];

/// A root left half updated by a fault in its own function, which content
/// of a boundary inside it called.
const OUTER_CODE_FAULTS: [(&str, &str); 4] = [
    (
        "called from an ErrorBoundary's content",
        "ErrorBoundary { Item { Component.onCompleted: d.init() } }",
    ),
    (
        "called from a Loader's item",
        "Loader { sourceComponent: Component { Item { Component.onCompleted: d.init() } } }",
    ),
    // A completed object's method is called with no host in between.
    (
        "in a function of an object declared before the boundary",
        r#"Item { id: h; function run() { if (d.started) return; d.started = true; FILL d.ok = true } }
    ErrorBoundary { Item { Component.onCompleted: h.run() } }"#,
    ),
    (
        "in a function of an object declared after the boundary",
        r#"ErrorBoundary { Item { Component.onCompleted: h.run() } }
    Item { id: h; function run() { if (d.started) return; d.started = true; FILL d.ok = true } }"#,
    ),
];

/// A root left half updated by content of a boundary inside it that wrote
/// to the root itself, not through any function of it, before it filled
/// the heap: the boundary discards its content, not what that wrote.
const OUTER_WRITE_FAULTS: [(&str, &str); 3] = [
    (
        "written by an ErrorBoundary's content",
        "ErrorBoundary { Item { Component.onCompleted: { if (d.started) return; d.started = true; FILL d.ok = true } } }",
    ),
    (
        "written by a Loader's item",
        "Loader { sourceComponent: Component { Item { Component.onCompleted: { if (d.started) return; d.started = true; FILL d.ok = true } } } }",
    ),
    (
        "written to a sibling of the boundary",
        r#"Item { id: h; property bool started: false }
    ErrorBoundary { Item { Component.onCompleted: { if (h.started) return; h.started = true; FILL d.ok = true } } }"#,
    ),
];

/// A root whole as it is, whose boundary's content wrote to itself, or
/// to an object that is not the root's, before it filled the heap: the
/// boundary discards the one, and the root holds neither.
const WRITE_ELSEWHERE_FAULTS: [(&str, &str); 2] = [
    (
        "content that wrote to itself",
        r#"ErrorBoundary { Item { id: c; Component.onCompleted: { c.objectName = "front"; FILL c.objectName = "back" } } }"#,
    ),
    (
        "content that wrote to a singleton the root does not hold",
        r#"ErrorBoundary { Item { id: c; Component.onCompleted: { Outside.touched++; FILL c.objectName = "back" } } }"#,
    ),
];

/// Content that connects a handler to the root before it fills the heap:
/// the connection outlasts the content the boundary discards.
const CONNECTS: &str = "ErrorBoundary { Item { Component.onCompleted: { d.ping.connect(function () { d.hits++ }); FILL d.ok = true } } }";

/// The file of what is made: `ok` says whether it is whole.
fn made_file(ok: bool, inner: &str, singleton: bool) -> String {
    format!(
        r#"{pragma}import QtQuick
import Ljos
import Ljos.Blocks
import QtQuick.Templates
Item {{
    id: d
    width: 12; height: 10
    property bool ok: {ok}
    property bool started: false
    property int hits: 0
    signal ping
    function poke() {{ ping(); return hits }}
    function init() {{
        if (started) return;
        started = true;
        FILL
        ok = true
    }}
    {inner}
}}
"#,
        pragma = if singleton { "pragma Singleton\n" } else { "" },
    )
    .replace("FILL", FILLS)
}

const INSTANTIATOR: &str = r#"
import QtQuick
Item {
    id: root
    property bool on: false
    Instantiator {
        objectName: "instantiator"
        asynchronous: true
        model: root.on ? 1 : 0
        delegate: Line {}
    }
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

const HOLDS_THEME: &str = r#"
import QtQuick
Item {
    id: root
    property bool on: false
    property var theme: root.on ? Theme : null
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

/// Each way an object is made that is thrown away when it is not whole.
#[derive(Clone, Copy)]
enum Path {
    Repeater,
    ListView,
    /// In slices, by a creation job.
    Incubated,
    Singleton,
}

struct Making {
    e: Engine,
    clock: Arc<VirtualClock>,
    s: QuickScene,
    root: ObjectId,
    path: Path,
}

impl Making {
    /// Makes `file` on `path` with the heap `room` bytes from full.
    fn start(path: Path, file: &str, room: usize) -> Self {
        let (e, clock) = timed();
        e.register_module(ljos_engine::module!("QtQuick.Templates" 2.0 => [Flaky]));
        let one = |main: &str| main.replace("root.on ? 3 : 0", "root.on ? 1 : 0");
        let (main, key) = match path {
            Path::Repeater => (one(REPEATER), "Line"),
            Path::ListView => (one(LIST_VIEW), "Line"),
            Path::Incubated => (INSTANTIATOR.to_owned(), "Line"),
            Path::Singleton => (HOLDS_THEME.to_owned(), "Theme"),
        };
        let shell = "import QtQuick\nItem { id: s; Component.onCompleted: { FILL s.objectName = \"back\" } }\n"
            .replace("FILL", FILLS);
        let outside = "pragma Singleton\nimport QtQuick\nQtObject { property int touched: 0 }\n";
        compile(
            &e,
            &[
                ("Main", &main),
                (key, file),
                ("Shell", &shell),
                ("Outside", outside),
            ],
        );
        let root = e.create_from_url("Main", None, Vec::new()).unwrap();
        fill(&e, root, room);
        set(&e, root, "on", Value::Bool(true));
        let s = scene(&e, root, 200.0, 200.0);
        let mut m = Self {
            e,
            clock,
            s,
            root,
            path,
        };
        // A job is made a slice a frame.
        for _ in 0..64 {
            frame(&mut m.s, &m.e);
            if m.object().is_some() {
                break;
            }
        }
        m
    }

    fn object(&self) -> Option<ObjectId> {
        let (e, root) = (&self.e, self.root);
        let call = |name: &str, method: &str| {
            e.invoke_by_name(find(e, root, name), method, &[Value::from(0.0)])
                .ok()
                .and_then(|v| v.as_object())
        };
        match self.path {
            Path::Repeater => call("repeater", "itemAt"),
            Path::ListView => call("view", "itemAtIndex"),
            Path::Incubated => call("instantiator", "objectAt"),
            Path::Singleton => get(e, root, "theme").as_object(),
        }
        .filter(|o| e.is_alive(*o))
    }

    fn is_whole(&self) -> bool {
        self.object()
            .is_some_and(|o| get(&self.e, o, "ok").to_boolean())
    }

    /// The heap has room again, and what waits for that runs.
    fn make_room(&mut self) {
        assert!(self.e.set_script_heap_limit(None));
        // A singleton is made after its frame's faults were delivered: no
        // timer waits until the next frame has delivered its own.
        frame(&mut self.s, &self.e);
        for _ in 0..16 {
            if self.e.next_timer_deadline().is_none() {
                break;
            }
            wait_out(&self.e, &self.clock, &mut self.s);
            for _ in 0..8 {
                frame(&mut self.s, &self.e);
            }
        }
    }
}

fn ran_out(m: &Making) -> bool {
    m.e.take_warnings()
        .iter()
        .any(|w| w.contains("JS heap limit exceeded"))
}

/// Whether an object named `name` is at or below `root`.
fn exists(e: &Engine, root: ObjectId, name: &str) -> bool {
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if !e.is_alive(o) {
            continue;
        }
        if get(e, o, "objectName").to_js_string().as_str() == name {
            return true;
        }
        stack.extend(e.children(o));
    }
    false
}

/// Every case of `inner` is kept as it was first made; says which are not.
fn assert_kept(path: Path, inner: &[(&str, &str)]) {
    let mut wrong = Vec::new();
    for (what, qml) in inner {
        let mut m = Making::start(
            path,
            &made_file(true, qml, matches!(path, Path::Singleton)),
            64 * 1024,
        );
        assert!(ran_out(&m), "{what}: nothing filled the heap");
        let Some(first) = m.object() else {
            wrong.push(format!("{what}: not shown"));
            continue;
        };
        m.make_room();
        assert!(!m.e.has_pending_faults(), "{what}: a fault was never taken");
        assert!(
            m.e.application_fault().is_none(),
            "{what}: the fault went past every boundary to the application"
        );
        if m.object() != Some(first) {
            wrong.push(format!("{what}: made again"));
        } else if !exists(&m.e, first, "back") {
            wrong.push(format!("{what}: what is inside never came back"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Every case of `inner` is thrown away and made again; says which are not.
fn assert_made_again(path: Path, inner: &[(&str, &str)]) {
    let mut wrong = Vec::new();
    for (what, qml) in inner {
        let mut m = Making::start(
            path,
            &made_file(false, qml, matches!(path, Path::Singleton)),
            64 * 1024,
        );
        assert!(ran_out(&m), "{what}: nothing filled the heap");
        if m.object().is_some() && !m.is_whole() {
            wrong.push(format!("{what}: shown half made"));
        }
        m.make_room();
        if !m.is_whole() {
            wrong.push(format!("{what}: never made again"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Every case of [`PANIC_FAULTS`] is whole once the panics are over, as it
/// is when what is made is thrown away; says which are not.
fn assert_recovers_from_panics(path: Path) {
    let mut wrong = Vec::new();
    for (what, panics, qml) in PANIC_FAULTS {
        PANICS_LEFT.set(panics);
        let inner = qml.replace("WORK", WORKS);
        let mut m = Making::start(
            path,
            &made_file(false, &inner, matches!(path, Path::Singleton)),
            // Room for all of it: only the panics break it.
            16 * 1024 * 1024,
        );
        assert!(PANICS_LEFT.get() < panics, "{what}: nothing panicked");
        m.make_room();
        if !m.is_whole() {
            wrong.push(format!("{what}: never whole again"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn a_repeater_row_recovers_from_panics_a_boundary_inside_gave_up_on() {
    assert_recovers_from_panics(Path::Repeater);
}

#[test]
fn a_list_view_delegate_recovers_from_panics_a_boundary_inside_gave_up_on() {
    assert_recovers_from_panics(Path::ListView);
}

#[test]
fn an_incubated_object_recovers_from_panics_a_boundary_inside_gave_up_on() {
    assert_recovers_from_panics(Path::Incubated);
}

#[test]
fn a_singleton_recovers_from_panics_a_boundary_inside_gave_up_on() {
    assert_recovers_from_panics(Path::Singleton);
}

#[test]
fn a_repeater_row_is_kept_when_content_inside_it_filled_the_heap() {
    assert_kept(Path::Repeater, &CONTENT_FAULTS);
}

#[test]
fn a_repeater_row_is_made_again_when_a_boundary_s_own_child_filled_the_heap() {
    assert_made_again(Path::Repeater, &OWN_CHILD_FAULTS);
}

#[test]
fn a_repeater_row_is_made_again_when_content_filled_the_heap_in_its_function() {
    assert_made_again(Path::Repeater, &OUTER_CODE_FAULTS);
}

#[test]
fn a_repeater_row_is_made_again_when_content_wrote_to_it_before_it_filled_the_heap() {
    assert_made_again(Path::Repeater, &OUTER_WRITE_FAULTS);
}

#[test]
fn a_list_view_delegate_is_kept_when_content_inside_it_filled_the_heap() {
    assert_kept(Path::ListView, &CONTENT_FAULTS);
}

#[test]
fn a_list_view_delegate_is_made_again_when_a_boundary_s_own_child_filled_the_heap() {
    assert_made_again(Path::ListView, &OWN_CHILD_FAULTS);
}

#[test]
fn a_list_view_delegate_is_made_again_when_content_filled_the_heap_in_its_function() {
    assert_made_again(Path::ListView, &OUTER_CODE_FAULTS);
}

#[test]
fn a_list_view_delegate_is_made_again_when_content_wrote_to_it_before_it_filled_the_heap() {
    assert_made_again(Path::ListView, &OUTER_WRITE_FAULTS);
}

#[test]
fn an_incubated_object_is_kept_when_content_inside_it_filled_the_heap() {
    assert_kept(Path::Incubated, &CONTENT_FAULTS);
}

#[test]
fn an_incubated_object_is_made_again_when_a_boundary_s_own_child_filled_the_heap() {
    assert_made_again(Path::Incubated, &OWN_CHILD_FAULTS);
}

#[test]
fn an_incubated_object_is_made_again_when_content_filled_the_heap_in_its_function() {
    assert_made_again(Path::Incubated, &OUTER_CODE_FAULTS);
}

#[test]
fn an_incubated_object_is_made_again_when_content_wrote_to_it_before_it_filled_the_heap() {
    assert_made_again(Path::Incubated, &OUTER_WRITE_FAULTS);
}

#[test]
fn a_singleton_is_kept_when_content_inside_it_filled_the_heap() {
    assert_kept(Path::Singleton, &CONTENT_FAULTS);
}

#[test]
fn a_singleton_is_made_again_when_a_boundary_s_own_child_filled_the_heap() {
    assert_made_again(Path::Singleton, &OWN_CHILD_FAULTS);
}

#[test]
fn a_singleton_is_made_again_when_content_filled_the_heap_in_its_function() {
    assert_made_again(Path::Singleton, &OUTER_CODE_FAULTS);
}

#[test]
fn a_singleton_is_made_again_when_content_wrote_to_it_before_it_filled_the_heap() {
    assert_made_again(Path::Singleton, &OUTER_WRITE_FAULTS);
}

#[test]
fn a_making_is_kept_when_content_inside_it_wrote_elsewhere_before_it_filled_the_heap() {
    for path in [
        Path::Repeater,
        Path::ListView,
        Path::Incubated,
        Path::Singleton,
    ] {
        assert_kept(path, &WRITE_ELSEWHERE_FAULTS);
    }
}

/// A root whose content connected to it and then filled the heap is made
/// again, not kept with that connection and another one the content's
/// next making adds: its signal reaches one handler.
#[test]
fn a_making_is_made_again_when_content_connected_to_it_before_it_filled_the_heap() {
    let mut wrong = Vec::new();
    for (what, path) in [
        ("a Repeater row", Path::Repeater),
        ("a ListView delegate", Path::ListView),
        ("an incubated object", Path::Incubated),
        ("a singleton", Path::Singleton),
    ] {
        let mut m = Making::start(
            path,
            &made_file(false, CONNECTS, matches!(path, Path::Singleton)),
            64 * 1024,
        );
        assert!(ran_out(&m), "{what}: nothing filled the heap");
        m.make_room();
        let Some(o) = m.object().filter(|_| m.is_whole()) else {
            wrong.push(format!("{what}: never whole"));
            continue;
        };
        let hits = m.e.invoke_by_name(o, "poke", &[]).unwrap().to_number();
        if hits != 1.0 {
            wrong.push(format!("{what}: its signal reached {hits} handlers"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The engine keeps who took the last 32 faults. Past that it cannot tell,
/// so what met them is not taken to be whole.
#[test]
fn a_making_is_kept_through_32_taken_faults_and_not_through_33() {
    for (count, kept) in [(32, true), (33, false)] {
        let inner = "ErrorBoundary { Item { Component.onCompleted: FILL } }\n".repeat(count);
        // Room for the boundaries themselves, none for one content's fill.
        let m = Making::start(Path::Repeater, &made_file(true, &inner, false), 512 * 1024);
        let faults =
            m.e.take_warnings()
                .iter()
                .filter(|w| w.contains("JS heap limit exceeded"))
                .count();
        assert!(faults >= count, "{count} boundaries, {faults} faults");
        assert_eq!(m.object().is_some(), kept, "{count} faults");
    }
}

/// What was thrown through belongs to one outermost run: an evaluation
/// makes a row as the model's own change does, whatever the run before it
/// threw and caught.
#[test]
fn a_row_an_evaluation_makes_is_kept_whatever_the_run_before_it_caught() {
    type Eval = fn(&Engine, ObjectId);
    let evals: [(&str, Eval); 2] = [
        ("eval_js", |e, _| {
            e.eval_js("Math.ljosRow.on = true; true").unwrap();
        }),
        ("eval_js_in", |e, root| {
            e.eval_js_in(
                Some(root),
                "Math.ljosRow.on = true; true",
                Duration::from_secs(1),
                [4, 16, 64, 64, 1024],
            )
            .unwrap();
        }),
    ];
    let main = REPEATER
        .replace("root.on ? 3 : 0", "root.on ? 1 : 0")
        .replace(
            "    function litter(n) {",
            "    function fail() { throw new Error(\"caught\") }\n    \
             function caught() { try { root.fail() } catch (why) { return true } return false }\n    \
             function share() { Math.ljosRow = root }\n    \
             function litter(n) {",
        );
    let mut wrong = Vec::new();
    for (what, eval) in evals {
        let (e, clock) = timed();
        // `made_file` imports the module `Making::start` registers.
        e.register_module(ljos_engine::module!("QtQuick.Templates" 2.0 => [Flaky]));
        let line = made_file(true, CONTENT_FAULTS[0].1, false);
        compile(&e, &[("Main", &main), ("Line", &line)]);
        let root = e.create_from_url("Main", None, Vec::new()).unwrap();
        e.invoke_by_name(root, "share", &[]).unwrap();
        // What the devtools install is not what fills the heap.
        e.eval_js_in(
            Some(root),
            "1",
            Duration::from_secs(1),
            [4, 16, 64, 64, 1024],
        )
        .unwrap();
        fill(&e, root, 64 * 1024);
        let caught = e.invoke_by_name(root, "caught", &[]).unwrap();
        assert!(caught.to_boolean(), "{what}: nothing was thrown");
        eval(&e, root);
        let mut s = scene(&e, root, 200.0, 200.0);
        frame(&mut s, &e);
        let mut m = Making {
            e,
            clock,
            s,
            root,
            path: Path::Repeater,
        };
        assert!(ran_out(&m), "{what}: nothing filled the heap");
        let Some(first) = m.object() else {
            wrong.push(format!("{what}: not shown"));
            continue;
        };
        m.make_room();
        assert!(!m.e.has_pending_faults(), "{what}: a fault was never taken");
        assert!(
            m.e.application_fault().is_none(),
            "{what}: the fault went past every boundary to the application"
        );
        if m.object() != Some(first) {
            wrong.push(format!("{what}: made again"));
        } else if !exists(&m.e, first, "back") {
            wrong.push(format!("{what}: what is inside never came back"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// A file's script loaded outside a run begins one: nothing enters script
/// between the exception the run before caught and the load that ran out.
#[test]
fn a_row_whose_content_could_not_load_its_script_is_kept_whatever_the_run_before_it_caught() {
    let main = REPEATER
        // Set from Rust: a binding's own run would begin a run before the load.
        .replace("model: root.on ? 3 : 0", "model: 0")
        .replace(
            "    function litter(n) {",
            "    function fail() { throw new Error(\"caught\") }\n    \
             function caught() { try { root.fail() } catch (why) { return true } return false }\n    \
             function litter(n) {",
        );
    let line = "import QtQuick\nimport Ljos\nimport Ljos.Blocks\n\
                Item { width: 12; height: 10; ErrorBoundary { Shout {} } }\n";
    // Its script is loaded by its first handler, and takes more than is free.
    let shout = row().replace(
        "    width: f0(1, 2).length; height: 10\n",
        "    height: 10\n    \
         Component.onCompleted: { width = f0(1, 2).length; objectName = \"back\" }\n",
    );
    let (e, clock) = timed();
    compile(&e, &[("Main", &main), ("Line", line), ("Shout", &shout)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let repeater = find(&e, root, "repeater");
    // Not `fill`, whose own run would forget what this one threw through.
    let caught = e.invoke_by_name(root, "caught", &[]).unwrap();
    assert!(caught.to_boolean(), "nothing was thrown");
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + 4 * 1024)));
    set(&e, repeater, "model", Value::Int(1));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let mut m = Making {
        e,
        clock,
        s,
        root,
        path: Path::Repeater,
    };
    assert!(ran_out(&m), "nothing filled the heap");
    let first = m.object().expect("the row is shown");
    m.make_room();
    assert_eq!(m.object(), Some(first), "the row is made again");
    assert!(
        exists(&m.e, first, "back"),
        "what is inside never came back"
    );
}
