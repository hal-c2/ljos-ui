//! `Repeater`: where its items go, and that a model change touches only
//! the items of its rows. `features/models/` holds the behaviour checked
//! against Qt.
//!
//! The benchmark is ignored by default:
//!
//! `cargo test --release -p ljos_quick --test repeater -- --ignored --nocapture`

mod common;

use std::time::Instant;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::Repeater;

fn call(e: &Engine, obj: ObjectId, method: &str, args: &[Value]) -> Value {
    e.invoke_by_name(obj, method, args)
        .unwrap_or_else(|err| panic!("{method}: {err}"))
}

fn names(e: &Engine, objects: &[ObjectId]) -> String {
    objects
        .iter()
        .map(|o| get(e, *o, "objectName").to_js_string().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn created(e: &Engine, repeater: ObjectId) -> u64 {
    e.with::<Repeater, _>(repeater, |r| r.delegates().map(|d| d.created()))
        .unwrap()
        .expect("a completed Repeater has delegates")
}

fn warnings(e: &Engine) -> Vec<String> {
    e.take_log()
        .into_iter()
        .filter(|l| l.level >= ljos_engine::LogLevel::Warning)
        .map(|l| l.message)
        .collect()
}

const ROW: &str = r#"
import QtQuick
Row {
    id: root
    property int touched: 0
    function add(at, name) { lm.insert(at, { name: name }) }
    ListModel {
        id: lm; objectName: "lm"
        ListElement { name: "a" }
        ListElement { name: "b" }
        ListElement { name: "c" }
    }
    Item { objectName: "head"; width: 5; height: 5 }
    Repeater {
        id: rep; objectName: "rep"; model: lm
        Item {
            objectName: name; width: 10; height: 10
            property int row: index
            onRowChanged: root.touched++
        }
    }
    Item { objectName: "tail"; width: 5; height: 5 }
}
"#;

#[test]
fn items_sit_before_the_repeater_in_model_order() {
    let e = engine();
    let root = load(&e, ROW);
    let rep = find(&e, root, "rep");
    let lm = find(&e, root, "lm");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(
        names(&e, &list(&e, root, "children")),
        "head a b c rep tail"
    );
    // The Row lays them out where the Repeater stands.
    let xs = |e: &Engine| -> Vec<f64> {
        list(e, root, "children")
            .iter()
            .filter(|o| **o != rep)
            .map(|o| num(e, *o, "x"))
            .collect()
    };
    assert_eq!(xs(&e), [0.0, 5.0, 15.0, 25.0, 35.0]);
    // Owned by the Repeater, shown by its parent.
    let a = find(&e, root, "a");
    assert_eq!(get(&e, a, "parent"), Value::Object(root));
    assert!(e.children(rep).contains(&a));

    call(&e, root, "add", &[Value::Int(1), Value::from("z")]);
    assert_eq!(
        names(&e, &list(&e, root, "children")),
        "head a z b c rep tail"
    );
    frame(&mut s, &e);
    assert_eq!(xs(&e), [0.0, 5.0, 15.0, 25.0, 35.0, 45.0]);

    call(
        &e,
        lm,
        "move",
        &[Value::Int(0), Value::Int(2), Value::Int(2)],
    );
    assert_eq!(
        names(&e, &list(&e, root, "children")),
        "head b c a z rep tail"
    );
    assert_eq!(
        names(&e, &list(&e, root, "data")),
        "lm head b c a z rep tail"
    );

    call(&e, lm, "remove", &[Value::Int(1), Value::Int(2)]);
    assert_eq!(names(&e, &list(&e, root, "children")), "head b z rep tail");
    frame(&mut s, &e);
    assert_eq!(xs(&e), [0.0, 5.0, 15.0, 25.0]);
    assert!(warnings(&e).is_empty());
}

#[test]
fn a_model_change_makes_only_the_items_of_its_rows() {
    let e = engine();
    let root = load(&e, ROW);
    let rep = find(&e, root, "rep");
    let lm = find(&e, root, "lm");
    let at = |i: i32| call(&e, rep, "itemAt", &[Value::Int(i)]);
    assert_eq!(created(&e, rep), 3);
    let (a, b, c) = (at(0), at(1), at(2));
    let touched = num(&e, root, "touched");

    // An append is one item; the others are not even renumbered.
    call(&e, root, "add", &[Value::Int(3), Value::from("d")]);
    assert_eq!(created(&e, rep), 4);
    assert_eq!((at(0), at(1), at(2)), (a.clone(), b.clone(), c.clone()));
    assert!(num(&e, root, "touched") - touched <= 1.0);

    call(
        &e,
        lm,
        "move",
        &[Value::Int(3), Value::Int(0), Value::Int(1)],
    );
    assert_eq!(created(&e, rep), 4);
    assert_eq!((at(1), at(2), at(3)), (a.clone(), b.clone(), c.clone()));

    call(
        &e,
        lm,
        "setProperty",
        &[Value::Int(1), Value::from("name"), Value::from("q")],
    );
    assert_eq!(created(&e, rep), 4);
    assert_eq!(
        get(&e, a.as_object().unwrap(), "objectName"),
        Value::from("q")
    );

    call(&e, lm, "remove", &[Value::Int(0), Value::Undefined]);
    assert_eq!(created(&e, rep), 4);
    assert_eq!((at(0), at(1), at(2)), (a, b, c));
    assert_eq!(at(3), Value::Null);
    assert_eq!(at(-1), Value::Null);
    assert_eq!(num(&e, rep, "count"), 3.0);
    assert!(warnings(&e).is_empty());
}

#[test]
fn removed_items_leave_the_tree_and_are_destroyed() {
    let e = engine();
    let root = load(&e, ROW);
    let lm = find(&e, root, "lm");
    e.process_deferred_deletes();
    let before = e.object_count();
    call(&e, root, "add", &[Value::Int(0), Value::from("z")]);
    assert!(e.object_count() > before);
    let z = find(&e, root, "z");
    call(&e, lm, "remove", &[Value::Int(0), Value::Undefined]);
    // Out of the parent at once, gone at the next frame.
    assert!(!list(&e, root, "children").contains(&z));
    e.process_deferred_deletes();
    assert_eq!(e.object_count(), before);
}

#[test]
fn a_repeater_moves_its_items_with_it() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    function hop() { rep.parent = right }
    Row {
        id: left; objectName: "left"
        Repeater {
            id: rep; objectName: "rep"; model: 2
            Item { objectName: "d" + index; width: 10; height: 10 }
        }
    }
    Row { id: right; objectName: "right" }
}
"#,
    );
    let (left, right) = (find(&e, root, "left"), find(&e, root, "right"));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(names(&e, &list(&e, left, "children")), "d0 d1 rep");
    assert_eq!(num(&e, left, "implicitWidth"), 20.0);
    call(&e, root, "hop", &[]);
    assert_eq!(names(&e, &list(&e, left, "children")), "");
    assert_eq!(names(&e, &list(&e, right, "children")), "d0 d1 rep");
    frame(&mut s, &e);
    assert_eq!(num(&e, left, "implicitWidth"), 0.0);
    assert_eq!(num(&e, right, "implicitWidth"), 20.0);
    assert!(warnings(&e).is_empty());
}

#[test]
fn a_delegate_that_is_not_an_item_is_refused() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Repeater { objectName: "rep"; model: 1; QtObject { objectName: "plain" } }
}
"#,
    );
    let w = warnings(&e);
    assert!(
        w.iter()
            .any(|m| m.contains("Delegate must be of Item type")),
        "{w:?}"
    );
    assert_eq!(names(&e, &list(&e, root, "children")), "rep");
}

/// A Column of a Repeater over a ListModel of `rows` rows.
fn column_of(rows: usize) -> String {
    let mut s = String::from(
        "import QtQuick\nColumn {\n  id: root\n  property int touched: 0\n  function add() { lm.append({ name: \"new\", cost: 1 }) }\n  ListModel {\n    id: lm; objectName: \"lm\"\n",
    );
    for i in 0..rows {
        s.push_str(&format!(
            "    ListElement {{ name: \"row {i}\"; cost: {i} }}\n"
        ));
    }
    s.push_str(
        r##"  }
  Repeater {
    id: rep; objectName: "rep"; model: lm
    Rectangle {
      width: 100 + cost; height: 20; color: index % 2 ? "#eeeeee" : "#ffffff"
      property string label: name
      property int row: index
      onRowChanged: root.touched++
      onLabelChanged: root.touched++
    }
  }
}
"##,
    );
    s
}

#[test]
fn appending_to_a_thousand_rows_makes_one_item() {
    let e = engine();
    let root = load(&e, &column_of(1000));
    let rep = find(&e, root, "rep");
    let mut s = scene(&e, root, 800.0, 600.0);
    frame(&mut s, &e);
    assert_eq!(created(&e, rep), 1000);
    assert_eq!(num(&e, root, "height"), 20_000.0);
    let touched = num(&e, root, "touched");
    let objects = e.object_count();
    call(&e, root, "add", &[]);
    assert_eq!(created(&e, rep), 1001);
    // Only the new item's own two properties changed.
    assert!(num(&e, root, "touched") - touched <= 2.0);
    // The item and its row object.
    assert_eq!(e.object_count() - objects, 2);
    frame(&mut s, &e);
    assert_eq!(num(&e, root, "height"), 20_020.0);
    assert!(warnings(&e).is_empty());
}

#[test]
#[ignore]
fn bench_repeater_1000_rows_and_one_append() {
    let e = engine();
    compile(&e, &[("Main", &column_of(1000))]);
    // Warm: the file's types and the delegate's are made once.
    let r = e.create_from_url("Main", None, Vec::new()).unwrap();
    e.destroy_now(r);

    let n = 20;
    let t = Instant::now();
    let mut roots = Vec::new();
    for _ in 0..n {
        roots.push(e.create_from_url("Main", None, Vec::new()).unwrap());
    }
    let create = t.elapsed().as_secs_f64() / f64::from(n);
    println!(
        "Repeater over 1000 rows: {:.2} ms to create ({:.1} us per delegate)",
        create * 1e3,
        create * 1e6 / 1000.0
    );

    let root = roots[0];
    let rep = find(&e, root, "rep");
    let before = (created(&e, rep), num(&e, root, "touched"));
    let appends = 200;
    let t = Instant::now();
    for _ in 0..appends {
        call(&e, root, "add", &[]);
    }
    let append = t.elapsed().as_secs_f64() / f64::from(appends);
    let made = created(&e, rep) - before.0;
    let touched = num(&e, root, "touched") - before.1;
    println!(
        "append 1 row to 1000+:   {:.1} us per append; {made} delegates made and {touched} change handlers run in {appends} appends",
        append * 1e6
    );
    assert_eq!(made, u64::from(appends as u32));
    assert!(touched <= 2.0 * f64::from(appends));

    // With the frame after it: the Column places its children again.
    let mut s = scene(&e, root, 800.0, 600.0);
    frame(&mut s, &e);
    let t = Instant::now();
    for _ in 0..appends {
        call(&e, root, "add", &[]);
        frame(&mut s, &e);
    }
    let framed = t.elapsed().as_secs_f64() / f64::from(appends);
    println!("append 1 row and frame:  {:.1} us per append", framed * 1e6);
}
