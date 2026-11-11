//! `Item.grabToImage` across scenes: an ask wakes only the scene that
//! shows the item, and a grab whose item left its window settles at that
//! scene's next frame and keeps no scene awake.

mod common;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::QuickScene;

const GRABBED: &str = r#"
import QtQuick
Rectangle {
    width: 40; height: 40
    property var asked: "not yet"
    property var got: "not yet"
    Rectangle { id: src; width: 20; height: 10; color: "red" }
    function grab() { asked = src.grabToImage(r => got = r.url) }
    function grabLeaving() {
        asked = src.grabToImage(r => got = "[" + r.url + "]")
        src.parent = null
    }
}
"#;

const OTHER: &str = r#"
import QtQuick
Rectangle { width: 40; height: 40; color: "blue" }
"#;

struct Rig {
    e: Engine,
    root: ObjectId,
    shows: QuickScene,
    other: QuickScene,
}

fn rig() -> Rig {
    let e = engine();
    let root = load(&e, GRABBED);
    let other_root = load(&e, OTHER);
    let mut shows = scene(&e, root, 40.0, 40.0);
    let mut other = scene(&e, other_root, 40.0, 40.0);
    // The engine's timers, the grab's callback among them, wake one
    // scene: a host gives them to the scene whose frames always run.
    shows.take_timer_wake(&e);
    frame(&mut shows, &e);
    frame(&mut other, &e);
    assert!(!shows.needs_frame() && !other.needs_frame(), "both rest");
    Rig {
        e,
        root,
        shows,
        other,
    }
}

/// Runs what is due after a frame, the grab's callback, as a host that
/// wakes at the engine's next deadline does, then the frame of the turn
/// it ran in, which the engine's timers ask of the scene they wake.
fn after_frame(r: &mut Rig) {
    let due =
        r.e.next_timer_deadline()
            .expect("the frame leaves the callback due");
    r.e.advance_timers(due);
    assert!(!r.other.needs_frame(), "a scene without the item rests");
    frame(&mut r.shows, &r.e);
}

/// Nothing is left to do: no scene asks for a frame and no timer waits.
fn rests(r: &mut Rig) {
    for _ in 0..3 {
        assert!(!r.shows.needs_frame(), "the item's scene rests");
        assert!(!r.other.needs_frame(), "a scene without the item rests");
        assert_eq!(r.e.next_timer_deadline(), None, "nothing is scheduled");
        frame(&mut r.shows, &r.e);
        frame(&mut r.other, &r.e);
    }
}

#[test]
fn a_grab_wakes_only_the_scene_that_shows_the_item() {
    let mut r = rig();
    r.e.invoke_by_name(r.root, "grab", &[]).unwrap();
    assert_eq!(get(&r.e, r.root, "asked"), Value::Bool(true));
    assert!(r.shows.needs_frame(), "the item's scene draws the grab");
    assert!(!r.other.needs_frame(), "a scene without the item rests");

    frame(&mut r.shows, &r.e);
    assert!(!r.other.needs_frame(), "a scene without the item rests");
    after_frame(&mut r);
    assert_eq!(
        get(&r.e, r.root, "got").to_js_string().to_string(),
        "itemgrabber:#1"
    );
    rests(&mut r);
}

#[test]
fn a_grab_whose_item_left_its_window_settles_and_wakes_nothing() {
    let mut r = rig();
    r.e.invoke_by_name(r.root, "grabLeaving", &[]).unwrap();
    assert_eq!(get(&r.e, r.root, "asked"), Value::Bool(true));
    assert!(!r.other.needs_frame(), "a scene without the item rests");

    frame(&mut r.shows, &r.e);
    assert!(!r.other.needs_frame(), "a scene without the item rests");
    after_frame(&mut r);
    assert_eq!(get(&r.e, r.root, "got").to_js_string().to_string(), "[]");
    rests(&mut r);
}
