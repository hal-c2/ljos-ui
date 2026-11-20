//! A `ShellLoader` whose source's shell ran out of script heap shows its
//! fallback meanwhile and tries the source again on its own, at a bounded
//! rate, until there is room (ljos-ui#667). A shell that fails for its own
//! reasons stays on the fallback until the source changes, as before.

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_runtime::{Clock, VirtualClock};

const MAIN: &str = r#"
import QtQuick
import Ljos.Blocks
Item {
    id: root
    property bool on: false
    ShellLoader {
        objectName: "loader"
        source: root.on ? "SOURCE" : ""
        fallback: "FALLBACK"
    }
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

const HEAVY: &str = r#"
import QtQuick
Item {
    objectName: "heavy"
    property var lots: {
        const a = [];
        for (let i = 0; i < 20000; i++) a.push({ i: i, s: "row " + i });
        return a;
    }
}
"#;

/// A shell whose script runs past its slice while it is made.
const ENDLESS: &str = r#"
import QtQuick
Item {
    objectName: "endless"
    Component.onCompleted: { while (true) {} }
}
"#;

const LIGHT: &str = r#"
import QtQuick
Item { objectName: "light" }
"#;

fn status(e: &Engine, loader: ObjectId) -> i32 {
    num(e, loader, "status") as i32
}

fn shown(e: &Engine, loader: ObjectId) -> String {
    get(e, loader, "item")
        .as_object()
        .map(|o| get(e, o, "objectName").to_js_string().to_string())
        .unwrap_or_default()
}

/// What a frame does: what is due runs, the shown shell is read (its
/// bindings run when read), and faults go to their boundaries.
fn settle(e: &Engine, clock: &VirtualClock, loader: ObjectId) {
    e.advance_timers(clock.now());
    e.run_pending_calls();
    if let Some(item) = get(e, loader, "item").as_object() {
        let _ = get(e, item, "lots");
    }
    e.deliver_faults();
    e.process_deferred_deletes();
}

fn timed(source: &str) -> (Engine, Arc<VirtualClock>, ObjectId, ObjectId) {
    timed_with(source, "Light")
}

fn timed_with(source: &str, fallback: &str) -> (Engine, Arc<VirtualClock>, ObjectId, ObjectId) {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    compile(
        &e,
        &[
            (
                "Main",
                &MAIN.replace("SOURCE", source).replace("FALLBACK", fallback),
            ),
            ("Heavy", HEAVY),
            ("Endless", ENDLESS),
            ("Light", LIGHT),
        ],
    );
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let loader = find(&e, root, "loader");
    (e, clock, root, loader)
}

#[test]
fn a_shell_that_ran_out_of_heap_falls_back_and_is_tried_again_once_there_is_room() {
    let (e, clock, root, loader) = timed("Heavy");
    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + 64 * 1024)));
    set(&e, root, "on", Value::Bool(true));
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 2, "the fallback stands in");
    assert_eq!(shown(&e, loader), "light");

    // While the heap stays full the source is tried again, at a bounded
    // rate, with the fallback shown in between.
    let mut tries = 0;
    let wait = loop {
        let due = e.next_timer_deadline().expect("the source is tried again");
        let wait = due.saturating_since(clock.now());
        // Waits under a second are a first try at once, a timer step on.
        if wait >= Duration::from_secs(1) {
            break wait;
        }
        clock.set(due);
        settle(&e, &clock, loader);
        assert_eq!(status(&e, loader), 2);
        tries += 1;
        assert!(tries < 4, "tried at once only at first");
    };
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    let warned = e.take_warnings();
    assert!(
        warned
            .iter()
            .any(|w| w.contains("ShellLoader: trying the source again in")),
        "{warned:?}"
    );

    // Room again: the source's shell comes back with nobody touching it.
    assert!(e.set_script_heap_limit(None));
    clock.set(clock.now() + wait);
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 1, "{:?}", get(&e, loader, "error"));
    assert_eq!(shown(&e, loader), "heavy");
    assert_eq!(get(&e, loader, "error").to_js_string().to_string(), "");
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

#[test]
fn a_fallback_that_is_also_interrupted_charges_the_backoff_once_a_try() {
    let (e, clock, root, loader) = timed_with("Endless", "Endless");
    e.set_js_dispatch_budget(Duration::from_millis(5), Duration::from_millis(20))
        .unwrap();
    // No source: the fallback alone fails while the root is made (tried
    // at once) and again in the frame (a second on).
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 3);
    // The source fails, and the fallback with it.
    set(&e, root, "on", Value::Bool(true));
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 3, "{:?}", get(&e, loader, "error"));
    let _ = e.take_warnings();

    // Each try the source and the fallback both fail is one fault to the
    // backoff, not two: the wait doubles once a try (charged twice, it
    // would go 4 s, 16 s, 60 s), one warning a try.
    let mut waits = Vec::new();
    for _ in 0..5 {
        let due = e.next_timer_deadline().expect("the source is tried again");
        waits.push(due.saturating_since(clock.now()));
        clock.set(due);
        settle(&e, &clock, loader);
        assert_eq!(status(&e, loader), 3, "{:?}", get(&e, loader, "error"));
    }
    // Whole seconds: a wait runs a timer step over.
    let secs: Vec<u64> = waits.iter().map(Duration::as_secs).collect();
    assert_eq!(secs, [2, 4, 8, 16, 32], "{waits:?}");
    let warned = e.take_warnings();
    let tries = warned
        .iter()
        .filter(|w| w.contains("ShellLoader: trying the source again in"))
        .count();
    assert_eq!(tries, 5, "{warned:?}");
}

#[test]
fn a_shell_that_cannot_be_made_for_its_own_reasons_stays_on_the_fallback() {
    let (e, clock, root, loader) = timed("Missing");
    set(&e, root, "on", Value::Bool(true));
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 2);
    assert_eq!(shown(&e, loader), "light");
    assert_eq!(
        e.next_timer_deadline(),
        None,
        "only a resource fault is tried again"
    );
}
