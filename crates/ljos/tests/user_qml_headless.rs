//! `App::user_qml` lays a user's files over the application where a
//! shell runs it. A host that opens the application with `build` or
//! `build_engine` has no loop to apply them in: it is told, never left
//! to find the setting dropped.
#![cfg(all(feature = "user-qml", feature = "shell"))]

use ljos::prelude::*;

const MAIN: &str = "import QtQuick\nItem { objectName: \"root\" }\n";

fn app(dir: &std::path::Path) -> ljos::App {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", MAIN);
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!c.has_errors(), "Main: {:?}", c.diagnostics);
    let mut pack = ljos::MapSource::new();
    pack.insert("Main", c.file);
    ljos::App::new("Shell")
        .source(pack)
        .user_qml(ljos::UserQml::dir(dir))
        .root("Main")
}

fn told(engine: &Engine) -> Vec<String> {
    engine
        .take_log()
        .into_iter()
        .filter(|e| e.level == LogLevel::Warning && e.message.contains("App::user_qml"))
        .map(|e| e.message)
        .collect()
}

#[test]
fn a_built_engine_says_it_applies_no_user_files() {
    let dir = tempfile::tempdir().expect("a directory");
    let engine = app(dir.path()).log_to_stderr(false).build_engine();
    let told = told(&engine);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("build_engine"), "{told:?}");
}

#[test]
fn a_built_application_says_it_applies_no_user_files() {
    let dir = tempfile::tempdir().expect("a directory");
    let (engine, root) = app(dir.path())
        .log_to_stderr(false)
        .build()
        .expect("the root is made");
    assert_eq!(engine.read_by_name(root, "objectName"), Value::from("root"));
    assert_eq!(told(&engine).len(), 1);
}
