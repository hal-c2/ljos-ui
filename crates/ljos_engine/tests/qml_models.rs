//! Models and what stands on them in the engine: the model interface and
//! its adapters, `ListModel`, delegate contexts, and `Instantiator`.
//! `features/models/` holds the behaviour checked against Qt; these tests
//! cover what a scenario cannot see, such as how many objects were made.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{
    Atom, Engine, ItemModel, ModelChange, ModelListener, ModelListeners, ObjectId, QtObject, Value,
};
use support::{Harness, engine};

fn call(h: &Harness, obj: ObjectId, method: &str, args: &[Value]) -> Value {
    h.engine
        .invoke_by_name(obj, method, args)
        .unwrap_or_else(|e| panic!("{method}: {e}"))
}

fn text(h: &Harness, obj: ObjectId, prop: &str) -> String {
    h.get(obj, prop).to_js_string().to_string()
}

const FRUIT: &str = r#"import QtQuick
import QtQml.Models
Item {
  id: root
  property string log: ""
  property int made: 0
  function names() {
    var s = ""
    for (var i = 0; i < lm.count; i++) s += lm.get(i).name
    return s
  }
  function costs() {
    var s = ""
    for (var i = 0; i < inst.count; i++) s += inst.objectAt(i).cost + ","
    return s
  }
  function rows() {
    var s = ""
    for (var i = 0; i < inst.count; i++) s += inst.objectAt(i).row
    return s
  }
  function add(name, cost) { lm.append({ name: name, cost: cost }) }
  ListModel {
    id: lm
    objectName: "lm"
    ListElement { name: "a"; cost: 1 }
    ListElement { name: "b"; cost: 2 }
    ListElement { name: "c"; cost: 3 }
  }
  Instantiator {
    id: inst
    objectName: "inst"
    model: lm
    QtObject {
      property int row: index
      property string label: name
      property real cost: model.cost
      Component.onCompleted: root.made++
    }
    onObjectAdded: (i, o) => root.log += "+" + i + o.label
    onObjectRemoved: (i, o) => root.log += "-" + i + o.label
  }
}
"#;

#[test]
fn list_elements_are_the_rows_and_not_objects() {
    let h = engine();
    let root = h.load(FRUIT);
    let lm = h.find(root, "lm");
    assert_eq!(h.num(lm, "count"), 3.0);
    // The elements were declarations: nothing of them is left as a child.
    h.engine.process_deferred_deletes();
    assert!(h.engine.children(lm).is_empty());
    assert_eq!(call(&h, root, "names", &[]), Value::from("abc"));
    assert!(h.warnings().is_empty());
}

#[test]
fn list_model_methods_edit_the_rows() {
    let h = engine();
    let root = h.load(FRUIT);
    let lm = h.find(root, "lm");
    call(&h, root, "add", &[Value::from("d"), Value::Int(4)]);
    assert_eq!(call(&h, root, "names", &[]), Value::from("abcd"));
    call(
        &h,
        lm,
        "move",
        &[Value::Int(0), Value::Int(2), Value::Int(2)],
    );
    assert_eq!(call(&h, root, "names", &[]), Value::from("cdab"));
    call(&h, lm, "remove", &[Value::Int(1), Value::Int(2)]);
    assert_eq!(call(&h, root, "names", &[]), Value::from("cb"));
    call(
        &h,
        lm,
        "setProperty",
        &[Value::Int(0), Value::from("name"), Value::from("z")],
    );
    assert_eq!(call(&h, root, "names", &[]), Value::from("zb"));
    call(&h, lm, "clear", &[]);
    assert_eq!(h.num(lm, "count"), 0.0);
    assert_eq!(call(&h, lm, "get", &[Value::Int(0)]), Value::Undefined);
    assert!(h.warnings().is_empty());
}

#[test]
fn list_model_refuses_rows_out_of_range() {
    let h = engine();
    let root = h.load(FRUIT);
    let lm = h.find(root, "lm");
    call(&h, lm, "remove", &[Value::Int(7), Value::Int(3)]);
    call(
        &h,
        lm,
        "move",
        &[Value::Int(2), Value::Int(0), Value::Int(5)],
    );
    let w = h.warnings();
    assert_eq!(w.len(), 2, "{w:?}");
    assert!(
        w[0].ends_with("ListModel: remove: indices [7 - 10] out of range [0 - 3]"),
        "{w:?}"
    );
    assert!(w[1].ends_with("ListModel: move: out of range"), "{w:?}");
    assert_eq!(call(&h, root, "names", &[]), Value::from("abc"));
}

#[test]
fn a_row_object_from_get_is_live_both_ways() {
    let h = engine();
    let root = h.load(FRUIT);
    let lm = h.find(root, "lm");
    let Value::Object(row) = call(&h, lm, "get", &[Value::Int(1)]) else {
        panic!("get(1) is an object");
    };
    // The same object every time, so a held one stays the row.
    assert_eq!(call(&h, lm, "get", &[Value::Int(1)]), Value::Object(row));
    call(
        &h,
        lm,
        "setProperty",
        &[Value::Int(1), Value::from("cost"), Value::Int(20)],
    );
    assert_eq!(h.num(row, "cost"), 20.0);
    h.set(row, "cost", Value::Int(30));
    assert_eq!(call(&h, root, "costs", &[]), Value::from("1,30,3,"));
    // It follows its row when rows before it go.
    call(&h, lm, "remove", &[Value::Int(0), Value::Undefined]);
    assert_eq!(call(&h, lm, "get", &[Value::Int(0)]), Value::Object(row));
    assert!(h.warnings().is_empty());
}

#[test]
fn a_list_in_a_row_is_a_list_model() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
import QtQml.Models
Item {
  function inner() { return lm.get(0).parts.get(1).part + lm.get(0).parts.count }
  function grow() { lm.get(0).parts.append({ part: "z" }); return lm.get(0).parts.count }
  ListModel {
    id: lm
    ListElement {
      name: "a"
      parts: [ ListElement { part: "x" }, ListElement { part: "y" } ]
    }
  }
}
"#,
    );
    assert_eq!(call(&h, root, "inner", &[]), Value::from("y2"));
    assert_eq!(call(&h, root, "grow", &[]).to_number(), 3.0);
    assert!(h.warnings().is_empty());
}

#[test]
fn a_model_change_makes_and_destroys_only_its_rows() {
    let h = engine();
    let root = h.load(FRUIT);
    let lm = h.find(root, "lm");
    let inst = h.find(root, "inst");
    let at = |i: i32| call(&h, inst, "objectAt", &[Value::Int(i)]);
    assert_eq!(h.num(root, "made"), 3.0);
    assert_eq!(text(&h, root, "log"), "+0a+1b+2c");
    let (a, b, c) = (at(0), at(1), at(2));

    call(&h, root, "add", &[Value::from("d"), Value::Int(4)]);
    assert_eq!(h.num(root, "made"), 4.0);
    assert_eq!((at(0), at(1), at(2)), (a.clone(), b.clone(), c.clone()));

    // A move makes nothing: the same objects, renumbered.
    call(
        &h,
        lm,
        "move",
        &[Value::Int(0), Value::Int(3), Value::Int(1)],
    );
    assert_eq!(h.num(root, "made"), 4.0);
    assert_eq!((at(0), at(1), at(3)), (b.clone(), c.clone(), a.clone()));
    assert_eq!(call(&h, root, "rows", &[]), Value::from("0123"));

    // Data reaches the row's delegate through its bindings.
    call(
        &h,
        lm,
        "setProperty",
        &[Value::Int(0), Value::from("cost"), Value::Int(9)],
    );
    assert_eq!(h.num(root, "made"), 4.0);
    assert_eq!(call(&h, root, "costs", &[]), Value::from("9,3,4,1,"));

    // The object of a removed row reads index -1 when it is announced.
    h.set(root, "log", Value::from(""));
    let Value::Object(gone) = at(1) else {
        panic!("row 1 has an object");
    };
    call(&h, lm, "remove", &[Value::Int(1), Value::Undefined]);
    assert_eq!(h.num(root, "made"), 4.0);
    assert_eq!(text(&h, root, "log"), "-1c");
    assert_eq!(h.num(gone, "row"), -1.0);
    assert_eq!((at(0), at(2)), (b, a));
    assert_eq!(call(&h, root, "rows", &[]), Value::from("012"));
    assert_eq!(h.num(inst, "count"), 3.0);
    assert!(h.warnings().is_empty());
}

#[test]
fn removed_delegates_are_destroyed() {
    let h = engine();
    let root = h.load(FRUIT);
    let lm = h.find(root, "lm");
    h.engine.process_deferred_deletes();
    let before = h.engine.object_count();
    call(&h, lm, "append", &[Value::Int(0)]);
    h.engine.take_log();
    call(&h, root, "add", &[Value::from("d"), Value::Int(4)]);
    let with_one = h.engine.object_count();
    assert!(with_one > before);
    call(&h, lm, "remove", &[Value::Int(3), Value::Undefined]);
    h.engine.process_deferred_deletes();
    assert_eq!(h.engine.object_count(), before);
}

#[test]
fn an_instantiator_follows_model_delegate_and_active() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
import QtQml
Item {
  id: root
  property string log: ""
  Component { id: other; QtObject { property string kind: "other" + index } }
  Instantiator {
    id: inst
    objectName: "inst"
    model: ["x", "y"]
    QtObject { property string kind: modelData + index }
    onObjectAdded: (i, o) => root.log += "+" + o.kind
    onObjectRemoved: (i, o) => root.log += "-" + o.kind
  }
  function swap() { inst.delegate = other }
}
"#,
    );
    let inst = h.find(root, "inst");
    assert_eq!(text(&h, root, "log"), "+x0+y1");
    assert_eq!(h.num(inst, "count"), 2.0);
    let Value::Object(first) = h.get(inst, "object") else {
        panic!("object is the first one");
    };
    assert_eq!(text(&h, first, "kind"), "x0");

    h.set(root, "log", Value::from(""));
    h.set(inst, "active", Value::Bool(false));
    assert_eq!(text(&h, root, "log"), "-x0-y1");
    assert_eq!(h.num(inst, "count"), 0.0);
    assert_eq!(h.get(inst, "object"), Value::Null);

    h.set(root, "log", Value::from(""));
    h.set(inst, "active", Value::Bool(true));
    h.set(inst, "model", Value::Int(1));
    // A row of a number model is its index.
    assert_eq!(text(&h, root, "log"), "+x0+y1-x0-y1+0");

    // A new delegate takes the old objects as a cleared model would:
    // they read index -1.
    h.set(root, "log", Value::from(""));
    call(&h, root, "swap", &[]);
    assert_eq!(text(&h, root, "log"), "--1+other0");
    assert!(h.warnings().is_empty());
}

#[test]
fn an_asynchronous_instantiator_makes_its_objects_in_jobs() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
import QtQml
Item {
  Instantiator {
    objectName: "inst"
    asynchronous: true
    model: 3
    QtObject { property int row: index }
  }
}
"#,
    );
    let inst = h.find(root, "inst");
    assert_eq!(
        call(&h, inst, "objectAt", &[Value::Int(0)]),
        Value::Null,
        "nothing is made until the host runs the jobs"
    );
    let mut rounds = 0;
    while h.engine.has_incubations() {
        for mut job in h.engine.take_incubations() {
            while !matches!(job.step(&|| false), ljos_runtime::JobPoll::Done) {}
        }
        rounds += 1;
        assert!(rounds < 10, "incubation does not settle");
    }
    assert_eq!(h.num(inst, "count"), 3.0);
    let Value::Object(last) = call(&h, inst, "objectAt", &[Value::Int(2)]) else {
        panic!("row 2 has an object");
    };
    assert_eq!(h.num(last, "row"), 2.0);
    assert!(h.warnings().is_empty());
}

#[test]
fn required_properties_take_the_roles_and_hide_the_names() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
import QtQml.Models
Item {
  ListModel { id: lm; objectName: "lm"; ListElement { name: "a"; cost: 1 } }
  function push() { lm.insert(0, { name: "n", cost: 0 }) }
  Instantiator {
    objectName: "inst"
    model: lm
    QtObject {
      required property string name
      required property int index
      property string free: typeof cost
    }
  }
}
"#,
    );
    let inst = h.find(root, "inst");
    let lm = h.find(root, "lm");
    let Value::Object(o) = call(&h, inst, "objectAt", &[Value::Int(0)]) else {
        panic!("row 0 has an object");
    };
    assert_eq!(text(&h, o, "name"), "a");
    assert_eq!(h.num(o, "index"), 0.0);
    assert_eq!(text(&h, o, "free"), "undefined");
    call(
        &h,
        lm,
        "setProperty",
        &[Value::Int(0), Value::from("name"), Value::from("z")],
    );
    assert_eq!(text(&h, o, "name"), "z");
    call(&h, root, "push", &[]);
    assert_eq!(h.num(o, "index"), 1.0);
    assert!(h.warnings().is_empty());
}

#[test]
fn plain_values_become_models() {
    let h = engine();
    let e = &h.engine;
    let m = e.model_from_value(&Value::Int(3), None);
    assert_eq!(m.row_count(), 3);
    assert!(m.role_names().is_empty());
    assert_eq!(m.model_data(e, 2), Some(Value::Int(2)));

    let m = e.model_from_value(&Value::list(vec![Value::from("x"), Value::Int(4)]), None);
    assert_eq!(m.row_count(), 2);
    assert_eq!(m.model_data(e, 0), Some(Value::from("x")));
    assert_eq!(m.model_data(e, 1), Some(Value::Int(4)));

    assert_eq!(e.model_from_value(&Value::Null, None).row_count(), 0);
    assert_eq!(e.model_from_value(&Value::Undefined, None).row_count(), 0);
    assert_eq!(e.model_from_value(&Value::Real(2.0), None).row_count(), 2);
    assert!(h.warnings().is_empty());

    assert_eq!(e.model_from_value(&Value::Int(-2), None).row_count(), 0);
    let w = h.warnings();
    assert!(w[0].contains("Model size of -2 is less than 0"), "{w:?}");

    // A list of objects: the objects are the rows, their properties the
    // roles.
    let a = e.create_native(QtObject::default(), None).unwrap();
    let b = e.create_native(QtObject::default(), None).unwrap();
    e.write_by_name(b, "objectName", Value::from("bee"))
        .unwrap();
    let m = e.model_from_value(&Value::list(vec![Value::Object(a), Value::Object(b)]), None);
    assert_eq!(m.row_count(), 2);
    assert_eq!(m.row_object(1), Some(b));
    assert_eq!(m.data(e, 1, "objectName"), Value::from("bee"));
    assert_eq!(m.model_data(e, 0), Some(Value::Object(a)));
}

/// A model written in Rust: rows of one role, `label`.
#[derive(Default)]
struct Labels {
    rows: RefCell<Vec<(u64, String)>>,
    listeners: ModelListeners,
}

impl ItemModel for Labels {
    fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }

    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::from("label")]
    }

    fn data(&self, _: &Engine, row: usize, role: &str) -> Value {
        match (self.rows.borrow().get(row), role) {
            (Some((_, s)), "label") => Value::from(s.as_str()),
            _ => Value::Undefined,
        }
    }

    fn set_data(&self, e: &Engine, row: usize, role: &str, value: &Value) -> bool {
        if role != "label" || row >= self.row_count() {
            return false;
        }
        self.rows.borrow_mut()[row].1 = value.to_js_string().to_string();
        self.listeners.emit(
            e,
            &ModelChange::DataChanged {
                first: row,
                count: 1,
                roles: vec![Atom::from("label")],
            },
        );
        true
    }

    fn row_key(&self, row: usize) -> u64 {
        self.rows.borrow()[row].0
    }

    fn subscribe(&self, listener: ModelListener) -> u64 {
        self.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.listeners.remove(id);
    }
}

#[test]
fn a_model_written_in_rust_drives_delegates() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
import QtQml
Item {
  id: root
  property string log: ""
  function labels() {
    var s = ""
    for (var i = 0; i < inst.count; i++) s += inst.objectAt(i).text + inst.objectAt(i).row
    return s
  }
  function rename() { inst.objectAt(0).rename("q") }
  Instantiator {
    id: inst
    objectName: "inst"
    model: null
    QtObject {
      property string text: label
      property int row: index
      function rename(to) { model.label = to }
    }
    onObjectAdded: (i, o) => root.log += "+" + i
    onObjectRemoved: (i, o) => root.log += "-" + i
  }
}
"#,
    );
    let e = &h.engine;
    let inst = h.find(root, "inst");
    let labels = Rc::new(Labels::default());
    labels
        .rows
        .borrow_mut()
        .extend([(10, "a".to_string()), (11, "b".to_string())]);
    let holder = e.create_native(QtObject::default(), Some(root)).unwrap();
    e.set_object_model(holder, Some(labels.clone())).unwrap();
    h.set(inst, "model", Value::Object(holder));
    assert_eq!(call(&h, root, "labels", &[]), Value::from("a0b1"));
    assert_eq!(text(&h, root, "log"), "+0+1");

    labels.rows.borrow_mut().insert(1, (12, "c".to_string()));
    labels
        .listeners
        .emit(e, &ModelChange::RowsInserted { first: 1, count: 1 });
    assert_eq!(call(&h, root, "labels", &[]), Value::from("a0c1b2"));
    assert_eq!(text(&h, root, "log"), "+0+1+1");
    // Keys stay with their rows, whatever the position.
    assert_eq!(labels.row_key(2), 11);

    // `model.label = v` in the delegate is the model's to accept.
    call(&h, root, "rename", &[]);
    assert_eq!(labels.rows.borrow()[0].1, "q");
    assert_eq!(call(&h, root, "labels", &[]), Value::from("q0c1b2"));

    let moved = labels.rows.borrow_mut().remove(0);
    labels.rows.borrow_mut().push(moved);
    labels.listeners.emit(
        e,
        &ModelChange::RowsMoved {
            from: 0,
            to: 2,
            count: 1,
        },
    );
    assert_eq!(call(&h, root, "labels", &[]), Value::from("c0b1q2"));
    assert_eq!(text(&h, root, "log"), "+0+1+1");

    labels.rows.borrow_mut().clear();
    labels.listeners.emit(e, &ModelChange::ModelReset);
    assert_eq!(h.num(inst, "count"), 0.0);

    // A view that is gone no longer listens.
    e.destroy_now(inst);
    assert!(labels.listeners.is_empty());
    assert!(h.warnings().is_empty());
}
