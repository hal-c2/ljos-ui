//! A `Task` whose run could not start because the script heap was full
//! has failed, not pending for good, and starts again on its own once
//! there is room (ljos-ui#668).

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_runtime::{Clock, VirtualClock};

const MAIN: &str = r#"
import QtQuick
import Ljos.Core
Item {
    id: root
    property int n: 21
    Task {
        id: task
        objectName: "task"
        args: [root.n]
        run: (n: int): int => n * 2
    }
    property string shown: task.status + " " + task.value
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

fn shown(e: &Engine, root: ObjectId) -> String {
    get(e, root, "shown").to_js_string().to_string()
}

/// Runs what is due now: promise jobs and calls.
fn settle(e: &Engine, clock: &VirtualClock) {
    e.advance_timers(clock.now());
    e.run_pending_calls();
}

#[test]
fn a_task_whose_start_ran_out_of_heap_fails_and_starts_again_once_there_is_room() {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    compile(&e, &[("Main", MAIN)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    settle(&e, &clock);
    assert_eq!(shown(&e, root), "done 42");

    // Nearly no room: the next run cannot even start.
    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + 256)));
    set(&e, root, "n", Value::from(5.0));
    settle(&e, &clock);
    let status = get(&e, find(&e, root, "task"), "status")
        .to_js_string()
        .to_string();
    assert_eq!(status, "failed", "not left pending");
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w.contains("could not start")),
        "{warned:?}"
    );

    // While the heap stays full it is tried again, at a bounded rate.
    let mut tries = 0;
    let wait = loop {
        let due = e.next_timer_deadline().expect("the run is started again");
        let wait = due.saturating_since(clock.now());
        if !wait.is_zero() {
            break wait;
        }
        clock.set(due);
        settle(&e, &clock);
        tries += 1;
        assert!(tries < 3, "a zero wait is the first try only");
    };
    assert!(
        wait >= Duration::from_secs(1) && wait <= Duration::from_secs(60),
        "{wait:?}"
    );

    // Room again: it runs with the args it was given, with nobody
    // touching it.
    assert!(e.set_script_heap_limit(None));
    clock.set(clock.now() + wait);
    settle(&e, &clock);
    settle(&e, &clock);
    assert_eq!(shown(&e, root), "done 10");
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}
