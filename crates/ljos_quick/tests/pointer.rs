//! Pointer delivery to MouseArea and TapHandler.

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::{
    Modifiers, MouseArea, MouseButtons, PointerEvent, QuickScene, TapHandler, WheelEvent,
};
use ljos_runtime::{Time, VirtualClock};

type Log = Rc<RefCell<Vec<String>>>;

/// Records `tag:signal` for each listed signal of `obj`.
fn record(e: &Engine, obj: ObjectId, tag: &str, signals: &[&str], log: &Log) {
    for &sig in signals {
        let log = log.clone();
        let entry = format!("{tag}:{sig}");
        e.connect_by_name(obj, sig, move |_, _| log.borrow_mut().push(entry.clone()))
            .unwrap();
    }
}

const MOUSE: &[&str] = &[
    "pressed",
    "released",
    "clicked",
    "doubleClicked",
    "entered",
    "exited",
    "canceled",
];

fn ms(n: u64) -> Time {
    Time::from_millis(n)
}

fn area(e: &Engine, parent: ObjectId, x: f64, y: f64, w: f64, h: f64) -> ObjectId {
    let a = make::<MouseArea>(e, Some(parent));
    geom(e, a, x, y, w, h);
    a
}

fn setup() -> (Engine, ObjectId) {
    let e = engine();
    let root = make::<ljos_quick::Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 200.0, 200.0);
    (e, root)
}

fn ready(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = scene(e, root, 200.0, 200.0);
    frame(&mut s, e);
    s
}

fn click(s: &mut QuickScene, e: &Engine, x: f64, y: f64, t: u64) {
    s.pointer_event(e, &PointerEvent::press(x, y, ms(t)));
    s.pointer_event(e, &PointerEvent::release(x, y, ms(t + 10)));
}

fn take(log: &Log) -> Vec<String> {
    std::mem::take(&mut *log.borrow_mut())
}

#[test]
fn click_goes_to_the_topmost_mouse_area() {
    let (e, root) = setup();
    let under = area(&e, root, 0.0, 0.0, 100.0, 100.0);
    let over = area(&e, root, 0.0, 0.0, 50.0, 50.0);
    let log = Log::default();
    record(&e, under, "under", MOUSE, &log);
    record(&e, over, "over", MOUSE, &log);
    let mut s = ready(&e, root);
    click(&mut s, &e, 10.0, 10.0, 0);
    assert_eq!(
        take(&log),
        [
            "over:entered",
            "over:pressed",
            "over:released",
            "over:clicked",
            "over:exited"
        ]
    );
    click(&mut s, &e, 70.0, 70.0, 1000);
    assert_eq!(
        take(&log),
        [
            "under:entered",
            "under:pressed",
            "under:released",
            "under:clicked",
            "under:exited"
        ]
    );
}

#[test]
fn rejecting_the_press_passes_it_down() {
    let (e, root) = setup();
    let under = area(&e, root, 0.0, 0.0, 100.0, 100.0);
    let over = area(&e, root, 0.0, 0.0, 100.0, 100.0);
    let log = Log::default();
    record(&e, under, "under", &["pressed", "clicked"], &log);
    record(&e, over, "over", &["clicked"], &log);
    e.connect_by_name(over, "pressed", |e, args| {
        let mouse = args[0].as_object().unwrap();
        e.write_by_name(mouse, "accepted", Value::Bool(false))
            .unwrap();
    })
    .unwrap();
    let mut s = ready(&e, root);
    click(&mut s, &e, 10.0, 10.0, 0);
    assert_eq!(take(&log), ["under:pressed", "under:clicked"]);
    assert!(!get(&e, over, "pressed").to_boolean());
}

#[test]
fn mouse_event_carries_local_coordinates() {
    let (e, root) = setup();
    let a = area(&e, root, 30.0, 40.0, 50.0, 50.0);
    let seen = Rc::new(RefCell::new(None));
    let s2 = seen.clone();
    e.connect_by_name(a, "clicked", move |e, args| {
        let m = args[0].as_object().unwrap();
        *s2.borrow_mut() = Some((
            get(e, m, "x").to_number(),
            get(e, m, "y").to_number(),
            get(e, m, "button"),
        ));
    })
    .unwrap();
    let mut s = ready(&e, root);
    click(&mut s, &e, 35.0, 47.0, 0);
    assert_eq!(*seen.borrow(), Some((5.0, 7.0, Value::Int(1))));
}

#[test]
fn double_click_replaces_the_second_clicked() {
    let (e, root) = setup();
    let a = area(&e, root, 0.0, 0.0, 100.0, 100.0);
    let log = Log::default();
    record(&e, a, "a", &["pressed", "clicked", "doubleClicked"], &log);
    let mut s = ready(&e, root);
    click(&mut s, &e, 10.0, 10.0, 0);
    click(&mut s, &e, 11.0, 10.0, 100);
    assert_eq!(
        take(&log),
        ["a:pressed", "a:clicked", "a:pressed", "a:doubleClicked"]
    );
    // Too late for a double click.
    click(&mut s, &e, 11.0, 10.0, 2000);
    assert_eq!(take(&log), ["a:pressed", "a:clicked"]);
}

#[test]
fn release_outside_is_not_a_click() {
    let (e, root) = setup();
    let a = area(&e, root, 0.0, 0.0, 50.0, 50.0);
    let log = Log::default();
    record(&e, a, "a", &["released", "clicked", "exited"], &log);
    let mut s = ready(&e, root);
    s.pointer_event(&e, &PointerEvent::press(10.0, 10.0, ms(0)));
    s.pointer_event(
        &e,
        &PointerEvent::moved(80.0, 80.0, MouseButtons::LEFT, ms(5)),
    );
    // The grab keeps the pointer even outside.
    assert!(get(&e, a, "pressed").to_boolean());
    assert!(!get(&e, a, "containsMouse").to_boolean());
    s.pointer_event(&e, &PointerEvent::release(80.0, 80.0, ms(10)));
    assert_eq!(take(&log), ["a:exited", "a:released"]);
    assert!(!get(&e, a, "pressed").to_boolean());
}

#[test]
fn hover_enter_and_exit() {
    let (e, root) = setup();
    let a = area(&e, root, 50.0, 50.0, 50.0, 50.0);
    set(&e, a, "hoverEnabled", Value::Bool(true));
    let log = Log::default();
    record(&e, a, "a", &["entered", "exited"], &log);
    let mut s = ready(&e, root);
    let none = MouseButtons::empty();
    s.pointer_event(&e, &PointerEvent::moved(10.0, 10.0, none, ms(0)));
    s.pointer_event(&e, &PointerEvent::moved(60.0, 60.0, none, ms(1)));
    assert!(get(&e, a, "containsMouse").to_boolean());
    s.pointer_event(&e, &PointerEvent::moved(61.0, 60.0, none, ms(2)));
    s.pointer_event(&e, &PointerEvent::leave(ms(3)));
    assert!(!get(&e, a, "containsMouse").to_boolean());
    assert_eq!(take(&log), ["a:entered", "a:exited"]);
}

#[test]
fn disabled_and_invisible_areas_get_nothing() {
    let (e, root) = setup();
    let under = area(&e, root, 0.0, 0.0, 100.0, 100.0);
    let off = area(&e, root, 0.0, 0.0, 100.0, 100.0);
    let hidden = area(&e, root, 0.0, 0.0, 100.0, 100.0);
    set(&e, off, "enabled", Value::Bool(false));
    set(&e, hidden, "visible", Value::Bool(false));
    let log = Log::default();
    record(&e, under, "under", &["clicked"], &log);
    record(&e, off, "off", &["pressed"], &log);
    record(&e, hidden, "hidden", &["pressed"], &log);
    let mut s = ready(&e, root);
    click(&mut s, &e, 10.0, 10.0, 0);
    assert_eq!(take(&log), ["under:clicked"]);
}

#[test]
fn accepted_buttons_filter_presses() {
    let (e, root) = setup();
    let a = area(&e, root, 0.0, 0.0, 100.0, 100.0);
    let log = Log::default();
    record(&e, a, "a", &["pressed"], &log);
    let mut s = ready(&e, root);
    s.pointer_event(
        &e,
        &PointerEvent::press(10.0, 10.0, ms(0)).with_button(MouseButtons::RIGHT),
    );
    assert!(take(&log).is_empty());
}

#[test]
fn cancel_ends_the_press_without_click() {
    let (e, root) = setup();
    let a = area(&e, root, 0.0, 0.0, 100.0, 100.0);
    let log = Log::default();
    record(&e, a, "a", &["clicked", "canceled"], &log);
    let mut s = ready(&e, root);
    s.pointer_event(&e, &PointerEvent::press(10.0, 10.0, ms(0)));
    s.pointer_event(&e, &PointerEvent::cancel(ms(5)));
    assert_eq!(take(&log), ["a:canceled"]);
    assert!(!get(&e, a, "pressed").to_boolean());
}

fn tap_handler(e: &Engine, item: ObjectId) -> ObjectId {
    make::<TapHandler>(e, Some(item))
}

#[test]
fn tap_handler_taps_and_counts() {
    let (e, root) = setup();
    let target = make::<ljos_quick::Item>(&e, Some(root));
    geom(&e, target, 0.0, 0.0, 100.0, 100.0);
    let h = tap_handler(&e, target);
    let log = Log::default();
    record(
        &e,
        h,
        "h",
        &["tapped", "singleTapped", "doubleTapped"],
        &log,
    );
    let mut s = ready(&e, root);
    click(&mut s, &e, 10.0, 10.0, 0);
    assert_eq!(get(&e, h, "tapCount"), Value::Int(1));
    click(&mut s, &e, 12.0, 10.0, 100);
    assert_eq!(get(&e, h, "tapCount"), Value::Int(2));
    assert_eq!(
        take(&log),
        ["h:tapped", "h:singleTapped", "h:tapped", "h:doubleTapped"]
    );
}

#[test]
fn dragging_past_the_threshold_cancels_a_tap() {
    let (e, root) = setup();
    let target = make::<ljos_quick::Item>(&e, Some(root));
    geom(&e, target, 0.0, 0.0, 100.0, 100.0);
    let h = tap_handler(&e, target);
    let log = Log::default();
    record(&e, h, "h", &["tapped", "canceled"], &log);
    let mut s = ready(&e, root);
    s.pointer_event(&e, &PointerEvent::press(10.0, 10.0, ms(0)));
    assert!(get(&e, h, "pressed").to_boolean());
    s.pointer_event(
        &e,
        &PointerEvent::moved(40.0, 10.0, MouseButtons::LEFT, ms(5)),
    );
    s.pointer_event(&e, &PointerEvent::release(40.0, 10.0, ms(10)));
    assert_eq!(take(&log), ["h:canceled"]);
    assert!(!get(&e, h, "pressed").to_boolean());
}

#[test]
fn tap_handler_does_not_block_the_mouse_area_below() {
    // The default gesture policy (DragThreshold) only takes a passive grab,
    // so the press keeps travelling to items underneath.
    let (e, root) = setup();
    let under = area(&e, root, 0.0, 0.0, 100.0, 100.0);
    let target = make::<ljos_quick::Item>(&e, Some(root));
    geom(&e, target, 0.0, 0.0, 100.0, 100.0);
    let h = tap_handler(&e, target);
    let log = Log::default();
    record(&e, h, "h", &["tapped"], &log);
    record(&e, under, "under", &["clicked"], &log);
    let mut s = ready(&e, root);
    click(&mut s, &e, 10.0, 10.0, 0);
    let got = take(&log);
    assert!(got.contains(&"h:tapped".to_owned()), "{got:?}");
    assert!(got.contains(&"under:clicked".to_owned()), "{got:?}");
}

// ------------------------------------------------------------------ wheel
//
// What Qt does with each of these was read off Qt 6.11.2 with
// `mouseWheel` under `qmltestrunner`; the same is in
// `features/input/mouse-area-wheel.feature` and `wheel-handler*.feature`.

/// A 300 by 200 window around `body`, with a `log` the handlers write to.
fn wheel_world(body: &str) -> (Engine, ObjectId, QuickScene) {
    let e = engine();
    let root = load(
        &e,
        &format!(
            "import QtQuick\nItem {{\n id: root\n width: 300; height: 200\n \
             property string log: \"\"\n{body}\n}}\n"
        ),
    );
    let mut s = scene(&e, root, 300.0, 200.0);
    frame(&mut s, &e);
    (e, root, s)
}

fn notch(x: f64, y: f64, dx: f64, dy: f64) -> WheelEvent {
    WheelEvent::notches(x, y, dx, dy, ms(0))
}

fn text(e: &Engine, o: ObjectId, name: &str) -> String {
    get(e, o, name).to_js_string().to_string()
}

#[test]
fn the_wheel_goes_to_the_handlers_of_an_item_then_to_the_item_then_beneath() {
    let (e, root, mut s) = wheel_world(
        r#"
    MouseArea { anchors.fill: parent; onWheel: root.log += "under|" }
    MouseArea {
      anchors.fill: parent
      onWheel: (wheel) => { root.log += "top|"; wheel.accepted = false }
      WheelHandler { blocking: false; onWheel: root.log += "first|" }
      WheelHandler { blocking: false; onWheel: root.log += "second|" }
    }"#,
    );
    assert!(s.wheel_event(&e, &notch(10.0, 10.0, 0.0, 1.0)));
    assert_eq!(text(&e, root, "log"), "second|first|top|under|");
}

#[test]
fn a_handler_that_blocks_keeps_the_wheel_from_its_item_and_from_beneath() {
    let (e, root, mut s) = wheel_world(
        r#"
    MouseArea { anchors.fill: parent; onWheel: root.log += "under|" }
    MouseArea {
      anchors.fill: parent
      onWheel: root.log += "top|"
      WheelHandler { onWheel: (event) => { root.log += "handler|"; event.accepted = false } }
    }"#,
    );
    assert!(s.wheel_event(&e, &notch(10.0, 10.0, 0.0, 1.0)));
    // Refusing the event in the handler's signal changes nothing.
    assert_eq!(text(&e, root, "log"), "handler|");
}

const OVER_A_FLICKABLE: &str = r#"
    Flickable {
      objectName: "f"
      anchors.fill: parent
      contentWidth: 300; contentHeight: 1000
    }"#;

#[test]
fn an_area_nobody_listens_to_lets_the_wheel_through() {
    let (e, root, mut s) = wheel_world(&format!(
        "{OVER_A_FLICKABLE}\n    MouseArea {{ anchors.fill: parent }}"
    ));
    assert!(s.wheel_event(&e, &notch(10.0, 10.0, 0.0, -1.0)));
    assert_eq!(num(&e, find(&e, root, "f"), "contentY"), 72.0);
}

#[test]
fn an_area_that_listens_keeps_the_wheel_unless_it_refuses_it() {
    let (e, root, mut s) = wheel_world(&format!(
        "{OVER_A_FLICKABLE}
    MouseArea {{
      anchors.fill: parent
      property bool keep: true
      objectName: \"a\"
      onWheel: (wheel) => {{ root.log += \"a|\"; wheel.accepted = keep }}
    }}"
    ));
    let f = find(&e, root, "f");
    assert!(s.wheel_event(&e, &notch(10.0, 10.0, 0.0, -1.0)));
    assert_eq!(num(&e, f, "contentY"), 0.0);
    set(&e, find(&e, root, "a"), "keep", Value::Bool(false));
    assert!(s.wheel_event(&e, &notch(10.0, 10.0, 0.0, -1.0)));
    assert_eq!(num(&e, f, "contentY"), 72.0);
    assert_eq!(text(&e, root, "log"), "a|a|");
}

#[test]
fn a_disabled_area_and_a_wheel_nobody_wants_change_nothing() {
    let (e, root, mut s) = wheel_world(
        r#"
    MouseArea { anchors.fill: parent; enabled: false; onWheel: root.log += "off|" }
    MouseArea { x: 200; width: 100; height: 100 }"#,
    );
    assert!(!s.wheel_event(&e, &notch(10.0, 10.0, 0.0, 1.0)));
    assert!(!s.wheel_event(&e, &notch(210.0, 10.0, 0.0, 1.0)));
    assert_eq!(text(&e, root, "log"), "");
    // Nothing was written, so no frame is asked for.
    assert!(!s.needs_frame());
}

#[test]
fn the_wheel_event_tells_where_how_far_and_what_is_held() {
    let (e, root, mut s) = wheel_world(
        r#"
    MouseArea {
      x: 20; y: 30; width: 100; height: 100
      acceptedButtons: Qt.NoButton
      onWheel: (wheel) => root.log = wheel.x + "," + wheel.y + " turned " + wheel.angleDelta.x
        + "," + wheel.angleDelta.y + " b" + wheel.buttons + " m" + wheel.modifiers
        + " " + wheel.inverted + " " + wheel.accepted
    }"#,
    );
    s.wheel_event(&e, &notch(30.0, 45.0, -2.0, 0.5));
    assert_eq!(
        text(&e, root, "log"),
        "10,15 turned -240,60 b0 m0 false true"
    );

    // The buttons are those of the last pointer event.
    let right = PointerEvent::press(30.0, 45.0, ms(0)).with_button(MouseButtons::RIGHT);
    s.pointer_event(&e, &right);
    s.wheel_event(
        &e,
        &notch(30.0, 45.0, 0.0, 1.0).with_modifiers(Modifiers::CONTROL),
    );
    assert_eq!(
        text(&e, root, "log"),
        "10,15 turned 0,120 b2 m67108864 false true"
    );
    s.pointer_event(
        &e,
        &PointerEvent::release(30.0, 45.0, ms(1)).with_button(MouseButtons::RIGHT),
    );
    // A host that knows better says so.
    s.wheel_event_held(&e, &notch(30.0, 45.0, 0.0, 1.0), MouseButtons::MIDDLE);
    assert_eq!(text(&e, root, "log"), "10,15 turned 0,120 b4 m0 false true");
}

#[test]
fn a_device_that_scrolls_by_the_pixel_turns_the_wheel_that_would_scroll_as_far() {
    let (e, root, mut s) = wheel_world(
        r#"
    MouseArea {
      anchors.fill: parent
      onWheel: (wheel) => root.log = wheel.angleDelta.y + " px " + wheel.pixelDelta.y
      WheelHandler { objectName: "h"; blocking: false }
    }"#,
    );
    // 72 pixels are a notch, so 36 are half of one.
    s.wheel_event(&e, &WheelEvent::pixels(10.0, 10.0, 0.0, 36.0, ms(0)));
    assert_eq!(text(&e, root, "log"), "60 px 36");
    assert_eq!(num(&e, find(&e, root, "h"), "rotation"), 7.5);
}

#[test]
fn a_wheel_handler_adds_up_degrees_and_scales_all_of_them() {
    let (e, root, mut s) = wheel_world(
        r#"
    Item {
      anchors.fill: parent
      WheelHandler { objectName: "h" }
    }"#,
    );
    let h = find(&e, root, "h");
    assert_eq!(get(&e, h, "parent"), get(&e, h, "target"));
    s.wheel_event(&e, &notch(10.0, 10.0, 0.0, 1.0));
    s.wheel_event(&e, &notch(10.0, 10.0, 0.0, 1.0));
    assert_eq!(num(&e, h, "rotation"), 30.0);
    // A new scale applies to what was turned before it.
    set(&e, h, "rotationScale", Value::Real(-2.0));
    assert_eq!(num(&e, h, "rotation"), -60.0);
    s.wheel_event(&e, &notch(10.0, 10.0, 0.0, -1.0));
    assert_eq!(num(&e, h, "rotation"), -30.0);
    // An assigned rotation is where the next turn goes on from.
    set(&e, h, "rotation", Value::Real(100.0));
    s.wheel_event(&e, &notch(10.0, 10.0, 0.0, 1.0));
    assert_eq!(num(&e, h, "rotation"), 70.0);
}

#[test]
fn a_wheel_handler_takes_only_its_own_way_and_its_own_modifiers() {
    let (e, root, mut s) = wheel_world(
        r#"
    Item {
      width: 100; height: 100
      WheelHandler { objectName: "across"; orientation: Qt.Horizontal; activeTimeout: 0.5 }
    }
    Item {
      x: 100; width: 100; height: 100
      WheelHandler { objectName: "ctrl"; acceptedModifiers: Qt.ControlModifier }
    }
    Item {
      x: 200; width: 100; height: 100
      WheelHandler { objectName: "off"; enabled: false }
    }"#,
    );
    let across = find(&e, root, "across");
    assert!(!s.wheel_event(&e, &notch(10.0, 10.0, 0.0, 1.0)));
    assert!(!get(&e, across, "active").to_boolean());
    assert!(s.wheel_event(&e, &notch(10.0, 10.0, 1.0, 3.0)));
    assert_eq!(num(&e, across, "rotation"), 15.0);
    // While it is active it takes a turn the other way too, and adds none.
    assert!(s.wheel_event(&e, &notch(10.0, 10.0, 0.0, 1.0)));
    assert_eq!(num(&e, across, "rotation"), 15.0);

    let ctrl = find(&e, root, "ctrl");
    assert!(!s.wheel_event(&e, &notch(110.0, 10.0, 0.0, 1.0)));
    let both = Modifiers::CONTROL | Modifiers::SHIFT;
    assert!(!s.wheel_event(&e, &notch(110.0, 10.0, 0.0, 1.0).with_modifiers(both)));
    assert!(s.wheel_event(
        &e,
        &notch(110.0, 10.0, 0.0, 1.0).with_modifiers(Modifiers::CONTROL)
    ));
    assert_eq!(num(&e, ctrl, "rotation"), 15.0);

    assert!(!s.wheel_event(&e, &notch(210.0, 10.0, 0.0, 1.0)));
    assert_eq!(num(&e, find(&e, root, "off"), "rotation"), 0.0);
}

#[test]
fn a_wheel_handler_is_active_until_the_wheel_has_been_quiet() {
    let (e, root, mut s) = wheel_world(
        r#"
    Item {
      anchors.fill: parent
      WheelHandler { objectName: "h"; activeTimeout: 0.2 }
    }"#,
    );
    let h = find(&e, root, "h");
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let pass = |e: &Engine, to: u64| {
        clock.set(ms(to));
        e.advance_timers(ms(to));
        get(e, h, "active").to_boolean()
    };
    assert!(!pass(&e, 0));
    s.wheel_event(&e, &notch(10.0, 10.0, 0.0, 1.0));
    assert!(pass(&e, 150));
    // Another turn starts the wait again.
    s.wheel_event(&e, &notch(10.0, 10.0, 0.0, 1.0));
    assert!(pass(&e, 300));
    assert!(!pass(&e, 360));
    // The timer fired once and is at rest: nothing is left to wake for.
    assert_eq!(e.next_timer_deadline(), None);
}

#[test]
fn a_wheel_handler_changes_a_property_of_its_target() {
    let (e, root, mut s) = wheel_world(
        r#"
    Item {
      objectName: "own"
      width: 100; height: 100
      property int steps: 0
      WheelHandler { property: "steps"; rotationScale: 1 / 15 }
    }
    Item {
      x: 100; width: 100; height: 100
      WheelHandler { target: other; property: "opacity"; rotationScale: -0.01 }
    }
    Item { id: other; objectName: "other" }
    Item {
      objectName: "none"
      x: 200; width: 100; height: 100
      WheelHandler { objectName: "untargeted"; target: null; property: "x" }
    }"#,
    );
    s.wheel_event(&e, &notch(10.0, 10.0, 0.0, 2.0));
    assert_eq!(get(&e, find(&e, root, "own"), "steps"), Value::Int(2));
    s.wheel_event(&e, &notch(110.0, 10.0, 0.0, 1.0));
    assert_eq!(num(&e, find(&e, root, "other"), "opacity"), 0.85);
    s.wheel_event(&e, &notch(210.0, 10.0, 0.0, 1.0));
    assert_eq!(num(&e, find(&e, root, "none"), "x"), 200.0);
    assert_eq!(num(&e, find(&e, root, "untargeted"), "rotation"), 15.0);
}

#[test]
fn a_rotation_and_a_scale_keep_the_point_under_the_pointer_where_it_is() {
    let (e, root, mut s) = wheel_world(
        r#"
    Item {
      objectName: "turn"
      x: 110; y: 10; width: 80; height: 80
      WheelHandler { property: "rotation" }
    }
    Item {
      objectName: "plain"
      x: 10; y: 110; width: 80; height: 80
      WheelHandler { property: "rotation"; targetTransformAroundCursor: false }
    }
    Item {
      objectName: "zoom"
      x: 210; y: 110; width: 80; height: 80
      WheelHandler { property: "scale"; targetScaleMultiplier: 2 }
    }"#,
    );
    let close = |a: f64, b: f64| (a - b).abs() < 0.001;

    let turn = find(&e, root, "turn");
    s.wheel_event(&e, &notch(130.0, 30.0, 0.0, 1.0));
    assert_eq!(num(&e, turn, "rotation"), 15.0);
    // Qt 6.11.2 puts the item here.
    assert!(close(num(&e, turn, "x"), 104.142), "{}", num(&e, turn, "x"));
    assert!(close(num(&e, turn, "y"), 14.495), "{}", num(&e, turn, "y"));

    let plain = find(&e, root, "plain");
    s.wheel_event(&e, &notch(30.0, 130.0, 0.0, 1.0));
    assert_eq!(rect(&e, plain), (10.0, 110.0, 80.0, 80.0));
    assert_eq!(num(&e, plain, "rotation"), 15.0);

    // Twice as large around its corner: the corner stays, and the item,
    // which scales around its centre, moves by half its size.
    let zoom = find(&e, root, "zoom");
    s.wheel_event(&e, &notch(210.0, 110.0, 0.0, 1.0));
    assert!(close(num(&e, zoom, "scale"), 2.0));
    assert!(close(num(&e, zoom, "x"), 250.0), "{}", num(&e, zoom, "x"));
    assert!(close(num(&e, zoom, "y"), 150.0), "{}", num(&e, zoom, "y"));
}

// ------------------------------------------------------- TapHandler.point

const POINT: &str = r#"
    function at(p) { return p.x + "," + p.y }
    function show(p) {
      return at(p.position) + " in " + at(p.scenePosition) + " from " + at(p.pressPosition)
        + " in " + at(p.scenePressPosition) + " b" + p.pressedButtons + " id " + p.id
    }
    property string now: show(handler.point)
    Item {
      x: 50; y: 40; width: 100; height: 100
      TapHandler {
        id: handler
        onPointChanged: root.log += "point " + root.at(point.position) + "|"
        onPressedChanged: root.log += "pressed " + pressed + " " + point.pressedButtons + "|"
        onTapped: root.log += "tapped " + root.at(point.position) + "|"
        onCanceled: root.log += "canceled " + root.at(handler.point.position) + "|"
      }
    }"#;

const EMPTY_POINT: &str = "0,0 in 0,0 from 0,0 in 0,0 b0 id -1";

#[test]
fn the_point_of_a_tap_handler_is_filled_while_it_is_pressed() {
    let (e, root, mut s) = wheel_world(POINT);
    assert_eq!(text(&e, root, "now"), EMPTY_POINT);
    // A move with no button held is no point of the handler's.
    s.pointer_event(
        &e,
        &PointerEvent::moved(60.0, 70.0, MouseButtons::empty(), ms(0)),
    );
    assert_eq!(text(&e, root, "now"), EMPTY_POINT);
    assert_eq!(text(&e, root, "log"), "");

    s.pointer_event(&e, &PointerEvent::press(60.0, 70.0, ms(1)));
    assert_eq!(
        text(&e, root, "now"),
        "10,30 in 60,70 from 10,30 in 60,70 b1 id 0"
    );
    s.pointer_event(
        &e,
        &PointerEvent::moved(62.0, 73.0, MouseButtons::LEFT, ms(2)),
    );
    assert_eq!(
        text(&e, root, "now"),
        "12,33 in 62,73 from 10,30 in 60,70 b1 id 0"
    );
    assert_eq!(
        text(&e, root, "log"),
        "pressed true 1|point 10,30|point 12,33|"
    );
}

#[test]
fn a_tap_reads_its_point_before_the_point_is_emptied() {
    let (e, root, mut s) = wheel_world(POINT);
    s.pointer_event(&e, &PointerEvent::press(60.0, 70.0, ms(0)));
    set(&e, root, "log", Value::from(""));
    s.pointer_event(&e, &PointerEvent::release(62.0, 73.0, ms(10)));
    // Qt's order: tapped, then pressed false with no button held, then
    // the empty point.
    assert_eq!(
        text(&e, root, "log"),
        "tapped 12,33|pressed false 0|point 0,0|"
    );
    assert_eq!(text(&e, root, "now"), EMPTY_POINT);
}

#[test]
fn a_cancelled_press_reads_its_point_before_the_point_is_emptied() {
    let (e, root, mut s) = wheel_world(POINT);
    s.pointer_event(&e, &PointerEvent::press(60.0, 70.0, ms(0)));
    set(&e, root, "log", Value::from(""));
    s.pointer_event(
        &e,
        &PointerEvent::moved(120.0, 70.0, MouseButtons::LEFT, ms(5)),
    );
    assert_eq!(
        text(&e, root, "log"),
        "pressed false 1|canceled 10,30|point 0,0|"
    );
    assert_eq!(text(&e, root, "now"), EMPTY_POINT);
}
