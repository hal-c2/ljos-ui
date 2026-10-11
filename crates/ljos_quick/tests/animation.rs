//! Animations: the driver asks for frames only while something that shows
//! is moving, and time comes from the frame clock alone.
//!
//! The rig below stands in for a host: it renders a frame when the scene
//! says it needs one, or when the time the last frame asked to be woken at
//! has come, and at no other moment. Counting those frames is counting the
//! work an idle or a busy scene costs.

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::{QuickScene, running_animation_count, set_animation_frame_period};
use ljos_runtime::{Clock as _, Time, VirtualClock};

/// More frames than any test here may take at one moment.
const MAX_FRAMES_AT_ONCE: usize = 16;

struct Rig {
    e: Engine,
    s: QuickScene,
    clock: Arc<VirtualClock>,
    root: ObjectId,
    /// What the last frame asked for.
    wake: Option<Time>,
}

fn ms(n: u64) -> Time {
    Time::from_millis(n)
}

impl Rig {
    /// Loads `qml` and renders until the scene asks for nothing: what the
    /// first paint costs is not what is counted.
    fn new(qml: &str) -> Rig {
        let e = engine();
        let clock = Arc::new(VirtualClock::new());
        e.set_clock(Rc::new(clock.clone()));
        // A round period, so the frame times below can be written out.
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

    fn call(&self, name: &str, method: &str) {
        self.e
            .invoke_by_name(self.obj(name), method, &[])
            .unwrap_or_else(|err| panic!("{name}.{method}(): {err}"));
    }

    fn set(&self, name: &str, prop: &str, v: Value) {
        set(&self.e, self.obj(name), prop, v);
    }

    fn num(&self, name: &str, prop: &str) -> f64 {
        num(&self.e, self.obj(name), prop)
    }

    fn flag(&self, name: &str, prop: &str) -> bool {
        get(&self.e, self.obj(name), prop).to_boolean()
    }

    fn text(&self, name: &str, prop: &str) -> String {
        get(&self.e, self.obj(name), prop)
            .to_js_string()
            .to_string()
    }

    fn frame(&mut self) {
        let out = self.s.frame(&self.e, self.clock.now());
        self.wake = out.next_wake;
    }

    /// The frames the scene wants at this moment. Returns how many.
    fn frames_now(&mut self) -> usize {
        let now = self.clock.now();
        let mut frames = 0;
        while self.s.needs_frame() || self.wake.is_some_and(|t| t <= now) {
            assert!(frames < MAX_FRAMES_AT_ONCE, "the scene never settles");
            self.frame();
            frames += 1;
        }
        frames
    }

    /// Lets time pass until `end`, rendering only when the scene asks.
    /// Returns the number of frames that took.
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

const ONE: &str = r#"import QtQuick
Item {
  id: root; objectName: "root"
  property string log
  Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
  NumberAnimation {
    objectName: "anim"; target: box; property: "x"; from: 0; to: 100; duration: 100
    onStarted: log += "started "
    onFinished: log += "finished "
  }
}
"#;

#[test]
fn frames_come_while_an_animation_runs_and_stop_when_it_ends() {
    let mut rig = Rig::new(ONE);
    assert!(rig.idle(), "nothing runs yet");
    assert_eq!(rig.run_until(ms(1000)), 0, "an idle scene renders nothing");

    rig.call("anim", "start");
    assert!(rig.s.needs_frame(), "starting asks for the first frame");
    // One frame at the start and one per 10 ms after it, the last of them
    // at the end.
    let frames = rig.run_until(ms(1100));
    assert_eq!(frames, 11);
    assert_eq!(rig.num("box", "x"), 100.0);
    assert_eq!(rig.text("root", "log"), "started finished ");
    assert!(!rig.flag("anim", "running"));
    assert_eq!(running_animation_count(&rig.e), 0);

    assert!(rig.idle(), "nothing is asked for once it ended");
    assert_eq!(rig.run_until(ms(60_000)), 0);
}

#[test]
fn every_frame_shows_the_value_of_its_own_time() {
    let mut rig = Rig::new(ONE);
    rig.call("anim", "start");
    rig.frames_now();
    for t in (10..=100).step_by(10) {
        rig.run_until(ms(t));
        assert_eq!(rig.num("box", "x"), t as f64, "at {t} ms");
    }
}

#[test]
fn a_paused_animation_asks_for_no_frames() {
    let mut rig = Rig::new(ONE);
    rig.call("anim", "start");
    rig.run_until(ms(40));
    assert_eq!(rig.num("box", "x"), 40.0);

    rig.call("anim", "pause");
    // The wake the last frame asked for still comes, and finds nothing to
    // do. After it, nothing.
    assert_eq!(rig.run_until(ms(1000)), 1);
    assert!(rig.idle(), "a paused animation asks for nothing");
    assert_eq!(rig.run_until(ms(10_000)), 0);
    assert_eq!(rig.num("box", "x"), 40.0);
    assert!(rig.flag("anim", "running"));
    assert_eq!(running_animation_count(&rig.e), 1);

    rig.call("anim", "resume");
    // The 60 ms that were left, a frame each 10 ms.
    let frames = rig.run_until(ms(10_030));
    assert_eq!(frames, 4);
    assert_eq!(rig.num("box", "x"), 70.0);
    rig.run_until(ms(11_000));
    assert_eq!(rig.num("box", "x"), 100.0);
    assert!(rig.idle());
}

#[test]
fn a_stopped_animation_asks_for_no_frames() {
    let mut rig = Rig::new(ONE);
    rig.call("anim", "start");
    rig.run_until(ms(30));
    rig.call("anim", "stop");
    // The wake the last frame asked for, and no other.
    assert_eq!(rig.run_until(ms(1000)), 1);
    assert!(rig.idle());
    assert_eq!(rig.run_until(ms(10_000)), 0);
    assert_eq!(running_animation_count(&rig.e), 0);
    assert_eq!(rig.num("box", "x"), 30.0);
}

#[test]
fn a_pause_inside_a_sequence_costs_one_wake() {
    let mut rig = Rig::new(
        r#"import QtQuick
Item {
  objectName: "root"
  Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
  SequentialAnimation {
    objectName: "seq"
    PauseAnimation { duration: 5000 }
    NumberAnimation { target: box; property: "x"; from: 0; to: 100; duration: 100 }
  }
}
"#,
    );
    rig.call("seq", "start");
    // The frame it starts in, then nothing until the pause is over.
    assert_eq!(rig.run_until(ms(4999)), 1);
    assert_eq!(rig.wake, Some(ms(5000)));
    // The frame at 5 s, and ten more for the 100 ms.
    assert_eq!(rig.run_until(ms(6000)), 11);
    assert_eq!(rig.num("box", "x"), 100.0);
    assert!(rig.idle());
}

#[test]
fn a_behavior_asks_for_frames_only_on_the_way() {
    let mut rig = Rig::new(
        r#"import QtQuick
Item {
  objectName: "root"
  Rectangle {
    id: box; objectName: "box"; width: 10; height: 10
    Behavior on x { NumberAnimation { duration: 100 } }
  }
}
"#,
    );
    assert!(rig.idle());
    rig.set("box", "x", Value::Real(100.0));
    assert_eq!(rig.num("box", "x"), 0.0, "the value is held back");
    assert_eq!(rig.run_until(ms(50)), 6);
    assert_eq!(rig.num("box", "x"), 50.0);
    // A new value on the way: from where it is, over the whole duration.
    rig.set("box", "x", Value::Real(0.0));
    rig.run_until(ms(100));
    assert_eq!(rig.num("box", "x"), 25.0);
    rig.run_until(ms(1000));
    assert_eq!(rig.num("box", "x"), 0.0);
    assert!(rig.idle());
    assert_eq!(rig.run_until(ms(60_000)), 0);
}

/// A binding's new value starts the Behavior where the binding ran, not
/// at the next frame: by the time the read that ran the binding returns,
/// the animation runs and `targetValue` is the value.
#[test]
fn a_behavior_on_a_bound_property_starts_where_the_binding_runs() {
    let mut rig = Rig::new(
        r#"import QtQuick
Item {
  id: root; objectName: "root"
  property real goal: 30
  property alias gliding: move.running
  property alias smooth: glide.enabled
  property var aim: glide.targetValue
  Rectangle {
    objectName: "box"; y: root.goal
    Behavior on y { id: glide; NumberAnimation { id: move; duration: 100 } }
  }
}
"#,
    );
    assert!(rig.idle());
    rig.set("root", "goal", Value::Real(80.0));
    assert_eq!(rig.num("box", "y"), 30.0, "the value is held back");
    assert!(rig.flag("root", "gliding"), "no frame was needed");
    assert_eq!(rig.num("root", "aim"), 80.0);
    rig.run_until(ms(50));
    assert_eq!(rig.num("box", "y"), 55.0);

    // Let through, the value is there and the run is over at once.
    rig.set("root", "smooth", Value::Bool(false));
    rig.set("root", "goal", Value::Real(90.0));
    assert_eq!(rig.num("box", "y"), 90.0);
    assert!(!rig.flag("root", "gliding"));
    assert_eq!(rig.num("root", "aim"), 90.0);
    rig.frames_now();
    assert!(rig.idle());
    assert_eq!(rig.run_until(ms(60_000)), 0);
    assert_eq!(rig.num("box", "y"), 90.0);
}

const HIDDEN: &str = r#"import QtQuick
Item {
  id: root; objectName: "root"
  property string log
  Item {
    id: page; objectName: "page"; visible: false
    Rectangle { id: box; objectName: "box"; width: 10; height: 10 }
  }
  NumberAnimation {
    objectName: "anim"; target: box; property: "x"; from: 0; to: 100; duration: 200
    easing.type: Easing.InOutQuad
    onFinished: log += "finished "
  }
  NumberAnimation {
    objectName: "forever"; target: box; property: "y"; from: 0; to: 100; duration: 200
    loops: Animation.Infinite
  }
}
"#;

#[test]
fn an_animation_of_a_hidden_item_costs_its_first_and_last_frame() {
    let mut rig = Rig::new(HIDDEN);
    rig.call("anim", "start");
    // No frame in between: nobody would see the difference.
    assert_eq!(rig.run_until(ms(199)), 1);
    assert_eq!(rig.wake, Some(ms(200)));
    assert_eq!(rig.run_until(ms(1000)), 1);
    assert_eq!(rig.num("box", "x"), 100.0);
    assert_eq!(rig.text("root", "log"), "finished ");
    assert!(rig.idle());
}

#[test]
fn an_item_shown_on_the_way_is_where_an_uninterrupted_run_would_be() {
    // The same animation, seen all the way.
    let mut seen = Rig::new(HIDDEN);
    seen.set("page", "visible", Value::Bool(true));
    seen.call("anim", "start");
    assert_eq!(seen.run_until(ms(70)), 8);
    let expected = seen.num("box", "x");
    assert!(expected > 0.0 && expected < 100.0);

    let mut rig = Rig::new(HIDDEN);
    rig.call("anim", "start");
    assert_eq!(rig.run_until(ms(70)), 1);
    rig.set("page", "visible", Value::Bool(true));
    assert!(rig.s.needs_frame(), "showing the item brings a frame");
    rig.frames_now();
    assert_eq!(rig.num("box", "x"), expected);

    // From here on it is seen, and gets its frames.
    seen.run_until(ms(130));
    assert_eq!(rig.run_until(ms(130)), 6);
    assert_eq!(rig.num("box", "x"), seen.num("box", "x"));
    rig.run_until(ms(1000));
    assert_eq!(rig.num("box", "x"), 100.0);
    assert!(rig.idle());
}

#[test]
fn an_endless_animation_of_a_hidden_item_asks_for_nothing() {
    let mut rig = Rig::new(HIDDEN);
    rig.call("forever", "start");
    assert_eq!(rig.run_until(ms(60_050)), 1);
    assert!(rig.idle(), "no wake, however long it runs");
    assert!(rig.flag("forever", "running"));

    // 60 050 ms is a quarter into a 200 ms pass.
    rig.set("page", "visible", Value::Bool(true));
    rig.frames_now();
    assert_eq!(rig.num("box", "y"), 25.0);
    assert!(!rig.idle(), "shown, it asks for frames again");
    assert_eq!(rig.run_until(ms(60_100)), 5);
    assert_eq!(rig.num("box", "y"), 50.0);

    rig.set("page", "visible", Value::Bool(false));
    rig.frames_now();
    assert!(rig.idle(), "hidden again, it asks for nothing");
    assert_eq!(rig.run_until(ms(120_000)), 0);
}

/// What qml6 showed, tick for tick; the format is described where the
/// motion itself is tested (`animation/smooth.rs`).
const SMOOTHED_QT: &str = include_str!("../src/animation/fixtures/smoothed_qt.txt");

/// qml6 prints five decimals.
const SMOOTHED_TOLERANCE: f64 = 2e-5;

/// The whole path from QML to the property: the Behavior hands the value
/// over, the animation plans the way, the driver shows it frame by frame,
/// and a second value on the way starts from the speed of the first.
#[test]
fn a_smoothed_behavior_shows_what_qt_shows_tick_for_tick() {
    let mut checked = 0;
    for line in SMOOTHED_QT.lines().filter(|l| !l.trim().is_empty()) {
        let mut words = line.split_whitespace();
        let mut head = || words.next().unwrap();
        let (velocity, duration, easing) = (head(), head(), head());
        let mode = match head() {
            "1" => "Immediate",
            "2" => "Sync",
            _ => "Eased",
        };
        let second: f64 = [head(), head()][1].parse().unwrap();
        let mut rig = Rig::new(&format!(
            r#"import QtQuick
Item {{
  objectName: "root"
  property real v: 0
  Behavior on v {{
    SmoothedAnimation {{
      velocity: {velocity}; duration: {duration}; maximumEasingTime: {easing}
      reversingMode: SmoothedAnimation.{mode}
    }}
  }}
}}
"#
        ));
        rig.set("root", "v", Value::Real(100.0));
        rig.frames_now();
        for word in words {
            if word == "|" {
                rig.set("root", "v", Value::Real(second));
                rig.frames_now();
                continue;
            }
            let (at, value) = word.split_once(':').unwrap();
            let at = ms(at.parse::<f64>().unwrap().round() as u64);
            let value: f64 = value.parse().unwrap();
            if at > rig.clock.now() {
                rig.clock.set(at);
                rig.frame();
            }
            let got = rig.num("root", "v");
            assert!(
                (got - value).abs() <= SMOOTHED_TOLERANCE,
                "{line:.40}: at {at:?} Qt shows {value}, this shows {got}"
            );
            checked += 1;
        }
        rig.run_until(ms(10_000));
        assert!(rig.idle(), "{line:.40}: there, it asks for nothing");
    }
    assert!(checked > 1000, "only {checked} values in the fixture");
}

const SMOOTHED: &str = r#"import QtQuick
Item {
  objectName: "root"
  Rectangle {
    objectName: "box"; width: 10; height: 10
    Behavior on x { SmoothedAnimation { velocity: 100 } }
  }
  Item {
    objectName: "page"; visible: false
    Rectangle {
      objectName: "unseen"; width: 10; height: 10
      Behavior on x { SmoothedAnimation { velocity: 100 } }
    }
  }
}
"#;

#[test]
fn a_smoothed_animation_asks_for_frames_only_on_the_way() {
    let mut rig = Rig::new(SMOOTHED);
    rig.set("box", "x", Value::Real(10.0));
    // A tenth of a second at one frame every 10 ms, and the frame it
    // starts in.
    assert_eq!(rig.run_until(ms(100)), 11);
    assert_eq!(rig.num("box", "x"), 10.0);
    assert!(rig.idle());
    assert_eq!(rig.run_until(ms(60_000)), 0);
}

#[test]
fn a_smoothed_animation_of_a_hidden_item_costs_its_first_and_last_frame() {
    let mut rig = Rig::new(SMOOTHED);
    rig.set("unseen", "x", Value::Real(10.0));
    assert_eq!(rig.run_until(ms(99)), 1);
    assert_eq!(rig.wake, Some(ms(100)));
    assert_eq!(rig.run_until(ms(1000)), 1);
    assert_eq!(rig.num("unseen", "x"), 10.0);
    assert!(rig.idle());
}

#[test]
fn a_smoothed_animation_shown_on_the_way_is_where_an_uninterrupted_run_would_be() {
    let mut seen = Rig::new(SMOOTHED);
    seen.set("box", "x", Value::Real(100.0));
    seen.run_until(ms(400));
    let mut rig = Rig::new(SMOOTHED);
    rig.set("unseen", "x", Value::Real(100.0));
    rig.run_until(ms(400));
    rig.set("page", "visible", Value::Bool(true));
    rig.frames_now();
    assert_eq!(rig.num("unseen", "x"), seen.num("box", "x"));
    // And it turns towards a new value from the speed it would have had.
    seen.set("box", "x", Value::Real(0.0));
    rig.set("unseen", "x", Value::Real(0.0));
    seen.run_until(ms(500));
    rig.run_until(ms(500));
    assert_eq!(rig.num("unseen", "x"), seen.num("box", "x"));
    assert!(seen.num("box", "x") > 32.0, "it was still going up");
}

/// What qml6 shows for a `SpringAnimation` in a `Behavior` (see the
/// fixture's own test in `animation/spring.rs` for how a line reads).
const SPRING_QT: &str = include_str!("../src/animation/fixtures/spring_qt.txt");

/// qml6 prints five decimals.
const SPRING_TOLERANCE: f64 = 2e-5;

#[test]
fn a_spring_behavior_shows_what_qt_shows_tick_for_tick() {
    let mut checked = 0;
    for line in SPRING_QT.lines().filter(|l| !l.trim().is_empty()) {
        let mut words = line.split_whitespace();
        let mut head = || words.next().unwrap();
        let (spring, damping, mass) = (head(), head(), head());
        let (velocity, epsilon, modulus, start) = (head(), head(), head(), head());
        let mut rig = Rig::new(&format!(
            r#"import QtQuick
Item {{
  objectName: "root"
  property real v: {start}
  property bool going: anim.running
  Behavior on v {{
    SpringAnimation {{
      id: anim
      spring: {spring}; damping: {damping}; mass: {mass}
      velocity: {velocity}; epsilon: {epsilon}; modulus: {modulus}
    }}
  }}
}}
"#
        ));
        let mut ends = true;
        // Each tick after the last new value, and whether it was running.
        let mut seen: Vec<(Time, bool)> = Vec::new();
        while let Some(word) = words.next() {
            if word == "|" {
                let to: f64 = words.next().unwrap().parse().unwrap();
                rig.set("root", "v", Value::Real(to));
                rig.frames_now();
                seen.clear();
                continue;
            }
            if let Some(end) = word.strip_prefix("end:") {
                match end.parse::<u64>() {
                    // Qt says it is over at this tick and not before.
                    Ok(end) => {
                        for (at, going) in &seen {
                            assert_eq!(*going, *at < ms(end), "{line:.40}: going at {at:?}");
                        }
                        rig.run_until(ms(end));
                        assert!(!rig.flag("root", "going"), "{line:.40}: going at {end} ms");
                    }
                    Err(_) => ends = false,
                }
                continue;
            }
            let (at, value) = word.split_once(':').unwrap();
            let at = ms(at.parse().unwrap());
            let value: f64 = value.parse().unwrap();
            if at > rig.clock.now() {
                rig.clock.set(at);
                rig.frame();
            }
            let got = rig.num("root", "v");
            assert!(
                (got - value).abs() <= SPRING_TOLERANCE,
                "{line:.40}: at {at:?} Qt shows {value}, this shows {got}"
            );
            if seen.last().is_none_or(|(last, _)| *last < at) {
                seen.push((at, rig.flag("root", "going")));
            }
            checked += 1;
        }
        if ends {
            rig.run_until(ms(60_000));
            assert!(rig.idle(), "{line:.40}: at rest, it asks for nothing");
        }
    }
    assert!(checked > 1000, "only {checked} values in the fixture");
}

const SPRUNG: &str = r#"import QtQuick
Item {
  objectName: "root"
  property int whole: 0
  Behavior on whole { SpringAnimation { spring: 2; damping: 0.2 } }
  Rectangle {
    objectName: "box"; width: 10; height: 10
    Behavior on x { SpringAnimation { spring: 2; damping: 0.2 } }
    Behavior on y { SpringAnimation { } }
  }
  Item {
    objectName: "page"; visible: false
    Rectangle {
      objectName: "unseen"; width: 10; height: 10
      Behavior on x { SpringAnimation { spring: 2 } }
    }
  }
}
"#;

#[test]
fn a_spring_asks_for_frames_until_it_rests_and_then_for_nothing() {
    let mut rig = Rig::new(SPRUNG);
    rig.set("box", "x", Value::Real(100.0));
    assert!(
        rig.run_until(ms(1000)) >= 100,
        "a frame each period on the way"
    );
    rig.run_until(ms(3000));
    assert_eq!(rig.num("box", "x"), 100.0);
    assert_eq!(running_animation_count(&rig.e), 0);
    assert!(rig.idle());
    assert_eq!(rig.run_until(ms(60_000)), 0);
}

#[test]
fn a_spring_without_a_spring_and_a_velocity_costs_one_frame() {
    let mut rig = Rig::new(SPRUNG);
    rig.set("box", "y", Value::Real(100.0));
    rig.frames_now();
    assert_eq!(rig.num("box", "y"), 0.0);
    assert_eq!(running_animation_count(&rig.e), 0);
    assert!(rig.idle());
}

#[test]
fn a_spring_that_never_rests_asks_for_nothing_while_its_item_is_hidden() {
    let mut rig = Rig::new(SPRUNG);
    rig.set("unseen", "x", Value::Real(100.0));
    rig.frames_now();
    assert_eq!(running_animation_count(&rig.e), 1);
    assert_eq!(rig.run_until(ms(60_000)), 0, "nobody sees it swing");
}

#[test]
fn a_spring_on_a_whole_number_goes_on_from_the_place_it_was_at() {
    // The property can only show the place rounded; a new value on the
    // way must not make the spring set out from the rounded place.
    let mut rig = Rig::new(SPRUNG);
    let mut real = Rig::new(SPRUNG);
    rig.set("root", "whole", Value::Int(100));
    real.set("box", "x", Value::Real(100.0));
    rig.run_until(ms(200));
    real.run_until(ms(200));
    rig.set("root", "whole", Value::Int(20));
    real.set("box", "x", Value::Real(20.0));
    for at in [300, 450, 700] {
        rig.run_until(ms(at));
        real.run_until(ms(at));
        assert_eq!(rig.num("root", "whole"), real.num("box", "x").round());
    }
}
