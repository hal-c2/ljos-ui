//! `MultiEffect` in a scene: a static effect costs nothing after its
//! first build, and a changed input recomposes its picture once.

mod common;

use common::*;
use ljos_engine::{ObjectId, Value};
use ljos_quick::QuickScene;
use ljos_render::{DrawOp, ImageHandle};

const QML: &str = r#"
import QtQuick
import QtQuick.Effects
Rectangle {
    width: 120; height: 120
    color: "black"
    Rectangle { id: src; width: 20; height: 20; color: "red" }
    MultiEffect {
        objectName: "fx"
        source: src
        x: 40; y: 40; width: 20; height: 20
        blurEnabled: true; blur: 0.5; blurMax: 4
        shadowEnabled: true; shadowHorizontalOffset: 3
    }
}
"#;

/// The picture `item` paints, if it paints one.
fn painted(s: &QuickScene, item: ObjectId) -> Option<ImageHandle> {
    s.node_of(item)
        .and_then(|n| s.render_tree().node(n))
        .and_then(|n| n.content())
        .and_then(|l| {
            l.ops().iter().find_map(|op| match op {
                DrawOp::Image { image, .. } => Some(image.clone()),
                _ => None,
            })
        })
}

#[test]
fn a_static_effect_asks_for_no_frame_and_keeps_its_picture() {
    let e = engine();
    let root = load(&e, QML);
    let fx = find(&e, root, "fx");
    let mut s = scene(&e, root, 120.0, 120.0);
    frame(&mut s, &e);
    let first = painted(&s, fx).expect("the effect paints a picture");
    // 20 px item, padded by blurMax * (1 + blurMultiplier) = 4 on each side.
    assert_eq!((first.width(), first.height()), (28, 28));
    assert!(!s.needs_frame(), "a static effect wants no frame");

    for _ in 0..3 {
        frame(&mut s, &e);
        let again = painted(&s, fx).unwrap();
        assert_eq!(again.id(), first.id(), "no pixels recomputed per frame");
        assert!(!s.needs_frame());
    }
}

#[test]
fn a_changed_input_recomposes_the_picture_once() {
    let e = engine();
    let root = load(&e, QML);
    let fx = find(&e, root, "fx");
    let mut s = scene(&e, root, 120.0, 120.0);
    frame(&mut s, &e);
    let before = painted(&s, fx).unwrap();

    set(&e, fx, "brightness", Value::Real(0.5));
    assert!(s.needs_frame(), "an input change asks for a frame");
    frame(&mut s, &e);
    let after = painted(&s, fx).unwrap();
    assert_ne!(after.id(), before.id(), "the picture is recomposed");
    assert!(!s.needs_frame(), "and then the effect rests again");

    frame(&mut s, &e);
    assert_eq!(painted(&s, fx).unwrap().id(), after.id());
}
