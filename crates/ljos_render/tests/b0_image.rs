//! Images on B0: textured quads, one texture upload per image and sampling
//! mode, and textures that go when their image does. B0 hands shapes to
//! egui, so these tests read the meshes (R covers the pixels, and
//! `ljos_egui` renders one image through egui_kittest).
#![cfg(feature = "b0")]

use egui::epaint::{ClippedShape, Mesh, Shape};
use egui::{Context, Pos2, TextureId};
use ljos_render::b0::{EguiSink, EpaintBackend};
use ljos_render::kurbo::{Rect, Size};
use ljos_render::{
    DisplayListBuilder, DrawOp, EncodeStats, FrameView, ImageHandle, Insets, MemoryPressure,
    NodeId, RenderBackend, RenderTree, Rgba, TileMode,
};

struct Scene {
    tree: RenderTree,
    vp: Rect,
    frame: u64,
}

impl Scene {
    fn new(w: f64, h: f64) -> Self {
        let mut tree = RenderTree::new();
        let r = tree.root();
        tree.set_size(r, Size::new(w, h));
        Self {
            tree,
            vp: Rect::new(0.0, 0.0, w, h),
            frame: 0,
        }
    }

    fn node(&mut self, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        let n = self.tree.add_child(self.tree.root());
        self.tree.set_pos(n, x, y);
        self.tree.set_size(n, Size::new(w, h));
        n
    }

    fn op(&mut self, id: NodeId, op: DrawOp) {
        let mut b = DisplayListBuilder::new();
        b.push(op);
        self.tree.set_content(id, Some(b.finish_arc()));
    }

    fn run(&mut self, be: &mut EpaintBackend, ctx: Option<&Context>) -> (Vec<Mesh>, EncodeStats) {
        self.frame += 1;
        let snap = self.tree.compose(self.vp, self.frame);
        let view = FrameView::new(&snap, 1.0);
        be.sync(&view);
        let mut sink = EguiSink::new(ctx, Pos2::ZERO);
        let stats = be.encode(&view, &mut sink);
        let meshes = sink
            .shapes
            .iter()
            .map(|ClippedShape { shape, .. }| match shape {
                Shape::Mesh(m) => Mesh::clone(m),
                other => panic!("expected a mesh, got {other:?}"),
            })
            .collect();
        (meshes, stats)
    }
}

fn picture(w: u32, h: u32) -> ImageHandle {
    ImageHandle::filled(w, h, [200, 40, 40, 255]).unwrap()
}

fn whole(image: &ImageHandle, dst: Rect, smooth: bool) -> DrawOp {
    DrawOp::Image {
        image: image.clone(),
        src: image.rect(),
        dst,
        tile: None,
        mirror: [false; 2],
        smooth,
        opacity: 1.0,
        tint: None,
    }
}

/// The corners of what a mesh covers: positions, then texture coordinates.
fn bounds(m: &Mesh) -> ([f32; 4], [f32; 4]) {
    let fold = |get: fn(&egui::epaint::Vertex) -> Pos2| {
        m.vertices.iter().map(get).fold(
            [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
            |[x0, y0, x1, y1], p| [x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)],
        )
    };
    (fold(|v| v.pos), fold(|v| v.uv))
}

#[test]
fn an_image_is_one_textured_quad_where_its_node_is() {
    let ctx = Context::default();
    let image = picture(40, 20);
    let mut s = Scene::new(200.0, 100.0);
    let n = s.node(10.0, 20.0, 80.0, 40.0);
    s.op(n, whole(&image, Rect::new(0.0, 0.0, 80.0, 40.0), true));
    let mut be = EpaintBackend::new();
    let (meshes, stats) = s.run(&mut be, Some(&ctx));
    assert_eq!(meshes.len(), 1);
    assert_eq!(stats.warnings, 0);
    let m = &meshes[0];
    assert_eq!((m.vertices.len(), m.indices.len()), (4, 6));
    assert_ne!(m.texture_id, TextureId::default(), "its own texture");
    let (pos, uv) = bounds(m);
    assert_eq!(pos, [10.0, 20.0, 90.0, 60.0]);
    assert_eq!(uv, [0.0, 0.0, 1.0, 1.0]);
    // The texture tints nothing.
    assert!(m.vertices.iter().all(|v| v.color == egui::Color32::WHITE));
}

#[test]
fn many_pictures_of_one_image_in_a_node_are_one_mesh_each_as_faded_as_it_is() {
    let ctx = Context::default();
    let image = picture(8, 8);
    let mut s = Scene::new(200.0, 100.0);
    let n = s.node(0.0, 0.0, 200.0, 100.0);
    let mut b = DisplayListBuilder::new();
    for i in 0..50 {
        let x = f64::from(i) * 4.0;
        b.push(DrawOp::Image {
            image: image.clone(),
            src: image.rect(),
            dst: Rect::new(x, 0.0, x + 4.0, 4.0),
            tile: None,
            mirror: [false; 2],
            smooth: true,
            opacity: if i == 0 { 0.5 } else { 1.0 },
            tint: None,
        });
    }
    s.tree.set_content(n, Some(b.finish_arc()));
    let mut be = EpaintBackend::new();
    let (meshes, _) = s.run(&mut be, Some(&ctx));
    assert_eq!(meshes.len(), 1, "one mesh for fifty quads");
    let m = &meshes[0];
    assert_eq!((m.vertices.len(), m.indices.len()), (200, 300));
    let first = m.vertices[0].color;
    assert_eq!(first, egui::Color32::WHITE.gamma_multiply(0.5));
    assert!(
        m.vertices[4..]
            .iter()
            .all(|v| v.color == egui::Color32::WHITE)
    );
}

#[test]
fn pictures_of_one_image_in_many_tints_are_one_mesh_on_one_texture() {
    let ctx = Context::default();
    let image = picture(8, 8);
    let mut s = Scene::new(200.0, 100.0);
    let n = s.node(0.0, 0.0, 200.0, 100.0);
    let mut b = DisplayListBuilder::new();
    for i in 0..50u8 {
        let x = f64::from(i) * 4.0;
        b.push(DrawOp::Image {
            image: image.clone(),
            src: image.rect(),
            dst: Rect::new(x, 0.0, x + 4.0, 4.0),
            tile: None,
            mirror: [false; 2],
            smooth: true,
            opacity: if i == 0 { 0.5 } else { 1.0 },
            tint: Some(Rgba::rgb(i * 5, 255 - i * 5, 128)),
        });
    }
    s.tree.set_content(n, Some(b.finish_arc()));
    let mut be = EpaintBackend::new();
    let (meshes, _) = s.run(&mut be, Some(&ctx));
    assert_eq!(meshes.len(), 1, "one mesh for fifty colours");
    assert_eq!(
        (be.image_textures(), be.texture_uploads()),
        (1, 1),
        "one texture for fifty colours"
    );
    let m = &meshes[0];
    assert_eq!(
        m.vertices[0].color,
        egui::Color32::from_rgb(0, 255, 128).gamma_multiply(0.5)
    );
    assert_eq!(m.vertices[4].color, egui::Color32::from_rgb(5, 250, 128));
}

#[test]
fn a_part_of_the_picture_and_a_flip_are_texture_coordinates() {
    let ctx = Context::default();
    let image = picture(40, 20);
    let mut s = Scene::new(100.0, 100.0);
    let n = s.node(0.0, 0.0, 40.0, 20.0);
    s.op(
        n,
        DrawOp::Image {
            image: image.clone(),
            src: Rect::new(20.0, 10.0, 40.0, 20.0),
            dst: Rect::new(0.0, 0.0, 40.0, 20.0),
            tile: None,
            mirror: [true, false],
            smooth: false,
            opacity: 1.0,
            tint: None,
        },
    );
    let mut be = EpaintBackend::new();
    let (meshes, _) = s.run(&mut be, Some(&ctx));
    let m = &meshes[0];
    let (_, uv) = bounds(m);
    assert_eq!(uv, [0.5, 0.5, 1.0, 1.0]);
    // Flipped: the left edge of the quad reads the right edge of the part.
    let left = m
        .vertices
        .iter()
        .find(|v| v.pos.x == 0.0)
        .expect("a left vertex");
    assert_eq!(left.uv.x, 1.0);
}

#[test]
fn an_image_is_uploaded_once_however_often_it_is_drawn() {
    let ctx = Context::default();
    let image = picture(16, 16);
    let mut s = Scene::new(100.0, 100.0);
    let a = s.node(0.0, 0.0, 16.0, 16.0);
    let b = s.node(50.0, 0.0, 32.0, 32.0);
    s.op(a, whole(&image, Rect::new(0.0, 0.0, 16.0, 16.0), true));
    s.op(b, whole(&image, Rect::new(0.0, 0.0, 32.0, 32.0), true));
    let mut be = EpaintBackend::new();
    let (meshes, _) = s.run(&mut be, Some(&ctx));
    assert_eq!(meshes.len(), 2);
    assert_eq!(meshes[0].texture_id, meshes[1].texture_id);
    assert_eq!((be.image_textures(), be.texture_uploads()), (1, 1));
    for _ in 0..5 {
        s.run(&mut be, Some(&ctx));
    }
    assert_eq!(be.texture_uploads(), 1, "later frames reuse the texture");
}

#[test]
fn sampling_modes_have_their_own_textures() {
    let ctx = Context::default();
    let image = picture(8, 8);
    let mut s = Scene::new(100.0, 100.0);
    let hard = s.node(0.0, 0.0, 16.0, 16.0);
    let soft = s.node(20.0, 0.0, 16.0, 16.0);
    let tiled = s.node(40.0, 0.0, 40.0, 40.0);
    s.op(hard, whole(&image, Rect::new(0.0, 0.0, 16.0, 16.0), false));
    s.op(soft, whole(&image, Rect::new(0.0, 0.0, 16.0, 16.0), true));
    s.op(
        tiled,
        DrawOp::Image {
            image: image.clone(),
            src: image.rect(),
            dst: Rect::new(0.0, 0.0, 40.0, 40.0),
            tile: Some(Rect::new(0.0, 0.0, 8.0, 8.0)),
            mirror: [false; 2],
            smooth: true,
            opacity: 1.0,
            tint: None,
        },
    );
    let mut be = EpaintBackend::new();
    let (meshes, stats) = s.run(&mut be, Some(&ctx));
    assert_eq!(stats.warnings, 0);
    assert_eq!(meshes.len(), 3);
    assert_eq!(be.image_textures(), 3, "nearest, linear, repeating");
    // A whole picture that repeats both ways is one quad whose coordinates
    // run past the texture: five copies each way here.
    let (_, uv) = bounds(&meshes[2]);
    assert_eq!(meshes[2].vertices.len(), 4);
    assert_eq!(uv, [0.0, 0.0, 5.0, 5.0]);
}

#[test]
fn a_nine_patch_is_nine_quads_of_one_texture() {
    let ctx = Context::default();
    let image = picture(30, 30);
    let mut s = Scene::new(200.0, 100.0);
    let n = s.node(0.0, 0.0, 100.0, 60.0);
    s.op(
        n,
        DrawOp::NinePatch {
            image: image.clone(),
            insets: Insets {
                left: 5.0,
                top: 6.0,
                right: 7.0,
                bottom: 8.0,
            },
            dst: Rect::new(0.0, 0.0, 100.0, 60.0),
            tile: [TileMode::Stretch; 2],
            mirror: false,
            smooth: false,
        },
    );
    let mut be = EpaintBackend::new();
    let (meshes, stats) = s.run(&mut be, Some(&ctx));
    assert_eq!(stats.warnings, 0);
    assert_eq!(meshes.len(), 1);
    assert_eq!(meshes[0].vertices.len(), 9 * 4);
    let (pos, uv) = bounds(&meshes[0]);
    assert_eq!(pos, [0.0, 0.0, 100.0, 60.0]);
    assert_eq!(uv, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(be.texture_uploads(), 1);
}

#[test]
fn without_a_context_an_image_is_left_out_and_counted() {
    let image = picture(8, 8);
    let mut s = Scene::new(100.0, 100.0);
    let n = s.node(0.0, 0.0, 8.0, 8.0);
    s.op(n, whole(&image, Rect::new(0.0, 0.0, 8.0, 8.0), true));
    let mut be = EpaintBackend::new();
    let (meshes, stats) = s.run(&mut be, None);
    assert!(meshes.is_empty());
    assert_eq!(stats.warnings, 1);
    assert_eq!(be.image_textures(), 0);
}

#[test]
fn the_texture_goes_when_the_image_does() {
    let ctx = Context::default();
    let image = picture(8, 8);
    let mut s = Scene::new(100.0, 100.0);
    let n = s.node(0.0, 0.0, 8.0, 8.0);
    s.op(n, whole(&image, Rect::new(0.0, 0.0, 8.0, 8.0), true));
    let mut be = EpaintBackend::new();
    s.run(&mut be, Some(&ctx));
    assert_eq!(be.image_textures(), 1);
    // The backend's caches are not what keeps the pixels.
    assert_eq!(image.holders(), 2, "the test and the display list");

    s.tree.set_content(n, None);
    drop(image);
    let (meshes, _) = s.run(&mut be, Some(&ctx));
    assert!(meshes.is_empty());
    assert_eq!(be.image_textures(), 0);
}

#[test]
fn a_critical_trim_drops_textures_and_the_next_frame_uploads_again() {
    let ctx = Context::default();
    let image = picture(8, 8);
    let mut s = Scene::new(100.0, 100.0);
    let n = s.node(0.0, 0.0, 8.0, 8.0);
    s.op(n, whole(&image, Rect::new(0.0, 0.0, 8.0, 8.0), true));
    let mut be = EpaintBackend::new();
    s.run(&mut be, Some(&ctx));
    be.trim(MemoryPressure::Critical);
    assert_eq!(be.image_textures(), 0);
    let (meshes, _) = s.run(&mut be, Some(&ctx));
    assert_eq!(meshes.len(), 1);
    assert_eq!((be.image_textures(), be.texture_uploads()), (1, 2));
}

/// The host tells egui a small limit, which is also the width of egui's
/// text atlas, and keeps the GPU's limit for pictures.
#[test]
fn egui_is_told_a_small_limit_and_pictures_keep_the_gpus() {
    const GPU_SIDE: usize = 16384;
    const EGUI_SIDE: usize = 2048;
    let ctx = Context::default();
    let input = egui::RawInput {
        max_texture_side: Some(EGUI_SIDE),
        ..Default::default()
    };
    ctx.run_ui(input, |_| {}).textures_delta.clear();
    let [w, h] = ctx.fonts(|f| f.font_image_size());
    assert_eq!(w, EGUI_SIDE);
    // RGBA bytes of atlas not held, on the CPU and again on the GPU.
    let saved = (GPU_SIDE - w) * h * 4;
    assert!(
        saved >= (GPU_SIDE - EGUI_SIDE) * 32 * 4,
        "saved {saved} bytes"
    );

    ljos_render::b0::set_max_texture_side(&ctx, GPU_SIDE);
    let image = picture(3000, 10);
    let mut s = Scene::new(100.0, 100.0);
    let n = s.node(0.0, 0.0, 100.0, 10.0);
    s.op(n, whole(&image, Rect::new(0.0, 0.0, 100.0, 10.0), true));
    let mut be = EpaintBackend::new();
    let (meshes, _) = s.run(&mut be, Some(&ctx));
    let size = ctx
        .tex_manager()
        .read()
        .meta(meshes[0].texture_id)
        .expect("the picture's texture")
        .size;
    assert_eq!(size, [3000, 10], "not shrunk to egui's limit");
}
