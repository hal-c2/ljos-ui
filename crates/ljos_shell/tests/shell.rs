//! ShellApp under egui_kittest's eframe mode: the inbox wakes the UI from
//! another thread, messages are drained at the start of a frame within the
//! budget, and an idle shell runs nothing.

use std::sync::Arc;
use std::time::Duration;

use egui::{Vec2, ViewportId};
use egui_kittest::Harness;
use ljos_engine::{Color, ObjectId, Value};
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
