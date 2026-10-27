//! A native list model may have no value for a role in some rows. Its
//! delegates are made all the same, and a typed required property with
//! nothing to take keeps its default, as in Qt.

use std::rc::Rc;

use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Rows")]
pub struct Rows {}

struct Mixed;

impl QmlListModel for Mixed {
    fn row_count(&self) -> usize {
        3
    }

    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::from("kind"), Atom::from("label"), Atom::from("size")]
    }

    fn data(&self, row: usize, role: &str) -> Value {
        // Row 0 has no label, the others no size.
        match (row, role) {
            (_, "kind") => Value::from(if row == 0 { "head" } else { "item" }),
            (0, "size") => Value::Int(3),
            (r, "label") if r > 0 => Value::from(format!("r{r}").as_str()),
            _ => Value::Undefined,
        }
    }
}

#[qml_impl]
impl Rows {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        cx.engine.set_list_model(cx.this, Rc::new(Mixed))?;
        Ok(())
    }
}

fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Rows]));
}

const MAIN: &str = r#"
import QtQuick
import Example
Item {
    property int made: 0
    property string seen: ""
    Rows { id: rows }
    Repeater {
        model: rows
        delegate: Item {
            required property string kind
            required property string label
            required property int size
            Component.onCompleted: { made++; seen += kind + ":" + label + ":" + size + " " }
        }
    }
}
"#;

#[test]
fn a_row_without_a_role_still_has_its_delegate() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let (engine, root) = ljos::App::new("Test")
        .register(register)
        .qml_dir(dir.path(), "Main")
        .unwrap()
        .build()
        .unwrap();
    let made = engine.read_by_name(root, "made").to_number();
    let seen = engine.read_by_name(root, "seen").to_js_string().to_string();
    assert_eq!((made, seen.as_str()), (3.0, "head::3 item:r1:0 item:r2:0 "));
}
