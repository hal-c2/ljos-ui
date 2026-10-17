//! A record Rust hands QML is one script object until it is set again:
//! two reads are `===`, and reading one field costs a field read, not a
//! copy of the record. A list on its own is a sequence, copied as in Qt.

use std::time::{Duration, Instant};

use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Holder")]
pub struct Holder {
    #[qml(notify)]
    pub state: serde_json::Value,
    #[qml(notify)]
    pub names: Vec<String>,
}

#[qml_impl]
impl Holder {}

const MAIN: &str = r#"
import QtQuick
import Example
Item {
    property alias holder: h
    Holder { id: h }
    property var kept
    function same() { return h.state === h.state }
    function sameNames() { return h.names === h.names }
    function sameRows() { return h.state.rows === h.state.rows }
    function sameEntry() { return shared.big === shared.big }
    function keep() { kept = h.state }
    function stillKept() { return kept === h.state }
    function read(n) { var t = 0; for (var i = 0; i < n; ++i) t += h.state.rows[i % 2000].size; return t }
    function readEntry(n) { var t = 0; for (var i = 0; i < n; ++i) t += shared.big.rows[i % 2000].size; return t }
    function readOwn(n) { var t = 0; for (var i = 0; i < n; ++i) t += own.rows[i % 2000].size; return t }
    property var own
    function makeOwn() { own = JSON.parse(JSON.stringify(h.state)) }
}
"#;

fn record() -> serde_json::Value {
    let rows: Vec<_> = (0..2000)
        .map(|i| serde_json::json!({ "id": i, "size": 1 }))
        .collect();
    serde_json::json!({ "rows": rows })
}

/// The fastest of a few runs, so one slow run on a busy machine does not
/// decide it.
fn fastest(mut f: impl FnMut()) -> Duration {
    (0..3)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .min()
        .unwrap()
}

#[test]
fn a_record_is_one_object_and_reads_at_script_speed() {
    let engine = ljos::engine();
    engine.register_module(ljos::module!("Example" 1.0 => [Holder]));
    let shared = PropertyMap::new(&engine, None).unwrap();
    engine.set_context_property(
        engine.root_context(),
        "shared",
        Value::Object(shared.object()),
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let source = ljos::dev::compile_dir_with(dir.path(), &engine).unwrap();
    let root = ljos::create_root(&engine, source, "Main").unwrap();
    let holder = engine.read_by_name(root, "holder").as_object().unwrap();

    engine.set(holder, Holder::STATE, record()).unwrap();
    engine
        .set(holder, Holder::NAMES, vec!["a".to_owned(), "b".to_owned()])
        .unwrap();
    shared.insert(&engine, "big", record().to_value());
    let call = |f: &str, args: &[Value]| engine.invoke_by_name(root, f, args).unwrap();

    assert_eq!(call("same", &[]), Value::Bool(true));
    assert_eq!(call("sameRows", &[]), Value::Bool(true));
    // A list on its own is a sequence, copied on each read as in Qt.
    assert_eq!(call("sameNames", &[]), Value::Bool(false));
    assert_eq!(call("sameEntry", &[]), Value::Bool(true));
    call("keep", &[]);
    assert_eq!(call("stillKept", &[]), Value::Bool(true));

    // Set again, it is another object.
    engine.set(holder, Holder::STATE, record()).unwrap();
    assert_eq!(call("stillKept", &[]), Value::Bool(false));
    assert_eq!(call("same", &[]), Value::Bool(true));

    // Rust still reads what it stored.
    let back: serde_json::Value = engine.get(holder, Holder::STATE).unwrap();
    assert_eq!(back, record());
    let names: Vec<String> = engine.get(holder, Holder::NAMES).unwrap();
    assert_eq!(names, ["a", "b"]);

    call("makeOwn", &[]);
    let n = [Value::Int(200)];
    assert_eq!(call("read", &n), Value::Real(200.0));
    let own = fastest(|| {
        call("readOwn", &n);
    });
    for f in ["read", "readEntry"] {
        let took = fastest(|| {
            call(f, &n);
        });
        // A copy of the record per read is a thousand times slower; a
        // field read is about as fast as the script's own object.
        assert!(
            took < own * 20 + Duration::from_millis(5),
            "{f}: {took:?} against {own:?} for the script's own object"
        );
    }
}
