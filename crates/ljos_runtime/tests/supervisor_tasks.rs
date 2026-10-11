//! Supervised tokio tasks: restart after a fault or a panic, cancellation,
//! giving up, and intensity on a virtual clock (ARCHITECTURE.md 9.4).

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use ljos_runtime::supervisor::tasks::TaskSupervisor;
use ljos_runtime::{ExitReason, Fault, Intensity, Outcome, Restart, Strategy, VirtualClock};
use tokio::runtime::{Builder, Handle, Runtime};

fn runtime() -> Runtime {
    Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("a runtime")
}

fn sup(max: u32, clock: &Rc<VirtualClock>, handle: Handle) -> TaskSupervisor {
    TaskSupervisor::new(
        "test",
        Strategy::OneForOne,
        Intensity::new(max, Duration::from_secs(30)),
        clock.clone(),
        handle,
    )
}

#[test]
fn a_task_that_faults_is_restarted_and_one_that_ends_normally_is_not() {
    let rt = runtime();
    let clock = Rc::new(VirtualClock::new());
    let mut sup = sup(5, &clock, rt.handle().clone());
    let runs = Arc::new(AtomicU32::new(0));
    let r = runs.clone();
    // Faults twice, then ends by itself.
    let child = sup.spawn("flaky", Restart::Transient, move |_token| {
        let n = r.fetch_add(1, Ordering::SeqCst);
        async move {
            if n < 2 {
                Err(Fault::Startup(format!("try {n}")))
            } else {
                Ok(())
            }
        }
    });
    rt.block_on(async {
        let exit = sup.next().await.expect("the first exit");
        assert_eq!(exit.child, child);
        assert_eq!(
            exit.reason,
            ExitReason::Fault(Fault::Startup("try 0".into()))
        );
        assert!(matches!(exit.outcome, Outcome::Restarted { .. }));
        assert_eq!(sup.run().await, Ok(()));
    });
    assert_eq!(runs.load(Ordering::SeqCst), 3);
    assert_eq!(sup.running(), 0);
    assert_eq!(sup.tree().recent_restarts(sup.tree().root()), 2);
}

#[test]
fn a_panic_in_a_task_is_a_fault() {
    let rt = runtime();
    let clock = Rc::new(VirtualClock::new());
    let mut sup = sup(5, &clock, rt.handle().clone());
    let runs = Arc::new(AtomicU32::new(0));
    let r = runs.clone();
    sup.spawn("panics once", Restart::Transient, move |_token| {
        let n = r.fetch_add(1, Ordering::SeqCst);
        async move {
            assert!(n > 0, "first run");
            Ok(())
        }
    });
    rt.block_on(async {
        let exit = sup.next().await.expect("the panic");
        match exit.reason {
            ExitReason::Fault(Fault::Panic(m)) => assert!(m.contains("first run"), "{m}"),
            other => panic!("not a panic: {other:?}"),
        }
        assert!(matches!(exit.outcome, Outcome::Restarted { .. }));
        assert_eq!(sup.run().await, Ok(()));
    });
    assert_eq!(runs.load(Ordering::SeqCst), 2);
}

#[test]
fn the_supervisor_gives_up_when_restarts_exceed_the_intensity() {
    let rt = runtime();
    let clock = Rc::new(VirtualClock::new());
    let mut sup = sup(2, &clock, rt.handle().clone());
    let runs = Arc::new(AtomicU32::new(0));
    let r = runs.clone();
    sup.spawn("broken", Restart::Permanent, move |_token| {
        r.fetch_add(1, Ordering::SeqCst);
        async { Err(Fault::JsOutOfMemory) }
    });
    // A healthy sibling that waits to be cancelled.
    let cancelled = Arc::new(AtomicU32::new(0));
    let c = cancelled.clone();
    sup.spawn("healthy", Restart::Permanent, move |token| {
        let c = c.clone();
        async move {
            token.cancelled().await;
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });
    let result = rt.block_on(async {
        let result = sup.run().await;
        // Giving up stopped the sibling; wait for it to notice.
        assert_eq!(sup.shutdown(Duration::from_secs(5)).await, 0);
        result
    });
    assert_eq!(result, Err(ExitReason::Fault(Fault::JsOutOfMemory)));
    // The first run and two restarts.
    assert_eq!(runs.load(Ordering::SeqCst), 3);
    assert_eq!(cancelled.load(Ordering::SeqCst), 1);
    assert!(sup.tree().is_down(sup.tree().root()));
}

#[test]
fn restarts_spread_over_virtual_time_never_exceed_the_intensity() {
    let rt = runtime();
    let clock = Rc::new(VirtualClock::new());
    let mut sup = sup(2, &clock, rt.handle().clone());
    let runs = Arc::new(AtomicU32::new(0));
    let r = runs.clone();
    sup.spawn("broken", Restart::Permanent, move |_token| {
        r.fetch_add(1, Ordering::SeqCst);
        async { Err(Fault::JsOutOfMemory) }
    });
    rt.block_on(async {
        for _ in 0..10 {
            let exit = sup.next().await.expect("an exit");
            assert!(
                matches!(exit.outcome, Outcome::Restarted { .. }),
                "{exit:?}"
            );
            // Only two restarts ever fall inside the 30 s window.
            clock.advance(Duration::from_secs(20));
        }
        assert_eq!(sup.shutdown(Duration::from_secs(5)).await, 0);
    });
    assert_eq!(runs.load(Ordering::SeqCst), 11);
}

#[test]
fn stopping_a_task_cancels_its_token_and_is_not_an_exit() {
    let rt = runtime();
    let clock = Rc::new(VirtualClock::new());
    let mut sup = sup(5, &clock, rt.handle().clone());
    let ended = Arc::new(AtomicU32::new(0));
    let e = ended.clone();
    let child = sup.spawn("waits", Restart::Permanent, move |token| {
        let e = e.clone();
        async move {
            token.cancelled().await;
            e.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });
    sup.stop(child).unwrap();
    // No supervised task is left: `run` drains the cancelled one and ends.
    assert_eq!(rt.block_on(sup.run()), Ok(()));
    assert_eq!(ended.load(Ordering::SeqCst), 1);
    assert!(sup.tree().children(sup.tree().root()).is_empty());
}

#[test]
fn shutdown_aborts_a_task_that_ignores_its_token() {
    let rt = runtime();
    let clock = Rc::new(VirtualClock::new());
    let mut sup = sup(5, &clock, rt.handle().clone());
    sup.spawn("deaf", Restart::Permanent, |_token| async {
        std::future::pending::<()>().await;
        Ok(())
    });
    sup.spawn("polite", Restart::Permanent, |token| async move {
        token.cancelled().await;
        Ok(())
    });
    let aborted = rt.block_on(sup.shutdown(Duration::from_millis(20)));
    assert_eq!(aborted, 1);
    assert_eq!(sup.running(), 0);
}

#[test]
fn a_temporary_task_is_forgotten_when_it_ends() {
    let rt = runtime();
    let clock = Rc::new(VirtualClock::new());
    let mut sup = sup(5, &clock, rt.handle().clone());
    let child = sup.spawn("once", Restart::Temporary, |_token| async {
        Err(Fault::JsOutOfMemory)
    });
    rt.block_on(async {
        let exit = sup.next().await.expect("its exit");
        assert_eq!(exit.outcome, Outcome::Stopped { child });
        assert!(sup.next().await.is_none());
    });
    assert!(sup.tree().children(sup.tree().root()).is_empty());
}

#[test]
fn the_presets_match_the_architecture() {
    let rt = runtime();
    let clock: Rc<VirtualClock> = Rc::new(VirtualClock::new());
    let services = TaskSupervisor::services(clock.clone(), rt.handle().clone());
    let plugins = TaskSupervisor::plugins(clock, rt.handle().clone());
    assert!(
        services
            .tree()
            .dump()
            .starts_with("services (OneForOne, 5 / 30s)")
    );
    assert!(
        plugins
            .tree()
            .dump()
            .starts_with("plugins (OneForOne, 3 / 60s)")
    );
}
