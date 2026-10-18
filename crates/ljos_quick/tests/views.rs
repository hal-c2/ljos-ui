//! ListView and GridView against the scene: what a scenario in
//! `features/views/` cannot see, or cannot count exactly because Qt does
//! not promise it (how many delegates there are, and when they are made).

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use ljos_engine::{Atom, Engine, ModelSignals, ObjectId, QmlListModel, QtObject, Value};
use ljos_quick::{QuickScene, child_items, update_system_prefs, view_layout_passes};
use ljos_runtime::Time;

/// A scene on `root` whose delegates are all made: frames until nothing
/// is left in the incubator.
fn settled(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = scene(e, root, 400.0, 400.0);
    settle(&mut s, e, 0);
    s
}

fn settle(s: &mut QuickScene, e: &Engine, from: u64) {
    for i in 0..40 {
        s.frame(e, Time::from_millis(from + i * 16));
    }
}

/// The names of the delegates a view has, in no order.
fn rows(e: &Engine, view: ObjectId) -> Vec<String> {
    let content = get(e, view, "contentItem")
        .as_object()
        .expect("a view has a content item");
    child_items(e, content)
        .into_iter()
        .map(|o| get(e, o, "objectName").to_js_string().to_string())
        .filter(|n| n.starts_with('r'))
        .collect()
}

fn row(e: &Engine, view: ObjectId, name: &str) -> ObjectId {
    let content = get(e, view, "contentItem").as_object().unwrap();
    child_items(e, content)
        .into_iter()
        .find(|o| get(e, *o, "objectName").to_js_string().as_str() == name)
        .unwrap_or_else(|| panic!("no delegate named {name}: {:?}", rows(e, view)))
}

fn warnings(e: &Engine) -> Vec<String> {
    e.take_log().into_iter().map(|l| l.message).collect()
}

const HUNDRED_THOUSAND: &str = r##"import QtQuick
ListView {
  width: 200; height: 300
  model: 100000
  delegate: Item { objectName: "r" + index; width: 200; height: 20 }
}
"##;

#[test]
fn a_list_of_100_000_rows_has_delegates_only_around_what_it_shows() {
    let e = engine();
    let root = load(&e, HUNDRED_THOUSAND);
    let mut s = settled(&e, root);
    assert_eq!(num(&e, root, "count"), 100_000.0);
    assert_eq!(num(&e, root, "contentHeight"), 2_000_000.0);
    // 15 rows are shown and `cacheBuffer` (320) holds 16 more after them;
    // one more starts on the edge of the buffer.
    let at_top = rows(&e, root);
    assert_eq!(at_top.len(), 32, "{at_top:?}");
    assert!(at_top.contains(&"r0".to_string()));

    // The middle: the buffer is on both sides now. The first row is the
    // current one, and `currentItem` stays wherever the view is.
    set(&e, root, "contentY", Value::Real(1_000_000.0));
    settle(&mut s, &e, 1000);
    let middle = rows(&e, root);
    assert_eq!(middle.len(), 48 + 1, "{middle:?}");
    assert!(middle.contains(&"r50000".to_string()));
    assert!(middle.contains(&"r0".to_string()));
    assert!(!middle.contains(&"r1".to_string()));
    assert_eq!(num(&e, row(&e, root, "r50000"), "y"), 1_000_000.0);

    // The end.
    e.invoke_by_name(root, "positionViewAtEnd", &[]).unwrap();
    settle(&mut s, &e, 2000);
    assert_eq!(num(&e, root, "contentY"), 2_000_000.0 - 300.0);
    let end = rows(&e, root);
    assert!(end.len() <= 32 + 1, "{end:?}");
    assert!(end.contains(&"r99999".to_string()));
    assert_eq!(warnings(&e), Vec::<String>::new());
}

#[test]
fn a_grid_of_100_000_cells_has_delegates_only_around_what_it_shows() {
    let e = engine();
    let root = load(
        &e,
        r##"import QtQuick
GridView {
  width: 240; height: 300
  cellWidth: 80; cellHeight: 60
  model: 100000
  delegate: Item { objectName: "r" + index; width: 80; height: 60 }
}
"##,
    );
    let mut s = settled(&e, root);
    assert_eq!(num(&e, root, "count"), 100_000.0);
    // Three in a line: 33 334 lines of 60.
    assert_eq!(num(&e, root, "contentHeight"), 33_334.0 * 60.0);
    let at_top = rows(&e, root).len();
    assert!((15..=40).contains(&at_top), "{at_top}");

    set(&e, root, "contentY", Value::Real(600_000.0));
    settle(&mut s, &e, 1000);
    let middle = rows(&e, root);
    assert!((15..=60).contains(&middle.len()), "{}", middle.len());
    let there = row(&e, root, "r30001");
    assert_eq!(
        (num(&e, there, "x"), num(&e, there, "y")),
        (80.0, 600_000.0)
    );
    assert_eq!(warnings(&e), Vec::<String>::new());
}

#[test]
fn the_rows_outside_what_is_shown_are_made_over_later_frames() {
    let e = engine();
    let root = load(&e, HUNDRED_THOUSAND);
    // Before any frame the view has what it shows, made at once: a view
    // is never blank where it has rows. The rows of the buffer wait for
    // the incubator, which a frame runs inside its budget.
    let at_once = rows(&e, root);
    assert_eq!(at_once.len(), 15, "{at_once:?}");
    let mut s = scene(&e, root, 400.0, 400.0);
    settle(&mut s, &e, 0);
    assert_eq!(rows(&e, root).len(), 32);
}

#[test]
fn the_rows_made_in_one_frame_are_laid_out_together() {
    let e = engine();
    let root = load(&e, HUNDRED_THOUSAND);
    let mut s = scene(&e, root, 400.0, 400.0);
    let mut t = 0;
    let mut frame = |s: &mut QuickScene| {
        let before = view_layout_passes(&e, root);
        s.frame(&e, Time::from_millis(t));
        t += 16;
        view_layout_passes(&e, root) - before
    };
    // The buffer's rows are made by the incubator. Each was laid out
    // twice as it became ready; a frame now lays out what it made once.
    let passes: Vec<u64> = (0..40).map(|_| frame(&mut s)).collect();
    assert_eq!(rows(&e, root).len(), 32);
    assert!(passes.iter().all(|p| *p <= 1), "{passes:?}");
    // Scrolling by five rows a frame makes five rows a frame.
    for i in 1..=20 {
        set(&e, root, "contentY", Value::Real(f64::from(i) * 100.0));
        assert_eq!(frame(&mut s), 1, "frame {i}");
    }
    // Idle: nothing is laid out.
    assert_eq!(frame(&mut s), 0);
}

#[test]
fn rows_prepended_above_what_is_shown_do_not_move_it() {
    let e = engine();
    let root = load(
        &e,
        r##"import QtQuick
ListView {
  width: 200; height: 300
  model: lm
  function fill() { for (var i = 0; i < 200; i++) lm.append({ name: "n" + i }) }
  function prepend() { for (var i = 0; i < 50; i++) lm.insert(0, { name: "p" + i }) }
  ListModel { id: lm }
  delegate: Item {
    required property string name
    objectName: "r" + name; width: 200; height: 20
  }
}
"##,
    );
    e.invoke_by_name(root, "fill", &[]).unwrap();
    let mut s = settled(&e, root);
    set(&e, root, "contentY", Value::Real(1005.0));
    settle(&mut s, &e, 1000);
    let first = row(&e, root, "rn50");
    let seen = |e: &Engine| num(e, first, "y") - num(e, root, "contentY");
    assert_eq!(seen(&e), -5.0);

    e.invoke_by_name(root, "prepend", &[]).unwrap();
    // At once, and after the frames that follow: the row the user was
    // looking at is the same delegate at the same place in the view.
    assert_eq!(seen(&e), -5.0);
    settle(&mut s, &e, 2000);
    assert!(e.is_alive(first));
    assert_eq!(seen(&e), -5.0);
    assert_eq!(num(&e, root, "count"), 250.0);
    assert_eq!(
        e.invoke_by_name(
            root,
            "indexAt",
            &[
                Value::Real(10.0),
                Value::Real(num(&e, root, "contentY") + 10.0)
            ]
        )
        .unwrap(),
        Value::Int(100)
    );
    // None of the 50 rows that arrived above is in the buffer's reach
    // but the ones right before what is shown.
    assert!(!rows(&e, root).contains(&"rp49".to_string()));
    assert_eq!(warnings(&e), Vec::<String>::new());
}

#[test]
fn a_row_keeps_its_length_by_its_key_when_rows_move_under_it() {
    let e = engine();
    let root = load(
        &e,
        r##"import QtQuick
ListView {
  width: 200; height: 300
  cacheBuffer: 0
  model: lm
  function fill() { for (var i = 0; i < 100; i++) lm.append({ name: "n" + i, tall: i % 2 ? 40 : 20 }) }
  function shift() { lm.remove(0, 1) }
  ListModel { id: lm }
  delegate: Item {
    required property string name
    required property int tall
    objectName: "r" + name; width: 200; height: tall
  }
}
"##,
    );
    e.invoke_by_name(root, "fill", &[]).unwrap();
    let mut s = settled(&e, root);
    set(&e, root, "contentY", Value::Real(600.0));
    settle(&mut s, &e, 1000);
    // Far from what was measured the rows are where rows of the average
    // length put them; back at the start they are where they were.
    set(&e, root, "contentY", Value::Real(0.0));
    settle(&mut s, &e, 2000);
    assert_eq!(num(&e, row(&e, root, "rn1"), "y"), 20.0);
    assert_eq!(num(&e, row(&e, root, "rn2"), "y"), 60.0);
    // Row 0 goes: every row has another number and the same length.
    e.invoke_by_name(root, "shift", &[]).unwrap();
    settle(&mut s, &e, 3000);
    let (one, two) = (row(&e, root, "rn1"), row(&e, root, "rn2"));
    assert_eq!(num(&e, one, "height"), 40.0);
    assert_eq!(num(&e, two, "y") - num(&e, one, "y"), 40.0);
    assert_eq!(warnings(&e), Vec::<String>::new());
}

/// Rows of one role, `label`, each with a key that stays with it.
#[derive(Default)]
struct Labels {
    rows: RefCell<Vec<(u64, String)>>,
    signals: RefCell<Option<ModelSignals>>,
}

impl Labels {
    fn signals(&self) -> ModelSignals {
        self.signals.borrow().clone().expect("the model is set")
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

    fn row_key(&self, row: usize) -> u64 {
        self.rows.borrow()[row].0
    }
}

#[test]
fn a_model_written_in_rust_drives_a_view_with_its_row_signals() {
    let e = engine();
    let root = load(
        &e,
        r##"import QtQuick
ListView {
  width: 200; height: 300
  model: null
  delegate: Item {
    required property string label
    required property int index
    objectName: "r" + label; width: 200; height: 20
  }
}
"##,
    );
    let labels = Rc::new(Labels::default());
    labels
        .rows
        .borrow_mut()
        .extend((0..1000u64).map(|k| (k, format!("{k}"))));
    let holder = e.create_native(QtObject::default(), Some(root)).unwrap();
    let signals = e.set_list_model(holder, labels.clone()).unwrap();
    *labels.signals.borrow_mut() = Some(signals);
    set(&e, root, "model", Value::Object(holder));
    let mut s = settled(&e, root);
    assert_eq!(num(&e, root, "count"), 1000.0);
    assert_eq!(rows(&e, root).len(), 32);
    assert_eq!(num(&e, row(&e, root, "r3"), "y"), 60.0);

    // rowsInserted.
    labels.signals().rows_about_to_be_inserted(&e, 1, 1);
    labels.rows.borrow_mut().insert(1, (5000, "new".into()));
    labels.signals().rows_inserted(&e, 1, 1);
    settle(&mut s, &e, 1000);
    assert_eq!(num(&e, root, "count"), 1001.0);
    assert_eq!(num(&e, row(&e, root, "rnew"), "y"), 20.0);
    assert_eq!(num(&e, row(&e, root, "r3"), "y"), 80.0);
    assert_eq!(num(&e, row(&e, root, "r3"), "index"), 4.0);

    // dataChanged.
    labels.rows.borrow_mut()[1].1 = "renamed".into();
    labels.signals().data_changed(&e, 1, 1, &["label"]);
    assert_eq!(num(&e, row(&e, root, "rrenamed"), "y"), 20.0);

    // rowsMoved: the first row goes after the third.
    let moved = labels.rows.borrow_mut().remove(0);
    labels.rows.borrow_mut().insert(2, moved);
    labels.signals().rows_moved(&e, 0, 2, 1);
    settle(&mut s, &e, 2000);
    assert_eq!(num(&e, row(&e, root, "rrenamed"), "y"), 0.0);
    assert_eq!(num(&e, row(&e, root, "r0"), "y"), 40.0);

    // rowsRemoved.
    labels.signals().rows_about_to_be_removed(&e, 0, 2);
    labels.rows.borrow_mut().drain(0..2);
    labels.signals().rows_removed(&e, 0, 2);
    settle(&mut s, &e, 3000);
    assert_eq!(num(&e, root, "count"), 999.0);
    assert_eq!(num(&e, row(&e, root, "r0"), "y"), 0.0);
    assert!(!rows(&e, root).contains(&"rrenamed".to_string()));

    // modelReset.
    labels.rows.borrow_mut().truncate(3);
    labels.signals().model_reset(&e);
    settle(&mut s, &e, 4000);
    assert_eq!(num(&e, root, "count"), 3.0);
    assert_eq!(rows(&e, root).len(), 3);
    assert_eq!(num(&e, root, "contentHeight"), 60.0);
    assert_eq!(warnings(&e), Vec::<String>::new());
}

#[test]
fn a_screen_reader_keeps_twenty_more_rows_on_each_side() {
    let e = engine();
    let root = load(
        &e,
        r##"import QtQuick
ListView {
  width: 200; height: 300
  cacheBuffer: 0
  model: 100000
  delegate: Item { objectName: "r" + index; width: 200; height: 20 }
}
"##,
    );
    let mut s = settled(&e, root);
    set(&e, root, "contentY", Value::Real(10_000.0));
    settle(&mut s, &e, 1000);
    let plain = rows(&e, root).len();
    assert!((15..=17).contains(&plain), "{plain}");
    update_system_prefs(&e, |p| p.screen_reader_active = true);
    settle(&mut s, &e, 2000);
    let listening = rows(&e, root).len();
    assert!(
        (plain + 40..=plain + 42).contains(&listening),
        "{plain} then {listening}"
    );
    update_system_prefs(&e, |p| p.screen_reader_active = false);
    settle(&mut s, &e, 3000);
    assert_eq!(rows(&e, root).len(), plain);
}

#[test]
fn a_delegate_model_filtered_on_a_group_shows_only_its_members() {
    let e = engine();
    let root = load(
        &e,
        r##"import QtQuick
import QtQml.Models
Item {
  width: 200; height: 300
  function pick() { dm.items.addGroups(1, 1, ["selected"]); dm.items.addGroups(3, 1, ["selected"]) }
  function filter() { dm.filterOnGroup = "selected" }
  ListModel { id: lm
    ListElement { name: "a" } ListElement { name: "b" } ListElement { name: "c" } ListElement { name: "d" } }
  DelegateModel {
    id: dm; objectName: "dm"; model: lm
    groups: [DelegateModelGroup { id: sel; objectName: "sel"; name: "selected" }]
    delegate: Item {
      required property string name
      objectName: "r" + name; width: 200; height: 20
      property var grp: DelegateModel.groups
    }
  }
  ListView { objectName: "list"; anchors.fill: parent; model: dm }
}
"##,
    );
    let mut s = settled(&e, root);
    let (list, dm, sel) = (
        find(&e, root, "list"),
        find(&e, root, "dm"),
        find(&e, root, "sel"),
    );
    assert_eq!(num(&e, list, "count"), 4.0);
    assert_eq!(
        num(&e, get(&e, dm, "items").as_object().unwrap(), "count"),
        4.0
    );
    assert_eq!(
        num(
            &e,
            get(&e, dm, "persistedItems").as_object().unwrap(),
            "count"
        ),
        0.0
    );
    e.invoke_by_name(root, "pick", &[]).unwrap();
    settle(&mut s, &e, 1000);
    assert_eq!(num(&e, sel, "count"), 2.0);
    let grp = match get(&e, row(&e, list, "rb"), "grp") {
        Value::List(items) => items
            .iter()
            .map(|v| v.to_js_string().to_string())
            .collect::<Vec<_>>(),
        v @ Value::Js(_) => {
            let n = e
                .js_member(&v, "length")
                .map_or(0, |l| l.to_number() as usize);
            (0..n)
                .filter_map(|i| e.js_member(&v, &i.to_string()))
                .map(|v| v.to_js_string().to_string())
                .collect()
        }
        v => panic!("grp is {v:?}"),
    };
    assert_eq!(grp, ["items", "selected"]);
    e.invoke_by_name(root, "filter", &[]).unwrap();
    settle(&mut s, &e, 2000);
    assert_eq!(num(&e, list, "count"), 2.0);
    assert_eq!(num(&e, list, "contentHeight"), 40.0);
    assert_eq!(num(&e, row(&e, list, "rb"), "y"), 0.0);
    assert_eq!(num(&e, row(&e, list, "rd"), "y"), 20.0);
    assert!(!rows(&e, list).contains(&"ra".to_string()));
    assert_eq!(warnings(&e), Vec::<String>::new());
}

#[test]
fn a_move_transition_moves_rows_and_then_the_view_is_idle() {
    let e = engine();
    let root = load(
        &e,
        r##"import QtQuick
ListView {
  width: 200; height: 300
  model: lm
  function shift() { lm.move(0, 2, 1) }
  ListModel { id: lm
    ListElement { name: "a" } ListElement { name: "b" } ListElement { name: "c" } }
  move: Transition { NumberAnimation { properties: "x,y"; duration: 160 } }
  displaced: Transition { NumberAnimation { properties: "x,y"; duration: 160 } }
  delegate: Item {
    required property string name
    objectName: "r" + name; width: 200; height: 20
  }
}
"##,
    );
    let mut s = settled(&e, root);
    let a = row(&e, root, "ra");
    e.invoke_by_name(root, "shift", &[]).unwrap();
    // Part of the way there: the moved row is between where it was and
    // its new place, and the view asks for frames until it is there.
    for i in 0..4 {
        s.frame(&e, Time::from_millis(1000 + i * 16));
    }
    let y = num(&e, a, "y");
    assert!((2.0..38.0).contains(&y), "{y}");
    settle(&mut s, &e, 1100);
    assert_eq!(num(&e, a, "y"), 40.0);
    let out = s.frame(&e, Time::from_millis(5000));
    assert_eq!(out.next_wake, None);
    assert!(!out.changed);
    assert_eq!(warnings(&e), Vec::<String>::new());
}

#[test]
fn a_view_that_is_idle_asks_for_no_frame() {
    let e = engine();
    let root = load(&e, HUNDRED_THOUSAND);
    let mut s = settled(&e, root);
    // Nothing polls (0 % CPU when idle): with every delegate made and
    // nothing moving, a frame leaves nothing to come back for.
    let out = s.frame(&e, Time::from_millis(5000));
    assert_eq!(out.next_wake, None);
    assert!(!out.changed);
}

/// A new view whose rows get their length from text laid out at polish
/// time: before the first row is measured, every row it could fit at 1
/// each was made and dropped again.
#[test]
fn a_new_view_of_text_rows_makes_only_what_it_shows() {
    let e = engine();
    let root = load(
        &e,
        r#"import QtQuick
Item {
  id: root
  width: 200; height: 300
  property int created: 0
  property int destroyed: 0
  readonly property int live: created - destroyed
  property bool grown: false
  ListView {
    anchors.fill: parent
    model: 1000
    delegate: Column {
      required property int index
      width: 200
      Text { text: "Row " + parent.index; font.pixelSize: 20 }
      Rectangle { width: 10; height: root.grown ? 60 : 0 }
      Component.onCompleted: root.created++
      Component.onDestruction: root.destroyed++
    }
  }
}"#,
    );
    let mut s = scene(&e, root, 200.0, 300.0);
    settle(&mut s, &e, 0);
    // Qt: 27 (rows of 23 and the cache). 66 before.
    let created = num(&e, root, "created");
    assert!((20.0..=34.0).contains(&created), "created {created}");
    assert_eq!(num(&e, root, "live"), created);
    set(&e, root, "grown", Value::Bool(true));
    settle(&mut s, &e, 1000);
    // Qt: 8.
    let live = num(&e, root, "live");
    assert!((5.0..=12.0).contains(&live), "live {live}");
}

/// A new view whose rows are a text over a block of fixed height: the
/// first row's length is not the block's alone, which a column has before
/// its text is laid out. Rows made at that length were dropped again once
/// they had their own.
#[test]
fn a_new_view_makes_no_row_it_lets_go_when_its_rows_are_text_over_a_block() {
    let e = engine();
    let root = load(
        &e,
        r#"import QtQuick
Item {
  id: root
  width: 400; height: 300
  property int created: 0
  property int destroyed: 0
  ListView {
    anchors.fill: parent
    model: 40
    delegate: Column {
      required property int index
      width: 400
      Text { text: "Row " + parent.index; font.pixelSize: 40 }
      Rectangle { width: 10; height: 50 }
      Component.onCompleted: root.created++
      Component.onDestruction: root.destroyed++
    }
  }
}"#,
    );
    let mut s = scene(&e, root, 400.0, 300.0);
    settle(&mut s, &e, 0);
    // Qt: 7 made (rows of 95, the view and its cache of 320) and none let go.
    let created = num(&e, root, "created");
    assert!((6.0..=8.0).contains(&created), "created {created}");
    assert_eq!(num(&e, root, "destroyed"), 0.0);
}

/// Rows that stay 0 long after their polish: the view does not wait on
/// them for good.
#[test]
fn a_view_of_rows_of_no_length_still_fills() {
    let e = engine();
    let root = load(
        &e,
        r#"import QtQuick
Item {
  id: root
  width: 200; height: 300
  property int created: 0
  ListView {
    anchors.fill: parent
    model: 1000
    delegate: Column {
      width: 200
      Component.onCompleted: root.created++
    }
  }
}"#,
    );
    let mut s = scene(&e, root, 200.0, 300.0);
    settle(&mut s, &e, 0);
    assert!(num(&e, root, "created") > 30.0);
}
