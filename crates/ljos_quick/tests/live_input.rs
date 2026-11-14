//! Input an application gives a live window (`post_input`) and input it
//! watches an item get (`watch_input`), the user's and the rest told
//! apart (#459).

mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use common::*;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::keys::key;
use ljos_quick::{
    Input, InputOrigin, KeyEvent, MouseButtons, PointerEvent, PointerKind, PostInputError,
    QuickScene, WheelEvent, post_input, unwatch_input, watch_input,
};
use ljos_runtime::Time;

const MAIN: &str = r#"
import QtQuick
Item {
    id: root
    width: 200; height: 100
    property int clicks: 0
    property int wheels: 0
    MouseArea {
        objectName: "left"
        x: 0; y: 0; width: 100; height: 100
        onClicked: root.clicks += 1
        onWheel: (wheel) => root.wheels += 1
    }
    Item {
        objectName: "right"
        x: 100; y: 0; width: 100; height: 100
        TextInput {
            objectName: "field"
            anchors.fill: parent
            focus: true
        }
    }
}
"#;

fn world() -> (Engine, ObjectId, QuickScene) {
    let e = engine();
    let root = load(&e, MAIN);
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    (e, root, s)
}

fn click(x: f64, y: f64) -> [Input; 2] {
    [
        Input::Pointer(PointerEvent::press(x, y, Time::ZERO)),
        Input::Pointer(PointerEvent::release(x, y, Time::ZERO)),
    ]
}

/// The kind and origin of each event the watcher of `item` sees.
type Seen = Rc<RefCell<Vec<(&'static str, InputOrigin)>>>;

fn watch(e: &Engine, item: ObjectId) -> (Seen, ljos_quick::InputWatch) {
    let seen: Seen = Rc::default();
    let into = Rc::clone(&seen);
    let w = watch_input(e, item, move |_, got| {
        let kind = match got.input {
            Input::Pointer(p) => match p.kind {
                PointerKind::Press => "press",
                PointerKind::Release => "release",
                PointerKind::Move => "move",
                _ => "pointer",
            },
            Input::Key(_) => "key",
            Input::Wheel(_) => "wheel",
        };
        into.borrow_mut().push((kind, got.origin));
    });
    (seen, w)
}

#[test]
fn a_posted_click_reaches_what_is_under_it_at_the_next_frame() {
    let (e, root, mut s) = world();
    for input in click(50.0, 50.0) {
        post_input(&e, root, input).unwrap();
    }
    // Queued, not delivered in the call: the host's frame hands it on.
    assert_eq!(num(&e, root, "clicks"), 0.0);
    assert!(s.needs_frame(), "a post asks the host for a frame");
    frame(&mut s, &e);
    assert_eq!(num(&e, root, "clicks"), 1.0);
    // Nothing under the right half takes a click.
    for input in click(150.0, 50.0) {
        post_input(&e, root, input).unwrap();
    }
    frame(&mut s, &e);
    assert_eq!(num(&e, root, "clicks"), 1.0);
}

#[test]
fn a_post_wakes_a_host_that_is_at_rest() {
    let (e, root, mut s) = world();
    let woken = Rc::new(Cell::new(0));
    let w = Rc::clone(&woken);
    s.set_waker(move || w.set(w.get() + 1));
    frame(&mut s, &e);
    let before = woken.get();
    post_input(
        &e,
        root,
        Input::Pointer(PointerEvent::moved(
            10.0,
            10.0,
            MouseButtons::empty(),
            Time::ZERO,
        )),
    )
    .unwrap();
    assert_eq!(woken.get(), before + 1);
}

#[test]
fn a_position_posted_to_an_item_is_in_its_coordinates() {
    let (e, root, mut s) = world();
    let right = find(&e, root, "right");
    // (-50, 50) in `right` is (50, 50) in the scene: over `left`.
    for input in click(-50.0, 50.0) {
        post_input(&e, right, input).unwrap();
    }
    frame(&mut s, &e);
    assert_eq!(num(&e, root, "clicks"), 1.0);
}

#[test]
fn posted_keys_and_wheels_go_where_the_users_would() {
    let (e, root, mut s) = world();
    let field = find(&e, root, "field");
    for k in [
        KeyEvent::press(key::A).with_text("a"),
        KeyEvent::release(key::A).with_text("a"),
    ] {
        post_input(&e, root, Input::Key(k)).unwrap();
    }
    post_input(
        &e,
        root,
        Input::Wheel(WheelEvent::notches(50.0, 50.0, 0.0, 1.0, Time::ZERO)),
    )
    .unwrap();
    frame(&mut s, &e);
    assert_eq!(get(&e, field, "text").to_js_string().as_str(), "a");
    assert_eq!(num(&e, root, "wheels"), 1.0);
}

#[test]
fn a_window_no_scene_shows_takes_no_post() {
    let (e, _, _s) = world();
    let other = load(&e, "import QtQuick\nItem {}\n");
    let posted = post_input(&e, other, click(1.0, 1.0)[0].clone());
    assert_eq!(posted, Err(PostInputError::NotShown(other)));
}

#[test]
fn a_watcher_tells_the_users_input_from_posted_input() {
    let (e, root, mut s) = world();
    let left = find(&e, root, "left");
    let (seen, _) = watch(&e, left);

    // The window system's: a host hands the scene its events.
    s.pointer_event(&e, &PointerEvent::press(50.0, 50.0, Time::ZERO));
    s.pointer_event(&e, &PointerEvent::release(50.0, 50.0, Time::ZERO));
    // An application's.
    for input in click(50.0, 50.0) {
        post_input(&e, root, input).unwrap();
    }
    frame(&mut s, &e);
    // A driver's (an agent's inspector, a test).
    s.with_input_origin(InputOrigin::Synthetic, |s| {
        s.wheel_event(&e, &WheelEvent::notches(50.0, 50.0, 0.0, 1.0, Time::ZERO))
    });

    use InputOrigin::{Synthetic, User};
    assert_eq!(
        *seen.borrow(),
        [
            // The pointer comes to the press first, as on qml6.
            ("move", User),
            ("press", User),
            ("release", User),
            ("press", Synthetic),
            ("release", Synthetic),
            ("wheel", Synthetic),
        ]
    );
    // Watching took nothing: the area got both clicks.
    assert_eq!(num(&e, root, "clicks"), 2.0);
}

#[test]
fn a_watcher_sees_only_what_is_routed_into_its_item() {
    let (e, root, mut s) = world();
    let right = find(&e, root, "right");
    let (seen, w) = watch(&e, right);

    // A click on the left half is not the right's.
    s.pointer_event(&e, &PointerEvent::press(50.0, 50.0, Time::ZERO));
    s.pointer_event(&e, &PointerEvent::release(50.0, 50.0, Time::ZERO));
    // A press that begins in it is its own till the release, wherever the
    // pointer goes.
    s.pointer_event(&e, &PointerEvent::press(150.0, 50.0, Time::ZERO));
    s.pointer_event(
        &e,
        &PointerEvent::moved(20.0, 50.0, MouseButtons::LEFT, Time::ZERO),
    );
    s.pointer_event(&e, &PointerEvent::release(20.0, 50.0, Time::ZERO));
    // Keys go to the field inside it, which has the focus.
    s.key_event(&e, &KeyEvent::press(key::A).with_text("a"));

    use InputOrigin::User;
    assert_eq!(
        *seen.borrow(),
        [
            ("move", User),
            ("press", User),
            ("move", User),
            ("release", User),
            ("key", User),
        ]
    );

    assert!(unwatch_input(&e, w));
    assert!(!unwatch_input(&e, w));
    s.key_event(&e, &KeyEvent::press(key::A).with_text("a"));
    assert_eq!(seen.borrow().len(), 5);
}
