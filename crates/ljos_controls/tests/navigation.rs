//! What the scenarios in `features/controls/` cannot see of the
//! navigation controls: StackView, SwipeView, TabBar.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::{Clock, Time, VirtualClock};

fn engine() -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    e.take_log();
    e
}

/// Compiles the files (`Main` first) and creates `Main`.
fn load(e: &Engine, files: &[(&str, &str)]) -> ObjectId {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    for &(name, source) in files {
        project.add(name, source);
    }
    let mut source = ljos_engine::MapSource::new();
    for &(name, _) in files {
        let c = compile_file(&project, name).unwrap_or_else(|d| panic!("{d:?}"));
        assert!(!c.has_errors(), "{name}: {:?}", c.diagnostics);
        source.insert(name, c.file);
    }
    e.set_component_source(source);
    e.create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"))
}

fn find(e: &Engine, root: ObjectId, name: &str) -> ObjectId {
    ljos_drive::find_below(e, root, name).unwrap_or_else(|| panic!("no object named {name}"))
}

fn scene(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = QuickScene::new(kurbo::Size::new(300.0, 200.0));
    s.set_root(e, root).unwrap();
    settle(&mut s, e);
    s
}

fn settle(s: &mut QuickScene, e: &Engine) {
    ljos_drive::settle_at(e, s, Time::ZERO).unwrap();
}

fn call(e: &Engine, obj: ObjectId, name: &str) {
    e.invoke_by_name(obj, name, &[])
        .unwrap_or_else(|err| panic!("{name}: {err}"));
}

fn number(e: &Engine, obj: ObjectId, name: &str) -> f64 {
    e.read_by_name(obj, name).to_number()
}

fn text(e: &Engine, obj: ObjectId, name: &str) -> String {
    e.read_by_name(obj, name).to_js_string().to_string()
}

const STACK: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    id: root; width: 300; height: 200
    property string log: ""
    property var made: null
    Component { id: page; Rectangle { objectName: "made" } }
    Rectangle { id: sized; objectName: "sized"; width: 40; height: 30 }
    StackView {
        id: stack; objectName: "stack"; anchors.fill: parent
        initialItem: Rectangle { objectName: "first" }
    }
    function pushPage() { made = stack.push(page, {}, StackView.Immediate) }
    function pushSized() { stack.push(sized, {}, StackView.Immediate) }
    function pushFile() { stack.push("Page.qml", {}, StackView.Immediate) }
    function popPage() { stack.pop(StackView.Immediate) }
}
"#;

const PAGE: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Rectangle {
    objectName: "file"
    StackView.onActivated: root.log += "activated "
}
"#;

#[test]
fn a_page_fills_the_view_and_follows_it() {
    let e = engine();
    let root = load(&e, &[("Main", STACK)]);
    let mut s = scene(&e, root);
    let first = find(&e, root, "first");
    assert_eq!(number(&e, first, "width"), 300.0);
    call(&e, root, "pushPage");
    settle(&mut s, &e);
    let made = find(&e, root, "made");
    assert_eq!(number(&e, made, "width"), 300.0);
    assert_eq!(number(&e, made, "height"), 200.0);
    e.write_by_name(root, "width", Value::Real(250.0)).unwrap();
    settle(&mut s, &e);
    assert_eq!(number(&e, made, "width"), 250.0);
}

#[test]
fn a_page_with_a_size_keeps_it_and_gets_its_parent_back() {
    let e = engine();
    let root = load(&e, &[("Main", STACK)]);
    let mut s = scene(&e, root);
    let sized = find(&e, root, "sized");
    call(&e, root, "pushSized");
    settle(&mut s, &e);
    assert_eq!(number(&e, sized, "width"), 40.0);
    assert_eq!(number(&e, sized, "height"), 30.0);
    assert_eq!(e.visual_parent(sized), Some(find(&e, root, "stack")));
    call(&e, root, "popPage");
    settle(&mut s, &e);
    // A page the view did not make is not destroyed: it goes home.
    assert!(e.is_alive(sized));
    assert_eq!(e.visual_parent(sized), Some(root));
}

#[test]
fn a_page_made_by_the_view_dies_when_it_leaves() {
    let e = engine();
    let root = load(&e, &[("Main", STACK)]);
    let mut s = scene(&e, root);
    call(&e, root, "pushPage");
    settle(&mut s, &e);
    let made = find(&e, root, "made");
    call(&e, root, "popPage");
    settle(&mut s, &e);
    e.process_deferred_deletes();
    assert!(!e.is_alive(made));
}

#[test]
fn a_page_from_a_file_sees_the_names_around_the_view() {
    let e = engine();
    let root = load(&e, &[("Main", STACK), ("Page", PAGE)]);
    let mut s = scene(&e, root);
    call(&e, root, "pushFile");
    settle(&mut s, &e);
    let file = find(&e, root, "file");
    assert_eq!(number(&e, file, "width"), 300.0);
    assert_eq!(text(&e, root, "log"), "activated ");
}

const SWIPE: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    id: root; width: 300; height: 200
    SwipeView {
        id: view; objectName: "view"; anchors.fill: parent
        Rectangle { objectName: "a"; color: "red" }
        Rectangle { objectName: "b"; color: "green" }
        Item { objectName: "c"; width: 50 }
    }
    SwipeView { y: 300 }
}
"#;

#[test]
fn swipe_pages_follow_the_size_of_the_view() {
    let e = engine();
    let root = load(&e, &[("Main", SWIPE)]);
    let mut s = scene(&e, root);
    let b = find(&e, root, "b");
    assert_eq!(number(&e, b, "x"), 300.0);
    assert_eq!(number(&e, b, "width"), 300.0);
    e.write_by_name(root, "width", Value::Real(200.0)).unwrap();
    settle(&mut s, &e);
    assert_eq!(number(&e, b, "width"), 200.0);
    assert_eq!(number(&e, b, "x"), 200.0);
}

const TABS: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    width: 300; height: 200
    Column {
        width: 300
        TabBar {
            id: bar; objectName: "bar"; width: parent.width
            currentIndex: view.currentIndex
            TabButton { text: "Home" }
            TabButton { text: "Mail" }
        }
        SwipeView {
            id: view; objectName: "view"; width: parent.width; height: 150
            currentIndex: bar.currentIndex
            Item {}
            Item {}
        }
    }
}
"#;

/// The pattern the documentation of both gives: each follows the other,
/// and neither is a binding loop (as on qml6): the change stops at the
/// binding it would come back to, which gives the value it has.
#[test]
fn a_tab_bar_and_a_swipe_view_follow_each_other() {
    let e = engine();
    let root = load(&e, &[("Main", TABS)]);
    let mut s = scene(&e, root);
    let (bar, view) = (find(&e, root, "bar"), find(&e, root, "view"));
    assert_eq!(e.take_warnings(), Vec::<String>::new());
    assert_eq!(number(&e, bar, "currentIndex"), 0.0);
    assert_eq!(number(&e, view, "currentIndex"), 0.0);
    call_with(&e, bar, "setCurrentIndex", &[Value::Int(1)]);
    settle(&mut s, &e);
    assert_eq!(number(&e, bar, "currentIndex"), 1.0);
    assert_eq!(number(&e, view, "currentIndex"), 1.0);
    call(&e, view, "decrementCurrentIndex");
    settle(&mut s, &e);
    assert_eq!(number(&e, bar, "currentIndex"), 0.0);
    assert_eq!(number(&e, view, "currentIndex"), 0.0);
    assert_eq!(e.take_warnings(), Vec::<String>::new());
}

fn call_with(e: &Engine, obj: ObjectId, name: &str, args: &[Value]) {
    e.invoke_by_name(obj, name, args)
        .unwrap_or_else(|err| panic!("{name}: {err}"));
}

const SPLIT: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    width: 400; height: 300
    SplitView {
        id: sv; objectName: "sv"; anchors.fill: parent
        Rectangle { objectName: "a"; implicitWidth: 100 }
        Rectangle { objectName: "b"; SplitView.fillWidth: true }
        Rectangle { objectName: "c"; implicitWidth: 70 }
    }
}
"#;

/// A split view's handles are children of the view, between the items,
/// and as long along it as the layout leaves room for.
#[test]
fn a_split_view_has_a_handle_between_each_two_items() {
    let e = engine();
    let root = load(&e, &[("Main", SPLIT)]);
    let _s = scene(&e, root);
    let sv = find(&e, root, "sv");
    let content = e.read_by_name(sv, "contentItem").as_object();
    let handles: Vec<(f64, f64)> = e
        .visual_children(sv)
        .into_iter()
        .filter(|h| Some(*h) != content)
        .map(|h| (number(&e, h, "x"), number(&e, h, "width")))
        .collect();
    assert_eq!(
        handles,
        vec![(100.0, 6.0), (324.0, 6.0)],
        "{:?}",
        e.take_log()
    );
    let b = find(&e, root, "b");
    assert_eq!(number(&e, b, "x"), 106.0);
    assert_eq!(number(&e, b, "width"), 218.0);
}

const COVERED: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    id: root; width: 300; height: 200
    StackView {
        id: stack; anchors.fill: parent
        initialItem: Item {
            objectName: "covered"
            property int ticks: 0
            // Moves nothing anybody sees, so only freezing stops it.
            QtObject { id: model; objectName: "model"; property real phase: 0 }
            NumberAnimation {
                target: model; property: "phase"; from: 0; to: 1
                duration: 1000; loops: Animation.Infinite; running: true
            }
            Timer { interval: 100; repeat: true; running: true; onTriggered: parent.ticks++ }
        }
    }
    Component { id: top; Rectangle { color: "red" } }
    function cover() { stack.push(top, {}, StackView.Immediate) }
    function uncover() { stack.pop(StackView.Immediate) }
}
"#;

/// ARCHITECTURE.md §8.3: a page under the top one is frozen. Its endless
/// animation and its repeating timer ask for no frame and do nothing
/// until it is shown again; then they go on.
#[test]
fn a_covered_page_costs_no_frame_work() {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let root = load(&e, &[("Main", COVERED)]);
    let mut s = QuickScene::new(kurbo::Size::new(300.0, 200.0));
    s.set_root(&e, root).unwrap();
    let frame = |s: &mut QuickScene, ms: u64| {
        clock.advance(Duration::from_millis(ms));
        s.frame(&e, clock.now())
    };
    let covered = find(&e, root, "covered");
    let model = find(&e, root, "model");
    frame(&mut s, 0);
    let out = frame(&mut s, 250);
    assert!(out.next_wake.is_some(), "a page on top is animated");
    assert!(number(&e, model, "phase") > 0.0);
    assert_eq!(number(&e, covered, "ticks"), 2.0);

    call(&e, root, "cover");
    let mut out = frame(&mut s, 0);
    for _ in 0..4 {
        if out.next_wake.is_none() {
            break;
        }
        out = frame(&mut s, 0);
    }
    assert_eq!(out.next_wake, None, "nothing wakes the host");
    assert!(e.is_frozen(covered));
    let phase = number(&e, model, "phase");
    let ticks = number(&e, covered, "ticks");
    // Frames that come for other reasons do not move it either.
    frame(&mut s, 500);
    frame(&mut s, 500);
    assert_eq!(number(&e, model, "phase"), phase);
    assert_eq!(number(&e, covered, "ticks"), ticks);

    call(&e, root, "uncover");
    assert!(!e.is_frozen(covered));
    let out = frame(&mut s, 0);
    assert!(out.next_wake.is_some(), "it asks for frames again");
    assert_eq!(
        number(&e, covered, "ticks"),
        ticks + 1.0,
        "the missed ticks, once"
    );
    frame(&mut s, 30);
    assert_ne!(number(&e, model, "phase"), phase, "the animation goes on");
}

const SWIPE_PAGES: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    id: root; width: 300; height: 200
    property int pulse: 0
    SwipeView {
        id: view; objectName: "view"; anchors.fill: parent
        Item {
            objectName: "first"
            property int ticks: 0
            Timer { interval: 100; repeat: true; running: true; onTriggered: parent.ticks++ }
        }
        Item {
            objectName: "second"
            property int ticks: 0
            property int seenPulse: root.pulse
            property int changes: 0
            onSeenPulseChanged: changes++
            Timer { interval: 100; repeat: true; running: true; onTriggered: parent.ticks++ }
        }
    }
    function takeSecond() { return view.takeItem(1) }
}
"#;

/// ARCHITECTURE.md §8.3: a SwipeView page that is not current is frozen
/// while the view rests. Its timer does not fire and its watches are
/// parked; made current, it catches up once. A page taken out thaws.
#[test]
fn a_non_current_swipe_page_is_frozen_until_made_current() {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let root = load(&e, &[("Main", SWIPE_PAGES)]);
    let mut s = QuickScene::new(kurbo::Size::new(300.0, 200.0));
    s.set_root(&e, root).unwrap();
    let frame = |s: &mut QuickScene, ms: u64| {
        clock.advance(Duration::from_millis(ms));
        s.frame(&e, clock.now())
    };
    let view = find(&e, root, "view");
    let first = find(&e, root, "first");
    let second = find(&e, root, "second");

    frame(&mut s, 0);
    assert!(!e.is_frozen(first), "the current page is awake");
    assert!(e.is_frozen(second), "a non-current page is frozen");

    e.write_by_name(root, "pulse", Value::Int(1)).unwrap();
    e.write_by_name(root, "pulse", Value::Int(2)).unwrap();
    frame(&mut s, 250);
    assert_eq!(number(&e, first, "ticks"), 2.0);
    assert_eq!(number(&e, second, "ticks"), 0.0);
    assert_eq!(number(&e, second, "changes"), 0.0);
    // A lazy binding still answers reads with the latest value.
    assert_eq!(number(&e, second, "seenPulse"), 2.0);

    e.write_by_name(view, "currentIndex", Value::Int(1))
        .unwrap();
    frame(&mut s, 0);
    assert!(e.is_frozen(first), "the previous page is now frozen");
    assert!(!e.is_frozen(second), "the new current page is thawed");
    assert_eq!(number(&e, second, "ticks"), 1.0, "the missed tick, once");
    assert_eq!(number(&e, second, "changes"), 1.0, "the watch, coalesced");

    e.write_by_name(view, "currentIndex", Value::Int(0))
        .unwrap();
    frame(&mut s, 0);
    assert!(e.is_frozen(second));
    call(&e, root, "takeSecond");
    assert!(!e.is_frozen(second), "a taken page is thawed");
}

/// A StackView whose pages are files, replaced again and again as an
/// application's menu does: every page replaced is gone with all it made.
#[test]
fn pages_replaced_by_file_leave_nothing_behind() {
    let e = engine();
    let main = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    id: root
    width: 300; height: 200
    property var rooms: [1, 2, 3]
    property int turn: 0
    StackView { id: stack; objectName: "stack"; anchors.fill: parent; initialItem: "A.qml" }
    function next() {
        turn += 1
        stack.replace((turn % 2 ? "B" : "A") + ".qml", {"rooms": root.rooms}, StackView.Immediate)
    }
}
"#;
    let page = r#"
import QtQuick
import QtQuick.Controls.Basic
Page {
    property var rooms
    header: Label { text: "rooms " + rooms.length }
    Column {
        Repeater { model: rooms; delegate: Button { text: modelData } }
        Canvas { width: 20; height: 20; onPaint: getContext("2d").fillRect(0, 0, 4, 4) }
        SwipeView { width: 100; height: 50; Item {} Item {} }
        ComboBox { model: rooms }
    }
}
"#;
    let root = load(&e, &[("Main", main), ("A", page), ("B", page)]);
    let mut s = scene(&e, root);
    let mut counts = Vec::new();
    for _ in 0..20 {
        call(&e, root, "next");
        e.process_deferred_deletes();
        settle(&mut s, &e);
        counts.push((e.object_count(), e.node_count()));
    }
    // A first replace may make what is made once; after it the counts
    // must stay where they are.
    assert_eq!(counts[19], counts[3], "{counts:?}");
}
