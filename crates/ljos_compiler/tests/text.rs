//! QML a running program holds as text (`Qt.createQmlObject`), and the
//! `Qt` a module is given when its scripts ask for files by address.

mod oracle;

use ljos_compiler::{Project, Severity, compile_file, compile_text};
use oracle::Runtime;

fn project() -> Project<Runtime> {
    let mut project = Project::new(Runtime::new());
    project.add("Panel", "import QtQuick\nItem { property int n: 1 }\n");
    project.add("sub/Panel", "import QtQuick\nItem { property int n: 2 }\n");
    project.add("Main", "import QtQuick\nItem { Panel { } }\n");
    project
}

fn keys(project: &Project<Runtime>) -> Vec<String> {
    project.files().map(|k| k.to_string()).collect()
}

#[test]
fn a_text_sees_what_a_file_of_its_key_sees_and_leaves_the_project_as_it_was() {
    let mut project = project();
    let before = keys(&project);
    let here = compile_text(&mut project, "inline", "import QtQuick\nPanel { }\n")
        .expect("compiles")
        .file;
    let there = compile_text(&mut project, "sub/inline", "import QtQuick\nPanel { }\n")
        .expect("compiles")
        .file;
    assert_ne!(
        format!("{:?}", here.components),
        format!("{:?}", there.components),
        "each finds the Panel beside it"
    );
    assert_eq!(keys(&project), before);
}

#[test]
fn a_text_under_the_key_of_a_file_leaves_the_file() {
    let mut project = project();
    let main = project.source("Main").map(str::to_owned);
    compile_text(&mut project, "Main", "import QtQuick\nItem { }\n").expect("compiles");
    assert_eq!(project.source("Main").map(str::to_owned), main);
    let file = compile_file(&project, "Main").expect("compiles").file;
    assert_eq!(file.components[0].objects.len(), 2, "Main has its Panel");
}

#[test]
fn a_text_that_does_not_parse_says_where() {
    let mut project = project();
    // A file with errors compiles in part, and so does a text.
    let compiled =
        compile_text(&mut project, "inline", "import QtQuick\nItem { ").expect("in part");
    assert!(compiled.has_errors());
    let error = compiled
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .expect("an error");
    assert_eq!(error.span.line, 2);
    assert_eq!(keys(&project).len(), 3);
}

#[test]
fn a_module_that_asks_for_files_by_address_has_a_qt_of_its_own() {
    let mut project = Project::new(Runtime::new());
    project.add(
        "Asks",
        "import QtQuick\nItem { function make() { return Qt.createComponent(\"Panel.qml\") } }\n",
    );
    project.add(
        "Plain",
        "import QtQuick\nItem { function quit() { Qt.quit() } }\n",
    );
    let asks = compile_file(&project, "Asks").expect("compiles").file;
    let source = &asks.js.source;
    assert!(
        source.contains("export function __ljos_at(url)"),
        "{source}"
    );
    let plain = compile_file(&project, "Plain").expect("compiles").file;
    let source = &plain.js.source;
    assert!(!source.contains("__ljos_at"), "{source}");
}
