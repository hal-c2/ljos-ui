//! `ShaderEffectSource`: the picture it takes and what a `ShaderEffect`
//! samples of it. How a custom shader samples it (`wrapMode`, `mipmap`) is
//! for the GL painter, which the ledger cannot read pixels from, so it is
//! checked here on the op the painter is given.

mod common;

use std::sync::Arc;

use common::*;
use ljos_engine::{ObjectId, Value};
use ljos_quick::QuickScene;
use ljos_render::{DrawOp, ImageHandle, ShaderOp};

fn ops(s: &QuickScene, item: ObjectId) -> Vec<DrawOp> {
    s.node_of(item)
        .and_then(|n| s.render_tree().node(n))
        .and_then(|n| n.content())
        .map(|l| l.ops().to_vec())
        .unwrap_or_default()
}

/// The picture `item` paints.
fn painted(s: &QuickScene, item: ObjectId) -> ImageHandle {
    ops(s, item)
        .into_iter()
        .find_map(|op| match op {
            DrawOp::Image { image, .. } => Some(image),
            _ => None,
        })
        .expect("an image op")
}

/// The program `item` runs.
fn shader(s: &QuickScene, item: ObjectId) -> Arc<ShaderOp> {
    ops(s, item)
        .into_iter()
        .find_map(|op| match op {
            DrawOp::Shader { op, .. } => Some(op),
            _ => None,
        })
        .expect("a shader op")
}

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255; 4];

/// Four 10-pixel quads, red, green, blue and white, a ShaderEffectSource
/// `s` of them and a ShaderEffect `fx` with Qt's default shaders over it.
const QUADS: &str = r##"
import QtQuick
Rectangle {
    width: 200; height: 100; color: "#000000"
    Item {
        id: quads; x: 150; width: 20; height: 20
        Rectangle { width: 10; height: 10; color: "#ff0000" }
        Rectangle { x: 10; width: 10; height: 10; color: "#00ff00" }
        Rectangle { y: 10; width: 10; height: 10; color: "#0000ff" }
        Rectangle { x: 10; y: 10; width: 10; height: 10; color: "#ffffff" }
    }
    ShaderEffectSource { id: s; objectName: "s"; sourceItem: quads; width: 20; height: 20 }
    ShaderEffect { objectName: "fx"; x: 30; width: 20; height: 20; property var source: s }
}
"##;

fn corners(i: &ImageHandle) -> [[u8; 4]; 4] {
    let (w, h) = (i.width() - 1, i.height() - 1);
    [i.pixel(0, 0), i.pixel(w, 0), i.pixel(0, h), i.pixel(w, h)]
}

#[test]
fn texture_mirroring_flips_what_is_sampled_and_not_what_the_source_draws() {
    let e = engine();
    let root = load(&e, QUADS);
    let (src, fx) = (find(&e, root, "s"), find(&e, root, "fx"));
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    assert_eq!(corners(&painted(&s, fx)), [RED, GREEN, BLUE, WHITE]);

    set(&e, src, "textureMirroring", Value::Int(0));
    frame(&mut s, &e);
    assert_eq!(corners(&painted(&s, fx)), [BLUE, WHITE, RED, GREEN]);
    assert_eq!(corners(&painted(&s, src)), [RED, GREEN, BLUE, WHITE]);

    set(&e, src, "textureMirroring", Value::Int(1));
    frame(&mut s, &e);
    assert_eq!(corners(&painted(&s, fx)), [WHITE, BLUE, GREEN, RED]);
}

#[test]
fn the_picture_is_taken_in_device_pixels() {
    let e = engine();
    let root = load(&e, QUADS);
    let src = find(&e, root, "s");
    let mut s = scene(&e, root, 200.0, 100.0);
    s.set_scale_factor(2.0);
    frame(&mut s, &e);
    let p = painted(&s, src);
    assert_eq!((p.width(), p.height()), (40, 40));
    assert_eq!(corners(&p), [RED, GREEN, BLUE, WHITE]);

    // sourceRect is in the item's coordinates; textureSize is in pixels.
    set(
        &e,
        src,
        "sourceRect",
        Value::Rect(Arc::new(ljos_engine::Rect {
            x: 10.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        })),
    );
    frame(&mut s, &e);
    let p = painted(&s, src);
    assert_eq!((p.width(), p.height()), (20, 20));
    assert_eq!(corners(&p), [GREEN; 4]);
    set(
        &e,
        src,
        "textureSize",
        Value::Size(Arc::new(ljos_engine::Size {
            width: 5.0,
            height: 5.0,
        })),
    );
    frame(&mut s, &e);
    let p = painted(&s, src);
    assert_eq!((p.width(), p.height()), (5, 5));

    // Back at one pixel per logical pixel, it is taken again.
    s.set_scale_factor(1.0);
    set(&e, src, "textureSize", Value::Size(Arc::default()));
    frame(&mut s, &e);
    let p = painted(&s, src);
    assert_eq!((p.width(), p.height()), (10, 10));
}

#[test]
fn a_custom_shader_is_told_how_to_sample_the_picture() {
    let dir = std::env::temp_dir().join(format!("ljos-shader-source-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let frag = "#version 440
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf { mat4 qt_Matrix; float qt_Opacity; };
layout(binding = 1) uniform sampler2D source;
void main() { fragColor = texture(source, qt_TexCoord0 * 2.0) * qt_Opacity; }
";
    let compiled = ljos_shader::compile(frag, ljos_shader::Stage::Fragment).unwrap();
    std::fs::write(dir.join("tex.frag.ljsh"), compiled.to_artifact()).unwrap();
    let url = format!("file://{}/tex.frag.qsb", dir.display());
    let e = engine();
    let root = load(
        &e,
        &QUADS.replace(
            "property var source: s }",
            &format!("property var source: s; fragmentShader: \"{url}\" }}"),
        ),
    );
    let (src, fx) = (find(&e, root, "s"), find(&e, root, "fx"));
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    let t = &shader(&s, fx).textures[0];
    assert_eq!((t.mipmap, t.wrap), (false, [false, false]));
    assert_eq!(t.image.as_ref().map(|i| i.pixel(0, 0)), Some(RED));

    for (mode, wrap) in [(1, [true, false]), (2, [false, true]), (3, [true, true])] {
        set(&e, src, "wrapMode", Value::Int(mode));
        frame(&mut s, &e);
        assert_eq!(shader(&s, fx).textures[0].wrap, wrap, "wrapMode {mode}");
    }
    set(&e, src, "mipmap", Value::Bool(true));
    set(&e, src, "smooth", Value::Bool(false));
    frame(&mut s, &e);
    let t = &shader(&s, fx).textures[0];
    assert!(t.mipmap && !t.smooth);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_source_that_holds_its_shader_effect_source_settles() {
    let e = engine();
    let root = load(
        &e,
        r##"
import QtQuick
Rectangle {
    width: 200; height: 100; color: "#000000"
    Item {
        id: rec; width: 80; height: 40
        Rectangle { width: 20; height: 40; color: "#ff0000" }
        ShaderEffectSource { objectName: "s"; x: 20; width: 60; height: 40; sourceItem: rec }
    }
}
"##,
    );
    let src = find(&e, root, "s");
    let mut s = scene(&e, root, 200.0, 100.0);
    let mut frames = 0;
    loop {
        frame(&mut s, &e);
        frames += 1;
        if !s.needs_frame() {
            break;
        }
        assert!(frames < 100, "still drawing after {frames} frames");
    }
    // Each picture holds the last one, a quarter narrower.
    assert!(frames > 3, "settled after {frames} frames");
    let p = painted(&s, src);
    assert_eq!(p.pixel(0, 20), RED);
    assert!(
        p.pixel(p.width() - 2, 20)[0] > 200,
        "{:?}",
        p.pixel(p.width() - 2, 20)
    );
}

/// A layered item out of sight is still drawn for the effect that comes
/// to sample it, though the frame shows the same items as before.
#[test]
fn a_layer_sampled_later_is_drawn_out_of_sight() {
    let e = engine();
    let root = load(
        &e,
        r##"
import QtQuick
Rectangle {
    width: 200; height: 100; color: "#000000"
    Item {
        objectName: "quads"; x: 500; width: 20; height: 20; layer.enabled: true
        Rectangle { width: 10; height: 10; color: "#ff0000" }
        Rectangle { x: 10; width: 10; height: 10; color: "#00ff00" }
        Rectangle { y: 10; width: 10; height: 10; color: "#0000ff" }
        Rectangle { x: 10; y: 10; width: 10; height: 10; color: "#ffffff" }
    }
    ShaderEffect { objectName: "fx"; x: 30; width: 20; height: 20; property var source: null }
}
"##,
    );
    let (quads, fx) = (find(&e, root, "quads"), find(&e, root, "fx"));
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    set(&e, fx, "source", Value::Object(quads));
    for _ in 0..3 {
        frame(&mut s, &e);
    }
    assert_eq!(corners(&painted(&s, fx)), [RED, GREEN, BLUE, WHITE]);
}
