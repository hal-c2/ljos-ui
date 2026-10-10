//! `Loader`: what it makes and drops, its sizes, and asynchronous loading
//! through the scene's incubation step.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::{Loader, QuickScene};
use ljos_runtime::{Clock, Time};

const KID: &str = r#"
import QtQuick
Item { objectName: "kid"; property string tag: "kid"; property int n: 1; width: 40; height: 10 }
"#;

fn text(e: &Engine, o: ObjectId, name: &str) -> String {
    get(e, o, name).to_js_string().to_string()
}

fn exists(e: &Engine, root: ObjectId, name: &str) -> bool {
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if text(e, o, "objectName") == name {
            return true;
        }
        stack.extend(e.children(o));
    }
    false
}

#[test]
fn a_loader_makes_its_item_and_says_so_in_order() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    id: root
    property string events: ""
    Component {
        id: comp
        Rectangle {
            objectName: "kid"; width: 30; height: 20
            Component.onCompleted: root.events += "made "
            Component.onDestruction: root.events += "gone "
        }
    }
    Loader {
        id: loader; objectName: "loader"; sourceComponent: comp
        onItemChanged: root.events += item ? "item " : "none "
        onStatusChanged: root.events += "status" + status + " "
        onProgressChanged: root.events += "progress" + progress + " "
        onLoaded: root.events += "loaded "
    }
    function clear() { loader.sourceComponent = undefined }
}
"#,
    );
    let loader = find(&e, root, "loader");
    let kid = find(&e, root, "kid");
    assert_eq!(get(&e, loader, "item").as_object(), Some(kid));
    assert_eq!(e.visual_parent(kid), Some(loader));
    assert_eq!(list(&e, loader, "children"), vec![kid]);
    assert_eq!(num(&e, loader, "status"), 1.0);
    assert_eq!(
        text(&e, root, "events"),
        "made item status1 progress1 loaded "
    );

    // Switched off: the item hears of it first and leaves the Loader at
    // once; `progress` stays, as in Qt.
    set(&e, root, "events", Value::from(""));
    set(&e, loader, "active", Value::Bool(false));
    assert_eq!(text(&e, root, "events"), "gone none status0 ");
    assert_eq!(list(&e, loader, "children"), vec![]);
    assert_eq!(num(&e, loader, "progress"), 1.0);
    // It is destroyed at the next frame, not under a running handler.
    assert!(e.is_alive(kid));
    e.process_deferred_deletes();
    assert!(!e.is_alive(kid));

    set(&e, root, "events", Value::from(""));
    set(&e, loader, "active", Value::Bool(true));
    assert_eq!(text(&e, root, "events"), "made item status1 loaded ");
    set(&e, root, "events", Value::from(""));
    e.invoke_by_name(root, "clear", &[]).unwrap();
    assert_eq!(text(&e, root, "events"), "gone status0 progress0 none ");
    assert!(e.take_warnings().is_empty());
}

#[test]
fn the_item_is_shown_and_removed_by_the_scene() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 100; height: 100
    Component { id: comp; Rectangle { objectName: "kid"; width: 30; height: 20 } }
    Loader { objectName: "loader"; sourceComponent: comp }
}
"#,
    );
    let loader = find(&e, root, "loader");
    let kid = find(&e, root, "kid");
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    let node = s.node_of(kid).expect("the item is in the scene");
    let parent = s.node_of(loader).unwrap();
    assert_eq!(s.render_tree().node(parent).unwrap().children(), &[node]);

    set(&e, loader, "active", Value::Bool(false));
    assert!(s.needs_frame());
    frame(&mut s, &e);
    assert!(s.node_of(kid).is_none());
    assert!(!e.is_alive(kid));
    assert!(s.render_tree().node(parent).unwrap().children().is_empty());
    assert!(!s.needs_frame());
}

#[test]
fn a_loader_without_a_size_follows_its_item() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Component { id: comp; Item { objectName: "kid"; width: 30; height: 20 } }
    Loader { objectName: "loader"; sourceComponent: comp }
}
"#,
    );
    let (loader, kid) = (find(&e, root, "loader"), find(&e, root, "kid"));
    assert_eq!(rect(&e, loader), (0.0, 0.0, 30.0, 20.0));
    assert_eq!(num(&e, loader, "implicitWidth"), 30.0);
    set(&e, kid, "width", Value::from(60.0));
    assert_eq!(num(&e, loader, "width"), 60.0);

    // Once the Loader is given a width, the item takes the Loader's, and
    // only along that axis.
    set(&e, loader, "width", Value::from(80.0));
    assert_eq!(num(&e, kid, "width"), 80.0);
    set(&e, kid, "height", Value::from(25.0));
    assert_eq!(num(&e, loader, "height"), 25.0);
    set(&e, loader, "width", Value::from(90.0));
    assert_eq!(num(&e, kid, "width"), 90.0);

    // An unloaded Loader keeps its size.
    set(&e, loader, "active", Value::Bool(false));
    assert_eq!(num(&e, loader, "height"), 25.0);
}

#[test]
fn a_sized_or_anchored_loader_sizes_its_item() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 300; height: 200
    Component { id: comp; Item { implicitWidth: 11; implicitHeight: 12; width: 30; height: 20 } }
    Loader { objectName: "sized"; width: 100; height: 50; sourceComponent: comp }
    Loader { objectName: "filling"; anchors.fill: parent; sourceComponent: comp }
    Loader { objectName: "wide"; anchors.left: parent.left; anchors.right: parent.right; sourceComponent: comp }
}
"#,
    );
    let mut s = scene(&e, root, 300.0, 200.0);
    frame(&mut s, &e);
    let item = |name| {
        get(&e, find(&e, root, name), "item")
            .as_object()
            .expect("loaded")
    };
    let sized = find(&e, root, "sized");
    assert_eq!(num(&e, item("sized"), "width"), 100.0);
    assert_eq!(num(&e, item("sized"), "height"), 50.0);
    assert_eq!(num(&e, sized, "implicitWidth"), 11.0);
    assert_eq!(num(&e, sized, "implicitHeight"), 12.0);
    assert_eq!(num(&e, sized, "width"), 100.0);
    assert_eq!(num(&e, item("filling"), "width"), 300.0);
    assert_eq!(num(&e, item("filling"), "height"), 200.0);
    // Stretched across, free down: the Loader takes the item's height.
    let wide = find(&e, root, "wide");
    assert_eq!(num(&e, item("wide"), "width"), 300.0);
    assert_eq!(num(&e, item("wide"), "height"), 20.0);
    assert_eq!(num(&e, wide, "height"), 20.0);

    set(&e, root, "width", Value::from(250.0));
    frame(&mut s, &e);
    assert_eq!(num(&e, item("filling"), "width"), 250.0);
    assert!(e.take_warnings().is_empty());
}

const BY_SOURCE: &str = r#"
import QtQuick
Item {
    id: root
    property int loads: 0
    Component { id: comp; Item { objectName: "inline" } }
    Loader { id: loader; objectName: "loader"; source: "Kid.qml"; onLoaded: root.loads++ }
    function inline() { loader.sourceComponent = comp }
    function withProps() { loader.setSource("Kid.qml", { tag: "set", n: 9 }) }
    function plain() { loader.setSource("Kid.qml") }
}
"#;

fn by_source(e: &Engine) -> (ObjectId, ObjectId) {
    compile(
        e,
        &[
            ("Main", BY_SOURCE),
            ("Kid", KID),
            (
                "Other",
                "import QtQuick\nItem { objectName: \"other\"; width: 70 }",
            ),
        ],
    );
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    (root, find(e, root, "loader"))
}

#[test]
fn a_source_names_the_file_to_load() {
    let e = engine();
    let (root, loader) = by_source(&e);
    assert!(exists(&e, root, "kid"));
    assert_eq!(text(&e, loader, "source"), "Kid.qml");
    assert_eq!(num(&e, loader, "width"), 40.0);

    set(&e, loader, "source", Value::from("Other.qml"));
    e.process_deferred_deletes();
    assert!(!exists(&e, root, "kid"));
    assert!(exists(&e, root, "other"));
    assert_eq!(num(&e, root, "loads"), 2.0);
    assert_eq!(num(&e, loader, "width"), 70.0);

    set(&e, loader, "source", Value::from(""));
    e.process_deferred_deletes();
    assert!(!exists(&e, root, "other"));
    assert_eq!(num(&e, loader, "status"), 0.0);
    assert_eq!(num(&e, loader, "progress"), 0.0);
    assert!(get(&e, loader, "item").is_nullish());
    assert!(e.take_warnings().is_empty());
}

#[test]
fn source_and_component_replace_each_other() {
    let e = engine();
    let (root, loader) = by_source(&e);
    e.invoke_by_name(root, "inline", &[]).unwrap();
    e.process_deferred_deletes();
    assert!(!exists(&e, root, "kid"));
    assert!(exists(&e, root, "inline"));
    assert_eq!(text(&e, loader, "source"), "");

    set(&e, loader, "source", Value::from("Other.qml"));
    e.process_deferred_deletes();
    assert!(!exists(&e, root, "inline"));
    assert!(exists(&e, root, "other"));
    assert!(get(&e, loader, "sourceComponent").is_nullish());
    assert_eq!(num(&e, loader, "status"), 1.0);
    assert_eq!(num(&e, root, "loads"), 3.0);
}

#[test]
fn a_missing_file_is_an_error_with_a_warning() {
    let e = engine();
    let (root, loader) = by_source(&e);
    set(&e, loader, "source", Value::from("Missing.qml"));
    e.process_deferred_deletes();
    assert_eq!(num(&e, loader, "status"), 3.0);
    assert_eq!(num(&e, loader, "progress"), 1.0);
    assert!(get(&e, loader, "item").is_nullish());
    assert!(!exists(&e, root, "kid"));
    let warnings = e.take_warnings();
    assert!(
        warnings.iter().any(|w| w.contains("Missing.qml")),
        "{warnings:?}"
    );
    // A good source afterwards loads.
    set(&e, loader, "source", Value::from("Other.qml"));
    assert_eq!(num(&e, loader, "status"), 1.0);
}

#[test]
fn set_source_gives_first_values_that_last_until_the_source_changes() {
    let e = engine();
    let (root, loader) = by_source(&e);
    let kid = |e: &Engine| get(e, loader, "item").as_object().unwrap();
    e.invoke_by_name(root, "withProps", &[]).unwrap();
    assert_eq!(text(&e, kid(&e), "tag"), "set");
    assert_eq!(num(&e, kid(&e), "n"), 9.0);
    // The same source, loaded again.
    assert_eq!(num(&e, root, "loads"), 2.0);

    set(&e, loader, "active", Value::Bool(false));
    set(&e, loader, "active", Value::Bool(true));
    assert_eq!(text(&e, kid(&e), "tag"), "set");

    set(&e, loader, "source", Value::from("Other.qml"));
    set(&e, loader, "source", Value::from("Kid.qml"));
    assert_eq!(text(&e, kid(&e), "tag"), "kid");

    e.invoke_by_name(root, "withProps", &[]).unwrap();
    e.invoke_by_name(root, "plain", &[]).unwrap();
    assert_eq!(text(&e, kid(&e), "tag"), "kid");
    assert!(e.take_warnings().is_empty());
}

#[test]
fn an_inactive_or_empty_loader_makes_nothing() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Component { id: comp; Item { objectName: "kid"; width: 30 } }
    Loader { objectName: "empty" }
    Loader { objectName: "off"; active: false; sourceComponent: comp }
}
"#,
    );
    let (empty, off) = (find(&e, root, "empty"), find(&e, root, "off"));
    assert!(!exists(&e, root, "kid"));
    for l in [empty, off] {
        assert_eq!(num(&e, l, "status"), 0.0);
        assert!(get(&e, l, "item").is_nullish());
        assert_eq!(num(&e, l, "width"), 0.0);
    }
    set(&e, off, "active", Value::Bool(true));
    assert!(exists(&e, off, "kid"));
    assert_eq!(num(&e, off, "width"), 30.0);
}

#[test]
fn a_non_visual_object_can_be_loaded() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Component { id: comp; QtObject { objectName: "thing" } }
    Loader { objectName: "loader"; sourceComponent: comp }
}
"#,
    );
    let loader = find(&e, root, "loader");
    let thing = find(&e, root, "thing");
    assert_eq!(get(&e, loader, "item").as_object(), Some(thing));
    assert_eq!(num(&e, loader, "status"), 1.0);
    assert_eq!(list(&e, loader, "resources"), vec![]);
    assert_eq!(list(&e, loader, "data"), vec![]);
    set(&e, loader, "active", Value::Bool(false));
    e.process_deferred_deletes();
    assert!(!e.is_alive(thing));
    assert!(e.take_warnings().is_empty());
}

#[test]
fn an_item_may_unload_itself_from_its_own_handlers() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    id: root
    property int made: 0
    property string after: ""
    Component {
        id: once
        Item {
            id: kid
            objectName: "kid"
            function close() { loader.active = false; root.after = "still " + kid.objectName }
        }
    }
    Component {
        id: refuses
        Item { objectName: "refuser"; Component.onCompleted: { root.made++; loader.sourceComponent = once } }
    }
    Loader { id: loader; objectName: "loader"; sourceComponent: once }
    function refuse() { loader.sourceComponent = refuses }
}
"#,
    );
    let loader = find(&e, root, "loader");
    let kid = find(&e, root, "kid");
    e.invoke_by_name(kid, "close", &[]).unwrap();
    // The rest of the handler still saw its own object.
    assert_eq!(text(&e, root, "after"), "still kid");
    assert!(get(&e, loader, "item").is_nullish());
    e.process_deferred_deletes();
    assert!(!e.is_alive(kid));

    // An item that swaps the Loader's component while it completes is
    // dropped; the Loader ends with what it was told last.
    set(&e, loader, "active", Value::Bool(true));
    e.invoke_by_name(root, "refuse", &[]).unwrap();
    e.process_deferred_deletes();
    assert_eq!(num(&e, root, "made"), 1.0);
    assert!(!exists(&e, root, "refuser"));
    let item = get(&e, loader, "item").as_object().expect("loaded");
    assert_eq!(text(&e, item, "objectName"), "kid");
    assert_eq!(num(&e, loader, "status"), 1.0);
    assert_eq!(list(&e, loader, "children"), vec![item]);
}

// ------------------------------------------------------------ asynchronous

/// A clock on which every look takes a tenth of a millisecond, so a
/// creation job runs out of its slice after a few objects.
#[derive(Debug, Default)]
struct BusyClock(AtomicU64);

impl Clock for BusyClock {
    fn now(&self) -> Time {
        Time::from_micros(self.0.fetch_add(100, Ordering::Relaxed))
    }
}

const ASYNC: &str = r#"
import QtQuick
Item {
    id: root
    width: 200; height: 200
    property string events: ""
    Component {
        id: comp
        Rectangle {
            objectName: "kid"; width: 30; height: 20
            property bool parented: parent === loader
            Repeater0 {}
        }
    }
    Loader {
        id: loader; objectName: "loader"; asynchronous: true; sourceComponent: comp
        onStatusChanged: root.events += "status" + status + " "
        onLoaded: root.events += "loaded "
    }
}
"#;

/// `ASYNC` with `n` nested items in the component, so there is work to
/// slice.
fn async_source(n: usize) -> String {
    let kids: String = (0..n)
        .map(|i| format!("Item {{ objectName: \"k{i}\"; Item {{ width: {i} }} }}\n"))
        .collect();
    ASYNC.replace("Repeater0 {}", &kids)
}

fn busy_engine() -> Engine {
    let e = engine();
    e.set_clock(Rc::new(BusyClock::default()));
    e
}

#[test]
fn an_asynchronous_loader_makes_its_item_over_several_frames() {
    let e = busy_engine();
    let root = load(&e, &async_source(40));
    let loader = find(&e, root, "loader");
    assert_eq!(num(&e, loader, "status"), 2.0);
    assert_eq!(num(&e, loader, "progress"), 0.0);
    assert!(get(&e, loader, "item").is_nullish());
    assert!(e.has_incubations());

    let mut s = scene(&e, root, 200.0, 200.0);
    let loader_node = s.node_of(loader).unwrap();
    let mut frames = 0;
    loop {
        let out = frame(&mut s, &e);
        frames += 1;
        assert!(frames < 200, "the job never finished");
        if num(&e, loader, "status") != 2.0 {
            break;
        }
        // Still loading: the scene asks for the next frame, and nothing
        // of the half-made item shows.
        assert_eq!(out.next_wake, Some(Time::ZERO));
        assert!(s.needs_frame());
        assert!(get(&e, loader, "item").is_nullish());
        assert!(
            s.render_tree()
                .node(loader_node)
                .unwrap()
                .children()
                .is_empty()
        );
    }
    assert!(frames > 2, "one frame made everything: {frames}");
    let kid = get(&e, loader, "item").as_object().expect("loaded");
    assert_eq!(text(&e, root, "events"), "status2 status1 loaded ");
    assert_eq!(num(&e, loader, "progress"), 1.0);
    assert!(get(&e, kid, "parented").to_boolean());
    // Shown and sized in the frame that finished it.
    let node = s.node_of(kid).expect("the item is in the scene");
    assert_eq!(
        s.render_tree().node(loader_node).unwrap().children(),
        &[node]
    );
    assert_eq!(num(&e, loader, "width"), 30.0);
    assert!(s.node_of(find(&e, root, "k39")).is_some());
    // And then the scene is idle again.
    let out = frame(&mut s, &e);
    assert_eq!(out.next_wake, None);
    assert!(!s.needs_frame());
    assert!(e.take_warnings().is_empty());
}

#[test]
fn a_job_queued_outside_a_frame_wakes_the_host() {
    let e = busy_engine();
    let root = load(
        &e,
        &async_source(4).replace("asynchronous: true;", "asynchronous: true; active: false;"),
    );
    let loader = find(&e, root, "loader");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let woke = Rc::new(Cell::new(0));
    let w = Rc::clone(&woke);
    s.set_waker(move || w.set(w.get() + 1));
    assert!(!s.needs_frame());

    set(&e, loader, "active", Value::Bool(true));
    assert_eq!(num(&e, loader, "status"), 2.0);
    assert!(s.needs_frame());
    assert_eq!(woke.get(), 1);
    for _ in 0..200 {
        if !s.needs_frame() {
            break;
        }
        frame(&mut s, &e);
    }
    assert_eq!(num(&e, loader, "status"), 1.0);
    // Frames the scene asked for through `next_wake` do not wake again.
    assert_eq!(woke.get(), 1);
}

#[test]
fn switching_off_while_loading_leaves_nothing_behind() {
    let e = busy_engine();
    let root = load(&e, &async_source(40));
    let loader = find(&e, root, "loader");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    frame(&mut s, &e);
    assert_eq!(num(&e, loader, "status"), 2.0);
    // The job has allocated its root by now.
    assert!(exists(&e, root, "kid"));

    set(&e, loader, "active", Value::Bool(false));
    assert_eq!(num(&e, loader, "status"), 0.0);
    assert_eq!(list(&e, loader, "children"), vec![]);
    let out = frame(&mut s, &e);
    assert!(!exists(&e, root, "kid"));
    assert!(get(&e, loader, "item").is_nullish());
    assert_eq!(text(&e, root, "events"), "status2 status0 ");
    assert_eq!(out.next_wake, None);
    assert!(!s.needs_frame());

    // On again: a fresh job, from the start.
    set(&e, loader, "active", Value::Bool(true));
    for _ in 0..200 {
        if !s.needs_frame() {
            break;
        }
        frame(&mut s, &e);
    }
    assert_eq!(num(&e, loader, "status"), 1.0);
    assert!(s.node_of(find(&e, root, "kid")).is_some());
    assert!(e.take_warnings().is_empty());
}

#[test]
fn a_loader_destroyed_while_loading_takes_its_job_with_it() {
    let e = busy_engine();
    let root = load(&e, &async_source(40));
    let loader = find(&e, root, "loader");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    frame(&mut s, &e);
    assert_eq!(num(&e, loader, "status"), 2.0);
    let before = e.object_count();
    e.destroy(loader);
    let out = frame(&mut s, &e);
    assert!(!e.is_alive(loader));
    assert!(!exists(&e, root, "kid"));
    assert!(e.object_count() < before);
    assert_eq!(out.next_wake, None);
    assert!(!s.needs_frame());
}

#[test]
fn turning_asynchronous_off_finishes_the_load_at_once() {
    let e = busy_engine();
    let root = load(&e, &async_source(40));
    let loader = find(&e, root, "loader");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    assert_eq!(num(&e, loader, "status"), 2.0);
    set(&e, loader, "asynchronous", Value::Bool(false));
    assert_eq!(num(&e, loader, "status"), 1.0);
    let kid = get(&e, loader, "item").as_object().expect("loaded");
    frame(&mut s, &e);
    assert!(s.node_of(kid).is_some());
    // Only the finished item is left.
    assert_eq!(list(&e, loader, "children"), vec![kid]);
    let out = frame(&mut s, &e);
    assert_eq!(out.next_wake, None);
}

#[test]
fn an_asynchronous_source_that_is_missing_is_an_error() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item { Loader { objectName: "loader"; asynchronous: true; source: "Missing.qml" } }
"#,
    );
    let loader = find(&e, root, "loader");
    assert_eq!(num(&e, loader, "status"), 2.0);
    let mut s = scene(&e, root, 10.0, 10.0);
    frame(&mut s, &e);
    assert_eq!(num(&e, loader, "status"), 3.0);
    assert!(e.take_warnings().iter().any(|w| w.contains("Missing.qml")));
    let _ = (Loader::ITEM, QuickScene::new);
}
