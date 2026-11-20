//! A native type hands an item an event of its own making and hears at
//! once whether it was taken (`ljos::input::send_input`): a key row's key
//! for a terminal that has no focus, a finger's drag as the mouse a
//! selection reads, a scroll as a wheel. The item gets it in its own
//! coordinates; nothing is hit-tested and the focus stays where it was.

use std::cell::RefCell;

use ljos::input::{self, Input, InputOrigin, KeyEvent, MouseButtons, PointerEvent, WheelEvent};
use ljos::item::{self, KeyItem, PointerItem, WheelItem, key, kurbo};
use ljos::prelude::*;
use ljos::testing::TestWindow;

thread_local! {
    static SEEN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn note(s: String) {
    SEEN.with(|v| v.borrow_mut().push(s));
}

fn seen() -> Vec<String> {
    SEEN.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

/// Takes the letter A, the pointer, and the wheel.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Pads", name = "Pad", extends = Item)]
pub struct Pad {
    #[qml(base)]
    pub item: Item,
}

/// Takes the key Escape only: what its children leave reaches it.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Pads", name = "Frame", extends = Item)]
pub struct Frame {
    #[qml(base)]
    pub item: Item,
}

fn name(engine: &Engine, o: ObjectId) -> String {
    engine.read_by_name(o, "objectName").to_js_string().as_str().to_owned()
}

fn pad_press(engine: &Engine, o: ObjectId, ev: &KeyEvent) -> bool {
    note(format!("{} key {}", name(engine, o), ev.key));
    ev.key == key::A
}

fn frame_press(engine: &Engine, o: ObjectId, ev: &KeyEvent) -> bool {
    note(format!("{} key {}", name(engine, o), ev.key));
    ev.key == key::ESCAPE
}

fn refuse(_: &Engine, _: ObjectId, _: &KeyEvent) -> bool {
    false
}

fn at(p: kurbo::Point) -> String {
    format!("{},{}", p.x, p.y)
}

fn setup(engine: &Engine) -> (ObjectId, ObjectId) {
    engine.register_module(ljos::module!("Pads" 1.0 => [Pad, Frame]));
    item::register_key_item::<Pad>(
        engine,
        KeyItem {
            shortcut_override: refuse,
            press: pad_press,
            release: refuse,
        },
    );
    item::register_key_item::<Frame>(
        engine,
        KeyItem {
            shortcut_override: refuse,
            press: frame_press,
            release: refuse,
        },
    );
    item::register_pointer_item::<Pad>(
        engine,
        PointerItem {
            press: |_, _, ev, local| {
                note(format!("press {} in scene {}", at(local), at(ev.pos)));
                true
            },
            moved: |_, _, _, local| note(format!("move {}", at(local))),
            release: |_, _, _, local| note(format!("release {}", at(local))),
            cancel: |_, _| note("cancel".into()),
            hover: |_, _, _| {},
        },
    );
    item::register_wheel_item::<Pad>(
        engine,
        WheelItem {
            wheel: |_, _, ev, local| {
                note(format!("wheel {} by {}", at(local), ev.pixel_delta.y));
                true
            },
        },
    );
    let frame = engine.create_by_name("Pads", "Frame", None).unwrap();
    engine
        .write_by_name(frame, "objectName", "frame".into())
        .unwrap();
    engine.set(frame, Item::WIDTH, 200.0).unwrap();
    engine.set(frame, Item::HEIGHT, 200.0).unwrap();
    let pad = engine.create_by_name("Pads", "Pad", Some(frame)).unwrap();
    engine
        .write_by_name(pad, "objectName", "pad".into())
        .unwrap();
    engine
        .write_by_name(pad, "parent", Value::Object(frame))
        .unwrap();
    engine.set(pad, Item::X, 50.0).unwrap();
    engine.set(pad, Item::Y, 20.0).unwrap();
    engine.set(pad, Item::WIDTH, 100.0).unwrap();
    engine.set(pad, Item::HEIGHT, 100.0).unwrap();
    (frame, pad)
}

#[test]
fn a_key_reaches_an_item_without_focus_and_what_it_leaves_goes_up() {
    let engine = ljos::engine();
    let mut window = TestWindow::new(&engine, 200.0, 200.0);
    let (frame, pad) = setup(&engine);
    window.show(frame).unwrap();
    assert!(!engine.read_by_name(pad, "activeFocus").to_boolean());

    assert!(input::send_input(
        &engine,
        pad,
        &Input::Key(KeyEvent::press(key::A))
    ));
    assert_eq!(seen(), [format!("pad key {}", key::A)]);

    assert!(input::send_input(
        &engine,
        pad,
        &Input::Key(KeyEvent::press(key::ESCAPE))
    ));
    assert_eq!(
        seen(),
        [
            format!("pad key {}", key::ESCAPE),
            format!("frame key {}", key::ESCAPE)
        ]
    );

    assert!(!input::send_input(
        &engine,
        pad,
        &Input::Key(KeyEvent::press(key::SPACE))
    ));
    seen();
    assert!(
        !engine.read_by_name(pad, "activeFocus").to_boolean(),
        "the focus stays where it was"
    );
}

#[test]
fn a_pointer_reaches_the_item_at_its_own_point() {
    let engine = ljos::engine();
    let mut window = TestWindow::new(&engine, 200.0, 200.0);
    let (frame, pad) = setup(&engine);
    window.show(frame).unwrap();
    let t = window.now();
    let send = |ev: PointerEvent| input::send_input(&engine, pad, &Input::Pointer(ev));

    assert!(send(PointerEvent::press(10.0, 5.0, t)));
    assert!(send(PointerEvent::moved(30.0, 5.0, MouseButtons::LEFT, t)));
    assert!(send(PointerEvent::release(30.0, 6.0, t)));
    assert_eq!(
        seen(),
        ["press 10,5 in scene 60,25", "move 30,5", "release 30,6"]
    );
    // Not hit-tested: a point outside the item is still the item's.
    assert!(send(PointerEvent::press(-5.0, 500.0, t)));
    assert_eq!(seen(), ["press -5,500 in scene 45,520"]);
    // The frame takes no pointer.
    assert!(!input::send_input(
        &engine,
        frame,
        &Input::Pointer(PointerEvent::press(1.0, 1.0, t))
    ));
}

#[test]
fn a_wheel_reaches_the_item_at_its_own_point() {
    let engine = ljos::engine();
    let mut window = TestWindow::new(&engine, 200.0, 200.0);
    let (frame, pad) = setup(&engine);
    window.show(frame).unwrap();
    let mut ev = WheelEvent::pixels(50.0, 50.0, 0.0, -32.0, window.now());
    assert!(input::send_input(&engine, pad, &Input::Wheel(ev)));
    assert_eq!(seen(), ["wheel 50,50 by -32"]);
    ev.pos = kurbo::Point::new(1.0, 1.0);
    assert!(!input::send_input(&engine, frame, &Input::Wheel(ev)));
}

#[test]
fn a_watcher_hears_a_sent_event_as_synthetic() {
    let engine = ljos::engine();
    let mut window = TestWindow::new(&engine, 200.0, 200.0);
    let (frame, pad) = setup(&engine);
    window.show(frame).unwrap();
    let heard = std::rc::Rc::new(RefCell::new(Vec::new()));
    let log = heard.clone();
    input::watch_input(&engine, frame, move |_, w| {
        log.borrow_mut().push((w.origin, w.taken, w.input.clone()));
    });
    let t = window.now();
    input::send_input(
        &engine,
        pad,
        &Input::Pointer(PointerEvent::press(10.0, 5.0, t)),
    );
    seen();
    let heard = heard.borrow();
    assert_eq!(heard.len(), 1);
    let (origin, taken, ref ev) = heard[0];
    assert_eq!((origin, taken), (InputOrigin::Synthetic, true));
    let Input::Pointer(ev) = ev else {
        panic!("{ev:?}")
    };
    assert_eq!(
        ev.pos,
        kurbo::Point::new(60.0, 25.0),
        "in the scene's pixels"
    );
}

#[test]
fn a_gone_item_takes_nothing() {
    let engine = ljos::engine();
    let (frame, pad) = setup(&engine);
    engine.destroy(pad);
    let _ = frame;
    assert!(!input::send_input(
        &engine,
        pad,
        &Input::Key(KeyEvent::press(key::A))
    ));
    assert!(seen().is_empty());
}
