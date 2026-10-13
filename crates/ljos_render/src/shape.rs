//! Filling a [`DrawOp::Shape`](crate::DrawOp::Shape) with vello_cpu: what R
//! does for every shape and what B0 does for the shapes it keeps as textures.

use vello_cpu::color::{AlphaColor, Srgb};
use vello_cpu::kurbo::{self, Affine, BezPath, PathEl, Stroke, StrokeOpts};
use vello_cpu::peniko::{
    ColorStop, ColorStops, Extend, Fill, Gradient, LinearGradientPosition, RadialGradientPosition,
    SweepGradientPosition,
};
use vello_cpu::{PaintType, RenderContext};

use crate::display_list::{Brush, BrushGradient, FillRule, GradientGeometry, ShapeStyle, Spread};
use crate::rgba::Rgba;

/// How far a flattened curve may lie from the true one, in device pixels.
/// vello_cpu's own quarter of a pixel moves an edge far enough to change a
/// pixel by a quarter of its range, which the comparison with Qt would see.
const TOLERANCE: f64 = 0.02;

fn alpha_color(c: Rgba) -> AlphaColor<Srgb> {
    let [r, g, b, a] = c.to_straight();
    AlphaColor::new([r, g, b, a])
}

/// A brush once the gradients that are one colour, or none, are told apart.
pub(crate) enum Resolved<'a> {
    /// Nothing is painted.
    Nothing,
    /// One colour everywhere.
    Solid(Rgba),
    /// A gradient with at least two stops and room to change in.
    Gradient(&'a BrushGradient),
}

/// Sort out the brushes that need no gradient. Both backends go through
/// here, so they agree on the edge cases.
pub(crate) fn resolve(brush: &Brush) -> Resolved<'_> {
    let g = match brush {
        Brush::Solid(c) if c.is_transparent() => return Resolved::Nothing,
        Brush::Solid(c) => return Resolved::Solid(*c),
        Brush::Gradient(g) => g,
    };
    let (Some(first), Some(last)) = (g.stops.first(), g.stops.last()) else {
        return Resolved::Nothing;
    };
    if g.stops.iter().all(|s| s.color.is_transparent()) {
        return Resolved::Nothing;
    }
    if g.stops.len() == 1 {
        return Resolved::Solid(first.color);
    }
    match g.geometry {
        // No direction: everything lies at the start.
        GradientGeometry::Linear { start, end } if (end - start).hypot2() <= f64::EPSILON => {
            Resolved::Solid(first.color)
        }
        // Two circles that are one circle have nothing between them.
        GradientGeometry::Radial {
            focal,
            focal_radius,
            center,
            radius,
        } if (radius.max(0.0) - focal_radius.max(0.0)).abs() <= 1e-9
            && (center - focal).hypot2() <= 1e-18 =>
        {
            Resolved::Solid(last.color)
        }
        _ => Resolved::Gradient(g),
    }
}

/// What `brush` paints with, and the transform from the gradient's own
/// space to the node's when it has one. `None` paints nothing.
pub(crate) fn paint_of(brush: &Brush) -> Option<(PaintType, Option<Affine>)> {
    let g = match resolve(brush) {
        Resolved::Nothing => return None,
        Resolved::Solid(c) => return Some((PaintType::Solid(alpha_color(c)), None)),
        Resolved::Gradient(g) => g,
    };
    let (kind, transform) = match g.geometry {
        GradientGeometry::Linear { start, end } => {
            (LinearGradientPosition { start, end }.into(), None)
        }
        GradientGeometry::Radial {
            focal,
            focal_radius,
            center,
            radius,
        } => {
            let (r0, r1) = (focal_radius.max(0.0), radius.max(0.0));
            (
                RadialGradientPosition {
                    start_center: focal,
                    start_radius: r0 as f32,
                    end_center: center,
                    end_radius: r1 as f32,
                }
                .into(),
                None,
            )
        }
        GradientGeometry::Conical { center, angle } => (
            SweepGradientPosition {
                center: kurbo::Point::ZERO,
                start_angle: 0.0,
                end_angle: std::f32::consts::TAU,
            }
            .into(),
            // The sweep turns with the y axis, clockwise on the screen;
            // flipping y turns it the other way.
            Some(
                Affine::translate(center.to_vec2())
                    * Affine::rotate(-angle.to_radians())
                    * Affine::scale_non_uniform(1.0, -1.0),
            ),
        ),
    };
    let stops: Vec<ColorStop> = g
        .stops
        .iter()
        .map(|s| ColorStop::from((s.offset.clamp(0.0, 1.0), alpha_color(s.color))))
        .collect();
    let extend = match (g.geometry, g.spread) {
        (GradientGeometry::Conical { .. }, _) | (_, Spread::Pad) => Extend::Pad,
        (_, Spread::Reflect) => Extend::Reflect,
        (_, Spread::Repeat) => Extend::Repeat,
    };
    Some((
        PaintType::Gradient(Gradient {
            kind,
            stops: ColorStops::from(stops.as_slice()),
            extend,
            ..Default::default()
        }),
        transform,
    ))
}

/// `path` as straight lines, within [`TOLERANCE`] at `scale` device pixels
/// per unit.
fn flattened(path: &BezPath, scale: f64) -> BezPath {
    if path.elements().iter().all(|el| {
        matches!(
            el,
            PathEl::MoveTo(_) | PathEl::LineTo(_) | PathEl::ClosePath
        )
    }) {
        return path.clone();
    }
    let mut out = BezPath::new();
    kurbo::flatten(path.elements().iter().copied(), TOLERANCE / scale, |el| {
        out.push(el);
    });
    out
}

/// The outline of `path` stroked with `stroke`, to be filled non-zero.
pub(crate) fn stroke_outline(path: &BezPath, stroke: &Stroke, tolerance: f64) -> BezPath {
    // A pattern that never advances would never end; such a line is solid.
    let dashes = &stroke.dash_pattern;
    let usable = dashes.iter().all(|d| d.is_finite() && *d >= 0.0)
        && dashes.iter().sum::<f64>() > 1e-9
        && stroke.dash_offset.is_finite();
    let solid;
    let stroke = if dashes.is_empty() || usable {
        stroke
    } else {
        solid = Stroke {
            dash_pattern: Default::default(),
            dash_offset: 0.0,
            ..stroke.clone()
        };
        &solid
    };
    kurbo::stroke(
        path.elements().iter().copied(),
        stroke,
        &StrokeOpts::default(),
        tolerance,
    )
}

/// True when `stroke` draws something.
pub(crate) fn stroke_is_visible(stroke: &Stroke) -> bool {
    stroke.width > 0.0 && stroke.width.is_finite()
}

/// Draw one shape under the context's current transform. `scale` is how many
/// device pixels one unit of the path is, which sets how finely curves are
/// cut into lines.
pub(crate) fn draw_shape(
    ctx: &mut RenderContext,
    path: &BezPath,
    style: &ShapeStyle,
    brush: &Brush,
    opacity: f32,
    scale: f64,
) {
    if opacity <= 0.0 || brush.is_invisible() || path.elements().is_empty() {
        return;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let Some((paint, paint_transform)) = paint_of(&brush.with_opacity(opacity)) else {
        return;
    };
    let (outline, rule) = match style {
        ShapeStyle::Fill(rule) => (flattened(path, scale), *rule),
        ShapeStyle::Stroke(stroke) => {
            if !stroke_is_visible(stroke) {
                return;
            }
            let tolerance = TOLERANCE / scale;
            (
                flattened(&stroke_outline(path, stroke, tolerance), scale),
                FillRule::NonZero,
            )
        }
    };
    ctx.set_paint(paint);
    if let Some(t) = paint_transform {
        ctx.set_paint_transform(t);
    }
    ctx.set_fill_rule(match rule {
        FillRule::NonZero => Fill::NonZero,
        FillRule::EvenOdd => Fill::EvenOdd,
    });
    ctx.fill_path(&outline);
    ctx.set_fill_rule(Fill::NonZero);
    if paint_transform.is_some() {
        ctx.reset_paint_transform();
    }
}

/// The colour a gradient is drawn in where only one colour can be: the
/// middle of its range.
#[cfg(feature = "b0")]
pub(crate) fn single_color(g: &BrushGradient) -> Rgba {
    g.color_at(0.5)
}
