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
use qml_front::QtTypeOracle;

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
    app_with(files, Vec::new())
}

/// As [`app`], with the root given `properties` when it is made.
fn app_with(files: &[(&str, &str)], properties: Vec<(String, Value)>) -> App {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    for (name, text) in files {
        write(&dir, name, text);
    }
    let engine = Engine::new();
    ljos_quick::register(&engine);
    let (tx, woke) = mpsc::channel();
    let session = DevSession::open_with(
        &engine,
        &dir.join("Main.qml"),
        None,
        properties,
        QtTypeOracle,
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

/// Saves as most editors do, beside the file and renamed over it: the
/// file is never half written, so a build that runs between two events
/// of one save reads all of it or none of it.
fn write(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent).unwrap();
    let file = path.file_name().unwrap().to_string_lossy();
    let saving = parent.join(format!(".{file}.saving"));
    fs::write(&saving, text).unwrap();
    fs::rename(saving, path).unwrap();
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

    // A signal more on the root: the objects are made again.
    let moved = MAIN.replacen("{\n", "{\n  signal remade\n", 1);
    app.save("Main.qml", &moved.replace("width: 10;", "width: 3 * 10;"));
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

    // And again, from the generation it is at: only the binding changed,
    // so the objects are edited where they stand.
    app.call("box", "tap");
    let kept = app.scene.root();
    app.save("Main.qml", &moved.replace("width: 10;", "width: 4 * 10;"));
    let reports = app.wait();
    assert_eq!(reloaded(&reports).generation, 2);
    assert_eq!(app.num("box", "width"), 40.0);
    assert_eq!(app.num("box", "taps"), 3.0);
    assert_eq!(app.scene.root(), kept);
    assert_eq!(app.session.root(), kept);
}

#[test]
fn a_reload_tells_the_warnings_of_the_files_the_program_uses_only() {
    let main = "import QtQuick\nItem {\n  Used { }\n  property int n: 1\n}\n";
    let mut app = app(&[
        ("Main.qml", main),
        (
            "Used.qml",
            "import QtQuick\nItem {\n  property var v: usedUnknown\n}\n",
        ),
        // Below the entry, and nothing makes one.
        (
            "stray/Stray.qml",
            "import QtQuick\nItem {\n  property var v: strayUnknown\n}\n",
        ),
    ]);
    app.save("Main.qml", &main.replace("n: 1", "n: 2"));
    let reports = app.wait();
    let files: Vec<&str> = reloaded(&reports)
        .warnings
        .iter()
        .map(|w| w.file.as_str())
        .collect();
    assert_eq!(files, ["Used.qml"]);
}

#[test]
fn a_reload_tells_a_warning_once_until_it_goes_away() {
    let main = "import QtQuick\nItem {\n  Used { }\n  property int n: 1\n}\n";
    let used = "import QtQuick\nItem {\n  property var v: usedUnknown\n}\n";
    let mut app = app(&[("Main.qml", main), ("Used.qml", used)]);
    let told = |reports: &[Report]| -> Vec<String> {
        reloaded(reports)
            .warnings
            .iter()
            .map(|w| format!("{}:{}", w.file, w.line))
            .collect()
    };
    app.save("Main.qml", &main.replace("n: 1", "n: 2"));
    assert_eq!(told(&app.wait()), ["Used.qml:3"]);
    // Another save to a file that has none: the warning was told.
    app.save("Main.qml", &main.replace("n: 1", "n: 3"));
    assert_eq!(told(&app.wait()), Vec::<String>::new());
    // A new one is told, the old one not again.
    let two = used.replace("}\n", "  property var w: alsoUnknown\n}\n");
    app.save("Used.qml", &two);
    assert_eq!(told(&app.wait()), ["Used.qml:4"]);
    // Gone, then back: told again.
    app.save("Used.qml", "import QtQuick\nItem { }\n");
    assert_eq!(told(&app.wait()), Vec::<String>::new());
    app.save("Used.qml", used);
    assert_eq!(told(&app.wait()), ["Used.qml:3"]);
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
fn saves_while_a_reload_is_out_are_one_reload() {
    let mut app = app(&[("Main.qml", MAIN)]);
    // The watcher builds nothing while the application has a patch it has
    // not answered, so the saves made then are one build however the
    // machine schedules them; on a clock they could fall either side of
    // the quiet time. That every change restarts the quiet time is the
    // debouncer's own test, on a clock it controls.
    app.save("Main.qml", &MAIN.replace("width: 10;", "width: 20;"));
    app.woke
        .recv_timeout(PATIENCE)
        .expect("the watcher woke the host");
    app.save("Main.qml", &MAIN.replace("width: 10;", "width: 30;"));
    app.save("Main.qml", "import QtQuick\nItem {");
    app.save("Main.qml", &MAIN.replace("width: 10;", "width: 70;"));
    assert_eq!(reloaded(&app.pump()).generation, 1);
    assert_eq!(app.num("box", "width"), 20.0);

    let reports = app.wait();
    assert_eq!(reloaded(&reports).generation, 2);
    assert_eq!(app.num("box", "width"), 70.0);
    assert!(app.sleeps(QUIET * 3), "three saves, one reload");
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
fn a_save_that_imports_a_directory_nothing_reached_reloads_with_it() {
    let mut app = app(&[
        ("Main.qml", MAIN),
        ("parts/Knob.qml", "import QtQuick\nItem { width: 7 }\n"),
    ]);
    assert!(app.try_find("knob").is_none());
    app.save(
        "Main.qml",
        &MAIN
            .replace("import QtQuick", "import QtQuick\nimport \"parts\"")
            .replace(
                "Rectangle {",
                "Knob { objectName: \"knob\" }\n  Rectangle {",
            ),
    );
    reloaded(&app.wait());
    assert_eq!(app.num("knob", "width"), 7.0);

    // It is the session's now: an edit of it reloads.
    app.save("parts/Knob.qml", "import QtQuick\nItem { width: 9 }\n");
    reloaded(&app.wait());
    assert_eq!(app.num("knob", "width"), 9.0);
}

#[test]
fn a_file_only_a_loader_reaches_loads_and_its_saves_reload() {
    let page = "import QtQuick\nItem { objectName: \"page\"; width: 3 }\n";
    let mut app = app(&[
        (
            "Main.qml",
            "import QtQuick\nItem {\n  objectName: \"root\"\n  Loader { source: \"pages/Page.qml\" }\n}\n",
        ),
        ("pages/Page.qml", page),
    ]);
    assert_eq!(app.num("page", "width"), 3.0);

    app.save("pages/Page.qml", &page.replace("width: 3", "width: 5"));
    reloaded(&app.wait());
    assert_eq!(app.num("page", "width"), 5.0);
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

/// A picture `width` by `height`, saved as `write` saves.
fn save_picture(dir: &Path, name: &str, width: u32, height: u32) {
    let png = ljos_render::RgbaImage::filled(width, height, [255, 0, 0, 255])
        .to_png()
        .unwrap();
    let saving = dir.join(format!(".{name}.saving"));
    fs::write(&saving, png).unwrap();
    fs::rename(saving, dir.join(name)).unwrap();
}

const PICTURE: &str = r#"import QtQuick
Item {
  objectName: "root"
  width: 200; height: 200
  property int taps: 0
  Image { objectName: "logo"; source: "logo.png" }
}
"#;

#[test]
fn a_saved_picture_is_shown_again_and_nothing_is_reloaded() {
    let mut app = app(&[("Main.qml", PICTURE)]);
    // Ready = 1, Error = 3: there is no picture yet.
    assert_eq!(app.num("logo", "status"), 3.0);
    let root = app.find("root");
    app.engine
        .write_by_name(root, "taps", Value::Int(2))
        .unwrap();

    save_picture(&app.dir, "logo.png", 40, 20);
    assert_eq!(app.wait(), [Report::Pictures { shown: 1 }]);
    assert_eq!(app.num("logo", "status"), 1.0);
    assert_eq!(app.num("logo", "implicitWidth"), 40.0);

    save_picture(&app.dir, "logo.png", 60, 30);
    assert_eq!(app.wait(), [Report::Pictures { shown: 1 }]);
    assert_eq!(app.num("logo", "implicitWidth"), 60.0);
    assert_eq!(app.num("logo", "implicitHeight"), 30.0);
    assert_eq!(app.num("root", "taps"), 2.0);
    assert_eq!(app.session.reloads(), 0);
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

#[test]
fn a_list_view_and_its_model_are_kept_and_nothing_is_left_behind() {
    const LIST: &str = r##"import QtQuick
Rectangle {
  objectName: "root"
  width: 200; height: 100
  color: "#646464"
  ListView {
    objectName: "list"
    anchors.fill: parent
    orientation: ListView.Horizontal
    delegate: Rectangle {
      required property string city
      width: 100; height: 100
    }
    model: ListModel {
      ListElement { city: "Oslo" }
      ListElement { city: "Tokyo" }
      ListElement { city: "Lima" }
    }
  }
}
"##;
    let mut app = app(&[("Main.qml", LIST)]);
    app.save("Main.qml", &LIST.replace("#646464", "#123456"));
    reloaded(&app.wait());
    let objects = app.engine.object_count();
    for colour in ["#654321", "#123456", "#654321"] {
        app.save("Main.qml", &LIST.replace("#646464", colour));
        reloaded(&app.wait());
        assert_eq!(app.engine.object_count(), objects);
    }
}

#[test]
fn the_properties_the_root_was_given_are_given_again_at_each_reload() {
    const GIVEN: &str = r#"import QtQuick
Item {
  objectName: "root"
  required property url source
  property real side: 10
  Rectangle { objectName: "box"; width: side; height: 10 }
}
"#;
    let given = || vec![("source".to_owned(), Value::from("file:///clip.ogv"))];
    let mut app = app_with(&[("Main.qml", GIVEN)], given());
    assert_eq!(app.engine.last_reload_error(), None);
    let source = |app: &App| {
        app.engine
            .read_by_name(app.find("root"), "source")
            .to_js_string()
            .to_string()
    };
    assert_eq!(source(&app), "file:///clip.ogv");

    app.save("Main.qml", &GIVEN.replace("side: 10", "side: 30"));
    reloaded(&app.wait());
    assert_eq!(app.num("box", "width"), 30.0);
    assert_eq!(source(&app), "file:///clip.ogv");

    // A root the first save that compiles makes is given them too.
    let mut app = app_with(&[("Main.qml", "import QtQuick\nItem {")], given());
    assert_eq!(app.session.root(), None);
    app.save("Main.qml", GIVEN);
    reloaded(&app.wait());
    assert_eq!(source(&app), "file:///clip.ogv");
}

/// A save the editor finished (the file closed after writing, or renamed
/// over the old one) is built at once: the quiet time is for a write that
/// is still going on, which a finished save is not.
#[test]
fn a_finished_save_does_not_wait_the_quiet_time() {
    use std::io::Write as _;
    use std::time::Instant;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    write(&dir, "Main.qml", MAIN);
    let mut reloader = ljos_dev::Reloader::open(&dir, QtTypeOracle).unwrap();
    reloader.start();
    let (tx, rx) = mpsc::channel();
    let quiet = Duration::from_secs(5);
    let watcher = ljos_dev::Watcher::spawn_with_quiet(reloader, quiet, move |m| {
        tx.send((Instant::now(), m)).is_ok()
    })
    .unwrap();

    // Renamed over the old file.
    let saved = Instant::now();
    write(&dir, "Main.qml", &MAIN.replace("10;", "12;"));
    let (at, msg) = rx.recv_timeout(PATIENCE).expect("a build");
    assert!(matches!(msg, ljos_dev::DevMsg::Reload { .. }), "{msg:?}");
    assert!(at - saved < quiet, "took {:?}", at - saved);
    let ljos_dev::DevMsg::Reload { patch, .. } = msg else {
        unreachable!()
    };
    watcher.ack(patch.generation);

    // Written in place: nothing is built while the file is open, and the
    // build follows its closing.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(dir.join("Main.qml"))
        .unwrap();
    file.write_all(MAIN.replace("10;", "14;").as_bytes())
        .unwrap();
    file.flush().unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    let closed = Instant::now();
    drop(file);
    let (at, msg) = rx.recv_timeout(PATIENCE).expect("a build");
    assert!(matches!(msg, ljos_dev::DevMsg::Reload { .. }), "{msg:?}");
    assert!(at - closed < quiet, "took {:?}", at - closed);
}
