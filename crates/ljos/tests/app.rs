//! The facade as an application sees it: a native type written only
//! against `ljos::prelude`, registered through the `App` builder,
//! used from QML the test compiles, and read back. The compile
//! pattern is `crates/ljos_controls/tests/navigation.rs`'s; compiling
//! in a test links the parser in the test only, never in the facade.
//!
//! The QML here reaches the native type through a context property,
//! because this file's `compile` helper compiles against the table
//! of types alone, which an application's natives do not reach
//! (ARCHITECTURE.md D1). Compiling through `App::qml_dir` instead
//! snapshots the registered natives, so the QML may `import` them —
//! that path is `native_import.rs` (and `docs/MIGRATION.md` §6).

use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Counter")]
pub struct Counter {
    #[qml(notify)]
    pub count: i32,
}

#[qml_impl]
impl Counter {
    #[qml(signal)]
    fn bumped();

    #[qml(invokable)]
    fn increment(cx: &Cx) -> Result<(), EngineError> {
        let n = cx.get(Counter::COUNT)?;
        cx.set(Counter::COUNT, n + 1)
    }
}

fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Counter]));
}

/// Compiles `Main` from `qml` the way the controls tests compile.
fn compile(qml: &str) -> ljos::MapSource {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", qml);
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!c.has_errors(), "Main: {:?}", c.diagnostics);
    let mut source = ljos::MapSource::new();
    source.insert("Main", c.file);
    source
}

fn number(engine: &Engine, obj: ObjectId, name: &str) -> f64 {
    engine.read_by_name(obj, name).to_number()
}

const MAIN: &str = r#"
import QtQuick
Item {
    property int seen: counter.count
    function bump() { counter.increment() }
}
"#;

/// A `Counter` at 41, as a context value: what each test below puts
/// in scope under the name the QML reads.
fn counter_at(engine: &Engine, count: i32) -> Value {
    let counter = engine.create_by_name("Example", "Counter", None).unwrap();
    engine.set(counter, Counter::COUNT, count).unwrap();
    Value::Object(counter)
}

#[test]
fn the_builder_registers_loads_and_creates_the_root() {
    let (engine, root) = ljos::App::new("Test")
        .register(register)
        .context_property("counter", |engine| counter_at(engine, 41))
        .source(compile(MAIN))
        .root("Main")
        .build()
        .unwrap();
    assert_eq!(number(&engine, root, "seen"), 41.0);
    engine.invoke_by_name(root, "bump", &[]).unwrap();
    assert_eq!(number(&engine, root, "seen"), 42.0);
}

#[test]
fn what_the_qml_changed_reads_back_through_the_typed_handle() {
    use std::cell::Cell;
    use std::rc::Rc;
    let made = Rc::new(Cell::new(None));
    let put = Rc::clone(&made);
    let (engine, root) = ljos::App::new("Test")
        .register(register)
        .context_property("counter", move |engine| {
            let value = counter_at(engine, 7);
            put.set(value.as_object());
            value
        })
        .source(compile(MAIN))
        .root("Main")
        .build()
        .unwrap();
    let counter = made.get().unwrap();
    assert_eq!(engine.get(counter, Counter::COUNT), Ok(7));
    engine.invoke_by_name(root, "bump", &[]).unwrap();
    assert_eq!(engine.get(counter, Counter::COUNT), Ok(8));
    assert_eq!(number(&engine, root, "seen"), 8.0);
}

#[test]
fn an_app_without_a_root_names_no_root() {
    let err = ljos::App::new("Test")
        .source(compile("import QtQuick\nItem {}\n"))
        .build()
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("no root"), "{err}");
}

#[test]
fn create_root_is_the_setup_lines_without_the_builder() {
    let engine = ljos::engine();
    register(&engine);
    let counter = counter_at(&engine, 41);
    engine.set_context_property(engine.root_context(), "counter", counter);
    let root = ljos::create_root(&engine, compile(MAIN), "Main").unwrap();
    assert_eq!(number(&engine, root, "seen"), 41.0);
}

#[test]
fn qml_dir_compiles_a_directory_for_development() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let (engine, root) = ljos::App::new("Test")
        .register(register)
        .context_property("counter", |engine| counter_at(engine, 41))
        .qml_dir(dir.path(), "Main")
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(number(&engine, root, "seen"), 41.0);

    std::fs::write(
        dir.path().join("Main.qml"),
        "import QtQuick\nItem {\n  width: 20 +\n}\n",
    )
    .unwrap();
    let err = ljos::dev::compile(dir.path(), None, ljos::dev::OnError::Fail)
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("did not compile"), "{err}");
}

#[test]
fn a_file_that_does_not_compile_fails_only_where_it_is_made() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Main.qml"),
        "import QtQuick\nItem { width: 10; height: 10 }\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Unused.qml"),
        "import QtQuick\nItem { NotAType {} }\n",
    )
    .unwrap();
    // `Main` never makes `Unused`: the application starts.
    let (engine, root) = ljos::App::new("Test")
        .qml_dir(dir.path(), "Main")
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(engine.read_by_name(root, "width").to_number(), 10.0);
    // Making the broken file fails with the compiler's message.
    let err = engine
        .create_from_url("Unused", None, Vec::new())
        .unwrap_err()
        .to_string();
    assert!(err.contains("NotAType"), "{err}");

    // A build step that wants every file clean asks for it.
    let err = match ljos::App::new("Test")
        .qml_errors(ljos::dev::OnError::Fail)
        .qml_dir(dir.path(), "Main")
    {
        Ok(_) => panic!("a strict qml_dir compiled a broken file"),
        Err(err) => err.to_string(),
    };
    assert!(err.contains("Unused.qml:2:8: error: "), "{err}");
}

#[test]
fn a_module_closure_captures_its_configuration_and_may_run_twice() {
    use std::cell::Cell;
    use std::rc::Rc;
    // What a module is configured with at start-up, captured rather
    // than passed through a static.
    let start = String::from("41");
    let runs = Rc::new(Cell::new(0));
    let counted = Rc::clone(&runs);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Main.qml"),
        "import QtQuick\nimport Example\nItem {\n  property Counter counter: Counter { count: start }\n  property int seen: counter.count\n}\n",
    )
    .unwrap();
    let (engine, root) = ljos::App::new("Test")
        .register(move |engine: &Engine| {
            counted.set(counted.get() + 1);
            register(engine);
            let count = start.parse::<i32>().unwrap();
            engine.set_context_property(engine.root_context(), "start", Value::from(count));
        })
        .qml_dir(dir.path(), "Main")
        .unwrap()
        .build()
        .unwrap();
    // Once on the Engine the QML was compiled against, once on the
    // Engine the application runs on.
    assert_eq!(runs.get(), 2);
    assert_eq!(number(&engine, root, "seen"), 41.0);
}

#[test]
fn qml_dirs_names_several_roots_each_keyed_from_its_own() {
    let top = tempfile::tempdir().unwrap();
    let app = top.path().join("app");
    let lib = top.path().join("shared/Lib");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        app.join("Main.qml"),
        "import QtQuick\nimport Lib\nItem { width: 10; height: 10; property Item brick: Brick {} }\n",
    )
    .unwrap();
    std::fs::write(lib.join("qmldir"), "module Lib\nBrick 1.0 Brick.qml\n").unwrap();
    std::fs::write(
        lib.join("Brick.qml"),
        "import QtQuick\nRectangle { width: 5; height: 5 }\n",
    )
    .unwrap();
    // The entry is `Main`, from its own root; `Lib` is found in the
    // other root, and nothing above the two trees is read.
    let (engine, root) = ljos::App::new("Test")
        .qml_dirs([app.as_path(), &top.path().join("shared")], "Main")
        .unwrap()
        .build()
        .unwrap();
    let brick = engine.read_by_name(root, "brick").as_object().unwrap();
    assert_eq!(engine.read_by_name(brick, "width").to_number(), 5.0);
}
