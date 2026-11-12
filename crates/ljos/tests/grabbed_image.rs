//! Rust reads the picture `Item.grabToImage` takes, from what a script
//! hands it: the result itself, or its `image` or `url`, which name the
//! picture while the result lives. A script that passes `result.image`
//! on to Rust does what a QML file shared with a Qt client does, where it
//! is a `QImage`.

use ljos::item::{self, ItemGrabResult};
use ljos::prelude::*;
use ljos_quick::QuickScene;

mod common;

const GRABBED: &str = r##"
import QtQuick
Rectangle {
    width: 40; height: 40
    property var got: null
    property var held: null
    Rectangle { id: src; width: 3; height: 2; color: "#ff0000" }
    function grab() {
        src.grabToImage(r => { held = r; got = r })
    }
    function release() { held = null; got = null }
}
"##;

/// The grab's callback has run; returns its result.
fn grabbed(engine: &Engine) -> (ObjectId, Value) {
    let root = common::create_main(engine, GRABBED);
    let mut scene = QuickScene::new(item::kurbo::Size::new(40.0, 40.0));
    scene.set_root(engine, root).unwrap();
    scene.take_timer_wake(engine);
    scene.frame(engine, Time::ZERO);
    engine.invoke_by_name(root, "grab", &[]).unwrap();
    scene.frame(engine, Time::ZERO);
    let due = engine.next_timer_deadline().expect("the callback is due");
    engine.advance_timers(due);
    let result = engine.read_by_name(root, "got");
    assert!(matches!(result, Value::Object(_)), "the callback ran");
    (root, result)
}

fn red(picture: &item::ImageHandle) {
    assert_eq!((picture.width(), picture.height()), (3, 2));
    assert_eq!(picture.pixel(2, 1), [255, 0, 0, 255]);
}

#[test]
fn rust_reads_the_picture_by_the_result_its_image_or_its_url() {
    let engine = ljos::engine();
    let (_, result) = grabbed(&engine);
    let Value::Object(obj) = result else {
        unreachable!()
    };
    assert!(engine.with_mut(obj, |_: &mut ItemGrabResult| ()).is_ok());

    red(&item::grabbed_image(&engine, &result).expect("by the result"));
    for name in ["image", "url"] {
        let named = engine.read_by_name(obj, name);
        red(&item::grabbed_image(&engine, &named).unwrap_or_else(|| panic!("by its {name}")));
        // As a script hands it on: a string.
        let text = Value::String(named.to_js_string());
        red(&item::grabbed_image(&engine, &text).unwrap_or_else(|| panic!("by its {name} as text")));
    }
}

#[test]
fn a_result_that_is_gone_names_no_picture() {
    let engine = ljos::engine();
    let (root, result) = grabbed(&engine);
    let Value::Object(obj) = result else {
        unreachable!()
    };
    let url = engine.read_by_name(obj, "url");
    assert!(item::grabbed_image(&engine, &url).is_some());
    engine.invoke_by_name(root, "release", &[]).unwrap();
    drop(result);
    // The collector lets it go; the engine destroys it after.
    engine.collect_garbage();
    engine.process_deferred_deletes();
    assert!(!engine.is_alive(obj), "the scripts let it go");
    assert!(item::grabbed_image(&engine, &url).is_none());
    // Not every url is a picture a grab took.
    let other = Value::Url(Atom::new("qrc:/red.png"));
    assert!(item::grabbed_image(&engine, &other).is_none());
}
