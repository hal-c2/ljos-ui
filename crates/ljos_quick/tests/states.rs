//! States, their transitions and the `Binding` element: what a state
//! holds comes off again whole, and none of it costs a frame once it has
//! settled.
//!
//! What a state does to a property is in `features/states/`; here is what
//! a scenario cannot see: the frames the scene asks for, and that the
//! binding a property had is the same binding afterwards.

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::{Item, QuickScene, running_animation_count, set_animation_frame_period};
use ljos_runtime::{Clock as _, Time, VirtualClock};

/// More frames than any test here may take at one moment.
const MAX_FRAMES_AT_ONCE: usize = 16;

/// A host that renders only when the scene asks (see `animation.rs`).
struct Rig {
    e: Engine,
    s: QuickScene,
    clock: Arc<VirtualClock>,
    root: ObjectId,
    wake: Option<Time>,
}

fn ms(n: u64) -> Time {
    Time::from_millis(n)
}

impl Rig {
    fn new(qml: &str) -> Rig {
        let e = engine();
        let clock = Arc::new(VirtualClock::new());
        e.set_clock(Rc::new(clock.clone()));
        set_animation_frame_period(&e, Duration::from_millis(10));
        let root = load(&e, qml);
        let s = scene(&e, root, 200.0, 200.0);
        let mut rig = Rig {
            e,
            s,
            clock,
            root,
            wake: None,
        };
        rig.frames_now();
        rig
    }

    fn obj(&self, name: &str) -> ObjectId {
        find(&self.e, self.root, name)
    }

    fn set(&self, name: &str, prop: &str, v: Value) {
        set(&self.e, self.obj(name), prop, v);
    }

    fn num(&self, name: &str, prop: &str) -> f64 {
        num(&self.e, self.obj(name), prop)
    }

    fn frames_now(&mut self) -> usize {
        let now = self.clock.now();
        let mut frames = 0;
        while self.s.needs_frame() || self.wake.is_some_and(|t| t <= now) {
            assert!(frames < MAX_FRAMES_AT_ONCE, "the scene never settles");
            self.wake = self.s.frame(&self.e, self.clock.now()).next_wake;
            frames += 1;
        }
        frames
    }

    fn run_until(&mut self, end: Time) -> usize {
        let mut frames = self.frames_now();
        while let Some(t) = self.wake.filter(|t| *t <= end) {
            if t > self.clock.now() {
                self.clock.set(t);
            }
            self.e.advance_timers(self.clock.now());
            frames += self.frames_now();
        }
        self.clock.set(end);
        self.e.advance_timers(end);
        frames + self.frames_now()
    }

    fn idle(&self) -> bool {
        !self.s.needs_frame() && self.wake.is_none()
    }
}

const PLAIN: &str = r#"import QtQuick
Item {
  id: root; objectName: "root"
  property int n: 3
  Rectangle { id: box; objectName: "box"; width: root.n * 10; height: 10 }
  states: State {
    name: "big"
    PropertyChanges { target: box; width: 150 }
  }
}
"#;

#[test]
fn a_change_of_state_without_a_transition_leaves_the_scene_idle() {
    let mut r = Rig::new(PLAIN);
    r.set("root", "state", "big".into());
    assert_eq!(r.num("box", "width"), 150.0);
    r.frames_now();
    assert!(r.idle(), "a state that has been entered asks for nothing");
    assert_eq!(running_animation_count(&r.e), 0);

    r.set("root", "state", "".into());
    assert_eq!(r.num("box", "width"), 30.0);
    r.frames_now();
    assert!(r.idle());
}

#[test]
fn the_binding_under_a_state_is_the_one_that_comes_back() {
    let mut r = Rig::new(PLAIN);
    let b = r.obj("box");
    assert!(r.e.is_bound(b, Item::WIDTH.index));
    for _ in 0..50 {
        r.set("root", "state", "big".into());
        r.set("root", "state", "".into());
    }
    assert!(
        r.e.is_bound(b, Item::WIDTH.index),
        "the item's own binding is back"
    );
    r.set("root", "n", 7.into());
    assert_eq!(r.num("box", "width"), 70.0);
    r.frames_now();
    assert!(r.idle());
}

const MOVING: &str = r#"import QtQuick
Item {
  id: root; objectName: "root"
  Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
  states: State {
    name: "big"
    PropertyChanges { target: box; width: 110 }
  }
  transitions: Transition {
    objectName: "t"
    NumberAnimation { property: "width"; duration: 100 }
  }
}
"#;

#[test]
fn a_transition_asks_for_frames_only_while_it_runs() {
    let mut r = Rig::new(MOVING);
    r.set("root", "state", "big".into());
    assert_eq!(r.num("box", "width"), 10.0, "the way starts where it was");
    assert!(get(&r.e, r.obj("t"), "running").to_boolean());

    let frames = r.run_until(ms(50));
    assert!((r.num("box", "width") - 60.0).abs() < 1e-6);
    assert!(
        frames >= 5,
        "one frame for each period on the way: {frames}"
    );

    r.run_until(ms(200));
    assert_eq!(r.num("box", "width"), 110.0);
    assert!(!get(&r.e, r.obj("t"), "running").to_boolean());
    assert_eq!(running_animation_count(&r.e), 0);
    assert!(r.idle(), "a transition that has ended asks for nothing");
    assert_eq!(r.run_until(ms(5000)), 0);
}

#[test]
fn an_item_destroyed_on_the_way_takes_its_transition_with_it() {
    let mut r = Rig::new(MOVING);
    r.set("root", "state", "big".into());
    r.run_until(ms(30));
    let b = r.obj("box");
    assert!(r.e.destroy_now(b));
    r.run_until(ms(300));
    assert_eq!(running_animation_count(&r.e), 0);
    assert!(r.idle());
    // The state has nothing left to change, and leaving it is no error:
    // the transition takes its time and moves nothing.
    r.set("root", "state", "".into());
    r.run_until(ms(500));
    assert_eq!(running_animation_count(&r.e), 0);
    assert!(r.idle());
}

#[test]
fn the_owner_of_the_states_destroyed_on_the_way_leaves_nothing_running() {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let holder = load(
        &e,
        r#"import QtQuick
Item {
  Item {
    id: inner; objectName: "inner"
    Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
    states: State { name: "big"; PropertyChanges { target: box; width: 110 } }
    transitions: Transition { NumberAnimation { property: "width"; duration: 100 } }
  }
}
"#,
    );
    let mut s = scene(&e, holder, 200.0, 200.0);
    s.frame(&e, clock.now());
    let inner = find(&e, holder, "inner");
    set(&e, inner, "state", "big".into());
    s.frame(&e, clock.now());
    assert!(e.destroy_now(inner));
    clock.set(ms(20));
    let out = s.frame(&e, clock.now());
    let out = if out.next_wake.is_some() {
        s.frame(&e, clock.now())
    } else {
        out
    };
    assert_eq!(running_animation_count(&e), 0);
    assert!(out.next_wake.is_none());
}

const FOLLOWED: &str = r#"import QtQuick
Item {
  id: root; objectName: "root"
  Rectangle {
    id: box; objectName: "box"; width: 10; height: 10
    Behavior on width { NumberAnimation { duration: 100 } }
  }
  states: State {
    name: "big"
    PropertyChanges { target: box; width: 110 }
  }
}
"#;

#[test]
fn a_behavior_that_follows_a_state_is_idle_when_it_has_arrived() {
    let mut r = Rig::new(FOLLOWED);
    r.set("root", "state", "big".into());
    assert_eq!(r.num("box", "width"), 10.0);
    r.run_until(ms(50));
    assert!((r.num("box", "width") - 60.0).abs() < 1e-6);
    r.run_until(ms(200));
    assert_eq!(r.num("box", "width"), 110.0);
    assert_eq!(running_animation_count(&r.e), 0);
    assert!(r.idle());
    assert_eq!(r.run_until(ms(5000)), 0);
}

const DELAYED: &str = r#"import QtQuick
Item {
  id: root; objectName: "root"
  property int src: 5
  Rectangle { id: box; objectName: "box"; width: 1; height: 10 }
  Binding { target: box; property: "width"; value: root.src; delayed: true }
}
"#;

#[test]
fn a_delayed_binding_costs_one_wake_and_then_nothing() {
    let mut r = Rig::new(DELAYED);
    assert_eq!(r.num("box", "width"), 5.0);
    assert!(r.idle());

    r.set("root", "src", 9.into());
    assert_eq!(r.num("box", "width"), 5.0, "not before the caller is done");
    let frames = r.frames_now();
    assert_eq!(r.num("box", "width"), 9.0);
    assert!(frames >= 1);
    assert!(r.idle(), "a value that was passed on asks for nothing");
    assert_eq!(r.run_until(ms(5000)), 0);
}

const HELD: &str = r#"import QtQuick
Item {
  id: root; objectName: "root"
  property int n: 3
  property bool on: false
  Rectangle { id: box; objectName: "box"; width: root.n * 10; height: 10 }
  Binding { target: box; property: "width"; value: 77; when: root.on }
}
"#;

#[test]
fn a_binding_element_gives_the_property_its_own_binding_back() {
    let mut r = Rig::new(HELD);
    let b = r.obj("box");
    for _ in 0..50 {
        r.set("root", "on", true.into());
        assert_eq!(r.num("box", "width"), 77.0);
        r.set("root", "on", false.into());
    }
    assert!(r.e.is_bound(b, Item::WIDTH.index));
    r.set("root", "n", 4.into());
    assert_eq!(r.num("box", "width"), 40.0);
    r.frames_now();
    assert!(r.idle());
}
