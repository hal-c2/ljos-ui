//! The example's own files, read as a user's would be: `Main.qml` over
//! `rows.json`, with the rows it says it has.

use ljos::prelude::{Engine, ObjectId, Platform};
use ljos_compiler::{Project, compile_file};

const MAIN: &str = include_str!("../Main.qml");
const ROWS: &[u8] = include_bytes!("../rows.json");

fn load() -> (Engine, ObjectId) {
    let e = ljos::engine();
    ljos_jsonmodel::register(&e);
    Platform::of(&e).register_resource("file:///rows.json", ROWS.to_vec());
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
fn the_example_shows_the_rows_of_its_file() {
    let (e, root) = load();
    let stock = find(&e, root, "stock");
    Platform::of(&e).finish(&e);
    assert_eq!(e.read_by_name(stock, "count").to_number() as i64, 4);
    assert_eq!(e.read_by_name(stock, "status").to_number() as i64, 1);
    assert_eq!(e.read_by_name(stock, "error").to_js_string().as_str(), "");
    let roles = e.read_by_name(stock, "roles").to_js_string();
    assert_eq!(roles.as_str(), "name,price,stocked");
}
