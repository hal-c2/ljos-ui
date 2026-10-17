//! A directory of QML compiled at start-up (`ljos::dev`, `App::qml_dir`)
//! as an application uses it: each file at its own URL, so what it
//! names by a relative address is found beside it, and what the
//! compile found handed back as values.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use ljos::dev::{Diagnostic, Severity};
use ljos::prelude::*;

const MAIN: &str = r#"
import QtQuick
Item {
    property string icon: Qt.resolvedUrl("icons/a.svg")
    property url image: "icons/a.svg"
    property Item panel: loader.item
    Loader { id: loader; source: "parts/Panel.qml" }
}
"#;

const PANEL: &str = r#"
import QtQuick
Item {
    property string icon: Qt.resolvedUrl("../icons/a.svg")
    property string here: Qt.resolvedUrl(".")
}
"#;

fn write(dir: &Path, path: &str, text: &str) {
    let path = dir.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn app_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Main.qml", MAIN);
    write(dir.path(), "parts/Panel.qml", PANEL);
    write(dir.path(), "icons/a.svg", "<svg/>");
    dir
}

fn text(engine: &Engine, obj: ObjectId, name: &str) -> String {
    engine.read_by_name(obj, name).to_js_string().to_string()
}

fn url_of(dir: &Path, path: &str) -> String {
    format!("file://{}/{path}", dir.display())
}

fn assert_beside_the_file(engine: &Engine, root: ObjectId, dir: &Path) {
    assert_eq!(text(engine, root, "icon"), url_of(dir, "icons/a.svg"));
    // As in Qt 6, a url property keeps the relative address it was
    // given; what uses it (an Image, a Loader) resolves it.
    assert_eq!(text(engine, root, "image"), "icons/a.svg");
    let panel = engine
        .read_by_name(root, "panel")
        .as_object()
        .expect("the Loader found parts/Panel.qml beside Main.qml");
    assert_eq!(text(engine, panel, "icon"), url_of(dir, "icons/a.svg"));
    assert_eq!(text(engine, panel, "here"), url_of(dir, "parts/"));
}

#[test]
fn app_qml_dir_resolves_against_the_file() {
    let dir = app_dir();
    let (engine, root) = ljos::App::new("Test")
        .qml_dir(dir.path(), "Main")
        .unwrap()
        .build()
        .unwrap();
    assert_beside_the_file(&engine, root, dir.path());
}

#[test]
fn compile_dir_with_resolves_against_the_file() {
    let dir = app_dir();
    let engine = ljos::engine();
    let source = ljos::dev::compile_dir_with(dir.path(), &engine).unwrap();
    let root = ljos::create_root(&engine, source, "Main").unwrap();
    assert_beside_the_file(&engine, root, dir.path());
}

#[test]
fn each_root_gives_its_files_their_own_url() {
    let top = tempfile::tempdir().unwrap();
    let app = top.path().join("app");
    let shared = top.path().join("shared");
    write(
        &app,
        "Main.qml",
        "import QtQuick\nimport Lib\nItem { property Item brick: Brick {} }\n",
    );
    write(&shared, "Lib/qmldir", "module Lib\nBrick 1.0 Brick.qml\n");
    write(
        &shared,
        "Lib/Brick.qml",
        "import QtQuick\nItem { property string icon: Qt.resolvedUrl(\"brick.svg\") }\n",
    );
    let (engine, root) = ljos::App::new("Test")
        .qml_dirs([app.as_path(), shared.as_path()], "Main")
        .unwrap()
        .build()
        .unwrap();
    let brick = engine.read_by_name(root, "brick").as_object().unwrap();
    assert_eq!(
        text(&engine, brick, "icon"),
        url_of(&shared, "Lib/brick.svg")
    );
}

#[test]
fn a_base_url_stays_for_a_source_with_no_files() {
    let dir = app_dir();
    let engine = ljos::engine();
    let pack = ljos::dev::compile_dir_with(dir.path(), &engine)
        .unwrap()
        .to_pack()
        .unwrap();
    // A pack does not say where its files were: the application does.
    let source = ljos::MapSource::from_pack(&pack)
        .unwrap()
        .with_base_url("qrc:/app");
    let root = ljos::create_root(&engine, source, "Main").unwrap();
    assert_eq!(text(&engine, root, "icon"), "qrc:/app/icons/a.svg");
}

fn checked_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "Warns.qml",
        "import QtQuick\nItem {\n    property var x: nowhere\n}\n",
    );
    write(
        dir.path(),
        "Fails.qml",
        "import QtQuick\nItem {\n    width: 1 +\n}\n",
    );
    write(dir.path(), "Fine.qml", "import QtQuick\nItem {}\n");
    dir
}

#[test]
fn a_compile_hands_back_what_it_found() {
    let dir = checked_dir();
    let engine = ljos::engine();
    let found = ljos::dev::compile_quiet(&[dir.path()], Some(&engine)).unwrap();
    assert!(found.has_errors());

    let fails: Vec<&Diagnostic> = found
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(fails.len(), 1, "{:?}", found.diagnostics);
    let fail = fails[0];
    assert_eq!(fail.key, "Fails");
    assert_eq!(fail.path, dir.path().join("Fails.qml"));
    assert_eq!((fail.line, fail.column), (4, 1));
    assert_eq!(
        fail.to_string(),
        format!("{}:4:1: error: {}", fail.path.display(), fail.message)
    );

    let warns = found
        .diagnostics
        .iter()
        .find(|d| d.key == "Warns")
        .expect("a warning for the name looked up when it runs");
    assert_eq!(warns.severity, Severity::Warning);
    assert_eq!(warns.line, 3);
    assert!(warns.to_string().contains(":3:"), "{warns}");

    let source = found.source;
    assert_eq!(source.keys().collect::<Vec<_>>(), ["Fine", "Warns"]);
    let failures: Vec<(&str, &str)> = source.failures().collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, "Fails");
    assert!(failures[0].1.starts_with("4:1: "), "{}", failures[0].1);
    let debug = format!("{source:?}");
    assert!(debug.contains("failures: [\"Fails\"]"), "{debug}");

    let err = ljos::create_root(&engine, source, "Fails")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(&format!("{}:4:1: ", url_of(dir.path(), "Fails.qml"))),
        "{err}"
    );
}

#[test]
fn an_app_hands_its_diagnostics_to_the_application() {
    let dir = checked_dir();
    let seen: Rc<RefCell<Vec<Diagnostic>>> = Rc::default();
    let into = seen.clone();
    let (engine, root) = ljos::App::new("Test")
        .qml_diagnostics(move |ds| into.borrow_mut().extend_from_slice(ds))
        .qml_dir(dir.path(), "Fine")
        .unwrap()
        .build()
        .unwrap();
    assert!(engine.is_alive(root));
    let seen = seen.borrow();
    assert!(
        seen.iter()
            .any(|d| d.key == "Fails" && d.severity == Severity::Error)
    );
    assert!(
        seen.iter()
            .any(|d| d.key == "Warns" && d.severity == Severity::Warning)
    );
}

#[test]
fn an_app_that_fails_on_errors_has_shown_them_first() {
    let dir = checked_dir();
    let seen = Rc::new(RefCell::new(0));
    let into = seen.clone();
    let err = ljos::App::new("Test")
        .qml_errors(ljos::dev::OnError::Fail)
        .qml_diagnostics(move |ds| *into.borrow_mut() += ds.len())
        .qml_dir(dir.path(), "Fine")
        .unwrap_err();
    assert!(err.contains("did not compile"), "{err}");
    assert!(err.contains("Fails.qml:4:1: error: "), "{err}");
    assert!(*seen.borrow() >= 2);
}
