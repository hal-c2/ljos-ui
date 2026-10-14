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

// The derive's generated `::ljos_engine::…` paths, resolved as an
// application crate depending on the facade alone resolves them.
pub use ljos::ljos_engine;

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
    let err = ljos::dev::compile_dir(dir.path()).err().unwrap();
    assert!(err.contains("did not compile"), "{err}");
}
