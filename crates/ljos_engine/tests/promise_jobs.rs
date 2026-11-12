//! Promise jobs (ARCHITECTURE.md §7.2, step 3): a handler that queues one
//! wakes the host and makes it due at once, the host's next pass runs it,
//! and then nothing is due, so a host asleep stays asleep (rule 6). A job
//! cut off at its slice is reported as a dispatch cut off is.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::Value;
use ljos_runtime::{Fault, Time};
use support::engine;

const PAGE: &str = r#"import QtQuick
Item {
  objectName: "root"
  property int n: 0
  property string seen: "-"
  property bool spin: false
  onNChanged: {
    if (spin) Promise.resolve().then(() => { for (;;) {} })
    else Promise.resolve(n).then(v => seen = "got " + v)
  }
}
"#;

#[test]
fn a_queued_job_wakes_the_host_runs_on_its_next_pass_and_leaves_nothing_due() {
    let h = engine();
    let root = h.load(PAGE);
    h.pass_ms(0);
    assert!(!h.engine.has_due_calls());
    assert_eq!(h.engine.next_timer_deadline(), None, "an idle app sleeps");

    let wakes = Rc::new(Cell::new(0));
    let woken = wakes.clone();
    h.engine.set_timer_wake(move || woken.set(woken.get() + 1));
    h.set(root, "n", Value::Int(1));

    assert_eq!(
        h.get(root, "seen"),
        Value::from("-"),
        "not inside the handler"
    );
    assert!(wakes.get() > 0, "the host is woken");
    assert!(h.engine.has_due_calls());
    assert_eq!(h.engine.next_timer_deadline(), Some(Time::ZERO));

    h.pass_ms(0);
    assert_eq!(h.get(root, "seen"), Value::from("got 1"));
    assert!(!h.engine.has_due_calls());
    assert_eq!(h.engine.next_timer_deadline(), None, "and sleeps again");
}

#[test]
fn a_job_that_never_ends_is_cut_off_and_reported() {
    let h = engine();
    let root = h.load(PAGE);
    h.pass_ms(0);
    h.log();
    h.set(root, "spin", Value::Bool(true));
    h.set(root, "n", Value::Int(1));
    h.pass_ms(0);

    assert!(
        h.engine
            .faults()
            .iter()
            .any(|r| matches!(r.fault, Fault::JsInterrupted { .. })),
        "{:?}",
        h.engine.faults()
    );
    let warnings = h.warnings();
    assert!(
        warnings.iter().any(|w| w.contains("promise job")),
        "{warnings:?}"
    );
    assert!(!h.engine.has_due_calls(), "the job is gone, not retried");
}
