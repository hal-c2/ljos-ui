//! An `ErrorBoundary` whose content ran out of script heap shows its
//! fallback and makes the content again on its own, after a bounded wait,
//! however often it runs out: a resource fault is not counted toward the
//! restart intensity (ljos-ui#666). A fault of the content's own code is
//! counted and escalates as before.

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_runtime::{Clock, VirtualClock};
use proptest::prelude::*;

const MAIN: &str = r#"
import QtQuick
import Ljos
Item {
    id: root
    property bool on: false
    property bool broken: false
    ErrorBoundary {
        id: boundary
        objectName: "boundary"
        maxRestarts: 1
        fallback: Text { objectName: "sorry"; text: boundary.fault.kind }
        Item {
            objectName: "content"
            property int size: root.on ? root.make().length : 0
        }
    }
    property Component other: Component {
        Item { objectName: "other"; property int size: 1 }
    }
    function make() {
        const a = [];
        for (let i = 0; i < 20000; i++) a.push({ i: i, s: "row " + i });
        return a;
    }
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

fn shown(e: &Engine, boundary: ObjectId) -> String {
    get(e, boundary, "item")
        .as_object()
        .map(|o| get(e, o, "objectName").to_js_string().to_string())
        .unwrap_or_default()
}

/// What a frame does: what is due runs, what the boundary shows is read
/// (a binding runs when read), and faults go to their boundaries.
fn settle(e: &Engine, clock: &VirtualClock, boundary: ObjectId) {
    e.advance_timers(clock.now());
    e.run_pending_calls();
    if let Some(item) = get(e, boundary, "item").as_object() {
        let _ = get(e, item, "size");
    }
    e.deliver_faults();
    e.process_deferred_deletes();
}

#[test]
fn a_boundary_whose_content_keeps_running_out_of_heap_waits_and_never_gives_up() {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    compile(&e, &[("Main", MAIN)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let boundary = find(&e, root, "boundary");
    assert_eq!(shown(&e, boundary), "content");

    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + 64 * 1024)));
    set(&e, root, "on", Value::Bool(true));
    settle(&e, &clock, boundary);
    assert!(get(&e, boundary, "failed").to_boolean());
    assert_eq!(shown(&e, boundary), "sorry");

    // Far more faults than `maxRestarts` allows, and the boundary neither
    // gives up nor hands them on: after a first try at once (a timer step
    // on) it waits longer each time, up to a minute, the fallback shown.
    let mut waits = Vec::new();
    for _ in 0..10 {
        let due = e
            .next_timer_deadline()
            .expect("the content is made again after a wait");
        waits.push(due.saturating_since(clock.now()));
        clock.set(due);
        settle(&e, &clock, boundary);
        assert_eq!(shown(&e, boundary), "sorry");
        assert!(e.application_fault().is_none(), "not escalated");
    }
    let long: Vec<_> = waits
        .iter()
        .filter(|w| **w >= Duration::from_secs(1))
        .collect();
    assert!(long.len() >= 6, "{waits:?}");
    assert!(long.windows(2).all(|w| w[0] <= w[1]), "{waits:?}");
    // A minute at most, and the timer step a wait rounds up to.
    assert!(
        long.iter().all(|w| **w <= Duration::from_millis(60_010)),
        "{waits:?}"
    );
    let warned = e.take_warnings();
    assert!(
        warned
            .iter()
            .any(|w| w.contains("ErrorBoundary: JS out of memory, making the content again in")),
        "{warned:?}"
    );

    // Room again: the content comes back with nobody calling retry().
    assert!(e.set_script_heap_limit(None));
    let due = e.next_timer_deadline().expect("still trying");
    clock.set(due);
    settle(&e, &clock, boundary);
    assert!(!get(&e, boundary, "failed").to_boolean());
    assert_eq!(shown(&e, boundary), "content");
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
    assert!(e.application_fault().is_none());
}

#[test]
fn new_content_is_made_at_once_while_the_boundary_waits() {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    compile(&e, &[("Main", MAIN)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let boundary = find(&e, root, "boundary");
    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + 64 * 1024)));
    set(&e, root, "on", Value::Bool(true));
    // Into the long waits.
    for _ in 0..3 {
        settle(&e, &clock, boundary);
        let due = e.next_timer_deadline().expect("it waits");
        clock.set(due);
    }
    settle(&e, &clock, boundary);
    assert_eq!(shown(&e, boundary), "sorry");
    assert!(e.next_timer_deadline().is_some());

    // A change of what it shows is not left waiting for the timer.
    let other = get(&e, root, "other");
    set(&e, boundary, "content", other);
    settle(&e, &clock, boundary);
    assert!(!get(&e, boundary, "failed").to_boolean());
    assert_eq!(shown(&e, boundary), "other");
    assert_eq!(
        get(&e, boundary, "fault.kind").to_js_string().to_string(),
        ""
    );
    assert_eq!(e.next_timer_deadline(), None, "the wait was dropped");
}

#[derive(Clone, Copy, Debug)]
enum Op {
    /// The heap nearly full.
    Pressure,
    /// Room again.
    Room,
    /// `root.on` flipped: the content's binding allocates, or not.
    Toggle,
    /// A frame.
    Frame,
    /// To the next timer, and a frame.
    Tick,
    /// `retry()`.
    Retry,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        1 => Just(Op::Pressure),
        1 => Just(Op::Room),
        1 => Just(Op::Toggle),
        1 => Just(Op::Frame),
        2 => Just(Op::Tick),
        1 => Just(Op::Retry),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, ..ProptestConfig::default() })]

    /// Whatever the pressure does, the model's two promises hold: the
    /// boundary never gives up on a resource fault (nothing escalates, and
    /// a failed boundary always has its next try on a timer, at most a
    /// minute away), and once there is room it shows its content again
    /// within the waits the backoff allows.
    #[test]
    fn a_boundary_always_comes_back_from_resource_faults(ops in proptest::collection::vec(op(), 1..24)) {
        let e = engine();
        let clock = Arc::new(VirtualClock::new());
        e.set_clock(Rc::new(clock.clone()));
        compile(&e, &[("Main", MAIN)]);
        let root = e.create_from_url("Main", None, Vec::new()).unwrap();
        let boundary = find(&e, root, "boundary");
        e.invoke_by_name(root, "litter", &[Value::from(0.0)]).unwrap();
        prop_assert!(e.collect_garbage());
        let held = e.script_memory().unwrap().bytes;
        let mut on = false;
        let failed = |e: &Engine| get(e, boundary, "failed").to_boolean();
        for op in ops {
            match op {
                Op::Pressure => {
                    e.set_script_heap_limit(Some(held + 64 * 1024));
                }
                Op::Room => {
                    e.set_script_heap_limit(None);
                }
                Op::Toggle => {
                    on = !on;
                    set(&e, root, "on", Value::Bool(on));
                }
                Op::Frame => settle(&e, &clock, boundary),
                Op::Tick => {
                    if let Some(due) = e.next_timer_deadline() {
                        clock.set(due.max(clock.now()));
                    }
                    settle(&e, &clock, boundary);
                }
                Op::Retry => {
                    e.invoke_by_name(boundary, "retry", &[]).unwrap();
                    settle(&e, &clock, boundary);
                }
            }
            prop_assert!(e.application_fault().is_none(), "escalated after {op:?}");
            if failed(&e) {
                let due = e.next_timer_deadline();
                prop_assert!(due.is_some(), "failed with no try to come after {op:?}");
                let wait = due.unwrap().saturating_since(clock.now());
                prop_assert!(wait <= Duration::from_millis(60_010), "{wait:?}");
                prop_assert_eq!(shown(&e, boundary), "sorry");
            }
        }
        // Room: back within a few tries, without anyone's help.
        e.set_script_heap_limit(None);
        let mut ticks = 0;
        while failed(&e) {
            prop_assert!(ticks < 3, "still failed with room");
            let due = e.next_timer_deadline().unwrap();
            clock.set(due.max(clock.now()));
            settle(&e, &clock, boundary);
            ticks += 1;
        }
        prop_assert_eq!(shown(&e, boundary), "content");
        prop_assert!(e.application_fault().is_none());
    }
}
