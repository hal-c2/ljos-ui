//! An application reaches side streams and window-scoped singletons
//! through `ljos` alone: nothing here names `ljos_engine`, `ljos_runtime`
//! or `ljos_shell`.

use std::cell::RefCell;
use std::rc::Rc;

use ljos::prelude::*;
use ljos::{BatchPolicy, BatchSender, Drain, DrainBudget, ShellConfig};

/// One for each window, which the host gives each window before its QML.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Facade", name = "Document")]
#[qml_singleton(scope = "window")]
pub struct Document {
    #[qml(notify)]
    pub title: Atom,
}

/// Generic over the trait, as a host's helper for any window's object is.
fn scope_of<T: QmlSingleton>() -> SingletonScope {
    T::SCOPE
}

#[test]
fn a_window_scoped_singleton_is_reached_through_the_facade() {
    assert_eq!(scope_of::<Document>(), SingletonScope::Window);
    let engine = ljos::engine();
    engine.register_singleton::<Document>("Facade", Version::new(1, 0));
    // What became of a window's object is told for a window with a root;
    // these have none, so nothing is told.
    let seen: Rc<RefCell<Vec<ScopedEvent>>> = Rc::default();
    let events = Rc::clone(&seen);
    engine.on_scoped_singleton(move |_, s: &ScopedSingleton| {
        if s.is::<Document>() {
            events.borrow_mut().push(s.event);
        }
    });
    let made = engine
        .create_native(
            Document {
                title: Atom::from("given"),
            },
            None,
        )
        .unwrap();
    let window: WindowScope = engine.window_scope();
    engine.set_singleton_in::<Document>(window, made).unwrap();
    assert_eq!(engine.singleton_in_scope::<Document>(window), Ok(made));
    // The window's QML is made in `in_window_scope`; with none, its result
    // is passed through.
    assert_eq!(engine.in_window_scope(window, || 7), 7);
    // A window of its own makes its own.
    let other = engine.window_scope();
    let theirs = engine.singleton_in_scope::<Document>(other).unwrap();
    assert_ne!(theirs, made);
    // A closed window has no singletons left to read.
    engine.close_window_scope(other).unwrap();
    assert!(engine.singleton_in_scope::<Document>(other).is_err());
    assert_eq!(engine.singleton_in_scope::<Document>(window), Ok(made));
    assert!(seen.borrow().is_empty(), "{:?}", seen.borrow());
}

/// Runs `run` on its own thread, failing the test if it has not ended in a
/// minute: a run that never quits would hang the test instead.
fn within_limit<T: Send + 'static>(run: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(run());
    });
    rx.recv_timeout(std::time::Duration::from_secs(60))
        .expect("the headless run ends when the application quits")
}

#[test]
fn a_side_stream_is_made_and_handled_through_the_facade() {
    const ITEMS: u32 = 1_000;
    let total = within_limit(|| {
        let total = Rc::new(RefCell::new(0u32));
        let sum = Rc::clone(&total);
        let config = ShellConfig {
            drain_budget: DrainBudget::UNLIMITED,
            ..ShellConfig::default()
        };
        ljos::run_headless::<()>(config, move |s| {
            let deltas: BatchSender<u32> = s.batch("facade.deltas", 64, BatchPolicy::DropOldest);
            s.on_batch(&deltas, move |engine: &Engine, run: Drain<'_, u32>| {
                for item in run {
                    *sum.borrow_mut() += 1;
                    if item == ITEMS {
                        engine.request_quit();
                    }
                }
            });
            std::thread::spawn(move || {
                for i in 1..=ITEMS {
                    // A full stream evicts its oldest: the last item is
                    // always handed over.
                    deltas.send(i).unwrap();
                }
            });
            let root = s
                .engine()
                .create_native(Item::default(), None)
                .map_err(|e| e.to_string())?;
            s.engine().complete(root).map_err(|e| e.to_string())?;
            Ok(root)
        })
        .expect("the headless run");
        *total.borrow()
    });
    assert!((1..=ITEMS).contains(&total), "{total}");
}
