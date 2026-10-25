//! What the scenarios in `features/controls/` cannot see of popups.

use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::{PointerEvent, QuickScene};
use ljos_render::kurbo;
use ljos_runtime::{Clock, Time, VirtualClock};

fn engine() -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    // What a test then finds in the log is what its own steps said, not
    // what registering every type of the module did.
    e.take_log();
    e
}

/// Compiles `source` as `Main.qml` and creates it.
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
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if e.read_by_name(o, "objectName").to_js_string().as_str() == name {
            return o;
        }
        stack.extend(e.children(o));
    }
    panic!("no object named {name}")
}

fn scene(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = QuickScene::new(kurbo::Size::new(400.0, 400.0));
    s.set_root(e, root).unwrap();
    s
}

/// Frames until the scene asks for no more, and returns its next wake.
fn settle(s: &mut QuickScene, e: &Engine, now: Time) -> Option<Time> {
    for _ in 0..16 {
        let out = s.frame(e, now);
        if !s.needs_frame() && out.next_wake.is_none_or(|t| t > now) {
            return out.next_wake;
        }
    }
    panic!("the scene did not settle");
}

fn text(e: &Engine, o: ObjectId, name: &str) -> String {
    e.read_by_name(o, name).to_js_string().to_string()
}

fn flag(e: &Engine, o: ObjectId, name: &str) -> bool {
    e.read_by_name(o, name).to_boolean()
}

fn click(s: &mut QuickScene, e: &Engine, x: f64, y: f64) {
    s.pointer_event(e, &PointerEvent::press(x, y, Time::ZERO));
    s.pointer_event(e, &PointerEvent::release(x, y, Time::ZERO));
    settle(s, e, Time::ZERO);
}

/// Nothing is waiting: no frame, no wake and no timer.
fn assert_idle(s: &mut QuickScene, e: &Engine, when: &str) {
    assert_eq!(settle(s, e, Time::ZERO), None, "a wake {when}");
    assert!(!s.needs_frame(), "a frame {when}");
    assert_eq!(e.next_timer_deadline(), None, "a timer {when}");
}

const POPUPS: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    id: root
    width: 400; height: 400
    Item { objectName: "before"; focus: true; width: 10; height: 10 }
    Popup { objectName: "plain"; width: 50; height: 50 }
    Dialog {
        objectName: "dialog"; title: "Sure?"; modal: true; focus: true
        standardButtons: Dialog.Ok | Dialog.Cancel
    }
    Menu {
        objectName: "menu"
        MenuItem { text: "One" }
        Menu { title: "More"; MenuItem { text: "Deep" } }
    }
    ComboBox { objectName: "combo"; y: 300; model: ["One", "Two"] }
}
"#;

#[test]
fn a_popup_that_is_open_or_closed_schedules_nothing() {
    let e = engine();
    let root = load(&e, POPUPS);
    let mut s = scene(&e, root);
    assert_idle(&mut s, &e, "before anything is open");
    for name in ["plain", "dialog", "menu"] {
        let p = find(&e, root, name);
        e.invoke_by_name(p, "open", &[]).unwrap();
        assert!(flag(&e, p, "opened"), "{name}");
        assert_idle(&mut s, &e, &format!("with {name} open"));
        e.invoke_by_name(p, "close", &[]).unwrap();
        assert_idle(&mut s, &e, &format!("with {name} closed again"));
    }
    click(&mut s, &e, 20.0, 320.0);
    let combo = find(&e, root, "combo");
    assert!(flag(&e, combo, "down"));
    assert_idle(&mut s, &e, "with the combo box open");
    assert_eq!(format!("{:?}", e.take_log()), "[]", "a warning");
}

#[test]
fn a_tool_tip_holds_a_timer_only_while_it_waits() {
    let clock = Arc::new(VirtualClock::new());
    let e = engine();
    e.set_clock(Rc::new(clock.clone()));
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    width: 400; height: 400
    ToolTip { objectName: "tip"; text: "Hint"; delay: 500; timeout: 1000 }
}
"#,
    );
    let mut s = scene(&e, root);
    let tip = find(&e, root, "tip");
    assert_idle(&mut s, &e, "before the tip is asked for");

    // A timer is due within its own coarseness of what was asked.
    let due = |ms: u64| {
        let at = e.next_timer_deadline().expect("a timer");
        let range = Time::from_millis(ms)..Time::from_millis(ms + 20);
        assert!(range.contains(&at), "{at:?} is not {ms} ms");
        at
    };
    e.invoke_by_name(tip, "open", &[]).unwrap();
    let shows = due(500);
    assert!(!flag(&e, tip, "opened"));

    clock.set(shows);
    e.advance_timers(clock.now());
    settle(&mut s, &e, clock.now());
    assert!(flag(&e, tip, "opened"));
    let hides = due(1500);

    clock.set(hides);
    e.advance_timers(clock.now());
    settle(&mut s, &e, clock.now());
    assert!(!flag(&e, tip, "visible"));
    assert_eq!(e.next_timer_deadline(), None);
}

/// The nodes of the tree as (role word, name, modal).
fn nodes(s: &QuickScene) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    s.accessibility_tree().walk(|_, _, d| {
        out.push((ljos_a11y::role_word(d.role), d.name.clone(), d.modal));
    });
    out
}

#[test]
fn a_shown_modal_popup_is_modal_to_an_assistive_technology() {
    let e = engine();
    let root = load(&e, POPUPS);
    let mut s = scene(&e, root);
    s.set_accessibility_active(&e, true);
    settle(&mut s, &e, Time::ZERO);
    assert!(nodes(&s).iter().all(|(role, ..)| role != "dialog"));

    let dialog = find(&e, root, "dialog");
    e.invoke_by_name(dialog, "open", &[]).unwrap();
    settle(&mut s, &e, Time::ZERO);
    let dialogs: Vec<_> = nodes(&s)
        .into_iter()
        .filter(|(role, ..)| role == "dialog")
        .collect();
    assert_eq!(dialogs, [("dialog".to_owned(), "Sure?".to_owned(), true)]);

    // One that is not modal leaves the rest of the window reachable.
    e.write_by_name(dialog, "modal", Value::Bool(false))
        .unwrap();
    settle(&mut s, &e, Time::ZERO);
    assert!(nodes(&s).iter().all(|(.., modal)| !modal));
}

#[test]
fn the_keyboard_goes_into_a_popup_and_back() {
    let e = engine();
    let root = load(&e, POPUPS);
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let before = find(&e, root, "before");
    let dialog = find(&e, root, "dialog");
    assert!(flag(&e, before, "activeFocus"));
    e.invoke_by_name(dialog, "open", &[]).unwrap();
    settle(&mut s, &e, Time::ZERO);
    assert!(flag(&e, dialog, "activeFocus"));
    assert!(!flag(&e, before, "activeFocus"));
    e.invoke_by_name(dialog, "close", &[]).unwrap();
    settle(&mut s, &e, Time::ZERO);
    assert!(flag(&e, before, "activeFocus"));
}

const COMBO: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    id: root
    width: 400; height: 400
    ListModel {
        id: rows
        objectName: "rows"
        ListElement { name: "Alpha"; code: 10 }
        ListElement { name: "Beta"; code: 20 }
    }
    ComboBox { objectName: "combo"; model: rows; textRole: "name"; valueRole: "code" }
}
"#;

/// The texts of the rows of the combo box's popup, from the top.
fn rows(e: &Engine, combo: ObjectId) -> Vec<String> {
    let popup = e.read_by_name(combo, "popup").as_object().unwrap();
    // The style's popup has a list view, which keeps the rows in its own
    // content item, with the item that marks its current row among them;
    // a popup of the application's may have any item.
    let list = e.read_by_name(popup, "contentItem").as_object().unwrap();
    let content = e
        .read_by_name(list, "contentItem")
        .as_object()
        .unwrap_or(list);
    let mark = e.read_by_name(list, "highlightItem").as_object();
    let mut rows: Vec<(f64, String)> = ljos_quick::child_items(e, content)
        .into_iter()
        .filter(|d| Some(*d) != mark)
        .map(|d| (e.read_by_name(d, "y").to_number(), text(e, d, "text")))
        .collect();
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows.into_iter().map(|(_, t)| t).collect()
}

fn call(e: &Engine, o: ObjectId, method: &str, args: &[Value]) {
    e.invoke_by_name(o, method, args).unwrap();
}

#[test]
fn a_combo_box_makes_its_rows_when_it_first_opens() {
    let e = engine();
    let root = load(&e, COMBO);
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let combo = find(&e, root, "combo");
    assert!(rows(&e, combo).is_empty());
    click(&mut s, &e, 20.0, 20.0);
    assert_eq!(rows(&e, combo), ["Alpha", "Beta"]);
    // They are kept when it closes.
    click(&mut s, &e, 20.0, 20.0);
    assert!(!flag(&e, combo, "down"));
    assert_eq!(rows(&e, combo), ["Alpha", "Beta"]);
}

#[test]
fn the_rows_of_a_combo_box_follow_its_model() {
    let e = engine();
    let root = load(&e, COMBO);
    let mut s = scene(&e, root);
    let combo = find(&e, root, "combo");
    let model = find(&e, root, "rows");
    settle(&mut s, &e, Time::ZERO);
    click(&mut s, &e, 20.0, 20.0);

    let name = |n: &str| Value::string(n);
    call(
        &e,
        model,
        "setProperty",
        &[Value::Int(1), name("name"), name("Bravo")],
    );
    settle(&mut s, &e, Time::ZERO);
    assert_eq!(rows(&e, combo), ["Alpha", "Bravo"]);

    call(
        &e,
        model,
        "move",
        &[Value::Int(1), Value::Int(0), Value::Int(1)],
    );
    settle(&mut s, &e, Time::ZERO);
    assert_eq!(rows(&e, combo), ["Bravo", "Alpha"]);

    call(&e, model, "remove", &[Value::Int(0)]);
    settle(&mut s, &e, Time::ZERO);
    assert_eq!(rows(&e, combo), ["Alpha"]);
    assert_eq!(text(&e, combo, "currentText"), "Alpha");

    // Another role names the rows, and they say so.
    e.write_by_name(combo, "textRole", name("code")).unwrap();
    settle(&mut s, &e, Time::ZERO);
    assert_eq!(rows(&e, combo), ["10"]);
    assert_eq!(format!("{:?}", e.take_log()), "[]", "a warning");
}

#[test]
fn a_bound_index_outlives_the_model_change_that_ran_it() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    function pick(value) {
        combo.opt = { value: value, choices: [{ id: "a" }, { id: "b" }, { id: "c" }] }
    }
    ComboBox {
        id: combo; objectName: "combo"
        property var opt: ({ value: "a", choices: [{ id: "a" }, { id: "b" }, { id: "c" }] })
        model: opt.choices.map(c => c.id)
        currentIndex: opt.choices.findIndex(c => c.id === opt.value)
    }
}
"#,
    );
    let mut s = scene(&e, root);
    let combo = find(&e, root, "combo");
    settle(&mut s, &e, Time::ZERO);
    e.invoke_by_name(root, "pick", &[Value::string("c")])
        .unwrap();
    // However long it is left.
    for _ in 0..3 {
        settle(&mut s, &e, Time::ZERO);
    }
    assert_eq!(text(&e, combo, "currentIndex"), "2");
    assert_eq!(text(&e, combo, "currentText"), "c");
    assert_eq!(text(&e, combo, "displayText"), "c");
    assert!(e.is_bound(combo, ljos_controls::ComboBox::CURRENT_INDEX.index));
}

#[test]
fn a_model_that_outlives_its_combo_box_does_not_reach_it() {
    let e = engine();
    let root = load(&e, COMBO);
    let mut s = scene(&e, root);
    let combo = find(&e, root, "combo");
    let model = find(&e, root, "rows");
    settle(&mut s, &e, Time::ZERO);
    click(&mut s, &e, 20.0, 20.0);
    assert_eq!(rows(&e, combo).len(), 2);
    e.destroy(combo);
    settle(&mut s, &e, Time::ZERO);
    assert!(!e.is_alive(combo));
    call(&e, model, "remove", &[Value::Int(0)]);
    settle(&mut s, &e, Time::ZERO);
    assert_eq!(e.read_by_name(model, "count").to_number(), 1.0);
    assert_eq!(format!("{:?}", e.take_log()), "[]", "a warning");
}

#[test]
fn a_combo_box_given_its_own_popup_has_no_other() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    width: 400; height: 400
    ComboBox {
        objectName: "combo"; model: 2
        popup: Popup { objectName: "own"; y: 40; width: 80; contentItem: Column {} }
    }
}
"#,
    );
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let combo = find(&e, root, "combo");
    let own = find(&e, root, "own");
    assert_eq!(e.read_by_name(combo, "popup").as_object(), Some(own));
    let popups = e
        .children(combo)
        .into_iter()
        .filter(|c| e.is_alive(*c) && e.property_index(*c, "closePolicy").is_ok())
        .count();
    assert_eq!(popups, 1);
    click(&mut s, &e, 20.0, 20.0);
    assert!(flag(&e, own, "visible"));
    assert_eq!(rows(&e, combo), ["0", "1"]);
}

#[test]
fn a_screen_reader_opens_a_combo_box_and_picks_a_row() {
    let e = engine();
    let root = load(&e, COMBO);
    let mut s = scene(&e, root);
    s.set_accessibility_active(&e, true);
    settle(&mut s, &e, Time::ZERO);
    let combo = find(&e, root, "combo");
    let key = |s: &QuickScene, role: &str, name: &str| {
        let mut found = None;
        s.accessibility_tree().walk(|key, _, d| {
            if found.is_none() && ljos_a11y::role_word(d.role) == role && d.name == name {
                found = Some(key);
            }
        });
        found.unwrap_or_else(|| panic!("no {role} named {name}"))
    };
    let node = key(&s, "combobox", "Alpha");
    assert!(s.accessible_press(&e, node, Time::ZERO));
    settle(&mut s, &e, Time::ZERO);
    assert!(flag(&e, combo, "down"));
    let row = key(&s, "listitem", "Beta");
    assert!(s.accessible_press(&e, row, Time::ZERO));
    settle(&mut s, &e, Time::ZERO);
    assert!(!flag(&e, combo, "down"));
    assert_eq!(text(&e, combo, "currentText"), "Beta");
    assert_eq!(e.read_by_name(combo, "currentValue").to_number(), 20.0);
}

/// The second popup used to look for the overlay while the first was
/// still making it, and made another that no scene showed and that took
/// nothing from the window.
#[test]
fn the_popups_of_a_window_are_in_one_overlay() {
    let e = engine();
    let root = load(
        &e,
        r##"
import QtQuick
import QtQuick.Controls.Basic
ApplicationWindow {
    width: 300; height: 300; visible: true
    palette.button: "#aa0000"
    Popup { objectName: "first" }
    Popup { objectName: "second"; palette.window: "#00aa00" }
}
"##,
    );
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let overlay_of = |name: &str| {
        let popup = find(&e, root, name);
        let Value::Object(content) = e.read_by_name(popup, "contentItem") else {
            panic!("{name} has no content item");
        };
        let item = e.visual_parent(content).expect("a popup item");
        e.visual_parent(item).expect("an overlay")
    };
    let overlay = overlay_of("first");
    assert_eq!(overlay, overlay_of("second"));
    assert_eq!(ljos_quick::layer_host(&e, overlay), Some(root));

    e.write_by_name(root, "palette.button", Value::from("#0000ff"))
        .unwrap();
    for name in ["first", "second"] {
        let popup = find(&e, root, name);
        assert_eq!(
            e.read_by_name(popup, "palette.button")
                .to_js_string()
                .as_str(),
            "#0000ff",
            "{name}"
        );
    }
}

const BAR: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
ApplicationWindow {
    width: 400; height: 400; visible: true
    menuBar: MenuBar {
        objectName: "bar"
        delegate: MenuBarItem { width: 60 }
        Menu { objectName: "file"; title: "File"; MenuItem { text: "New" } }
        MenuBarItem {
            objectName: "own"
            text: "Own"
            menu: Menu { objectName: "edit"; MenuItem { text: "Cut" } }
        }
        Button { objectName: "stray"; text: "Stray" }
    }
    header: ToolBar {}
}
"#;

/// The oracle's driver cannot press on the menu bar of a window, so what
/// a press does there is seen here.
#[test]
fn the_menu_bar_of_a_window_opens_its_menus_under_its_items() {
    let e = engine();
    let root = load(&e, BAR);
    let mut s = scene(&e, root);
    s.set_accessibility_active(&e, true);
    settle(&mut s, &e, Time::ZERO);
    let bar = find(&e, root, "bar");
    let (file, edit) = (find(&e, root, "file"), find(&e, root, "edit"));
    let own = find(&e, root, "own");

    // A menu and an item of the bar's own type are items; a button is not.
    assert_eq!(e.read_by_name(bar, "count"), Value::Int(2));
    assert_eq!(e.read_by_name(own, "x").to_number(), 60.0);
    assert_eq!(e.read_by_name(own, "menuBar"), Value::Object(bar));
    let roles: Vec<_> = nodes(&s)
        .into_iter()
        .map(|(role, name, _)| (role, name))
        .collect();
    assert!(
        roles.contains(&("menubar".to_owned(), String::new())),
        "{roles:?}"
    );
    assert!(
        roles.contains(&("menuitem".to_owned(), "File".to_owned())),
        "{roles:?}"
    );
    assert!(!roles.iter().any(|(_, name)| name == "Stray"), "{roles:?}");

    click(&mut s, &e, 10.0, 20.0);
    assert!(flag(&e, file, "visible"));
    let Value::Object(content) = e.read_by_name(file, "contentItem") else {
        panic!("the menu has no content item");
    };
    let popup_item = e.visual_parent(content).expect("a popup item");
    let at = ljos_quick::scene_transform(&e, popup_item) * kurbo::Point::ZERO;
    assert_eq!((at.x, at.y), (0.0, 40.0));
    assert_idle(&mut s, &e, "with a menu of the bar open");

    // The pointer over another item brings the open menu there.
    s.pointer_event(
        &e,
        &PointerEvent::moved(70.0, 20.0, ljos_quick::MouseButtons::empty(), Time::ZERO),
    );
    settle(&mut s, &e, Time::ZERO);
    assert!(!flag(&e, file, "visible"));
    assert!(flag(&e, edit, "visible"));
    assert!(flag(&e, own, "highlighted"));

    click(&mut s, &e, 300.0, 300.0);
    assert!(!flag(&e, edit, "visible"));
    assert!(!flag(&e, own, "highlighted"));
    assert_idle(&mut s, &e, "after the menus of the bar closed");
}

/// A transition wants frames while it runs and nothing afterwards, and
/// the keyboard is back as soon as the popup starts to go.
#[test]
fn a_popup_wants_frames_only_while_it_comes_and_goes() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    width: 400; height: 400
    Item { objectName: "before"; focus: true; width: 10; height: 10 }
    Popup {
        objectName: "p"; width: 50; height: 50; focus: true
        enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 200 } }
        exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: 200 } }
    }
}
"#,
    );
    let mut s = scene(&e, root);
    let (p, before) = (find(&e, root, "p"), find(&e, root, "before"));
    assert_idle(&mut s, &e, "before the popup is opened");
    let at = Time::from_millis;
    let rests = |s: &mut QuickScene, now: Time, when: &str| {
        assert_eq!(settle(s, &e, now), None, "a wake {when}");
        assert_eq!(ljos_quick::running_animation_count(&e), 0, "{when}");
        assert_eq!(e.next_timer_deadline(), None, "a timer {when}");
    };

    e.invoke_by_name(p, "open", &[]).unwrap();
    assert!(
        settle(&mut s, &e, at(0)).is_some(),
        "no frame while it comes"
    );
    assert!(!flag(&e, p, "opened"));
    assert!(flag(&e, p, "activeFocus"));
    rests(&mut s, at(300), "once it has come");
    assert!(flag(&e, p, "opened"));

    e.invoke_by_name(p, "close", &[]).unwrap();
    assert!(flag(&e, p, "visible"));
    assert!(flag(&e, before, "activeFocus"));
    assert!(
        settle(&mut s, &e, at(300)).is_some(),
        "no frame while it goes"
    );
    rests(&mut s, at(600), "once it has gone");
    assert!(!flag(&e, p, "visible"));
    assert_eq!(e.read_by_name(p, "opacity").to_number(), 1.0);
    assert_eq!(format!("{:?}", e.take_log()), "[]", "a warning");
}

/// A dialog centred in the overlay of its window, the usual way.
fn centred(name: &str) -> String {
    format!(
        r#"Popup {{
            objectName: "{name}"
            parent: Overlay.overlay
            x: Math.round((parent.width - width) / 2)
            y: Math.round((parent.height - height) / 2)
            width: 200; height: 100
            modal: true
        }}"#
    )
}

/// What was logged as a warning or worse since the last look.
fn warnings(e: &Engine) -> Vec<String> {
    e.take_log()
        .into_iter()
        .filter(|l| l.level >= ljos_engine::LogLevel::Warning)
        .map(|l| l.to_string())
        .collect()
}

/// Shows `root` in a 640 × 480 view.
fn window_scene(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = QuickScene::new(kurbo::Size::new(640.0, 480.0));
    s.set_root(e, root).unwrap();
    settle(&mut s, e, Time::ZERO);
    s
}

/// Opens each of `popups` in turn, and closes them all again: each of the
/// `centred` ones is in the middle of the window when it opens.
fn opens_centred(s: &mut QuickScene, e: &Engine, root: ObjectId, popups: &[&str]) {
    for name in popups {
        let p = find(e, root, name);
        e.invoke_by_name(p, "open", &[]).unwrap();
        settle(s, e, Time::ZERO);
        if text(e, p, "modal") == "true" {
            let at = (text(e, p, "x"), text(e, p, "y"));
            assert_eq!(at, ("220".into(), "190".into()), "{name}");
        }
    }
    for name in popups.iter().rev() {
        e.invoke_by_name(find(e, root, name), "close", &[]).unwrap();
        settle(s, e, Time::ZERO);
    }
    assert_eq!(warnings(e), Vec::<String>::new(), "once shown");
}

#[test]
fn the_overlay_of_a_window_is_there_while_its_tree_is_made() {
    for window in ["Window", "ApplicationWindow"] {
        let e = engine();
        let root = load(
            &e,
            &format!(
                r#"
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
{window} {{
    width: 640; height: 480
    Item {{
        anchors.fill: parent
        {top}
        Column {{ Item {{ Item {{ {deep} }} }} }}
    }}
    Popup {{
        objectName: "outer"; width: 300; height: 300
        Item {{ {inner} }}
    }}
}}
"#,
                top = centred("top"),
                deep = centred("deep"),
                inner = centred("inner"),
            ),
        );
        assert_eq!(warnings(&e), Vec::<String>::new(), "{window}");
        let mut s = window_scene(&e, root);
        opens_centred(&mut s, &e, root, &["top"]);
        opens_centred(&mut s, &e, root, &["deep"]);
        opens_centred(&mut s, &e, root, &["outer", "inner"]);
    }
}

#[test]
fn a_window_made_later_has_its_overlay_while_its_tree_is_made() {
    let e = engine();
    let root = load(
        &e,
        &format!(
            r#"
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
Window {{
    id: root
    width: 640; height: 480
    Component {{
        id: later
        Window {{ width: 300; height: 200; visible: true; Item {{ {made} }} }}
    }}
    Loader {{
        objectName: "loader"; active: false
        sourceComponent: Window {{ width: 300; height: 200; visible: true; Item {{ {loaded} }} }}
    }}
    function make() {{ return later.createObject(root) }}
}}
"#,
            made = centred("made"),
            loaded = centred("loaded"),
        ),
    );
    let mut s = window_scene(&e, root);
    assert_eq!(warnings(&e), Vec::<String>::new(), "at the start");
    let made = e.invoke_by_name(root, "make", &[]).unwrap();
    assert!(made.as_object().is_some(), "createObject made nothing");
    assert_eq!(warnings(&e), Vec::<String>::new(), "after createObject");
    let loader = find(&e, root, "loader");
    e.write_by_name(loader, "active", Value::Bool(true))
        .unwrap();
    assert_eq!(warnings(&e), Vec::<String>::new(), "after the Loader");
    // Without a host a window declared in another one's tree is shown in
    // that tree, and so are its popups.
    settle(&mut s, &e, Time::ZERO);
    opens_centred(&mut s, &e, root, &["made"]);
    opens_centred(&mut s, &e, root, &["loaded"]);
}

#[test]
fn a_hosted_window_has_its_overlay_while_its_tree_is_made() {
    let e = engine();
    ljos_controls::WindowSystem::of(&e).set_host(|| {});
    let root = load(
        &e,
        &format!(
            r#"
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
Window {{
    width: 640; height: 480
    Item {{ {main} }}
    Window {{ objectName: "toolWindow"; width: 300; height: 200; Item {{ {tool} }} }}
}}
"#,
            main = centred("main"),
            tool = centred("tool"),
        ),
    );
    assert_eq!(warnings(&e), Vec::<String>::new());
    // Each window is a tree of its own, with an overlay of its own.
    let overlay = |name: &str| e.read_by_name(find(&e, root, name), "parent").as_object();
    let window = find(&e, root, "toolWindow");
    let (main, tool) = (overlay("main"), overlay("tool"));
    assert!(main.is_some() && tool.is_some(), "{main:?} {tool:?}");
    assert_ne!(main, tool);
    assert_eq!(ljos_quick::window_item(&e, tool.unwrap()), window);
    let x = |name: &str| text(&e, find(&e, root, name), "x");
    assert_eq!((x("main"), x("tool")), ("220".into(), "50".into()));
}

#[test]
fn a_loose_window_takes_the_overlay_of_the_tree_it_is_put_in() {
    // A timer is due at a whole step of the clock after it is asked for:
    // on the wall clock a frame in the same step would not run it.
    let clock = Arc::new(VirtualClock::new());
    let e = engine();
    e.set_clock(Rc::new(clock.clone()));
    let root = load(
        &e,
        &format!(
            r#"
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
Item {{
    id: root
    width: 640; height: 480
    Component {{
        id: later
        Window {{ width: 300; height: 200; visible: true; Item {{ {loose} }}
            Popup {{
                objectName: "outer"; width: 300; height: 300
                Item {{ {inner} }}
            }}
        }}
    }}
    function make() {{ return later.createObject(null) }}
}}
"#,
            loose = centred("loose"),
            inner = centred("inner"),
        ),
    );
    let mut s = window_scene(&e, root);
    let made = e.invoke_by_name(root, "make", &[]).unwrap();
    let made = made.as_object().expect("createObject made nothing");
    assert_eq!(warnings(&e), Vec::<String>::new(), "after createObject");
    // The window is put in once the work that made it is over: at the
    // timer step after.
    clock.advance(ljos_runtime::TIMER_GRANULARITY);
    settle(&mut s, &e, clock.now());
    assert_eq!(
        e.visual_parent(made),
        Some(root),
        "the window was not put in"
    );
    assert_eq!(warnings(&e), Vec::<String>::new(), "once put in");
    opens_centred(&mut s, &e, made, &["loose", "outer", "inner"]);
}
