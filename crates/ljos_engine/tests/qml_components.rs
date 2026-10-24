//! Components: file components, inline components, required properties,
//! completion order, `Component.createObject` and incremental creation.

mod support;

use ljos_engine::{CreationJob, EngineError, ObjectFlags, Value};
use ljos_runtime::{Lane, ProcId};
use support::engine;

#[test]
fn on_completed_runs_in_qts_order() {
    let h = engine();
    h.files(&[
        (
            "Foo",
            r#"import QtQuick
Item { Component.onCompleted: console.log("foo root")
  Item { Component.onCompleted: console.log("foo child") } }
"#,
        ),
        (
            "Main",
            r#"import QtQuick
Item {
    Component.onCompleted: console.log("root")
    Item { Component.onCompleted: console.log("a")
        Item { Component.onCompleted: console.log("a1") }
        Item { Component.onCompleted: console.log("a2") }
    }
    Item { Component.onCompleted: console.log("b") }
    Foo { Component.onCompleted: console.log("foo use") }
}
"#,
        ),
    ]);
    h.engine.create_from_url("Main", None, vec![]).unwrap();
    // Checked with qml6: each object before its children, the last child
    // first, and a component's own handler before its user's.
    assert_eq!(
        h.log(),
        vec![
            "root",
            "foo root",
            "foo use",
            "foo child",
            "b",
            "a",
            "a2",
            "a1"
        ]
    );
}

#[test]
fn a_file_component_is_a_type_with_its_own_properties() {
    let h = engine();
    h.files(&[
        (
            "Button",
            r#"import QtQuick
Item {
  id: me
  property string label: "?"
  property int clicks: 0
  signal clicked()
  onClicked: clicks++
  width: label.length * 10
}
"#,
        ),
        (
            "Main",
            r#"import QtQuick
Item {
  Button { id: b; objectName: "b"; label: "four" }
  property int total: b.clicks
}
"#,
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    let b = h.find(root, "b");
    assert_eq!(h.num(b, "width"), 40.0);
    h.engine.emit_by_name(b, "clicked", &[]).unwrap();
    h.engine.emit_by_name(b, "clicked", &[]).unwrap();
    assert_eq!(h.num(root, "total"), 2.0);
    assert_eq!(h.engine.meta_of(b).unwrap().name.as_str(), "Button");
}

#[test]
fn a_property_typed_through_an_import_namespace_holds_an_object() {
    let h = engine();
    h.files(&[
        (
            "Holder",
            r#"import QtQuick as Q
Q.Item { property Q.Item target }
"#,
        ),
        (
            "Main",
            r#"import QtQuick
Item {
  Item { id: other; objectName: "other" }
  Holder { id: holder; target: other }
  property bool held: holder.target === other
}
"#,
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    assert!(matches!(h.get(root, "held"), Value::Bool(true)));
    let log = h.log();
    assert!(
        !log.iter().any(|l| l.contains("Unable to assign")),
        "{log:?}"
    );
}

#[test]
fn ids_of_a_file_component_are_private_to_it() {
    let h = engine();
    h.files(&[
        (
            "Inner",
            "import QtQuick\nItem { id: box; property int v: 1; property int w: box.v * 2 }\n",
        ),
        (
            "Main",
            "import QtQuick\nItem { id: box; property int v: 10\n  Inner { objectName: \"i\" } }\n",
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    let i = h.find(root, "i");
    assert_eq!(h.num(i, "w"), 2.0);
}

#[test]
fn inline_components_and_required_properties() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  component Cell: Item { required property int n; width: n * 2 }
  Cell { objectName: "c"; n: 4 }
}
"#,
    );
    let c = h.find(root, "c");
    assert_eq!(h.num(c, "width"), 8.0);
}

#[test]
fn a_missing_required_property_is_an_error() {
    let h = engine();
    h.files(&[(
        "Main",
        "import QtQuick\nItem {\n  required property int n\n}\n",
    )]);
    let before = h.engine.object_count();
    let err = h.engine.create_from_url("Main", None, vec![]).unwrap_err();
    assert!(
        matches!(&err, EngineError::RequiredNotSet(m)
            if m == "file:///Main.qml:3:3: Required property n was not initialized"),
        "{err:?}"
    );
    // Each line says it whole: the error reads as its lines, once.
    assert_eq!(
        err.to_string(),
        "file:///Main.qml:3:3: Required property n was not initialized"
    );
    assert_eq!(h.engine.object_count(), before);
    // Given as an initial property, it is set.
    let root = h
        .engine
        .create_from_url("Main", None, vec![("n".into(), Value::Int(3))])
        .unwrap();
    assert_eq!(h.num(root, "n"), 3.0);
}

#[test]
fn objects_are_completed_once_created() {
    let h = engine();
    let root = h.load("import QtQuick\nItem { Item { objectName: \"k\" } }\n");
    let k = h.find(root, "k");
    assert!(
        h.engine
            .flags(root)
            .unwrap()
            .contains(ObjectFlags::COMPLETED)
    );
    assert!(h.engine.flags(k).unwrap().contains(ObjectFlags::COMPLETED));
}

#[test]
fn component_create_object_makes_instances_with_properties() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  id: root
  property int base: 5
  property var made: []
  Component {
    id: comp
    Item { property int n; width: n + root.base }
  }
  function make(n) {
    var o = comp.createObject(root, { n: n })
    made = made.concat([o])
    return o.width
  }
  property int status: comp.status
}
"#,
    );
    assert_eq!(h.num(root, "status"), 1.0);
    let w = h
        .engine
        .invoke_by_name(root, "make", &[Value::Int(3)])
        .unwrap();
    assert_eq!(w.to_number(), 8.0);
    let kids = h.engine.visual_children(root);
    assert_eq!(kids.len(), 1);
    h.set(root, "base", Value::Int(10));
    assert_eq!(h.num(kids[0], "width"), 13.0);
    assert!(h.warnings().is_empty());
}

#[test]
fn a_component_from_a_url_is_created_with_initial_properties() {
    let h = engine();
    h.files(&[
        (
            "Label",
            "import QtQuick\nItem { property string text; width: text.length }\n",
        ),
        ("Main", "import QtQuick\nItem {}\n"),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    let l = h
        .engine
        .create_from_url(
            "Label",
            Some(root),
            vec![("text".into(), Value::from("hello"))],
        )
        .unwrap();
    assert_eq!(h.num(l, "width"), 5.0);
    assert_eq!(h.engine.visual_parent(l), Some(root));
    assert_eq!(h.engine.owner(l), Some(root));
}

#[test]
fn destroy_from_js_removes_the_object() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  Item { id: kid; objectName: "kid" }
  property bool gone: false
  function drop() { kid.destroy() }
}
"#,
    );
    let kid = h.find(root, "kid");
    h.engine.invoke_by_name(root, "drop", &[]).unwrap();
    // Deferred, as in Qt: gone once control returns to the event loop.
    h.pass_ms(0);
    assert!(!h.engine.is_alive(kid));
}

#[test]
fn incremental_creation_yields_and_finishes() {
    let h = engine();
    let mut src = String::from("import QtQuick\nItem {\n  property int sum: 0\n");
    for i in 0..50 {
        src.push_str(&format!("  Item {{ width: {i} }}\n"));
    }
    src.push_str("  Component.onCompleted: console.log(\"done\")\n}\n");
    h.files(&[("Main", &src)]);
    let (mut job, handle) = CreationJob::new(
        &h.engine,
        "Main",
        None,
        vec![],
        Lane::Visible,
        ProcId::new(0, 0),
    );
    let mut steps = 0;
    loop {
        // A slice that is spent after every object.
        match job.step(&|| true) {
            ljos_runtime::JobPoll::Done => break,
            ljos_runtime::JobPoll::Yield => steps += 1,
            ljos_runtime::JobPoll::Failed(f) => panic!("{f:?}"),
        }
        assert!(h.log().is_empty(), "completion runs at the end");
    }
    assert!(steps > 10, "{steps}");
    let root = handle.result().unwrap().unwrap();
    assert_eq!(h.engine.visual_children(root).len(), 50);
    assert_eq!(h.log(), vec!["done"]);
}

/// A parent with fifty children, and a `Component` of the same.
const MANY: &str = r#"import QtQuick
Item {
  Component {
    id: comp
    Item {
      objectName: "made"
      property int n: 0
      Item {} Item {} Item {} Item {} Item {} Item {} Item {} Item {}
      Component.onCompleted: console.log("done " + n)
    }
  }
  property Component c: comp
}
"#;

#[test]
fn a_job_makes_an_object_from_a_component_and_reports_it() {
    let h = engine();
    let root = h.load(MANY);
    let comp = h.get(root, "c").as_object().unwrap();
    let seen = std::rc::Rc::new(std::cell::RefCell::new(None));
    let s = seen.clone();
    let (job, handle) = CreationJob::for_component(
        &h.engine,
        comp,
        Some(root),
        vec![("n".into(), Value::Int(4))],
        Lane::Visible,
        ProcId::new(0, 0),
    );
    let mut job = job.on_done(move |_, r| *s.borrow_mut() = Some(r.clone()));
    assert_eq!(handle.progress(), 0.0);
    let mut yields = 0;
    while matches!(job.step(&|| true), ljos_runtime::JobPoll::Yield) {
        yields += 1;
        assert!(seen.borrow().is_none());
    }
    assert!(yields > 4, "{yields}");
    let made = handle.result().unwrap().unwrap();
    assert_eq!(*seen.borrow(), Some(Ok(made)));
    assert_eq!(handle.progress(), 1.0);
    assert_eq!(h.num(made, "n"), 4.0);
    assert_eq!(h.engine.owner(made), Some(root));
    assert_eq!(h.log(), vec!["done 4"]);
}

#[test]
fn a_cancelled_job_leaves_nothing_behind() {
    let h = engine();
    let root = h.load(MANY);
    let comp = h.get(root, "c").as_object().unwrap();
    let before = h.engine.object_count();
    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    let c = called.clone();
    let (job, handle) = CreationJob::for_component(
        &h.engine,
        comp,
        Some(root),
        vec![],
        Lane::Visible,
        ProcId::new(0, 0),
    );
    let mut job = job.on_done(move |_, _| c.set(true));
    for _ in 0..4 {
        assert!(matches!(job.step(&|| true), ljos_runtime::JobPoll::Yield));
    }
    assert!(h.engine.object_count() > before, "the object is half made");
    handle.cancel();
    assert!(matches!(job.step(&|| true), ljos_runtime::JobPoll::Done));
    assert_eq!(h.engine.object_count(), before);
    assert!(h.engine.visual_children(root).is_empty());
    assert!(handle.result().unwrap().is_err());
    assert!(!called.get(), "nobody waits for a cancelled job");
    assert!(h.log().is_empty(), "it never completed");
}

#[test]
fn a_job_whose_parent_died_ends_with_an_error() {
    let h = engine();
    let root = h.load(MANY);
    let comp = h.get(root, "c").as_object().unwrap();
    let holder = h.engine.create_by_name("QtQuick", "Item", None).unwrap();
    let (mut job, handle) = CreationJob::for_component(
        &h.engine,
        comp,
        Some(holder),
        vec![],
        Lane::Visible,
        ProcId::new(0, 0),
    );
    for _ in 0..3 {
        assert!(matches!(job.step(&|| true), ljos_runtime::JobPoll::Yield));
    }
    h.engine.destroy_now(holder);
    assert!(matches!(job.step(&|| true), ljos_runtime::JobPoll::Done));
    assert_eq!(handle.result(), Some(Err(EngineError::DeadObject(holder))));
}

#[test]
fn queued_jobs_wake_the_host_and_do_not_keep_the_engine() {
    let h = engine();
    let root = h.load(MANY);
    let comp = h.get(root, "c").as_object().unwrap();
    let woke = std::rc::Rc::new(std::cell::Cell::new(0));
    let w = woke.clone();
    h.engine.set_timer_wake(move || w.set(w.get() + 1));
    let (job, handle) = CreationJob::for_component(
        &h.engine,
        comp,
        Some(root),
        vec![],
        Lane::Visible,
        ProcId::new(0, 0),
    );
    assert!(!h.engine.has_incubations());
    h.engine.incubate(job);
    assert_eq!(woke.get(), 1);
    assert!(h.engine.has_incubations());
    let mut jobs = h.engine.take_incubations();
    assert_eq!(jobs.len(), 1);
    assert!(!h.engine.has_incubations());
    // A job outlives its engine without keeping it alive.
    drop(h);
    assert!(matches!(
        jobs[0].step(&|| false),
        ljos_runtime::JobPoll::Done
    ));
    assert!(handle.result().unwrap().is_err());
}

#[test]
fn a_file_that_uses_a_missing_module_is_rejected() {
    let h = engine();
    h.files(&[("Main", "import QtQuick\nItem {}\n")]);
    // Loading succeeds when the module exists.
    assert!(h.engine.create_from_url("Main", None, vec![]).is_ok());
    let err = h.engine.create_from_url("Nope", None, vec![]).unwrap_err();
    assert!(err.to_string().contains("No such file"), "{err}");
}

/// Loads `source`, runs `drop()` and the deferred deletes, and returns
/// what the handlers wrote to `events`.
fn events_after_drop(source: &str) -> String {
    let h = engine();
    let root = h.load(source);
    h.engine.invoke_by_name(root, "drop", &[]).unwrap();
    assert_eq!(
        h.get(root, "events"),
        Value::from(""),
        "destroy() is deferred"
    );
    h.engine.process_deferred_deletes();
    assert!(h.warnings().is_empty());
    h.get(root, "events").to_js_string().to_string()
}

#[test]
fn destroying_a_component_instance_tells_children_before_parents() {
    let events = events_after_drop(
        r#"import QtQuick
Item {
  id: root
  property string events: ""
  Component {
    id: comp
    Item {
      id: top
      Component.onDestruction: root.events += "top" + top.children.length + (top.parent === root) + " "
      Item {
        Component.onDestruction: root.events += "middle "
        Item { Component.onDestruction: root.events += "bottom " }
      }
      QtObject { Component.onDestruction: root.events += "plain " }
    }
  }
  property var made: comp.createObject(root)
  function drop() { made.destroy() }
}
"#,
    );
    // Everything is still in place while the handlers run.
    assert_eq!(events, "bottom middle plain top1true ");
}

const LEAVES: &str = r#"import QtQuick
Item {
  id: root
  property string events: ""
  component Leaf: Item {
    property string name
    Component.onDestruction: root.events += name + " "
    Item { Component.onDestruction: root.events += parent.name + ".inner " }
  }
  Component {
    id: comp
    Item {
      Component.onDestruction: root.events += "top "
      Leaf { name: "first"; Component.onDestruction: root.events += "(first) " }
      Item {
        objectName: "inline"
        Component.onDestruction: root.events += "inline "
        Leaf { name: "second" }
      }
    }
  }
  property var made: comp.createObject(root)
  function drop() { DROP.destroy() }
}
"#;

#[test]
fn nested_component_instances_are_told_after_the_file_that_uses_them() {
    // The youngest instance first; an instance's root is told with its
    // own component, handlers written at the place of use included.
    assert_eq!(
        events_after_drop(&LEAVES.replace("DROP", "made")),
        "inline top second.inner second first.inner first (first) "
    );
}

#[test]
fn an_inline_object_destroyed_alone_is_not_told() {
    // Its file lives on, so only the component instance inside it hears.
    assert_eq!(
        events_after_drop(&LEAVES.replace("DROP", "made.children[1]")),
        "second.inner second "
    );
}

#[test]
fn a_root_of_derived_components_is_told_from_the_second_layer_outwards() {
    // Qt 6.11.2, by running qml6. A Loader, which announces the whole
    // instance itself, starts with the outermost file instead.
    let source = r#"import QtQuick
Item {
  id: root
  property string events: ""
  component Base: Item {
    property string name
    Component.onDestruction: root.events += name + ".base "
    Item { Component.onDestruction: root.events += parent.name + ".baseinner " }
  }
  component Kid: Base {
    Component.onDestruction: root.events += name + ".kid "
    Item { Component.onDestruction: root.events += parent.name + ".kidinner " }
  }
  component Top: Kid {
    Component.onDestruction: root.events += name + ".top "
    Item { Component.onDestruction: root.events += parent.name + ".topinner " }
  }
  Component {
    id: comp
    Top {
      name: "r"
      Component.onDestruction: root.events += "r.use "
      Item { Component.onDestruction: root.events += "r.usechild " }
      Kid { name: "k1" }
      Kid { name: "k2" }
    }
  }
  property var made: comp.createObject(root)
  function drop() { made.destroy() }
}
"#;
    let kids = "k2.kidinner k2.baseinner k2.base k2.kid k1.kidinner k1.baseinner k1.base k1.kid ";
    assert_eq!(
        events_after_drop(source),
        format!("r.kidinner r.baseinner r.base r.kid r.top r.use r.topinner r.usechild {kids}")
    );

    let h = engine();
    let root = h.load(source);
    let made = h.get(root, "made").as_object().unwrap();
    h.engine.announce_destruction(made);
    assert_eq!(
        h.get(root, "events").to_js_string().to_string(),
        format!("r.usechild {kids}r.topinner r.kidinner r.baseinner r.base r.kid r.top r.use ")
    );
    // Told once: the deletion that follows is silent.
    h.set(root, "events", Value::from(""));
    h.engine.destroy_now(made);
    assert_eq!(h.get(root, "events"), Value::from(""));
}

#[test]
fn a_creation_that_fails_tells_the_object_it_drops() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  id: root
  property string events: ""
  Component {
    id: comp
    Item { required property int needed; Component.onDestruction: root.events += "failed " }
  }
  property var made: comp.createObject(root)
}
"#,
    );
    assert_eq!(h.get(root, "made"), Value::Null);
    assert_eq!(h.get(root, "events"), Value::from("failed "));
    assert_eq!(h.warnings().len(), 1);
}

#[test]
fn an_inline_component_reads_the_ids_of_the_file_that_uses_it() {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  id: root
  property int n: 5
  component Leaf: Item { property int m: root.n + 1 }
  Leaf { objectName: "leaf" }
}
"#,
    );
    let leaf = h.find(root, "leaf");
    assert_eq!(h.num(leaf, "m"), 6.0);
    h.set(root, "n", Value::Int(9));
    assert_eq!(h.num(leaf, "m"), 10.0);
    assert!(h.warnings().is_empty());
}

#[test]
fn a_url_is_resolved_against_the_file_that_declares_the_object() {
    let h = engine();
    h.files(&[
        (
            "parts/Part",
            "import QtQuick\nItem { Item { objectName: \"inner\" } }\n",
        ),
        ("Main", "import QtQuick\nItem { objectName: \"main\" }\n"),
    ]);
    let e = &h.engine;
    let main = e.create_from_url("Main", None, Vec::new()).unwrap();
    // A root made straight from a URL is declared by its own file.
    let part = e
        .create_from_url("file:///parts/Part.qml", Some(main), Vec::new())
        .unwrap();
    let inner = h.find(part, "inner");
    assert_eq!(e.resolve_url(main, "Leaf.qml"), "file:///Leaf.qml");
    assert_eq!(e.resolve_url(inner, "Leaf.qml"), "file:///parts/Leaf.qml");
    assert_eq!(e.resolve_url(inner, "../Leaf.qml"), "file:///Leaf.qml");
    assert_eq!(e.resolve_url(inner, "qrc:/x/Leaf.qml"), "qrc:/x/Leaf.qml");
    assert_eq!(e.resolve_url(inner, ""), "");
    // The resolved URL names the file again.
    let again = e.resolve_url(inner, "Part.qml");
    assert!(e.create_from_url(&again, Some(main), Vec::new()).is_ok());
}

/// The first object of a type made from another builds accessors only for
/// what the type adds: the base's are its base's own functions, which every
/// type made from the same base shares. The prototype lists them first.
#[test]
fn a_type_s_first_object_builds_only_what_its_type_adds() {
    let h = engine();
    h.files(&[
        (
            "Base",
            "import QtQuick\nItem { property int alpha: 1\n function hello() { return 'hi' }\n signal rang }\n",
        ),
        (
            "One",
            "import QtQuick\nBase { property int omega: 2\n function bye() { return alpha + omega } }\n",
        ),
        (
            "Two",
            "import QtQuick\nBase { property int alpha: 5\n function hello() { return 'own' } }\n",
        ),
        (
            "Main",
            r#"import QtQuick
Item {
    One { id: one }
    Two { id: two }
    function shape() {
        var proto = (o) => Object.getPrototypeOf(o);
        var get = (o, n) => Object.getOwnPropertyDescriptor(proto(o), n).get;
        var same = (n) => get(one, n) === get(two, n);
        var names = Object.getOwnPropertyNames(proto(one));
        var before = (a, b) => names.indexOf(a) >= 0 && names.indexOf(a) < names.indexOf(b);
        return [same("x"), same("rang"), same("alpha"), same("hello"),
                before("x", "alpha"), before("alpha", "omega"), before("omega", "rang"),
                before("rang", "omegaChanged"), before("hello", "bye"),
                one.bye(), one.hello(), two.alpha, two.hello(), typeof two.rang,
                String(one).replace(/\(.*/, "")].join(" ")
    }
}
"#,
        ),
    ]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    let shape = h.engine.invoke_by_name(root, "shape", &[]).unwrap();
    assert_eq!(
        shape.to_js_string().to_string(),
        "true true false false true true true true true 3 hi 5 own function One_QMLTYPE_1"
    );
}

#[test]
fn the_engine_knows_the_modules_the_application_uses() {
    let h = engine();
    h.files(&[
        ("Leaf", "import QtQml\nQtObject {}\n"),
        ("Main", "import QtQuick\nItem { Leaf {} }\n"),
    ]);
    assert!(!h.engine.uses_module("QtQuick"));
    h.engine.create_from_url("Main", None, vec![]).unwrap();
    assert!(h.engine.uses_module("QtQuick"));
    // Leaf's import, and the module of the type it makes.
    assert!(h.engine.uses_module("QtQml"));
    assert!(!h.engine.uses_module("QtQuick.Controls"));
}
