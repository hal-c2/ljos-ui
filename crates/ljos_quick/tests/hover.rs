//! Hover delivery and `MouseArea.enabled`.
//!
//! The Qt oracle of the feature ledger cannot inject pointer input, so
//! every expectation here was read off Qt 6.11.2 by running the same QML
//! under `qmltestrunner` (offscreen) with `mouseMove`/`mouseClick`.

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
        let log = Rc::new(RefCell::new(Vec::new()));
        for name in names {
            for sig in ["entered", "exited", "clicked"] {
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
    assert_eq!(
        take(&log),
        [
            "s1 entered",
            "s1 exited",
            "s2 entered",
            "s2 exited",
            "s1 entered"
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
    w.hover(10.0, 10.0);
    assert_eq!(take(&log), ["d1 entered", "d2 entered", "d3 entered"]);
    w.hover(60.0, 10.0);
    assert_eq!(take(&log), ["d3 exited"]);
    w.hover(90.0, 10.0);
    assert_eq!(take(&log), ["d2 exited"]);
    w.hover(10.0, 10.0);
    assert_eq!(take(&log), ["d2 entered", "d3 entered"]);
    w.hover(300.0, 190.0);
    assert_eq!(take(&log), ["d3 exited", "d2 exited", "d1 exited"]);
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
fn a_move_inside_a_hovered_area_is_a_position_change_but_entering_is_not() {
    let mut w = World::new(
        r#"
    property int moves: 0
    MouseArea { objectName: "p"; width: 100; height: 50; hoverEnabled: true
        onPositionChanged: moves++ }
"#,
    );
    w.hover(10.0, 10.0);
    assert_eq!(num(&w.e, w.root, "moves"), 0.0);
    w.hover(20.0, 10.0);
    assert_eq!(num(&w.e, w.root, "moves"), 1.0);
    assert_eq!(num(&w.e, w.id("p"), "mouseX"), 20.0);
}
