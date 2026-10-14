//! The slow native call watchdog (slow.rs): disabled unless a budget is
//! set, then a native call over budget is recorded and named in the log,
//! at most once a second per method.

use std::time::Duration;

use ljos_engine::{Engine, ObjectId, QmlObject, QtObject, qml_impl};

/// A native type with a fast method and methods that block the thread the
/// way a synchronous service call would.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Slow", name = "Worker", extends = QtObject)]
struct Worker {
    #[qml(base)]
    base: QtObject,
}

#[qml_impl]
impl Worker {
    #[qml(invokable)]
    fn quick(&self) -> f64 {
        1.0
    }

    #[qml(invokable)]
    fn nap(&self) {
        std::thread::sleep(Duration::from_millis(25));
    }

    /// A shorter block, for tests that need many slow calls.
    #[qml(invokable)]
    fn pause(&self) {
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn worker(e: &Engine) -> ObjectId {
    e.create_native(Worker::default(), None).unwrap()
}

/// The watchdog's lines among the warnings so far; drains them.
fn slow_warnings(e: &Engine) -> Vec<String> {
    e.take_warnings()
        .into_iter()
        .filter(|w| w.contains("slow native call"))
        .collect()
}

#[test]
fn disabled_by_default_records_and_logs_nothing() {
    let e = Engine::new();
    let w = worker(&e);
    assert_eq!(e.slow_call_budget(), None);
    e.invoke_by_name(w, "nap", &[]).unwrap();
    assert_eq!(e.slow_call_count(), 0);
    assert!(e.take_slow_calls().is_empty());
    assert!(slow_warnings(&e).is_empty());
}

#[test]
fn a_call_over_budget_is_recorded_and_logged_once_a_second() {
    let e = Engine::new();
    let w = worker(&e);
    let budget = Duration::from_millis(5);
    e.set_slow_call_budget(Some(budget));
    assert_eq!(e.slow_call_budget(), Some(budget));

    e.invoke_by_name(w, "nap", &[]).unwrap();
    let calls = e.take_slow_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].ty, "Worker");
    assert_eq!(calls[0].method, "nap");
    assert!(calls[0].took >= budget, "took {:?}", calls[0].took);
    let logged = slow_warnings(&e);
    assert_eq!(logged.len(), 1);
    assert!(logged[0].contains("Worker.nap"), "{}", logged[0]);

    e.invoke_by_name(w, "nap", &[]).unwrap();
    e.invoke_by_name(w, "nap", &[]).unwrap();
    assert_eq!(e.slow_call_count(), 3);
    assert_eq!(e.take_slow_calls().len(), 2);
    assert!(
        slow_warnings(&e).is_empty(),
        "the same method is not logged again within a second"
    );

    e.invoke_by_name(w, "quick", &[]).unwrap();
    assert_eq!(e.slow_call_count(), 3, "a fast call is never recorded");
    assert!(e.take_slow_calls().is_empty());
}

#[test]
fn take_drains_the_ring_but_not_the_count() {
    let e = Engine::new();
    let w = worker(&e);
    e.set_slow_call_budget(Some(Duration::from_millis(5)));
    e.invoke_by_name(w, "nap", &[]).unwrap();
    e.invoke_by_name(w, "nap", &[]).unwrap();
    assert_eq!(e.take_slow_calls().len(), 2);
    assert!(e.take_slow_calls().is_empty());
    assert_eq!(e.slow_call_count(), 2);
    e.invoke_by_name(w, "nap", &[]).unwrap();
    assert_eq!(e.slow_call_count(), 3);
    assert_eq!(e.take_slow_calls().len(), 1);
}

#[test]
fn the_ring_keeps_only_the_last_64() {
    let e = Engine::new();
    let w = worker(&e);
    e.set_slow_call_budget(Some(Duration::from_millis(1)));
    for _ in 0..70 {
        e.invoke_by_name(w, "pause", &[]).unwrap();
    }
    assert_eq!(e.slow_call_count(), 70);
    assert_eq!(e.take_slow_calls().len(), 64);
}

#[test]
fn setting_the_budget_back_to_none_stops_recording() {
    let e = Engine::new();
    let w = worker(&e);
    e.set_slow_call_budget(Some(Duration::from_millis(5)));
    e.invoke_by_name(w, "nap", &[]).unwrap();
    assert_eq!(e.slow_call_count(), 1);
    e.take_warnings();

    e.set_slow_call_budget(None);
    assert_eq!(e.slow_call_budget(), None);
    e.invoke_by_name(w, "nap", &[]).unwrap();
    assert_eq!(e.slow_call_count(), 1);
    assert_eq!(e.take_slow_calls().len(), 1, "only the first was recorded");
    assert!(slow_warnings(&e).is_empty());
}
