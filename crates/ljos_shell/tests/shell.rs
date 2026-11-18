//! ShellApp under egui_kittest's eframe mode: the inbox wakes the UI from
//! another thread, messages are drained at the start of a frame within the
//! budget, and an idle shell runs nothing.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use egui::{Vec2, ViewportId};
use egui_kittest::Harness;
use ljos_engine::{Color, ObjectId, Value};
use ljos_quick::roots::{RootError, replace_root};
use ljos_quick::{Item, Rectangle, add_data};
use ljos_runtime::{DrainBudget, SystemClock};
use ljos_shell::{Setup, SetupError, ShellApp, ShellConfig};

const GREEN: Color = Color::from_rgba8(0, 255, 0, 255);

/// A root Item with one Rectangle; messages set the Rectangle's width.
fn setup(s: &mut Setup<'_, f64>) -> Result<ObjectId, SetupError> {
    let e = s.engine();
    let root = e
        .create_native(Item::default(), None)
        .map_err(|e| e.to_string())?;
    e.complete(root).map_err(|e| e.to_string())?;
    let r = e
        .create_native(Rectangle::default(), Some(root))
        .map_err(|e| e.to_string())?;
    add_data(e, root, r).map_err(|e| e.to_string())?;
    e.complete(r).map_err(|e| e.to_string())?;
    e.set(r, Item::WIDTH, 10.0).map_err(|e| e.to_string())?;
    e.set(r, Item::HEIGHT, 10.0).map_err(|e| e.to_string())?;
    e.write_by_name(r, "color", Value::Color(GREEN))
        .map_err(|e| e.to_string())?;
    s.on_message(move |e, w| {
        e.set(r, Item::WIDTH, w).unwrap();
    });
    Ok(root)
}

fn harness(config: ShellConfig) -> Harness<'static, ShellApp<f64>> {
    Harness::builder()
        .with_size(Vec2::new(320.0, 200.0))
        .build_eframe(move |cc| {
            ShellApp::new(&cc.egui_ctx, &config, Arc::new(SystemClock::new()), setup).unwrap()
        })
}

fn rect_width(h: &Harness<'_, ShellApp<f64>>) -> f64 {
    let e = h.state().engine();
    let root = h.state().view().scene().root().unwrap();
    let child = ljos_quick::child_items(e, root)[0];
    e.get(child, Item::WIDTH).unwrap()
}

fn repaint_delay(h: &Harness<'_, ShellApp<f64>>) -> Duration {
    h.output().viewport_output[&ViewportId::ROOT].repaint_delay
}

#[test]
fn the_root_fills_the_window() {
    let mut h = harness(ShellConfig::default());
    h.run();
    let e = h.state().engine();
    let root = h.state().view().scene().root().unwrap();
    assert_eq!(e.get(root, Item::WIDTH).unwrap(), 320.0);
    assert_eq!(e.get(root, Item::HEIGHT).unwrap(), 200.0);
}

#[test]
fn a_message_from_another_thread_wakes_the_ui() {
    let mut h = harness(ShellConfig::default());
    h.run();
    h.step();
    assert!(!h.ctx.has_requested_repaint());
    let frames = h.state().view().stats().frames;

    let handle = h.state().handle();
    std::thread::spawn(move || handle.send(42.0).unwrap())
        .join()
        .unwrap();
    assert!(h.ctx.has_requested_repaint(), "the inbox did not wake egui");

    h.run();
    assert_eq!(rect_width(&h), 42.0);
    assert_eq!(
        h.state().last_drain().drained,
        0,
        "later frames find it empty"
    );
    // Drained in `logic`, shown by the `ui` of the same frame: one frame.
    assert_eq!(h.state().view().stats().frames, frames + 1);
    assert_eq!(repaint_delay(&h), Duration::MAX);
}

#[test]
fn the_drain_budget_spreads_messages_over_frames() {
    let config = ShellConfig {
        drain_budget: DrainBudget::count(2),
        ..ShellConfig::default()
    };
    let mut h = harness(config);
    h.run();
    let handle = h.state().handle();
    for w in [1.0, 2.0, 3.0, 4.0, 5.0] {
        handle.send(w).unwrap();
    }
    h.step();
    assert_eq!(h.state().last_drain().drained, 2);
    assert_eq!(h.state().last_drain().remaining, 3);
    assert_eq!(rect_width(&h), 2.0);
    // The leftovers asked for another frame themselves.
    assert_eq!(repaint_delay(&h), Duration::ZERO);
    h.run();
    assert_eq!(rect_width(&h), 5.0);
    assert_eq!(repaint_delay(&h), Duration::MAX);
}

#[test]
fn an_idle_shell_runs_nothing() {
    let mut h = harness(ShellConfig::default());
    h.run();
    let frames = h.state().view().stats().frames;
    for _ in 0..200 {
        h.step();
        assert_eq!(repaint_delay(&h), Duration::MAX);
    }
    assert_eq!(h.state().view().stats().frames, frames);
}

/// A message the handler of [`a_panicking_message_is_a_fault_and_the_rest_are_handled`]
/// either takes or panics on.
enum Probe {
    Ok(u32),
    Boom,
}

#[test]
fn a_panicking_message_is_a_fault_and_the_rest_are_handled() {
    let ctx = egui::Context::default();
    let handled = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = std::rc::Rc::clone(&handled);
    // All three in one drain, however slow the panic's report is.
    let config = ShellConfig {
        drain_budget: DrainBudget::UNLIMITED,
        ..ShellConfig::default()
    };
    let mut app: ShellApp<Probe> =
        ShellApp::new(&ctx, &config, Arc::new(SystemClock::new()), move |s| {
            s.on_message(move |_, m| match m {
                Probe::Ok(n) => seen.borrow_mut().push(n),
                Probe::Boom => panic!("the handler panicked"),
            });
            let root = s
                .engine()
                .create_native(Item::default(), None)
                .map_err(|e| e.to_string())?;
            s.engine().complete(root).map_err(|e| e.to_string())?;
            Ok(root)
        })
        .unwrap();
    let heard = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let count = std::rc::Rc::clone(&heard);
    app.engine()
        .set_fault_handler(move |_, _| count.set(count.get() + 1));
    let h = app.handle();
    for m in [Probe::Ok(100), Probe::Boom, Probe::Ok(101)] {
        h.send(m).unwrap_or_else(|_| panic!("room in the inbox"));
    }

    let report = app.drain_inbox();
    assert_eq!(report.drained, 3);
    assert_eq!(*handled.borrow(), vec![100, 101]);
    let faults = app.engine().faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].kind(), "Panic");
    assert_eq!(faults[0].message(), "the handler panicked");
    app.engine().deliver_faults();
    assert_eq!(heard.get(), 1, "the fault handler heard of it");

    // And the application goes on.
    h.send(Probe::Ok(102)).unwrap_or_else(|_| panic!("room"));
    app.drain_inbox();
    assert_eq!(*handled.borrow(), vec![100, 101, 102]);
}

/// A message whose handler takes longer than the turn budget is named in
/// the warning by the kind the application gave it (issue #164).
#[test]
fn a_slow_message_is_named_by_its_kind() {
    let ctx = egui::Context::default();
    let config = ShellConfig {
        drain_budget: DrainBudget::UNLIMITED,
        ..ShellConfig::default()
    };
    let mut app: ShellApp<Probe> =
        ShellApp::new(&ctx, &config, Arc::new(SystemClock::new()), move |s| {
            s.name_messages(|m| match m {
                Probe::Ok(_) => "Ok",
                Probe::Boom => "Boom",
            });
            s.on_message(|_, _| std::thread::sleep(Duration::from_millis(20)));
            let root = s
                .engine()
                .create_native(Item::default(), None)
                .map_err(|e| e.to_string())?;
            s.engine().complete(root).map_err(|e| e.to_string())?;
            Ok(root)
        })
        .unwrap();
    app.engine().set_turn_budget(Some(Duration::from_millis(5)));
    app.engine().take_warnings();
    app.handle()
        .send(Probe::Ok(1))
        .unwrap_or_else(|_| panic!("room in the inbox"));
    app.drain_inbox();
    let warnings = app.engine().take_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0]
            .ends_with("longer than its budget of 5 ms: message `Ok` of mailbox `application`"),
        "{}",
        warnings[0]
    );
}

/// A root asked for and destroyed before the host makes the change is
/// dropped: the window keeps the root it shows (issue #141).
#[test]
fn a_root_destroyed_before_the_swap_leaves_the_one_shown() {
    let current = Rc::new(Cell::new(None::<ObjectId>));
    let root = current.clone();
    let mut h = Harness::builder()
        .with_size(Vec2::new(320.0, 200.0))
        .build_eframe(move |cc| {
            ShellApp::new(
                &cc.egui_ctx,
                &ShellConfig::default(),
                Arc::new(SystemClock::new()),
                move |s: &mut Setup<'_, f64>| {
                    let e = s.engine();
                    let first = e
                        .create_native(Item::default(), None)
                        .map_err(|e| e.to_string())?;
                    e.complete(first).map_err(|e| e.to_string())?;
                    root.set(Some(first));
                    s.on_message(move |e, _| {
                        let old = root.get().unwrap();
                        let next = e.create_native(Item::default(), None).unwrap();
                        e.complete(next).unwrap();
                        replace_root(e, old, next).unwrap();
                        e.destroy(next);
                    });
                    Ok(first)
                },
            )
            .unwrap()
        });
    h.run();
    let first = current.get().unwrap();
    h.state().handle().send(1.0).unwrap();
    h.run();
    let e = h.state().engine();
    assert!(e.is_alive(first));
    assert_eq!(h.state().view().scene().root(), Some(first));
    // It is the shown root again: another may take its window.
    let next = e.create_native(Item::default(), None).unwrap();
    assert_eq!(replace_root(e, first, next), Ok(()));
}

/// The application makes its interface again from a handler and puts the
/// new root where the old one was (issue #100's repro, with a message for
/// the click).
#[test]
fn the_application_puts_another_root_in_its_window() {
    let current = Rc::new(Cell::new(None::<ObjectId>));
    let root = current.clone();
    let mut h = Harness::builder()
        .with_size(Vec2::new(320.0, 200.0))
        .build_eframe(move |cc| {
            ShellApp::new(
                &cc.egui_ctx,
                &ShellConfig::default(),
                Arc::new(SystemClock::new()),
                move |s: &mut Setup<'_, f64>| {
                    let e = s.engine();
                    let first = e
                        .create_native(Item::default(), None)
                        .map_err(|e| e.to_string())?;
                    e.complete(first).map_err(|e| e.to_string())?;
                    root.set(Some(first));
                    s.on_message(move |e, _| {
                        let old = root.get().unwrap();
                        let next = e.create_native(Item::default(), None).unwrap();
                        e.complete(next).unwrap();
                        replace_root(e, old, next).unwrap();
                        root.set(Some(next));
                    });
                    Ok(first)
                },
            )
            .unwrap()
        });
    h.run();
    let first = current.get().unwrap();
    let handle = h.state().handle();
    handle.send(1.0).unwrap();
    handle.send(2.0).unwrap();
    h.run();
    let e = h.state().engine();
    let shown = h.state().view().scene().root().unwrap();
    assert_eq!(Some(shown), current.get());
    assert_ne!(shown, first);
    assert!(!e.is_alive(first), "the old root stays");
    assert_eq!(e.get(shown, Item::WIDTH).unwrap(), 320.0);
    assert_eq!(e.get(shown, Item::HEIGHT).unwrap(), 200.0);
    // Two asked for in one pass: the one between never had the window.
    assert_eq!(replace_root(e, first, shown), Err(RootError::Gone(first)));
    h.step();
    assert!(!h.ctx.has_requested_repaint());
}

#[test]
fn a_window_without_a_painter_says_which_painter_failed() {
    use ljos_shell::{PainterError, ShellError};
    use std::error::Error as _;
    let e = ShellError::from(PainterError::Cpu("no buffer".to_owned()));
    assert_eq!(e.to_string(), "graphics: no buffer");
    assert!(matches!(
        e.source().and_then(|s| s.downcast_ref::<PainterError>()),
        Some(PainterError::Cpu(_))
    ));
    let e = ShellError::from(PainterError::Gpu("no adapter".to_owned()));
    assert_eq!(e.to_string(), "graphics: no adapter");
}

/// What a side stream's handler saw: the length of each run it was handed.
type Runs = Rc<std::cell::RefCell<Vec<usize>>>;

/// [`setup`] with a side stream of `u32` named "deltas", whose handler
/// notes each run's length and sets the Rectangle's width to the run's
/// last item. The stream's sender is put in `sender`.
fn streaming_setup(
    s: &mut Setup<'_, f64>,
    capacity: usize,
    runs: &Runs,
    sender: &Rc<Cell<Option<ljos_runtime::BatchSender<u32>>>>,
) -> Result<ObjectId, SetupError> {
    let root = setup(s)?;
    let rect = ljos_quick::child_items(s.engine(), root)[0];
    let deltas = s.batch("deltas", capacity, ljos_runtime::BatchPolicy::DropNewest);
    let seen = Rc::clone(runs);
    s.on_batch(&deltas, move |e, run| {
        seen.borrow_mut().push(run.len());
        if let Some(last) = run.last() {
            e.set(rect, Item::WIDTH, f64::from(last)).unwrap();
        }
    });
    sender.set(Some(deltas));
    Ok(root)
}

/// A [`ShellApp`] with [`streaming_setup`], driven by hand.
fn streaming(
    config: &ShellConfig,
    capacity: usize,
) -> (ShellApp<f64>, ljos_runtime::BatchSender<u32>, Runs) {
    let runs: Runs = Rc::default();
    let sender = Rc::new(Cell::new(None));
    let app = ShellApp::new(
        &egui::Context::default(),
        config,
        Arc::new(SystemClock::new()),
        |s| streaming_setup(s, capacity, &runs, &sender),
    )
    .unwrap();
    let sender = sender.take().expect("the setup made the stream");
    (app, sender, runs)
}

#[test]
fn a_burst_into_a_side_stream_wakes_the_ui_once_and_is_one_turn() {
    let runs: Runs = Rc::default();
    let sender = Rc::new(Cell::new(None));
    let (made, seen) = (Rc::clone(&sender), Rc::clone(&runs));
    let config = ShellConfig {
        drain_budget: DrainBudget::UNLIMITED,
        ..ShellConfig::default()
    };
    let mut h = Harness::builder()
        .with_size(Vec2::new(320.0, 200.0))
        .build_eframe(move |cc| {
            ShellApp::new(&cc.egui_ctx, &config, Arc::new(SystemClock::new()), |s| {
                streaming_setup(s, 10_000, &seen, &made)
            })
            .unwrap()
        });
    h.run();
    h.step();
    assert!(!h.ctx.has_requested_repaint());
    let frames = h.state().view().stats().frames;

    let deltas = sender.take().expect("the setup made the stream");
    std::thread::spawn(move || {
        for i in 1..=10_000 {
            deltas.send(i).unwrap();
        }
    })
    .join()
    .unwrap();
    assert!(
        h.ctx.has_requested_repaint(),
        "the stream did not wake egui"
    );

    h.run();
    assert_eq!(*runs.borrow(), vec![10_000], "one turn for the burst");
    assert_eq!(rect_width(&h), 10_000.0);
    assert_eq!(h.state().view().stats().frames, frames + 1);
    // An empty stream is not handed an empty run, and wakes nothing.
    assert_eq!(repaint_delay(&h), Duration::MAX);
    assert_eq!(runs.borrow().len(), 1);
}

#[test]
fn side_streams_drain_after_the_inbox_out_of_the_same_budget() {
    let config = ShellConfig {
        drain_budget: DrainBudget::count(3),
        ..ShellConfig::default()
    };
    let (mut app, deltas, runs) = streaming(&config, 16);
    let handle = app.handle();
    handle.send(1.0).unwrap();
    handle.send(2.0).unwrap();
    for i in 1..=5 {
        deltas.send(i).unwrap();
    }
    // The inbox takes two of the three; the stream gets the last one.
    let report = app.drain_inbox();
    assert_eq!((report.drained, report.remaining), (3, 4));
    assert_eq!(*runs.borrow(), vec![1]);
    // A busy inbox does not starve the stream: it still gets one item.
    for w in [3.0, 4.0, 5.0] {
        handle.send(w).unwrap();
    }
    let report = app.drain_inbox();
    assert_eq!((report.drained, report.remaining), (4, 3));
    assert_eq!(*runs.borrow(), vec![1, 1]);
    let report = app.drain_inbox();
    assert_eq!((report.drained, report.remaining), (3, 0));
    assert_eq!(*runs.borrow(), vec![1, 1, 3]);
}

#[test]
fn a_panicking_batch_handler_is_a_fault_and_the_stream_goes_on() {
    let ctx = egui::Context::default();
    let handled = Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = Rc::clone(&handled);
    let sender = Rc::new(Cell::new(None));
    let made = Rc::clone(&sender);
    let mut app: ShellApp<()> = ShellApp::new(
        &ctx,
        &ShellConfig::default(),
        Arc::new(SystemClock::new()),
        move |s| {
            let lines = s.batch("lines", 8, ljos_runtime::BatchPolicy::DropOldest);
            s.on_batch(&lines, move |_, run| {
                for line in run {
                    assert_ne!(line, "boom", "the handler panicked");
                    seen.borrow_mut().push(line);
                }
            });
            made.set(Some(lines));
            let root = s
                .engine()
                .create_native(Item::default(), None)
                .map_err(|e| e.to_string())?;
            s.engine().complete(root).map_err(|e| e.to_string())?;
            Ok(root)
        },
    )
    .unwrap();
    let lines = sender.take().unwrap();
    for line in ["one", "boom", "lost"] {
        lines.send(line).unwrap();
    }
    assert_eq!(app.drain_inbox().drained, 3);
    assert_eq!(
        *handled.borrow(),
        vec!["one"],
        "the rest of the run went with it"
    );
    let faults = app.engine().faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].kind(), "Panic");
    lines.send("two").unwrap();
    app.drain_inbox();
    assert_eq!(*handled.borrow(), vec!["one", "two"]);
}

/// A run whose handler takes longer than the turn budget is named in the
/// warning by its stream.
#[test]
fn a_slow_run_is_named_by_its_stream() {
    let ctx = egui::Context::default();
    let sender = Rc::new(Cell::new(None));
    let made = Rc::clone(&sender);
    let mut app: ShellApp<()> = ShellApp::new(
        &ctx,
        &ShellConfig::default(),
        Arc::new(SystemClock::new()),
        move |s| {
            let chunks = s.batch("chunks", 4, ljos_runtime::BatchPolicy::DropNewest);
            s.on_batch(&chunks, |_, _| {
                std::thread::sleep(Duration::from_millis(20))
            });
            made.set(Some(chunks));
            let root = s
                .engine()
                .create_native(Item::default(), None)
                .map_err(|e| e.to_string())?;
            s.engine().complete(root).map_err(|e| e.to_string())?;
            Ok(root)
        },
    )
    .unwrap();
    app.engine().set_turn_budget(Some(Duration::from_millis(5)));
    app.engine().take_warnings();
    sender.take().unwrap().send(1u8).unwrap();
    app.drain_inbox();
    let warnings = app.engine().take_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("mailbox `chunks`"), "{}", warnings[0]);
}

#[test]
#[should_panic(expected = "was not made by this setup")]
fn a_handler_for_a_stream_another_setup_made_is_refused() {
    let ctx = egui::Context::default();
    let (inbox, handle) = ljos_runtime::UiInbox::<()>::channel(1);
    let (_stream, foreign) =
        handle.batch("foreign", 1, ljos_runtime::BatchPolicy::<u8>::DropNewest);
    let _ = ShellApp::<()>::new(
        &ctx,
        &ShellConfig::default(),
        Arc::new(SystemClock::new()),
        move |s| {
            s.on_batch(&foreign, |_, _| {});
            Err("unreachable".into())
        },
    );
    drop(inbox);
}

/// A stream made with no handler drops what it is sent, and says so once.
#[test]
fn a_stream_without_a_handler_drops_its_items_and_warns_once() {
    let sender = Rc::new(Cell::new(None));
    let made = Rc::clone(&sender);
    let mut app: ShellApp<()> = ShellApp::new(
        &egui::Context::default(),
        &ShellConfig::default(),
        Arc::new(SystemClock::new()),
        move |s| {
            made.set(Some(s.batch(
                "unheard",
                4,
                ljos_runtime::BatchPolicy::DropNewest,
            )));
            let root = s
                .engine()
                .create_native(Item::default(), None)
                .map_err(|e| e.to_string())?;
            s.engine().complete(root).map_err(|e| e.to_string())?;
            Ok(root)
        },
    )
    .unwrap();
    let unheard = sender.take().unwrap();
    let warned = |app: &ShellApp<()>| {
        app.engine()
            .log_snapshot()
            .iter()
            .filter(|e| e.message.contains("side stream \"unheard\" has no handler"))
            .count()
    };
    for round in 0..2 {
        unheard.send(round).unwrap();
        unheard.send(round).unwrap();
        let report = app.drain_inbox();
        assert_eq!((report.drained, report.remaining), (2, 0));
        assert_eq!(warned(&app), 1, "warned once, not every frame");
    }
}
