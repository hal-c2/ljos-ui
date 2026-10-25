//! What the scenarios in `features/controls/` cannot see of `TextField`,
//! `TextArea` and `SpinBox`: that nothing waits while they rest, the one
//! deadline a held press has, and what an assistive technology is told.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos_a11y::Role;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::keys::key;
use ljos_quick::{AccessibleAction, KeyEvent, PointerEvent, QuickScene, WheelEvent};
use ljos_render::kurbo;
use ljos_runtime::{Clock, Time, VirtualClock};

fn engine() -> (Engine, Arc<VirtualClock>) {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    // The timers of a control run on the engine's clock.
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    e.take_log();
    (e, clock)
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
    ljos_drive::find_below(e, root, name).unwrap_or_else(|| panic!("no object named {name}"))
}

fn scene(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = QuickScene::new(kurbo::Size::new(400.0, 300.0));
    s.set_root(e, root).unwrap();
    s
}

/// Frames until the scene asks for no more, and returns its next wake.
fn settle(s: &mut QuickScene, e: &Engine, now: Time) -> Option<Time> {
    ljos_drive::settle_at(e, s, now).unwrap()
}

fn text(e: &Engine, o: ObjectId, name: &str) -> String {
    e.read_by_name(o, name).to_js_string().to_string()
}

fn int(e: &Engine, o: ObjectId, name: &str) -> i32 {
    e.read_by_name(o, name).to_number() as i32
}

fn flag(e: &Engine, o: ObjectId, name: &str) -> bool {
    e.read_by_name(o, name).to_boolean()
}

/// Nothing is waiting: no frame, no wake and no timer.
fn assert_idle(s: &mut QuickScene, e: &Engine, now: Time, when: &str) {
    assert_eq!(settle(s, e, now), None, "a wake {when}");
    assert!(!s.needs_frame(), "a frame {when}");
    assert_eq!(e.next_timer_deadline(), None, "a timer {when}");
}

/// The engine's next timer, which is due `ms` after `from` give or take
/// a timer's own coarseness.
fn due(e: &Engine, from: Time, ms: u64) -> Time {
    let at = e.next_timer_deadline().expect("a timer");
    let start = from.saturating_add(Duration::from_millis(ms));
    let range = start..start.saturating_add(Duration::from_millis(20));
    assert!(range.contains(&at), "{at:?} is not {ms} ms after {from:?}");
    at
}

/// Lets the clock reach `at` and runs what is due then.
fn reach(s: &mut QuickScene, e: &Engine, clock: &VirtualClock, at: Time) {
    clock.set(at);
    e.advance_timers(clock.now());
    settle(s, e, clock.now());
}

const ALL: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    width: 400; height: 300
    TextField {
        objectName: "field"; placeholderText: "Name"
        property string log: ""
        onPressed: log += "pressed "
        onReleased: log += "released "
        onPressAndHold: log += "held "
    }
    TextArea { objectName: "area"; y: 50; width: 200; height: 60; text: "one\ntwo" }
    SpinBox {
        objectName: "spin"; y: 120
        property string log: ""
        onValueModified: log += value + " "
    }
    TextField { objectName: "secret"; y: 180; echoMode: TextInput.Password; text: "hunter2" }
}
"#;

#[test]
fn every_style_file_is_a_type_of_both_imports() {
    for import in ["QtQuick.Controls", "QtQuick.Controls.Basic"] {
        for ty in ["TextField", "TextArea", "SpinBox"] {
            let (e, _) = engine();
            let root = load(
                &e,
                &format!(
                    "import QtQuick\nimport {import}\nItem {{ {ty} {{ objectName: \"x\" }} }}"
                ),
            );
            assert!(e.is_alive(find(&e, root, "x")), "{import} {ty}");
            assert_eq!(format!("{:?}", e.take_log()), "[]", "{import} {ty}");
        }
    }
}

#[test]
fn text_controls_nobody_touches_schedule_nothing() {
    let (e, _) = engine();
    let root = load(&e, ALL);
    let mut s = scene(&e, root);
    assert_idle(&mut s, &e, Time::ZERO, "at rest");
    // Nothing changed, so the next frame is the same one.
    assert!(!s.frame(&e, Time::ZERO).changed);
}

#[test]
fn a_held_field_waits_on_one_deadline_and_then_on_nothing() {
    let (e, clock) = engine();
    let root = load(&e, ALL);
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let field = find(&e, root, "field");

    s.pointer_event(&e, &PointerEvent::press(50.0, 20.0, Time::ZERO));
    settle(&mut s, &e, Time::ZERO);
    assert_eq!(text(&e, field, "log"), "pressed ");
    // The press is the field's own until it knows what it is: no cursor
    // blinks yet, so the press-and-hold deadline is the only one.
    assert!(!flag(&e, field, "activeFocus"));
    let hold = due(&e, Time::ZERO, 800);

    reach(&mut s, &e, &clock, hold);
    assert_eq!(text(&e, field, "log"), "pressed held ");
    assert!(!flag(&e, field, "activeFocus"));
    assert_eq!(e.next_timer_deadline(), None);

    s.pointer_event(&e, &PointerEvent::release(50.0, 20.0, clock.now()));
    assert_eq!(text(&e, field, "log"), "pressed held released ");
}

#[test]
fn a_field_that_was_only_clicked_drops_its_deadline_for_the_cursor() {
    let (e, clock) = engine();
    let root = load(&e, ALL);
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let field = find(&e, root, "field");

    s.pointer_event(&e, &PointerEvent::press(50.0, 20.0, Time::ZERO));
    s.pointer_event(&e, &PointerEvent::release(50.0, 20.0, Time::ZERO));
    let wake = settle(&mut s, &e, Time::ZERO);
    assert!(flag(&e, field, "activeFocus"));
    assert_eq!(text(&e, field, "log"), "pressed released ");
    // What is left is the blink of the cursor, not the hold: the hold
    // would be due at 800 ms and say so.
    let blink = wake.or(e.next_timer_deadline()).expect("the cursor blinks");
    reach(&mut s, &e, &clock, Time::from_millis(900).max(blink));
    assert_eq!(text(&e, field, "log"), "pressed released ");
}

#[test]
fn a_held_spin_button_steps_after_a_wait_and_then_at_a_pace() {
    let (e, clock) = engine();
    let root = load(&e, ALL);
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let spin = find(&e, root, "spin");

    // The up button is the right 40 pixels of the 140 the box is wide.
    s.pointer_event(&e, &PointerEvent::press(120.0, 140.0, Time::ZERO));
    settle(&mut s, &e, Time::ZERO);
    assert!(flag(&e, spin, "up.pressed"));
    assert_eq!(int(&e, spin, "value"), 0);
    let first = due(&e, Time::ZERO, 400);

    reach(&mut s, &e, &clock, first);
    assert_eq!(int(&e, spin, "value"), 1);
    let second = due(&e, first, 100);
    reach(&mut s, &e, &clock, second);
    assert_eq!(int(&e, spin, "value"), 2);

    s.pointer_event(&e, &PointerEvent::release(120.0, 140.0, clock.now()));
    // The release of a button that repeated is not one more step.
    assert_eq!(int(&e, spin, "value"), 2);
    assert_eq!(text(&e, spin, "log"), "1 2 ");
    assert!(!flag(&e, spin, "up.pressed"));
    assert_idle(&mut s, &e, clock.now(), "after the release");
}

#[test]
fn a_spin_button_at_the_end_of_its_range_stops_waiting() {
    let (e, clock) = engine();
    let root = load(&e, ALL);
    let mut s = scene(&e, root);
    let spin = find(&e, root, "spin");
    e.write_by_name(spin, "value", Value::Int(98)).unwrap();
    settle(&mut s, &e, Time::ZERO);

    s.pointer_event(&e, &PointerEvent::press(120.0, 140.0, Time::ZERO));
    settle(&mut s, &e, Time::ZERO);
    let first = due(&e, Time::ZERO, 400);
    reach(&mut s, &e, &clock, first);
    assert_eq!(int(&e, spin, "value"), 99);
    // Nowhere further to go: holding on costs nothing.
    let next = e.next_timer_deadline();
    if let Some(next) = next {
        reach(&mut s, &e, &clock, next);
    }
    assert_eq!(int(&e, spin, "value"), 99);
    assert_eq!(e.next_timer_deadline(), None);
    s.pointer_event(&e, &PointerEvent::release(120.0, 140.0, clock.now()));
    assert_eq!(text(&e, spin, "log"), "99 ");
}

#[test]
fn destroying_a_held_spin_box_leaves_nothing_behind() {
    let (e, clock) = engine();
    let root = load(&e, ALL);
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let spin = find(&e, root, "spin");
    s.pointer_event(&e, &PointerEvent::press(120.0, 140.0, Time::ZERO));
    settle(&mut s, &e, Time::ZERO);
    assert!(e.next_timer_deadline().is_some());

    e.destroy(spin);
    e.process_deferred_deletes();
    // Its timer went with it, and the release finds nobody.
    assert_idle(&mut s, &e, Time::ZERO, "after the box is gone");
    s.pointer_event(&e, &PointerEvent::release(120.0, 140.0, clock.now()));
    assert_idle(&mut s, &e, Time::ZERO, "after the release");
}

#[test]
fn keys_and_the_wheel_step_a_spin_box_that_has_them() {
    let (e, _) = engine();
    let root = load(&e, ALL);
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let spin = find(&e, root, "spin");

    e.invoke_by_name(spin, "forceActiveFocus", &[]).unwrap();
    assert!(s.key_event(&e, &KeyEvent::press(key::UP)));
    assert!(s.key_event(&e, &KeyEvent::press(key::UP)));
    assert!(s.key_event(&e, &KeyEvent::press(key::DOWN)));
    assert_eq!(text(&e, spin, "log"), "1 2 1 ");

    // The wheel is off until its author turns it on.
    let up = WheelEvent::notches(60.0, 140.0, 0.0, 1.0, Time::ZERO);
    s.wheel_event(&e, &up);
    assert_eq!(int(&e, spin, "value"), 1);
    e.write_by_name(spin, "wheelEnabled", Value::Bool(true))
        .unwrap();
    assert!(s.wheel_event(&e, &up));
    assert_eq!(text(&e, spin, "log"), "1 2 1 2 ");
}

#[test]
fn the_functions_a_spin_box_starts_with_can_be_called() {
    let (e, _) = engine();
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    SpinBox { id: spin; objectName: "spin"; to: 100000; value: 1234 }
    property string shown: spin.textFromValue(spin.value, spin.locale)
    property int read: spin.valueFromText("56", spin.locale)
}
"#,
    );
    let spin = find(&e, root, "spin");
    assert_eq!(text(&e, root, "shown"), text(&e, spin, "displayText"));
    assert_eq!(int(&e, root, "read"), 56);
    assert_eq!(format!("{:?}", e.take_log()), "[]");
}

#[test]
fn an_assistive_technology_reads_and_steps_a_spin_box() {
    let (e, _) = engine();
    let root = load(&e, ALL);
    let mut s = scene(&e, root);
    s.set_accessibility_active(&e, true);
    settle(&mut s, &e, Time::ZERO);
    let spin = find(&e, root, "spin");
    let key = s.accessible_node(spin).expect("a node");
    let range = |s: &QuickScene| s.accessibility_tree().get(key).unwrap().range.unwrap();
    assert_eq!(
        s.accessibility_tree().get(key).unwrap().qt_role,
        Role::SpinBox
    );
    let r = range(&s);
    assert_eq!((r.value, r.min, r.max, r.step), (0.0, 0.0, 99.0, 1.0));

    assert!(s.accessible_action(&e, key, AccessibleAction::Increase));
    assert!(s.accessible_action(&e, key, AccessibleAction::SetValue(500.0)));
    settle(&mut s, &e, Time::ZERO);
    assert_eq!(range(&s).value, 99.0);
    // What it did is what the user did.
    assert_eq!(text(&e, spin, "log"), "1 99 ");

    e.write_by_name(spin, "enabled", Value::Bool(false))
        .unwrap();
    settle(&mut s, &e, Time::ZERO);
    assert!(!s.accessible_action(&e, key, AccessibleAction::Decrease));
    assert_eq!(int(&e, spin, "value"), 99);
}

#[test]
fn an_assistive_technology_reads_a_field_and_never_a_password() {
    let (e, _) = engine();
    let root = load(&e, ALL);
    let mut s = scene(&e, root);
    s.set_accessibility_active(&e, true);
    settle(&mut s, &e, Time::ZERO);

    let field = find(&e, root, "field");
    let node = |s: &QuickScene, o: ObjectId| {
        let key = s.accessible_node(o).expect("a node");
        s.accessibility_tree().get(key).unwrap().clone()
    };
    let n = node(&s, field);
    assert_eq!(n.qt_role, Role::EditableText);
    assert_eq!(n.description, "Name");

    let secret = find(&e, root, "secret");
    let n = node(&s, secret);
    assert_eq!(n.qt_role, Role::EditableText);
    let said = format!("{n:?}");
    assert!(!said.contains("hunter2"), "{said}");

    let area = find(&e, root, "area");
    assert_eq!(node(&s, area).qt_role, Role::EditableText);
}
