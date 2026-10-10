//! Pointer delivery to MouseArea and TapHandler.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::{MouseArea, MouseButtons, PointerEvent, QuickScene, TapHandler};
use ljos_runtime::Time;

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
