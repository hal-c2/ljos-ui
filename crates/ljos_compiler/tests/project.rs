//! A project read from disk: its files, and the modules `qmldir` describes.

mod oracle;

use ljos_compiler::{Severity, compile_project_with};
use ljos_ir::{CompiledFile, ObjectValue, TypeRef};
use oracle::Runtime;

#[test]
fn a_project_on_disk_compiles_file_by_file() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    std::fs::create_dir_all(root.join("ui")).expect("ui");
    std::fs::create_dir_all(root.join("target")).expect("target");
    std::fs::write(
        root.join("Main.qml"),
        "import QtQuick\nimport App.Ui\nItem { Card { title: \"hi\" } }\n",
    )
    .expect("Main");
    std::fs::write(
        root.join("ui/Card.qml"),
        "import QtQuick\nRectangle { property string title }\n",
    )
    .expect("Card");
    std::fs::write(root.join("ui/qmldir"), "module App.Ui\nCard 1.0 Card.qml\n").expect("qmldir");
    // What a build leaves behind is not part of the project.
    std::fs::write(root.join("target/Stale.qml"), "Item {}\n").expect("stale");

    let results = compile_project_with(root, Runtime::new()).expect("readable");
    let keys: Vec<&str> = results.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, ["Main", "ui/Card"]);

    let main = results[0].result.as_ref().expect("Main compiles");
    assert!(
        main.diagnostics
            .iter()
            .all(|d| d.severity != Severity::Error),
        "{:?}",
        main.diagnostics
    );
    let card = &main.file.components[0].objects[1];
    assert_eq!(
        card.ty,
        TypeRef::Component {
            file: "ui/Card".into(),
            inline: None
        }
    );
    assert_eq!(
        main.file.components[0].objects[0].children,
        [ObjectValue::Object(ljos_ir::ObjIdx(1))]
    );

    // What is compiled is what a shipped program loads.
    let bytes = main.file.to_bytes().expect("serialises");
    assert_eq!(CompiledFile::from_bytes(&bytes).expect("loads"), main.file);
}

#[test]
fn a_missing_directory_is_an_error() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let missing = dir.path().join("nowhere");
    let error = compile_project_with(&missing, Runtime::new()).expect_err("no such directory");
    assert!(error.to_string().contains("nowhere"), "{error}");
}

#[test]
fn a_file_with_errors_is_kept_and_says_what_was_wrong() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    std::fs::write(
        root.join("Main.qml"),
        "import QtQuick\nItem {\n    width: 1 +\n    height: 2\n    Card {}\n}\n",
    )
    .expect("Main");
    std::fs::write(root.join("Card.qml"), "import QtQuick\nItem {\n").expect("Card");

    let project = ljos_compiler::Project::load(root, Runtime::new()).expect("readable");
    let front = project.front();
    // Both are known to the others, in part.
    assert!(front.has("Main") && front.has("Card"));
    assert!(front.is_partial("Main") && front.is_partial("Card"));
    assert!(!front.diagnostics("Main").is_empty());
    assert!(
        front
            .diagnostics("Main")
            .iter()
            .all(|error| error.code == qml_front::Code::Syntax)
    );
}
