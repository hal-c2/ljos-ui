//! One frame of a composition as filled and stroked paths.
//!
//! Everything ends as [`Op`]s, the shapes the display list draws for
//! QtQuick.Shapes and SVG, so a Lottie frame is tessellated, anti-aliased
//! and given its gradients by the same code on every backend.
//!
//! The order follows the format: the first layer of a list is on top, and
//! so is the first item of a shape group. A fill or stroke paints every
//! path before it in its group, those of nested groups included, each
//! group's own transform applied; a trim cuts the paths before it.

use std::sync::Arc;

use ljos::item::kurbo::{
    self, Affine, BezPath, Cap, Ellipse, Join, ParamCurve, ParamCurveArclen, PathSeg, Point, Rect,
    RoundedRect, Shape as _, Vec2,
};
use ljos::item::{
    Brush, BrushGradient, FillRule, GradientGeometry, GradientStop, Rgba, ShapeStyle, Spread,
};

use crate::model::{
    Bez, Composition, DashKind, Gradient, Layer, LayerKind, Paint, Position, Prop, Shape,
    StrokeStyle, Transform,
};

/// How deep compositions may hold each other: a file whose assets hold
/// themselves stops here instead of overflowing the stack.
const MAX_DEPTH: usize = 16;

/// Curve flattening tolerance for the built-in shapes, in composition
/// pixels.
const TOLERANCE: f64 = 0.05;

/// One shape to draw, in the coordinates the frame was asked for.
#[derive(Clone, Debug)]
pub struct Op {
    pub path: Arc<BezPath>,
    pub style: ShapeStyle,
    pub brush: Brush,
}

/// The shapes of `comp` at `frame`, mapped by `view` (composition to
/// item coordinates), bottom first.
pub fn render(comp: &Composition, frame: f64, view: Affine) -> Vec<Op> {
    // The last frame is part of the run, but a layer's out point is
    // exclusive: draw the end just before it, where the layers still are.
    let t = if frame >= comp.out_point {
        comp.out_point - 1e-3
    } else {
        frame
    };
    let mut out = Vec::new();
    layers(comp, &comp.layers, t, view, 1.0, 0, &mut out);
    out
}

fn layers(
    comp: &Composition,
    list: &[Layer],
    t: f64,
    m: Affine,
    alpha: f64,
    depth: usize,
    out: &mut Vec<Op>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    for layer in list.iter().rev() {
        if layer.hidden || t < layer.in_point || t >= layer.out_point {
            continue;
        }
        let local = local_time(layer, t);
        let a = alpha * (layer.transform.opacity.num(local, 0, 100.0) / 100.0);
        if a <= 0.0 {
            continue;
        }
        let world = m * layer_matrix(list, layer, t, 0);
        match &layer.kind {
            LayerKind::Shapes(shapes) => group(shapes, local, world, a, out),
            LayerKind::Precomp { id, remap } => {
                let Some(inner) = comp.assets.get(id) else {
                    continue;
                };
                let inner_t = remap
                    .as_ref()
                    .map_or(local, |r| r.num(local, 0, 0.0) * comp.frame_rate);
                layers(comp, inner, inner_t, world, a, depth + 1, out);
            }
            LayerKind::Solid {
                color,
                width,
                height,
            } => {
                let mut path = Rect::new(0.0, 0.0, *width, *height).to_path(TOLERANCE);
                path.apply_affine(world);
                let [r, g, b, _] = *color;
                out.push(Op {
                    path: Arc::new(path),
                    style: ShapeStyle::Fill(FillRule::NonZero),
                    brush: Brush::Solid(Rgba::from_straight(
                        r as f32, g as f32, b as f32, a as f32,
                    )),
                });
            }
            LayerKind::Empty => {}
        }
    }
}

/// The time inside a layer: its keyframes count from its start time, at
/// its stretch.
fn local_time(layer: &Layer, t: f64) -> f64 {
    (t - layer.start_time) / layer.stretch
}

/// A layer's transform with its parents', each at its own time. Parents
/// lend their transform only, never their opacity.
fn layer_matrix(list: &[Layer], layer: &Layer, t: f64, depth: usize) -> Affine {
    let own = matrix(&layer.transform, local_time(layer, t));
    let parent = layer
        .parent
        .filter(|_| depth < MAX_DEPTH)
        .and_then(|p| list.iter().find(|l| l.index == Some(p)));
    match parent {
        Some(p) => layer_matrix(list, p, t, depth + 1) * own,
        None => own,
    }
}

fn point(p: &Prop, t: f64) -> Point {
    p.point(t)
}

/// Position, then rotation, skew and scale about the anchor.
fn matrix(tr: &Transform, t: f64) -> Affine {
    let anchor = point(&tr.anchor, t);
    let position = match &tr.position {
        Position::Joined(p) => point(p, t),
        Position::Split(x, y) => Point::new(x.num(t, 0, 0.0), y.num(t, 0, 0.0)),
    };
    let scale = tr.scale.at(t);
    let sx = scale.first().copied().unwrap_or(100.0) / 100.0;
    let sy = scale.get(1).copied().unwrap_or(sx * 100.0) / 100.0;
    let rotation = tr.rotation.num(t, 0, 0.0).to_radians();
    let skew = tr.skew.num(t, 0, 0.0);
    let mut m = Affine::translate(position.to_vec2()) * Affine::rotate(rotation);
    if skew != 0.0 {
        let axis = tr.skew_axis.num(t, 0, 0.0).to_radians();
        m = m
            * Affine::rotate(axis)
            * Affine::skew((-skew.to_radians()).tan(), 0.0)
            * Affine::rotate(-axis);
    }
    m * Affine::scale_non_uniform(sx, sy) * Affine::translate(-anchor.to_vec2())
}

/// The transform and opacity a group's `tr` item gives everything in it.
fn group_transform(items: &[Shape], t: f64) -> (Affine, f64) {
    items
        .iter()
        .find_map(|s| match s {
            Shape::Transform(tr) => Some((matrix(tr, t), tr.opacity.num(t, 0, 100.0) / 100.0)),
            _ => None,
        })
        .unwrap_or((Affine::IDENTITY, 1.0))
}

fn group(items: &[Shape], t: f64, m: Affine, alpha: f64, out: &mut Vec<Op>) {
    let (own, opacity) = group_transform(items, t);
    let (m, alpha) = (m * own, alpha * opacity);
    if alpha <= 0.0 {
        return;
    }
    // Bottom first: the last item of a group is the lowest.
    for (j, item) in items.iter().enumerate().rev() {
        match item {
            Shape::Group(inner) => group(inner, t, m, alpha, out),
            Shape::Fill {
                paint,
                opacity,
                even_odd,
            } => {
                let paths = paths(&items[..j], t, m);
                let Some(path) = merged(paths) else { continue };
                let a = alpha * opacity.num(t, 0, 100.0) / 100.0;
                let Some(brush) = brush(paint, t, m, a) else {
                    continue;
                };
                let rule = if *even_odd {
                    FillRule::EvenOdd
                } else {
                    FillRule::NonZero
                };
                out.push(Op {
                    path: Arc::new(path),
                    style: ShapeStyle::Fill(rule),
                    brush,
                });
            }
            Shape::Stroke {
                paint,
                opacity,
                style,
            } => {
                let paths = paths(&items[..j], t, m);
                let Some(path) = merged(paths) else { continue };
                let a = alpha * opacity.num(t, 0, 100.0) / 100.0;
                let Some(brush) = brush(paint, t, m, a) else {
                    continue;
                };
                let Some(stroke) = stroke(style, t, m) else {
                    continue;
                };
                out.push(Op {
                    path: Arc::new(path),
                    style: ShapeStyle::Stroke(Arc::new(stroke)),
                    brush,
                });
            }
            _ => {}
        }
    }
}

/// One path of all of `paths`, or `None` when nothing would show.
fn merged(paths: Vec<BezPath>) -> Option<BezPath> {
    let mut it = paths.into_iter().filter(|p| !p.is_empty());
    let mut first = it.next()?;
    for p in it {
        first.extend(p);
    }
    Some(first)
}

/// The paths `items` make, in the coordinates `m` maps to, trims applied.
fn paths(items: &[Shape], t: f64, m: Affine) -> Vec<BezPath> {
    let mut out: Vec<BezPath> = Vec::new();
    for item in items {
        match item {
            Shape::Group(inner) => {
                let (own, _) = group_transform(inner, t);
                out.extend(paths(inner, t, m * own));
            }
            Shape::Trim {
                start,
                end,
                offset,
                sequential,
            } => trim(
                &mut out,
                start.num(t, 0, 0.0) / 100.0,
                end.num(t, 0, 100.0) / 100.0,
                offset.num(t, 0, 0.0) / 360.0,
                *sequential,
            ),
            s if s.is_geometry() => {
                if let Some(mut p) = geometry(s, t) {
                    p.apply_affine(m);
                    out.push(p);
                }
            }
            _ => {}
        }
    }
    out
}

fn vec2(p: &Prop, t: f64) -> Vec2 {
    let v = p.at(t);
    Vec2::new(
        v.first().copied().unwrap_or(0.0),
        v.get(1).copied().unwrap_or(0.0),
    )
}

fn geometry(shape: &Shape, t: f64) -> Option<BezPath> {
    match shape {
        Shape::Rect {
            position,
            size,
            roundness,
        } => {
            let size = vec2(size, t);
            let rect = Rect::from_center_size(point(position, t), size.to_size());
            let r = roundness
                .num(t, 0, 0.0)
                .min(size.x.abs().min(size.y.abs()) / 2.0);
            Some(if r > 0.0 {
                RoundedRect::from_rect(rect, r).to_path(TOLERANCE)
            } else {
                rect.to_path(TOLERANCE)
            })
        }
        Shape::Ellipse { position, size } => {
            let size = vec2(size, t);
            let rect = Rect::from_center_size(point(position, t), size.to_size());
            Some(Ellipse::from_rect(rect).to_path(TOLERANCE))
        }
        Shape::Path(bez) => Some(bez_path(&bez.at(t))),
        Shape::Star {
            star,
            points,
            position,
            rotation,
            outer,
            inner,
        } => {
            let n = points.num(t, 0, 5.0).round().max(2.0) as usize;
            let centre = point(position, t);
            let (ro, ri) = (outer.num(t, 0, 0.0), inner.num(t, 0, 0.0));
            let start = (rotation.num(t, 0, 0.0) - 90.0).to_radians();
            let corners = if *star { 2 * n } else { n };
            let step = std::f64::consts::TAU / corners as f64;
            let mut path = BezPath::new();
            for k in 0..corners {
                let r = if *star && k % 2 == 1 { ri } else { ro };
                let a = start + step * k as f64;
                let p = centre + Vec2::new(a.cos(), a.sin()) * r;
                if k == 0 {
                    path.move_to(p);
                } else {
                    path.line_to(p);
                }
            }
            path.close_path();
            Some(path)
        }
        _ => None,
    }
}

fn bez_path(b: &Bez) -> BezPath {
    let mut path = BezPath::new();
    let v = &b.vertices;
    let Some(first) = v.first() else {
        return path;
    };
    let pt = |p: [f64; 2]| Point::new(p[0], p[1]);
    let tangent = |list: &[[f64; 2]], k: usize| list.get(k).copied().unwrap_or([0.0, 0.0]);
    let seg = |path: &mut BezPath, a: usize, b_: usize| {
        let (o, i) = (tangent(&b.out_tangents, a), tangent(&b.in_tangents, b_));
        let (pa, pb) = (pt(v[a]), pt(v[b_]));
        if o == [0.0, 0.0] && i == [0.0, 0.0] {
            path.line_to(pb);
        } else {
            path.curve_to(pa + Vec2::new(o[0], o[1]), pb + Vec2::new(i[0], i[1]), pb);
        }
    };
    path.move_to(pt(*first));
    for k in 1..v.len() {
        seg(&mut path, k - 1, k);
    }
    if b.closed && v.len() > 1 {
        seg(&mut path, v.len() - 1, 0);
        path.close_path();
    }
    path
}

/// A colour of the file: components 0 to 1, or 0 to 255 in some files.
fn color(v: &[f64]) -> [f64; 3] {
    let c = |i: usize| v.get(i).copied().unwrap_or(0.0);
    let (r, g, b) = (c(0), c(1), c(2));
    if r > 1.0 || g > 1.0 || b > 1.0 {
        [r / 255.0, g / 255.0, b / 255.0]
    } else {
        [r, g, b]
    }
}

fn brush(paint: &Paint, t: f64, m: Affine, alpha: f64) -> Option<Brush> {
    if alpha <= 0.0 {
        return None;
    }
    match paint {
        Paint::Color(c) => {
            let v = c.at(t);
            let [r, g, b] = color(&v);
            let a = v.get(3).copied().unwrap_or(1.0).min(1.0) * alpha;
            Some(Brush::Solid(Rgba::from_straight(
                r as f32, g as f32, b as f32, a as f32,
            )))
        }
        Paint::Gradient(g) => Some(Brush::Gradient(Arc::new(gradient(g, t, m, alpha)))),
    }
}

fn gradient(g: &Gradient, t: f64, m: Affine, alpha: f64) -> BrushGradient {
    let values = g.colors.at(t);
    let count = g.count.min(values.len() / 4);
    let (colours, opacities) = values.split_at((count * 4).min(values.len()));
    let opacity_stops = opacities.as_chunks::<2>().0;
    let opacity_at = |offset: f64| -> f64 {
        match opacity_stops {
            [] => 1.0,
            [only] => only[1],
            many => {
                let i = many.partition_point(|s| s[0] <= offset);
                if i == 0 {
                    many[0][1]
                } else if i == many.len() {
                    many[i - 1][1]
                } else {
                    let (a, b) = (many[i - 1], many[i]);
                    let u = if b[0] > a[0] {
                        (offset - a[0]) / (b[0] - a[0])
                    } else {
                        0.0
                    };
                    a[1] + (b[1] - a[1]) * u
                }
            }
        }
    };
    let stops = colours
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| {
            let a = opacity_at(c[0]) * alpha;
            GradientStop {
                offset: c[0].clamp(0.0, 1.0) as f32,
                color: Rgba::from_straight(c[1] as f32, c[2] as f32, c[3] as f32, a as f32),
            }
        })
        .collect();
    let start = point(&g.start, t);
    let end = point(&g.end, t);
    let geometry = if g.radial {
        let radius = (end - start).hypot();
        let towards = (g.highlight.num(t, 0, 0.0) / 100.0).clamp(-0.99, 0.99) * radius;
        let angle = g.angle.num(t, 0, 0.0).to_radians() + (end - start).atan2();
        let focal = start + Vec2::new(angle.cos(), angle.sin()) * towards;
        GradientGeometry::Radial {
            focal: m * focal,
            focal_radius: 0.0,
            center: m * start,
            radius: radius * mean_scale(m),
        }
    } else {
        GradientGeometry::Linear {
            start: m * start,
            end: m * end,
        }
    };
    BrushGradient::new(geometry, stops, Spread::Pad)
}

/// How much `m` scales lengths, on average over directions.
fn mean_scale(m: Affine) -> f64 {
    m.determinant().abs().sqrt()
}

fn stroke(style: &StrokeStyle, t: f64, m: Affine) -> Option<kurbo::Stroke> {
    let scale = mean_scale(m);
    let width = style.width.num(t, 0, 0.0) * scale;
    if width <= 0.0 {
        return None;
    }
    let cap = match style.cap {
        1 => Cap::Butt,
        3 => Cap::Square,
        _ => Cap::Round,
    };
    let join = match style.join {
        1 => Join::Miter,
        3 => Join::Bevel,
        _ => Join::Round,
    };
    let mut stroke = kurbo::Stroke::new(width)
        .with_caps(cap)
        .with_join(join)
        .with_miter_limit(style.miter);
    let mut offset = 0.0;
    let mut pattern = Vec::new();
    for (kind, v) in &style.dashes {
        let len = v.num(t, 0, 0.0) * scale;
        match kind {
            DashKind::Offset => offset = len,
            DashKind::Dash | DashKind::Gap => pattern.push(len.max(0.0)),
        }
    }
    if pattern.len() % 2 == 1 {
        let again = pattern.clone();
        pattern.extend(again);
    }
    if pattern.iter().sum::<f64>() > 0.0 {
        stroke = stroke.with_dashes(offset, pattern);
    }
    Some(stroke)
}

/// Accuracy of the lengths a trim measures, in pixels.
const ARCLEN_ACCURACY: f64 = 1e-2;

fn length(path: &BezPath) -> f64 {
    path.segments().map(|s| s.arclen(ARCLEN_ACCURACY)).sum()
}

/// Cuts `paths` down to the stretch from `start` to `end` (fractions of
/// their length) moved on by `offset`: each path on its own, or all as
/// one long path when `sequential`.
fn trim(paths: &mut Vec<BezPath>, start: f64, end: f64, offset: f64, sequential: bool) {
    let (start, end) = if start > end {
        (end, start)
    } else {
        (start, end)
    };
    if end - start >= 1.0 {
        return;
    }
    if end - start <= 0.0 {
        paths.clear();
        return;
    }
    let shift = (start + offset).floor();
    let (s, e) = (start + offset - shift, end + offset - shift);
    let ranges: Vec<(f64, f64)> = if e <= 1.0 {
        vec![(s, e)]
    } else {
        vec![(s, 1.0), (0.0, e - 1.0)]
    };
    if sequential {
        let lengths: Vec<f64> = paths.iter().map(length).collect();
        let total: f64 = lengths.iter().sum();
        let mut at = 0.0;
        for (path, len) in paths.iter_mut().zip(lengths) {
            let mut cut = BezPath::new();
            for (a, b) in &ranges {
                let (from, to) = ((a * total - at).max(0.0), (b * total - at).min(len));
                if to > from {
                    cut.extend(sub_path(path, from, to));
                }
            }
            at += len;
            *path = cut;
        }
    } else {
        for path in paths.iter_mut() {
            let len = length(path);
            let mut cut = BezPath::new();
            for (a, b) in &ranges {
                cut.extend(sub_path(path, a * len, b * len));
            }
            *path = cut;
        }
    }
    paths.retain(|p| !p.is_empty());
}

/// The part of `path` from length `from` to length `to`.
fn sub_path(path: &BezPath, from: f64, to: f64) -> BezPath {
    let mut out = BezPath::new();
    let mut at = 0.0;
    let mut last: Option<Point> = None;
    for seg in path.segments() {
        let len = seg.arclen(ARCLEN_ACCURACY);
        let (a, b) = (from.max(at), to.min(at + len));
        if b > a && len > 0.0 {
            let t0 = seg.inv_arclen(a - at, ARCLEN_ACCURACY);
            let t1 = seg.inv_arclen(b - at, ARCLEN_ACCURACY);
            let piece = seg.subsegment(t0..t1);
            let p0 = piece.start();
            if last.is_none_or(|l| (l - p0).hypot() > 1e-6) {
                out.move_to(p0);
            }
            match piece {
                PathSeg::Line(l) => out.line_to(l.p1),
                PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
                PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
            }
            last = Some(piece.end());
        }
        at += len;
        if at >= to {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ljos::item::kurbo::PathEl;

    fn elements(path: &BezPath) -> Vec<PathEl> {
        path.elements().to_vec()
    }
    use crate::model::parse;

    fn one_layer(shapes: &str) -> Composition {
        let json = format!(
            r#"{{"fr":10,"ip":0,"op":10,"w":100,"h":100,"layers":[{{"ty":4,"ip":0,"op":10,"st":0,"ks":{{}},"shapes":[{shapes}]}}]}}"#
        );
        parse(json.as_bytes()).unwrap()
    }

    #[test]
    fn a_fill_paints_the_paths_before_it() {
        let c = one_layer(
            r#"{"ty":"rc","p":{"a":0,"k":[50,50]},"s":{"a":0,"k":[20,20]},"r":{"a":0,"k":0}},
               {"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100}}"#,
        );
        let ops = render(&c, 0.0, Affine::IDENTITY);
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0].path.bounding_box(),
            Rect::new(40.0, 40.0, 60.0, 60.0)
        );
        assert!(matches!(ops[0].style, ShapeStyle::Fill(FillRule::NonZero)));
    }

    #[test]
    fn the_first_item_of_a_group_is_drawn_last() {
        let c = one_layer(
            r#"{"ty":"gr","it":[{"ty":"rc","p":{"a":0,"k":[10,10]},"s":{"a":0,"k":[4,4]}},{"ty":"fl","c":{"a":0,"k":[1,0,0,1]}}]},
               {"ty":"gr","it":[{"ty":"rc","p":{"a":0,"k":[20,20]},"s":{"a":0,"k":[4,4]}},{"ty":"fl","c":{"a":0,"k":[0,0,1,1]}}]}"#,
        );
        let ops = render(&c, 0.0, Affine::IDENTITY);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[1].path.bounding_box().center(), Point::new(10.0, 10.0));
    }

    #[test]
    fn a_style_after_a_group_paints_its_paths_too() {
        let c = one_layer(
            r#"{"ty":"gr","it":[{"ty":"rc","p":{"a":0,"k":[10,10]},"s":{"a":0,"k":[4,4]}},
                 {"ty":"tr","p":{"a":0,"k":[5,0]}}]},
               {"ty":"fl","c":{"a":0,"k":[1,0,0,1]}}"#,
        );
        let ops = render(&c, 0.0, Affine::IDENTITY);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].path.bounding_box().center(), Point::new(15.0, 10.0));
    }

    #[test]
    fn trimming_keeps_the_stretch_asked_for() {
        let mut paths = vec![{
            let mut p = BezPath::new();
            p.move_to((10.0, 50.0));
            p.line_to((90.0, 50.0));
            p
        }];
        trim(&mut paths, 0.0, 0.5, 0.0, false);
        let b = paths[0].bounding_box();
        assert!(
            (b.x0 - 10.0).abs() < 1e-6 && (b.x1 - 50.0).abs() < 1e-3,
            "{b:?}"
        );
        let mut wrapped = vec![{
            let mut p = BezPath::new();
            p.move_to((0.0, 0.0));
            p.line_to((100.0, 0.0));
            p
        }];
        trim(&mut wrapped, 0.0, 0.2, 0.9, false);
        assert_eq!(
            elements(&wrapped[0])
                .iter()
                .filter(|e| matches!(e, PathEl::MoveTo(_)))
                .count(),
            2
        );
    }

    #[test]
    fn a_layer_outside_its_time_is_not_drawn() {
        let json = br##"{"fr":10,"ip":0,"op":20,"w":100,"h":100,"layers":[
            {"ty":1,"sc":"#ff0000","sw":10,"sh":10,"ip":10,"op":20,"st":0,"ks":{}}]}"##;
        let c = parse(json).unwrap();
        assert!(render(&c, 5.0, Affine::IDENTITY).is_empty());
        assert_eq!(render(&c, 15.0, Affine::IDENTITY).len(), 1);
        // The last frame shows what is there just before the end.
        assert_eq!(render(&c, 20.0, Affine::IDENTITY).len(), 1);
    }

    #[test]
    fn a_parent_moves_its_child() {
        let json = br##"{"fr":10,"ip":0,"op":10,"w":100,"h":100,"layers":[
            {"ty":1,"sc":"#ffffff","sw":10,"sh":10,"ind":2,"parent":1,"ip":0,"op":10,"st":0,"ks":{}},
            {"ty":3,"ind":1,"ip":0,"op":10,"st":0,"ks":{"p":{"a":0,"k":[30,0]}}}]}"##;
        let c = parse(json).unwrap();
        let ops = render(&c, 0.0, Affine::IDENTITY);
        assert_eq!(ops[0].path.bounding_box(), Rect::new(30.0, 0.0, 40.0, 10.0));
    }
}
