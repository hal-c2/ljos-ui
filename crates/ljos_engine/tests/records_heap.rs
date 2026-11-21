//! The script objects of records Rust let go of are let go within a few
//! newer records, however many live records the table holds. A value a
//! controller republishes under steady news (a whole state tree in a
//! `PropertyMap`) makes a generation of script objects per publish; with
//! a large live table the sweep that frees the replaced generations must
//! not wait for the table to double, or they pile up to the heap's cap.

use std::sync::Arc;

use ljos_compiler::{Project, compile_file};
use ljos_engine::{Atom, Engine, MapSource, ObjectId, PropertyMap, QtObject, Value};
use qml_front::QtTypeOracle;

const MAIN: &str = r#"import QtQml
QtObject {
  property var big: owner.big
  property int bn: big ? big.items.length + big.n : 0
  property var all: {
    const o = {};
    for (let i = 0; i < 600; i++)
      o[i] = owner["k" + i];
    return o;
  }
}
"#;

/// One generation of the churned record: a tree of a few tens of KiB as
/// script objects, differing from the last in `n` alone.
fn generation(n: usize) -> Value {
    let items: Vec<Value> = (0..200)
        .map(|i| {
            Value::Map(Arc::new(vec![(
                Atom::intern("t"),
                Value::from(format!("item {i} of generation {n} ").repeat(8).as_str()),
            )]))
        })
        .collect();
    Value::Map(Arc::new(vec![
        (Atom::intern("items"), Value::List(Arc::new(items))),
        (Atom::intern("n"), Value::Real(n as f64)),
    ]))
}

fn heap(engine: &Engine) -> usize {
    engine.collect_garbage();
    engine.script_memory().expect("scripts ran").bytes
}

#[test]
fn replaced_records_are_let_go_within_a_few_newer_ones() {
    let engine = Engine::new();
    engine.register_module(ljos_engine::module!("QtQml" 2.0 => [QtObject]));
    let map = PropertyMap::new(&engine, None).expect("map");
    // A live table of records the script keeps reading: its size must not
    // decide how long a replaced record's script object outlives it.
    for i in 0..600 {
        map.insert(
            &engine,
            &format!("k{i}"),
            Value::Map(Arc::new(vec![(
                Atom::intern("v"),
                Value::from(format!("live {i}").as_str()),
            )])),
        );
    }
    map.insert(&engine, "big", generation(0));
    engine.set_context_id(engine.root_context(), "owner", map.object());

    let mut project = Project::new(QtTypeOracle);
    project.add("Main", MAIN);
    let compiled = compile_file(&project, "Main").expect("compiles");
    let mut source: MapSource = MapSource::new();
    source.insert("Main", compiled.file);
    engine.set_component_source(source);
    let root: ObjectId = engine
        .create_from_url("Main", None, Vec::new())
        .expect("creates");
    // The creation read every live record once.
    assert_eq!(engine.read_by_name(root, "bn").to_number(), 200.0);
    let all = engine.read_by_name(root, "all");
    assert!(
        !matches!(all, Value::Undefined | Value::Null),
        "the live records were read: {all:?}"
    );

    let base = heap(&engine);
    let mut at_fifty = 0;
    for n in 1..=400 {
        map.insert(&engine, "big", generation(n));
        // The binding reads the new record, as a frame would.
        assert_eq!(
            engine.read_by_name(root, "bn").to_number(),
            200.0 + n as f64
        );
        if n == 50 {
            at_fifty = heap(&engine);
        }
    }
    let after = heap(&engine);
    assert!(
        after < at_fifty + 4 * 1024 * 1024,
        "script heap grew from {at_fifty} (generation 50, base {base}) to {after} \
         over 350 replaced records: replaced generations pile up between sweeps"
    );
}
