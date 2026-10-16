//! Background tasks as an application sees them: heavy work runs on the
//! compute pool, its value comes back on the Engine thread only when the
//! host delivers the platform inbox, and cancellation discards quietly.
//! Every wait is a channel or `Platform::finish`; nothing sleeps.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::ThreadId;

use ljos::prelude::*;
use ljos::task::{Lane, TaskPanic, spawn, spawn_in};

fn finish(engine: &Engine) {
    Platform::of(engine).finish(engine);
    assert_eq!(Platform::of(engine).pending(), 0);
}

#[test]
fn a_value_comes_back_only_after_delivery() {
    let engine = ljos::engine();
    let (worked_tx, worked_rx) = mpsc::channel::<()>();
    let got = Rc::new(RefCell::new(None::<i32>));
    let calls = Rc::new(Cell::new(0u32));
    let seen = Rc::clone(&got);
    let counted = Rc::clone(&calls);
    let task = spawn(
        &engine,
        move || {
            worked_tx.send(()).expect("test is listening");
            6 * 7
        },
        move |_, value| {
            counted.set(counted.get() + 1);
            *seen.borrow_mut() = Some(value.expect("work made a value"));
        },
    )
    .expect("queued");
    assert!(!task.is_cancelled());

    // The pool has done the work, but `done` runs only when the host
    // drains the inbox: this thread is the only one that can run it,
    // and it has not delivered yet.
    worked_rx.recv().expect("work ran");
    assert_eq!(*got.borrow(), None);
    assert_eq!(calls.get(), 0);

    finish(&engine);
    assert_eq!(*got.borrow(), Some(42));
    assert_eq!(calls.get(), 1);

    // Cancelling after delivery does nothing.
    task.cancel();
    assert!(task.is_cancelled());
    Platform::of(&engine).deliver(&engine);
    assert_eq!(calls.get(), 1);
    assert_eq!(*got.borrow(), Some(42));
}

#[test]
fn done_runs_on_the_engine_thread_and_work_does_not() {
    let engine = ljos::engine();
    let here = std::thread::current().id();
    let work_thread = Arc::new(Mutex::new(None::<ThreadId>));
    let in_work = Arc::clone(&work_thread);
    let done_thread = Rc::new(Cell::new(None::<ThreadId>));
    let in_done = Rc::clone(&done_thread);
    spawn(
        &engine,
        move || {
            *in_work.lock().expect("not poisoned") = Some(std::thread::current().id());
        },
        move |_, _: Result<(), _>| in_done.set(Some(std::thread::current().id())),
    )
    .expect("queued");

    finish(&engine);
    let work_thread = work_thread.lock().expect("not poisoned").expect("work ran");
    assert_ne!(work_thread, here, "work must leave the Engine thread");
    assert_eq!(done_thread.get(), Some(here), "done runs where QML lives");
}

#[test]
fn cancel_before_start_never_runs_the_work() {
    let engine = ljos::engine();
    // Fill the pool with work that waits for this test's word. The task
    // under test queues behind all of it in the same lane, so it cannot
    // start until every blocker has finished, whatever the pool's
    // thread count is.
    const BLOCKERS: usize = 16;
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let gate = Arc::new(Mutex::new(gate_rx));
    for _ in 0..BLOCKERS {
        let gate = Arc::clone(&gate);
        spawn(
            &engine,
            move || {
                let _ = gate.lock().expect("not poisoned").recv();
            },
            |_, _: Result<(), _>| {},
        )
        .expect("blocker queued");
    }

    let work_ran = Arc::new(AtomicBool::new(false));
    let in_work = Arc::clone(&work_ran);
    let done_ran = Rc::new(Cell::new(false));
    let in_done = Rc::clone(&done_ran);
    let task = spawn(
        &engine,
        move || {
            in_work.store(true, Ordering::SeqCst);
            7
        },
        move |_, _: Result<i32, _>| in_done.set(true),
    )
    .expect("queued");

    task.cancel();
    assert!(task.is_cancelled());
    for _ in 0..BLOCKERS {
        gate_tx.send(()).expect("a blocker is waiting");
    }
    finish(&engine);
    assert!(!work_ran.load(Ordering::SeqCst), "cancelled work never ran");
    assert!(!done_ran.get(), "and its done was never called");
}

#[test]
fn cancel_while_running_lets_it_finish_but_discards_the_result() {
    let engine = ljos::engine();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let work_finished = Arc::new(AtomicBool::new(false));
    let in_work = Arc::clone(&work_finished);
    let done_ran = Rc::new(Cell::new(false));
    let in_done = Rc::clone(&done_ran);
    let task = spawn(
        &engine,
        move || {
            started_tx.send(()).expect("test is listening");
            gate_rx.recv().expect("test releases the work");
            in_work.store(true, Ordering::SeqCst);
            9
        },
        move |_, _: Result<i32, _>| in_done.set(true),
    )
    .expect("queued");

    started_rx.recv().expect("work started");
    task.cancel();
    gate_tx.send(()).expect("work is waiting");
    finish(&engine);
    assert!(
        work_finished.load(Ordering::SeqCst),
        "running work is not interrupted"
    );
    assert!(!done_ran.get(), "but its result is discarded");
}

#[test]
fn dropping_the_handle_still_delivers() {
    let engine = ljos::engine();
    let got = Rc::new(RefCell::new(None::<String>));
    let seen = Rc::clone(&got);
    let task = spawn(
        &engine,
        || "fire and forget".to_owned(),
        move |_, value| *seen.borrow_mut() = value.ok(),
    )
    .expect("queued");
    drop(task);

    finish(&engine);
    assert_eq!(got.borrow().as_deref(), Some("fire and forget"));
}

#[test]
fn spawn_in_runs_in_the_named_lane_and_the_prelude_holds_the_handle() {
    let engine = ljos::engine();
    let got = Rc::new(Cell::new(0i32));
    let seen = Rc::clone(&got);
    // `Task` is the one task type the prelude names; the functions stay
    // behind the module.
    let task: Task = spawn_in(
        &engine,
        Lane::Idle,
        || 3 + 4,
        move |_, v| seen.set(v.unwrap_or(0)),
    )
    .expect("queued");
    assert!(!task.is_cancelled());
    finish(&engine);
    assert_eq!(got.get(), 7);
}

#[test]
fn a_panicking_work_reaches_done_and_the_fault_handler() {
    let engine = ljos::engine();
    let heard = Rc::new(RefCell::new(Vec::<String>::new()));
    let told = Rc::clone(&heard);
    engine.set_fault_handler(move |_, report| told.borrow_mut().push(report.message()));
    let got = Rc::new(RefCell::new(None::<Result<i32, TaskPanic>>));
    let seen = Rc::clone(&got);
    spawn(
        &engine,
        || -> i32 { panic!("work panicked") },
        move |_, outcome| *seen.borrow_mut() = Some(outcome),
    )
    .expect("queued");

    finish(&engine);
    let outcome = got.borrow_mut().take().expect("done ran");
    let panic = outcome.expect_err("the task ended without a value");
    assert_eq!(panic.message(), "work panicked");
    assert_eq!(panic.to_string(), "task panicked: work panicked");

    let faults = engine.faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].kind(), "Panic");
    assert_eq!(faults[0].message(), "work panicked");
    engine.deliver_faults();
    assert_eq!(*heard.borrow(), vec!["work panicked".to_owned()]);
}

#[test]
fn a_cancelled_task_whose_work_panicked_is_a_fault_but_done_stays_quiet() {
    let engine = ljos::engine();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let done_ran = Rc::new(Cell::new(false));
    let in_done = Rc::clone(&done_ran);
    let task = spawn(
        &engine,
        move || -> i32 {
            started_tx.send(()).expect("test is listening");
            gate_rx.recv().expect("test releases the work");
            panic!("cancelled work panicked")
        },
        move |_, _| in_done.set(true),
    )
    .expect("queued");
    started_rx.recv().expect("work started");
    task.cancel();
    gate_tx.send(()).expect("work is waiting");

    finish(&engine);
    assert!(!done_ran.get(), "a cancelled task's done never runs");
    assert_eq!(engine.faults().len(), 1, "but the bug is still reported");
}

#[test]
fn a_panicking_done_is_a_fault_and_other_answers_still_arrive() {
    let engine = ljos::engine();
    let got = Rc::new(Cell::new(0));
    let seen = Rc::clone(&got);
    spawn(
        &engine,
        || 1,
        |_, _: Result<i32, _>| panic!("done panicked"),
    )
    .expect("queued");
    spawn(&engine, || 2, move |_, v| seen.set(v.unwrap_or(0))).expect("queued");

    finish(&engine);
    assert_eq!(got.get(), 2);
    let faults = engine.faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].message(), "done panicked");
}
