//! Dev hot reload in the engine: a patch applied whole or not at all,
//! and what the objects that go on carry. `features/hot-reload/` holds
//! the behaviour as a developer sees it; these tests cover the engine's
//! own API and what a scenario cannot see (which objects were made anew,
//! what a failed patch leaves behind).

mod support;

use ljos_compiler::ReloadSession;
use ljos_engine::{
    Engine, MapSource, Migrate, ObjectId, QmlObject, QtObject, ReloadError, ReloadOutcome, Request,
    Value,
};
use ljos_runtime::{JobPoll, Lane, ProcId};
use qml_front::QtTypeOracle;
use support::{Harness, engine};

struct Rig {
    h: Harness,
    session: ReloadSession<QtTypeOracle>,
    root: ObjectId,
}

fn rig_on(h: Harness, files: &[(&str, &str)]) -> Rig {
    h.engine.set_reload_tracking(true);
    let mut session = ReloadSession::in_memory(QtTypeOracle);
    for (key, source) in files {
        session.set_file(key, *source);
    }
    let mut source = MapSource::new();
    for (key, file) in session.start() {
        source.insert(key.clone(), file.clone());
    }
    h.engine.set_component_source(source);
    let root = h
        .engine
        .create_from_url("Main", None, Vec::new())
        .expect("Main.qml creates");
    Rig { h, session, root }
}

fn rig(files: &[(&str, &str)]) -> Rig {
    rig_on(engine(), files)
}

impl Rig {
    /// Saves `source` as `key` and reloads.
    fn edit(&mut self, key: &str, source: &str) -> Result<ReloadOutcome, ReloadError> {
        self.session.set_file(key, source);
        self.apply()
    }

    fn apply(&mut self) -> Result<ReloadOutcome, ReloadError> {
        let patch = self
            .session
            .build()
            .expect("the edit compiles")
            .expect("the edit changes something");
        let r = self
            .h
            .engine
            .reload(&patch, &[self.root], &mut |_, _, _| {});
        match &r {
            Ok(done) => {
                self.session.ack(patch.generation);
                self.root = done.roots[0].1;
                self.h.engine.finish_reload();
            }
            Err(_) => self.session.reject(patch.generation),
        }
        r
    }

    fn find(&self, name: &str) -> ObjectId {
        self.h.find(self.root, name)
    }

    fn num(&self, name: &str, prop: &str) -> f64 {
        self.h.num(self.find(name), prop)
    }

    fn text(&self, name: &str, prop: &str) -> String {
        self.h.get(self.find(name), prop).to_js_string().to_string()
    }

    fn call(&self, name: &str, method: &str) -> Value {
        self.h
            .engine
            .invoke_by_name(self.find(name), method, &[])
            .unwrap_or_else(|e| panic!("{method}: {e}"))
    }
}

/// `src` with one more signal on its root: an edit of no value, binding
/// or body, so the file is made again rather than edited in place.
fn remade(src: &str) -> String {
    src.replacen("{\n", "{\n  signal remade\n", 1)
}

const COUNTER: &str = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  width: 100
  Rectangle {
    id: box
    objectName: "box"
    property int taps: 0
    function tap() { taps += 1 }
    width: 10
  }
}
"#;

#[test]
fn an_object_with_an_id_keeps_what_a_script_assigned() {
    let mut r = rig(&[("Main", COUNTER)]);
    r.call("box", "tap");
    r.call("box", "tap");
    let before = r.find("box");
    // A signal added: the file is made again, not edited in place.
    let done = r
        .edit("Main", &remade(&COUNTER.replace("width: 10", "width: 30")))
        .unwrap();
    assert_eq!(done.generation, 1);
    assert_eq!(r.h.engine.reload_generation(), 1);
    assert_eq!((done.kept, done.fresh, done.remounted), (2, 0, 0));
    // The file's new word, and what the script did.
    assert_eq!(r.num("box", "width"), 30.0);
    assert_eq!(r.num("box", "taps"), 2.0);
    // Another object stands for it: the old one is gone.
    assert_ne!(r.find("box"), before);
    assert!(!r.h.engine.is_alive(before));
    assert!(!r.h.engine.is_alive(done.roots[0].0));
    // And it goes on counting from there.
    r.call("box", "tap");
    assert_eq!(r.num("box", "taps"), 3.0);
}

#[test]
fn another_id_is_another_object() {
    let mut r = rig(&[("Main", COUNTER)]);
    r.call("box", "tap");
    let done = r
        .edit("Main", &COUNTER.replace("id: box", "id: crate1"))
        .unwrap();
    assert_eq!(r.num("box", "taps"), 0.0);
    assert_eq!((done.kept, done.fresh), (1, 1));
}

#[test]
fn an_object_moved_under_a_new_parent_keeps_its_state() {
    let mut r = rig(&[("Main", COUNTER)]);
    r.call("box", "tap");
    let moved = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  width: 100
  Item {
    Rectangle {
      id: box
      objectName: "box"
      property int taps: 0
      function tap() { taps += 1 }
      width: 10
    }
  }
}
"#;
    let done = r.edit("Main", moved).unwrap();
    assert_eq!(r.num("box", "taps"), 1.0);
    // In place: the root stays, the new Item and the box are made, and
    // the box goes on with what the old one was given.
    assert_eq!(done.roots[0].0, done.roots[0].1);
    assert_eq!((done.kept, done.fresh, done.remounted), (1, 2, 0));
}

#[test]
fn another_type_at_the_same_place_is_made_anew_with_what_is_in_it() {
    let src = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  Rectangle {
    id: outer
    objectName: "outer"
    property int taps: 0
    Item {
      id: inner
      objectName: "inner"
      property int taps: 0
    }
  }
}
"#;
    let mut r = rig(&[("Main", src)]);
    r.h.set(r.find("outer"), "taps", Value::Int(4));
    r.h.set(r.find("inner"), "taps", Value::Int(5));
    let done = r
        .edit(
            "Main",
            &src.replace("Rectangle {\n    id: outer", "Item {\n    id: outer"),
        )
        .unwrap();
    assert_eq!(r.num("outer", "taps"), 0.0);
    assert_eq!(r.num("inner", "taps"), 0.0);
    // In place: the root stays, the two below it are made.
    assert_eq!((done.kept, done.fresh, done.remounted), (1, 2, 0));
}

#[test]
fn the_file_wins_where_its_declaration_was_edited() {
    let src = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property int count: 1
  property int other: 1
  width: count * 10
}
"#;
    let mut r = rig(&[("Main", src)]);
    r.h.set(r.root, "count", Value::Int(7));
    r.h.set(r.root, "other", Value::Int(8));
    // A script replaced the binding.
    r.h.set(r.root, "width", Value::Real(99.0));
    r.edit("Main", &src.replace("count: 1", "count: 40"))
        .unwrap();
    assert_eq!(r.num("root", "count"), 40.0);
    assert_eq!(r.num("root", "other"), 8.0);
    assert_eq!(r.num("root", "width"), 99.0);
    // Until the binding itself is edited.
    r.edit(
        "Main",
        &src.replace("count: 1", "count: 40")
            .replace("count * 10", "count * 2"),
    )
    .unwrap();
    assert_eq!(r.num("root", "width"), 80.0);
}

#[test]
fn a_kept_object_is_reloaded_and_not_completed_again() {
    let src = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property string events: ""
  Component.onCompleted: events += "completed "
  Component.onReloaded: events += "reloaded "
  Component.onDestruction: console.log("root destroyed")
  Item {
    id: gone
    Component.onDestruction: console.log("child destroyed")
  }
}
"#;
    let mut r = rig(&[("Main", src)]);
    assert_eq!(r.text("root", "events"), "completed ");
    let _ = r.h.log();
    let with_child = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property string events: ""
  Component.onCompleted: events += "completed "
  Component.onReloaded: events += "reloaded "
  Component.onDestruction: console.log("root destroyed")
  Item {
    id: fresh
    Component.onCompleted: root.events += "child "
    Component.onReloaded: root.events += "never "
  }
}
"#;
    r.edit("Main", with_child).unwrap();
    assert_eq!(r.text("root", "events"), "completed child reloaded ");
    // The object that went on was not told it is destroyed; the one the
    // file no longer has was.
    assert_eq!(r.h.log(), vec!["child destroyed".to_owned()]);
}

#[test]
fn the_replaced_object_of_a_kept_one_does_not_work_out_what_it_owes() {
    let src = |word: &str| {
        format!(
            r#"import QtQuick
Item {{
  id: root
  objectName: "root"
  property int n: 0
  property int seen: {{ console.log("{word} read " + n); return n }}
}}
"#
        )
    };
    let mut r = rig(&[("Main", &src("old"))]);
    assert_eq!(r.num("root", "seen"), 0.0);
    // Written between frames: nothing has read `seen` since.
    r.h.set(r.find("root"), "n", Value::from(3.0));
    let _ = r.h.log();
    r.edit("Main", &src("new")).unwrap();
    assert_eq!(r.num("root", "seen"), 3.0);
    assert!(
        !r.h.log().iter().any(|l| l.starts_with("old")),
        "the old tree evaluated a binding as it went"
    );
}

#[test]
fn a_patch_that_cannot_be_applied_changes_nothing() {
    let src = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property int taps: 0
  property string events: ""
  Component.onCompleted: events += "completed "
  Component.onDestruction: console.log("destroyed")
  onTapsChanged: events += "taps "
}
"#;
    let mut r = rig(&[("Main", src)]);
    r.h.set(r.root, "taps", Value::Int(3));
    let _ = r.h.log();
    let old = r.root;
    // Nothing gives the root its required property.
    let broken = src.replace(
        "property int taps: 0",
        "property int taps: 0\n  required property string label",
    );
    let err = r.edit("Main", &broken).unwrap_err();
    let ReloadError::Failed(fault) = err else {
        panic!("{err:?}");
    };
    assert_eq!(fault.file, "Main.qml");
    assert!(fault.line > 0, "{fault:?}");
    assert!(fault.message.contains("label"), "{fault:?}");
    assert_eq!(r.h.engine.last_reload_error(), Some(fault));
    assert_eq!(r.h.engine.reload_generation(), 0);
    // The old tree is the live one and heard nothing.
    assert_eq!(r.root, old);
    assert!(r.h.engine.is_alive(old));
    assert_eq!(r.num("root", "taps"), 3.0);
    assert_eq!(r.text("root", "events"), "completed taps ");
    assert_eq!(r.h.log(), Vec::<String>::new());

    // Mended: applied against the generation that is still live.
    let done = r
        .edit("Main", &src.replace("taps: 0", "taps: 0\n  width: 5"))
        .unwrap();
    assert_eq!(done.generation, 2);
    assert_eq!(r.h.engine.last_reload_error(), None);
    assert_eq!(r.num("root", "taps"), 3.0);
    assert_eq!(r.num("root", "width"), 5.0);
}

#[test]
fn a_value_edited_in_place_keeps_the_objects() {
    let mut r = rig(&[("Main", COUNTER)]);
    r.call("box", "tap");
    let (root, before) = (r.root, r.find("box"));
    let done = r
        .edit("Main", &COUNTER.replace("width: 10", "width: 30"))
        .unwrap();
    assert_eq!(done.roots, vec![(root, root)]);
    assert_eq!((done.kept, done.fresh, done.remounted), (2, 0, 0));
    assert_eq!(r.find("box"), before);
    assert_eq!(r.num("box", "width"), 30.0);
    assert_eq!(r.num("box", "taps"), 1.0);
    // And back, once more in place.
    r.edit("Main", COUNTER).unwrap();
    assert_eq!(r.find("box"), before);
    assert_eq!(r.num("box", "width"), 10.0);
}

const POKED: &str = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property int hits: 0
  signal poke
  Rectangle {
    id: box
    objectName: "box"
    width: 10
  }
  onPoke: hits += 1
  function tap() { poke() }
}
"#;

#[test]
fn handlers_edited_in_place_run_their_new_body_after_the_expressions_moved() {
    let mut r = rig(&[("Main", POKED)]);
    let (root, box_) = (r.root, r.find("box"));
    let edit = |r: &mut Rig, text: &str| {
        let done = r.edit("Main", text).unwrap();
        assert_eq!(done.roots, vec![(root, root)], "in place");
        assert_eq!(r.find("box"), box_);
    };
    // A value made a binding: every expression after it moves by one.
    let bound = POKED.replace("width: 10", "width: root.hits + 10");
    edit(&mut r, &bound);
    r.call("root", "tap");
    assert_eq!(r.num("root", "hits"), 1.0);
    assert_eq!(r.num("box", "width"), 11.0);
    edit(&mut r, &bound.replace("hits += 1", "hits += 5"));
    r.call("root", "tap");
    assert_eq!(r.num("root", "hits"), 6.0);
    assert_eq!(r.num("box", "width"), 16.0);
    // And back to a value: they move back.
    edit(&mut r, &POKED.replace("hits += 1", "hits += 2"));
    r.call("root", "tap");
    assert_eq!(r.num("root", "hits"), 8.0);
    assert_eq!(r.num("box", "width"), 10.0);
}

const PLACED: &str = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property var thing: ({ name: "a" })
  Item {
    id: label
    objectName: "label"
    property string text: root.thing.name
  }
}
"#;

#[test]
fn objects_and_bindings_kept_over_moved_lines_are_told_where_they_now_are() {
    let mut r = rig(&[("Main", PLACED)]);
    let (root, label) = (r.root, r.find("label"));
    let done = r
        .edit(
            "Main",
            &PLACED.replace("  property var", "  // A line more.\n  property var"),
        )
        .unwrap();
    assert_eq!(done.roots, vec![(root, root)], "in place");
    assert_eq!(r.find("label"), label);
    let o = r.h.engine.inspect_object(label).unwrap();
    assert_eq!(o.declared_at.map(|p| p.line), Some(7));
    assert_eq!(o.id.as_deref(), Some("label"));
    let text =
        r.h.engine
            .inspect_properties(label)
            .into_iter()
            .find(|p| p.name == "text")
            .unwrap();
    assert_eq!(text.written_at.map(|p| p.line), Some(10));
    // What the binding throws is told at its line too.
    r.h.set(root, "thing", Value::Null);
    r.h.get(label, "text");
    let said = r.h.warnings().join("\n");
    assert!(said.contains("Main.qml:10:"), "{said}");
    assert!(!said.contains("Main.qml:9:"), "{said}");
}

const CLOSURES: &str = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property var thing: ({ name: "a" })
  property string got
  property string bound
  signal poke
  function fire() { poke() }
  Component.onCompleted: {
    root.poke.connect(function () {
      root.got = root.thing.name
    })
    root.bound = Qt.binding(function () {
      return root.thing.name
    })
  }
}
"#;

#[test]
fn closures_made_before_lines_moved_throw_at_the_lines_they_are_now_on() {
    let mut r = rig(&[("Main", CLOSURES)]);
    let root = r.root;
    let done = r
        .edit(
            "Main",
            &CLOSURES.replace("  property var", "  // A line more.\n  property var"),
        )
        .unwrap();
    assert_eq!(done.roots, vec![(root, root)], "in place");
    r.h.warnings();
    r.h.set(root, "thing", Value::Null);
    r.h.get(root, "bound");
    r.call("root", "fire");
    let said = r.h.warnings().join("\n");
    assert!(
        said.contains("Main.qml:13:"),
        "the connect callback: {said}"
    );
    assert!(said.contains("Main.qml:16:"), "the binding: {said}");
    assert!(!said.contains("Main.qml:12:"), "{said}");
    assert!(!said.contains("Main.qml:15:"), "{said}");
}

#[test]
fn a_function_given_other_parameters_is_made_again() {
    let mut r = rig(&[("Main", COUNTER)]);
    let root = r.root;
    let done = r
        .edit("Main", &COUNTER.replace("taps += 1", "taps += 2"))
        .unwrap();
    assert_eq!(done.roots, vec![(root, root)], "a body alone, in place");
    r.call("box", "tap");
    assert_eq!(r.num("box", "taps"), 2.0);
    let done = r
        .edit(
            "Main",
            &COUNTER.replace(
                "function tap() { taps += 1 }",
                "function tap(n) { taps += 3 }",
            ),
        )
        .unwrap();
    assert_ne!(done.roots[0].0, done.roots[0].1, "made again");
    r.call("box", "tap");
    assert_eq!(r.num("box", "taps"), 5.0);
}

#[test]
fn a_patch_for_another_generation_is_refused() {
    let mut r = rig(&[("Main", COUNTER)]);
    r.session
        .set_file("Main", COUNTER.replace("width: 10", "width: 30"));
    let mut patch = r.session.build().unwrap().unwrap();
    patch.base = 7;
    let err =
        r.h.engine
            .reload(&patch, &[r.root], &mut |_, _, _| {})
            .unwrap_err();
    assert_eq!(
        err,
        ReloadError::Stale {
            patch: 7,
            engine: 0
        }
    );
    assert_eq!(r.num("box", "width"), 10.0);
}

#[test]
fn reload_needs_tracking_and_a_root_it_made() {
    let h = engine();
    let root = h.load(COUNTER);
    let mut session = ReloadSession::in_memory(QtTypeOracle);
    session.set_file("Main", COUNTER);
    session.start();
    session.set_file("Main", COUNTER.replace("width: 10", "width: 30"));
    let patch = session.build().unwrap().unwrap();
    let mut swap = |_: &Engine, _: ObjectId, _: ObjectId| {};
    assert_eq!(
        h.engine.reload(&patch, &[root], &mut swap),
        Err(ReloadError::NotTracking)
    );
    // Made before tracking was on: the engine does not know how.
    h.engine.set_reload_tracking(true);
    assert_eq!(
        h.engine.reload(&patch, &[root], &mut swap),
        Err(ReloadError::UnknownRoot(root))
    );
    assert!(h.engine.is_alive(root));
}

#[test]
fn the_host_swaps_the_roots_before_the_old_ones_go() {
    let mut r = rig(&[("Main", COUNTER)]);
    // A signal added, so the roots are made again.
    r.session
        .set_file("Main", remade(&COUNTER.replace("width: 10", "width: 30")));
    let patch = r.session.build().unwrap().unwrap();
    let old = r.root;
    let mut seen = Vec::new();
    let done =
        r.h.engine
            .reload(&patch, &[old], &mut |e, was, now| {
                seen.push((was, now, e.is_alive(was), e.is_alive(now)));
            })
            .unwrap();
    assert_eq!(seen, vec![(old, done.roots[0].1, true, true)]);
    // Until the host says the reload is through, the old object leads to
    // the new one.
    let old_box = r.h.engine.reload_counterpart(old);
    assert_eq!(old_box, Some(done.roots[0].1));
    r.h.engine.finish_reload();
    assert_eq!(r.h.engine.reload_counterpart(old), None);
}

#[test]
fn a_root_marked_to_be_made_again_follows_the_ones_the_host_gives() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let second = r#"import QtQuick
Item {
  objectName: "second"
  Rectangle { objectName: "card"; width: 10 }
}
"#;
    let mut r = rig(&[("Main", COUNTER), ("Second", second)]);
    let made = |r: &Rig| {
        r.h.engine
            .create_from_url("Second", None, Vec::new())
            .expect("Second.qml creates")
    };
    let window = made(&r);
    let told = Rc::new(RefCell::new(Vec::new()));
    let heard = Rc::clone(&told);
    r.h.engine
        .remake_on_reload(window, move |e, was, now| {
            heard
                .borrow_mut()
                .push((was, now, e.is_alive(was), e.is_alive(now)));
        })
        .unwrap();

    // A value: applied where it stands, and no one is told of a new root.
    let done = r
        .edit("Second", &second.replace("width: 10", "width: 20"))
        .unwrap();
    assert_eq!(done.roots.len(), 1, "only the roots the host gave");
    assert_eq!(r.h.num(r.h.find(window, "card"), "width"), 20.0);
    assert!(told.borrow().is_empty());

    // A signal added: the window's root is made again with the host's.
    let done = r
        .edit("Second", &remade(&second.replace("width: 10", "width: 30")))
        .unwrap();
    assert_eq!(done.roots.len(), 1, "only the roots the host gave");
    let (was, now, ..) = told.borrow()[0];
    assert_eq!(*told.borrow(), vec![(window, now, true, true)]);
    assert!(!r.h.engine.is_alive(was));
    assert_eq!(r.h.num(r.h.find(now, "card"), "width"), 30.0);

    // The mark went with it: the signal taken away again.
    r.edit("Second", second).unwrap();
    assert_eq!(told.borrow().len(), 2);
    assert_eq!(told.borrow()[1].0, now);
    let now = told.borrow()[1].1;

    // Destroyed, it is let go of.
    r.h.engine.destroy(now);
    r.edit("Second", &remade(second)).unwrap();
    assert_eq!(told.borrow().len(), 2);

    // Not one a reload can make: refused while tracking.
    let plain = r.find("box");
    assert_eq!(
        r.h.engine.remake_on_reload(plain, |_, _, _| {}),
        Err(ReloadError::UnknownRoot(plain))
    );
}

#[test]
fn a_marked_root_the_host_gives_hears_of_its_new_root_once() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let mut r = rig(&[("Main", COUNTER)]);
    let first = r.root;
    let told = Rc::new(RefCell::new(Vec::new()));
    let heard = Rc::clone(&told);
    r.h.engine
        .remake_on_reload(first, move |_, was, now| {
            heard.borrow_mut().push((was, now))
        })
        .unwrap();
    let reload = |r: &mut Rig, source: &str, given: &[ObjectId]| {
        r.session.set_file("Main", source);
        let patch = r
            .session
            .build()
            .unwrap()
            .expect("the edit changes something");
        let mut swapped = Vec::new();
        let done =
            r.h.engine
                .reload(&patch, given, &mut |_, was, now| swapped.push((was, now)))
                .unwrap();
        r.session.ack(patch.generation);
        r.h.engine.finish_reload();
        (done, swapped)
    };

    // Given by the host too: the host's swap hears of it, and only it.
    let (done, swapped) = reload(&mut r, &remade(COUNTER), &[first]);
    let now = done.roots[0].1;
    assert_eq!(swapped, vec![(first, now)]);
    assert!(told.borrow().is_empty(), "replaced is not called as well");

    // The mark moved with it: once the host no longer gives it, the
    // application hears of the next one.
    let (done, swapped) = reload(&mut r, COUNTER, &[]);
    assert!(done.roots.is_empty() && swapped.is_empty());
    assert_eq!(told.borrow().len(), 1);
    assert_eq!(told.borrow()[0].0, now);
}

#[test]
fn a_host_that_marks_the_new_root_in_its_swap_is_the_one_heard_next() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let mut r = rig(&[("Main", COUNTER)]);
    let first = r.root;
    let told = Rc::new(RefCell::new(Vec::new()));
    let heard = Rc::clone(&told);
    r.h.engine
        .remake_on_reload(first, move |_, _, _| heard.borrow_mut().push("first mark"))
        .unwrap();
    r.session.set_file("Main", remade(COUNTER));
    let patch = r.session.build().unwrap().unwrap();
    let marks = Rc::clone(&told);
    let done =
        r.h.engine
            .reload(&patch, &[first], &mut |e, _, now| {
                let heard = Rc::clone(&marks);
                e.remake_on_reload(now, move |_, _, _| heard.borrow_mut().push("its own"))
                    .unwrap();
            })
            .unwrap();
    r.session.ack(patch.generation);
    r.h.engine.finish_reload();
    r.root = done.roots[0].1;

    // The host no longer gives it: what it marked the new root with hears.
    r.session.set_file("Main", COUNTER);
    let patch = r.session.build().unwrap().unwrap();
    r.h.engine.reload(&patch, &[], &mut |_, _, _| {}).unwrap();
    assert_eq!(*told.borrow(), vec!["its own"]);
}

#[test]
fn a_marked_root_with_an_owner_is_made_again_under_what_goes_on_for_it() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let second = r#"import QtQuick
Item {
  objectName: "second"
}
"#;
    let mut r = rig(&[("Main", COUNTER), ("Second", second)]);
    let told = Rc::new(RefCell::new(Vec::new()));
    let mark = |owner: ObjectId| {
        let w =
            r.h.engine
                .create_from_url("Second", Some(owner), Vec::new())
                .expect("Second.qml creates");
        let heard = Rc::clone(&told);
        r.h.engine
            .remake_on_reload(w, move |_, was, now| heard.borrow_mut().push((was, now)))
            .unwrap();
        w
    };
    let by_root = mark(r.root);
    let by_box = mark(r.find("box"));

    let done = r.edit("Second", &remade(second)).unwrap();
    let new_root = done.roots[0].1;
    let now = told
        .borrow()
        .iter()
        .find(|(was, _)| *was == by_root)
        .unwrap()
        .1;
    assert!(
        r.h.engine.is_alive(now),
        "it outlives the old root that owned it"
    );
    assert_eq!(r.h.engine.owner(now), Some(new_root));
    let in_box = told
        .borrow()
        .iter()
        .find(|(was, _)| *was == by_box)
        .unwrap()
        .1;
    assert_eq!(r.h.engine.owner(in_box), Some(r.find("box")));

    // Its owner gone from the file, it goes with it.
    let no_box = COUNTER.replace(
        "  Rectangle {\n    id: box\n    objectName: \"box\"\n    property int taps: 0\n    function tap() { taps += 1 }\n    width: 10\n  }\n",
        "",
    );
    assert_ne!(no_box, COUNTER);
    told.borrow_mut().clear();
    r.edit("Main", &remade(&no_box)).unwrap();
    assert!(!r.h.engine.is_alive(in_box));
    assert_eq!(told.borrow().len(), 1, "only the one whose owner goes on");
    let now = told.borrow()[0].1;
    assert_eq!(r.h.engine.owner(now), Some(r.root));
}

#[test]
fn a_value_that_names_a_root_made_later_is_given_once_it_stands() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let holds = |name: &str| {
        format!(
            "import QtQuick\nItem {{\n  objectName: \"{name}\"\n  property var other: null\n  function hold(o) {{ other = o }}\n}}\n"
        )
    };
    let (main, second) = (holds("main"), holds("second"));
    let mut r = rig(&[("Main", main.as_str()), ("Second", second.as_str())]);
    let window =
        r.h.engine
            .create_from_url("Second", None, Vec::new())
            .expect("Second.qml creates");
    let told = Rc::new(RefCell::new(Vec::new()));
    let heard = Rc::clone(&told);
    r.h.engine
        .remake_on_reload(window, move |_, _, now| heard.borrow_mut().push(now))
        .unwrap();
    let hold = |r: &Rig, on: ObjectId, what: ObjectId| {
        r.h.engine
            .invoke_by_name(on, "hold", &[Value::Object(what)])
            .unwrap();
    };
    hold(&r, r.root, window);
    hold(&r, window, r.root);

    r.edit("Second", &remade(&second)).unwrap();
    let now = told.borrow()[0];
    // Each holds the other's new root: the one made first as well.
    assert_eq!(r.h.get(r.root, "other").as_object(), Some(now));
    assert_eq!(r.h.get(now, "other").as_object(), Some(r.root));
}

const HOLDER: &str =
    "import QtQuick\nItem {\n  objectName: \"second\"\n  property var other: null\n}\n";

/// What each marked root was replaced by.
type Told = std::rc::Rc<std::cell::RefCell<std::collections::HashMap<ObjectId, ObjectId>>>;

/// Marks `roots` in that order; what each is replaced by lands in the map.
fn mark_all(r: &Rig, roots: &[ObjectId]) -> Told {
    let told = Told::default();
    for w in roots {
        let heard = std::rc::Rc::clone(&told);
        r.h.engine
            .remake_on_reload(*w, move |_, was, now| {
                heard.borrow_mut().insert(was, now);
            })
            .unwrap();
    }
    told
}

#[test]
fn a_marked_root_is_made_after_the_roots_its_initial_properties_name() {
    let mut r = rig(&[("Main", COUNTER), ("Second", HOLDER)]);
    let first =
        r.h.engine
            .create_from_url("Second", None, Vec::new())
            .unwrap();
    let next =
        r.h.engine
            .create_from_url("Second", None, vec![("other".into(), Value::Object(first))])
            .unwrap();
    // Marked the other way round from how they were made.
    let told = mark_all(&r, &[next, first]);

    r.edit("Second", &remade(HOLDER)).unwrap();
    let (first, next) = (told.borrow()[&first], told.borrow()[&next]);
    assert_eq!(r.h.get(next, "other").as_object(), Some(first));
}

#[test]
fn a_marked_root_is_made_after_the_root_its_scope_is_of() {
    let holder = |tag: &str| {
        format!(
            "import QtQuick\nItem {{\n  objectName: \"holder\"\n  property string tag: \"{tag}\"\n}}\n"
        )
    };
    let scoped =
        "import QtQuick\nItem {\n  objectName: \"scoped\"\n  property string seen: tag\n}\n";
    let first = holder("first");
    let mut r = rig(&[
        ("Main", COUNTER),
        ("Holder", first.as_str()),
        ("Scoped", scoped),
    ]);
    let held =
        r.h.engine
            .create_from_url("Holder", None, Vec::new())
            .unwrap();
    let (inside, _) =
        r.h.engine
            .make(Request::url("Scoped").in_scope(held))
            .unwrap();
    assert_eq!(r.h.get(inside, "seen").to_js_string().as_str(), "first");
    let told = mark_all(&r, &[inside, held]);

    r.edit("Holder", &remade(&holder("second"))).unwrap();
    let now = told
        .borrow()
        .get(&inside)
        .copied()
        .expect("made again, in the scope that goes on for its own");
    assert_eq!(r.h.get(now, "seen").to_js_string().as_str(), "second");
}

#[test]
fn an_initial_property_that_names_a_root_made_later_is_given_once_it_stands() {
    let mut r = rig(&[("Main", COUNTER), ("Second", HOLDER)]);
    let make = |r: &Rig, props: Vec<(String, Value)>| {
        r.h.engine.create_from_url("Second", None, props).unwrap()
    };
    let lone = make(&r, Vec::new());
    let a = make(&r, vec![("other".into(), Value::Object(lone))]);
    let b = make(&r, vec![("other".into(), Value::Object(a))]);
    // Moved under the root made last, the one `a` names closes a circle:
    // `a` is made with an object of `b`, and `b` with `a`.
    r.h.engine.set_owner(lone, Some(b)).unwrap();
    let told = mark_all(&r, &[a, b, lone]);

    r.edit("Second", &remade(HOLDER)).unwrap();
    let now = |old: ObjectId| told.borrow()[&old];
    assert_eq!(r.h.get(now(a), "other").as_object(), Some(now(lone)));
    assert_eq!(r.h.get(now(b), "other").as_object(), Some(now(a)));

    // Given late, it is still on record for the next reload.
    let (a, lone) = (now(a), now(lone));
    r.edit("Second", HOLDER).unwrap();
    assert_eq!(r.h.get(now(a), "other").as_object(), Some(now(lone)));
}

#[test]
fn a_value_that_names_what_a_script_made_under_a_later_root_is_given() {
    let main = "import QtQuick\nItem {\n  objectName: \"main\"\n  property var other: null\n  function hold(o) { other = o }\n}\n";
    let second = r#"import QtQuick
Item {
  id: root
  objectName: "second"
  function fill() { return tile.createObject(root) }
  Component {
    id: tile
    Item { objectName: "tile" }
  }
}
"#;
    let mut r = rig(&[("Main", main), ("Second", second)]);
    let window =
        r.h.engine
            .create_from_url("Second", None, Vec::new())
            .unwrap();
    let made =
        r.h.engine
            .invoke_by_name(window, "fill", &[])
            .unwrap()
            .as_object()
            .expect("the tile is made");
    r.h.engine
        .invoke_by_name(r.root, "hold", &[Value::Object(made)])
        .unwrap();
    let told = mark_all(&r, &[window]);

    r.edit("Second", &remade(second)).unwrap();
    let now = told.borrow()[&window];
    let tile = r.h.find(now, "tile");
    assert_ne!(tile, made);
    assert_eq!(r.h.get(r.root, "other").as_object(), Some(tile));
}

const MODEL: &str = r#"import QtQuick
import QtQml.Models
Item {
  id: root
  objectName: "root"
  width: 1
  function names() {
    var s = ""
    for (var i = 0; i < lm.count; i++) s += lm.get(i).name + " "
    return s
  }
  function add(name) { lm.append({ name: name, tags: [{ tag: "new" }] }) }
  function tag(row) { return lm.get(row).tags.get(0).tag }
  ListModel {
    id: lm
    objectName: "lm"
    ListElement { name: "Anna" }
  }
}
"#;

#[test]
fn a_list_model_keeps_its_rows_until_its_elements_are_edited() {
    let mut r = rig(&[("Main", MODEL)]);
    r.h.engine
        .invoke_by_name(r.root, "add", &[Value::string("Bjarni")])
        .unwrap();
    assert_eq!(
        r.call("root", "names").to_js_string().as_str(),
        "Anna Bjarni "
    );
    r.edit("Main", &MODEL.replace("width: 1", "width: 2"))
        .unwrap();
    assert_eq!(r.num("lm", "count"), 2.0);
    assert_eq!(
        r.call("root", "names").to_js_string().as_str(),
        "Anna Bjarni "
    );
    // A model in a role came along.
    let tag =
        r.h.engine
            .invoke_by_name(r.root, "tag", &[Value::Int(1)])
            .unwrap();
    assert_eq!(tag.to_js_string().as_str(), "new");
    // The developer edits the rows: now the file is right.
    let edited = MODEL.replace(
        "ListElement { name: \"Anna\" }",
        "ListElement { name: \"Anna\" }\n    ListElement { name: \"Dagur\" }",
    );
    r.edit("Main", &edited).unwrap();
    assert_eq!(
        r.call("root", "names").to_js_string().as_str(),
        "Anna Dagur "
    );
}

const TIMER: &str = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property int ticks: 0
  function stop() { timer.running = false }
  Timer {
    id: timer
    objectName: "timer"
    interval: 100
    repeat: true
    running: true
    onTriggered: root.ticks += 1
  }
}
"#;

#[test]
fn a_timer_goes_on_running_with_the_new_handler() {
    let mut r = rig(&[("Main", TIMER)]);
    r.h.pass_ms(250);
    assert_eq!(r.num("root", "ticks"), 2.0);
    r.h.set(r.root, "ticks", Value::Int(2));
    r.edit("Main", &TIMER.replace("ticks += 1", "ticks += 10"))
        .unwrap();
    r.h.pass_ms(100);
    // One tick of the new handler; the old timer is gone and adds none.
    assert_eq!(r.num("root", "ticks"), 12.0);
}

#[test]
fn a_timer_a_script_stopped_stays_stopped() {
    let mut r = rig(&[("Main", TIMER)]);
    r.h.pass_ms(250);
    r.call("root", "stop");
    r.edit("Main", &TIMER.replace("ticks += 1", "ticks += 10"))
        .unwrap();
    r.h.pass_ms(300);
    assert_eq!(r.num("root", "ticks"), 2.0);
    assert_eq!(r.h.get(r.find("timer"), "running"), Value::Bool(false));
}

const USES_COUNTER: &str = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  Counter { id: one; objectName: "one" }
  Counter { id: two; objectName: "two" }
}
"#;

const COUNTER_FILE: &str = r#"import QtQuick
Item {
  id: self
  property int count: 0
  property string label: "none"
  property int inner: mark.hits
  function bump() { count += 1; mark.hits += 2; return 1 }
  Item {
    id: mark
    property int hits: 0
  }
}
"#;

#[test]
fn a_component_edited_reloads_every_use_of_it() {
    let mut r = rig(&[("Main", USES_COUNTER), ("Counter", COUNTER_FILE)]);
    r.call("one", "bump");
    r.call("two", "bump");
    r.call("two", "bump");
    r.h.set(r.find("one"), "label", Value::string("first"));
    // `label` becomes a number, `extra` is new, and `bump` counts more.
    let edited = COUNTER_FILE
        .replace(
            "property string label: \"none\"",
            "property real label: 1.5",
        )
        .replace(
            "property int inner",
            "property bool extra: true\n  property int inner",
        )
        .replace("count += 1;", "count += 10;");
    let done = r.edit("Counter", &edited).unwrap();
    assert_eq!(done.remounted, 0);
    assert_eq!(r.num("one", "count"), 1.0);
    assert_eq!(r.num("two", "count"), 2.0);
    // What is inside each use went on as well, each with its own.
    assert_eq!(r.num("one", "inner"), 2.0);
    assert_eq!(r.num("two", "inner"), 4.0);
    assert_eq!(r.num("one", "label"), 1.5);
    assert_eq!(r.h.get(r.find("one"), "extra"), Value::Bool(true));
    r.call("one", "bump");
    assert_eq!(r.num("one", "count"), 11.0);
}

#[test]
fn the_script_of_a_file_that_did_not_change_still_runs() {
    let mut r = rig(&[("Main", USES_COUNTER), ("Counter", COUNTER_FILE)]);
    r.call("one", "bump");
    r.edit(
        "Main",
        &USES_COUNTER.replace("Counter { id: two", "Counter { width: 3; id: two"),
    )
    .unwrap();
    r.call("one", "bump");
    assert_eq!(r.num("one", "count"), 2.0);
    assert_eq!(r.num("two", "width"), 3.0);
    // And twice over, as each reload names the scripts anew.
    r.edit(
        "Main",
        &USES_COUNTER.replace("Counter { id: two", "Counter { width: 4; id: two"),
    )
    .unwrap();
    r.call("one", "bump");
    assert_eq!(r.num("one", "count"), 3.0);
    assert_eq!(r.h.warnings(), Vec::<String>::new());
}

#[test]
fn a_file_that_asks_for_a_reset_is_made_anew() {
    let counter = format!("pragma LjosReload: reset\n{COUNTER_FILE}");
    let main = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property int taps: 0
  Counter { id: one; objectName: "one" }
}
"#;
    let mut r = rig(&[("Main", main), ("Counter", &counter)]);
    r.call("one", "bump");
    r.h.set(r.root, "taps", Value::Int(5));
    let done = r
        .edit(
            "Main",
            &remade(&main.replace("taps: 0", "taps: 0\n  width: 2")),
        )
        .unwrap();
    assert_eq!(r.num("one", "count"), 0.0);
    assert_eq!(r.num("root", "taps"), 5.0);
    assert_eq!(done.kept, 1);
    assert!(done.remounted >= 1);
}

const ROWS: &str = r#"import QtQuick
import QtQml.Models
Item {
  id: root
  objectName: "root"
  function mark(i, n) { inst.objectAt(i).taps = n }
  function taps() {
    var s = ""
    for (var i = 0; i < inst.count; i++) s += inst.objectAt(i).taps + ":" + inst.objectAt(i).size + " "
    return s
  }
  Instantiator {
    id: inst
    model: 3
    QtObject {
      property int taps: 0
      property int size: 10
    }
  }
}
"#;

#[test]
fn a_delegate_goes_on_by_its_row() {
    let mut r = rig(&[("Main", ROWS)]);
    for (row, n) in [(0, 1), (1, 4), (2, 9)] {
        r.h.engine
            .invoke_by_name(r.root, "mark", &[Value::Int(row), Value::Int(n)])
            .unwrap();
    }
    r.edit("Main", &ROWS.replace("size: 10", "size: 20"))
        .unwrap();
    assert_eq!(
        r.call("root", "taps").to_js_string().as_str(),
        "1:20 4:20 9:20 "
    );
}

// A delegate with required properties has no `index` in sight: its row
// is the view's to say.
const REQUIRED_ROWS: &str = r#"import QtQuick
import QtQml.Models
Item {
  id: root
  objectName: "root"
  function mark(i, n) { inst.objectAt(i).taps = n }
  function taps() {
    var s = ""
    for (var i = 0; i < inst.count; i++) s += inst.objectAt(i).name + inst.objectAt(i).taps + ":" + inst.objectAt(i).size + " "
    return s
  }
  ListModel {
    id: rows
    ListElement { name: "a" }
    ListElement { name: "b" }
    ListElement { name: "c" }
  }
  Instantiator {
    id: inst
    model: rows
    QtObject {
      required property string name
      property int taps: 0
      property int size: 10
      property QtObject part: QtObject { }
    }
  }
}
"#;

#[test]
fn a_delegate_with_required_properties_goes_on_by_its_row() {
    let mut r = rig(&[("Main", REQUIRED_ROWS)]);
    for (row, n) in [(0, 1), (1, 4), (2, 9)] {
        r.h.engine
            .invoke_by_name(r.root, "mark", &[Value::Int(row), Value::Int(n)])
            .unwrap();
    }
    let done = r
        .edit("Main", &REQUIRED_ROWS.replace("size: 10", "size: 20"))
        .unwrap();
    assert_eq!(
        r.call("root", "taps").to_js_string().as_str(),
        "a1:20 b4:20 c9:20 "
    );
    assert_eq!(done.fresh, 0);
}

// The rows are made again as the file completes and as it is reloaded,
// while the first ones are still on their way out (as a view does whose
// parent changes).
const REMADE_ROWS: &str = r#"import QtQuick
import QtQml.Models
Item {
  id: root
  objectName: "root"
  property int rows: 2
  Component.onCompleted: rows = 3
  Component.onReloaded: { rows = 0; rows = 3 }
  function mark(i, n) { inst.objectAt(i).taps = n }
  function taps() {
    var s = ""
    for (var i = 0; i < inst.count; i++) s += inst.objectAt(i).taps + ":" + inst.objectAt(i).size + " "
    return s
  }
  Instantiator {
    id: inst
    model: root.rows
    QtObject {
      property int taps: 0
      property int size: 10
    }
  }
}
"#;

#[test]
fn rows_made_again_while_the_first_ones_go_are_carried() {
    let mut r = rig(&[("Main", REMADE_ROWS)]);
    for (row, n) in [(0, 1), (1, 4), (2, 9)] {
        r.h.engine
            .invoke_by_name(r.root, "mark", &[Value::Int(row), Value::Int(n)])
            .unwrap();
    }
    let done = r
        // A signal added, so the rows are made again on the full path.
        .edit(
            "Main",
            &remade(&REMADE_ROWS.replace("size: 10", "size: 20")),
        )
        .unwrap();
    assert_eq!(
        r.call("root", "taps").to_js_string().as_str(),
        "1:20 4:20 9:20 "
    );
    assert_eq!(done.fresh, 0);
}

#[test]
fn every_object_made_from_qml_has_a_path_that_outlives_it() {
    let mut r = rig(&[("Main", USES_COUNTER), ("Counter", COUNTER_FILE)]);
    let e = &r.h.engine;
    let root = e.reload_path(r.root).unwrap();
    assert_eq!(e.reload_path(r.find("one")), Some(format!("{root}#one")));
    let mark = e.children(r.find("two"))[0];
    assert_eq!(
        e.reload_path(mark),
        Some(format!("{root}#two>Counter|Counter#mark"))
    );
    let was = r.find("two");
    r.edit(
        "Main",
        &remade(&USES_COUNTER.replace("Counter { id: two", "Counter { width: 3; id: two")),
    )
    .unwrap();
    let e = &r.h.engine;
    assert_eq!(e.reload_path(r.find("two")), Some(format!("{root}#two")));
    assert_eq!(e.reload_path(was), None);
}

/// Goes by the name of a Qt type, as the compiler's oracle knows only
/// those; nothing of `Text` is meant.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Text", extends = QtObject)]
pub struct Tally {
    #[qml(base)]
    object: QtObject,
    #[qml(notify)]
    width: f64,
    #[qml(skip)]
    marks: Vec<u32>,
}

impl Migrate for Tally {
    const VERSION: u32 = 3;
    type State = Vec<u32>;

    fn snapshot(engine: &Engine, obj: ObjectId) -> Option<Vec<u32>> {
        engine.with::<Tally, _>(obj, |t| t.marks.clone()).ok()
    }

    fn migrate(engine: &Engine, obj: ObjectId, from: u32, marks: Vec<u32>) -> Result<(), String> {
        assert_eq!(from, 3);
        if marks.contains(&13) {
            return Err("thirteen".into());
        }
        engine
            .with_mut::<Tally, _>(obj, |t| t.marks = marks)
            .map_err(|e| e.to_string())
    }
}

#[test]
fn a_native_type_carries_its_own_state() {
    let h = engine();
    h.engine
        .register_module(ljos_engine::module!("QtQuick" 2.0 => [Tally]));
    h.engine.register_migrate::<Tally>();
    let src = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  Text { id: tally; objectName: "tally"; width: 1 }
}
"#;
    let mut r = rig_on(h, &[("Main", src)]);
    let marks = |r: &Rig| {
        r.h.engine
            .with::<Tally, _>(r.find("tally"), |t| t.marks.clone())
            .unwrap()
    };
    r.h.engine
        .with_mut::<Tally, _>(r.find("tally"), |t| t.marks = vec![1, 2])
        .unwrap();
    // A signal added, then taken away: each time the objects are made
    // again and the state migrates.
    r.edit("Main", &remade(&src.replace("width: 1", "width: 2")))
        .unwrap();
    assert_eq!(marks(&r), vec![1, 2]);
    assert_eq!(r.num("tally", "width"), 2.0);

    // State that cannot be carried: the object starts over, and says so.
    r.h.engine
        .with_mut::<Tally, _>(r.find("tally"), |t| t.marks = vec![13])
        .unwrap();
    let _ = r.h.log();
    let done = r
        .edit("Main", &src.replace("width: 1", "width: 3"))
        .unwrap();
    assert_eq!(marks(&r), Vec::<u32>::new());
    assert_eq!((done.kept, done.fresh), (1, 1));
    let warnings = r.h.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("thirteen"), "{warnings:?}");
}

#[test]
fn a_registered_property_is_carried_where_the_file_binds_nothing() {
    let h = engine();
    h.engine.register_reload_state("Rectangle", "radius");
    let src = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property real given: 2
  Rectangle { id: a; objectName: "a"; width: 1 }
  Rectangle { id: b; objectName: "b"; width: 1 }
}
"#;
    let mut r = rig_on(h, &[("Main", src)]);
    // Native code moved both: not a script's assignment.
    for name in ["a", "b"] {
        r.h.engine
            .set(r.find(name), support::Rectangle::RADIUS, 6.0)
            .unwrap();
    }
    r.edit(
        "Main",
        &src.replace(
            "objectName: \"b\"; width: 1",
            "objectName: \"b\"; radius: root.given",
        ),
    )
    .unwrap();
    assert_eq!(r.num("a", "radius"), 6.0);
    assert_eq!(r.num("b", "radius"), 2.0);
}

#[test]
fn files_come_and_go_with_a_patch() {
    let main = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property int taps: 0
}
"#;
    let mut r = rig(&[("Main", main)]);
    r.h.set(r.root, "taps", Value::Int(2));
    // A file that was not there, used at once.
    r.session
        .set_file("Badge", "import QtQuick\nItem { width: 7 }\n");
    r.edit(
        "Main",
        &main.replace("taps: 0", "taps: 0\n  Badge { objectName: \"badge\" }"),
    )
    .unwrap();
    assert_eq!(r.num("badge", "width"), 7.0);
    assert_eq!(r.num("root", "taps"), 2.0);
    // And gone again, with its use.
    r.session.remove_file("Badge");
    r.edit("Main", main).unwrap();
    assert_eq!(support::find(&r.h.engine, r.root, "badge"), None);
    assert_eq!(r.num("root", "taps"), 2.0);
    assert!(
        r.h.engine
            .create_from_url("Badge", None, Vec::new())
            .is_err(),
        "the removed file is gone for the engine too"
    );
}

#[test]
fn what_stands_in_follows_a_reload() {
    // `stood_in_modules` is what the design audit reads: a reload that
    // takes the last stand-in out drops its module, one that puts it back
    // lists it again.
    let gizmo = r#"import QtQuick
import QtQuick3D.Helpers
Item {
  id: root
  objectName: "root"
  SceneGizmo { }
}
"#;
    let without = gizmo.replace("  SceneGizmo { }\n", "");
    let mut r = rig(&[("Main", gizmo)]);
    assert_eq!(r.h.engine.stood_in_modules(), ["QtQuick3D.Helpers"]);
    r.edit("Main", &without).unwrap();
    assert!(r.h.engine.stood_in_modules().is_empty());
    r.edit("Main", gizmo).unwrap();
    assert_eq!(r.h.engine.stood_in_modules(), ["QtQuick3D.Helpers"]);
}

#[test]
fn a_failed_reload_takes_back_what_it_stood_in_for() {
    // The attempt loads a file that uses a stand-in, then fails: the old
    // tree goes on, and so does what the audit read before.
    let main = r#"import QtQuick
Item {
  id: root
  objectName: "root"
}
"#;
    let mut r = rig(&[("Main", main)]);
    assert!(r.h.engine.stood_in_modules().is_empty());
    // Nothing gives the root its required property.
    let broken = main
        .replace(
            "import QtQuick\n",
            "import QtQuick\nimport QtQuick3D.Helpers\n",
        )
        .replace(
            "  objectName: \"root\"\n",
            "  objectName: \"root\"\n  required property string label\n  SceneGizmo { }\n",
        );
    assert!(matches!(
        r.edit("Main", &broken),
        Err(ReloadError::Failed(_))
    ));
    assert!(r.h.engine.stood_in_modules().is_empty());
}

#[test]
fn a_library_type_that_is_a_stand_in_is_counted_while_it_stands() {
    // The library file's only object is its root, which carries the
    // location of the declaration that uses it.
    let h = engine();
    let mut project = ljos_compiler::Project::new(QtTypeOracle);
    project.add(
        "Lib/Gizmo",
        "import QtQuick\nimport QtQuick3D.Helpers\nSceneGizmo { }\n",
    );
    project.add(
        "Main",
        "import QtQuick\nimport Kit\nItem {\n  Gizmo { objectName: \"gizmo\" }\n}\n",
    );
    project.add_type("Kit", "Gizmo", "Lib/Gizmo");
    let compiled = |key: &str| {
        let c = ljos_compiler::compile_file(&project, key).unwrap_or_else(|d| panic!("{d:?}"));
        assert!(!c.has_errors(), "{key}: {:?}", c.diagnostics);
        c.file
    };
    let mut library = MapSource::new();
    library.insert("Lib/Gizmo", compiled("Lib/Gizmo"));
    let mut app = MapSource::new();
    app.insert("Main", compiled("Main"));
    h.engine.add_component_library(library);
    h.engine.set_component_source(app);
    let root = h.engine.create_from_url("Main", None, Vec::new()).unwrap();
    assert_eq!(h.engine.stood_in_modules(), ["QtQuick3D.Helpers"]);
    let gizmo = h.find(root, "gizmo");
    assert!(h.engine.destroy(gizmo));
    h.engine.process_deferred_deletes();
    assert!(h.engine.stood_in_modules().is_empty());
}

#[test]
fn reloads_leave_no_script_types_behind() {
    let mut r = rig(&[("Main", USES_COUNTER), ("Counter", COUNTER_FILE)]);
    // The file that is edited has a script of its own, which a value
    // edited on its own line leaves as it was.
    let edit = |r: &mut Rig, width: u32| {
        r.edit(
            "Counter",
            &COUNTER_FILE.replace("label: \"none\"", &format!("label: \"{width}\"")),
        )
        .unwrap();
        r.call("one", "bump");
    };
    edit(&mut r, 1);
    assert!(r.h.engine.collect_garbage());
    let before = r.h.engine.script_memory().expect("scripts ran");
    for width in 2..22 {
        edit(&mut r, width);
    }
    assert!(r.h.engine.collect_garbage());
    let after = r.h.engine.script_memory().expect("scripts ran");
    assert_eq!(before, after);
    assert_eq!(r.num("one", "count"), 21.0);
}

#[test]
fn reloads_forget_the_replaced_objects_no_script_holds() {
    let mut r = rig(&[("Main", COUNTER)]);
    // A signal added and taken away: the file is made again each time.
    let edit = |r: &mut Rig, i: u32| {
        let text = if i.is_multiple_of(2) {
            remade(&COUNTER.replace("width: 10", "width: 30"))
        } else {
            COUNTER.to_string()
        };
        r.edit("Main", &text).unwrap();
        r.call("box", "tap");
        assert!(r.h.engine.collect_garbage());
    };
    edit(&mut r, 0);
    edit(&mut r, 1);
    let before = r.h.engine.reload_forward_count();
    for i in 2..12 {
        edit(&mut r, i);
    }
    assert_eq!(r.h.engine.reload_forward_count(), before);
    assert_eq!(r.num("box", "taps"), 12.0);
}

const WITH_TEMPLATES: &str = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  width: 10
  Component { id: chip; objectName: "chip"; Item {} }
  ListModel {
    id: rows
    ListElement { name: "a" }
    ListElement { name: "b" }
  }
  function spend() { rows.clear() }
}
"#;

#[test]
fn a_component_and_list_elements_made_again_are_not_new() {
    let mut r = rig(&[("Main", WITH_TEMPLATES)]);
    // What they were made for is done (a control made its part, a script
    // emptied the model), and they are gone.
    r.h.engine.destroy_now(r.find("chip"));
    r.call("root", "spend");
    assert!(r.h.engine.collect_garbage());
    let done = r
        .edit(
            "Main",
            &remade(&WITH_TEMPLATES.replace("width: 10", "width: 30")),
        )
        .unwrap();
    assert_ne!(done.roots[0].0, done.roots[0].1, "made again");
    assert_eq!(done.fresh, 0);
}

const NESTED: &str = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  ListModel {
    id: lm
    objectName: "lm"
    ListElement { name: "a"; days: [ ListElement { d: 0 }, ListElement { d: 1 } ] }
    ListElement { name: "b"; days: [ ListElement { d: 2 } ] }
  }
}
"#;

#[test]
fn a_row_removed_from_the_file_takes_its_nested_model_with_it() {
    let mut r = rig(&[("Main", NESTED)]);
    let lists = |r: &Rig| {
        let lm = r.find("lm");
        r.h.engine.process_deferred_deletes();
        r.h.engine
            .children(lm)
            .into_iter()
            .filter(|c| r.h.engine.is_alive(*c))
            .count()
    };
    assert_eq!(lists(&r), 2);
    let edited = NESTED.replace(
        "ListElement { name: \"a\"; days: [ ListElement { d: 0 }, ListElement { d: 1 } ] }\n",
        "",
    );
    r.edit("Main", &edited).unwrap();
    assert_eq!(r.num("lm", "count"), 1.0);
    assert_eq!(lists(&r), 1);
}

const MODEL_FILE: &str = r#"import QtQuick
ListModel {
  ListElement { name: "a"; days: [ ListElement { d: 0 }, ListElement { d: 1 } ] }
  ListElement { name: "b"; days: [ ListElement { d: 2 } ] }
}
"#;

#[test]
fn a_row_removed_from_a_model_file_takes_its_nested_model_with_it() {
    let main = "import QtQuick\nItem {\n  objectName: \"root\"\n  Rows { objectName: \"lm\" }\n  Other {}\n}\n";
    let other = "import QtQuick\nItem {\n  property int a: 1\n}\n";
    let mut r = rig(&[("Main", main), ("Rows", MODEL_FILE), ("Other", other)]);
    let lists = |r: &Rig| {
        let lm = r.find("lm");
        r.h.engine.process_deferred_deletes();
        r.h.engine
            .children(lm)
            .into_iter()
            .filter(|c| r.h.engine.is_alive(*c))
            .count()
    };
    assert_eq!(lists(&r), 2);
    // Made again first, as after an edit of what uses it.
    r.edit("Other", &remade(other)).unwrap();
    assert_eq!(lists(&r), 2);
    let edited = MODEL_FILE.replace(
        "ListElement { name: \"a\"; days: [ ListElement { d: 0 }, ListElement { d: 1 } ] }\n",
        "",
    );
    r.edit("Rows", &edited).unwrap();
    assert_eq!(r.num("lm", "count"), 1.0);
    assert_eq!(lists(&r), 1);
}

#[test]
fn an_object_a_script_made_comes_back_as_its_create_object_would_make_it() {
    let src = |ty: &str, word: &str| {
        format!(
            r#"import QtQuick
Item {{
  id: root
  objectName: "root"
  function fill() {{ tile.createObject(area, {{ kind: 3 }}) }}
  Component {{
    id: tile
    Item {{
      objectName: "tile"
      property {ty} kind
      Component.onDestruction: console.log("{word} destroyed")
    }}
  }}
  Item {{ id: area; objectName: "area" }}
}}
"#
        )
    };
    let mut r = rig(&[("Main", &src("int", "old"))]);
    r.call("root", "fill");
    assert_eq!(r.num("tile", "kind"), 3.0);
    let _ = r.h.log();

    // The value still fits: the object is made again, with it.
    r.edit("Main", &remade(&src("int", "kept"))).unwrap();
    assert_eq!(r.num("tile", "kind"), 3.0);
    let _ = r.h.log();

    // It no longer does: a createObject would give back no object, made,
    // completed and destroyed (Qt 6.12), so the reload keeps none either.
    r.edit("Main", &src("point", "new")).unwrap();
    assert_eq!(support::find(&r.h.engine, r.root, "tile"), None);
    let log = r.h.log();
    assert!(
        log.iter()
            .any(|l| l.ends_with("Could not set initial property kind")),
        "{log:?}"
    );
    assert!(log.contains(&"new destroyed".to_owned()), "{log:?}");
}

#[test]
fn a_remade_object_tells_what_its_create_object_call_cannot_set_at_that_call() {
    let src = |inner: &str, more: &str| {
        format!(
            r#"import QtQuick
Item {{
  id: root
  objectName: "root"
  function fill() {{ tile.createObject(area, {{ "q.zz": 3 }}) }}
  Component {{
    id: tile
    Item {{ objectName: "tile"; property QtObject q: QtObject {{ {inner} }} }}
  }}
  Item {{ id: area; objectName: "area" }}{more}
}}
"#
        )
    };
    let mut r = rig(&[("Main", &src("property int zz", ""))]);
    r.call("root", "fill");
    assert!(support::find(&r.h.engine, r.root, "tile").is_some());
    let _ = r.h.log();

    // No script runs in a reload: the place is the one the call had.
    // (The signal makes the file again rather than editing it in place.)
    r.edit("Main", &src("property int y", "\n  signal remade"))
        .unwrap();
    let log = r.h.log();
    assert_eq!(support::find(&r.h.engine, r.root, "tile"), None, "{log:?}");
    assert!(
        log.iter().any(
            |l| l.ends_with("Main.qml:5: Error: Cannot assign to non-existent property \"zz\"")
        ),
        "{log:?}"
    );
}

#[test]
fn an_object_incubate_object_made_follows_its_member_binding_after_a_reload() {
    let src = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property int size: 10
  function fill() {
    label.incubateObject(area, { "face.pixelSize": Qt.binding(function() { return root.size }) },
                         Qt.Synchronous)
  }
  Component { id: label; Item { objectName: "label"; property font face } }
  Item { id: area; objectName: "area" }
}
"#;
    let mut r = rig(&[("Main", src)]);
    r.call("root", "fill");
    r.edit("Main", &remade(src)).unwrap();
    r.h.set(r.find("root"), "size", Value::Int(20));
    // Remade as createObject would make it, the member would keep the
    // first value the binding gave.
    let face = r.h.get(r.find("label"), "face");
    assert_eq!(
        face.member("pixelSize")
            .map(|v| v.to_js_string().to_string()),
        Some("20".to_owned())
    );
}

#[test]
fn objects_incubated_later_come_back_as_ones_incubated_at_once() {
    let src = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  function fill() {
    for (var k = 3; k < 5; k++)
      tile.incubateObject(area, { kind: k }, Qt.Asynchronous)
  }
  Component { id: tile; Item { objectName: "tile"; property int kind } }
  Item { id: area; objectName: "area" }
}
"#;
    let kinds = |r: &Rig| -> Vec<f64> {
        let area = r.find("area");
        r.h.engine
            .children(area)
            .into_iter()
            .map(|t| r.h.num(t, "kind"))
            .collect()
    };
    let mut r = rig(&[("Main", src)]);
    r.call("root", "fill");
    for mut job in r.h.engine.take_incubations() {
        while matches!(job.step(&|| false), JobPoll::Yield) {}
    }
    assert!(!r.h.engine.has_half_made());
    assert_eq!(kinds(&r), [3.0, 4.0]);

    // Made later or at once, each is the object of its own call, in the
    // order the calls made them.
    r.edit("Main", &remade(src)).unwrap();
    assert_eq!(kinds(&r), [3.0, 4.0]);
}

#[test]
fn objects_still_being_incubated_come_back_after_a_reload() {
    let src = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  function fill() {
    for (var k = 3; k < 5; k++)
      tile.incubateObject(area, { kind: k }, Qt.Asynchronous)
  }
  Component { id: tile; Item { objectName: "tile"; property int kind } }
  Item { id: area; objectName: "area" }
}
"#;
    let mut r = rig(&[("Main", src)]);
    r.call("root", "fill");
    // Their turn has not come when the file is made again.
    assert!(r.h.engine.has_half_made() || r.h.engine.has_incubations());
    r.edit("Main", &remade(src)).unwrap();
    for mut job in r.h.engine.take_incubations() {
        while matches!(job.step(&|| false), JobPoll::Yield) {}
    }
    assert!(!r.h.engine.has_half_made());
    let area = r.find("area");
    let kinds: Vec<f64> =
        r.h.engine
            .children(area)
            .into_iter()
            .map(|t| r.h.num(t, "kind"))
            .collect();
    assert_eq!(kinds, [3.0, 4.0]);
}

/// A host that makes its root in slices reloads it as one it made at
/// once: the job keeps what it was asked for once it is done.
#[test]
fn a_root_made_in_slices_reloads_as_one_made_at_once() {
    let page = r#"import QtQuick
Item { objectName: "page"; width: 10 }
"#;
    let mut r = rig(&[("Main", COUNTER), ("Page", page)]);
    let (mut job, handle) =
        r.h.engine
            .make_later(Request::url("Page"), Lane::Visible, ProcId::new(0, 0));
    while matches!(job.step(&|| true), JobPoll::Yield) {}
    let made = handle.result().unwrap().expect("Page.qml creates");
    r.session
        .set_file("Page", remade(page).replace("width: 10", "width: 30"));
    let patch = r.session.build().unwrap().unwrap();
    let done =
        r.h.engine
            .reload(&patch, &[made], &mut |_, _, _| {})
            .expect("the root made in slices reloads");
    assert_eq!(r.h.num(done.roots[0].1, "width"), 30.0);
}
