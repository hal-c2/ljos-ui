//! A directory of QML compiled at start-up (`ljos::dev`, `App::qml_dir`)
//! as an application uses it: each file at its own URL, so what it
//! names by a relative address is found beside it.

use std::path::Path;

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
