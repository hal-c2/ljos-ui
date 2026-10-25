//! A record (`Value::Map`) read through tier 1 and through tier 2: one
//! holds the record and the other the script object made of it, and the
//! cross-check takes them for the same value by their fields, read or
//! written to a `var`.

use std::sync::Arc;

use ljos_compiler::{Project, compile_file};
use ljos_engine::{Atom, Engine, MapSource, PropertyMap, QtObject, Value};
use qml_front::QtTypeOracle;

const MAIN: &str = r#"import QtQml
QtObject {
  property QtObject holder: owner
  property var a: owner.route
  property var b: owner.route ?? null
  property var c: holder.route
  property var d: holder.route ?? null
  property var e: null
  signal moved(var point)
  onMoved: point => e = point
}
"#;

#[test]
fn a_record_through_a_nullish_coalescing_agrees_with_tier_two() {
    let engine = Engine::new();
    engine.register_module(ljos_engine::module!("QtQml" 2.0 => [QtObject]));
    engine.set_tier1(true);
    engine.set_tier1_check(true);
    let map = PropertyMap::new(&engine, None).expect("map");
    let record = Value::Map(Arc::new(vec![(Atom::intern("kind"), Value::from("home"))]));
    map.insert(&engine, "route", record);
    engine.set_context_id(engine.root_context(), "owner", map.object());

    let mut project = Project::new(QtTypeOracle);
    project.add("Main", MAIN);
    let compiled = compile_file(&project, "Main").expect("compiles");
    let mut source = MapSource::new();
    source.insert("Main", compiled.file);
    engine.set_component_source(source);
    let root = engine
        .create_from_url("Main", None, Vec::new())
        .expect("creates");

    engine
        .emit_by_name(
            root,
            "moved",
            &[Value::map([(Atom::intern("x"), Value::Real(3.0))])],
        )
        .expect("emits");
    for p in ["a", "b", "c", "d", "e"] {
        let v = engine.plain(&engine.read_by_name(root, p));
        assert!(!matches!(v, Value::Undefined | Value::Null), "{p} is {v:?}");
    }
    let warnings = engine.take_warnings();
    assert!(warnings.is_empty(), "{warnings:?}");
    let stats = engine.tier1_stats();
    assert!(stats.checked >= 1, "{stats:?}");
    assert_eq!(stats.mismatches, 0, "{stats:?}");
}
