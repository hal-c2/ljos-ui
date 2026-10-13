//! Flickable against the scene: what is painted and hit of content that
//! is mostly out of sight, what a wheel event costs, and who keeps a press
//! when the content could be dragged.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::flickable::{self, ScrollPhysics};
use ljos_quick::{MouseButtons, PointerEvent, QuickScene, WheelEvent};
use ljos_runtime::Time;

fn ms(n: u64) -> Time {
    Time::from_millis(n)
}

fn ready(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = scene(e, root, 400.0, 400.0);
    frame(&mut s, e);
    s
}

/// Ten thousand rows of 20 pixels in a view of 200.
const LONG_LIST: &str = r##"import QtQuick
Item {
  width: 400; height: 400
  Flickable {
    objectName: "f"
    width: 200; height: 200; clip: true
    contentWidth: 200; contentHeight: 200000
    Repeater {
      model: 10000
      Rectangle { y: index * 20; width: 200; height: 20; color: "#336699" }
    }
  }
}
"##;

/// A view of 200 holds ten rows, and an eleventh when it shows two halves.
const ROWS_IN_VIEW: usize = 11;

#[test]
fn content_out_of_sight_is_not_painted() {
    let e = engine();
    let root = load(&e, LONG_LIST);
    let f = find(&e, root, "f");
    let mut s = scene(&e, root, 400.0, 400.0);

    let first = frame(&mut s, &e).snapshot;
    assert!(
        (1..=ROWS_IN_VIEW).contains(&first.op_count()),
        "{} paint ops for a view of ten rows",
        first.op_count()
    );
    assert!(first.stats.culled >= 9_980, "{:?}", first.stats);

    // Somewhere in the middle, half a row down: the cost is the same.
    set(&e, f, "contentY", Value::Real(100_010.0));
    let middle = frame(&mut s, &e).snapshot;
    assert!(
        (1..=ROWS_IN_VIEW).contains(&middle.op_count()),
        "{} paint ops in the middle",
        middle.op_count()
    );
    assert!(middle.stats.culled >= 9_980, "{:?}", middle.stats);
}

#[test]
fn a_view_that_does_not_clip_paints_what_the_window_shows() {
    let e = engine();
    let root = load(&e, &LONG_LIST.replace("clip: true", "clip: false"));
    let mut s = scene(&e, root, 400.0, 400.0);
    // The window is 400 high: twenty rows, the rest is out of sight.
    let ops = frame(&mut s, &e).snapshot.op_count();
    assert!((ROWS_IN_VIEW..=21).contains(&ops), "{ops} paint ops");
}

const CLIPPED_AREA: &str = r##"import QtQuick
Item {
  id: root
  width: 400; height: 400
  property int presses: 0
  Flickable {
    objectName: "f"
    width: 200; height: 100; clip: true
    contentWidth: 200; contentHeight: 1000
    MouseArea { y: 150; width: 200; height: 50; onPressed: root.presses++ }
  }
}
"##;

fn click(s: &mut QuickScene, e: &Engine, x: f64, y: f64) {
    s.pointer_event(e, &PointerEvent::press(x, y, ms(0)));
    s.pointer_event(e, &PointerEvent::release(x, y, ms(10)));
}

#[test]
fn content_out_of_sight_is_not_hit() {
    let e = engine();
    let root = load(&e, CLIPPED_AREA);
    let f = find(&e, root, "f");
    let mut s = ready(&e, root);
    // The area is below the view, where the window still is.
    click(&mut s, &e, 100.0, 170.0);
    assert_eq!(num(&e, root, "presses"), 0.0);

    // Scrolled into the view it is hit where it now is.
    set(&e, f, "contentY", Value::Real(100.0));
    frame(&mut s, &e);
    click(&mut s, &e, 100.0, 70.0);
    assert_eq!(num(&e, root, "presses"), 1.0);
    click(&mut s, &e, 100.0, 170.0);
    assert_eq!(num(&e, root, "presses"), 1.0);
}

const PLAIN: &str = r##"import QtQuick
Item {
  width: 400; height: 400
  Flickable {
    objectName: "f"
    width: 200; height: 200
    contentWidth: 200; contentHeight: 1000
    Rectangle { width: 200; height: 1000; color: "#336699" }
  }
}
"##;

#[test]
fn a_wheel_event_that_moves_nothing_costs_nothing() {
    let e = engine();
    let root = load(&e, PLAIN);
    let f = find(&e, root, "f");
    let mut s = ready(&e, root);
    assert!(!s.needs_frame());

    // At the beginning there is nowhere to go; outside, nothing to scroll.
    assert!(!s.wheel_event(&e, &WheelEvent::notches(100.0, 100.0, 0.0, 1.0, ms(0))));
    assert!(!s.wheel_event(&e, &WheelEvent::notches(300.0, 300.0, 0.0, -1.0, ms(0))));
    assert!(!s.wheel_event(&e, &WheelEvent::notches(100.0, 100.0, 1.0, 0.0, ms(0))));
    assert!(!s.needs_frame());
    assert_eq!(get(&e, f, "moving"), Value::Bool(false));
    assert!(!frame(&mut s, &e).changed);
}

#[test]
fn a_wheel_event_that_moves_the_content_asks_for_one_frame() {
    let e = engine();
    let root = load(&e, PLAIN);
    let f = find(&e, root, "f");
    let mut s = ready(&e, root);

    assert!(s.wheel_event(&e, &WheelEvent::notches(100.0, 100.0, 0.0, -1.0, ms(0))));
    assert_eq!(num(&e, f, "contentY"), flickable::WHEEL_NOTCH_PIXELS);
    assert!(s.needs_frame());
    assert!(frame(&mut s, &e).changed);
    assert!(!s.needs_frame());
    assert!(!frame(&mut s, &e).changed);
}

#[test]
fn a_device_that_ends_its_gesture_ends_the_movement() {
    let e = engine();
    let root = load(&e, PLAIN);
    let f = find(&e, root, "f");
    let mut s = ready(&e, root);
    let at = |dy: f64, phase| WheelEvent::pixels(100.0, 100.0, 0.0, dy, ms(0)).with_phase(phase);

    assert!(s.wheel_event(&e, &at(-30.0, ljos_quick::ScrollPhase::Begin)));
    assert!(s.wheel_event(&e, &at(-12.0, ljos_quick::ScrollPhase::Update)));
    assert_eq!(num(&e, f, "contentY"), 42.0);
    assert_eq!(get(&e, f, "moving"), Value::Bool(true));
    // The fingers left: no timer has to say so.
    s.wheel_event(&e, &at(0.0, ljos_quick::ScrollPhase::End));
    assert_eq!(get(&e, f, "moving"), Value::Bool(false));
}

#[test]
fn a_scroll_bar_reads_and_moves_the_content_through_the_seam() {
    let e = engine();
    let root = load(&e, PLAIN);
    let f = find(&e, root, "f");
    let _s = ready(&e, root);

    let y = flickable::axis_state(&e, f, false);
    assert_eq!((y.position, y.min, y.max), (0.0, 0.0, 800.0));
    let x = flickable::axis_state(&e, f, true);
    assert_eq!((x.min, x.max), (0.0, 0.0));

    flickable::scroll(&e, f, false, 250.0);
    assert_eq!(num(&e, f, "contentY"), 250.0);
    assert_eq!(flickable::axis_state(&e, f, false).position, 250.0);
    assert!(flickable::is_flickable(&e, f));
    assert!(!flickable::is_flickable(&e, root));
    assert_eq!(flickable::scroll_physics(&e), ScrollPhysics::default());
    assert!(flickable::scroll_physics(&e).kinetic().is_some());
}

#[test]
fn a_view_can_turn_the_wheel_off_for_its_flickable() {
    let e = engine();
    let root = load(&e, PLAIN);
    let f = find(&e, root, "f");
    let mut s = ready(&e, root);
    let down = WheelEvent::notches(100.0, 100.0, 0.0, -1.0, ms(0));

    flickable::set_wheel_enabled(&e, f, false);
    assert!(!s.wheel_event(&e, &down));
    assert_eq!(num(&e, f, "contentY"), 0.0);
    flickable::set_wheel_enabled(&e, f, true);
    assert!(s.wheel_event(&e, &down));
    assert_eq!(num(&e, f, "contentY"), 72.0);
}

const WITH_AREA: &str = r##"import QtQuick
Item {
  width: 400; height: 400
  Flickable {
    objectName: "f"
    width: 200; height: 200
    contentWidth: 200; contentHeight: 1000
    MouseArea { objectName: "area"; width: 200; height: 1000 }
  }
}
"##;

/// A press at 150 and three moves upwards: enough to drag by 20.
fn drag_up(s: &mut QuickScene, e: &Engine) {
    s.pointer_event(e, &PointerEvent::press(100.0, 150.0, ms(0)));
    for (i, y) in [130.0, 120.0, 100.0].into_iter().enumerate() {
        let at = ms(20 * (i as u64 + 1));
        s.pointer_event(e, &PointerEvent::moved(100.0, y, MouseButtons::LEFT, at));
    }
}

#[test]
fn a_drag_takes_the_press_from_the_child_and_tells_it() {
    let e = engine();
    let root = load(&e, WITH_AREA);
    let (f, area) = (find(&e, root, "f"), find(&e, root, "area"));
    let log = Rc::new(RefCell::new(Vec::new()));
    for signal in ["pressed", "released", "clicked", "canceled"] {
        let log = log.clone();
        e.connect_by_name(area, signal, move |_, _| log.borrow_mut().push(signal))
            .unwrap();
    }
    let mut s = ready(&e, root);

    drag_up(&mut s, &e);
    assert_eq!(*log.borrow(), ["pressed", "canceled"]);
    assert_eq!(num(&e, f, "contentY"), 20.0);
    assert_eq!(get(&e, f, "dragging"), Value::Bool(true));

    s.pointer_event(&e, &PointerEvent::release(100.0, 100.0, ms(500)));
    assert_eq!(*log.borrow(), ["pressed", "canceled"]);
    assert_eq!(get(&e, f, "dragging"), Value::Bool(false));
    assert_eq!(get(&e, f, "moving"), Value::Bool(false));
    assert_eq!(num(&e, f, "contentY"), 20.0);
}

#[test]
fn an_item_that_keeps_its_press_is_not_dragged_away_from() {
    let e = engine();
    let root = load(&e, WITH_AREA);
    let (f, area) = (find(&e, root, "f"), find(&e, root, "area"));
    let mut s = ready(&e, root);

    flickable::keep_pointer_grab(&e, area, true);
    assert!(flickable::keeps_pointer_grab(&e, area));
    drag_up(&mut s, &e);
    assert_eq!(get(&e, area, "pressed"), Value::Bool(true));
    assert_eq!(num(&e, f, "contentY"), 0.0);
    s.pointer_event(&e, &PointerEvent::release(100.0, 100.0, ms(500)));

    // Once it lets go of that, the next press can become a drag.
    flickable::keep_pointer_grab(&e, area, false);
    drag_up(&mut s, &e);
    assert_eq!(get(&e, area, "pressed"), Value::Bool(false));
    assert_eq!(num(&e, f, "contentY"), 20.0);
}

#[test]
fn a_drag_costs_a_frame_only_when_the_content_moves() {
    let e = engine();
    let root = load(&e, PLAIN);
    let mut s = ready(&e, root);

    s.pointer_event(&e, &PointerEvent::press(100.0, 150.0, ms(0)));
    // Inside the threshold nothing changes.
    s.pointer_event(
        &e,
        &PointerEvent::moved(100.0, 146.0, MouseButtons::LEFT, ms(10)),
    );
    assert!(!frame(&mut s, &e).changed);
    for (t, y) in [(20, 130.0), (30, 120.0), (40, 100.0)] {
        s.pointer_event(
            &e,
            &PointerEvent::moved(100.0, y, MouseButtons::LEFT, ms(t)),
        );
    }
    assert!(frame(&mut s, &e).changed);
    assert!(!frame(&mut s, &e).changed);
}

/// A list of 400 in a view of 100, with a button at its far end.
const NAMED: &str = r##"import QtQuick
Item {
  width: 400; height: 400
  Flickable {
    objectName: "f"
    x: 10; y: 20; width: 150; height: 100; clip: true
    contentWidth: 150; contentHeight: 400
    Accessible.role: Accessible.List
    Accessible.name: "Rows"
    Rectangle {
      y: 380; width: 150; height: 20
      Accessible.role: Accessible.Button
      Accessible.name: "Last"
    }
  }
}
"##;

#[test]
fn a_scroll_asked_by_an_assistive_technology_moves_the_content() {
    use ljos_a11y::accesskit::Role;
    use ljos_quick::AccessibleAction;

    let e = engine();
    let root = load(&e, NAMED);
    let f = find(&e, root, "f");
    let mut s = scene(&e, root, 400.0, 400.0);
    s.set_accessibility_active(&e, true);
    frame(&mut s, &e);

    let rows = s.accessibility_tree().find(Role::List, "Rows").unwrap();
    let last = s.accessibility_tree().find(Role::Button, "Last").unwrap();
    let data = s.accessibility_tree().get(rows).unwrap();
    assert!(data.clips_children);
    let at = data.scroll.unwrap();
    assert_eq!((at.y, at.y_min, at.y_max), (0.0, 0.0, 300.0));
    assert_eq!((at.x_min, at.x_max), (0.0, 0.0));
    // What is scrolled out of view is in the tree, below the view, where
    // its node says it is: the clipping parent tells a reader it is hidden.
    assert_eq!(s.accessibility_tree().parent(last), Some(rows));
    let below = s.accessibility_tree().get(last).unwrap().bounds;
    assert_eq!((below.y0, below.y1), (400.0, 420.0));

    let page = AccessibleAction::ScrollDown { page: true };
    assert!(s.accessible_action(&e, rows, page));
    assert_eq!(num(&e, f, "contentY"), 100.0);
    assert!(s.needs_frame());
    frame(&mut s, &e);
    assert!(!s.needs_frame());
    assert_eq!(
        s.accessibility_tree().get(rows).unwrap().scroll.unwrap().y,
        100.0
    );
    assert_eq!(s.accessibility_tree().get(last).unwrap().bounds.y0, 300.0);

    let step = AccessibleAction::ScrollDown { page: false };
    assert!(s.accessible_action(&e, rows, step));
    assert_eq!(
        num(&e, f, "contentY"),
        100.0 + flickable::WHEEL_NOTCH_PIXELS
    );
    // The content is no wider than the view.
    assert!(!s.accessible_action(&e, rows, AccessibleAction::ScrollRight { page: false }));
    // A value is not something a Flickable takes.
    assert!(!s.accessible_action(&e, rows, AccessibleAction::SetValue(1.0)));
}

#[test]
fn a_fling_asks_for_frames_until_it_rests_and_then_for_none() {
    let e = engine();
    let root = load(&e, PLAIN);
    let f = find(&e, root, "f");
    let mut s = ready(&e, root);
    e.invoke_by_name(f, "flick", &[Value::Real(0.0), Value::Real(-800.0)])
        .unwrap();
    assert_eq!(get(&e, f, "flicking"), Value::Bool(true));

    let mut now = ms(0);
    let mut frames = 0;
    loop {
        let out = s.frame(&e, now);
        frames += 1;
        match out.next_wake {
            Some(t) => {
                assert!(t > now, "a wake that does not move on");
                now = t;
            }
            None => break,
        }
        assert!(frames < 200, "the fling never rested");
    }
    // 800 / 1500 s at about sixty frames a second.
    assert!((30..=36).contains(&frames), "{frames} frames");
    assert!((num(&e, f, "contentY") - 213.333).abs() < 0.01);
    assert_eq!(get(&e, f, "flicking"), Value::Bool(false));
    assert_eq!(get(&e, f, "moving"), Value::Bool(false));
    assert!(!s.needs_frame());
    assert_eq!(s.frame(&e, now).next_wake, None);
}

#[test]
fn immediate_physics_flings_nothing() {
    let e = engine();
    flickable::set_scroll_physics(&e, ScrollPhysics::Immediate);
    let root = load(&e, PLAIN);
    let f = find(&e, root, "f");
    let mut s = ready(&e, root);
    e.invoke_by_name(f, "flick", &[Value::Real(0.0), Value::Real(-800.0)])
        .unwrap();
    assert_eq!(get(&e, f, "flicking"), Value::Bool(false));
    assert_eq!(s.frame(&e, ms(0)).next_wake, None);
}
