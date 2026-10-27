//! A ListView whose model is replaced in the same change that resizes it
//! shows only the new model's rows.

mod common;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_runtime::Time;

const MAIN: &str = r##"import QtQuick
Item {
  id: root
  width: 200; height: 300
  // Which list is shown; the first has a bar under the view.
  property bool first: true
  ListModel { id: a }
  ListModel { id: b }
  Component.onCompleted: {
    for (var i = 0; i < 40; i++) a.append({ name: "a" + i })
    for (var j = 0; j < 40; j++) b.append({ name: "b" + j })
  }
  // Each delegate whose row is not the model's row at its index.
  function stale() {
    var out = []
    for (var i = 0; i < view.count; i++) {
      var it = view.itemAtIndex(i)
      if (it && it.name !== view.model.get(i).name) out.push(i + ":" + it.name)
    }
    return out.join(" ")
  }
  ListView {
    id: view
    anchors.fill: parent
    anchors.bottomMargin: root.first ? 40 : 0
    model: root.first ? a : b
    delegate: Item {
      required property string name
      width: 200; height: 20
    }
  }
}
"##;

fn stale(e: &Engine, root: ObjectId) -> String {
    let v = e.invoke_by_name(root, "stale", &[]).unwrap();
    v.to_js_string().to_string()
}

#[test]
fn a_view_resized_as_its_model_is_replaced_shows_only_the_new_rows() {
    let e = engine();
    let root = load(&e, MAIN);
    let mut s = scene(&e, root, 200.0, 300.0);
    for i in 0..10 {
        s.frame(&e, Time::from_millis(i * 16));
    }
    set(&e, root, "first", Value::Bool(false));
    let at_once = stale(&e, root);
    for i in 10..40 {
        s.frame(&e, Time::from_millis(i * 16));
    }
    assert_eq!((at_once, stale(&e, root)), (String::new(), String::new()));
}

#[test]
fn a_view_whose_model_is_replaced_alone_shows_only_the_new_rows() {
    let e = engine();
    let root = load(&e, MAIN.replace("root.first ? 40 : 0", "40").as_str());
    let mut s = scene(&e, root, 200.0, 300.0);
    for i in 0..10 {
        s.frame(&e, Time::from_millis(i * 16));
    }
    set(&e, root, "first", Value::Bool(false));
    let at_once = stale(&e, root);
    for i in 10..40 {
        s.frame(&e, Time::from_millis(i * 16));
    }
    assert_eq!((at_once, stale(&e, root)), (String::new(), String::new()));
}
