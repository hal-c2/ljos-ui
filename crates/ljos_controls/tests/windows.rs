//! What the scenarios in `features/window/` cannot see of windows: the
//! side a host talks to.

use std::cell::Cell;
use std::rc::Rc;

use ljos_controls::{
    ApplicationState, FRAMELESS, Modality, ResizeEdges, STAYS_ON_TOP, ScreenSpec, WindowRequest,
    WindowShow, WindowSystem,
};
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::Time;

fn engine() -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    e.take_log();
    e
}

/// An engine whose windows a host shows, and how often the host was told
/// that something changed.
fn hosted() -> (Engine, Rc<Cell<u32>>) {
    let e = engine();
    let told = Rc::new(Cell::new(0));
    let t = told.clone();
    WindowSystem::of(&e).set_host(move || t.set(t.get() + 1));
    (e, told)
}

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

fn find(e: &Engine, root: ObjectId, name: &str) -> ObjectId {
    ljos_drive::find_below(e, root, name).unwrap_or_else(|| panic!("no object named {name}"))
}

fn scene(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = QuickScene::new(kurbo::Size::new(400.0, 400.0));
    s.set_root(e, root).unwrap();
    ljos_drive::settle_at(e, &mut s, Time::ZERO).unwrap();
    s
}

fn flag(e: &Engine, o: ObjectId, name: &str) -> bool {
    e.read_by_name(o, name).to_boolean()
}

fn int(e: &Engine, o: ObjectId, name: &str) -> i32 {
    e.read_by_name(o, name).to_number() as i32
}

fn call(e: &Engine, o: ObjectId, method: &str) -> Value {
    e.invoke_by_name(o, method, &[])
        .unwrap_or_else(|err| panic!("{method}: {err}"))
}

const TWO: &str = r#"
import QtQuick
Window {
    id: main
    objectName: "main"
    title: "Main"
    width: 320; height: 240
    minimumWidth: 100; maximumHeight: 600
    visible: true
    color: "red"
    TextInput { objectName: "mainInput"; focus: true }
    Window {
        id: tool
        objectName: "tool"
        title: "Tool"
        width: 200; height: 100
        x: 40; y: 50
        flags: Qt.Tool | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint
        modality: Qt.WindowModal
        property int asked: 0
        property bool refuse: false
        onClosing: (close) => { asked += 1; close.accepted = !refuse }
        TextInput { objectName: "toolInput"; focus: true }
    }
}
"#;

#[test]
fn without_a_host_a_window_stays_where_it_was_declared() {
    let e = engine();
    let main = load(&e, TWO);
    let tool = find(&e, main, "tool");
    assert!(!WindowSystem::of(&e).is_hosted());
    assert_eq!(e.visual_parent(tool), Some(main));
    assert_eq!(WindowSystem::of(&e).windows(&e), vec![tool, main]);
}

#[test]
fn a_host_gets_every_window_as_a_tree_of_its_own() {
    let (e, told) = hosted();
    let main = load(&e, TWO);
    let tool = find(&e, main, "tool");
    assert_eq!(e.visual_parent(tool), None, "the tool window was detached");
    assert!(!e.visual_children(main).contains(&tool));
    // Ownership is what it was: the tool window dies with the main one.
    assert_eq!(e.owner(tool), Some(main));
    assert!(told.get() > 0, "the host heard of the windows");
    // Where it was declared still says what it belongs to.
    let sys = WindowSystem::of(&e);
    assert_eq!(sys.spec(&e, tool).unwrap().transient_parent, Some(main));
    assert_eq!(sys.spec(&e, main).unwrap().transient_parent, None);
}

#[test]
fn a_spec_says_what_the_window_wants() {
    let (e, _) = hosted();
    let main = load(&e, TWO);
    let tool = find(&e, main, "tool");
    let sys = WindowSystem::of(&e);

    let m = sys.spec(&e, main).unwrap();
    assert_eq!(m.title, "Main");
    assert_eq!(m.size, [320.0, 240.0]);
    assert_eq!(m.position, None, "nobody gave the main window a place");
    assert_eq!(m.min_size, [100.0, 0.0]);
    assert_eq!(m.max_size, [None, Some(600.0)]);
    assert!(m.visible);
    assert_eq!(m.show, WindowShow::Windowed);
    assert_eq!(m.modality, Modality::NonModal);
    assert_eq!(m.color, [255, 0, 0, 255]);

    let t = sys.spec(&e, tool).unwrap();
    assert!(!t.visible);
    assert_eq!(t.position, Some([40.0, 50.0]));
    assert_ne!(t.flags & FRAMELESS, 0);
    assert_ne!(t.flags & STAYS_ON_TOP, 0);
    assert_eq!(t.flags & ljos_controls::KIND_MASK, 11, "Qt.Tool");
    assert_eq!(t.modality, Modality::Window);

    assert!(sys.spec(&e, find(&e, main, "mainInput")).is_none());
}

#[test]
fn the_host_is_told_when_a_window_wants_something_else() {
    let (e, told) = hosted();
    let main = load(&e, TWO);
    let sys = WindowSystem::of(&e);
    let before = told.get();
    e.write_by_name(main, "title", Value::String("Other".into()))
        .unwrap();
    assert!(told.get() > before);
    assert_eq!(sys.spec(&e, main).unwrap().title, "Other");

    let before = told.get();
    call(&e, main, "showMaximized");
    assert!(told.get() > before);
    assert_eq!(sys.spec(&e, main).unwrap().show, WindowShow::Maximized);
    // The platform decides how large that is: the size is as it was.
    assert_eq!(sys.spec(&e, main).unwrap().size, [320.0, 240.0]);

    // A place given later is a place asked for.
    assert_eq!(sys.spec(&e, main).unwrap().position, None);
    e.write_by_name(main, "x", Value::Int(7)).unwrap();
    assert_eq!(sys.spec(&e, main).unwrap().position, Some([7.0, 0.0]));
}

#[test]
fn what_the_platform_made_of_a_window_is_what_the_window_says() {
    let (e, _) = hosted();
    let main = load(&e, TWO);
    let sys = WindowSystem::of(&e);

    sys.resized(&e, main, [500.0, 300.0]);
    assert_eq!(int(&e, main, "width"), 500);
    assert_eq!(int(&e, main, "height"), 300);

    sys.moved(&e, main, [12.0, 34.0]);
    assert_eq!(int(&e, main, "x"), 12);
    assert_eq!(int(&e, main, "y"), 34);
    assert_eq!(
        sys.spec(&e, main).unwrap().position,
        None,
        "where the platform put it is not a place the application asked for"
    );

    sys.shown_as(&e, main, WindowShow::Maximized);
    assert_eq!(int(&e, main, "visibility"), 4);
    assert!(flag(&e, main, "visible"));
    sys.shown_as(&e, main, WindowShow::Windowed);
    assert_eq!(int(&e, main, "visibility"), 2);

    // A hidden window is not shown some way.
    let tool = find(&e, main, "tool");
    sys.shown_as(&e, tool, WindowShow::Maximized);
    assert_eq!(int(&e, tool, "visibility"), 0);
}

#[test]
fn the_user_closing_a_window_asks_the_application() {
    let (e, _) = hosted();
    let main = load(&e, TWO);
    let tool = find(&e, main, "tool");
    let sys = WindowSystem::of(&e);
    call(&e, tool, "show");
    e.write_by_name(tool, "refuse", Value::Bool(true)).unwrap();

    assert!(!sys.request_close(&e, tool));
    assert_eq!(int(&e, tool, "asked"), 1);
    assert!(flag(&e, tool, "visible"));
    assert_eq!(sys.closed_count(), 0);

    e.write_by_name(tool, "refuse", Value::Bool(false)).unwrap();
    assert!(sys.request_close(&e, tool));
    assert!(!flag(&e, tool, "visible"));
    assert_eq!(sys.closed_count(), 1);

    // Hiding is not closing.
    call(&e, main, "hide");
    assert_eq!(sys.closed_count(), 1);
}

#[test]
fn what_is_no_property_reaches_the_host_as_a_request() {
    let (e, told) = hosted();
    let main = load(&e, TWO);
    let sys = WindowSystem::of(&e);
    let before = told.get();
    call(&e, main, "requestActivate");
    call(&e, main, "raise");
    call(&e, main, "lower");
    e.invoke_by_name(main, "alert", &[Value::Int(0)]).unwrap();
    assert!(told.get() > before);
    assert_eq!(
        sys.take_requests(),
        vec![
            (main, WindowRequest::Activate),
            (main, WindowRequest::Raise),
            (main, WindowRequest::Lower),
            (main, WindowRequest::Alert),
        ]
    );
    assert!(sys.take_requests().is_empty());
}

#[test]
fn a_host_is_asked_to_move_or_resize_from_the_press() {
    let (e, _) = hosted();
    let main = load(&e, TWO);
    let sys = WindowSystem::of(&e);
    assert_eq!(call(&e, main, "startSystemMove"), Value::Bool(true));
    let resize = |edges: i32| {
        e.invoke_by_name(main, "startSystemResize", &[Value::Int(edges)])
            .unwrap()
    };
    // Qt.TopEdge | Qt.LeftEdge: a corner.
    assert_eq!(resize(1 | 2), Value::Bool(true));
    // No edge, opposite edges and bits that are no edge ask for nothing.
    for refused in [0, 1 | 8, 2 | 4, 16] {
        assert_eq!(resize(refused), Value::Bool(false), "{refused}");
    }
    let corner = ResizeEdges {
        top: true,
        left: true,
        right: false,
        bottom: false,
    };
    assert_eq!(
        sys.take_requests(),
        vec![
            (main, WindowRequest::StartMove),
            (main, WindowRequest::StartResize(corner))
        ]
    );
}

#[test]
fn a_modal_window_keeps_the_user_out_of_the_others() {
    let (e, _) = hosted();
    let main = load(&e, TWO);
    let tool = find(&e, main, "tool");
    let sys = WindowSystem::of(&e);
    assert!(!sys.blocked(&e, main), "the modal window is hidden");
    call(&e, tool, "show");
    assert!(
        sys.blocked(&e, main),
        "window modal over what it belongs to"
    );
    assert!(!sys.blocked(&e, tool));

    // Belonging to nothing, it blocks nothing; application modal, all.
    e.write_by_name(tool, "transientParent", Value::Null)
        .unwrap();
    assert!(!sys.blocked(&e, main));
    e.write_by_name(tool, "modality", Value::Int(2)).unwrap();
    assert!(sys.blocked(&e, main));
    call(&e, tool, "hide");
    assert!(!sys.blocked(&e, main));
}

#[test]
fn each_hosted_window_has_its_own_focus_and_the_active_one_gets_the_keys() {
    let (e, _) = hosted();
    let main = load(&e, TWO);
    let tool = find(&e, main, "tool");
    e.write_by_name(tool, "transientParent", Value::Null)
        .unwrap();
    call(&e, tool, "show");
    let mut main_scene = scene(&e, main);
    let mut tool_scene = scene(&e, tool);
    let (mi, ti) = (find(&e, main, "mainInput"), find(&e, main, "toolInput"));

    // A scene is active until its host says otherwise.
    assert!(flag(&e, mi, "activeFocus"));
    assert!(flag(&e, ti, "activeFocus"));
    assert!(flag(&e, main, "active") && flag(&e, tool, "active"));

    tool_scene.set_window_active(&e, false);
    assert!(flag(&e, mi, "activeFocus"));
    assert!(!flag(&e, ti, "activeFocus"));
    assert!(
        flag(&e, ti, "focus"),
        "it gets the keys back with the window"
    );
    assert!(flag(&e, main, "active"));
    assert!(!flag(&e, tool, "active"));
    assert_eq!(
        e.read_by_name(main, "activeFocusItem").as_object(),
        Some(mi)
    );
    assert_eq!(e.read_by_name(tool, "activeFocusItem").as_object(), None);

    main_scene.set_window_active(&e, false);
    tool_scene.set_window_active(&e, true);
    assert!(!flag(&e, main, "active"));
    assert!(flag(&e, tool, "active"));
    assert_eq!(
        e.read_by_name(tool, "activeFocusItem").as_object(),
        Some(ti)
    );
}

#[test]
fn a_window_is_active_with_the_window_that_belongs_to_it() {
    let (e, _) = hosted();
    let main = load(&e, TWO);
    let tool = find(&e, main, "tool");
    call(&e, tool, "show");
    let mut main_scene = scene(&e, main);
    let _tool_scene = scene(&e, tool);
    main_scene.set_window_active(&e, false);
    assert!(flag(&e, tool, "active"));
    assert!(flag(&e, main, "active"), "the keys are in its tool window");
}

#[test]
fn the_host_says_which_screens_there_are() {
    let (e, _) = hosted();
    let main = load(&e, TWO);
    let sys = WindowSystem::of(&e);
    // Before the host said anything there is one of 800 by 800.
    let first = e.read_by_name(main, "screen").as_object().unwrap();
    assert_eq!(int(&e, first, "width"), 800);

    let laptop = ScreenSpec {
        name: "eDP-1".into(),
        size: [1440.0, 900.0],
        position: [0.0, 0.0],
        scale: 2.0,
    };
    let desk = ScreenSpec {
        name: "DP-3".into(),
        size: [2560.0, 1440.0],
        position: [1440.0, 0.0],
        scale: 1.0,
    };
    sys.set_screens(&e, &[laptop.clone(), desk]);
    // The first display keeps its object and says the new things.
    assert_eq!(e.read_by_name(main, "screen").as_object(), Some(first));
    assert_eq!(int(&e, first, "width"), 1440);
    assert_eq!(e.read_by_name(first, "devicePixelRatio").to_number(), 2.0);
    assert_eq!(
        e.read_by_name(first, "name").to_js_string().as_str(),
        "eDP-1"
    );

    sys.set_window_screen(&e, main, 1);
    let second = e.read_by_name(main, "screen").as_object().unwrap();
    assert_ne!(second, first);
    assert_eq!(int(&e, second, "virtualX"), 1440);

    // The display goes: the window is on the first one again.
    sys.set_screens(&e, &[laptop]);
    assert_eq!(e.read_by_name(main, "screen").as_object(), Some(first));
}

/// An object in no window reads the primary display, and a binding on
/// it is evaluated again while the host is still saying what changed.
#[test]
fn what_is_in_no_window_follows_the_primary_screen() {
    let (e, _) = hosted();
    let root = load(
        &e,
        r#"
import QtQuick
QtObject {
    property int width: Screen.width
    property bool portrait: Screen.width < Screen.height
    property int turned: 0
    onPortraitChanged: turned++
}
"#,
    );
    assert_eq!(int(&e, root, "width"), 800);
    let tall = ScreenSpec {
        size: [1080.0, 1920.0],
        ..ScreenSpec::default()
    };
    WindowSystem::of(&e).set_screens(&e, &[tall]);
    assert_eq!(int(&e, root, "width"), 1080);
    assert!(flag(&e, root, "portrait"));
    assert_eq!(int(&e, root, "turned"), 1);
}

#[test]
fn a_binding_on_the_screen_follows_the_display_it_reads() {
    let (e, _) = hosted();
    // Not an item: its Screen is the primary display's, looked up again
    // as the display changes, by a binding something follows.
    let root = load(
        &e,
        r#"
import QtQuick
QtObject {
    property int w: Screen.width
    property string told: ""
    onWChanged: told += w + " "
}
"#,
    );
    assert_eq!(int(&e, root, "w"), 800);
    let laptop = ScreenSpec {
        size: [1440.0, 900.0],
        ..ScreenSpec::default()
    };
    WindowSystem::of(&e).set_screens(&e, &[laptop]);
    // As qml6, it is told of the first display as it starts.
    assert_eq!(
        e.read_by_name(root, "told").to_js_string().as_str(),
        "800 1440 "
    );
    assert_eq!(int(&e, root, "w"), 1440);
}

#[test]
fn the_application_says_what_the_host_says() {
    let (e, _) = hosted();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    property int state: Application.state
    property bool active: Application.active
    property int screens: Application.screens.length
    property int args: Application.arguments.length
}
"#,
    );
    let sys = WindowSystem::of(&e);
    // A list from the start: nothing was said of how the program was started.
    assert_eq!(int(&e, root, "args"), 0);
    assert_eq!(int(&e, root, "state"), 4);
    assert!(flag(&e, root, "active"));
    assert_eq!(int(&e, root, "screens"), 1);

    sys.set_application_state(&e, ApplicationState::Inactive);
    assert_eq!(int(&e, root, "state"), 2);
    assert!(!flag(&e, root, "active"));

    sys.set_screens(&e, &[ScreenSpec::default(), ScreenSpec::default()]);
    assert_eq!(int(&e, root, "screens"), 2);
    sys.set_screens(&e, &[ScreenSpec::default()]);
    assert_eq!(int(&e, root, "screens"), 1);
}

#[test]
fn the_application_s_direction_is_the_language_s() {
    let (e, _) = hosted();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    property int direction: Application.layoutDirection
    property int asked: Qt.application.layoutDirection
}
"#,
    );
    assert_eq!(int(&e, root, "direction"), 0);
    e.set_ui_language("ar");
    assert_eq!(int(&e, root, "direction"), 1, "Qt.RightToLeft");
    assert_eq!(int(&e, root, "asked"), 1, "the same object");
    e.set_ui_language("is_IS");
    assert_eq!(int(&e, root, "direction"), 0);
    assert_eq!(int(&e, root, "asked"), 0);
}

#[test]
fn a_window_that_died_is_no_longer_listed() {
    let (e, _) = hosted();
    let main = load(&e, TWO);
    let tool = find(&e, main, "tool");
    let sys = WindowSystem::of(&e);
    assert_eq!(sys.windows(&e).len(), 2);
    e.destroy_now(tool);
    assert_eq!(sys.windows(&e), vec![main]);
}

/// A root an application replaced (`ljos_quick::roots::replace_root`) is
/// destroyed when its window shows the new one, and deleted a turn later:
/// listed in between, it got a platform window of its own, empty, over
/// the one that showed the new root.
#[test]
fn a_window_that_is_going_is_no_longer_listed() {
    let (e, _) = hosted();
    let main = load(&e, TWO);
    let tool = find(&e, main, "tool");
    let sys = WindowSystem::of(&e);
    e.destroy(tool);
    assert!(e.is_alive(tool), "deleted with the deferred deletes");
    assert_eq!(sys.windows(&e), vec![main]);
}

#[test]
fn what_is_in_a_window_nothing_shows_is_not_hidden_itself() {
    // doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop: an item is hidden
    // by an item above it, and a window is none.
    let e = engine();
    let driver = e.create_native(ljos_quick::Item::default(), None).unwrap();
    e.complete(driver).unwrap();
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add(
        "Main",
        r#"
import QtQuick
Window {
    objectName: "w"
    width: 200; height: 100
    Item { objectName: "inside" }
}
"#,
    );
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    let mut files = ljos_engine::MapSource::new();
    files.insert("Main", c.file);
    e.set_component_source(files);
    let w = e.create_from_url("Main", Some(driver), Vec::new()).unwrap();
    ljos_quick::add_data(&e, driver, w).unwrap();
    let _scene = scene(&e, driver);
    assert!(!flag(&e, w, "visible"), "the window is not shown");
    assert!(flag(&e, find(&e, w, "inside"), "visible"));
}

const SHOWN: &str = r#"
import QtQuick
import QtQuick.Controls
ApplicationWindow {
    id: main
    objectName: "main"
    width: 320; height: 240
    visible: true
    TextField { objectName: "mainInput"; focus: true }
    Window {
        id: tool
        objectName: "tool"
        width: 200; height: 100
        visible: true
        transientParent: null
        TextField { objectName: "toolInput"; focus: true }
    }
}
"#;

#[test]
fn a_window_that_is_shown_from_the_start_takes_the_keys_when_its_host_says_so() {
    let (e, _) = hosted();
    let main = load(&e, SHOWN);
    let tool = find(&e, main, "tool");
    let mut main_scene = scene(&e, main);
    let mut tool_scene = scene(&e, tool);
    let ti = find(&e, main, "toolInput");
    main_scene.set_window_active(&e, false);
    tool_scene.set_window_active(&e, false);
    assert!(!flag(&e, tool, "active"));
    tool_scene.set_window_active(&e, true);
    assert!(flag(&e, ti, "activeFocus"));
    assert!(flag(&e, tool, "active"));
    assert!(!flag(&e, main, "active"));
    // And the window it was declared in kept its own.
    tool_scene.set_window_active(&e, false);
    main_scene.set_window_active(&e, true);
    assert!(flag(&e, find(&e, main, "mainInput"), "activeFocus"));
    assert!(flag(&e, main, "active"));
    assert!(!flag(&e, tool, "active"));
}
