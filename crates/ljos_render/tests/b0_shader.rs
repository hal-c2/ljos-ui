//! Shader effects on B0: a paint callback for a host that runs them, the
//! same one while nothing changes, and nothing for a host that does not.
#![cfg(feature = "b0")]

use std::sync::Arc;

use egui::epaint::Shape;
use egui::{Context, Pos2};
use ljos_render::b0::{EguiSink, EpaintBackend, ShaderPaint};
use ljos_render::kurbo::{Point, Rect, Size};
use ljos_render::ljos_shader::{Program, default_fragment, default_vertex};
use ljos_render::{
    Cull, DisplayListBuilder, FrameView, ImageHandle, RenderBackend, RenderTree, ShaderOp,
    ShaderTexture, Viewport,
};

fn op() -> Arc<ShaderOp> {
    let program = Arc::new(Program::new(default_vertex(), default_fragment()).unwrap());
    Arc::new(ShaderOp {
        uniforms: vec![0; program.size as usize],
        program,
        textures: vec![ShaderTexture {
            image: Some(ImageHandle::filled(4, 4, [255, 0, 0, 255]).unwrap()),
            smooth: true,
        }],
        grid: (1, 1),
        blending: true,
        cull: Cull::None,
    })
}

fn encode(be: &mut EpaintBackend, tree: &mut RenderTree, frame: u64, shaders: bool) -> Vec<Shape> {
    let ctx = Context::default();
    let snap = tree.compose(Rect::new(0.0, 0.0, 100.0, 100.0), frame);
    let view = FrameView::new(&snap, 1.0);
    be.sync(&view);
    let mut sink = EguiSink::new(Some(&ctx), Pos2::ZERO);
    sink.shaders = shaders;
    be.encode(&view, &mut sink);
    sink.shapes.into_iter().map(|s| s.shape).collect()
}

#[test]
fn a_shader_is_a_callback_that_stays_the_same() {
    let mut tree = RenderTree::new();
    let n = tree.add_child(tree.root());
    tree.set_pos(n, 10.0, 20.0);
    tree.set_size(n, Size::new(40.0, 30.0));
    let mut b = DisplayListBuilder::new();
    b.shader(Rect::new(0.0, 0.0, 40.0, 30.0), op());
    tree.set_content(n, Some(b.finish_arc()));
    let mut be = EpaintBackend::new();

    assert!(encode(&mut be, &mut tree, 1, false).is_empty());
    let first = encode(&mut be, &mut tree, 2, true);
    let [Shape::Callback(a)] = first.as_slice() else {
        panic!("{first:?}");
    };
    assert_eq!(
        a.rect,
        egui::Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(40.0, 30.0))
    );
    let paint = a.callback.downcast_ref::<ShaderPaint>().unwrap();
    assert!(paint.textures[0].is_some());

    let second = encode(&mut be, &mut tree, 3, true);
    let [Shape::Callback(b)] = second.as_slice() else {
        panic!("{second:?}");
    };
    assert!(Arc::ptr_eq(&a.callback, &b.callback));

    // qt_Matrix takes the item's corners to the viewport's.
    let viewport = Viewport {
        left: 10.0,
        top: 20.0,
        width: 40.0,
        height: 30.0,
    };
    let buf = paint.uniforms(viewport);
    let f = |i: usize| {
        f64::from(f32::from_ne_bytes(
            buf[4 * i..4 * i + 4].try_into().unwrap(),
        ))
    };
    let clip = |p: Point| {
        (
            f(0) * p.x + f(4) * p.y + f(12),
            f(1) * p.x + f(5) * p.y + f(13),
        )
    };
    let near =
        |(x, y): (f64, f64), (ex, ey): (f64, f64)| (x - ex).abs() < 1e-5 && (y - ey).abs() < 1e-5;
    assert!(near(clip(Point::new(0.0, 0.0)), (-1.0, 1.0)));
    assert!(near(clip(Point::new(40.0, 30.0)), (1.0, -1.0)));
    assert_eq!(f(16), 1.0, "qt_Opacity");
}
