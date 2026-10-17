//! B0 (epaint) backend: shape structure, cache minimality, gradients, opacity,
//! clip approximation warnings and the text stop-gap. B0 hands shapes to egui,
//! so these tests inspect the shapes instead of pixels (R covers pixels).
#![cfg(feature = "b0")]

use std::sync::Arc;
use std::time::Instant;

use egui::epaint::{Mesh, Shape};
use egui::{Context, Pos2};
use ljos_render::b0::{EguiSink, EpaintBackend};
use ljos_render::kurbo::{Affine, BezPath, Point, Rect, Size};
use ljos_render::{
    ClipSpec, DisplayListBuilder, DrawOp, FrameView, GradientStop, LinearGradient, MemoryPressure,
    NodeId, Paint, PathStyle, RenderBackend, RenderTree, Rgba, TextAlign,
};

const RED: Rgba = Rgba::from_premul(1.0, 0.0, 0.0, 1.0);
const BLUE: Rgba = Rgba::from_premul(0.0, 0.0, 1.0, 1.0);

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

    fn node(&mut self, parent: Option<NodeId>, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        let p = parent.unwrap_or(self.tree.root());
        let n = self.tree.add_child(p);
        self.tree.set_pos(n, x, y);
        self.tree.set_size(n, Size::new(w, h));
        n
    }

    fn fill(&mut self, id: NodeId, color: Rgba) {
        let size = self.tree.node(id).unwrap().size();
        let mut b = DisplayListBuilder::new();
        b.fill_rect(Rect::from_origin_size((0.0, 0.0), size), color);
        self.tree.set_content(id, Some(b.finish_arc()));
    }
}

fn run(
    s: &mut Scene,
    be: &mut EpaintBackend,
    ctx: Option<&Context>,
    origin: Pos2,
) -> (Vec<egui::epaint::ClippedShape>, ljos_render::EncodeStats) {
    s.frame += 1;
    let snap = s.tree.compose(s.vp, s.frame);
    let view = FrameView::new(&snap, 1.0);
    be.sync(&view);
    let mut sink = EguiSink::new(ctx, origin);
    let stats = be.encode(&view, &mut sink);
    (sink.shapes, stats)
}

fn mesh(s: &egui::epaint::ClippedShape) -> &Arc<Mesh> {
    match &s.shape {
        Shape::Mesh(m) => m,
        other => panic!("expected a mesh, got {other:?}"),
    }
}

/// Vertices that are fully covered (not the anti-aliasing feather).
fn opaque(m: &Mesh) -> Vec<&egui::epaint::Vertex> {
    m.vertices.iter().filter(|v| v.color.a() > 200).collect()
}

#[test]
fn one_clipped_mesh_per_painted_node_in_paint_order() {
    let mut s = Scene::new(100.0, 50.0);
    let a = s.node(None, 0.0, 0.0, 40.0, 40.0);
    let b = s.node(None, 20.0, 10.0, 40.0, 30.0);
    s.fill(a, RED);
    s.fill(b, BLUE);
    let mut be = EpaintBackend::new();
    let (shapes, stats) = run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!(shapes.len(), 2);
    assert_eq!((stats.items, stats.shapes), (2, 2));
    assert_eq!(stats.warnings, 0);
    // Back to front: red first.
    let first = mesh(&shapes[0]);
    let last = mesh(&shapes[1]);
    assert!(
        opaque(first)
            .iter()
            .all(|v| v.color.r() > 200 && v.color.b() < 50)
    );
    assert!(
        opaque(last)
            .iter()
            .all(|v| v.color.b() > 200 && v.color.r() < 50)
    );
    // No clip: the viewport is the clip rect.
    assert_eq!(shapes[0].clip_rect.min, Pos2::ZERO);
    assert_eq!(shapes[0].clip_rect.max, Pos2::new(100.0, 50.0));
    // Geometry sits where the node is.
    let (minx, maxx) = opaque(last)
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), v| {
            (lo.min(v.pos.x), hi.max(v.pos.x))
        });
    assert!(
        (minx - 20.0).abs() < 1.0 && (maxx - 60.0).abs() < 1.0,
        "{minx} {maxx}"
    );
}

#[test]
fn clip_rect_follows_the_clip_chain_and_viewport_origin() {
    let mut s = Scene::new(100.0, 100.0);
    let parent = s.node(None, 10.0, 10.0, 30.0, 30.0);
    s.tree
        .set_clip(parent, Some(ClipSpec::bounds(Size::new(30.0, 30.0))));
    let child = s.node(Some(parent), 5.0, 5.0, 60.0, 60.0);
    s.fill(child, RED);
    let mut be = EpaintBackend::new();
    let (shapes, stats) = run(&mut s, &mut be, None, Pos2::new(200.0, 300.0));
    assert_eq!(shapes.len(), 1);
    assert_eq!(stats.warnings, 0, "axis-aligned square clips are exact");
    let c = shapes[0].clip_rect;
    assert_eq!(
        (c.min, c.max),
        (Pos2::new(210.0, 310.0), Pos2::new(240.0, 340.0))
    );
    let (minx, _) = opaque(mesh(&shapes[0]))
        .iter()
        .fold((f32::MAX, 0.0f32), |(lo, hi), v| {
            (lo.min(v.pos.x), hi.max(v.pos.x))
        });
    assert!((minx - 215.0).abs() < 1.0, "origin shift applied: {minx}");
}

#[test]
fn rotated_and_rounded_clips_warn_but_still_draw() {
    let mut s = Scene::new(100.0, 100.0);
    let rounded = s.node(None, 0.0, 0.0, 40.0, 40.0);
    s.tree.set_clip(
        rounded,
        Some(ClipSpec::rounded(Rect::new(0.0, 0.0, 40.0, 40.0), 8.0)),
    );
    let c1 = s.node(Some(rounded), 0.0, 0.0, 20.0, 20.0);
    s.fill(c1, RED);
    let rotated = s.node(None, 60.0, 20.0, 30.0, 30.0);
    s.tree.set_local(
        rotated,
        Affine::translate((60.0, 20.0)) * Affine::rotate(std::f64::consts::FRAC_PI_4),
    );
    s.tree
        .set_clip(rotated, Some(ClipSpec::bounds(Size::new(30.0, 30.0))));
    let c2 = s.node(Some(rotated), 0.0, 0.0, 20.0, 20.0);
    s.fill(c2, BLUE);
    let mut be = EpaintBackend::new();
    let (shapes, stats) = run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!(shapes.len(), 2);
    assert_eq!(stats.warnings, 2, "one per approximated clip");
}

#[test]
fn transform_change_remaps_vertices_without_tessellating_again() {
    let mut s = Scene::new(100.0, 100.0);
    let n = s.node(None, 0.0, 0.0, 20.0, 20.0);
    s.fill(n, RED);
    let mut be = EpaintBackend::new();

    let (a, st) = run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!((be.tessellations(), be.world_rebuilds()), (1, 1));
    assert_eq!(st.caches_built, 1);

    // Nothing changed: the very same mesh is handed out again.
    let (b, st) = run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!((be.tessellations(), be.world_rebuilds()), (1, 1));
    assert_eq!((st.caches_built, st.caches_reused), (0, 1));
    assert!(Arc::ptr_eq(mesh(&a[0]), mesh(&b[0])));

    // Moving the node re-maps vertices only.
    s.tree.set_pos(n, 30.0, 0.0);
    let (c, _) = run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!((be.tessellations(), be.world_rebuilds()), (1, 2));
    assert!(!Arc::ptr_eq(mesh(&b[0]), mesh(&c[0])));
    let minx = opaque(mesh(&c[0]))
        .iter()
        .map(|v| v.pos.x)
        .fold(f32::MAX, f32::min);
    assert!((minx - 30.0).abs() < 1.0);

    // Opacity is a world-side change as well.
    s.tree.set_opacity(n, 0.5);
    run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!((be.tessellations(), be.world_rebuilds()), (1, 3));

    // Only new content tessellates again.
    s.fill(n, BLUE);
    run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!(be.tessellations(), 2);
}

#[test]
fn opacity_scales_vertex_colours() {
    let mut s = Scene::new(40.0, 40.0);
    let n = s.node(None, 0.0, 0.0, 20.0, 20.0);
    s.fill(n, RED);
    s.tree.set_opacity(n, 0.5);
    let kid = s.node(Some(n), 0.0, 0.0, 10.0, 10.0);
    s.fill(kid, RED);
    s.tree.set_opacity(kid, 0.5);
    let mut be = EpaintBackend::new();
    let (shapes, _) = run(&mut s, &mut be, None, Pos2::ZERO);
    let parent_alpha = opaque_or_max(mesh(&shapes[0]));
    let child_alpha = opaque_or_max(mesh(&shapes[1]));
    assert!((i32::from(parent_alpha) - 128).abs() <= 2, "{parent_alpha}");
    assert!(
        (i32::from(child_alpha) - 64).abs() <= 2,
        "multiplied down the tree: {child_alpha}"
    );
}

/// One egui pass; fonts exist only inside it. Its texture delta is discarded
/// because nothing renders it here.
fn pass(ctx: &Context, f: impl FnMut(&mut egui::Ui)) {
    ctx.run_ui(Default::default(), f).textures_delta.clear();
}

fn opaque_or_max(m: &Mesh) -> u8 {
    m.vertices.iter().map(|v| v.color.a()).max().unwrap()
}

#[test]
fn zero_opacity_and_hidden_nodes_emit_nothing() {
    let mut s = Scene::new(40.0, 40.0);
    let a = s.node(None, 0.0, 0.0, 10.0, 10.0);
    let b = s.node(None, 10.0, 0.0, 10.0, 10.0);
    s.fill(a, RED);
    s.fill(b, RED);
    s.tree.set_opacity(a, 0.0);
    s.tree.set_visible(b, false);
    let mut be = EpaintBackend::new();
    let (shapes, _) = run(&mut s, &mut be, None, Pos2::ZERO);
    assert!(shapes.is_empty());
}

#[test]
fn gradient_is_carried_by_vertex_colours() {
    let mut s = Scene::new(32.0, 8.0);
    let n = s.node(None, 0.0, 0.0, 32.0, 8.0);
    let g = LinearGradient::new(
        Point::new(0.0, 0.0),
        Point::new(32.0, 0.0),
        vec![
            GradientStop {
                offset: 0.0,
                color: Rgba::BLACK,
            },
            GradientStop {
                offset: 0.5,
                color: RED,
            },
            GradientStop {
                offset: 1.0,
                color: Rgba::WHITE,
            },
        ],
    );
    let mut b = DisplayListBuilder::new();
    b.rounded_rect(
        Rect::new(0.0, 0.0, 32.0, 8.0),
        [0.0; 4],
        Paint::LinearGradient(Arc::new(g)),
        None,
    );
    s.tree.set_content(n, Some(b.finish_arc()));
    let mut be = EpaintBackend::new();
    let (shapes, _) = run(&mut s, &mut be, None, Pos2::ZERO);
    let m = mesh(&shapes[0]);
    let solid = opaque(m);
    assert!(!solid.is_empty());
    // A vertex exactly at the middle stop must be pure red, which a two-colour
    // interpolation across the whole rect could never produce.
    let mid = solid
        .iter()
        .find(|v| (v.pos.x - 16.0).abs() < 0.01)
        .expect("a vertex is cut at the middle stop");
    assert_eq!((mid.color.r(), mid.color.g(), mid.color.b()), (255, 0, 0));
    let left = solid
        .iter()
        .min_by(|a, b| a.pos.x.total_cmp(&b.pos.x))
        .unwrap();
    let right = solid
        .iter()
        .max_by(|a, b| a.pos.x.total_cmp(&b.pos.x))
        .unwrap();
    assert!(
        left.color.r() < 12 && left.color.g() < 12,
        "{:?}",
        left.color
    );
    assert!(
        right.color.g() > 240 && right.color.b() > 240,
        "{:?}",
        right.color
    );
    // Colour is a function of x alone for a horizontal gradient.
    for v in &solid {
        let t = (v.pos.x / 32.0).clamp(0.0, 1.0);
        let want_r = if t < 0.5 { t * 2.0 * 255.0 } else { 255.0 };
        assert!(
            (f32::from(v.color.r()) - want_r).abs() <= 3.0,
            "x={} r={}",
            v.pos.x,
            v.color.r()
        );
    }
}

#[test]
fn borders_paths_and_shadows_produce_meshes() {
    let mut s = Scene::new(80.0, 80.0);
    let n = s.node(None, 0.0, 0.0, 80.0, 80.0);
    let mut b = DisplayListBuilder::new();
    b.shadow(
        Rect::new(10.0, 10.0, 40.0, 40.0),
        [4.0; 4],
        8.0,
        0.0,
        Rgba::from_straight(0.0, 0.0, 0.0, 0.5),
    );
    b.rounded_rect(
        Rect::new(10.0, 10.0, 40.0, 40.0),
        [4.0; 4],
        Paint::Solid(RED),
        Some(ljos_render::Border {
            width: 2.0,
            paint: Paint::Solid(BLUE),
        }),
    );
    let mut p = BezPath::new();
    p.move_to((50.0, 50.0));
    p.line_to((70.0, 50.0));
    p.line_to((60.0, 70.0));
    p.close_path();
    b.path(p.clone(), PathStyle::Fill, Paint::Solid(RED));
    b.path(p, PathStyle::Stroke { width: 2.0 }, Paint::Solid(BLUE));
    s.tree.set_content(n, Some(b.finish_arc()));
    let mut be = EpaintBackend::new();
    let (shapes, stats) = run(&mut s, &mut be, None, Pos2::ZERO);
    let shadow_only = {
        let mut s2 = Scene::new(80.0, 80.0);
        let n2 = s2.node(None, 0.0, 0.0, 80.0, 80.0);
        let mut b2 = DisplayListBuilder::new();
        b2.shadow(
            Rect::new(20.0, 20.0, 60.0, 60.0),
            [4.0; 4],
            8.0,
            0.0,
            Rgba::from_straight(0.0, 0.0, 0.0, 0.5),
        );
        s2.tree.set_content(n2, Some(b2.finish_arc()));
        run(&mut s2, &mut EpaintBackend::new(), None, Pos2::ZERO).0
    };
    let sm = mesh(&shadow_only[0]);
    let core = sm.vertices.iter().map(|v| v.color.a()).max().unwrap();
    assert!(core > 20 && core <= 128, "soft shadow alpha: {core}");
    assert!(
        sm.vertices
            .iter()
            .all(|v| v.color.r() == 0 && v.color.b() == 0)
    );
    // One merged mesh for the whole list (no text splits it).
    assert_eq!(shapes.len(), 1);
    let m = mesh(&shapes[0]);
    assert!(m.indices.len().is_multiple_of(3) && !m.indices.is_empty());
    assert!(m.indices.iter().all(|i| (*i as usize) < m.vertices.len()));
    let has = |pred: fn(egui::Color32) -> bool| m.vertices.iter().any(|v| pred(v.color));
    assert!(
        has(|c| c.r() > 200 && c.b() < 60 && c.a() > 200),
        "red fill"
    );
    assert!(
        has(|c| c.b() > 200 && c.r() < 60 && c.a() > 200),
        "blue border/stroke"
    );
    assert_eq!(stats.warnings, 1, "convex-only path fill is reported");
}

#[test]
fn unsupported_ops_are_counted_not_fatal() {
    let mut s = Scene::new(20.0, 20.0);
    let n = s.node(None, 0.0, 0.0, 20.0, 20.0);
    let mut b = DisplayListBuilder::new();
    b.embed(ljos_render::EmbedId(1), Rect::new(0.0, 0.0, 10.0, 10.0));
    b.fill_rect(Rect::new(0.0, 0.0, 5.0, 5.0), RED);
    s.tree.set_content(n, Some(b.finish_arc()));
    let mut be = EpaintBackend::new();
    let (shapes, stats) = run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!(shapes.len(), 1);
    assert_eq!(stats.warnings, 1);
}

#[test]
fn text_stopgap_needs_a_context_and_lays_out_with_one() {
    let mut s = Scene::new(200.0, 40.0);
    let n = s.node(None, 10.0, 5.0, 180.0, 30.0);
    let mut b = DisplayListBuilder::new();
    b.fill_rect(Rect::new(0.0, 0.0, 180.0, 30.0), RED);
    b.text_stopgap(
        Rect::new(0.0, 0.0, 180.0, 30.0),
        "hello",
        14.0,
        Rgba::BLACK,
        TextAlign::Left,
    );
    b.fill_rect(Rect::new(0.0, 0.0, 5.0, 5.0), BLUE);
    s.tree.set_content(n, Some(b.finish_arc()));
    let mut be = EpaintBackend::new();

    // Without a Context the text is skipped with a warning; the meshes around it stay.
    let (shapes, stats) = run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!(shapes.len(), 2);
    assert_eq!(stats.warnings, 1);

    // With one, the text sits between the two meshes in paint order.
    let ctx = Context::default();
    let mut out = None;
    pass(&ctx, |_ui| {
        out = Some(run(&mut s, &mut be, Some(&ctx), Pos2::ZERO));
    });
    let (shapes, stats) = out.unwrap();
    assert_eq!(stats.warnings, 0);
    assert_eq!(shapes.len(), 3);
    assert!(matches!(shapes[0].shape, Shape::Mesh(_)));
    let Shape::Text(t) = &shapes[1].shape else {
        panic!("middle shape is text: {:?}", shapes[1].shape)
    };
    assert!(
        (t.pos.x - 10.0).abs() < 0.01 && (t.pos.y - 5.0).abs() < 0.01,
        "{:?}",
        t.pos
    );
    assert_eq!(t.galley.text(), "hello");
    assert!(matches!(shapes[2].shape, Shape::Mesh(_)));
}

#[test]
fn scale_factor_change_drops_caches_and_trim_empties_them() {
    let mut s = Scene::new(20.0, 20.0);
    let n = s.node(None, 0.0, 0.0, 20.0, 20.0);
    s.fill(n, RED);
    let mut be = EpaintBackend::new();
    run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!(be.tessellations(), 1);

    let snap = s.tree.compose(s.vp, 99);
    let view = FrameView::new(&snap, 2.0);
    be.sync(&view);
    let mut sink = EguiSink::new(None, Pos2::ZERO);
    be.encode(&view, &mut sink);
    assert_eq!(
        be.tessellations(),
        2,
        "feathering width depends on pixels per point"
    );

    be.trim(MemoryPressure::Critical);
    be.encode(&view, &mut sink);
    assert_eq!(
        be.tessellations(),
        3,
        "usable after trim, rebuilt on demand"
    );
    assert!(
        be.snapshot_rgba(&view).is_none(),
        "B0 has no pixels of its own"
    );
}

#[test]
fn a_list_no_node_draws_any_more_is_let_go_at_once() {
    // An idle window draws a frame now and then, so a cache that ages by
    // frames would hold every list a repaint replaced for a long time.
    let mut s = Scene::new(40.0, 20.0);
    let a = s.node(None, 0.0, 0.0, 20.0, 20.0);
    let b = s.node(None, 20.0, 0.0, 20.0, 20.0);
    let mut be = EpaintBackend::new();
    for i in 0..10 {
        s.fill(a, if i % 2 == 0 { RED } else { BLUE });
        run(&mut s, &mut be, None, Pos2::ZERO);
        assert_eq!(be.display_lists(), 1);
    }

    // A list two nodes draw stays while either does.
    let mut list = DisplayListBuilder::new();
    list.fill_rect(Rect::new(0.0, 0.0, 20.0, 20.0), RED);
    let shared = list.finish_arc();
    s.tree.set_content(a, Some(shared.clone()));
    s.tree.set_content(b, Some(shared));
    run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!(be.display_lists(), 1);
    let built = be.tessellations();
    s.fill(a, BLUE);
    run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!(be.display_lists(), 2);
    s.tree.set_pos(b, 21.0, 0.0);
    run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!(be.tessellations(), built + 1, "only the new list is built");
    s.fill(b, RED);
    run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!(be.display_lists(), 2);
}

/// Release-mode numbers for the report:
/// `cargo test -p ljos_render --release --test b0 -- --ignored --nocapture`.
#[test]
#[ignore = "timing only"]
fn perf_300_text_nodes_and_1000_rects() {
    let mut s = Scene::new(1280.0, 800.0);
    let mut rects = Vec::new();
    for i in 0..1000 {
        let (x, y) = ((i % 40) as f64 * 32.0, (i / 40) as f64 * 30.0);
        let n = s.node(None, x, y, 30.0, 28.0);
        let mut b = DisplayListBuilder::new();
        b.rounded_rect(
            Rect::new(0.0, 0.0, 30.0, 28.0),
            [4.0; 4],
            Paint::Solid(Rgba::from_straight(0.2, 0.4, 0.8, 1.0)),
            Some(ljos_render::Border {
                width: 1.0,
                paint: Paint::Solid(Rgba::BLACK),
            }),
        );
        s.tree.set_content(n, Some(b.finish_arc()));
        rects.push(n);
    }
    let mut texts = Vec::new();
    for i in 0..300 {
        let (x, y) = ((i % 10) as f64 * 120.0, (i / 10) as f64 * 26.0);
        let n = s.node(None, x, y, 118.0, 24.0);
        let mut b = DisplayListBuilder::new();
        b.fill_rect(
            Rect::new(0.0, 0.0, 118.0, 24.0),
            Rgba::from_straight(0.9, 0.9, 0.9, 1.0),
        );
        b.text_stopgap(
            Rect::new(0.0, 0.0, 118.0, 24.0),
            format!("label number {i}"),
            13.0,
            Rgba::BLACK,
            TextAlign::Left,
        );
        s.tree.set_content(n, Some(b.finish_arc()));
        texts.push(n);
    }

    let t = Instant::now();
    let first = s.tree.compose(s.vp, 1);
    let compose_cold = t.elapsed();
    let t = Instant::now();
    s.tree.set_pos(rects[0], 1.0, 1.0);
    let second = s.tree.compose(s.vp, 2);
    let compose_one_move = t.elapsed();
    let t = Instant::now();
    let third = s.tree.compose(s.vp, 3);
    let compose_idle = t.elapsed();
    assert!(Arc::ptr_eq(&second, &third));
    println!(
        "compose: cold {compose_cold:?}, one node moved {compose_one_move:?}, unchanged {compose_idle:?} ({} items)",
        first.items.len()
    );

    let ctx = Context::default();
    let mut be = EpaintBackend::new();
    let mut cold = None;
    let mut warm = None;
    let mut moved = None;
    let mut done = 0;
    // Each run_ui is one egui pass; fonts become available inside it. egui's
    // text caches settle over a few passes, so "warm" is the best of the
    // later passes rather than the second one.
    for _ in 0..8 {
        pass(&ctx, |_ui| {
            let snap = &second;
            let view = FrameView::new(snap, 1.0);
            be.sync(&view);
            let mut sink = EguiSink::new(Some(&ctx), Pos2::ZERO);
            let t = Instant::now();
            let stats = be.encode(&view, &mut sink);
            let d = t.elapsed();
            match done {
                0 => cold = Some((d, stats.shapes)),
                1.. if warm.is_none_or(|(w, _)| d < w) => warm = Some((d, stats.shapes)),
                _ => {}
            }
            done += 1;
        });
    }
    s.tree.set_pos(rects[1], 2.0, 2.0);
    let snap = s.tree.compose(s.vp, 4);
    pass(&ctx, |_ui| {
        let view = FrameView::new(&snap, 1.0);
        let mut sink = EguiSink::new(Some(&ctx), Pos2::ZERO);
        let t = Instant::now();
        be.encode(&view, &mut sink);
        moved = Some(t.elapsed());
    });
    let (cold, cold_shapes) = cold.unwrap();
    let (warm, warm_shapes) = warm.unwrap();
    println!(
        "B0 encode: cold {cold:?} ({cold_shapes} shapes), warm {warm:?} ({warm_shapes} shapes), one node moved {:?}",
        moved.unwrap()
    );
}

fn run_folding(s: &mut Scene, be: &mut EpaintBackend) -> (usize, Option<egui::Color32>) {
    s.frame += 1;
    let snap = s.tree.compose(s.vp, s.frame);
    let view = FrameView::new(&snap, 1.0);
    be.sync(&view);
    let mut sink = EguiSink::new(None, Pos2::ZERO);
    sink.fold_backdrop = true;
    be.encode(&view, &mut sink);
    (sink.shapes.len(), sink.backdrop)
}

#[test]
fn an_opaque_rectangle_under_everything_becomes_the_backdrop() {
    let mut s = Scene::new(100.0, 50.0);
    let back = s.node(None, 0.0, 0.0, 100.0, 50.0);
    let front = s.node(None, 10.0, 10.0, 20.0, 20.0);
    s.fill(back, BLUE);
    s.fill(front, RED);
    let mut be = EpaintBackend::new();
    assert_eq!(
        run_folding(&mut s, &mut be),
        (1, Some(egui::Color32::BLUE)),
        "the blue rectangle is the clear colour, the red one a shape"
    );
    // Without the host's say-so it stays a shape.
    let (shapes, _) = run(&mut s, &mut be, None, Pos2::ZERO);
    assert_eq!(shapes.len(), 2);
}

#[test]
fn only_what_covers_the_view_opaquely_and_first_is_folded() {
    let half = Rgba::from_premul(0.0, 0.0, 0.5, 0.5);
    for (x, w, color, under) in [
        (10.0, 90.0, BLUE, false), // leaves a strip uncovered
        (0.0, 100.0, half, false), // lets through what is behind
        (0.0, 100.0, BLUE, true),  // drawn over something
    ] {
        let mut s = Scene::new(100.0, 50.0);
        if under {
            let first = s.node(None, 0.0, 0.0, 5.0, 5.0);
            s.fill(first, RED);
        }
        let back = s.node(None, x, 0.0, w, 50.0);
        s.fill(back, color);
        let mut be = EpaintBackend::new();
        let (shapes, backdrop) = run_folding(&mut s, &mut be);
        assert_eq!(backdrop, None, "x {x} w {w} under {under}");
        assert_eq!(shapes, if under { 2 } else { 1 });
    }
}

fn run_solid(s: &mut Scene, be: &mut EpaintBackend) -> Vec<egui::epaint::ClippedShape> {
    s.frame += 1;
    let snap = s.tree.compose(s.vp, s.frame);
    let view = FrameView::new(&snap, 1.0);
    be.sync(&view);
    let mut sink = EguiSink::new(None, Pos2::ZERO);
    sink.solid_rects = true;
    be.encode(&view, &mut sink);
    sink.shapes
}

fn rect_of(list: &mut DisplayListBuilder, x: f64, w: f64, radius: f32, color: Rgba, aa: bool) {
    list.push(DrawOp::Rect {
        rect: Rect::new(x, 0.0, x + w, 10.0),
        radii: [radius; 4],
        fill: Paint::Solid(color),
        border: None,
        aa,
    });
}

/// One node holding one rectangle, encoded with `solid_rects` on: the
/// rectangle it was handed over as, if it was not a mesh.
fn solid_of(x: f64, w: f64, radius: f32, color: Rgba, aa: bool) -> Option<egui::Rect> {
    let mut s = Scene::new(100.0, 50.0);
    let n = s.node(None, 0.0, 0.0, 100.0, 50.0);
    let mut b = DisplayListBuilder::new();
    rect_of(&mut b, x, w, radius, color, aa);
    s.tree.set_content(n, Some(b.finish_arc()));
    let mut be = EpaintBackend::new();
    let shapes = run_solid(&mut s, &mut be);
    assert_eq!(shapes.len(), 1);
    match &shapes[0].shape {
        Shape::Rect(r) => Some(r.rect),
        Shape::Mesh(_) => None,
        other => panic!("{other:?}"),
    }
}

#[test]
fn opaque_square_rectangles_on_whole_pixels_are_handed_over_as_rectangles() {
    assert_eq!(
        solid_of(10.0, 20.0, 0.0, RED, true),
        Some(egui::Rect::from_min_max(
            Pos2::new(10.0, 0.0),
            Pos2::new(30.0, 10.0)
        ))
    );
    // Without the host's say-so it stays a mesh.
    let mut s = Scene::new(100.0, 50.0);
    let n = s.node(None, 10.0, 0.0, 20.0, 10.0);
    s.fill(n, RED);
    let (shapes, _) = run(&mut s, &mut EpaintBackend::new(), None, Pos2::ZERO);
    assert!(matches!(shapes[0].shape, Shape::Mesh(_)));
}

#[test]
fn a_hard_edged_rectangle_between_pixels_covers_the_pixels_its_centres_hold() {
    // 10.4..30.4 holds the centres of pixels 10 to 29.
    assert_eq!(
        solid_of(10.4, 20.0, 0.0, RED, false),
        Some(egui::Rect::from_min_max(
            Pos2::new(10.0, 0.0),
            Pos2::new(30.0, 10.0)
        ))
    );
    // 10.6.. starts with pixel 11.
    assert_eq!(
        solid_of(10.6, 20.0, 0.0, RED, false).map(|r| r.min.x),
        Some(11.0)
    );
}

#[test]
fn what_a_rectangle_cannot_draw_stays_a_mesh() {
    let half = Rgba::from_premul(0.5, 0.0, 0.0, 0.5);
    assert_eq!(solid_of(10.4, 20.0, 0.0, RED, true), None, "feathered edge");
    assert_eq!(solid_of(10.0, 20.0, 3.0, RED, false), None, "rounded");
    assert_eq!(solid_of(10.0, 20.0, 0.0, half, false), None, "translucent");
    assert_eq!(
        solid_of(10.2, 0.2, 0.0, RED, false),
        None,
        "covers no pixel"
    );
}

#[test]
fn rectangles_handed_over_keep_their_place_in_paint_order() {
    let mut s = Scene::new(100.0, 50.0);
    let a = s.node(None, 0.0, 0.0, 10.0, 10.0);
    let b = s.node(None, 0.0, 0.0, 20.0, 20.0);
    let c = s.node(None, 0.0, 0.0, 10.0, 10.0);
    s.fill(a, RED);
    let mut list = DisplayListBuilder::new();
    list.rounded_rect(
        Rect::new(0.0, 0.0, 20.0, 20.0),
        [4.0; 4],
        Paint::Solid(BLUE),
        None,
    );
    s.tree.set_content(b, Some(list.finish_arc()));
    s.fill(c, BLUE);
    let shapes = run_solid(&mut s, &mut EpaintBackend::new());
    let kinds: Vec<_> = shapes
        .iter()
        .map(|c| matches!(c.shape, Shape::Rect(_)))
        .collect();
    assert_eq!(kinds, [true, false, true]);
}
