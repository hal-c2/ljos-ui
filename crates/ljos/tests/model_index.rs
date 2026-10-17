//! Model indexes as Rust makes and keeps them: two of the same place are
//! one value, however each was made, so a native property written the
//! index it holds has not changed.

use ljos::prelude::*;

/// A native type with a property holding a model index.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Holder")]
pub struct Holder {
    #[qml(notify)]
    pub index: Value,
}

const MAIN: &str = r#"
import QtQuick
import Example
Item {
    property ListModel lm: ListModel { ListElement { a: 1 } ListElement { a: 2 } }
    property var fromScript: lm.index(1, 0)
    property Holder holder: Holder { property int changes: 0; onIndexChanged: changes++ }
}
"#;

fn make() -> (Engine, ObjectId, ObjectId, ObjectId) {
    let engine = ljos::engine();
    engine.register_module(ljos::module!("Example" 1.0 => [Holder]));
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let source = ljos::dev::compile_dir_with(dir.path(), &engine).unwrap();
    let root = ljos::create_root(&engine, source, "Main").unwrap();
    let lm = engine.read_by_name(root, "lm").as_object().unwrap();
    let holder = engine.read_by_name(root, "holder").as_object().unwrap();
    (engine, root, lm, holder)
}

#[test]
fn two_indexes_of_one_place_are_equal() {
    let (engine, root, lm, _) = make();
    let a = engine.model_index(Some(lm), &[(0, 0)]);
    assert_eq!(a, engine.model_index(Some(lm), &[(0, 0)]));
    assert_ne!(a, engine.model_index(Some(lm), &[(1, 0)]));
    assert_ne!(a, engine.model_index(None, &[(0, 0)]));
    assert_ne!(a, engine.model_index(Some(root), &[(0, 0)]));
    // Under a parent the whole path counts.
    let deep = engine.model_index(Some(lm), &[(0, 0), (1, 0)]);
    assert_eq!(deep, engine.model_index(Some(lm), &[(0, 0), (1, 0)]));
    assert_ne!(deep, engine.model_index(Some(lm), &[(1, 0), (1, 0)]));
}

#[test]
fn an_index_a_script_made_equals_one_rust_makes() {
    let (engine, root, lm, _) = make();
    let from_script = engine.read_by_name(root, "fromScript");
    assert_eq!(from_script, engine.model_index(Some(lm), &[(1, 0)]));
}

#[test]
fn a_native_property_written_the_index_it_holds_does_not_notify() {
    let (engine, _, lm, holder) = make();
    for _ in 0..2 {
        let index = engine.model_index(Some(lm), &[(0, 0)]);
        Cx::new(&engine, holder).set(Holder::INDEX, index).unwrap();
    }
    assert_eq!(engine.read_by_name(holder, "changes").to_number(), 1.0);
    let other = engine.model_index(Some(lm), &[(1, 0)]);
    Cx::new(&engine, holder).set(Holder::INDEX, other).unwrap();
    assert_eq!(engine.read_by_name(holder, "changes").to_number(), 2.0);
}
