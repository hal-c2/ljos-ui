//! A watch over several roots, an application's import path, with the
//! entry chosen again at every reload from a list: the first that
//! compiles and can be made is the root.
//!
//! The tests wait for the watcher's wake as a host does: blocked on a
//! channel the wake hook sends on.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Duration,
};

use ljos_dev::{DevSession, Host, Report};
use ljos_engine::{Engine, ObjectId};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::{Clock as _, SystemClock};
use qml_front::QtTypeOracle;

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
    // Dropped first: the watcher's thread ends before the directories go.
    session: DevSession,
    engine: Engine,
    scene: QuickScene,
    clock: SystemClock,
    woke: mpsc::Receiver<()>,
    dir: PathBuf,
    _tmp: tempfile::TempDir,
}

/// The roots `user`, `lib` and `app`, in that order, and the entries
/// `override` then `Main`.
fn app(files: &[(&str, &str)]) -> App {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    for root in ["user", "lib", "app"] {
        fs::create_dir_all(dir.join(root)).unwrap();
    }
    for (name, text) in files {
        write(&dir, name, text);
    }
    let engine = Engine::new();
    ljos_quick::register(&engine);
    let (tx, woke) = mpsc::channel();
    let roots = ["user", "lib", "app"].map(|r| dir.join(r));
    let roots: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
    let session = DevSession::open_roots(
        &engine,
        &roots,
        &["override", "Main"],
        Vec::new(),
        QtTypeOracle,
        Arc::new(move || {
            let _ = tx.send(());
        }),
    )
    .expect("the roots can be watched");
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

/// Saves beside the file and renames it over, as most editors do.
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

    fn remove(&self, name: &str) {
        fs::remove_file(self.dir.join(name)).unwrap();
    }

    fn wait(&mut self) -> Vec<Report> {
        self.woke
            .recv_timeout(PATIENCE)
            .expect("the watcher woke the host");
        let reports = self
            .session
            .pump(&self.engine, &self.clock, &mut SceneHost(&mut self.scene));
        self.frame();
        reports
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

    fn shown(&self) -> String {
        let root = self.scene.root().expect("something is shown");
        self.engine
            .read_by_name(root, "objectName")
            .to_js_string()
            .to_string()
    }

    fn tap(&self) {
        self.engine
            .invoke_by_name(self.find("box"), "tap", &[])
            .unwrap();
    }
}

const MAIN: &str = r#"import QtQuick
import Lib
Item {
  objectName: "main"
  Badge {
    id: box
    objectName: "box"
    property int taps: 0
    function tap() { taps += 1 }
  }
}
"#;

const BADGE: &str = "import QtQuick\nRectangle { width: 10; height: 10 }\n";

const OVERRIDE: &str = r#"import QtQuick
import Lib
Item {
  objectName: "override"
  Badge { objectName: "mine" }
}
"#;

fn module() -> [(&'static str, &'static str); 3] {
    [
        ("app/Main.qml", MAIN),
        ("lib/Lib/qmldir", "module Lib\nBadge 1.0 Badge.qml\n"),
        ("lib/Lib/Badge.qml", BADGE),
    ]
}

fn reloaded(reports: &[Report]) -> &ljos_dev::ReloadStats {
    match reports {
        [Report::Reloaded(stats)] => stats,
        other => panic!("expected one reload, got {other:?}"),
    }
}

#[test]
fn a_module_of_another_root_is_imported_and_its_saves_reload() {
    let mut app = app(&module());
    assert_eq!(app.engine.last_reload_error(), None);
    assert_eq!(app.session.entry(), "Main");
    assert_eq!(app.num("box", "width"), 10.0);
    app.tap();

    app.save(
        "lib/Lib/Badge.qml",
        &BADGE.replace("width: 10", "width: 30"),
    );
    reloaded(&app.wait());
    assert_eq!(app.num("box", "width"), 30.0);
    assert_eq!(app.num("box", "taps"), 1.0, "the state stays");
}

#[test]
fn a_file_new_to_a_root_is_part_of_the_next_reload() {
    let mut app = app(&module());
    app.save(
        "app/Extra.qml",
        "import QtQuick\nItem { objectName: \"extra\" }\n",
    );
    app.save(
        "app/Main.qml",
        &MAIN.replace("  Badge {", "  Extra { }\n  Badge {"),
    );
    // The two saves may be one reload or two.
    let mut reports = app.wait();
    while app.try_find("extra").is_none() {
        assert!(!reports.is_empty(), "nothing came");
        reports = app.wait();
    }
    assert_eq!(app.engine.last_reload_error(), None);
}

#[test]
fn an_entry_that_appears_takes_the_window_and_gives_it_back_when_it_goes() {
    let mut app = app(&module());
    assert_eq!(app.shown(), "main");
    let main = app.scene.root();

    app.save("user/override.qml", OVERRIDE);
    reloaded(&app.wait());
    assert_eq!(app.shown(), "override");
    assert_eq!(app.session.entry(), "override");
    assert_eq!(app.session.root(), app.scene.root());
    assert!(
        main.is_some_and(|m| !app.engine.is_alive(m)),
        "the old root stays"
    );
    assert_eq!(app.num("mine", "width"), 10.0);

    app.remove("user/override.qml");
    reloaded(&app.wait());
    assert_eq!(app.shown(), "main");
    assert_eq!(app.session.entry(), "Main");
    assert_eq!(app.engine.last_reload_error(), None);
}

#[test]
fn an_entry_that_no_longer_compiles_gives_the_window_to_the_next() {
    let mut files = module().to_vec();
    files.push(("user/override.qml", OVERRIDE));
    let mut app = app(&files);
    assert_eq!(app.shown(), "override");

    app.save("user/override.qml", "import QtQuick\nItem {");
    let reports = app.wait();
    assert!(
        matches!(reports.as_slice(), [Report::Failed { .. }]),
        "{reports:?}"
    );
    assert_eq!(app.shown(), "main");
    let fault = app.engine.last_reload_error().expect("it says why");
    assert_eq!(fault.file, "override.qml");

    app.save(
        "user/override.qml",
        &OVERRIDE.replace("\"mine\"", "\"mended\""),
    );
    app.wait();
    assert_eq!(app.shown(), "override");
    assert!(app.try_find("mended").is_some());
    assert_eq!(app.engine.last_reload_error(), None);
}

#[test]
fn an_entry_broken_from_the_start_is_passed_over() {
    let mut files = module().to_vec();
    files.push(("user/override.qml", "import QtQuick\nItem {"));
    let app = app(&files);
    assert_eq!(app.shown(), "main");
    let fault = app.engine.last_reload_error().expect("it says why");
    assert_eq!(fault.file, "override.qml");
}

#[test]
fn a_file_an_earlier_root_has_hides_the_later_one() {
    let mut files = module().to_vec();
    files.push((
        "user/Main.qml",
        "import QtQuick\nItem { objectName: \"users\" }\n",
    ));
    let mut app = app(&files);
    assert_eq!(app.shown(), "users");

    // A save to the hidden file is no change.
    app.save("app/Main.qml", &MAIN.replace("\"main\"", "\"still main\""));
    app.remove("user/Main.qml");
    let mut reports = app.wait();
    while app.shown() == "users" {
        assert!(!reports.is_empty(), "nothing came");
        reports = app.wait();
    }
    assert_eq!(app.shown(), "still main");
}
