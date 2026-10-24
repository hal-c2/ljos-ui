//! R backend: exact pixel checks and golden PNGs. R is deterministic, so
//! goldens are compared byte for byte. Set `LJOS_BLESS=1` to rewrite them.

use std::path::PathBuf;
use std::sync::Arc;

use ljos_render::kurbo::{Affine, BezPath, Point, Rect, Size};
use ljos_render::{
    Border, ClipSpec, DisplayListBuilder, FrameView, GradientStop, LinearGradient, NodeId, Paint,
    PathStyle, RasterConfig, RenderBackend, RenderTree, Rgba, RgbaImage, TextAlign, TextMode,
    VelloCpuReference,
};

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"))
}

fn assert_golden(name: &str, img: &RgbaImage) {
    let path = golden_path(name);
    if std::env::var_os("LJOS_BLESS").is_some() {
        std::fs::write(&path, img.to_png().unwrap()).unwrap();
    }
    let bytes = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {}: {e} (run with LJOS_BLESS=1)",
            path.display()
        )
    });
    let want = RgbaImage::from_png(&bytes).unwrap();
    assert_eq!(
        (img.width, img.height),
        (want.width, want.height),
        "{name}: size"
    );
    if img.data != want.data {
        let first = img
            .data
            .iter()
            .zip(&want.data)
            .position(|(a, b)| a != b)
            .unwrap();
        let px = first / 4;
        panic!(
            "{name}: pixel ({}, {}) differs: got {:?}, want {:?}",
            px as u32 % img.width,
            px as u32 / img.width,
            img.pixel(px as u32 % img.width, px as u32 / img.width),
            want.pixel(px as u32 % img.width, px as u32 / img.width),
        );
    }
}

const WHITE_BG: RasterConfig = RasterConfig {
    background: Rgba::WHITE,
    text: TextMode::Skip,
};

struct Scene {
    tree: RenderTree,
    vp: Rect,
}

impl Scene {
    fn new(w: f64, h: f64) -> Self {
        let mut tree = RenderTree::new();
        let r = tree.root();
        tree.set_size(r, Size::new(w, h));
        Self {
            tree,
            vp: Rect::new(0.0, 0.0, w, h),
        }
    }

    fn node(&mut self, parent: Option<NodeId>, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        let p = parent.unwrap_or(self.tree.root());
        let n = self.tree.add_child(p);
        self.tree.set_pos(n, x, y);
        self.tree.set_size(n, Size::new(w, h));
        n
    }

    fn paint(&mut self, id: NodeId, f: impl FnOnce(&mut DisplayListBuilder)) {
        let mut b = DisplayListBuilder::new();
        f(&mut b);
        self.tree.set_content(id, Some(b.finish_arc()));
    }

    fn fill(&mut self, id: NodeId, color: Rgba) {
        let size = self.tree.node(id).unwrap().size();
        self.paint(id, |b| {
            b.fill_rect(Rect::from_origin_size((0.0, 0.0), size), color);
        });
    }

    fn render(&mut self, cfg: RasterConfig, scale: f32) -> RgbaImage {
        let snap = self.tree.compose(self.vp, 1);
        VelloCpuReference::new(cfg)
            .render(&FrameView::new(&snap, scale))
            .unwrap()
    }
}

const RED: Rgba = Rgba::from_premul(1.0, 0.0, 0.0, 1.0);
const BLUE: Rgba = Rgba::from_premul(0.0, 0.0, 1.0, 1.0);

#[test]
fn pixel_aligned_rect_is_exact() {
    let mut s = Scene::new(20.0, 20.0);
    let n = s.node(None, 5.0, 5.0, 10.0, 10.0);
    s.fill(n, RED);
    let img = s.render(RasterConfig::default(), 1.0);
    assert_eq!((img.width, img.height), (20, 20));
    assert_eq!(img.pixel(5, 5), [255, 0, 0, 255]);
    assert_eq!(img.pixel(14, 14), [255, 0, 0, 255]);
    assert_eq!(img.pixel(4, 5), [0, 0, 0, 0]);
    assert_eq!(img.pixel(15, 14), [0, 0, 0, 0]);
    assert_eq!(img.pixel(0, 0), [0, 0, 0, 0]);
}

#[test]
fn background_and_scale_factor() {
    let mut s = Scene::new(10.0, 10.0);
    let n = s.node(None, 2.0, 2.0, 4.0, 4.0);
    s.fill(n, BLUE);
    let img = s.render(WHITE_BG, 2.0);
    assert_eq!((img.width, img.height), (20, 20));
    assert_eq!(img.pixel(0, 0), [255, 255, 255, 255]);
    assert_eq!(img.pixel(4, 4), [0, 0, 255, 255]);
    assert_eq!(img.pixel(11, 11), [0, 0, 255, 255]);
    assert_eq!(img.pixel(12, 12), [255, 255, 255, 255]);
}

#[test]
fn viewport_origin_maps_to_pixel_zero() {
    let mut s = Scene::new(100.0, 100.0);
    let n = s.node(None, 50.0, 50.0, 10.0, 10.0);
    s.fill(n, RED);
    s.vp = Rect::new(50.0, 50.0, 60.0, 60.0);
    let img = s.render(RasterConfig::default(), 1.0);
    assert_eq!((img.width, img.height), (10, 10));
    assert_eq!(img.pixel(0, 0), [255, 0, 0, 255]);
    assert_eq!(img.pixel(9, 9), [255, 0, 0, 255]);
}

#[test]
fn opacity_is_multiplied_not_grouped() {
    let mut s = Scene::new(10.0, 10.0);
    let p = s.node(None, 0.0, 0.0, 10.0, 10.0);
    let c = s.node(Some(p), 0.0, 0.0, 10.0, 10.0);
    s.fill(c, RED);
    s.tree.set_opacity(p, 0.5);
    s.tree.set_opacity(c, 0.5);
    let img = s.render(WHITE_BG, 1.0);
    // 25 % red over white: green and blue channels drop to 75 %.
    let [r, g, b, a] = img.pixel(5, 5);
    assert_eq!((r, a), (255, 255));
    assert!((i32::from(g) - 191).abs() <= 1 && g == b, "got {g}");
}

#[test]
fn paint_order_decides_the_overlap() {
    let mut s = Scene::new(10.0, 10.0);
    let a = s.node(None, 0.0, 0.0, 10.0, 10.0);
    let b = s.node(None, 0.0, 0.0, 10.0, 10.0);
    s.fill(a, RED);
    s.fill(b, BLUE);
    assert_eq!(
        s.render(RasterConfig::default(), 1.0).pixel(5, 5),
        [0, 0, 255, 255]
    );
    s.tree.set_z(a, 1.0);
    assert_eq!(
        s.render(RasterConfig::default(), 1.0).pixel(5, 5),
        [255, 0, 0, 255]
    );
    s.tree.set_z(a, -1.0);
    s.tree.set_z(b, -2.0);
    // Both are under the root, but a (z -1) is above b (z -2).
    assert_eq!(
        s.render(RasterConfig::default(), 1.0).pixel(5, 5),
        [255, 0, 0, 255]
    );
}

#[test]
fn axis_aligned_clip_cuts_content() {
    let mut s = Scene::new(20.0, 20.0);
    let clipper = s.node(None, 5.0, 5.0, 10.0, 10.0);
    s.tree
        .set_clip(clipper, Some(ClipSpec::bounds(Size::new(10.0, 10.0))));
    let big = s.node(Some(clipper), -5.0, -5.0, 30.0, 30.0);
    s.fill(big, RED);
    let img = s.render(RasterConfig::default(), 1.0);
    assert_eq!(img.pixel(5, 5), [255, 0, 0, 255]);
    assert_eq!(img.pixel(14, 14), [255, 0, 0, 255]);
    assert_eq!(img.pixel(4, 10), [0, 0, 0, 0]);
    assert_eq!(img.pixel(15, 10), [0, 0, 0, 0]);
    assert_eq!(img.pixel(10, 15), [0, 0, 0, 0]);
}

#[test]
fn nested_clips_intersect_and_siblings_are_unclipped() {
    let mut s = Scene::new(30.0, 10.0);
    let outer = s.node(None, 0.0, 0.0, 20.0, 10.0);
    s.tree
        .set_clip(outer, Some(ClipSpec::bounds(Size::new(20.0, 10.0))));
    let inner = s.node(Some(outer), 10.0, 0.0, 20.0, 10.0);
    s.tree
        .set_clip(inner, Some(ClipSpec::bounds(Size::new(20.0, 10.0))));
    let big = s.node(Some(inner), -10.0, 0.0, 40.0, 10.0);
    s.fill(big, RED);
    let after = s.node(None, 25.0, 0.0, 5.0, 10.0);
    s.fill(after, BLUE);
    let img = s.render(RasterConfig::default(), 1.0);
    assert_eq!(img.pixel(9, 5), [0, 0, 0, 0], "left of the inner clip");
    assert_eq!(img.pixel(10, 5), [255, 0, 0, 255]);
    assert_eq!(img.pixel(19, 5), [255, 0, 0, 255]);
    assert_eq!(img.pixel(20, 5), [0, 0, 0, 0], "right of the outer clip");
    assert_eq!(
        img.pixel(27, 5),
        [0, 0, 255, 255],
        "pushed clips are popped"
    );
}

#[test]
fn rounded_rect_and_border_golden() {
    let mut s = Scene::new(32.0, 32.0);
    let n = s.node(None, 2.0, 2.0, 28.0, 28.0);
    s.paint(n, |b| {
        b.rounded_rect(
            Rect::new(0.0, 0.0, 28.0, 28.0),
            [10.0, 10.0, 10.0, 10.0],
            Paint::Solid(Rgba::rgb(240, 200, 60)),
            Some(Border {
                width: 3.0,
                paint: Paint::Solid(Rgba::rgb(20, 40, 160)),
            }),
        );
    });
    let img = s.render(WHITE_BG, 1.0);
    assert_eq!(img.pixel(2, 2), [255, 255, 255, 255], "corner is cut away");
    assert_eq!(img.pixel(16, 16), [240, 200, 60, 255], "fill");
    assert_eq!(img.pixel(16, 3), [20, 40, 160, 255], "border, top edge");
    assert_eq!(img.pixel(3, 16), [20, 40, 160, 255], "border, left edge");
    assert_golden("rounded_rect_border", &img);
}

#[test]
fn per_corner_radii_golden() {
    let mut s = Scene::new(24.0, 24.0);
    let n = s.node(None, 2.0, 2.0, 20.0, 20.0);
    s.paint(n, |b| {
        b.rounded_rect(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            [0.0, 10.0, 0.0, 6.0],
            Paint::Solid(Rgba::rgb(30, 160, 90)),
            None,
        );
    });
    let img = s.render(WHITE_BG, 1.0);
    assert_eq!(img.pixel(2, 2), [30, 160, 90, 255], "square corner");
    assert_eq!(img.pixel(21, 2), [255, 255, 255, 255], "round corner");
    assert_golden("per_corner_radii", &img);
}

#[test]
fn aliased_rect_has_only_two_colours() {
    let mut s = Scene::new(16.0, 16.0);
    let n = s.node(None, 0.0, 0.0, 16.0, 16.0);
    s.paint(n, |b| {
        b.push(ljos_render::DrawOp::Rect {
            rect: Rect::new(0.0, 0.0, 16.0, 16.0),
            radii: [8.0; 4],
            fill: Paint::Solid(RED),
            border: None,
            aa: false,
        });
    });
    let img = s.render(RasterConfig::default(), 1.0);
    for px in img.data.as_chunks::<4>().0 {
        assert!(*px == [255, 0, 0, 255] || *px == [0, 0, 0, 0], "{px:?}");
    }
}

#[test]
fn horizontal_gradient_golden() {
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
    s.paint(n, |b| {
        b.rounded_rect(
            Rect::new(0.0, 0.0, 32.0, 8.0),
            [0.0; 4],
            Paint::LinearGradient(Arc::new(g)),
            None,
        );
    });
    let img = s.render(RasterConfig::default(), 1.0);
    assert!(img.pixel(0, 4)[0] < 20, "starts near black");
    let mid = img.pixel(16, 4);
    assert!(
        mid[0] > 240 && mid[1] < 30 && mid[2] < 30,
        "red in the middle: {mid:?}"
    );
    assert!(img.pixel(31, 4)[1] > 225, "ends near white");
    // Rows are identical for a horizontal gradient.
    assert_eq!(img.pixel(7, 0), img.pixel(7, 7));
    assert_golden("horizontal_gradient", &img);
}

#[test]
fn gradient_follows_opacity() {
    let mut s = Scene::new(8.0, 8.0);
    let n = s.node(None, 0.0, 0.0, 8.0, 8.0);
    let g = LinearGradient::new(
        Point::new(0.0, 0.0),
        Point::new(0.0, 8.0),
        vec![
            GradientStop {
                offset: 0.0,
                color: RED,
            },
            GradientStop {
                offset: 1.0,
                color: RED,
            },
        ],
    );
    s.paint(n, |b| {
        b.rounded_rect(
            Rect::new(0.0, 0.0, 8.0, 8.0),
            [0.0; 4],
            Paint::LinearGradient(Arc::new(g)),
            None,
        );
    });
    s.tree.set_opacity(n, 0.5);
    let img = s.render(RasterConfig::default(), 1.0);
    let [r, g, b, a] = img.pixel(4, 4);
    assert_eq!((r, g, b), (255, 0, 0));
    assert!((i32::from(a) - 128).abs() <= 1);
}

#[test]
fn paths_fill_and_stroke() {
    let mut s = Scene::new(24.0, 24.0);
    let n = s.node(None, 0.0, 0.0, 24.0, 24.0);
    let mut tri = BezPath::new();
    tri.move_to((2.0, 2.0));
    tri.line_to((22.0, 2.0));
    tri.line_to((2.0, 22.0));
    tri.close_path();
    let mut line = BezPath::new();
    line.move_to((22.0, 12.0));
    line.line_to((22.0, 23.0));
    s.paint(n, |b| {
        b.path(tri, PathStyle::Fill, Paint::Solid(RED));
        b.path(line, PathStyle::Stroke { width: 2.0 }, Paint::Solid(BLUE));
    });
    let img = s.render(RasterConfig::default(), 1.0);
    assert_eq!(img.pixel(5, 5), [255, 0, 0, 255]);
    assert_eq!(img.pixel(20, 20), [0, 0, 0, 0]);
    assert_eq!(img.pixel(21, 18), [0, 0, 255, 255]);
    assert_golden("path_fill_stroke", &img);
}

#[test]
fn shadow_blurs_outside_the_caster() {
    let mut s = Scene::new(40.0, 40.0);
    let n = s.node(None, 10.0, 10.0, 20.0, 20.0);
    s.paint(n, |b| {
        b.shadow(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            [4.0; 4],
            8.0,
            0.0,
            Rgba::BLACK,
        );
    });
    let img = s.render(WHITE_BG, 1.0);
    assert!(
        img.pixel(20, 20)[0] <= 4,
        "core of the shadow is near-opaque black"
    );
    let edge = img.pixel(8, 20)[0];
    assert!(
        edge > 0 && edge < 255,
        "penumbra outside the caster: {edge}"
    );
    assert_eq!(
        img.pixel(0, 0),
        [255, 255, 255, 255],
        "far corner untouched"
    );
}

#[test]
fn a_shadow_spreads_outward() {
    let mut s = Scene::new(60.0, 60.0);
    let n = s.node(None, 20.0, 20.0, 20.0, 20.0);
    s.paint(n, |b| {
        b.shadow(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            [0.0; 4],
            0.0,
            10.0,
            Rgba::BLACK,
        );
    });
    let img = s.render(WHITE_BG, 1.0);
    // The caster grown by 10 on every side: 10..50.
    assert_eq!(img.pixel(12, 30), [0, 0, 0, 255], "inside the spread");
    assert_eq!(img.pixel(47, 30), [0, 0, 0, 255], "inside the spread");
    assert_eq!(img.pixel(7, 30), [255, 255, 255, 255], "past the spread");
}

#[test]
fn text_stopgap_boxes_mode() {
    let mut s = Scene::new(20.0, 10.0);
    let n = s.node(None, 0.0, 0.0, 20.0, 10.0);
    s.paint(n, |b| {
        b.text_stopgap(
            Rect::new(2.0, 2.0, 18.0, 8.0),
            "hi",
            12.0,
            Rgba::BLACK,
            TextAlign::Left,
        );
    });
    let skip = s.render(RasterConfig::default(), 1.0);
    assert!(skip.data.iter().all(|b| *b == 0), "Skip draws nothing");
    let boxes = s.render(
        RasterConfig {
            text: TextMode::Boxes,
            ..Default::default()
        },
        1.0,
    );
    assert_eq!(boxes.pixel(10, 5)[3], 64);
    assert_eq!(boxes.pixel(0, 0)[3], 0);
}

#[test]
fn rendering_is_deterministic_and_png_round_trips() {
    let mut s = Scene::new(32.0, 32.0);
    let n = s.node(None, 1.5, 1.5, 28.3, 28.7);
    s.paint(n, |b| {
        b.rounded_rect(
            Rect::new(0.0, 0.0, 28.3, 28.7),
            [7.5; 4],
            Paint::Solid(Rgba::from_straight(0.2, 0.6, 0.9, 0.7)),
            None,
        );
    });
    let a = s.render(WHITE_BG, 1.0);
    let b = s.render(WHITE_BG, 1.0);
    assert_eq!(a, b);
    assert_eq!(RgbaImage::from_png(&a.to_png().unwrap()).unwrap(), a);
}

#[test]
fn backend_trait_snapshot_and_encode() {
    let mut s = Scene::new(8.0, 8.0);
    let n = s.node(None, 0.0, 0.0, 8.0, 8.0);
    s.fill(n, RED);
    let snap = s.tree.compose(s.vp, 1);
    let view = FrameView::new(&snap, 1.0);
    let mut r = VelloCpuReference::default();
    r.sync(&view);
    let shot = r.snapshot_rgba(&view).unwrap();
    assert_eq!(shot.pixel(3, 3), [255, 0, 0, 255]);
    let mut sink = RgbaImage::filled(16, 16, [0; 4]);
    let stats = r.encode(&view, &mut sink);
    assert_eq!((sink.width, sink.height), (16, 16));
    assert_eq!(stats.items, 1);
    assert_eq!(sink.pixel(3, 3), [255, 0, 0, 255]);
    assert_eq!(sink.pixel(12, 12), [0, 0, 0, 0]);
    r.trim(ljos_render::MemoryPressure::Critical);
    assert!(r.snapshot_rgba(&view).is_some(), "usable after trim");
    assert!(
        r.render_sized(&view, 0, 4).is_err(),
        "zero-sized targets are rejected"
    );
}

#[test]
fn rotated_clip_is_exact_in_r() {
    let mut s = Scene::new(40.0, 40.0);
    let clipper = s.node(None, 0.0, 0.0, 20.0, 20.0);
    s.tree
        .set_clip(clipper, Some(ClipSpec::bounds(Size::new(20.0, 20.0))));
    s.tree.set_local(
        clipper,
        Affine::translate((20.0, 4.0)) * Affine::rotate(std::f64::consts::FRAC_PI_4),
    );
    let big = s.node(Some(clipper), -20.0, -20.0, 80.0, 80.0);
    s.fill(big, RED);
    let img = s.render(RasterConfig::default(), 1.0);
    assert_eq!(img.pixel(20, 14), [255, 0, 0, 255], "centre of the diamond");
    assert_eq!(
        img.pixel(8, 10),
        [0, 0, 0, 0],
        "outside the diamond but inside its bbox"
    );
    assert_golden("rotated_clip", &img);
}
