//! A native object that makes itself a list model in its own completion
//! is a model to every view that names it, wherever the view is declared
//! (as in Qt, where a view's model is resolved once the document is
//! made). Completion runs last-made-first, so a view declared after the
//! object completes before the object has become a model.

use std::rc::Rc;

use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Rows")]
pub struct Rows {}

struct Three;

impl QmlListModel for Three {
    fn row_count(&self) -> usize {
        3
    }

    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::from("title")]
    }

    fn data(&self, row: usize, _: &str) -> Value {
        Value::from(format!("r{row}").as_str())
    }
}

#[qml_impl]
impl Rows {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        cx.engine.set_list_model(cx.this, Rc::new(Three))?;
        Ok(())
    }
}

fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Rows]));
}

const MAIN: &str = r#"
import QtQuick
import QtQml
import QtQuick.Controls
import Example
Item {
    property alias repeaterBefore: rb.count
    property alias repeaterAfter: ra.count
    property alias listBefore: lb.count
    property alias listAfter: la.count
    property alias instantiatorAfter: ia.count
    property alias comboBoxAfter: cb.count
    property alias comboBoxText: cb.currentText
    property alias tumblerAfter: tu.count
    property string titles: ""
    Repeater { id: rb; model: rows; delegate: Item {} }
    ListView { id: lb; width: 100; height: 100; model: rows; delegate: Item { height: 10 } }
    Rows { id: rows }
    Repeater {
        id: ra
        model: rows
        delegate: Item { Component.onCompleted: titles += title }
    }
    ListView { id: la; width: 100; height: 100; model: rows; delegate: Item { height: 10 } }
    Instantiator { id: ia; model: rows; delegate: QtObject {} }
    ComboBox { id: cb; model: rows; textRole: "title" }
    Tumbler { id: tu; model: rows }
}
"#;

#[test]
fn a_view_declared_after_a_native_model_sees_its_rows() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let (engine, root) = ljos::App::new("Test")
        .register(register)
        .qml_dir(dir.path(), "Main")
        .unwrap()
        .build()
        .unwrap();
    let n = |name| engine.read_by_name(root, name).to_number();
    for name in [
        "repeaterBefore",
        "repeaterAfter",
        "listBefore",
        "listAfter",
        "instantiatorAfter",
        "comboBoxAfter",
        "tumblerAfter",
    ] {
        assert_eq!(n(name), 3.0, "{name}");
    }
    assert_eq!(
        engine
            .read_by_name(root, "titles")
            .to_js_string()
            .to_string(),
        "r0r1r2"
    );
    assert_eq!(
        engine
            .read_by_name(root, "comboBoxText")
            .to_js_string()
            .to_string(),
        "r0"
    );
}
