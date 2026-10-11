//! Hover delivery and `MouseArea.enabled`.
//!
//! Every expectation here was read off Qt 6.11.2 by running the same QML
//! under `qmltestrunner` (offscreen) with `mouseMove`/`mouseClick`; the
//! order of the signals is also in `features/input/mouse-area-hover*.feature`,
//! which the ledger runs on qml6.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{MouseButtons, PointerEvent, QuickScene};
use ljos_runtime::Time;

struct World {
    e: Engine,
    root: ObjectId,
    s: QuickScene,
    t: u64,
}

impl World {
    fn new(body: &str) -> World {
        let e = engine();
        let root = load(
            &e,
            &format!("import QtQuick\nItem {{\n width: 400; height: 200\n{body}\n}}\n"),
        );
        let mut s = scene(&e, root, 400.0, 200.0);
        frame(&mut s, &e);
        World { e, root, s, t: 0 }
    }

    fn hover(&mut self, x: f64, y: f64) {
        self.t += 1;
        let ev = PointerEvent::moved(x, y, MouseButtons::empty(), Time::from_millis(self.t));
        self.s.pointer_event(&self.e, &ev);
    }

    fn press(&mut self, x: f64, y: f64) {
        self.t += 1000;
        let ev = PointerEvent::press(x, y, Time::from_millis(self.t));
        self.s.pointer_event(&self.e, &ev);
    }

    fn drag(&mut self, x: f64, y: f64) {
        self.t += 1;
        let ev = PointerEvent::moved(x, y, MouseButtons::LEFT, Time::from_millis(self.t));
        self.s.pointer_event(&self.e, &ev);
    }

    fn release(&mut self, x: f64, y: f64) {
        self.t += 1;
        let ev = PointerEvent::release(x, y, Time::from_millis(self.t));
        self.s.pointer_event(&self.e, &ev);
    }

    fn click(&mut self, x: f64, y: f64) {
        self.t += 1000;
        let t = Time::from_millis(self.t);
        self.s.pointer_event(&self.e, &PointerEvent::press(x, y, t));
        self.s
            .pointer_event(&self.e, &PointerEvent::release(x, y, t));
    }

    fn id(&self, name: &str) -> ObjectId {
        find(&self.e, self.root, name)
    }

    /// `containsMouse` of each named area.
    fn hovered<const N: usize>(&self, names: [&str; N]) -> [bool; N] {
        names.map(|n| get(&self.e, self.id(n), "containsMouse").to_boolean())
    }

    fn enabled(&self, name: &str) -> bool {
        get(&self.e, self.id(name), "enabled").to_boolean()
    }

    /// Records `name signal` for `entered`, `exited` and `clicked`.
    fn record(&self, names: &[&str]) -> Rc<RefCell<Vec<String>>> {
        self.record_signals(names, &["entered", "exited", "clicked"])
    }

    /// The same with `positionChanged` and `released`.
    fn record_moves(&self, names: &[&str]) -> Rc<RefCell<Vec<String>>> {
        self.record_signals(names, &["entered", "exited", "positionChanged", "released"])
    }

    fn record_signals(&self, names: &[&str], signals: &[&str]) -> Rc<RefCell<Vec<String>>> {
        let log = Rc::new(RefCell::new(Vec::new()));
        for name in names {
            for sig in signals {
                let (log, entry) = (Rc::clone(&log), format!("{name} {sig}"));
                self.e
                    .connect_by_name(self.id(name), sig, move |_, _| {
                        log.borrow_mut().push(entry.clone());
                    })
                    .unwrap();
            }
        }
        log
    }
}

fn take(log: &Rc<RefCell<Vec<String>>>) -> Vec<String> {
    std::mem::take(&mut *log.borrow_mut())
}

/// The clicks recorded since the last call.
fn clicks(log: &Rc<RefCell<Vec<String>>>) -> Vec<String> {
    let mut all = take(log);
    all.retain(|l| l.ends_with("clicked"));
    all
}

#[test]
fn only_the_topmost_hover_area_is_hovered() {
    let mut w = World::new(
        r#"
    MouseArea { objectName: "s1"; x: 150; width: 100; height: 50; hoverEnabled: true }
    MouseArea { objectName: "s2"; x: 200; width: 100; height: 50; hoverEnabled: true }
    MouseArea { objectName: "e1"; x: 300; y: 100; width: 50; height: 50; hoverEnabled: true }
    Item { x: 300; y: 100; width: 50; height: 50
        MouseArea { objectName: "e2"; width: 50; height: 50; hoverEnabled: true } }
"#,
    );
    let log = w.record(&["s1", "s2"]);
    w.hover(160.0, 10.0);
    assert_eq!(w.hovered(["s1", "s2"]), [true, false]);
    w.hover(210.0, 10.0);
    assert_eq!(w.hovered(["s1", "s2"]), [false, true]);
    w.hover(160.0, 10.0);
    // The area that is left exits after the other one was entered.
    assert_eq!(
        take(&log),
        [
            "s1 entered",
            "s2 entered",
            "s1 exited",
            "s1 entered",
            "s2 exited"
        ]
    );
    // An area inside a plain item above counts as above.
    w.hover(310.0, 110.0);
    assert_eq!(w.hovered(["e1", "e2"]), [false, true]);
}

#[test]
fn what_does_not_take_hover_does_not_block_it() {
    let mut w = World::new(
        r#"
    MouseArea { objectName: "a1"; width: 50; height: 50; hoverEnabled: true }
    MouseArea { objectName: "a2"; width: 50; height: 50 }
    MouseArea { objectName: "b1"; x: 60; width: 50; height: 50; hoverEnabled: true }
    Rectangle { x: 60; width: 50; height: 50 }
    MouseArea { objectName: "f1"; y: 100; width: 50; height: 50; hoverEnabled: true }
    MouseArea { objectName: "f2"; y: 100; width: 50; height: 50; hoverEnabled: true; visible: false }
"#,
    );
    w.hover(10.0, 10.0);
    assert_eq!(w.hovered(["a1", "a2"]), [true, false]);
    w.hover(70.0, 10.0);
    assert_eq!(w.hovered(["b1"]), [true]);
    w.hover(10.0, 110.0);
    assert_eq!(w.hovered(["f1", "f2"]), [true, false]);
}

#[test]
fn a_nested_area_hovers_together_with_the_areas_around_it() {
    let mut w = World::new(
        r#"
    MouseArea { objectName: "d1"; width: 100; height: 50; hoverEnabled: true
        MouseArea { objectName: "d2"; width: 80; height: 50; hoverEnabled: true
            MouseArea { objectName: "d3"; width: 50; height: 50; hoverEnabled: true } } }
    MouseArea { objectName: "b1"; y: 100; width: 50; height: 50; hoverEnabled: true
        MouseArea { objectName: "b2"; width: 40; height: 40
            MouseArea { objectName: "b3"; width: 20; height: 20; hoverEnabled: true } } }
    MouseArea { objectName: "p"; x: 200; width: 50; height: 50; hoverEnabled: true
        MouseArea { objectName: "c"; x: 40; width: 50; height: 50; hoverEnabled: true } }
"#,
    );
    let log = w.record(&["d1", "d2", "d3"]);
    // The innermost area hears of it first.
    w.hover(10.0, 10.0);
    assert_eq!(take(&log), ["d3 entered", "d2 entered", "d1 entered"]);
    w.hover(60.0, 10.0);
    assert_eq!(take(&log), ["d3 exited"]);
    w.hover(90.0, 10.0);
    assert_eq!(take(&log), ["d2 exited"]);
    w.hover(10.0, 10.0);
    assert_eq!(take(&log), ["d3 entered", "d2 entered"]);
    // Areas exit in the order they were entered.
    w.hover(300.0, 190.0);
    assert_eq!(take(&log), ["d1 exited", "d3 exited", "d2 exited"]);
    // Through an area that does not take hover itself.
    w.hover(10.0, 110.0);
    assert_eq!(w.hovered(["b1", "b2", "b3"]), [true, false, true]);
    // A child that sticks out takes its parent along.
    w.hover(265.0, 10.0);
    assert_eq!(w.hovered(["p", "c"]), [true, true]);
}

#[test]
fn a_disabled_area_keeps_the_hover_from_what_is_beneath() {
    let mut w = World::new(
        r#"
    MouseArea { objectName: "c1"; x: 120; width: 50; height: 50; hoverEnabled: true }
    MouseArea { objectName: "c2"; x: 120; width: 50; height: 50; hoverEnabled: true; enabled: false }
    MouseArea { objectName: "b0"; y: 100; width: 120; height: 50; hoverEnabled: true }
    MouseArea { objectName: "b1"; y: 100; width: 50; height: 50; hoverEnabled: true
        MouseArea { objectName: "b2"; x: 40; width: 50; height: 50; hoverEnabled: true; enabled: false } }
    MouseArea { objectName: "h"; x: 300; width: 50; height: 50; hoverEnabled: true }
"#,
    );
    w.hover(130.0, 10.0);
    assert_eq!(w.hovered(["c1", "c2"]), [false, false]);
    // The disabled child's parent is hovered, inside it or not.
    w.hover(70.0, 110.0);
    assert_eq!(w.hovered(["b0", "b1", "b2"]), [false, true, false]);
    w.hover(45.0, 110.0);
    assert_eq!(w.hovered(["b0", "b1", "b2"]), [false, true, false]);
    // Disabling a hovered area ends its hover at the next move.
    w.hover(310.0, 10.0);
    assert_eq!(w.hovered(["h"]), [true]);
    set(&w.e, w.id("h"), "enabled", ljos_engine::Value::Bool(false));
    w.hover(311.0, 10.0);
    assert_eq!(w.hovered(["h"]), [false]);
}

#[test]
fn mouse_area_enabled_is_the_areas_own_switch() {
    let mut w = World::new(
        r#"
    Item { objectName: "box"; enabled: false; width: 50; height: 50
        MouseArea { objectName: "m1"; anchors.fill: parent }
        Item { objectName: "i0" } }
    MouseArea { objectName: "m2"; x: 60; width: 50; height: 50; enabled: false
        Item { objectName: "i1" }
        MouseArea { objectName: "m3"; width: 20; height: 20 } }
    MouseArea { objectName: "under"; x: 120; width: 50; height: 50 }
    MouseArea { objectName: "over"; x: 120; width: 50; height: 50; enabled: false }
    MouseArea { objectName: "c0"; x: 250; width: 120; height: 50; hoverEnabled: true }
    MouseArea { objectName: "c1"; x: 250; width: 50; height: 50; hoverEnabled: true; enabled: false
        MouseArea { objectName: "c2"; x: 40; width: 50; height: 50; hoverEnabled: true } }
    MouseArea { objectName: "d1"; y: 100; width: 50; height: 50; hoverEnabled: true }
    Item { y: 100; width: 50; height: 50; enabled: false
        MouseArea { objectName: "d2"; width: 50; height: 50; hoverEnabled: true } }
"#,
    );
    // It does not follow a disabled item, and does not disable its own
    // children.
    assert_eq!(
        [w.enabled("m1"), w.enabled("i0")],
        [true, false],
        "inside a disabled Item"
    );
    assert_eq!(
        [w.enabled("m2"), w.enabled("i1"), w.enabled("m3")],
        [false, true, true],
        "inside a disabled MouseArea"
    );
    let log = w.record(&["m1", "m2", "m3", "under", "over"]);
    w.click(10.0, 10.0);
    assert_eq!(clicks(&log), [] as [&str; 0], "an area in a disabled item");
    w.click(70.0, 10.0);
    assert_eq!(clicks(&log), ["m3 clicked"]);
    w.click(100.0, 40.0);
    assert_eq!(clicks(&log), [] as [&str; 0], "a disabled area");
    w.click(130.0, 10.0);
    assert_eq!(clicks(&log), ["under clicked"]);
    // Hover: the child of a disabled area is hovered, the area is not, and
    // nothing beneath either is.
    w.hover(320.0, 10.0);
    assert_eq!(w.hovered(["c0", "c1", "c2"]), [false, false, true]);
    w.hover(295.0, 10.0);
    assert_eq!(w.hovered(["c0", "c1", "c2"]), [false, false, true]);
    w.hover(260.0, 10.0);
    assert_eq!(w.hovered(["c0", "c1", "c2"]), [false, false, false]);
    // An area inside a disabled item is not there for hover at all.
    w.hover(10.0, 110.0);
    assert_eq!(w.hovered(["d1", "d2"]), [true, false]);
    set(&w.e, w.id("box"), "enabled", ljos_engine::Value::Bool(true));
    assert_eq!([w.enabled("m1"), w.enabled("i0")], [true, true]);
}

#[test]
fn entering_an_area_is_also_a_position_change() {
    let mut w = World::new(
        r#"
    property int moves: 0
    property int buttons: -1
    MouseArea { objectName: "p"; width: 100; height: 50; hoverEnabled: true
        onPositionChanged: (mouse) => { moves++; buttons = mouse.buttons } }
"#,
    );
    w.hover(10.0, 10.0);
    assert_eq!(num(&w.e, w.root, "moves"), 1.0);
    w.hover(20.0, 10.0);
    assert_eq!(num(&w.e, w.root, "moves"), 2.0);
    assert_eq!(num(&w.e, w.id("p"), "mouseX"), 20.0);
    // A button that nothing took does not stop the hover, and the hover
    // does not carry it.
    w.press(300.0, 150.0);
    w.drag(30.0, 10.0);
    assert_eq!(num(&w.e, w.root, "moves"), 3.0);
    assert_eq!(num(&w.e, w.root, "buttons"), 0.0);
}

#[test]
fn each_area_gets_its_position_change_before_the_next_area_hears_anything() {
    let mut w = World::new(
        r#"
    MouseArea { objectName: "d1"; width: 100; height: 50; hoverEnabled: true
        MouseArea { objectName: "d2"; width: 50; height: 50; hoverEnabled: true } }
"#,
    );
    let log = w.record_moves(&["d1", "d2"]);
    w.hover(10.0, 10.0);
    assert_eq!(
        take(&log),
        [
            "d2 entered",
            "d2 positionChanged",
            "d1 entered",
            "d1 positionChanged"
        ]
    );
    w.hover(60.0, 10.0);
    assert_eq!(take(&log), ["d1 positionChanged", "d2 exited"]);
}

#[test]
fn entered_reads_the_new_position_and_exited_the_last_one_inside() {
    let mut w = World::new(
        r#"
    property string log
    MouseArea { x: 50; width: 100; height: 50; hoverEnabled: true
        onEntered: log += "in " + mouseX + " " + containsMouse + "|"
        onExited: log += "out " + mouseX + " " + containsMouse + "|"
        onContainsMouseChanged: log += "contains " + mouseX + "|" }
"#,
    );
    w.hover(60.0, 10.0);
    w.hover(70.0, 10.0);
    w.hover(10.0, 10.0);
    assert_eq!(
        &*get(&w.e, w.root, "log").to_js_string(),
        "contains 10|in 10 true|contains 20|out 20 false|"
    );
}

#[test]
fn both_coordinates_tell_of_a_change_when_one_moved() {
    let mut w = World::new(
        r#"
    property string log
    MouseArea { width: 100; height: 50; hoverEnabled: true
        onMouseXChanged: log += "x"
        onMouseYChanged: log += "y"
        onPositionChanged: log += "|" }
"#,
    );
    w.hover(10.0, 10.0);
    w.hover(20.0, 10.0);
    w.hover(20.0, 15.0);
    assert_eq!(&*get(&w.e, w.root, "log").to_js_string(), "xy|xy|xy|");
}

#[test]
fn the_pressed_area_crosses_its_edge_before_its_position_changes() {
    let mut w = World::new(
        r#"
    property string log
    MouseArea { x: 50; width: 100; height: 50; hoverEnabled: true
        onEntered: log += "in " + mouseX + "|"
        onExited: log += "out " + mouseX + "|"
        onPositionChanged: (mouse) => log += "at " + mouseX + " b" + mouse.buttons + "|" }
"#,
    );
    w.hover(60.0, 10.0);
    w.press(60.0, 10.0);
    w.drag(10.0, 10.0);
    w.drag(70.0, 10.0);
    assert_eq!(
        &*get(&w.e, w.root, "log").to_js_string(),
        "in 10|at 10 b0|out 10|at -40 b1|in -40|at 20 b1|"
    );
}

#[test]
fn nothing_else_hovers_while_an_area_holds_the_press() {
    let mut w = World::new(
        r#"
    MouseArea { objectName: "plain"; width: 50; height: 50 }
    MouseArea { objectName: "out"; x: 100; width: 50; height: 50; hoverEnabled: true
        MouseArea { objectName: "in"; width: 20; height: 50; hoverEnabled: true } }
"#,
    );
    let log = w.record_moves(&["plain", "out", "in"]);
    w.press(10.0, 10.0);
    w.drag(110.0, 10.0);
    w.drag(112.0, 10.0);
    assert_eq!(
        take(&log),
        [
            "plain entered",
            "plain exited",
            "plain positionChanged",
            "plain positionChanged"
        ]
    );
    assert_eq!(w.hovered(["out", "in"]), [false, false]);
    // The hover follows at the first move after the release, not at the
    // release.
    w.release(112.0, 10.0);
    assert_eq!(take(&log), ["plain released"]);
    assert_eq!(w.hovered(["out", "in"]), [false, false]);
    w.hover(114.0, 10.0);
    assert_eq!(
        take(&log),
        [
            "in entered",
            "in positionChanged",
            "out entered",
            "out positionChanged"
        ]
    );
}

#[test]
fn the_areas_around_the_pressed_one_keep_their_hover_until_the_move_after_the_release() {
    let mut w = World::new(
        r#"
    MouseArea { objectName: "out"; width: 100; height: 50; hoverEnabled: true
        MouseArea { objectName: "in"; width: 50; height: 50; hoverEnabled: true } }
"#,
    );
    w.hover(10.0, 10.0);
    let log = w.record_moves(&["out", "in"]);
    w.press(10.0, 10.0);
    w.drag(300.0, 150.0);
    assert_eq!(take(&log), ["in exited", "in positionChanged"]);
    assert_eq!(w.hovered(["out", "in"]), [true, false]);
    w.release(300.0, 150.0);
    assert_eq!(take(&log), ["in released"]);
    w.hover(302.0, 150.0);
    assert_eq!(take(&log), ["out exited"]);
}
