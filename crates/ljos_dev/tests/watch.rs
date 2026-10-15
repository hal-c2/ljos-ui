//! End to end: files edited on disk under the watcher, and a scene that
//! follows them with its state kept. Also that the watcher sleeps while
//! nothing changes.
//!
//! The tests wait for the watcher's wake as a host does: blocked on a
//! channel the wake hook sends on, never looking at a clock in a loop.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Duration,
};

use ljos_dev::{DevSession, Host, QUIET, Report};
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::{Clock as _, SystemClock};

/// Long enough for a debug build on a loaded machine; a wake ends the
/// wait at once.
const PATIENCE: Duration = Duration::from_secs(30);

struct SceneHost<'a>(&'a mut QuickScene);

impl Host for SceneHost<'_> {
    fn window(&self) -> Option<ObjectId> {
        self.0.root()
    }

    fn show(&mut self, engine: &Engine, _old: Option<ObjectId>, new: ObjectId) {
        self.0.set_root(engine, new).expect("the root is an Item");
    }
}

struct App {
    // Dropped first: the watcher's thread ends before the directory goes.
    session: DevSession,
    engine: Engine,
    scene: QuickScene,
    clock: SystemClock,
    woke: mpsc::Receiver<()>,
    dir: PathBuf,
    _tmp: tempfile::TempDir,
}

fn app(files: &[(&str, &str)]) -> App {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    for (name, text) in files {
        write(&dir, name, text);
    }
    let engine = Engine::new();
    ljos_quick::register(&engine);
    let (tx, woke) = mpsc::channel();
    let session = DevSession::open(
        &engine,
        &dir.join("Main.qml"),
        Arc::new(move || {
            let _ = tx.send(());
        }),
    )
    .expect("the directory can be watched");
    let mut scene = QuickScene::new(kurbo::Size::new(400.0, 400.0));
    if let Some(root) = session.root() {
        scene.set_root(&engine, root).unwrap();
    }
    let mut app = App {
        session,
        engine,
        scene,
        clock: SystemClock::new(),
        woke,
        dir,
        _tmp: tmp,
    };
    app.frame();
    app
}

fn write(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

impl App {
    fn frame(&mut self) {
        for _ in 0..16 {
            self.scene.frame(&self.engine, self.clock.now());
            if !self.scene.needs_frame() {
                return;
            }
        }
    }

    fn save(&self, name: &str, text: &str) {
        write(&self.dir, name, text);
    }

    /// Sleeps until the watcher wakes the host, then runs the frame a
    /// host runs: the inbox, then the scene.
    fn wait(&mut self) -> Vec<Report> {
        self.woke
            .recv_timeout(PATIENCE)
            .expect("the watcher woke the host");
        self.pump()
    }

    fn pump(&mut self) -> Vec<Report> {
        let reports = self
            .session
            .pump(&self.engine, &self.clock, &mut SceneHost(&mut self.scene));
        self.frame();
        reports
    }

    /// Whether the host stays asleep for `time`.
    fn sleeps(&self, time: Duration) -> bool {
        self.woke.recv_timeout(time).is_err()
    }

    fn try_find(&self, name: &str) -> Option<ObjectId> {
        fn below(e: &Engine, obj: ObjectId, name: &str) -> Option<ObjectId> {
            if e.read_by_name(obj, "objectName").to_js_string().as_ref() == name {
                return Some(obj);
            }
            e.children(obj).into_iter().find_map(|c| below(e, c, name))
        }
        let root = self.scene.root().expect("something is shown");
        below(&self.engine, root, name)
    }

    fn find(&self, name: &str) -> ObjectId {
        self.try_find(name)
            .unwrap_or_else(|| panic!("no object named {name}"))
    }

    fn num(&self, name: &str, property: &str) -> f64 {
        self.engine
            .read_by_name(self.find(name), property)
            .to_number()
    }

    fn call(&self, name: &str, method: &str) {
        self.engine
            .invoke_by_name(self.find(name), method, &[])
            .unwrap_or_else(|e| panic!("{method}: {e}"));
    }
}

const MAIN: &str = r#"import QtQuick
Item {
  objectName: "root"
  width: 200; height: 200
  Rectangle {
    id: box
    objectName: "box"
    property int taps: 0
    function tap() { taps += 1 }
    width: 10; height: 10
  }
}
"#;

fn reloaded(reports: &[Report]) -> &ljos_dev::ReloadStats {
    match reports {
        [Report::Reloaded(stats)] => stats,
        other => panic!("expected one reload, got {other:?}"),
    }
}

#[test]
fn a_save_changes_the_scene_and_keeps_its_state() {
    let mut app = app(&[("Main.qml", MAIN)]);
    app.call("box", "tap");
    app.call("box", "tap");
    let before = app.scene.root();

    app.save("Main.qml", &MAIN.replace("width: 10;", "width: 30;"));
    let reports = app.wait();
    let stats = reloaded(&reports);
    assert_eq!(stats.generation, 1);
    assert_eq!((stats.kept, stats.fresh, stats.remounted), (2, 0, 0));

    // What the file now says, and what the user did.
    assert_eq!(app.num("box", "width"), 30.0);
    assert_eq!(app.num("box", "taps"), 2.0);
    assert_ne!(app.scene.root(), before);
    assert_eq!(app.session.root(), app.scene.root());
    assert_eq!(app.engine.reload_generation(), 1);
    assert_eq!(app.session.reloads(), 1);
    assert!(before.is_some_and(|old| !app.engine.is_alive(old)));

    // And again, from the generation it is at.
    app.call("box", "tap");
    app.save("Main.qml", &MAIN.replace("width: 10;", "width: 40;"));
    let reports = app.wait();
    assert_eq!(reloaded(&reports).generation, 2);
    assert_eq!(app.num("box", "width"), 40.0);
    assert_eq!(app.num("box", "taps"), 3.0);
}

#[test]
fn a_broken_save_keeps_the_scene_and_a_mended_one_clears_the_error() {
    let mut app = app(&[("Main.qml", MAIN)]);
    app.call("box", "tap");
    let before = app.scene.root();

    app.save("Main.qml", &MAIN.replace("width: 10;", "width: ;"));
    let reports = app.wait();
    let [Report::Failed { fault, .. }] = &reports[..] else {
        panic!("expected a failure, got {reports:?}");
    };
    assert_eq!(fault.file, "Main.qml");
    assert_eq!(fault.line, 10);
    assert_eq!(app.engine.last_reload_error().as_ref(), Some(fault));
    let overlay = ljos_dev::Overlay::current(&app.engine, app.session.more_errors()).unwrap();
    assert!(overlay.location.starts_with("Main.qml:10"), "{overlay:?}");
    // The old generation is live and whole.
    assert_eq!(app.scene.root(), before);
    assert_eq!(app.num("box", "width"), 10.0);
    assert_eq!(app.num("box", "taps"), 1.0);
    assert_eq!(app.engine.reload_generation(), 0);

    // The watcher goes on watching.
    app.save("Main.qml", &MAIN.replace("width: 10;", "width: 50;"));
    let reports = app.wait();
    reloaded(&reports);
    assert_eq!(app.engine.last_reload_error(), None);
    assert_eq!(app.num("box", "width"), 50.0);
    assert_eq!(app.num("box", "taps"), 1.0);
}

#[test]
fn undoing_a_broken_save_clears_the_error_without_a_reload() {
    let mut app = app(&[("Main.qml", MAIN)]);
    app.save("Main.qml", "import QtQuick\nItem {");
    assert!(matches!(app.wait()[..], [Report::Failed { .. }]));
    app.save("Main.qml", MAIN);
    assert_eq!(app.wait(), [Report::Cleared]);
    assert_eq!(app.engine.last_reload_error(), None);
    assert_eq!(app.engine.reload_generation(), 0);
}

#[test]
fn a_patch_the_engine_refuses_is_an_error_like_any_other() {
    let mut app = app(&[
        (
            "Main.qml",
            &MAIN.replace("Rectangle {", "Rectangle {\n    Badge {}"),
        ),
        ("Badge.qml", "import QtQuick\nItem {}\n"),
    ]);
    app.call("box", "tap");
    // It compiles, and cannot be made: nobody sets what it requires.
    app.save(
        "Badge.qml",
        "import QtQuick\nItem {\n  required property string label\n}\n",
    );
    let reports = app.wait();
    let [Report::Failed { fault, .. }] = &reports[..] else {
        panic!("expected a failure, got {reports:?}");
    };
    assert!(fault.message.contains("label"), "{fault:?}");
    assert_eq!(app.num("box", "taps"), 1.0);
    assert_eq!(app.engine.reload_generation(), 0);

    app.save(
        "Badge.qml",
        "import QtQuick\nItem {\n  property string label\n}\n",
    );
    let reports = app.wait();
    reloaded(&reports);
    assert_eq!(app.engine.last_reload_error(), None);
    assert_eq!(app.num("box", "taps"), 1.0);
}

#[test]
fn a_project_that_starts_broken_gets_its_root_when_it_compiles() {
    let mut app = app(&[("Main.qml", "import QtQuick\nItem {")]);
    assert_eq!(app.session.root(), None);
    assert_eq!(app.scene.root(), None);
    let fault = app.engine.last_reload_error().expect("it says why");
    assert_eq!(fault.file, "Main.qml");

    app.save("Main.qml", MAIN);
    let reports = app.wait();
    let stats = reloaded(&reports);
    assert_eq!((stats.generation, stats.kept), (1, 0));
    assert!(app.scene.root().is_some());
    assert_eq!(app.engine.last_reload_error(), None);
    assert_eq!(app.num("box", "width"), 10.0);

    // From here on it is a project like any other.
    app.call("box", "tap");
    app.save("Main.qml", &MAIN.replace("width: 10;", "width: 60;"));
    let reports = app.wait();
    reloaded(&reports);
    assert_eq!(app.num("box", "width"), 60.0);
    assert_eq!(app.num("box", "taps"), 1.0);
}

#[test]
fn saves_within_the_quiet_time_are_one_reload() {
    let mut app = app(&[("Main.qml", MAIN)]);
    // Back to back: a pause between the saves would be stretched past the
    // quiet time on a loaded machine. That every change restarts the quiet
    // time is the debouncer's own test, on a clock it controls.
    app.save("Main.qml", &MAIN.replace("width: 10;", "width: 20;"));
    app.save("Main.qml", "import QtQuick\nItem {");
    app.save("Main.qml", &MAIN.replace("width: 10;", "width: 70;"));
    let reports = app.wait();
    assert_eq!(reloaded(&reports).generation, 1);
    assert_eq!(app.num("box", "width"), 70.0);
    assert!(app.sleeps(QUIET * 3), "one save, one reload");
}

#[test]
fn a_file_in_a_new_directory_is_seen() {
    let mut app = app(&[("Main.qml", MAIN)]);
    app.save("parts/Knob.qml", "import QtQuick\nItem { width: 7 }\n");
    app.save(
        "Main.qml",
        &MAIN
            .replace("import QtQuick", "import QtQuick\nimport \"parts\"")
            .replace(
                "Rectangle {",
                "Knob { objectName: \"knob\" }\n  Rectangle {",
            ),
    );
    // On a loaded machine the two saves can be two reloads, the first
    // without the knob; the last one has it.
    let mut reports = app.wait();
    while app.try_find("knob").is_none() {
        reports = app.wait();
    }
    reloaded(&reports);
    assert_eq!(app.num("knob", "width"), 7.0);

    // The new directory is watched: an edit in it reloads.
    app.save("parts/Knob.qml", "import QtQuick\nItem { width: 9 }\n");
    let reports = app.wait();
    reloaded(&reports);
    assert_eq!(app.num("knob", "width"), 9.0);
}

#[test]
fn the_keyboard_focus_stays_where_the_user_put_it() {
    const TWO: &str = r#"import QtQuick
Item {
  width: 200; height: 200
  Item { id: one; objectName: "one"; focus: true }
  Item { id: two; objectName: "two"; width: 5 }
}
"#;
    let mut app = app(&[("Main.qml", TWO)]);
    app.scene.set_window_active(&app.engine, true);
    app.call("two", "forceActiveFocus");
    app.frame();
    assert_eq!(
        app.engine.read_by_name(app.find("two"), "activeFocus"),
        Value::Bool(true)
    );
    app.save("Main.qml", &TWO.replace("width: 5", "width: 6"));
    let reports = app.wait();
    reloaded(&reports);
    assert_eq!(app.num("two", "width"), 6.0);
    assert_eq!(
        app.engine.read_by_name(app.find("two"), "activeFocus"),
        Value::Bool(true)
    );
    assert_eq!(
        app.engine.read_by_name(app.find("one"), "activeFocus"),
        Value::Bool(false)
    );
}

#[test]
fn the_watcher_sleeps_while_nothing_changes() {
    let app = app(&[("Main.qml", MAIN), ("target/debug/Out.qml", MAIN)]);
    let before = app.session.watcher().wakes();
    assert_eq!(before, 0, "nothing has happened yet");

    // Reading, and files that are nothing to the project, wake nobody.
    let _ = fs::read_to_string(app.dir.join("Main.qml")).unwrap();
    app.save("notes.txt", "remember the milk");
    app.save(".Main.qml.swp", "swap");
    app.save("target/debug/Out.qml", "import QtQuick\nItem {}\n");
    assert!(app.sleeps(QUIET * 4));
    assert_eq!(app.session.watcher().wakes(), before);

    // A save that changes nothing wakes the watcher, which compiles
    // nothing and leaves the application asleep.
    app.save("Main.qml", MAIN);
    assert!(app.sleeps(QUIET * 4));
    let after = app.session.watcher().wakes();
    assert!(after > before);
    // And then it is asleep again.
    assert!(app.sleeps(QUIET * 4));
    assert_eq!(app.session.watcher().wakes(), after);
}

#[test]
fn a_flush_with_nothing_changed_is_answered_and_nothing_is_applied() {
    let mut app = app(&[("Main.qml", MAIN)]);
    app.session.reload_now();
    let reports = app.wait();
    assert_eq!(reports, [], "nothing was applied");
    assert!(app.session.take_flushed(), "the flush was answered");
    assert!(!app.session.take_flushed(), "and only once");
    assert!(app.sleeps(Duration::from_millis(300)), "then it rests");
}

#[test]
fn a_flush_applies_the_save_before_the_answer_to_it() {
    let mut app = app(&[("Main.qml", MAIN)]);
    app.save("Main.qml", &MAIN.replace("width: 10;", "width: 50;"));
    app.session.reload_now();
    let mut reports = Vec::new();
    while !app.session.take_flushed() {
        reports.extend(app.wait());
    }
    reloaded(&reports);
    assert_eq!(app.num("box", "width"), 50.0);
}

#[test]
fn a_flush_with_a_broken_save_reports_it_and_is_answered() {
    let mut app = app(&[("Main.qml", MAIN)]);
    app.save("Main.qml", "import QtQuick\nItem {\n");
    app.session.reload_now();
    let mut reports = Vec::new();
    while !app.session.take_flushed() {
        reports.extend(app.wait());
    }
    assert!(
        matches!(reports[..], [Report::Failed { .. }]),
        "{reports:?}"
    );
}
