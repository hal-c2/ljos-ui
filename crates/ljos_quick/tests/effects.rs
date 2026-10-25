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

/// Two cards of a rounded rectangle with a text on it, each with a
/// shadow, as a page of cards draws them.
const CARDS: &str = r#"
import QtQuick
import QtQuick.Effects
Item {
    width: 200; height: 120
    Rectangle {
        id: a; objectName: "a"
        x: 10; y: 10; width: 60; height: 80; radius: 8; color: "white"
        Text { text: "A" }
    }
    MultiEffect {
        objectName: "fxA"
        source: a; anchors.fill: a
        shadowEnabled: true; shadowOpacity: 0.5
    }
    Rectangle {
        id: b; objectName: "b"
        x: 110; y: 10; width: 60; height: 80; radius: 8; color: "white"
        Text { text: "B" }
    }
    MultiEffect {
        objectName: "fxB"
        source: b; anchors.fill: b
        shadowEnabled: true; shadowOpacity: 0.5
    }
}
"#;

#[test]
fn a_moved_shadow_is_not_drawn_again() {
    let e = engine();
    let root = load(&e, CARDS);
    let (a, fx) = (find(&e, root, "a"), find(&e, root, "fxA"));
    let mut s = scene(&e, root, 200.0, 120.0);
    frame(&mut s, &e);
    let before = painted(&s, fx).expect("the shadow paints a picture");

    set(&e, a, "y", Value::Real(25.0));
    frame(&mut s, &e);
    assert_eq!(painted(&s, fx).unwrap().id(), before.id());
}

#[test]
fn a_resize_draws_again_only_the_shadow_whose_size_changed() {
    let e = engine();
    let root = load(&e, CARDS);
    let a = find(&e, root, "a");
    let (fx_a, fx_b) = (find(&e, root, "fxA"), find(&e, root, "fxB"));
    let mut s = scene(&e, root, 200.0, 120.0);
    frame(&mut s, &e);
    let (a0, b0) = (painted(&s, fx_a).unwrap(), painted(&s, fx_b).unwrap());

    set(&e, a, "width", Value::Real(70.0));
    frame(&mut s, &e);
    let a1 = painted(&s, fx_a).unwrap();
    assert_ne!(a1.id(), a0.id(), "the resized card's shadow is drawn again");
    assert_eq!(a1.width(), a0.width() + 10);
    assert_eq!(
        painted(&s, fx_b).unwrap().id(),
        b0.id(),
        "the other is kept"
    );
}

/// A panel with a shadow that is shown only while it is open, as an
/// overlay is.
const PANEL: &str = r#"
import QtQuick
import QtQuick.Effects
Item {
    width: 200; height: 120
    Rectangle {
        id: panel; objectName: "panel"
        x: 10; y: 10; width: 60; height: 80; radius: 8; color: "white"
        Text { text: "A" }
    }
    MultiEffect {
        objectName: "fx"
        source: panel; anchors.fill: panel
        shadowEnabled: panel.visible; shadowOpacity: 0.5
        visible: panel.visible
    }
}
"#;

#[test]
fn a_closed_panel_s_shadow_is_drawn_again_only_when_it_is_shown() {
    let e = engine();
    let root = load(&e, PANEL);
    let (panel, fx) = (find(&e, root, "panel"), find(&e, root, "fx"));
    let mut s = scene(&e, root, 200.0, 120.0);
    frame(&mut s, &e);
    let open = painted(&s, fx).expect("the open panel has its shadow");
    assert_eq!(open.width(), 60 + 2 * 32, "the panel, padded by blurMax");

    set(&e, panel, "visible", Value::Bool(false));
    let blue = ljos_engine::Color::from_rgba8(0, 0, 255, 255);
    set(&e, panel, "color", Value::Color(blue));
    frame(&mut s, &e);
    assert!(
        painted(&s, fx).is_none_or(|p| p.id() == open.id()),
        "no picture is made of a closed panel"
    );

    set(&e, panel, "visible", Value::Bool(true));
    frame(&mut s, &e);
    let again = painted(&s, fx).expect("open again, it has its shadow");
    assert_ne!(
        again.id(),
        open.id(),
        "with what changed while it was closed"
    );
}

#[test]
fn an_effect_in_a_hidden_item_a_shader_effect_source_samples_is_drawn() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Effects
Item {
    width: 200; height: 120
    Item {
        id: holder
        width: 100; height: 100
        visible: false
        Rectangle { id: card; width: 60; height: 80; radius: 8; Text { text: "A" } }
        MultiEffect { objectName: "fx"; source: card; anchors.fill: card; shadowEnabled: true }
    }
    ShaderEffectSource { sourceItem: holder; x: 100; width: 100; height: 100 }
}
"#,
    );
    let fx = find(&e, root, "fx");
    let mut s = scene(&e, root, 200.0, 120.0);
    frame(&mut s, &e);
    frame(&mut s, &e);
    assert!(painted(&s, fx).is_some(), "what is sampled is drawn");
}
