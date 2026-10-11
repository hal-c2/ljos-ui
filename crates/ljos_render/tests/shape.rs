//! `DrawOp::Shape` as pixels, from both backends and one display list.
//!
//! R is read from its picture. B0 hands triangles to egui, so they are
//! filled here the way a GPU without multisampling fills them: each pixel
//! centre takes what its triangle has there, a textured triangle the nearest
//! texel. The Qt numbers quoted are what Qt 6.11.2 paints (qml6, software
//! backend); `features/shapes/` holds the full comparison.
#![cfg(feature = "b0")]

use std::collections::HashMap;
use std::sync::Arc;

use egui::epaint::{ClippedShape, ImageData, Shape};
use egui::{Color32, ColorImage, Context, Pos2, TextureId};
use ljos_render::b0::{EguiSink, EpaintBackend};
use ljos_render::kurbo::{Affine, BezPath, Cap, Circle, Join, Rect, Shape as _, Size, Stroke};
use ljos_render::{
    Brush, BrushGradient, DisplayListBuilder, EncodeStats, FillRule, FrameView, GradientGeometry,
    GradientStop, MemoryPressure, NodeId, RasterConfig, RenderBackend, RenderTree, Rgba,
    ShapeStyle, Spread, TextMode, VelloCpuReference,
};

const RED: Rgba = Rgba::from_premul(1.0, 0.0, 0.0, 1.0);
const BLUE: Rgba = Rgba::from_premul(0.0, 0.0, 1.0, 1.0);
const GREEN: Rgba = Rgba::from_premul(0.0, 1.0, 0.0, 1.0);

type Rgb = [u8; 3];

struct Scene {
    tree: RenderTree,
    vp: Rect,
    frame: u64,
    ppp: f32,
    textures: HashMap<TextureId, Arc<ColorImage>>,
}

struct Frame {
    shapes: Vec<ClippedShape>,
    stats: EncodeStats,
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
            ppp: 1.0,
            textures: HashMap::new(),
        }
    }

    fn node(&mut self, x: f64, y: f64) -> NodeId {
        let n = self.tree.add_child(self.tree.root());
        self.tree.set_pos(n, x, y);
        self.tree.set_size(n, Size::new(10.0, 10.0));
        n
    }

    fn content(&mut self, id: NodeId, f: impl FnOnce(&mut DisplayListBuilder)) {
        let mut b = DisplayListBuilder::new();
        f(&mut b);
        self.tree.set_content(id, Some(b.finish_arc()));
    }

    /// One node at the origin with this content.
    fn with(w: f64, h: f64, f: impl FnOnce(&mut DisplayListBuilder)) -> Self {
        let mut s = Self::new(w, h);
        let n = s.node(0.0, 0.0);
        s.content(n, f);
        s
    }

    fn r(&mut self) -> ljos_render::RgbaImage {
        let snap = self.tree.compose(self.vp, self.frame + 1);
        VelloCpuReference::new(RasterConfig {
            background: Rgba::WHITE,
            text: TextMode::Skip,
        })
        .render(&FrameView::new(&snap, self.ppp))
        .unwrap()
    }

    fn b0(&mut self, be: &mut EpaintBackend, ctx: Option<&Context>) -> Frame {
        self.frame += 1;
        let snap = self.tree.compose(self.vp, self.frame);
        let view = FrameView::new(&snap, self.ppp);
        be.sync(&view);
        let mut sink = EguiSink::new(ctx, Pos2::ZERO);
        let stats = be.encode(&view, &mut sink);
        if let Some(ctx) = ctx {
            let mut delta = ctx.tex_manager().write().take_delta();
            for (id, deltas) in &delta.set {
                for d in deltas {
                    assert!(d.pos.is_none(), "shape textures are set whole");
                    let ImageData::Color(image) = &d.image;
                    self.textures.insert(*id, Arc::clone(image));
                }
            }
            // Read, which is all a test does with them.
            delta.clear();
        }
        Frame {
            shapes: sink.shapes,
            stats,
        }
    }

    /// What B0's shapes give the device pixel, over white.
    fn sample(&self, frame: &Frame, x: u32, y: u32) -> Rgb {
        let p = Pos2::new((x as f32 + 0.5) / self.ppp, (y as f32 + 0.5) / self.ppp);
        let mut dst = [255.0_f32; 3];
        for shape in &frame.shapes {
            let Shape::Mesh(mesh) = &shape.shape else {
                panic!("a shape is a mesh, got {:?}", shape.shape);
            };
            if !shape.clip_rect.contains(p) {
                continue;
            }
            let texture =
                (mesh.texture_id != TextureId::default()).then(|| &self.textures[&mesh.texture_id]);
            for tri in mesh.indices.as_chunks::<3>().0 {
                let [a, b, c] = tri.map(|i| mesh.vertices[i as usize]);
                let Some(weights) = inside(a.pos, b.pos, c.pos, p) else {
                    continue;
                };
                let lerp = |f: &dyn Fn(&egui::epaint::Vertex) -> f32| -> f32 {
                    [a, b, c].iter().zip(weights).map(|(v, k)| f(v) * k).sum()
                };
                let mut src: [f32; 4] =
                    std::array::from_fn(|ch| lerp(&|v| f32::from(v.color.to_array()[ch])));
                if let Some(t) = texture {
                    let (u, v) = (lerp(&|v| v.uv.x), lerp(&|v| v.uv.y));
                    let tx = ((u * t.size[0] as f32) as usize).min(t.size[0] - 1);
                    let ty = ((v * t.size[1] as f32) as usize).min(t.size[1] - 1);
                    let texel: Color32 = t.pixels[ty * t.size[0] + tx];
                    for (s, t) in src.iter_mut().zip(texel.to_array()) {
                        *s = *s * f32::from(t) / 255.0;
                    }
                }
                // Premultiplied source over.
                for (d, s) in dst.iter_mut().zip(src) {
                    *d = s + *d * (1.0 - src[3] / 255.0);
                }
            }
        }
        dst.map(|c| c.round().clamp(0.0, 255.0) as u8)
    }
}

/// The weights of `p` in the triangle, or `None` outside it. A pixel centre
/// on an edge two triangles share belongs to one of them (the top-left rule).
fn inside(a: Pos2, b: Pos2, c: Pos2, p: Pos2) -> Option<[f32; 3]> {
    let edge = |a: Pos2, b: Pos2, p: Pos2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let area = edge(a, b, c);
    if area == 0.0 {
        return None;
    }
    let (b, c, flipped) = if area < 0.0 {
        (c, b, true)
    } else {
        (b, c, false)
    };
    let area = area.abs();
    let owns = |a: Pos2, b: Pos2| (b.y == a.y && b.x > a.x) || b.y < a.y;
    let covers = |a: Pos2, b: Pos2| {
        let w = edge(a, b, p);
        (w > 0.0 || (w == 0.0 && owns(a, b))).then_some(w / area)
    };
    let (wa, wb, wc) = (covers(b, c)?, covers(c, a)?, covers(a, b)?);
    Some(if flipped { [wa, wc, wb] } else { [wa, wb, wc] })
}

fn rgb(image: &ljos_render::RgbaImage, x: u32, y: u32) -> Rgb {
    let [r, g, b, a] = image.pixel(x, y);
    // A faded shape over white can come out one step short of opaque.
    assert!(a >= 254, "R paints over a white background");
    [r, g, b]
}

#[track_caller]
fn near(got: Rgb, want: Rgb, by: u8, what: &str) {
    assert!(
        got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= by),
        "{what}: {got:?}, wanted {want:?} within {by}"
    );
}

/// How far B0's picture is from R's: the mean over the pixels that differ at
/// all in either, and the largest difference, in steps of 1/255.
fn difference(scene: &mut Scene, ctx: Option<&Context>) -> (f32, u8) {
    let r = scene.r();
    let mut be = EpaintBackend::new();
    let frame = scene.b0(&mut be, ctx);
    let (mut sum, mut n, mut max) = (0u64, 0u64, 0u8);
    for y in 0..r.height {
        for x in 0..r.width {
            let (a, b) = (rgb(&r, x, y), scene.sample(&frame, x, y));
            let d = a.iter().zip(b).map(|(a, b)| a.abs_diff(b)).max().unwrap();
            if a != [255; 3] || b != [255; 3] {
                sum += u64::from(d);
                n += 1;
            }
            max = max.max(d);
        }
    }
    assert!(n > 0, "nothing was painted");
    (sum as f32 / n as f32, max)
}

fn path(points: &[(f64, f64)], close: bool) -> Arc<BezPath> {
    let mut p = BezPath::new();
    for (i, &(x, y)) in points.iter().enumerate() {
        if i == 0 {
            p.move_to((x, y));
        } else {
            p.line_to((x, y));
        }
    }
    if close {
        p.close_path();
    }
    Arc::new(p)
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Arc<BezPath> {
    path(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], true)
}

fn stroke(width: f64) -> Stroke {
    Stroke::new(width)
        .with_caps(Cap::Butt)
        .with_join(Join::Bevel)
}

fn stroked(s: Stroke) -> ShapeStyle {
    ShapeStyle::Stroke(Arc::new(s))
}

fn stops(colours: &[(f32, Rgba)]) -> Vec<GradientStop> {
    colours
        .iter()
        .map(|&(offset, color)| GradientStop { offset, color })
        .collect()
}

fn linear(from: (f64, f64), to: (f64, f64), spread: Spread, colours: &[(f32, Rgba)]) -> Brush {
    Brush::Gradient(Arc::new(BrushGradient::new(
        GradientGeometry::Linear {
            start: from.into(),
            end: to.into(),
        },
        stops(colours),
        spread,
    )))
}

// ------------------------------------------------------------------ R

#[test]
fn a_fill_follows_its_rule() {
    // Two squares, one inside the other, wound the same way.
    let mut p = BezPath::new();
    for (a, b) in [(10.0, 90.0), (30.0, 70.0)] {
        p.move_to((a, a));
        p.line_to((b, a));
        p.line_to((b, b));
        p.line_to((a, b));
        p.close_path();
    }
    let p = Arc::new(p);
    for (rule, middle) in [
        (FillRule::NonZero, [255, 0, 0]),
        (FillRule::EvenOdd, [255, 255, 255]),
    ] {
        let mut s = Scene::with(100.0, 100.0, |b| {
            b.shape(Arc::clone(&p), ShapeStyle::Fill(rule), RED.into());
        });
        let r = s.r();
        assert_eq!(rgb(&r, 50, 50), middle, "{rule:?}");
        assert_eq!(rgb(&r, 20, 50), [255, 0, 0], "{rule:?}");
        assert_eq!(rgb(&r, 5, 50), [255, 255, 255], "{rule:?}");
        let mut be = EpaintBackend::new();
        let f = s.b0(&mut be, None);
        assert_eq!(s.sample(&f, 50, 50), middle, "B0 {rule:?}");
        assert_eq!(s.sample(&f, 20, 50), [255, 0, 0], "B0 {rule:?}");
    }
}

#[test]
fn an_open_path_is_filled_as_if_closed() {
    let p = path(&[(10.0, 10.0), (90.0, 10.0), (90.0, 90.0)], false);
    let mut s = Scene::with(100.0, 100.0, |b| {
        b.shape(p, ShapeStyle::Fill(FillRule::NonZero), RED.into());
    });
    let r = s.r();
    assert_eq!(rgb(&r, 80, 30), [255, 0, 0]);
    assert_eq!(rgb(&r, 30, 80), [255, 255, 255]);
}

#[test]
fn a_stroke_one_unit_wide_on_a_pixel_edge_is_half_in_each_row() {
    // Qt: rows 49 and 50 at #808080 for a black line at y = 50.
    let p = path(&[(10.0, 50.0), (90.0, 50.0)], false);
    let mut s = Scene::with(100.0, 100.0, |b| {
        b.shape(p, stroked(stroke(1.0)), Rgba::BLACK.into());
    });
    let r = s.r();
    for y in [49, 50] {
        near(rgb(&r, 50, y), [128; 3], 1, "R");
    }
    assert_eq!(rgb(&r, 50, 48), [255; 3]);
    assert_eq!(rgb(&r, 50, 51), [255; 3]);
    let mut be = EpaintBackend::new();
    let f = s.b0(&mut be, None);
    for y in [49, 50] {
        near(s.sample(&f, 50, y), [128; 3], 2, "B0");
    }
    assert_eq!(s.sample(&f, 50, 47), [255; 3]);
}

#[test]
fn a_stroke_without_width_or_colour_draws_nothing() {
    let p = path(&[(10.0, 50.0), (90.0, 50.0)], false);
    for (width, color) in [
        (0.0, Rgba::BLACK),
        (-2.0, Rgba::BLACK),
        (4.0, Rgba::TRANSPARENT),
    ] {
        let p = Arc::clone(&p);
        let mut s = Scene::with(100.0, 100.0, |b| {
            b.shape(p, stroked(stroke(width)), color.into());
        });
        let r = s.r();
        assert!(
            (0..100).all(|x| rgb(&r, x, 50) == [255; 3]),
            "width {width}"
        );
        let mut be = EpaintBackend::new();
        let f = s.b0(&mut be, None);
        assert!(f.shapes.is_empty(), "width {width}");
        assert_eq!(be.shape_tessellations(), 0);
    }
}

/// The columns of row `y` that have ink, as runs of `first..=last`.
fn runs(image: &ljos_render::RgbaImage, y: u32) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for x in 0..image.width {
        if rgb(image, x, y)[0] < 128 {
            match out.last_mut() {
                Some(last) if last.1 + 1 == x => last.1 = x,
                _ => out.push((x, x)),
            }
        }
    }
    out
}

#[test]
fn dashes_follow_the_pattern_the_offset_and_the_caps() {
    let p = path(&[(10.0, 50.0), (90.0, 50.0)], false);
    let dashed = |pattern: &[f64], offset: f64, cap: Cap| {
        let p = Arc::clone(&p);
        let style = stroked(
            stroke(4.0)
                .with_caps(cap)
                .with_dashes(offset, pattern.iter().copied()),
        );
        let mut s = Scene::with(100.0, 100.0, |b| {
            b.shape(p, style, Rgba::BLACK.into());
        });
        let r = s.r();
        let mut be = EpaintBackend::new();
        let f = s.b0(&mut be, None);
        // The triangles put the same pixels in ink.
        for x in 0..100 {
            let (a, b) = (rgb(&r, x, 50)[0], s.sample(&f, x, 50)[0]);
            assert!(a.abs_diff(b) <= 8, "B0 at {x}: {b}, R has {a}");
        }
        runs(&r, 50)
    };
    // Qt: width 4, pattern [4, 2] in widths, from x = 10.
    assert_eq!(
        dashed(&[16.0, 8.0], 0.0, Cap::Butt)[..2],
        [(10, 25), (34, 49)]
    );
    // Qt: dashOffset 1 starts one width into the pattern.
    assert_eq!(
        dashed(&[16.0, 8.0], 4.0, Cap::Butt)[..2],
        [(10, 21), (30, 45)]
    );
    // Qt: square caps add half a width to both ends of every dash.
    assert_eq!(
        dashed(&[16.0, 8.0], 0.0, Cap::Square)[..2],
        [(8, 27), (32, 51)]
    );
    // A pattern that never advances is a solid line, not a hang.
    assert_eq!(dashed(&[0.0, 0.0], 0.0, Cap::Butt), [(10, 89)]);
}

#[test]
fn joins_and_the_miter_limit() {
    // A corner pointing left with its tip at (30, 50).
    let p = path(&[(90.0, 40.0), (30.0, 50.0), (90.0, 60.0)], false);
    let tip = |join: Join, limit: f64| {
        let p = Arc::clone(&p);
        let style = stroked(stroke(10.0).with_join(join).with_miter_limit(limit));
        let mut s = Scene::with(100.0, 100.0, |b| {
            b.shape(p, style, Rgba::BLACK.into());
        });
        let r = s.r();
        (rgb(&r, 10, 50), rgb(&r, 27, 50))
    };
    // The miter of this corner is a little over six widths long.
    assert_eq!(tip(Join::Miter, 7.0).0, [0; 3]);
    assert_eq!(tip(Join::Miter, 6.0).0, [255; 3]);
    assert_eq!(tip(Join::Bevel, 7.0).0, [255; 3]);
    assert_eq!(tip(Join::Round, 7.0), ([255; 3], [0; 3]));
}

#[test]
fn a_linear_gradient_pads_repeats_and_reflects() {
    let colours = [(0.0, RED), (1.0, BLUE)];
    for (spread, at_10, at_70, at_90) in [
        // Past the end: the last colour.
        (Spread::Pad, [215, 0, 40], [0, 0, 255], [0, 0, 255]),
        // The second period starts again at red.
        (Spread::Repeat, [215, 0, 40], [215, 0, 40], [130, 0, 125]),
        // The second period runs backwards.
        (Spread::Reflect, [215, 0, 40], [40, 0, 215], [125, 0, 130]),
    ] {
        let brush = linear((0.0, 0.0), (60.0, 0.0), spread, &colours);
        let mut s = Scene::with(120.0, 20.0, |b| {
            b.shape(
                rect(0.0, 0.0, 120.0, 20.0),
                ShapeStyle::Fill(FillRule::NonZero),
                brush,
            );
        });
        let r = s.r();
        let mut be = EpaintBackend::new();
        let f = s.b0(&mut be, None);
        assert_eq!(f.stats.warnings, 0, "{spread:?} is exact as triangles");
        for (x, want) in [(9, at_10), (69, at_70), (89, at_90)] {
            near(rgb(&r, x, 10), want, 4, &format!("R {spread:?} at {x}"));
            near(
                s.sample(&f, x, 10),
                want,
                4,
                &format!("B0 {spread:?} at {x}"),
            );
        }
    }
}

#[test]
fn a_gradient_that_cannot_change_is_one_colour() {
    let fill = ShapeStyle::Fill(FillRule::NonZero);
    let cases: [(Brush, Option<Rgb>); 3] = [
        // Start and end at one point: the first stop.
        (
            linear(
                (5.0, 5.0),
                (5.0, 5.0),
                Spread::Pad,
                &[(0.0, RED), (1.0, BLUE)],
            ),
            Some([255, 0, 0]),
        ),
        (
            linear((0.0, 0.0), (20.0, 0.0), Spread::Pad, &[(0.3, GREEN)]),
            Some([0, 255, 0]),
        ),
        (linear((0.0, 0.0), (20.0, 0.0), Spread::Pad, &[]), None),
    ];
    for (brush, want) in cases {
        let mut s = Scene::with(20.0, 20.0, |b| {
            b.shape(rect(0.0, 0.0, 20.0, 20.0), fill.clone(), brush);
        });
        let r = s.r();
        let mut be = EpaintBackend::new();
        let f = s.b0(&mut be, None);
        assert_eq!(rgb(&r, 15, 10), want.unwrap_or([255; 3]));
        assert_eq!(s.sample(&f, 15, 10), want.unwrap_or([255; 3]));
    }
}

fn radial(focal: (f64, f64), focal_radius: f64, center: (f64, f64), radius: f64) -> Brush {
    Brush::Gradient(Arc::new(BrushGradient::new(
        GradientGeometry::Radial {
            focal: focal.into(),
            focal_radius,
            center: center.into(),
            radius,
        },
        stops(&[(0.0, RED), (1.0, BLUE)]),
        Spread::Pad,
    )))
}

fn conical(center: (f64, f64), angle: f64) -> Brush {
    Brush::Gradient(Arc::new(BrushGradient::new(
        GradientGeometry::Conical {
            center: center.into(),
            angle,
        },
        stops(&[(0.0, RED), (1.0, BLUE)]),
        Spread::Pad,
    )))
}

#[test]
fn a_radial_gradient_runs_from_the_focal_circle_to_the_outer_one() {
    let mut s = Scene::with(100.0, 100.0, |b| {
        b.shape(
            rect(0.0, 0.0, 100.0, 100.0),
            ShapeStyle::Fill(FillRule::NonZero),
            radial((50.0, 50.0), 0.0, (50.0, 50.0), 40.0),
        );
    });
    let r = s.r();
    near(rgb(&r, 50, 50), [253, 0, 2], 4, "the middle");
    near(rgb(&r, 70, 50), [124, 0, 131], 4, "half way out");
    near(rgb(&r, 95, 50), [0, 0, 255], 1, "past the circle");
    near(rgb(&r, 50, 30), [131, 0, 124], 4, "half way up");
    // A focal circle with a radius: red inside it.
    let mut s = Scene::with(100.0, 100.0, |b| {
        b.shape(
            rect(0.0, 0.0, 100.0, 100.0),
            ShapeStyle::Fill(FillRule::NonZero),
            radial((50.0, 50.0), 20.0, (50.0, 50.0), 40.0),
        );
    });
    let r = s.r();
    near(rgb(&r, 60, 50), [255, 0, 0], 1, "inside the focal circle");
    near(rgb(&r, 80, 50), [124, 0, 131], 5, "between the circles");
}

#[test]
fn a_conical_gradient_turns_anticlockwise_from_its_angle() {
    let at = |angle: f64| {
        let mut s = Scene::with(100.0, 100.0, |b| {
            b.shape(
                rect(0.0, 0.0, 100.0, 100.0),
                ShapeStyle::Fill(FillRule::NonZero),
                conical((50.0, 50.0), angle),
            );
        });
        let r = s.r();
        // Up, left, down: a quarter, a half and three quarters of a turn on
        // a screen whose y axis points down.
        [rgb(&r, 50, 10), rgb(&r, 10, 50), rgb(&r, 50, 90)]
    };
    let [up, left, down] = at(0.0);
    near(up, [191, 0, 64], 4, "a quarter turn");
    near(left, [128, 0, 128], 4, "half a turn");
    near(down, [64, 0, 191], 4, "three quarters");
    // Starting at 90 degrees, straight up is where it begins and ends.
    let [_, left, down] = at(90.0);
    near(left, [191, 0, 64], 4, "a quarter turn from 90");
    near(down, [128, 0, 128], 4, "half a turn from 90");
}

#[test]
fn opacity_fades_a_shape_once() {
    let mut s = Scene::new(40.0, 40.0);
    let n = s.node(0.0, 0.0);
    s.content(n, |b| {
        b.shape(
            rect(0.0, 0.0, 40.0, 40.0),
            ShapeStyle::Fill(FillRule::NonZero),
            Rgba::BLACK.into(),
        );
    });
    s.tree.set_opacity(n, 0.5);
    let r = s.r();
    near(rgb(&r, 20, 20), [128; 3], 1, "R");
    let mut be = EpaintBackend::new();
    let f = s.b0(&mut be, None);
    near(s.sample(&f, 20, 20), [128; 3], 2, "B0");
}

// --------------------------------------------------- B0 against R

fn star() -> Arc<BezPath> {
    let mut p = BezPath::new();
    for i in 0..10 {
        let a = std::f64::consts::TAU * f64::from(i) / 10.0;
        let r = if i % 2 == 0 { 44.0 } else { 20.0 };
        let at = (50.0 + r * a.sin(), 50.0 - r * a.cos());
        if i == 0 {
            p.move_to(at);
        } else {
            p.line_to(at);
        }
    }
    p.close_path();
    Arc::new(p)
}

#[test]
fn triangles_are_close_to_the_reference_at_every_scale() {
    let circle = Arc::new(Circle::new((50.0, 50.0), 40.0).to_path(0.01));
    let wave = {
        let mut p = BezPath::new();
        p.move_to((5.0, 50.0));
        p.curve_to((30.0, -20.0), (60.0, 120.0), (95.0, 50.0));
        Arc::new(p)
    };
    let cases: Vec<(&str, Arc<BezPath>, ShapeStyle)> = vec![
        ("star", star(), ShapeStyle::Fill(FillRule::NonZero)),
        (
            "disc",
            Arc::clone(&circle),
            ShapeStyle::Fill(FillRule::NonZero),
        ),
        ("ring", circle, stroked(stroke(6.0))),
        (
            "wave",
            wave,
            stroked(stroke(5.0).with_caps(Cap::Round).with_join(Join::Round)),
        ),
    ];
    for ppp in [1.0, 1.25, 1.5, 2.0] {
        for (name, p, style) in &cases {
            let mut s = Scene::with(100.0, 100.0, |b| {
                b.shape(Arc::clone(p), style.clone(), Rgba::BLACK.into());
            });
            s.ppp = ppp;
            let (mean, max) = difference(&mut s, None);
            // The soft edge is a straight ramp where the reference has the
            // exact area; they differ most at sharp corners.
            assert!(mean < 6.0, "{name} at {ppp}: mean {mean}");
            assert!(max < 128, "{name} at {ppp}: max {max}");
        }
    }
}

#[test]
fn a_radial_gradient_is_the_reference_picture() {
    let ctx = Context::default();
    for ppp in [1.0, 1.5] {
        let mut s = Scene::with(100.0, 100.0, |b| {
            b.shape(
                star(),
                ShapeStyle::Fill(FillRule::NonZero),
                radial((40.0, 40.0), 0.0, (50.0, 50.0), 45.0),
            );
        });
        s.ppp = ppp;
        let (mean, max) = difference(&mut s, Some(&ctx));
        assert!(max <= 1, "at {ppp}: mean {mean}, max {max}");
    }
}

#[test]
fn a_shape_from_pixels_lands_where_the_reference_puts_it() {
    // Not at the origin, not on a whole pixel, and faded.
    let ctx = Context::default();
    let mut s = Scene::new(120.0, 120.0);
    let n = s.node(13.3, 7.7);
    s.content(n, |b| {
        b.shape(
            star(),
            ShapeStyle::Fill(FillRule::NonZero),
            conical((50.0, 50.0), 30.0),
        );
    });
    s.tree.set_opacity(n, 0.6);
    s.ppp = 1.5;
    let (mean, max) = difference(&mut s, Some(&ctx));
    assert!(max <= 2, "mean {mean}, max {max}");
}

#[test]
fn without_a_context_a_radial_gradient_is_one_colour_and_reported() {
    let mut s = Scene::with(100.0, 100.0, |b| {
        b.shape(
            rect(0.0, 0.0, 100.0, 100.0),
            ShapeStyle::Fill(FillRule::NonZero),
            radial((50.0, 50.0), 0.0, (50.0, 50.0), 40.0),
        );
    });
    let mut be = EpaintBackend::new();
    let f = s.b0(&mut be, None);
    assert_eq!(f.stats.warnings, 1);
    let (a, b) = (s.sample(&f, 20, 20), s.sample(&f, 50, 50));
    assert_eq!(a, b);
    near(a, [128, 0, 128], 1, "the middle of the gradient");
}

// ------------------------------------------------------- B0 caches

fn mesh_of(frame: &Frame, at: usize) -> Arc<egui::epaint::Mesh> {
    match &frame.shapes[at].shape {
        Shape::Mesh(m) => Arc::clone(m),
        other => panic!("expected a mesh, got {other:?}"),
    }
}

#[test]
fn an_unchanged_shape_is_handed_over_again_as_it_is() {
    let mut s = Scene::with(100.0, 100.0, |b| {
        b.shape(star(), ShapeStyle::Fill(FillRule::NonZero), RED.into());
    });
    let mut be = EpaintBackend::new();
    let first = s.b0(&mut be, None);
    let second = s.b0(&mut be, None);
    assert!(Arc::ptr_eq(&mesh_of(&first, 0), &mesh_of(&second, 0)));
    assert_eq!(be.shape_tessellations(), 1);
    assert_eq!(second.stats.caches_reused, 1);
}

#[test]
fn moving_turning_or_fading_a_shape_does_not_cut_it_again() {
    let mut s = Scene::new(200.0, 200.0);
    let n = s.node(0.0, 0.0);
    s.content(n, |b| {
        b.shape(star(), ShapeStyle::Fill(FillRule::NonZero), RED.into());
        b.shape(star(), stroked(stroke(2.0)), BLUE.into());
    });
    let mut be = EpaintBackend::new();
    s.b0(&mut be, None);
    assert_eq!(be.shape_tessellations(), 2);
    s.tree.set_pos(n, 30.5, 12.0);
    s.b0(&mut be, None);
    s.tree
        .set_local(n, Affine::translate((80.0, 30.0)) * Affine::rotate(0.3));
    s.b0(&mut be, None);
    s.tree.set_opacity(n, 0.5);
    let f = s.b0(&mut be, None);
    assert_eq!(f.shapes.len(), 2);
    assert_eq!(be.shape_tessellations(), 2);
    assert_eq!(be.shape_geometries(), 2);
}

#[test]
fn changing_one_shape_of_a_node_cuts_only_that_one() {
    let (a, b2) = (star(), rect(10.0, 10.0, 60.0, 60.0));
    let mut s = Scene::new(200.0, 200.0);
    let n = s.node(0.0, 0.0);
    let fill = ShapeStyle::Fill(FillRule::NonZero);
    s.content(n, |b| {
        b.shape(Arc::clone(&a), fill.clone(), RED.into());
        b.shape(Arc::clone(&b2), fill.clone(), BLUE.into());
    });
    let mut be = EpaintBackend::new();
    s.b0(&mut be, None);
    assert_eq!(be.shape_tessellations(), 2);
    // A new display list in which only the second shape's colour differs.
    s.content(n, |b| {
        b.shape(Arc::clone(&a), fill.clone(), RED.into());
        b.shape(Arc::clone(&b2), fill.clone(), GREEN.into());
    });
    s.b0(&mut be, None);
    assert_eq!(be.shape_tessellations(), 3);
    // What the blue one had built is gone with it.
    assert_eq!(be.shape_geometries(), 2);
}

#[test]
fn a_shape_out_of_sight_builds_nothing() {
    let mut s = Scene::new(100.0, 100.0);
    let n = s.node(500.0, 0.0);
    s.content(n, |b| {
        b.shape(star(), ShapeStyle::Fill(FillRule::NonZero), RED.into());
    });
    let mut be = EpaintBackend::new();
    let f = s.b0(&mut be, None);
    assert!(f.shapes.is_empty());
    assert_eq!(be.shape_tessellations(), 0);
    s.tree.set_pos(n, 10.0, 0.0);
    let f = s.b0(&mut be, None);
    assert_eq!(f.shapes.len(), 1);
    assert_eq!(be.shape_tessellations(), 1);
}

#[test]
fn memory_pressure_drops_what_a_shape_out_of_sight_built() {
    let mut s = Scene::new(100.0, 100.0);
    let (seen, gone) = (s.node(0.0, 0.0), s.node(0.0, 0.0));
    for n in [seen, gone] {
        s.content(n, |b| {
            b.shape(star(), ShapeStyle::Fill(FillRule::NonZero), RED.into());
        });
    }
    let mut be = EpaintBackend::new();
    s.b0(&mut be, None);
    assert_eq!(be.shape_geometries(), 2);
    // Scrolled away: still held, since coming back is likely.
    s.tree.set_pos(gone, 0.0, 400.0);
    s.b0(&mut be, None);
    assert_eq!(be.shape_geometries(), 2);
    be.trim(MemoryPressure::Moderate);
    assert_eq!(be.shape_geometries(), 1);
    // The one in sight is drawn from what it has.
    let before = be.shape_tessellations();
    let f = s.b0(&mut be, None);
    assert_eq!(f.shapes.len(), 1);
    assert_eq!(be.shape_tessellations(), before);
    // Coming back builds it again.
    s.tree.set_pos(gone, 0.0, 0.0);
    let f = s.b0(&mut be, None);
    assert_eq!(f.shapes.len(), 2);
    assert_eq!(be.shape_tessellations(), before + 1);
    be.trim(MemoryPressure::Critical);
    assert_eq!(be.shape_geometries(), 0);
}

#[test]
fn a_texture_is_filled_again_only_when_the_shape_lands_differently() {
    let ctx = Context::default();
    let mut s = Scene::new(300.0, 300.0);
    let n = s.node(10.0, 10.0);
    s.content(n, |b| {
        b.shape(
            star(),
            ShapeStyle::Fill(FillRule::NonZero),
            radial((50.0, 50.0), 0.0, (50.0, 50.0), 40.0),
        );
    });
    let mut be = EpaintBackend::new();
    let first = s.b0(&mut be, Some(&ctx));
    assert_eq!((be.shape_rasters(), be.shape_textures()), (1, 1));
    assert_eq!(be.shape_tessellations(), 0);
    // Nothing changed: the same mesh, no fill.
    let second = s.b0(&mut be, Some(&ctx));
    assert!(Arc::ptr_eq(&mesh_of(&first, 0), &mesh_of(&second, 0)));
    assert_eq!(be.shape_rasters(), 1);
    // Whole pixels: the same texture somewhere else.
    s.tree.set_pos(n, 50.0, 30.0);
    let moved = s.b0(&mut be, Some(&ctx));
    assert_eq!(be.shape_rasters(), 1);
    assert_eq!(mesh_of(&moved, 0).texture_id, mesh_of(&first, 0).texture_id);
    // Fading is the quad's colour.
    s.tree.set_opacity(n, 0.5);
    s.b0(&mut be, Some(&ctx));
    assert_eq!(be.shape_rasters(), 1);
    // Half a pixel: filled again, into the texture it has.
    s.tree.set_pos(n, 50.5, 30.0);
    let shifted = s.b0(&mut be, Some(&ctx));
    assert_eq!((be.shape_rasters(), be.shape_textures()), (2, 1));
    assert_eq!(
        mesh_of(&shifted, 0).texture_id,
        mesh_of(&first, 0).texture_id
    );
    // Out of sight: the pixels go at once.
    s.tree.set_pos(n, 1000.0, 30.0);
    let away = s.b0(&mut be, Some(&ctx));
    assert!(away.shapes.is_empty());
    assert_eq!(be.shape_textures(), 0);
}

#[test]
fn a_stroke_of_thousands_of_segments_is_drawn_from_pixels() {
    let mut p = BezPath::new();
    p.move_to((0.0, 50.0));
    for i in 1..5000 {
        let x = f64::from(i) * 0.06;
        p.line_to((x, 50.0 + 30.0 * (x * 0.2).sin()));
    }
    let p = Arc::new(p);
    let ctx = Context::default();
    let mut s = Scene::with(300.0, 100.0, |b| {
        b.shape(Arc::clone(&p), stroked(stroke(1.5)), Rgba::BLACK.into());
    });
    let mut be = EpaintBackend::new();
    let f = s.b0(&mut be, Some(&ctx));
    assert_eq!((be.shape_rasters(), be.shape_tessellations()), (1, 0));
    assert_eq!(f.stats.warnings, 0);
    let (_, max) = difference(&mut s, Some(&ctx));
    assert!(max <= 1, "max {max}");
    // Without a context it is still drawn, the slow way, and says so.
    let mut be = EpaintBackend::new();
    let f = s.b0(&mut be, None);
    assert_eq!((be.shape_rasters(), be.shape_tessellations()), (0, 1));
    assert_eq!(f.stats.warnings, 1);
    assert_eq!(f.shapes.len(), 1);
}

#[test]
fn a_gradient_that_repeats_very_often_is_drawn_from_pixels() {
    let ctx = Context::default();
    let colours = [(0.0, RED), (1.0, BLUE)];
    let count = |period: f64| {
        let brush = linear((0.0, 0.0), (period, 0.0), Spread::Repeat, &colours);
        let mut s = Scene::with(200.0, 20.0, |b| {
            b.shape(
                rect(0.0, 0.0, 200.0, 20.0),
                ShapeStyle::Fill(FillRule::NonZero),
                brush,
            );
        });
        let mut be = EpaintBackend::new();
        s.b0(&mut be, Some(&ctx));
        (be.shape_tessellations(), be.shape_rasters())
    };
    assert_eq!(count(50.0), (1, 0));
    assert_eq!(count(4.0), (0, 1));
}
