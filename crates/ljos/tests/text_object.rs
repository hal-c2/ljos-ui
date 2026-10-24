//! `Qt.createQmlObject` in an application on the facade with `dev-qml`:
//! QML it holds as text, a user's file it reads at run time, made with a
//! parent and a place, and the types beside that place found as a file
//! there finds them.

use std::fs;
use std::path::Path;

use ljos::prelude::*;
use ljos::testing::TestWindow;

const MAIN: &str = r#"
import QtQuick
Item {
    id: root
    width: 200; height: 100
    property string made: ""
    function make(text, place) {
        try {
            made = Qt.createQmlObject(text, root, place).objectName
        } catch (e) {
            made = "" + e
        }
    }
}
"#;

/// `app/Main.qml`, `app/parts/Panel.qml`, `app/lib/Brick.qml`, and a
/// directory of the user's own, `plugins/`, with `Card.qml` and the
/// `Badge.qml` beside it that it names.
fn tree(dir: &Path) {
    let files = [
        ("app/Main.qml", MAIN),
        (
            "app/parts/Panel.qml",
            "import QtQuick\nItem { property int n: 7 }\n",
        ),
        (
            "app/lib/Brick.qml",
            "import QtQuick\nItem { property int n: 9 }\n",
        ),
        (
            "plugins/Badge.qml",
            "import QtQuick\nRectangle { width: 12 }\n",
        ),
        (
            "plugins/Card.qml",
            "import QtQuick\nItem { objectName: \"card\"; Badge { objectName: \"badge\" } }\n",
        ),
    ];
    for (name, text) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

fn make(engine: &Engine, root: ObjectId, text: &str, place: &str) -> String {
    engine
        .invoke_by_name(root, "make", &[Value::from(text), Value::from(place)])
        .unwrap();
    engine.read_by_name(root, "made").to_js_string().to_string()
}

fn number_below(engine: &Engine, root: ObjectId, name: &str, property: &str) -> f64 {
    fn find(e: &Engine, obj: ObjectId, name: &str) -> Option<ObjectId> {
        if e.read_by_name(obj, "objectName").to_js_string().as_ref() == name {
            return Some(obj);
        }
        e.children(obj).into_iter().find_map(|c| find(e, c, name))
    }
    let obj = find(engine, root, name).unwrap_or_else(|| panic!("no {name}"));
    engine.read_by_name(obj, property).to_number()
}

const PLAIN: &str = "import QtQuick\nItem { objectName: \"made\" }";

#[test]
fn the_issue_repro_makes_an_object_from_text() {
    let tmp = tempfile::tempdir().unwrap();
    tree(tmp.path());
    let app = tmp.path().join("app");
    let engine = ljos::engine();
    let source = ljos::dev::compile_dirs(&[&app], Some(&engine), ljos::dev::OnError::Fail).unwrap();
    engine.set_component_source(source);
    ljos::dev::install_text_compiler(&engine, &[&app]);
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();

    assert_eq!(make(&engine, root, PLAIN, "inline.qml"), "made");
}

#[test]
fn a_text_finds_the_types_beside_its_place() {
    let tmp = tempfile::tempdir().unwrap();
    tree(tmp.path());
    let (engine, root) = ljos::App::new("texts")
        .qml_dir(tmp.path().join("app"), "Main")
        .unwrap()
        .build()
        .unwrap();

    let beside = "import QtQuick\nPanel { objectName: \"panel\" }";
    assert_eq!(make(&engine, root, beside, "parts/inline.qml"), "panel");
    assert_eq!(number_below(&engine, root, "panel", "n"), 7.0);

    let imported = "import QtQuick\nimport \"../lib\"\nBrick { objectName: \"brick\" }";
    assert_eq!(make(&engine, root, imported, "parts/other.qml"), "brick");
    assert_eq!(number_below(&engine, root, "brick", "n"), 9.0);

    // Beside the root file there is no Panel.
    let made = make(&engine, root, beside, "inline.qml");
    assert!(made.contains("`Panel` is not a type"), "{made}");
}

#[test]
fn a_users_file_read_at_run_time_is_made_with_what_is_beside_it() {
    let tmp = tempfile::tempdir().unwrap();
    tree(tmp.path());
    let plugins = tmp.path().join("plugins");
    let (engine, root) = ljos::App::new("plugins")
        .qml_dirs([tmp.path().join("app"), plugins.clone()], "Main")
        .unwrap()
        .build()
        .unwrap();

    let card = plugins.join("Card.qml");
    let text = fs::read_to_string(&card).unwrap();
    let url = format!("file://{}", card.canonicalize().unwrap().display());
    assert_eq!(make(&engine, root, &text, &url), "card");
    assert_eq!(number_below(&engine, root, "badge", "width"), 12.0);
}

#[test]
fn a_test_window_makes_objects_from_text() {
    let tmp = tempfile::tempdir().unwrap();
    tree(tmp.path());
    let app = ljos::App::new("window")
        .qml_dir(tmp.path().join("app"), "Main")
        .unwrap();
    let engine = app.build_engine();
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();
    let mut window = TestWindow::new(&engine, 200.0, 100.0);
    window.show(root).unwrap();
    window.settle().unwrap();

    assert_eq!(make(&engine, root, PLAIN, "inline.qml"), "made");
    window.settle().unwrap();
}

#[test]
fn without_a_compiler_the_script_is_told_why() {
    let tmp = tempfile::tempdir().unwrap();
    tree(tmp.path());
    let app = tmp.path().join("app");
    let engine = ljos::engine();
    let source = ljos::dev::compile_dirs(&[&app], Some(&engine), ljos::dev::OnError::Fail).unwrap();
    engine.set_component_source(source);
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();

    let made = make(&engine, root, PLAIN, "inline.qml");
    assert!(
        made.contains("has no compiler to make an object from text"),
        "{made}"
    );
}

#[test]
fn a_text_placed_outside_the_directories_finds_the_files_beside_it() {
    let tmp = tempfile::tempdir().unwrap();
    tree(tmp.path());
    let (engine, root) = ljos::App::new("outside")
        .qml_dir(tmp.path().join("app"), "Main")
        .unwrap()
        .build()
        .unwrap();

    // `plugins/` is none of the application's directories.
    let card = tmp.path().join("plugins/Card.qml").canonicalize().unwrap();
    let text = fs::read_to_string(&card).unwrap();
    let url = format!("file://{}", card.display());
    assert_eq!(make(&engine, root, &text, &url), "card");
    assert_eq!(number_below(&engine, root, "badge", "width"), 12.0);
}

#[test]
fn a_file_added_after_the_first_text_is_a_type_the_next_may_name() {
    let tmp = tempfile::tempdir().unwrap();
    tree(tmp.path());
    let app = tmp.path().join("app");
    let (engine, root) = ljos::App::new("added")
        .qml_dir(&app, "Main")
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(make(&engine, root, PLAIN, "inline.qml"), "made");

    fs::write(
        app.join("parts/Fresh.qml"),
        "import QtQuick\nItem { property int n: 5 }\n",
    )
    .unwrap();
    let fresh = "import QtQuick\nFresh { objectName: \"fresh\" }";
    assert_eq!(make(&engine, root, fresh, "parts/inline.qml"), "fresh");
    assert_eq!(number_below(&engine, root, "fresh", "n"), 5.0);

    // And in a directory that was not there at the first text.
    fs::create_dir(app.join("more")).unwrap();
    fs::write(
        app.join("more/Later.qml"),
        "import QtQuick\nItem { property int n: 6 }\n",
    )
    .unwrap();
    let later = "import QtQuick\nLater { objectName: \"later\" }";
    assert_eq!(make(&engine, root, later, "more/inline.qml"), "later");
    assert_eq!(number_below(&engine, root, "later", "n"), 6.0);
}
