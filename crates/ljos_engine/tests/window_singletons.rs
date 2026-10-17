//! Singletons that are one for each window (`scope = "window"`): each
//! root the host makes owns a scope, everything under it reads that
//! scope's object, and the object goes with the root. The behaviour as a
//! QML author sees it is in `features/language/singletons-per-window.feature`;
//! these tests cover the Rust side: `singleton_in`, the listener and a
//! reload. Tier 1, which needs a compile that knows the type, is in
//! `crates/ljos/tests/singletons.rs`.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use ljos_compiler::ReloadSession;
use ljos_engine::{
    Engine, MapSource, ObjectId, QmlObject, ScopedEvent, Value, Version, qml_singleton,
};
use qml_front::QtTypeOracle;
use support::{Harness, engine};

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Repro", name = "Document")]
#[qml_singleton(scope = "window")]
pub struct Document {
    #[qml(notify)]
    pub title: String,
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Repro", name = "Session")]
#[qml_singleton]
pub struct Session {
    #[qml(notify)]
    pub title: String,
}

const MAIN: &str = r#"import QtQuick
import Repro
Item {
    property string title: Document.title
    property string session: Session.title
    function name(t) { Document.title = t }
}
"#;

fn with_types() -> Harness {
    let h = engine();
    h.engine
        .register_singleton::<Document>("Repro", Version::new(1, 0));
    h.engine
        .register_singleton::<Session>("Repro", Version::new(1, 0));
    h
}

fn text(h: &Harness, obj: ObjectId, prop: &str) -> String {
    match h.get(obj, prop) {
        Value::String(s) => s.to_string(),
        other => panic!("{prop} is {other:?}"),
    }
}

#[test]
fn each_root_reads_its_own_object() {
    let h = with_types();
    h.files(&[("Main", MAIN)]);
    let a = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    let b = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    h.engine
        .invoke_by_name(a, "name", &[Value::from("A")])
        .unwrap();
    h.engine
        .invoke_by_name(b, "name", &[Value::from("B")])
        .unwrap();
    assert_eq!(text(&h, a, "title"), "A");
    assert_eq!(text(&h, b, "title"), "B");

    let da = h.engine.singleton_in::<Document>(a).unwrap();
    let db = h.engine.singleton_in::<Document>(b).unwrap();
    assert_ne!(da, db);
    assert_eq!(text(&h, da, "title"), "A");
    // From Rust, a write reaches the window's QML.
    h.engine
        .write_by_name(db, "title", Value::from("B2"))
        .unwrap();
    assert_eq!(text(&h, b, "title"), "B2");
    assert_eq!(text(&h, a, "title"), "A");
    assert_eq!(h.engine.singleton_scope_root(da), None);
    assert_eq!(h.engine.scoped_singletons(a), vec![da]);
    assert!(h.warnings().is_empty());
}

#[test]
fn there_is_no_one_object_of_a_type_for_each_window() {
    let h = with_types();
    let err = h.engine.singleton::<Document>().unwrap_err();
    assert!(err.to_string().contains("singleton_in"), "{err}");
    assert_eq!(h.engine.singleton_by_name("Repro", "Document"), None);
}

#[test]
fn an_engine_singleton_stays_shared() {
    let h = with_types();
    h.files(&[("Main", MAIN)]);
    let a = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    let b = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    let session = h.engine.singleton::<Session>().unwrap();
    assert_eq!(h.engine.singleton_in::<Session>(a), Ok(session));
    h.engine
        .write_by_name(session, "title", Value::from("both"))
        .unwrap();
    assert_eq!(text(&h, a, "session"), "both");
    assert_eq!(text(&h, b, "session"), "both");
}

#[test]
fn what_is_under_a_root_reads_the_root_s_object() {
    let h = with_types();
    h.files(&[
        (
            "Probe",
            "import QtQuick\nimport Repro\nItem { property string shown: Document.title }\n",
        ),
        (
            "Main",
            r#"import QtQuick
import Repro
Item {
    property Item made
    Item { objectName: "inner"; property string shown: Document.title }
    function make() { made = Qt.createComponent("Probe.qml").createObject(null) }
    Component.onCompleted: Document.title = "mine"
}
"#,
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    let inner = h.find(root, "inner");
    assert_eq!(text(&h, inner, "shown"), "mine");
    // A root the host makes under an object is in that object's window.
    let child = h
        .engine
        .create_from_url("Probe", Some(root), Vec::new())
        .unwrap();
    assert_eq!(text(&h, child, "shown"), "mine");
    assert_eq!(h.engine.singleton_scope_root(child), Some(root));
    // So is one a script of the window makes with no parent.
    h.engine.invoke_by_name(root, "make", &[]).unwrap();
    let made = h.get(root, "made").as_object().expect("made");
    assert_eq!(text(&h, made, "shown"), "mine");
    assert_eq!(h.engine.singleton_scope_root(made), Some(root));
}

/// What the listener was told, and whether the object stood then.
type Heard = (ScopedEvent, ObjectId, ObjectId, bool);

#[test]
fn the_listener_hears_an_object_made_and_dropped_with_its_window() {
    let h = with_types();
    h.files(&[("Main", MAIN)]);
    let heard: Rc<RefCell<Vec<Heard>>> = Rc::default();
    let sink = heard.clone();
    h.engine.on_scoped_singleton(move |e, s| {
        assert!(s.is::<Document>());
        sink.borrow_mut()
            .push((s.event, s.root, s.object, e.is_alive(s.object)));
    });
    let a = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    let doc = h.engine.singleton_in::<Document>(a).unwrap();
    assert_eq!(*heard.borrow(), vec![(ScopedEvent::Made, a, doc, true)]);
    // Asked again: the same object, and nothing heard.
    assert_eq!(h.engine.singleton_in::<Document>(a), Ok(doc));
    assert_eq!(heard.borrow().len(), 1);

    assert!(h.engine.destroy_now(a));
    assert_eq!(
        heard.borrow()[1],
        (ScopedEvent::Dropped, a, doc, true),
        "told while it stands"
    );
    assert!(!h.engine.is_alive(doc), "it goes with the window");

    // Another window has another object.
    let b = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    let other = h.engine.singleton_in::<Document>(b).unwrap();
    assert_ne!(other, doc);
    assert_eq!(text(&h, b, "title"), "");
}

#[test]
fn a_window_that_never_reads_the_type_has_no_object() {
    let h = with_types();
    h.files(&[("Main", "import QtQuick\nItem {}\n")]);
    let made = Rc::new(RefCell::new(0));
    let sink = made.clone();
    h.engine
        .on_scoped_singleton(move |_, _| *sink.borrow_mut() += 1);
    let a = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    assert_eq!(*made.borrow(), 0);
    assert!(h.engine.scoped_singletons(a).is_empty());
    h.engine.destroy_now(a);
    assert_eq!(*made.borrow(), 0);
}

#[test]
fn an_object_under_no_window_has_none() {
    let h = with_types();
    let plain = h
        .engine
        .create_native(support::Item::default(), None)
        .expect("an item");
    assert!(h.engine.singleton_in::<Document>(plain).is_err());
}

#[test]
fn a_reload_keeps_each_window_s_object() {
    let h = with_types();
    h.engine.set_reload_tracking(true);
    let mut session = ReloadSession::in_memory(QtTypeOracle);
    session.set_file("Main", MAIN);
    let mut source = MapSource::new();
    for (key, file) in session.start() {
        source.insert(key.clone(), file.clone());
    }
    h.engine.set_component_source(source);
    let a = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    let b = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    h.engine
        .invoke_by_name(a, "name", &[Value::from("A")])
        .unwrap();
    h.engine
        .invoke_by_name(b, "name", &[Value::from("B")])
        .unwrap();
    let da = h.engine.singleton_in::<Document>(a).unwrap();
    let db = h.engine.singleton_in::<Document>(b).unwrap();
    let dropped = Rc::new(RefCell::new(0));
    let sink = dropped.clone();
    h.engine.on_scoped_singleton(move |_, s| {
        if s.event == ScopedEvent::Dropped {
            *sink.borrow_mut() += 1;
        }
    });

    session.set_file(
        "Main",
        MAIN.replace(
            "property string title: Document.title",
            "property string title: \"now \" + Document.title",
        ),
    );
    let patch = session.build().unwrap().unwrap();
    let done = h
        .engine
        .reload(&patch, &[a, b], &mut |_: &Engine, _, _| {})
        .expect("reloads");
    session.ack(patch.generation);
    h.engine.finish_reload();
    let (na, nb) = (done.roots[0].1, done.roots[1].1);
    assert!(!h.engine.is_alive(a));
    assert_eq!(text(&h, na, "title"), "now A");
    assert_eq!(text(&h, nb, "title"), "now B");
    assert_eq!(h.engine.singleton_in::<Document>(na), Ok(da));
    assert_eq!(h.engine.singleton_in::<Document>(nb), Ok(db));
    assert!(h.engine.is_alive(da));
    assert_eq!(*dropped.borrow(), 0);
    // The new root owns the scope now: it goes with it.
    h.engine.destroy_now(na);
    assert!(!h.engine.is_alive(da));
    assert!(h.engine.is_alive(db));
}

#[test]
fn the_issue_s_repro_reads_two_titles() {
    // Two roots of one file, as an application with two windows makes them.
    let h = with_types();
    h.files(&[(
        "Main",
        "import Repro\nimport QtQuick\nItem { property string title: Document.title }\n",
    )]);
    let a = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    let b = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    for (root, t) in [(a, "first"), (b, "second")] {
        let doc = h.engine.singleton_in::<Document>(root).unwrap();
        h.engine
            .write_by_name(doc, "title", Value::from(t))
            .unwrap();
    }
    assert_eq!(text(&h, a, "title"), "first");
    assert_eq!(text(&h, b, "title"), "second");
}
