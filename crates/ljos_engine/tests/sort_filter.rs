//! `SortFilterProxyModel` where a scenario cannot look: the change
//! notifications a view is sent. Behaviour against Qt is in
//! `features/models/sort-filter-proxy-model.feature`.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{Engine, ItemModel, ModelChange, Value};
use support::{Harness, engine};

const PLAIN: &str = r#"import QtQuick
import QtQml.Models
Item {
  ListModel {
    id: lm
    objectName: "lm"
    ListElement { name: "c"; cost: 3; grp: "x" }
    ListElement { name: "a"; cost: 1; grp: "y" }
    ListElement { name: "b"; cost: 2; grp: "x" }
  }
  RoleSorter { id: rs; objectName: "rs"; roleName: "cost" }
  ValueFilter { id: vf; objectName: "vf"; roleName: "grp"; value: "x"; enabled: false }
  SortFilterProxyModel { id: pm; objectName: "pm"; model: lm; sorters: [rs]; filters: [vf] }
}
"#;

fn proxy(h: &Harness, root: ljos_engine::ObjectId) -> Rc<dyn ItemModel> {
    let pm = h.find(root, "pm");
    h.engine
        .object_model(pm)
        .expect("the proxy object is a model")
}

fn names(h: &Harness, m: &Rc<dyn ItemModel>) -> String {
    (0..m.row_count())
        .map(|r| m.data(&h.engine, r, "name").to_js_string().to_string())
        .collect()
}

fn record(m: &Rc<dyn ItemModel>) -> Rc<RefCell<Vec<String>>> {
    let seen: Rc<RefCell<Vec<String>>> = Rc::default();
    let out = seen.clone();
    m.subscribe(Rc::new(move |_: &Engine, c: &ModelChange| {
        out.borrow_mut().push(match c {
            ModelChange::RowsInserted { first, count } => format!("+{first}x{count}"),
            ModelChange::RowsRemoved { first, count } => format!("-{first}x{count}"),
            ModelChange::RowsMoved { from, to, count } => format!("m{from}>{to}x{count}"),
            ModelChange::DataChanged { first, count, .. } => format!("d{first}x{count}"),
            ModelChange::RolesChanged => "roles".into(),
            ModelChange::ModelReset => "reset".into(),
            _ => return,
        });
    }));
    seen
}

#[test]
fn sorted_rows_and_notifications() {
    let h = engine();
    let root = h.load(PLAIN);
    let m = proxy(&h, root);
    assert_eq!(names(&h, &m), "abc", "sorted by cost: a1 b2 c3");
    let seen = record(&m);
    let lm = h.find(root, "lm");
    // A data change that moves the row: a move and a data change, no
    // reset (a view keeps its delegate).
    h.engine
        .invoke_by_name(
            lm,
            "setProperty",
            &[Value::Int(0), Value::from("cost"), Value::Int(0)],
        )
        .unwrap();
    assert_eq!(names(&h, &m), "cab");
    assert_eq!(seen.borrow().join(" "), "m2>0x1 d0x1");
    seen.borrow_mut().clear();
    // Enabling the filter removes the row that does not match.
    let vf = h.find(root, "vf");
    h.set(vf, "enabled", Value::Bool(true));
    assert_eq!(names(&h, &m), "cb");
    assert_eq!(seen.borrow().join(" "), "-1x1");
    // A source move resets, as Qt makes the delegates again for one.
    seen.borrow_mut().clear();
    h.engine
        .invoke_by_name(lm, "move", &[Value::Int(0), Value::Int(2), Value::Int(1)])
        .unwrap();
    assert_eq!(seen.borrow().join(" "), "reset");
}

#[test]
fn defaults_and_row_count() {
    let h = engine();
    let root = h.load(PLAIN);
    let pm = h.find(root, "pm");
    assert_eq!(h.get(pm, "dynamicSortFilter"), Value::Bool(true));
    assert_eq!(
        h.engine.invoke_by_name(pm, "rowCount", &[]).unwrap(),
        Value::Int(3)
    );
    let rs = h.find(root, "rs");
    assert_eq!(h.get(rs, "priority").to_number(), f64::from(i32::MAX));
    assert_eq!(h.get(rs, "sortOrder").to_number(), 0.0);
}

#[test]
fn instantiator_over_proxy() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
import QtQml.Models
Item {
  ListModel {
    id: lm
    ListElement { name: "c"; cost: 3 }
    ListElement { name: "a"; cost: 1 }
  }
  SortFilterProxyModel { id: pm; objectName: "pm"; model: lm }
  Instantiator {
    id: inst
    objectName: "inst"
    model: pm
    QtObject { property string tag: name }
  }
}
"#,
    );
    let inst = h.find(root, "inst");
    assert_eq!(h.get(inst, "count").to_number(), 2.0, "log: {:?}", h.log());
}
