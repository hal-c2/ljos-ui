//! Assigning to a name of an outer component, or to a role, from inside a
//! component: the object that has the name changes, not a copy.

mod support;

use support::engine;

#[test]
fn a_delegate_assigns_to_a_property_of_the_component_around() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
import QtQml
Item {
  id: root
  property int created: 0
  property int total: 0
  property int doubled: created * 2
  Instantiator {
    model: 3
    delegate: QtObject {
      Component.onCompleted: { created++; total += index }
    }
  }
}
"#,
    );
    assert_eq!(h.num(root, "created"), 3.0);
    assert_eq!(h.num(root, "total"), 3.0);
    assert_eq!(h.num(root, "doubled"), 6.0);
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

#[test]
fn the_nearest_object_with_the_name_is_the_one_assigned() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
import QtQml
Item {
  property int count: 0
  Instantiator {
    objectName: "made"
    model: 1
    delegate: QtObject {
      property int count: 10
      function bump() { count++; return count }
    }
  }
}
"#,
    );
    let made = h.find(root, "made");
    let near = h
        .engine
        .invoke_by_name(made, "objectAt", &[ljos_engine::Value::Int(0)])
        .unwrap();
    let near = near.as_object().expect("a delegate");
    assert_eq!(
        h.engine
            .invoke_by_name(near, "bump", &[])
            .unwrap()
            .to_number(),
        11.0
    );
    assert_eq!(h.num(near, "count"), 11.0);
    assert_eq!(h.num(root, "count"), 0.0);
}

#[test]
fn assigning_to_a_role_changes_the_model() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
import QtQml
import QtQml.Models
Item {
  property int second: lm.get(1).cost
  function reread() { second = lm.get(1).cost }
  ListModel {
    id: lm
    objectName: "lm"
    ListElement { name: "a"; cost: 1 }
    ListElement { name: "b"; cost: 2 }
  }
  Instantiator {
    objectName: "made"
    model: lm
    delegate: QtObject {
      property int price: cost
      function assign() { cost = 50; return cost }
    }
  }
}
"#,
    );
    let made = h.find(root, "made");
    let second = h
        .engine
        .invoke_by_name(made, "objectAt", &[ljos_engine::Value::Int(1)])
        .unwrap()
        .as_object()
        .expect("a delegate");
    assert_eq!(
        h.engine
            .invoke_by_name(second, "assign", &[])
            .unwrap()
            .to_number(),
        50.0
    );
    assert_eq!(h.num(second, "price"), 50.0);
    h.engine.invoke_by_name(root, "reread", &[]).unwrap();
    assert_eq!(h.num(root, "second"), 50.0);
}
