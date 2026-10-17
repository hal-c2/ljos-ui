//! A native type the application declares with no base still builds on
//! `QtObject`, as every QML object does: `objectName` compiles against
//! its type dump, reads and writes, and finds the object among its
//! parent's children.

use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "App", name = "Model")]
pub struct Model {
    #[qml(notify)]
    pub enabled: bool,
}

fn register(engine: &Engine) {
    engine.register_module(ljos::module!("App" 1.0 => [Model]));
}

const MAIN: &str = r#"
import QtQuick
import App
Item {
    id: root
    property string seen: model.objectName
    function rename() { model.objectName = "renamed" }
    Model { id: model; objectName: "model"; enabled: true }
}
"#;

/// `Main.qml` compiled against the type dump file the application's test
/// writes, and the engine running it.
fn run() -> (Engine, ObjectId) {
    let dir = tempfile::tempdir().unwrap();
    let qml = dir.path().join("qml");
    std::fs::create_dir(&qml).unwrap();
    std::fs::write(qml.join("Main.qml"), MAIN).unwrap();
    let types = dir.path().join("ljostypes.json");
    std::fs::write(&types, ljos::types::dump(register).to_json()).unwrap();
    ljos::types::check_file(&types, register).unwrap();
    let pack = dir.path().join("qml.pack");
    ljos::build::compile_dir_with_types(&qml, &types, &pack).unwrap();
    let pack = std::fs::read(pack).unwrap();
    ljos::App::new("objectName")
        .register(register)
        .source(ljos::MapSource::from_pack(&pack).unwrap())
        .root("Main")
        .build()
        .unwrap()
}

fn named(engine: &Engine, parent: ObjectId, name: &str) -> Option<ObjectId> {
    engine
        .children(parent)
        .into_iter()
        .find(|&c| matches!(engine.read_by_name(c, "objectName"), Value::String(s) if &*s == name))
}

#[test]
fn a_native_type_without_a_base_has_object_name() {
    let (engine, root) = run();
    assert!(matches!(engine.read_by_name(root, "seen"), Value::String(s) if &*s == "model"));
    let model = named(&engine, root, "model").expect("the model, by its objectName");
    // The struct is still the object's native part.
    assert!(engine.read_by_name(model, "enabled").to_boolean());

    engine.invoke_by_name(root, "rename", &[]).unwrap();
    assert!(matches!(engine.read_by_name(root, "seen"), Value::String(s) if &*s == "renamed"));
    assert_eq!(named(&engine, root, "renamed"), Some(model));
}

#[test]
fn the_dump_gives_the_type_qt_objects_members() {
    let dump = ljos::types::dump(register);
    let model = &dump.modules[0].types[0];
    assert_eq!(model.name, "Model");
    assert_eq!(model.base.as_deref(), Some("QtObject"));
}
