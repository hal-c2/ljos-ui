//! `Qt.createComponent` and `Loader` of a `.qml` file written while the
//! program runs (a plugin's page fetched into a cache, say): with
//! `dev-qml` it is compiled when it is asked for, with the files beside
//! it; a program without a compiler says why it cannot show it.

use std::fs;
use std::path::Path;

use ljos::prelude::*;
use ljos::testing::TestWindow;

const MAIN: &str = r#"
import QtQuick
Item {
    id: root
    property string made: ""
    property string problem: ""
    property alias loaded: loader.item
    property alias loaderStatus: loader.status
    Loader { id: loader }
    function make(url) {
        const c = Qt.createComponent(url)
        if (c.status === Component.Ready) {
            made = c.createObject(root).objectName
        } else {
            problem = c.errorString()
        }
    }
    function load(url) { loader.source = url }
}
"#;

fn app(dir: &Path) -> std::path::PathBuf {
    let app = dir.join("app");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("Main.qml"), MAIN).unwrap();
    app
}

/// `cache/Page.qml` and the `Note.qml` beside it that it names, written
/// after the program started.
fn fetched(dir: &Path) -> String {
    let cache = dir.join("cache/plugins/p/1/ui");
    fs::create_dir_all(&cache).unwrap();
    fs::write(
        cache.join("Note.qml"),
        "import QtQuick\nItem { objectName: \"note\"; property int n: 3 }\n",
    )
    .unwrap();
    fs::write(
        cache.join("Page.qml"),
        "import QtQuick\nItem { objectName: \"page\"; Note { } }\n",
    )
    .unwrap();
    ljos::ljos_engine::file_url(&cache.join("Page.qml").canonicalize().unwrap())
}

fn text(engine: &Engine, root: ObjectId, name: &str) -> String {
    engine.read_by_name(root, name).to_js_string().to_string()
}

fn named(engine: &Engine, obj: ObjectId, name: &str) -> Option<ObjectId> {
    if engine
        .read_by_name(obj, "objectName")
        .to_js_string()
        .as_ref()
        == name
    {
        return Some(obj);
    }
    engine
        .children(obj)
        .into_iter()
        .find_map(|c| named(engine, c, name))
}

#[test]
fn the_issue_repro_a_file_written_after_start_up_is_made() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path());
    let engine = ljos::engine();
    let source = ljos::dev::compile_dirs(&[&app], Some(&engine), ljos::dev::OnError::Fail).unwrap();
    engine.set_component_source(source);
    ljos::dev::install_text_compiler(&engine, &[&app]);
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();

    let late = tmp.path().join("Late.qml");
    fs::write(&late, "import QtQuick\nItem { objectName: \"late\" }\n").unwrap();
    let url = ljos::ljos_engine::file_url(&late.canonicalize().unwrap());
    engine
        .invoke_by_name(root, "make", &[Value::from(url.as_str())])
        .unwrap();
    assert_eq!(text(&engine, root, "problem"), "");
    assert_eq!(text(&engine, root, "made"), "late");
}

#[test]
fn an_application_shows_a_fetched_page_with_the_files_beside_it() {
    let tmp = tempfile::tempdir().unwrap();
    let (engine, root) = ljos::App::new("fetched")
        .qml_dir(app(tmp.path()), "Main")
        .unwrap()
        .build()
        .unwrap();
    let page = fetched(tmp.path());

    engine
        .invoke_by_name(root, "make", &[Value::from(page.as_str())])
        .unwrap();
    assert_eq!(text(&engine, root, "problem"), "");
    assert_eq!(text(&engine, root, "made"), "page");
    let note = named(&engine, root, "note").expect("the page made its note");
    assert_eq!(engine.read_by_name(note, "n").to_number(), 3.0);
}

#[test]
fn an_engine_set_up_by_hand_shows_a_fetched_page_with_the_files_beside_it() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path());
    let engine = ljos::engine();
    let source = ljos::dev::compile_dirs(&[&app], Some(&engine), ljos::dev::OnError::Fail).unwrap();
    engine.set_component_source(source);
    ljos::dev::install_text_compiler(&engine, &[&app]);
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();
    let page = fetched(tmp.path());

    engine
        .invoke_by_name(root, "make", &[Value::from(page.as_str())])
        .unwrap();
    assert_eq!(text(&engine, root, "problem"), "");
    assert_eq!(text(&engine, root, "made"), "page");
    let note = named(&engine, root, "note").expect("the page made its note");
    assert_eq!(engine.read_by_name(note, "n").to_number(), 3.0);
}

#[test]
fn a_loader_shows_a_page_written_after_start_up() {
    let tmp = tempfile::tempdir().unwrap();
    let built = ljos::App::new("loaded")
        .qml_dir(app(tmp.path()), "Main")
        .unwrap();
    let engine = built.build_engine();
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();
    let mut window = TestWindow::new(&engine, 200.0, 100.0);
    window.show(root).unwrap();
    window.settle().unwrap();
    let page = fetched(tmp.path());

    engine
        .invoke_by_name(root, "load", &[Value::from(page.as_str())])
        .unwrap();
    window.settle().unwrap();
    assert_eq!(engine.read_by_name(root, "loaderStatus").to_number(), 1.0);
    let item = engine.read_by_name(root, "loaded").as_object().unwrap();
    assert_eq!(text(&engine, item, "objectName"), "page");
}

#[test]
fn a_file_added_to_the_application_s_directory_is_made() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path());
    let (engine, root) = ljos::App::new("added")
        .qml_dir(&app, "Main")
        .unwrap()
        .build()
        .unwrap();
    fs::create_dir(app.join("extra")).unwrap();
    fs::write(
        app.join("extra/Added.qml"),
        "import QtQuick\nItem { objectName: \"added\" }\n",
    )
    .unwrap();

    engine
        .invoke_by_name(root, "make", &[Value::from("extra/Added.qml")])
        .unwrap();
    assert_eq!(text(&engine, root, "problem"), "");
    assert_eq!(text(&engine, root, "made"), "added");
}

#[test]
fn without_a_compiler_a_file_written_after_start_up_says_why_it_is_not_made() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path());
    let engine = ljos::engine();
    let source = ljos::dev::compile_dirs(&[&app], Some(&engine), ljos::dev::OnError::Fail).unwrap();
    engine.set_component_source(source);
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();
    let page = fetched(tmp.path());

    engine
        .invoke_by_name(root, "make", &[Value::from(page.as_str())])
        .unwrap();
    assert_eq!(
        text(&engine, root, "problem"),
        format!(
            "{page}: this program runs compiled QML and has no compiler for a file it was not \
             built with\n"
        )
    );
    assert_eq!(text(&engine, root, "made"), "");
}
