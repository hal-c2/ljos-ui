//! `DrawOp::scaled` and `DisplayListBuilder::append_scaled`: an op made
//! larger about the origin, as a scaled painter draws it.

use std::sync::Arc;

use ljos_render::kurbo::{BezPath, Point, Rect, Stroke};
use ljos_render::{
    Border, Brush, BrushGradient, DisplayListBuilder, DrawOp, FillRule, GradientGeometry,
    GradientStop, Paint, PathStyle, Rgba, ShapeStyle, Spread,
};

const RED: Rgba = Rgba::from_premul(1.0, 0.0, 0.0, 1.0);

fn square() -> Arc<BezPath> {
    let mut p = BezPath::new();
    p.move_to((1.0, 2.0));
    p.line_to((5.0, 2.0));
    p.line_to((5.0, 6.0));
    p.close_path();
    Arc::new(p)
}

#[test]
fn a_rectangle_grows_with_its_corners_and_border() {
    let op = DrawOp::Rect {
        rect: Rect::new(1.0, 2.0, 3.0, 4.0),
        radii: [1.0, 2.0, 3.0, 4.0],
        fill: Paint::Solid(RED),
        border: Some(Border {
            width: 0.5,
            paint: Paint::Solid(RED),
        }),
        aa: true,
    };
    let DrawOp::Rect {
        rect,
        radii,
        border,
        ..
    } = op.scaled(2.0)
    else {
        panic!("still a rectangle");
    };
    assert_eq!(rect, Rect::new(2.0, 4.0, 6.0, 8.0));
    assert_eq!(radii, [2.0, 4.0, 6.0, 8.0]);
    assert_eq!(border.map(|b| b.width), Some(1.0));
}

#[test]
fn a_path_and_its_stroke_grow() {
    let op = DrawOp::Path {
        path: square(),
        style: PathStyle::Stroke { width: 1.5 },
        paint: Paint::Solid(RED),
    };
    let scaled = op.scaled(3.0);
    assert_eq!(
        scaled.bounds(),
        Rect::new(3.0 - 2.25, 6.0 - 2.25, 15.0 + 2.25, 18.0 + 2.25)
    );
}

#[test]
fn a_shape_scales_its_stroke_dashes_and_gradient() {
    let stroke = Stroke::new(2.0).with_dashes(1.0, [3.0, 4.0]);
    let op = DrawOp::Shape {
        path: square(),
        style: ShapeStyle::Stroke(Arc::new(stroke)),
        brush: Brush::Gradient(Arc::new(BrushGradient::new(
            GradientGeometry::Radial {
                focal: Point::new(1.0, 1.0),
                focal_radius: 0.5,
                center: Point::new(2.0, 3.0),
                radius: 4.0,
            },
            vec![GradientStop {
                offset: 0.0,
                color: RED,
            }],
            Spread::Pad,
        ))),
    };
    let DrawOp::Shape {
        path, style, brush, ..
    } = op.scaled(2.0)
    else {
        panic!("still a shape");
    };
    assert_eq!(path.elements()[1].end_point(), Some(Point::new(10.0, 4.0)));
    let ShapeStyle::Stroke(k) = style else {
        panic!("still a stroke");
    };
    assert_eq!(k.width, 4.0);
    assert_eq!(k.dash_offset, 2.0);
    assert_eq!(k.dash_pattern.as_slice(), &[6.0, 8.0]);
    let Brush::Gradient(g) = brush else {
        panic!("still a gradient");
    };
    assert_eq!(
        g.geometry,
        GradientGeometry::Radial {
            focal: Point::new(2.0, 2.0),
            focal_radius: 1.0,
            center: Point::new(4.0, 6.0),
            radius: 8.0,
        }
    );
}

#[test]
fn a_fill_keeps_its_rule() {
    let op = DrawOp::Shape {
        path: square(),
        style: ShapeStyle::Fill(FillRule::EvenOdd),
        brush: Brush::Solid(RED),
    };
    let DrawOp::Shape { style, .. } = op.scaled(0.5) else {
        panic!("still a shape");
    };
    assert_eq!(style, ShapeStyle::Fill(FillRule::EvenOdd));
}

#[test]
fn append_scaled_scales_every_op_and_the_bounds() {
    let mut inner = DisplayListBuilder::new();
    inner.fill_rect(Rect::new(0.0, 0.0, 10.0, 5.0), RED);
    inner.shadow(Rect::new(0.0, 0.0, 10.0, 5.0), [0.0; 4], 2.0, 1.0, RED);
    let mut out = DisplayListBuilder::new();
    out.append_scaled(inner, 2.0);
    let list = out.finish();
    assert_eq!(list.ops().len(), 2);
    assert_eq!(list.bounds(), Rect::new(-6.0, -6.0, 26.0, 16.0));
}

#[test]
fn append_scaled_by_one_keeps_the_ops() {
    let mut inner = DisplayListBuilder::new();
    inner.fill_rect(Rect::new(1.0, 1.0, 2.0, 2.0), RED);
    let mut out = DisplayListBuilder::new();
    out.append_scaled(inner, 1.0);
    assert_eq!(out.finish().bounds(), Rect::new(1.0, 1.0, 2.0, 2.0));
}
