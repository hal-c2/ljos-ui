//! The watchdog names the step that blocks, as cucumber really runs it.
//!
//! Cucumber hands its writers a scenario's step events only once the
//! scenario has finished, so the watchdog learns which step runs from the
//! steps themselves (`watched_steps`). This runs a feature whose third step
//! blocks until the watchdog fires, and pins the step it names.

use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

use cucumber::{World, WriterExt as _, given, then, when, writer};
use futures::FutureExt as _;
use ljos_bdd::watchdog::{Watchdog, watched_steps};

#[derive(Debug, Default, World)]
struct Plain;

/// Let go by the watchdog when it fires.
static RELEASE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

#[given("a step")]
#[when("a step")]
#[then("a step")]
fn a_step(_: &mut Plain) {}

#[when("a step blocks")]
fn a_step_blocks(_: &mut Plain) {
    let (released, wake) = &RELEASE;
    let released = released.lock().unwrap();
    let (_released, timeout) = wake
        .wait_timeout_while(released, Duration::from_secs(60), |r| !*r)
        .unwrap();
    assert!(!timeout.timed_out(), "the watchdog never fired");
}

const FEATURE: &str = "\
Feature: Watched
  Background:
    Given a step

  Scenario: One that blocks
    Given a step
    When a step blocks
    Then a step
";

#[test]
fn the_watchdog_names_the_step_that_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("watched.feature");
    std::fs::write(&path, FEATURE).unwrap();
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let dog = Watchdog::with_overrun(
        Duration::from_millis(300),
        dir.path(),
        Box::new(move |report| {
            let _ = tx.lock().unwrap().send(report.to_owned());
            let (released, wake) = &RELEASE;
            *released.lock().unwrap() = true;
            wake.notify_all();
        }),
    );
    let hook = Arc::new(dog.clone());
    let writer = writer::Basic::raw(std::io::sink(), writer::Coloring::Never, 0).normalized();
    futures::executor::block_on(
        Plain::cucumber()
            .with_writer(writer)
            .steps(watched_steps::<Plain>(&dog))
            .with_default_cli()
            .max_concurrent_scenarios(1)
            .before(move |feature, rule, scenario, _| {
                hook.scenario(feature, rule, scenario);
                async {}.boxed_local()
            })
            .run(path),
    );
    let report = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("the watchdog fired");
    assert_eq!(
        report,
        "ljos-bdd: a scenario ran past its deadline of 0 s (LJOS_BDD_SCENARIO_DEADLINE) and \
         failed; the run ends here\n  scenario: watched.feature:5: Scenario: One that blocks\n  \
         step 3 of 4: watched.feature:7: When a step blocks"
    );
}
