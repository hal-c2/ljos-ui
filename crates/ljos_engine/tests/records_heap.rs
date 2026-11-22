//! The script objects of records Rust let go of are let go within a few
//! newer records, however many live records the table holds. A value a
//! controller republishes under steady news (a whole state tree in a
//! `PropertyMap`) makes a generation of script objects per publish; with
//! a large live table the sweep that frees the replaced generations must
//! not wait for the table to double, or for a count of newer records when
//! each is large, or they pile up to the heap's cap. A collection lets
//! go of them all, whatever the sweep still waits for.

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

/// One generation of the churned record: a tree of `items` records as
/// script objects, each with a text of about `repeats` times 22 bytes.
fn generation(n: usize, items: usize, repeats: usize) -> Value {
    let items: Vec<Value> = (0..items)
        .map(|i| {
            Value::Map(Arc::new(vec![(
                Atom::intern("t"),
                Value::from(
                    format!("item {i} of generation {n} ")
                        .repeat(repeats)
                        .as_str(),
                ),
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
    held(engine)
}

/// What the heap holds without collecting first: a collection lets go of
/// every dead record, so it would hide how many wait for a sweep.
fn held(engine: &Engine) -> usize {
    engine.script_memory().expect("scripts ran").bytes
}

/// An engine whose script read 600 live records and the first generation
/// of `big`, a tree of a few tens of KiB.
fn shell() -> (Engine, PropertyMap, ObjectId) {
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
    map.insert(&engine, "big", generation(0, 200, 8));
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
    (engine, map, root)
}

#[test]
fn replaced_records_are_let_go_within_a_few_newer_ones() {
    let (engine, map, root) = shell();
    let base = heap(&engine);
    let mut at_fifty = 0;
    for n in 1..=400 {
        map.insert(&engine, "big", generation(n, 200, 8));
        // The binding reads the new record, as a frame would.
        assert_eq!(
            engine.read_by_name(root, "bn").to_number(),
            200.0 + n as f64
        );
        if n == 50 {
            at_fifty = held(&engine);
        }
    }
    let after = held(&engine);
    assert!(
        after < at_fifty + 4 * 1024 * 1024,
        "script heap grew from {at_fifty} (generation 50, base {base}) to {after} \
         over 350 replaced records: replaced generations pile up between sweeps"
    );
}

/// A shell's sidebar tree is one entry and megabytes of script heap: a
/// count of newer records lets dozens of replaced ones fill the heap.
#[test]
fn a_few_large_replaced_records_do_not_fill_the_heap() {
    let (engine, map, root) = shell();
    let base = heap(&engine);
    // Room for a few generations of about 2 MiB, not for the dozens that
    // a step of 64 entries, or of an eighth of them, keeps.
    let limit = base + 12 * 1024 * 1024;
    assert!(engine.set_script_heap_limit(Some(limit)));
    for n in 1..=40 {
        map.insert(&engine, "big", generation(n, 1_000, 80));
        let read = engine.read_by_name(root, "bn").to_number();
        assert_eq!(
            engine.faults_reported(),
            0,
            "the script heap ran out at generation {n}, under a limit of {limit} \
             (base {base}): replaced trees waited for newer records"
        );
        assert_eq!(read, 1_000.0 + n as f64);
    }
}

/// A large tree replaced by a small one makes too few bytes and entries
/// to be swept, so a collection, above all the one that is to make room in
/// a full heap, has to let go of it.
fn collecting_lets_go_of_a_replaced_tree(collect: impl FnOnce(&Engine) -> bool) {
    let (engine, map, root) = shell();
    map.insert(&engine, "big", generation(1, 1_000, 80));
    assert_eq!(engine.read_by_name(root, "bn").to_number(), 1_001.0);
    let with_tree = heap(&engine);
    map.insert(&engine, "big", generation(2, 1, 1));
    assert_eq!(engine.read_by_name(root, "bn").to_number(), 3.0);
    assert!(collect(&engine), "it collected");
    let after = held(&engine);
    assert!(
        after + 1024 * 1024 < with_tree,
        "the heap held {with_tree} bytes with the tree and {after} after \
         collecting: the replaced tree was kept"
    );
}

#[test]
fn a_collection_lets_go_of_replaced_records() {
    collecting_lets_go_of_a_replaced_tree(Engine::collect_garbage);
}

#[test]
fn a_collection_for_a_full_heap_lets_go_of_replaced_records() {
    collecting_lets_go_of_a_replaced_tree(|engine| {
        engine.report_fault(None, ljos_runtime::Fault::JsOutOfMemory);
        engine.collect_for_full_heap(engine.faults_reported())
    });
}
