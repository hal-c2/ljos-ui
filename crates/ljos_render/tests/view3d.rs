//! 3D views: a paint callback for a painter that draws 3D, and a picture
//! read back from the off-screen renderer everywhere else, let go of when
//! the view is gone.
#![cfg(feature = "offscreen3d")]

use std::sync::Arc;

use ljos_render::kurbo::{Rect, Size};
use ljos_render::ljos_3d::offscreen::Offscreen;
use ljos_render::ljos_3d::{Background, Color, Environment, Scene};
use ljos_render::scene3d::View3dOp;
use ljos_render::{DisplayListBuilder, FrameView, NodeId, RenderTree, VelloCpuReference};

/// No adapter (a machine with no GPU driver and no software Vulkan or GL)
/// is said, not failed: the view is not what is wrong there.
fn has_adapter() -> bool {
    match Offscreen::new() {
        Ok(_) => true,
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            false
        }
    }
}

/// A view that shows nothing but its clear colour, red.
fn red(key: u64) -> Arc<View3dOp> {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::rgb(1.0, 0.0, 0.0)),
    });
    Arc::new(View3dOp {
        key,
        scene: Arc::new(scene),
    })
}

fn add_view(tree: &mut RenderTree, key: u64) -> NodeId {
    let n = tree.add_child(tree.root());
    tree.set_pos(n, 10.0, 20.0);
    tree.set_size(n, Size::new(40.0, 30.0));
    let mut b = DisplayListBuilder::new();
    b.view3d(Rect::new(0.0, 0.0, 40.0, 30.0), red(key));
    tree.set_content(n, Some(b.finish_arc()));
    n
}

fn render(r: &mut VelloCpuReference, tree: &mut RenderTree, frame: u64) -> ljos_render::RgbaImage {
    let snap = tree.compose(Rect::new(0.0, 0.0, 100.0, 100.0), frame);
    r.render(&FrameView::new(&snap, 1.0)).unwrap()
}

#[test]
fn r_draws_a_view_where_it_is() {
    if !has_adapter() {
        return;
    }
    let mut tree = RenderTree::new();
    add_view(&mut tree, 1);
    let mut r = VelloCpuReference::default();
    let img = render(&mut r, &mut tree, 1);
    assert_eq!(img.pixel(30, 35), [255, 0, 0, 255]);
    assert_eq!(img.pixel(5, 5)[3], 0, "outside the view nothing is drawn");
    assert_eq!(img.pixel(55, 35)[3], 0);
}

#[test]
fn a_view_made_and_gone_again_and_again_keeps_one_picture_at_most() {
    if !has_adapter() {
        return;
    }
    let mut tree = RenderTree::new();
    let mut r = VelloCpuReference::default();
    for key in 0..32 {
        let n = add_view(&mut tree, key);
        render(&mut r, &mut tree, 2 * key + 1);
        assert_eq!(r.view3d_pictures(), 1, "view {key} is drawn");
        tree.remove(n).unwrap();
        render(&mut r, &mut tree, 2 * key + 2);
        assert_eq!(r.view3d_pictures(), 0, "view {key} is gone");
    }
}

#[cfg(feature = "b0")]
mod b0 {
    use egui::epaint::Shape;
    use egui::{Context, Pos2};
    use ljos_render::RenderBackend;
    use ljos_render::b0::{EguiSink, EpaintBackend};
    use ljos_render::scene3d::View3dPaint;

    use super::*;

    fn encode(
        be: &mut EpaintBackend,
        tree: &mut RenderTree,
        frame: u64,
        scenes3d: bool,
    ) -> Vec<Shape> {
        let ctx = Context::default();
        let snap = tree.compose(Rect::new(0.0, 0.0, 100.0, 100.0), frame);
        let view = FrameView::new(&snap, 2.0);
        be.sync(&view);
        let mut sink = EguiSink::new(Some(&ctx), Pos2::ZERO);
        sink.scenes3d = scenes3d;
        be.encode(&view, &mut sink);
        sink.shapes.into_iter().map(|s| s.shape).collect()
    }

    #[test]
    fn a_painter_that_draws_3d_gets_the_scene_and_its_size_in_pixels() {
        let mut tree = RenderTree::new();
        add_view(&mut tree, 7);
        let mut be = EpaintBackend::new();
        let shapes = encode(&mut be, &mut tree, 1, true);
        let [Shape::Callback(c)] = shapes.as_slice() else {
            panic!("{shapes:?}");
        };
        assert_eq!(
            c.rect,
            egui::Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(40.0, 30.0))
        );
        let paint = c.callback.downcast_ref::<View3dPaint>().unwrap();
        assert_eq!((paint.key, paint.size_px), (7, [80, 60]));
        assert_eq!(be.view3d_pictures(), 0, "nothing is read back");
    }

    #[test]
    fn a_painter_without_3d_gets_a_picture_that_goes_with_the_view() {
        if !has_adapter() {
            return;
        }
        let mut tree = RenderTree::new();
        let n = add_view(&mut tree, 7);
        let mut be = EpaintBackend::new();
        let shapes = encode(&mut be, &mut tree, 1, false);
        let [Shape::Mesh(m)] = shapes.as_slice() else {
            panic!("{shapes:?}");
        };
        assert_ne!(m.texture_id, egui::TextureId::default());
        assert_eq!(be.view3d_pictures(), 1);
        let uploads = be.texture_uploads();
        encode(&mut be, &mut tree, 2, false);
        assert_eq!(
            be.texture_uploads(),
            uploads,
            "an unchanged view is not drawn again"
        );
        tree.remove(n).unwrap();
        encode(&mut be, &mut tree, 3, false);
        assert_eq!(be.view3d_pictures(), 0);
    }
}
