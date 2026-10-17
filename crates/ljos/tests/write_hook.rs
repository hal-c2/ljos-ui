//! A field's `#[qml(write = ..)]` is its setter: it clamps or refuses a
//! write before the value is stored or anyone hears of it, for QML
//! assignments, bindings and writes from Rust alike.

use std::cell::RefCell;
use std::rc::Rc;

use ljos::prelude::{Engine, ObjectId, QmlObject, Value, Version, qml_singleton};

/// The issue's repro: with no setter, the type corrects the value in a
/// handler of its own change signal, after it was stored and told.
mod corrected_after {
    use super::*;

    #[derive(Debug, Default, QmlObject)]
    #[qml(module = "Repro", name = "Model", uncreatable)]
    #[qml_singleton]
    pub struct Model {
        #[qml(notify)]
        pub highlighted: i32,
    }
}

/// The same type with a setter.
mod clamped_before {
    use super::*;

    #[derive(Debug, Default, QmlObject)]
    #[qml(module = "Repro", name = "Model", uncreatable)]
    #[qml_singleton]
    pub struct Model {
        #[qml(notify, write = Self::clamp_highlighted)]
        pub highlighted: i32,
    }

    impl Model {
        fn clamp_highlighted(&self, v: i32) -> Option<i32> {
            Some(v.max(0))
        }
    }
}

fn listen(engine: &Engine, obj: ObjectId) -> Rc<RefCell<Vec<Value>>> {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let log = heard.clone();
    engine
        .connect_by_name(obj, "highlightedChanged", move |engine, _| {
            log.borrow_mut()
                .push(engine.read_by_name(obj, "highlighted"));
        })
        .expect("highlightedChanged");
    heard
}

#[test]
fn a_value_corrected_in_its_change_handler_is_heard() {
    use corrected_after::Model;
    let engine = Engine::new();
    engine.register_singleton::<Model>("Repro", Version::new(1, 0));
    let obj = engine.singleton::<Model>().expect("the singleton");
    engine
        .connect_by_name(obj, "highlightedChanged", move |engine, _| {
            if engine.read_by_name(obj, "highlighted") != Value::Int(0) {
                let _ = engine.write_by_name(obj, "highlighted", Value::Int(0));
            }
        })
        .expect("highlightedChanged");
    let heard = listen(&engine, obj);
    engine
        .write_by_name(obj, "highlighted", Value::Int(-7))
        .expect("highlighted is writable");
    // Qt's setter would have kept 0 and told nobody; this is why a type
    // wants one.
    assert!(!heard.borrow().is_empty());
    assert_eq!(engine.read_by_name(obj, "highlighted"), Value::Int(0));
}

#[test]
fn a_clamped_write_that_changes_nothing_is_not_heard() {
    use clamped_before::Model;
    let engine = Engine::new();
    engine.register_singleton::<Model>("Repro", Version::new(1, 0));
    let obj = engine.singleton::<Model>().expect("the singleton");
    let heard = listen(&engine, obj);
    engine
        .write_by_name(obj, "highlighted", Value::Int(-7))
        .expect("highlighted is writable");
    assert_eq!(*heard.borrow(), Vec::<Value>::new());
    assert_eq!(engine.read_by_name(obj, "highlighted"), Value::Int(0));

    // A write the setter keeps is stored and heard once.
    engine
        .write_by_name(obj, "highlighted", Value::Int(3))
        .unwrap();
    assert_eq!(*heard.borrow(), vec![Value::Int(3)]);
    // One it corrects to something new is heard as the corrected value.
    engine
        .set(obj, Model::HIGHLIGHTED, -1)
        .expect("a typed write");
    assert_eq!(*heard.borrow(), vec![Value::Int(3), Value::Int(0)]);
    assert_eq!(engine.get(obj, Model::HIGHLIGHTED), Ok(0));
}

mod gauge {
    use ljos::prelude::QmlObject;

    /// A gauge whose `level` stays in 0..=`top`, and whose `label`
    /// refuses an empty string.
    #[derive(Debug, Default, QmlObject)]
    #[qml(module = "App", name = "Gauge")]
    pub struct Gauge {
        #[qml(notify)]
        pub top: f64,
        #[qml(notify, write = Self::keep_level)]
        pub level: f64,
        #[qml(notify, write = Self::refuse_empty)]
        pub label: String,
    }

    impl Gauge {
        fn keep_level(&self, v: f64) -> Option<f64> {
            Some(v.clamp(0.0, self.top))
        }

        fn refuse_empty(&self, v: String) -> Option<String> {
            (!v.is_empty()).then_some(v)
        }
    }
}

use gauge::Gauge;

fn register(engine: &Engine) {
    engine.register_module(ljos::module!("App" 1.0 => [Gauge]));
}

const MAIN: &str = r#"
import QtQuick
import App
Item {
    property alias gauge: gauge
    property real wanted: 0.5
    property int levels: 0
    property int labels: 0
    function overshoot() { gauge.level = 20; gauge.level = 30 }
    function blank() { gauge.label = "" }
    Gauge {
        id: gauge
        top: 10
        level: wanted
        label: "fuel"
        onLevelChanged: levels++
        onLabelChanged: labels++
    }
}
"#;

fn run() -> (Engine, ObjectId) {
    let dir = tempfile::tempdir().unwrap();
    let qml = dir.path().join("qml");
    std::fs::create_dir(&qml).unwrap();
    std::fs::write(qml.join("Main.qml"), MAIN).unwrap();
    let types = dir.path().join("ljostypes.json");
    std::fs::write(&types, ljos::types::dump(register).to_json()).unwrap();
    let pack = dir.path().join("qml.pack");
    ljos::build::compile_dir_with_types(&qml, &types, &pack).unwrap();
    let pack = std::fs::read(pack).unwrap();
    ljos::App::new("writeHook")
        .register(register)
        .source(ljos::MapSource::from_pack(&pack).unwrap())
        .root("Main")
        .build()
        .unwrap()
}

fn gauge(engine: &Engine, root: ObjectId) -> ObjectId {
    match engine.read_by_name(root, "gauge") {
        Value::Object(o) => o,
        v => panic!("no gauge: {v:?}"),
    }
}

#[test]
fn a_qml_assignment_goes_through_the_setter() {
    let (engine, root) = run();
    let g = gauge(&engine, root);
    let before = engine.read_by_name(root, "levels");
    engine.invoke_by_name(root, "overshoot", &[]).unwrap();
    assert_eq!(engine.read_by_name(g, "level").to_number(), 10.0);
    // 20 is stored as 10 and heard; 30 is stored as 10 again and is not.
    assert_eq!(
        engine.read_by_name(root, "levels").to_number(),
        before.to_number() + 1.0
    );
}

#[test]
fn a_binding_goes_through_the_setter() {
    let (engine, root) = run();
    let g = gauge(&engine, root);
    assert_eq!(engine.read_by_name(g, "level").to_number(), 0.5);
    let before = engine.read_by_name(root, "levels").to_number();
    engine
        .write_by_name(root, "wanted", Value::Real(40.0))
        .unwrap();
    assert_eq!(engine.read_by_name(g, "level").to_number(), 10.0);
    engine
        .write_by_name(root, "wanted", Value::Real(90.0))
        .unwrap();
    assert_eq!(engine.read_by_name(g, "level").to_number(), 10.0);
    assert_eq!(
        engine.read_by_name(root, "levels").to_number(),
        before + 1.0
    );
    // The binding is still in place.
    engine
        .write_by_name(root, "wanted", Value::Real(2.0))
        .unwrap();
    assert_eq!(engine.read_by_name(g, "level").to_number(), 2.0);
}

#[test]
fn a_refused_write_keeps_the_value_and_is_not_heard() {
    let (engine, root) = run();
    let g = gauge(&engine, root);
    let before = engine.read_by_name(root, "labels");
    engine.invoke_by_name(root, "blank", &[]).unwrap();
    assert_eq!(engine.read_by_name(g, "label"), Value::from("fuel"));
    engine
        .write_by_name(g, "label", Value::from(""))
        .expect("a refused write is no error");
    assert_eq!(engine.read_by_name(g, "label"), Value::from("fuel"));
    assert_eq!(engine.read_by_name(root, "labels"), before);
}
