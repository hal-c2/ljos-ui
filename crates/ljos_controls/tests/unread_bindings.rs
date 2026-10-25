//! Effects that hang on a binding only the type itself would read.
//!
//! An application's engine runs a binding when something reads it, so a
//! type whose effect starts from a property nothing else reads (a hidden
//! item's, a non-visual object's) must watch that property itself. Each
//! case flips `root.on` on such an engine and checks the log against what
//! qml6 logs for the same QML (measured with qmltestrunner, offscreen).
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::{Clock, Time, VirtualClock};

fn load(e: &Engine, source: &str) -> ObjectId {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", source);
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let mut files = ljos_engine::MapSource::new();
    files.insert("Main", c.file);
    e.set_component_source(files);
    e.create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"))
}

fn run(body: &str) -> String {
    let clock = Arc::new(VirtualClock::new());
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    e.take_log();
    e.set_clock(Rc::new(clock.clone()));
    let src = format!(
        "import QtQuick\nimport QtQuick.Controls.Basic\nItem {{\n id: root\n width: 400; height: 400\n property bool on: false\n property string log: \"\"\n{body}\n}}\n"
    );
    let root = load(&e, &src);
    let mut s = QuickScene::new(kurbo::Size::new(400.0, 400.0));
    s.set_root(&e, root).unwrap();
    let step = |s: &mut QuickScene, ms: u64| {
        clock.set(Time::from_millis(ms));
        e.advance_timers(clock.now());
        s.frame(&e, clock.now());
    };
    for ms in (0..100).step_by(16) {
        step(&mut s, ms);
    }
    e.write_by_name(root, "on", Value::Bool(true)).unwrap();
    for ms in (100..1500).step_by(16) {
        step(&mut s, ms);
    }
    let log = e.read_by_name(root, "log").to_js_string().to_string();
    let warnings = e.take_log();
    assert!(warnings.is_empty(), "{body}: {warnings:?}");
    log
}

#[test]
fn effects_run_when_nothing_else_reads_the_binding() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "popup",
            r#"Popup { visible: root.on; onOpened: root.log += "opened" }"#,
            "opened",
        ),
        (
            "popup-enter",
            r#"Popup { visible: root.on; enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 100 } } onOpened: root.log += "opened" }"#,
            "opened",
        ),
        (
            "drawer",
            r#"Drawer { width: 100; height: 400; visible: root.on; onOpened: root.log += "opened" }"#,
            "opened",
        ),
        (
            "dialog",
            r#"Dialog { visible: root.on; onOpened: root.log += "opened" }"#,
            "opened",
        ),
        (
            "menu",
            r#"Menu { visible: root.on; onOpened: root.log += "opened"; MenuItem { text: "a" } }"#,
            "opened",
        ),
        (
            "tooltip",
            r#"ToolTip { text: "t"; visible: root.on; onOpened: root.log += "opened" }"#,
            "opened",
        ),
        (
            "behavior-hidden",
            r#"Rectangle { visible: false; width: 10; height: 10; x: root.on ? 100 : 0; Behavior on x { NumberAnimation { duration: 100; onRunningChanged: root.log += "run" + running + " " } } }"#,
            "runtrue runfalse ",
        ),
        (
            "behavior-custom",
            r#"Item { property real v: root.on ? 100 : 0; Behavior on v { NumberAnimation { duration: 100; onRunningChanged: root.log += "run" + running + " " } } }"#,
            "runtrue runfalse ",
        ),
        (
            "behavior-visible",
            r#"Rectangle { width: 10; height: 10; x: root.on ? 100 : 0; Behavior on x { NumberAnimation { duration: 100; onRunningChanged: root.log += "run" + running + " " } } }"#,
            "runtrue runfalse ",
        ),
        (
            "timer",
            r#"Timer { interval: 50; running: root.on; onTriggered: root.log += "tick" }"#,
            "tick",
        ),
        (
            "anim-running",
            r#"NumberAnimation { target: root; property: "width"; to: 300; duration: 50; running: root.on; onFinished: root.log += "done" }"#,
            "done",
        ),
        (
            "state",
            r#"Item { id: s; state: root.on ? "a" : ""; states: State { name: "a"; StateChangeScript { script: root.log += "entered" } } }"#,
            "entered",
        ),
        (
            "state-when",
            r#"Item { states: State { name: "a"; when: root.on; StateChangeScript { script: root.log += "entered" } } }"#,
            "entered",
        ),
        (
            "repeater-hidden",
            r#"Item { visible: false; Repeater { model: root.on ? 2 : 0; onItemAdded: root.log += "added "; Item {} } }"#,
            "added added ",
        ),
        (
            "instantiator",
            r#"Instantiator { model: root.on ? 1 : 0; onObjectAdded: root.log += "added"; QtObject {} }"#,
            "added",
        ),
        (
            "loader-hidden",
            r#"Item { visible: false; Loader { active: root.on; sourceComponent: Item {} onLoaded: root.log += "loaded" } }"#,
            "loaded",
        ),
        (
            "connections",
            r#"Item { id: src; signal ping; Connections { target: root.on ? src : null; function onPing() { root.log += "ping" } } Timer { interval: 300; running: true; onTriggered: src.ping() } }"#,
            "ping",
        ),
        (
            "connections-enabled",
            r#"Item { id: src2; signal ping; Connections { target: src2; enabled: root.on; function onPing() { root.log += "ping" } } Timer { interval: 300; running: true; onTriggered: src2.ping() } }"#,
            "ping",
        ),
        (
            "focus-hidden",
            r#"Item { visible: true; Item { focus: root.on; onActiveFocusChanged: root.log += "af" + activeFocus } }"#,
            "aftrue",
        ),
        (
            "listview-current",
            r#"ListView { width: 100; height: 100; model: 3; delegate: Item { width: 100; height: 20 } currentIndex: root.on ? 2 : 0; onCurrentItemChanged: root.log += "cur" }"#,
            "curcur",
        ),
        (
            "swipeview",
            r#"SwipeView { width: 100; height: 100; currentIndex: root.on ? 1 : 0; onCurrentItemChanged: if (root.on) root.log += "cur" + currentIndex; Item {} Item {} }"#,
            "cur1",
        ),
        (
            "binding-el",
            r#"Item { id: t; property int n: 0; onNChanged: root.log += "n" + n } Binding { target: t; property: "n"; value: 5; when: root.on }"#,
            "n5",
        ),
        (
            "window-hidden",
            r#"Window { visible: root.on; onVisibleChanged: root.log += "vis" }"#,
            "vis",
        ),
        (
            "drag-hidden",
            r#"Item { visible: false; Drag.active: root.on; Drag.onActiveChanged: root.log += "da" }"#,
            "da",
        ),
        (
            "flick-content",
            r#"Flickable { visible: false; width: 50; height: 50; contentHeight: 500; contentY: root.on ? 100 : 0; onContentYChanged: root.log += "cy" }"#,
            "cy",
        ),
        (
            "busy",
            r#"BusyIndicator { visible: false; running: root.on; onRunningChanged: root.log += "br" }"#,
            "brbr",
        ),
        (
            "stackview",
            r#"StackView { initialItem: root.on ? c : null; Component { id: c; Item {} } onCurrentItemChanged: root.log += "cur" }"#,
            "",
        ),
        (
            "shortcut",
            r#"Shortcut { sequence: "Ctrl+K"; enabled: root.on; onEnabledChanged: root.log += "en" }"#,
            "enen",
        ),
        (
            "behavior-member",
            r#"Item { property font f; f.pixelSize: root.on ? 30 : 10; Behavior on f.pixelSize { NumberAnimation { duration: 100; onRunningChanged: root.log += "run" + running + " " } } }"#,
            "runtrue runfalse ",
        ),
    ];
    // SwipeView and TabBar signal their first current item where qml6
    // signals nothing while it creates them, so that one counts only the
    // flip.
    let mut wrong = String::new();
    for (name, body, qt) in cases {
        let got = run(body);
        if got != *qt {
            wrong += &format!("{name}: got {got:?}, qml6 logs {qt:?}\n");
        }
    }
    assert!(wrong.is_empty(), "{wrong}");
}
