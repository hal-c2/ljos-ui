//! An application's own native types, imported by the QML the
//! application compiles with `App::qml_dir` (the `dev-qml` feature).
//!
//! The compiler's table of types does not know an application's
//! natives (ARCHITECTURE.md D1), so the table-only
//! `ljos::dev::compile_dir` still cannot compile this directory —
//! pinned below. `compile_dir_with` and `App::qml_dir` snapshot the
//! natives an Engine has registered (for `App`, the modules
//! `register`ed before the `qml_dir` call) and compile against
//! them, so `import Example` works with no oracle named by the user.

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

const MAIN: &str = r#"
import QtQuick
import Example
Item {
    Counter { id: counter; count: 41 }
    property int seen: counter.count
    function bump() { counter.increment() }
}
"#;

fn qml_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    dir
}

fn number(engine: &Engine, obj: ObjectId, name: &str) -> f64 {
    engine.read_by_name(obj, name).to_number()
}

#[test]
fn register_then_qml_dir_imports_the_native_type() {
    let dir = qml_dir();
    let (engine, root) = ljos::App::new("Test")
        .register(register)
        .qml_dir(dir.path(), "Main")
        .unwrap()
        .build()
        .unwrap();
    // The native object exists with the property the QML gave it,
    // and its method runs through the root.
    assert_eq!(number(&engine, root, "seen"), 41.0);
    engine.invoke_by_name(root, "bump", &[]).unwrap();
    assert_eq!(number(&engine, root, "seen"), 42.0);
}

#[test]
fn compile_dir_with_compiles_against_the_engines_natives() {
    let dir = qml_dir();
    let engine = ljos::engine();
    register(&engine);
    let source = ljos::dev::compile_dir_with(dir.path(), &engine).unwrap();
    let root = ljos::create_root(&engine, source, "Main").unwrap();
    assert_eq!(number(&engine, root, "seen"), 41.0);
}

#[test]
fn compile_dir_stays_table_only_and_does_not_know_the_native_type() {
    let dir = qml_dir();
    let err = ljos::dev::compile_dir(dir.path()).err().unwrap();
    assert!(err.contains("did not compile"), "{err}");
    assert!(err.contains("`Counter` is not a type"), "{err}");
}

#[test]
fn qml_dir_before_register_fails_as_a_compile_error() {
    let dir = qml_dir();
    // The module is registered only after `qml_dir` would run, so
    // the snapshot has no `Example` in it: an ordinary compile
    // error, not a panic.
    let err = match ljos::App::new("Test").qml_dir(dir.path(), "Main") {
        Ok(_) => panic!("qml_dir before register compiled"),
        Err(err) => err,
    };
    assert!(err.contains("did not compile"), "{err}");
    assert!(err.contains("`Counter` is not a type"), "{err}");
}
