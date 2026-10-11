//! A rectangle filled with a linear gradient, as pixels, from both backends
//! and one display list. The numbers are what Qt 6.11.2 paints for the same
//! rectangle (qml6, software backend, grabbed and read pixel by pixel).
//!
//! R is read from its picture. B0 hands triangles to egui, so they are
//! filled here the way a GPU fills them: each pixel centre takes the colour
//! its triangle has there.
#![cfg(feature = "b0")]

use std::sync::Arc;

use egui::Pos2;
use egui::epaint::{ClippedShape, Shape};
use ljos_render::b0::{EguiSink, EpaintBackend};
use ljos_render::kurbo::{Point, Rect, Size};
use ljos_render::{
    Border, DisplayListBuilder, DrawOp, FrameView, GradientStop, LinearGradient, Paint,
    RasterConfig, RenderBackend, RenderTree, Rgba, TextMode, VelloCpuReference, fill_area,
};

const RED: Rgba = Rgba::from_premul(1.0, 0.0, 0.0, 1.0);
const BLUE: Rgba = Rgba::from_premul(0.0, 0.0, 1.0, 1.0);

/// One node over white, painted by both backends.
struct Painted {
    r: ljos_render::RgbaImage,
    b0: Vec<ClippedShape>,
}

fn paint(w: f64, h: f64, opacity: f32, content: impl FnOnce(&mut DisplayListBuilder)) -> Painted {
    let mut tree = RenderTree::new();
    let root = tree.root();
    tree.set_size(root, Size::new(w, h));
    let node = tree.add_child(root);
    tree.set_size(node, Size::new(w, h));
    tree.set_opacity(node, opacity);
    let mut list = DisplayListBuilder::new();
    content(&mut list);
    tree.set_content(node, Some(list.finish_arc()));
    let snap = tree.compose(Rect::new(0.0, 0.0, w, h), 1);
    let view = FrameView::new(&snap, 1.0);
    let r = VelloCpuReference::new(RasterConfig {
        background: Rgba::WHITE,
        text: TextMode::Skip,
    })
    .render(&view)
    .unwrap();
    let mut backend = EpaintBackend::new();
    backend.sync(&view);
    let mut sink = EguiSink::new(None, Pos2::ZERO);
    backend.encode(&view, &mut sink);
    Painted { r, b0: sink.shapes }
}

impl Painted {
    fn r(&self, x: u32, y: u32) -> [u8; 3] {
        let [r, g, b, a] = self.r.pixel(x, y);
        assert_eq!(a, 255, "R paints over a white background");
        [r, g, b]
    }

    /// The colour B0's triangles give the pixel, over white.
    fn b0(&self, x: u32, y: u32) -> [u8; 3] {
        let p = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
        let mut dst = [255.0_f32; 3];
        for shape in &self.b0 {
            let Shape::Mesh(mesh) = &shape.shape else {
                panic!("a rectangle is a mesh, got {:?}", shape.shape);
            };
            if !shape.clip_rect.contains(p) {
                continue;
            }
            for tri in mesh.indices.as_chunks::<3>().0 {
                let [a, b, c] = tri.map(|i| mesh.vertices[i as usize]);
                let Some(weights) = inside(a.pos, b.pos, c.pos, p) else {
                    continue;
                };
                let src: [f32; 4] = std::array::from_fn(|ch| {
                    [a, b, c]
                        .iter()
                        .zip(weights)
                        .map(|(v, k)| f32::from(v.color.to_array()[ch]) * k)
                        .sum()
                });
                // Premultiplied source over.
                for ch in 0..3 {
                    dst[ch] = src[ch] + dst[ch] * (1.0 - src[3] / 255.0);
                }
            }
        }
        dst.map(|c| c.round().clamp(0.0, 255.0) as u8)
    }

    /// Both backends paint `want` at the pixel. R's arithmetic is Qt's to
    /// the last bit or one off it; B0 rounds each vertex to eight bits
    /// before the triangle is filled, which costs it another step or two.
    #[track_caller]
    fn is(&self, x: u32, y: u32, want: [u8; 3]) {
        let near = |got: [u8; 3], by: u8| got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= by);
        let (r, b0) = (self.r(x, y), self.b0(x, y));
        assert!(near(r, 1), "R at {x}, {y}: {r:?}, wanted {want:?}");
        assert!(near(b0, 3), "B0 at {x}, {y}: {b0:?}, wanted {want:?}");
    }
}

/// The weights of `p` in the triangle, or `None` outside it. A pixel centre
/// on an edge two triangles share belongs to one of them (the top-left rule),
/// or a translucent mesh would be painted twice along its diagonals.
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

fn stops(colours: &[(f32, Rgba)]) -> Vec<GradientStop> {
    colours
        .iter()
        .map(|&(offset, color)| GradientStop { offset, color })
        .collect()
}

fn gradient(from: (f64, f64), to: (f64, f64), colours: &[(f32, Rgba)]) -> Paint {
    Paint::LinearGradient(Arc::new(LinearGradient::new(
        Point::new(from.0, from.1),
        Point::new(to.0, to.1),
        stops(colours),
    )))
}

fn red_to_blue(from: (f64, f64), to: (f64, f64)) -> Paint {
    gradient(from, to, &[(0.0, RED), (1.0, BLUE)])
}

fn rect(w: f64, h: f64, radius: f32, fill: Paint, border: Option<Border>) -> DrawOp {
    DrawOp::Rect {
        rect: Rect::new(0.0, 0.0, w, h),
        radii: [radius; 4],
        fill,
        border,
        aa: true,
    }
}

fn border(width: f32, colour: Rgba) -> Option<Border> {
    Some(Border {
        width,
        paint: Paint::Solid(colour),
    })
}

#[test]
fn vertical_runs_from_the_top_to_the_bottom() {
    let p = paint(20.0, 100.0, 1.0, |l| {
        l.push(rect(
            20.0,
            100.0,
            0.0,
            red_to_blue((0.0, 0.0), (0.0, 100.0)),
            None,
        ));
    });
    // A pixel has the colour at its centre, so neither end is quite reached.
    p.is(10, 0, [0xfe, 0x00, 0x01]);
    p.is(10, 50, [0x7e, 0x00, 0x81]);
    p.is(10, 99, [0x01, 0x00, 0xfe]);
    p.is(0, 50, [0x7e, 0x00, 0x81]);
    p.is(19, 50, [0x7e, 0x00, 0x81]);
}

#[test]
fn horizontal_runs_from_the_left_to_the_right() {
    let p = paint(100.0, 20.0, 1.0, |l| {
        l.push(rect(
            100.0,
            20.0,
            0.0,
            red_to_blue((0.0, 0.0), (100.0, 0.0)),
            None,
        ));
    });
    p.is(0, 10, [0xfe, 0x00, 0x01]);
    p.is(50, 10, [0x7e, 0x00, 0x81]);
    p.is(99, 10, [0x01, 0x00, 0xfe]);
    p.is(50, 0, [0x7e, 0x00, 0x81]);
    p.is(50, 19, [0x7e, 0x00, 0x81]);
}

#[test]
fn a_stop_in_the_middle_is_passed_through() {
    let green = Rgba::rgb(0, 128, 0);
    let p = paint(20.0, 100.0, 1.0, |l| {
        l.push(rect(
            20.0,
            100.0,
            0.0,
            gradient(
                (0.0, 0.0),
                (0.0, 100.0),
                &[(0.0, RED), (0.5, green), (1.0, BLUE)],
            ),
            None,
        ));
    });
    p.is(10, 25, [125, 65, 0]);
    p.is(10, 49, [3, 127, 0]);
    p.is(10, 50, [0, 127, 3]);
    p.is(10, 75, [0, 63, 130]);
}

#[test]
fn the_end_colours_go_on_beyond_the_ends() {
    // What `GradientStop { position: 0.25 }` and `0.75` come to.
    let p = paint(20.0, 100.0, 1.0, |l| {
        l.push(rect(
            20.0,
            100.0,
            0.0,
            red_to_blue((0.0, 25.0), (0.0, 75.0)),
            None,
        ));
    });
    for y in [0, 12, 24] {
        p.is(10, y, [255, 0, 0]);
    }
    p.is(10, 25, [0xfc, 0x00, 0x03]);
    p.is(10, 50, [125, 0, 130]);
    p.is(10, 74, [0x03, 0x00, 0xfc]);
    for y in [75, 87, 99] {
        p.is(10, y, [0, 0, 255]);
    }
}

#[test]
fn the_ends_may_be_off_the_rectangle() {
    // Positions -1 and 2: only the middle third of the line is seen.
    let p = paint(20.0, 100.0, 1.0, |l| {
        l.push(rect(
            20.0,
            100.0,
            0.0,
            red_to_blue((0.0, -100.0), (0.0, 200.0)),
            None,
        ));
    });
    p.is(10, 0, [170, 0, 85]);
    p.is(10, 50, [127, 0, 128]);
    p.is(10, 99, [85, 0, 170]);
}

#[test]
fn a_transparent_stop_fades_without_its_own_colour() {
    // Red to transparent black: blended as written (not premultiplied)
    // the black would darken the middle.
    let p = paint(20.0, 100.0, 1.0, |l| {
        l.push(rect(
            20.0,
            100.0,
            0.0,
            gradient(
                (0.0, 0.0),
                (0.0, 100.0),
                &[(0.0, RED), (1.0, Rgba::TRANSPARENT)],
            ),
            None,
        ));
    });
    p.is(10, 9, [0xff, 0x18, 0x18]);
    p.is(10, 50, [0xff, 0x81, 0x81]);
    p.is(10, 99, [0xff, 0xfe, 0xfe]);
}

#[test]
fn half_transparent_stops_blend_with_what_is_behind() {
    let half = |c: Rgba| c.with_opacity(0.5);
    let p = paint(20.0, 100.0, 1.0, |l| {
        l.push(rect(
            20.0,
            100.0,
            0.0,
            gradient(
                (0.0, 0.0),
                (0.0, 100.0),
                &[(0.0, half(BLUE)), (1.0, half(RED))],
            ),
            None,
        ));
    });
    p.is(10, 0, [128, 128, 254]);
    p.is(10, 50, [192, 128, 191]);
    p.is(10, 99, [254, 128, 128]);
}

#[test]
fn the_opacity_of_the_node_fades_the_gradient() {
    let p = paint(20.0, 100.0, 0.5, |l| {
        l.push(rect(
            20.0,
            100.0,
            0.0,
            red_to_blue((0.0, 0.0), (0.0, 100.0)),
            None,
        ));
    });
    p.is(10, 0, [255, 128, 128]);
    p.is(10, 50, [191, 128, 192]);
    p.is(10, 99, [128, 128, 255]);
}

#[test]
fn round_corners_cut_the_gradient() {
    let p = paint(40.0, 100.0, 1.0, |l| {
        l.push(rect(
            40.0,
            100.0,
            12.0,
            red_to_blue((0.0, 0.0), (0.0, 100.0)),
            None,
        ));
    });
    for (x, y) in [(0, 0), (39, 0), (0, 99), (39, 99), (2, 2), (37, 97)] {
        p.is(x, y, [255, 255, 255]);
    }
    // Between the corners the first row is painted, and the sides lower down.
    p.is(20, 0, [0xfe, 0x00, 0x01]);
    p.is(20, 99, [0x01, 0x00, 0xfe]);
    p.is(0, 50, [0x7e, 0x00, 0x81]);
    p.is(39, 50, [0x7e, 0x00, 0x81]);
    // Where the corner's circle has its centre the gradient is whole.
    p.is(12, 12, [223, 0, 32]);
}

#[test]
fn the_gradient_spans_the_rectangle_and_not_what_the_border_leaves() {
    let p = paint(40.0, 100.0, 1.0, |l| {
        l.push(rect(
            40.0,
            100.0,
            0.0,
            red_to_blue((0.0, 0.0), (0.0, 100.0)),
            border(4.0, Rgba::BLACK),
        ));
    });
    for (x, y) in [(0, 0), (20, 3), (3, 50), (36, 50), (20, 96), (39, 99)] {
        p.is(x, y, [0, 0, 0]);
    }
    // The first row inside the border is the fifth of the gradient, not
    // its first.
    p.is(20, 4, [244, 0, 11]);
    p.is(4, 50, [0x7e, 0x00, 0x81]);
    p.is(35, 50, [0x7e, 0x00, 0x81]);
    p.is(20, 95, [11, 0, 244]);
}

#[test]
fn the_fill_is_not_painted_under_the_border() {
    // Qt paints fill and border side by side: a border one can see through
    // shows what is behind the rectangle, not the rectangle's fill.
    let green = Rgba::from_straight(0.0, 1.0, 0.0, 0.5);
    let fills = [
        red_to_blue((0.0, 0.0), (0.0, 100.0)),
        Paint::Solid(Rgba::rgb(0x7e, 0x00, 0x81)),
    ];
    for fill in fills {
        let p = paint(40.0, 100.0, 1.0, |l| {
            l.push(rect(40.0, 100.0, 0.0, fill, border(4.0, green)));
        });
        for (x, y) in [(0, 0), (20, 0), (20, 3), (3, 50), (36, 50), (20, 96)] {
            p.is(x, y, [0x7f, 0xff, 0x7f]);
        }
        p.is(4, 50, [0x7e, 0x00, 0x81]);
        p.is(35, 50, [0x7e, 0x00, 0x81]);
    }
}

#[test]
fn a_round_border_keeps_the_gradient_inside_it() {
    let green = Rgba::from_straight(0.0, 1.0, 0.0, 0.5);
    let p = paint(60.0, 100.0, 1.0, |l| {
        l.push(rect(
            60.0,
            100.0,
            16.0,
            red_to_blue((0.0, 0.0), (0.0, 100.0)),
            border(6.0, green),
        ));
    });
    p.is(0, 0, [255, 255, 255]);
    p.is(59, 99, [255, 255, 255]);
    for (x, y) in [(30, 0), (30, 4), (2, 50), (57, 50), (30, 97)] {
        p.is(x, y, [0x7f, 0xff, 0x7f]);
    }
    p.is(30, 7, [236, 0, 19]);
    p.is(30, 50, [0x7e, 0x00, 0x81]);
    p.is(8, 50, [0x7e, 0x00, 0x81]);
    p.is(30, 92, [19, 0, 236]);
}

#[test]
fn a_border_that_hides_the_fill_leaves_no_seam() {
    // Fill and border side by side would let the white through where their
    // anti-aliased edges meet on the curve. Under a border that nothing
    // shows through, the fill is painted whole.
    let p = paint(60.0, 60.0, 1.0, |l| {
        l.push(rect(
            60.0,
            60.0,
            20.0,
            Paint::Solid(Rgba::BLACK),
            border(6.0, Rgba::BLACK),
        ));
    });
    // Along the corner's diagonal, from just inside the outer curve inwards.
    for d in 8..30 {
        assert_eq!(p.r(d, d), [0, 0, 0], "R at {d}, {d}");
        assert_eq!(p.b0(d, d), [0, 0, 0], "B0 at {d}, {d}");
    }
}

#[test]
fn a_faded_node_has_no_fill_under_its_border_in_r() {
    // R knows the node's opacity when it paints. B0 builds its triangles
    // before it does and keeps them while the opacity changes, so there a
    // border that hides the fill only at full opacity still has it under it.
    let p = paint(40.0, 100.0, 0.5, |l| {
        l.push(rect(
            40.0,
            100.0,
            0.0,
            red_to_blue((0.0, 0.0), (0.0, 100.0)),
            border(4.0, Rgba::rgb(0, 255, 0)),
        ));
    });
    let near = |x: u32, y: u32, want: [u8; 3]| {
        let got = p.r(x, y);
        assert!(
            got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 1),
            "R at {x}, {y}: {got:?}, wanted {want:?}"
        );
    };
    for (x, y) in [(20, 0), (2, 50), (37, 50), (20, 98)] {
        near(x, y, [0x80, 0xff, 0x80]);
    }
    near(20, 50, [191, 128, 192]);
}

#[test]
fn where_the_fill_is_painted() {
    let whole = Rect::new(0.0, 0.0, 40.0, 30.0);
    let radii = [10.0, 10.0, 2.0, 0.0];
    let solid = |c: Rgba, width: f32| Border {
        width,
        paint: Paint::Solid(c),
    };
    let half = Rgba::BLACK.with_opacity(0.5);
    let inner = Some((Rect::new(4.0, 4.0, 36.0, 26.0), [6.0, 6.0, 0.0, 0.0]));

    assert_eq!(fill_area(whole, radii, None, 1.0), Some((whole, radii)));
    // A border that is not drawn takes nothing away.
    for b in [solid(Rgba::BLACK, 0.0), solid(Rgba::TRANSPARENT, 4.0)] {
        assert_eq!(fill_area(whole, radii, Some(&b), 1.0), Some((whole, radii)));
    }
    assert_eq!(
        fill_area(whole, radii, Some(&solid(Rgba::BLACK, 4.0)), 1.0),
        Some((whole, radii)),
        "nothing shows through this border"
    );
    assert_eq!(fill_area(whole, radii, Some(&solid(half, 4.0)), 1.0), inner);
    assert_eq!(
        fill_area(whole, radii, Some(&solid(Rgba::BLACK, 4.0)), 0.5),
        inner,
        "a faded node's border is seen through"
    );
    let gradient_border = |last: Rgba| Border {
        width: 4.0,
        paint: gradient((0.0, 0.0), (40.0, 0.0), &[(0.0, RED), (1.0, last)]),
    };
    assert_eq!(
        fill_area(whole, radii, Some(&gradient_border(BLUE)), 1.0),
        Some((whole, radii))
    );
    assert_eq!(
        fill_area(whole, radii, Some(&gradient_border(half)), 1.0),
        inner
    );
    // A border as wide as half the rectangle leaves no room.
    assert_eq!(fill_area(whole, radii, Some(&solid(half, 15.0)), 1.0), None);
}
