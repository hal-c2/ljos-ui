//! What a virtualised view stands on in the engine: a model written in
//! Rust with the full row-signal set (`QmlListModel`), and delegates made
//! only for the rows a view asks for (`ViewDelegates`). `features/views/`
//! holds the behaviour checked against Qt; these tests cover what a
//! scenario cannot see.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{
    Atom, DelegateRows, DelegateSource, Engine, ItemModel, Made, ModelChange, ModelSignals,
    ObjectId, ObjectRows, QmlListModel, QtObject, Urgency, Value, ViewDelegates, ViewHost,
};
use support::{Harness, engine};

/// Rows of one role, `label`, each with a key that stays with it.
#[derive(Default)]
struct Labels {
    rows: RefCell<Vec<(u64, String)>>,
    signals: RefCell<Option<ModelSignals>>,
    engine: RefCell<Option<Engine>>,
}

impl Labels {
    fn of(n: usize) -> Rc<Labels> {
        let l = Labels::default();
        l.rows
            .borrow_mut()
            .extend((0..n as u64).map(|k| (k, format!("r{k}"))));
        Rc::new(l)
    }

    fn signals(&self) -> ModelSignals {
        self.signals.borrow().clone().expect("the model is set")
    }

    fn insert(&self, e: &Engine, at: usize, key: u64) {
        self.signals().rows_about_to_be_inserted(e, at, 1);
        self.rows.borrow_mut().insert(at, (key, format!("r{key}")));
        self.signals().rows_inserted(e, at, 1);
    }

    fn remove(&self, e: &Engine, first: usize, count: usize) {
        self.signals().rows_about_to_be_removed(e, first, count);
        self.rows.borrow_mut().drain(first..first + count);
        self.signals().rows_removed(e, first, count);
    }
}

impl QmlListModel for Labels {
    fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }

    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::from("label")]
    }

    fn data(&self, row: usize, role: &str) -> Value {
        match (self.rows.borrow().get(row), role) {
            (Some((_, s)), "label") => Value::from(s.as_str()),
            _ => Value::Undefined,
        }
    }

    fn set_data(&self, row: usize, role: &str, value: &Value) -> bool {
        if role != "label" || row >= self.row_count() {
            return false;
        }
        self.rows.borrow_mut()[row].1 = value.to_js_string().to_string();
        if let Some(e) = self.engine.borrow().as_ref() {
            self.signals().data_changed(e, row, 1, &["label"]);
        }
        true
    }

    fn row_key(&self, row: usize) -> u64 {
        self.rows.borrow()[row].0
    }
}

fn call(h: &Harness, obj: ObjectId, method: &str, args: &[Value]) -> Value {
    h.engine
        .invoke_by_name(obj, method, args)
        .unwrap_or_else(|e| panic!("{method}: {e}"))
}

fn model_object(h: &Harness, root: ObjectId, labels: &Rc<Labels>) -> ObjectId {
    let e = &h.engine;
    let holder = e.create_native(QtObject::default(), Some(root)).unwrap();
    let signals = e.set_list_model(holder, labels.clone()).unwrap();
    *labels.signals.borrow_mut() = Some(signals);
    *labels.engine.borrow_mut() = Some(e.clone());
    holder
}

#[test]
fn a_native_model_sends_the_full_row_signal_set() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
import QtQml
Item {
  id: root
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
  }
}
"#,
    );
    let e = &h.engine;
    let inst = h.find(root, "inst");
    let labels = Labels::of(2);
    let holder = model_object(&h, root, &labels);
    // A second listener writes down every signal in order.
    let seen: Rc<RefCell<Vec<String>>> = Rc::default();
    let model = e.object_model(holder).expect("the object is a model");
    let log = seen.clone();
    model.subscribe(Rc::new(move |_: &Engine, c: &ModelChange| {
        log.borrow_mut().push(match c {
            ModelChange::RowsAboutToBeInserted { first, count } => format!("ai{first}+{count}"),
            ModelChange::RowsInserted { first, count } => format!("i{first}+{count}"),
            ModelChange::RowsAboutToBeRemoved { first, count } => format!("ar{first}+{count}"),
            ModelChange::RowsRemoved { first, count } => format!("r{first}+{count}"),
            ModelChange::RowsMoved { from, to, count } => format!("m{from}>{to}+{count}"),
            ModelChange::DataChanged { first, roles, .. } => format!("d{first}:{}", roles.len()),
            ModelChange::RolesChanged => "roles".to_string(),
            ModelChange::ModelReset => "reset".to_string(),
        });
    }));

    h.set(inst, "model", Value::Object(holder));
    assert_eq!(call(&h, root, "labels", &[]), Value::from("r00r11"));

    labels.insert(e, 1, 7);
    assert_eq!(call(&h, root, "labels", &[]), Value::from("r00r71r12"));

    // `model.label = v` in the delegate goes to the model, whose
    // `dataChanged` brings it back.
    call(&h, root, "rename", &[]);
    assert_eq!(call(&h, root, "labels", &[]), Value::from("q0r71r12"));

    let moved = labels.rows.borrow_mut().remove(0);
    labels.rows.borrow_mut().push(moved);
    labels.signals().rows_moved(e, 0, 2, 1);
    assert_eq!(call(&h, root, "labels", &[]), Value::from("r70r11q2"));

    labels.remove(e, 0, 1);
    assert_eq!(call(&h, root, "labels", &[]), Value::from("r10q1"));

    labels.rows.borrow_mut().clear();
    labels.signals().model_reset(e);
    assert_eq!(h.num(inst, "count"), 0.0);

    assert_eq!(
        *seen.borrow(),
        ["ai1+1", "i1+1", "d0:1", "m0>2+1", "ar0+1", "r0+1", "reset"]
    );
    assert!(labels.signals().is_watched());
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

#[test]
fn a_list_model_says_what_is_about_to_change() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
import QtQml.Models
Item {
  function add() { lm.append({ name: "x" }) }
  ListModel { id: lm; objectName: "lm"; ListElement { name: "a" } }
}
"#,
    );
    let e = &h.engine;
    let lm = h.find(root, "lm");
    let model = e.object_model(lm).expect("a ListModel is a model");
    let seen: Rc<RefCell<Vec<(String, usize)>>> = Rc::default();
    let log = seen.clone();
    let rows = model.clone();
    model.subscribe(Rc::new(move |_: &Engine, c: &ModelChange| {
        let name = match c {
            ModelChange::RowsAboutToBeInserted { .. } => "about-insert",
            ModelChange::RowsInserted { .. } => "insert",
            ModelChange::RowsAboutToBeRemoved { .. } => "about-remove",
            ModelChange::RowsRemoved { .. } => "remove",
            _ => return,
        };
        // The count the model has while the listener runs.
        log.borrow_mut().push((name.to_string(), rows.row_count()));
    }));
    call(&h, root, "add", &[]);
    call(&h, lm, "remove", &[Value::Int(0)]);
    call(&h, lm, "clear", &[]);
    let seen = seen.borrow();
    let seen: Vec<(&str, usize)> = seen.iter().map(|(n, c)| (n.as_str(), *c)).collect();
    assert_eq!(
        seen,
        [
            ("about-insert", 1),
            ("insert", 2),
            ("about-remove", 2),
            ("remove", 1),
            ("about-remove", 1),
            ("remove", 0),
        ]
    );
}

/// Writes down what the view was told.
#[derive(Default)]
struct Told {
    log: RefCell<Vec<String>>,
}

impl ViewHost for Told {
    fn allocated(&self, _: &Engine, index: usize, _: ObjectId) {
        self.log.borrow_mut().push(format!("alloc{index}"));
    }
    fn ready(&self, _: &Engine, index: usize, _: ObjectId) {
        self.log.borrow_mut().push(format!("ready{index}"));
    }
    fn changed(&self, _: &Engine, change: &ModelChange, gone: &[(usize, ObjectId)]) {
        if let ModelChange::RowsRemoved { .. } = change {
            let gone: Vec<String> = gone.iter().map(|(i, _)| i.to_string()).collect();
            self.log
                .borrow_mut()
                .push(format!("gone[{}]", gone.join(",")));
        }
    }
    fn destroying(&self, _: &Engine, _: ObjectId) {
        self.log.borrow_mut().push("destroy".to_string());
    }
}

struct View {
    h: Harness,
    labels: Rc<Labels>,
    host: Rc<Told>,
    delegates: ViewDelegates,
}

fn view(rows: usize) -> View {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  property Component c: Component {
    QtObject { property string text: label; property int row: index }
  }
}
"#,
    );
    let Value::Object(component) = h.get(root, "c") else {
        panic!("no component");
    };
    let labels = Labels::of(rows);
    let holder = model_object(&h, root, &labels);
    let host = Rc::new(Told::default());
    let delegates = ViewDelegates::new(root, host.clone());
    let model = h.engine.object_model(holder);
    delegates.set_source(&h.engine, model, Some(component));
    View {
        h,
        labels,
        host,
        delegates,
    }
}

impl View {
    fn now(&self, index: usize) -> (ObjectId, Made) {
        self.delegates
            .request(&self.h.engine, index, Urgency::Now)
            .unwrap_or_else(|| panic!("row {index} has no delegate"))
    }

    fn shows(&self, obj: ObjectId) -> String {
        format!(
            "{}@{}",
            self.h.get(obj, "text").to_js_string(),
            self.h.num(obj, "row")
        )
    }

    fn told(&self) -> Vec<String> {
        std::mem::take(&mut *self.host.log.borrow_mut())
    }
}

#[test]
fn delegates_exist_only_for_the_rows_asked_for() {
    let v = view(100_000);
    assert_eq!(v.delegates.count(), 100_000);
    assert_eq!(v.delegates.created(), 0, "nothing is made until asked");
    let (a, made) = v.now(50_000);
    assert_eq!(made, Made::Created);
    assert_eq!(v.shows(a), "r50000@50000");
    assert_eq!(v.now(50_000), (a, Made::Existing));
    v.now(50_001);
    assert_eq!(v.delegates.created(), 2);
    assert_eq!(v.delegates.live().len(), 2);
    assert_eq!(v.delegates.object_at(50_000), Some(a));
    assert_eq!(v.delegates.index_of(a), Some(50_000));
    assert_eq!(v.delegates.key_at(7), Some(7));
    assert_eq!(
        v.delegates.request(&v.h.engine, 100_000, Urgency::Now),
        None
    );
    assert_eq!(v.told(), ["alloc50000", "alloc50001"]);
}

#[test]
fn a_delegate_follows_its_row_through_model_changes() {
    let v = view(10);
    let e = &v.h.engine;
    let (three, _) = v.now(3);
    let (five, _) = v.now(5);
    v.told();

    v.labels.insert(e, 0, 100);
    assert_eq!(v.shows(three), "r3@4");
    assert_eq!(v.delegates.index_of(five), Some(6));

    // Row 4 (the delegate `three`) goes to the end.
    let moved = v.labels.rows.borrow_mut().remove(4);
    v.labels.rows.borrow_mut().push(moved);
    v.labels.signals().rows_moved(e, 4, 10, 1);
    assert_eq!(v.shows(three), "r3@10");
    assert_eq!(v.shows(five), "r5@5");

    v.labels.set_data(5, "label", &Value::from("five"));
    assert_eq!(v.shows(five), "five@5");

    // A row that leaves keeps its delegate, at index -1, until the view
    // gives it back.
    v.labels.remove(e, 5, 1);
    assert_eq!(v.told(), ["gone[5]"]);
    assert_eq!(v.h.num(five, "row"), -1.0);
    assert_eq!(v.delegates.index_of(five), None);
    assert!(e.is_alive(five));
    assert_eq!(v.shows(three), "r3@9");
    assert!(!v.delegates.release(e, five, false));
    assert_eq!(v.told(), ["destroy"]);

    v.labels.rows.borrow_mut().clear();
    v.labels.signals().model_reset(e);
    assert!(v.delegates.live().is_empty());
    assert_eq!(v.told(), ["destroy"]);
    assert!(v.h.warnings().is_empty(), "{:?}", v.h.warnings());
}

#[test]
fn a_released_delegate_shows_another_row() {
    let v = view(100);
    let e = &v.h.engine;
    let (a, _) = v.now(1);
    assert!(v.delegates.release(e, a, true), "it went to the pool");
    assert_eq!(v.delegates.pooled(), 1);
    assert!(v.delegates.live().is_empty());
    let (b, made) = v.now(40);
    assert_eq!((b, made), (a, Made::Reused));
    assert_eq!(v.shows(b), "r40@40");
    assert_eq!(v.delegates.created(), 1);

    // Without reuse it is destroyed, and the next row gets a new one.
    assert!(!v.delegates.release(e, b, false));
    let (c, made) = v.now(41);
    assert_eq!(made, Made::Created);
    assert_ne!(c, a);

    v.delegates.release(e, c, true);
    v.delegates.trim_pool(e, 0);
    assert_eq!(v.delegates.pooled(), 0);
}

fn run_incubation(h: &Harness) {
    let mut rounds = 0;
    while h.engine.has_incubations() {
        for mut job in h.engine.take_incubations() {
            while !matches!(job.step(&|| false), ljos_runtime::JobPoll::Done) {}
        }
        rounds += 1;
        assert!(rounds < 10, "incubation does not settle");
    }
}

#[test]
fn a_delegate_asked_for_later_is_made_in_the_incubation_queue() {
    let v = view(100);
    let e = &v.h.engine;
    assert_eq!(v.delegates.request(e, 8, Urgency::Later), None);
    assert_eq!(v.delegates.request(e, 9, Urgency::Later), None);
    assert_eq!(v.delegates.request(e, 10, Urgency::Later), None);
    assert!(v.delegates.is_pending(8));
    assert_eq!(v.delegates.created(), 0);
    assert_eq!(
        v.delegates.request(e, 8, Urgency::Later),
        None,
        "still one job"
    );

    // The view needs row 9 on screen now: its job is finished early.
    let (nine, made) = v.now(9);
    assert_eq!(made, Made::Created);
    assert_eq!(v.shows(nine), "r9@9");

    // Row 10 is no longer wanted.
    v.delegates.cancel(e, 10);

    // A row inserted above moves the pending delegate with its row.
    v.labels.insert(e, 0, 500);
    run_incubation(&v.h);
    let eight = v.delegates.object_at(9).expect("row 8 is row 9 now");
    assert_eq!(v.shows(eight), "r8@9");
    assert_eq!(v.delegates.created(), 2);
    assert_eq!(v.delegates.live().len(), 2);
    assert!(v.told().contains(&"ready9".to_string()));
    assert!(v.h.warnings().is_empty(), "{:?}", v.h.warnings());
}

#[test]
fn rows_that_show_themselves_are_never_made_or_destroyed() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  QtObject { objectName: "a" }
  QtObject { objectName: "b" }
  QtObject { objectName: "c" }
}
"#,
    );
    let e = &h.engine;
    let (a, b, c) = (h.find(root, "a"), h.find(root, "b"), h.find(root, "c"));
    let rows = Rc::new(ObjectRows::default());
    rows.insert(e, 0, a);
    rows.insert(e, 1, b);
    assert_eq!(rows.delegate_source(), DelegateSource::Rows);
    let host = Rc::new(Told::default());
    let delegates = ViewDelegates::new(root, host.clone());
    // No component: the rows are what is shown.
    delegates.set_source(e, Some(rows.clone()), None);
    assert!(delegates.rows_show_themselves());
    assert_eq!(delegates.count(), 2);
    assert_eq!(
        delegates.request(e, 1, Urgency::Later),
        Some((b, Made::Created))
    );
    assert_eq!(
        delegates.request(e, 1, Urgency::Now),
        Some((b, Made::Existing))
    );
    assert_eq!(
        delegates.request(e, 0, Urgency::Now),
        Some((a, Made::Created))
    );
    assert_eq!(delegates.created(), 0, "the view made nothing");

    rows.insert(e, 0, c);
    assert_eq!(delegates.index_of(b), Some(2));
    assert!(rows.move_rows(e, 2, 0, 1));
    assert_eq!(rows.objects(), [b, c, a]);
    assert_eq!(delegates.index_of(b), Some(0));
    assert_eq!(delegates.index_of(a), Some(2));
    assert!(!rows.move_rows(e, 2, 5, 1), "no such row to move to");

    // A row that is given back, or leaves the model, stays alive; the
    // view is only told to let go of it.
    std::mem::take(&mut *host.log.borrow_mut());
    assert!(!delegates.release(e, a, true), "an own row is not pooled");
    assert_eq!(delegates.pooled(), 0);
    assert_eq!(rows.remove(e, 0, 1), [b]);
    delegates.release(e, b, false);
    rows.clear(e);
    assert!(delegates.live().is_empty());
    assert!(e.is_alive(a) && e.is_alive(b) && e.is_alive(c));
    let told = std::mem::take(&mut *host.log.borrow_mut());
    assert_eq!(told, ["destroy", "gone[0]", "destroy"]);
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

#[test]
fn a_model_that_brings_its_delegate_is_told_of_each_one() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  property Component mine: Component { QtObject { property string text: "m" + index } }
  property Component views: Component { QtObject { property string text: "v" + index } }
}
"#,
    );
    let e = &h.engine;
    let component = |name: &str| match h.get(root, name) {
        Value::Object(c) => c,
        _ => panic!("no component {name}"),
    };
    let rows = Rc::new(DelegateRows::default());
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let (early, made, gone) = (log.clone(), log.clone(), log.clone());
    rows.on_allocated(Some(Rc::new(move |e: &Engine, row, object| {
        // Before any binding of the delegate ran.
        let text = e.read_by_name(object, "text").to_js_string();
        early.borrow_mut().push(format!("new{row}{text}"));
    })));
    rows.on_delegates(
        Some(Rc::new(move |_: &Engine, row, _| {
            made.borrow_mut().push(format!("made{row}"));
        })),
        Some(Rc::new(move |_: &Engine, _| {
            gone.borrow_mut().push("gone".to_string());
        })),
    );
    let host = Rc::new(Told::default());
    let delegates = ViewDelegates::new(root, host.clone());
    delegates.set_source(e, Some(rows.clone()), None);
    assert_eq!(delegates.count(), 0, "no model and no delegate yet");

    rows.set_model(e, Some(e.model_from_value(&Value::Int(3), Some(root))));
    assert_eq!(delegates.count(), 0, "rows without a delegate show nothing");
    rows.set_component(e, Some(component("mine")));
    assert_eq!(
        rows.delegate_source(),
        DelegateSource::Component(rows.component())
    );
    assert_eq!(delegates.count(), 3);
    let (one, _) = delegates.request(e, 1, Urgency::Now).expect("row 1");
    assert_eq!(h.get(one, "text"), Value::from("m1"));
    assert_eq!(*log.borrow(), ["new1", "made1"]);

    // Another delegate: every row is made again.
    rows.set_component(e, Some(component("mine")));
    assert_eq!(
        delegates.object_at(1),
        Some(one),
        "the same delegate changes nothing"
    );
    rows.set_component(e, None);
    assert!(delegates.live().is_empty());
    assert_eq!(*log.borrow(), ["new1", "made1", "gone"]);

    // What the view itself was given comes before the model's.
    rows.set_component(e, Some(component("mine")));
    delegates.set_source(e, Some(rows.clone()), Some(component("views")));
    let (two, _) = delegates.request(e, 2, Urgency::Now).expect("row 2");
    assert_eq!(h.get(two, "text"), Value::from("v2"));

    // The rows follow the model inside.
    rows.set_model(e, Some(e.model_from_value(&Value::Int(1), Some(root))));
    assert_eq!(delegates.count(), 1);
    assert!(delegates.live().is_empty());
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

#[test]
fn an_object_gets_named_values_in_its_context() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  property Component plain: Component { QtObject { property string text: "s-" + section } }
  property Component strict: Component {
    QtObject { required property string section; property string text: "r-" + section }
  }
}
"#,
    );
    let e = &h.engine;
    for (name, expected) in [("plain", "s-fruit"), ("strict", "r-fruit")] {
        let Value::Object(component) = h.get(root, name) else {
            panic!("no component");
        };
        let mut parented = None;
        let made = e
            .create_in_context(
                component,
                Some(root),
                &[("section", Value::from("fruit"))],
                |_, o| parented = Some(o),
            )
            .unwrap();
        assert_eq!(parented, Some(made));
        assert_eq!(h.get(made, "text"), Value::from(expected));
    }
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}
