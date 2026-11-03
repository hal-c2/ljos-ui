//! The example's own file, read as a user's would be. This build has no
//! web engine, so the home page fails and the window says so.

use ljos::prelude::{Engine, ObjectId, Platform};
use ljos_compiler::{Project, compile_file};

const MAIN: &str = include_str!("../Main.qml");

fn load() -> (Engine, ObjectId) {
    let e = ljos::engine();
    ljos_webengine::register(&e);
    let mut project = Project::new(ljos::dev::EngineOracle::from_engine(&e));
    project.add("Main", MAIN);
    let compiled = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!compiled.has_errors(), "{:?}", compiled.diagnostics);
    let mut source = ljos::MapSource::new();
    source.insert("Main", compiled.file);
    e.set_component_source(source);
    let root = e
        .create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"));
    (e, root)
}

fn find(e: &Engine, root: ObjectId, name: &str) -> ObjectId {
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if e.read_by_name(o, "objectName").to_js_string().as_str() == name {
            return o;
        }
        stack.extend(e.children(o));
    }
    panic!("no object named {name}")
}

#[test]
fn without_a_web_engine_the_home_page_fails_and_the_window_says_why() {
    let (e, root) = load();
    // The view loads its first page just after it completes.
    e.advance_timers(e.now());
    Platform::of(&e).finish(&e);
    let failure = e.read_by_name(root, "failure").to_js_string();
    assert_eq!(failure.as_str(), "net::ERR_FAILED");
    let error = find(&e, root, "error");
    assert!(e.read_by_name(error, "visible").to_boolean());
    let back = find(&e, root, "back");
    assert!(!e.read_by_name(back, "enabled").to_boolean());
    let status = find(&e, root, "status");
    assert_eq!(
        e.read_by_name(status, "text").to_js_string().as_str(),
        "Failed"
    );
}
