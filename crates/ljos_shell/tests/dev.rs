//! The shell's `dev` feature: a project on disk is shown in a window, a
//! save is applied to it with its state kept, a mistake is shown in the
//! overlay while the application runs on, and a watched window that
//! nobody edits runs nothing.
//!
//! The tests wait for the watcher as the window does: asleep until egui
//! is asked for a repaint.
#![cfg(feature = "dev")]

use std::{
    fs,
    path::PathBuf,
    sync::{Arc, mpsc},
    time::Duration,
};

use egui::{Vec2, ViewportId};
use egui_kittest::{Harness, kittest::Queryable as _};
use ljos_engine::{Engine, ObjectId, Value};
use ljos_runtime::SystemClock;
use ljos_shell::{ShellApp, ShellConfig};

/// Long enough for a debug build on a loaded machine; a wake ends the
/// wait at once.
const PATIENCE: Duration = Duration::from_secs(30);

struct Window {
    // Dropped first: the watcher's thread ends before the directory goes.
    h: Harness<'static, ShellApp<()>>,
    woke: mpsc::Receiver<()>,
    dir: PathBuf,
    _tmp: tempfile::TempDir,
}

fn window(main: &str) -> Window {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    fs::write(dir.join("Main.qml"), main).unwrap();
    let (tx, woke) = mpsc::channel();
    let entry = dir.join("Main.qml");
    let h = Harness::builder()
        .with_size(Vec2::new(320.0, 200.0))
        .build_eframe(move |cc| {
            // What a real window's event loop sleeps on.
            cc.egui_ctx.set_request_repaint_callback(move |_| {
                let _ = tx.send(());
            });
            ShellApp::new(
                &cc.egui_ctx,
                &ShellConfig::default(),
                Arc::new(SystemClock::new()),
                |setup| setup.watch(&entry),
            )
            .unwrap()
        });
    let mut w = Window {
        h,
        woke,
        dir,
        _tmp: tmp,
    };
    w.h.run();
    w
}

impl Window {
    fn save(&self, text: &str) {
        fs::write(self.dir.join("Main.qml"), text).unwrap();
    }

    /// Runs frames as a window does, one batch per wake, until `done`.
    fn until(&mut self, done: impl Fn(&ShellApp<()>) -> bool) {
        while !done(self.h.state()) {
            self.woke
                .recv_timeout(PATIENCE)
                .expect("the watcher woke the window");
            self.h.run();
        }
    }

    fn engine(&self) -> &Engine {
        self.h.state().engine()
    }

    fn root(&self) -> ObjectId {
        self.h
            .state()
            .view()
            .scene()
            .root()
            .expect("a root is shown")
    }

    fn find(&self, name: &str) -> Option<ObjectId> {
        fn below(e: &Engine, obj: ObjectId, name: &str) -> Option<ObjectId> {
            if e.read_by_name(obj, "objectName").to_js_string().as_ref() == name {
                return Some(obj);
            }
            e.children(obj).into_iter().find_map(|c| below(e, c, name))
        }
        below(self.engine(), self.root(), name)
    }

    fn num(&self, name: &str, property: &str) -> f64 {
        let obj = self
            .find(name)
            .unwrap_or_else(|| panic!("no object named {name}"));
        self.engine().read_by_name(obj, property).to_number()
    }

    fn repaint_delay(&self) -> Duration {
        self.h.output().viewport_output[&ViewportId::ROOT].repaint_delay
    }
}

const MAIN: &str = r#"import QtQuick
Item {
  objectName: "root"
  property int taps: 0
  Rectangle { objectName: "box"; width: 10; height: 10; color: "green" }
}
"#;

const BROKEN: &str = r#"import QtQuick
Item {
  objectName: "root"
  property int taps: 0
  Rectangle { objectName: "box"; width: 10 +; height: 10 }
}
"#;

#[test]
fn a_save_is_applied_to_the_window_with_its_state_kept() {
    let mut w = window(MAIN);
    assert_eq!(w.h.state().reloads(), Some(0));
    let root = w.root();
    w.engine()
        .write_by_name(root, "taps", Value::Int(3))
        .unwrap();

    w.save(&MAIN.replace("width: 10;", "width: 50;"));
    w.until(|app| app.reloads() == Some(1));

    assert_eq!(w.num("box", "width"), 50.0);
    assert_eq!(w.num("root", "taps"), 3.0);
    assert_ne!(w.root(), root, "the new root is what is shown");
    assert!(!w.engine().is_alive(root));
    // The new root fills the window as the old one did.
    assert_eq!(w.num("root", "width"), 320.0);
    assert_eq!(w.num("root", "height"), 200.0);
    assert!(w.h.state().reload_overlay().is_none());
}

#[test]
fn a_mistake_is_shown_while_the_application_runs_on() {
    let mut w = window(MAIN);
    assert!(w.h.query_by_label("Reload failed").is_none());

    w.save(BROKEN);
    w.until(|app| app.reload_overlay().is_some());
    let overlay = w.h.state().reload_overlay().unwrap();
    assert!(
        overlay.location.starts_with("Main.qml:5"),
        "{}",
        overlay.location
    );
    // On screen, where the file and line are; the application under it
    // is as it was.
    assert!(w.h.query_by_label("Reload failed").is_some());
    assert!(w.h.query_by_label(&overlay.location).is_some());
    assert_eq!(w.num("box", "width"), 10.0);
    assert_eq!(w.h.state().reloads(), Some(0));
    // The overlay is text, not an animation.
    w.h.run();
    assert_eq!(w.repaint_delay(), Duration::MAX);

    w.save(&MAIN.replace("width: 10;", "width: 60;"));
    w.until(|app| app.reloads() == Some(1));
    assert!(w.h.state().reload_overlay().is_none());
    w.h.run();
    assert!(w.h.query_by_label("Reload failed").is_none());
    assert_eq!(w.num("box", "width"), 60.0);
}

#[test]
fn a_project_that_does_not_compile_opens_with_the_overlay() {
    let mut w = window(BROKEN);
    assert!(w.h.query_by_label("Reload failed").is_some());
    assert!(w.find("box").is_none());
    let empty = w.root();

    w.save(MAIN);
    w.until(|app| app.reloads() == Some(1));
    w.h.run();
    assert!(w.h.query_by_label("Reload failed").is_none());
    assert_eq!(w.num("box", "width"), 10.0);
    assert_eq!(w.num("root", "width"), 320.0);
    assert_eq!(w.num("root", "height"), 200.0);
    assert!(!w.engine().is_alive(empty), "the empty root is gone");
}

#[test]
fn a_watched_window_that_nobody_edits_runs_nothing() {
    let mut w = window(MAIN);
    w.h.run();
    let frames = w.h.state().view().stats().frames;
    while w.woke.try_recv().is_ok() {}
    for _ in 0..200 {
        w.h.step();
        assert_eq!(w.repaint_delay(), Duration::MAX);
    }
    assert_eq!(w.h.state().view().stats().frames, frames);
    // Nor does the watcher ask for a frame.
    assert!(w.woke.recv_timeout(Duration::from_millis(500)).is_err());
}

#[test]
fn a_window_that_is_not_watched_has_no_reloads() {
    let h: Harness<'static, ShellApp<()>> = Harness::builder().build_eframe(|cc| {
        ShellApp::new(
            &cc.egui_ctx,
            &ShellConfig::default(),
            Arc::new(SystemClock::new()),
            |setup| {
                let e = setup.engine();
                let root = e
                    .create_native(ljos_quick::Item::default(), None)
                    .map_err(|e| e.to_string())?;
                e.complete(root).map_err(|e| e.to_string())?;
                Ok(root)
            },
        )
        .unwrap()
    });
    assert_eq!(h.state().reloads(), None);
    assert!(h.state().reload_overlay().is_none());
}
