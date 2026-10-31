//! `ExtrudedTextGeometry`: the outlines of a text, filled front and back
//! and joined by walls along the z axis.
//!
//! The text is laid out by the text stack every `Text` uses, so it has the
//! same fonts, shaping and line breaks; the outlines come from the fonts
//! themselves, flattened as finely as the font's pixel size asks (a larger
//! font is smoother text, not larger text) and filled by the tessellator
//! the 2D painter fills shapes with.

use ljos::item::kurbo::{self, BezPath, PathEl};
use ljos::item::{self, Font, GlyphRun, LayoutParams};
use ljos::prelude::*;
use lyon_tessellation::math::point;
use lyon_tessellation::path::Path as LyonPath;
use lyon_tessellation::{BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers};
use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};

use super::geometry::{Generated, NULL, changed, complete, status_enums};
use super::mesh::Built;
use crate::node::Geometry;

/// How far a flattened curve may stray from the outline, in pixels of the
/// font's size.
const TOLERANCE_PX: f64 = 0.2;

/// Walls whose directions differ by less than this share a normal where
/// they meet, so a curve is shaded round and a corner stays sharp.
const SMOOTH_COS: f64 = 0.866;

/// `ExtrudedTextGeometry` (QtQuick3D.Helpers).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "ExtrudedTextGeometry",
    extends = Geometry,
    enums = status_enums
)]
pub struct ExtrudedTextGeometry {
    #[qml(base)]
    geometry: Geometry,
    #[qml(skip)]
    complete: bool,
    #[qml(notify)]
    asynchronous: bool,
    #[qml(notify, readonly, enumeration = "Status")]
    status: i32,
    #[qml(notify, change = changed::<ExtrudedTextGeometry>)]
    text: String,
    #[qml(notify, change = changed::<ExtrudedTextGeometry>)]
    font: Font,
    /// Units an em of the text is.
    #[qml(notify, change = changed::<ExtrudedTextGeometry>)]
    scale: f64,
    /// Units the text is deep, whatever its scale.
    #[qml(notify, change = changed::<ExtrudedTextGeometry>)]
    depth: f64,
}

impl Default for ExtrudedTextGeometry {
    fn default() -> Self {
        ExtrudedTextGeometry {
            geometry: Geometry::default(),
            complete: false,
            asynchronous: false,
            status: NULL,
            text: String::new(),
            font: Font::default(),
            scale: 1.0,
            depth: 1.0,
        }
    }
}

impl Generated for ExtrudedTextGeometry {
    const STATUS: Option<Prop<i32>> = Some(ExtrudedTextGeometry::STATUS);
    /// No text, or only spaces, is a mesh of nothing made as asked.
    const EMPTY_IS_READY: bool = true;

    fn geometry(&mut self) -> &mut Geometry {
        &mut self.geometry
    }

    fn complete(&mut self) -> &mut bool {
        &mut self.complete
    }

    fn built(cx: &Cx) -> Option<Built> {
        type T = ExtrudedTextGeometry;
        let text: String = cx.get(T::TEXT).ok()?;
        let font: Font = cx.get(T::FONT).ok()?;
        let scale = cx.get(T::SCALE).ok()?;
        let depth = cx.get(T::DEPTH).ok()?;
        Some(extrude(
            &contours(cx.engine, &text, &font),
            scale as f32,
            depth.max(0.0) as f32,
        ))
    }
}

#[qml_impl]
impl ExtrudedTextGeometry {
    #[qml(on_complete)]
    fn completed(cx: &Cx) {
        complete::<ExtrudedTextGeometry>(cx);
    }
}

/// A closed polygon in ems, y up, counter-clockwise round what it fills
/// (clockwise round a hole).
type Contour = Vec<[f64; 2]>;

/// The outlines of `text` laid out in `font`, in ems from the left end of
/// the first line's baseline.
fn contours(e: &Engine, text: &str, font: &Font) -> Vec<Contour> {
    if text.is_empty() {
        return Vec::new();
    }
    let style = item::text_style(e, font);
    let em = f64::from(style.pixel_size);
    if em <= 0.0 {
        return Vec::new();
    }
    let Some(layout) =
        item::with_text_engine(|te| te.layout(text, &style, &LayoutParams::default()))
    else {
        return Vec::new();
    };
    let Some(first) = layout.lines.first() else {
        return Vec::new();
    };
    let baseline = first.baseline;
    let tolerance = TOLERANCE_PX / em;
    let mut out = Vec::new();
    for run in layout.lines.iter().flat_map(|l| &l.runs) {
        let run = &run.glyphs;
        for g in &run.glyphs {
            let Some(path) = glyph_path(run, g.id) else {
                continue;
            };
            let (gx, gy) = (f64::from(g.x), f64::from(g.y));
            // Font units, y up, into ems from the first baseline, y up.
            let to_em =
                kurbo::Affine::new([1.0 / em, 0.0, 0.0, 1.0 / em, gx / em, (baseline - gy) / em]);
            let mut glyph = flatten(&(to_em * path), tolerance);
            // TrueType goes clockwise round what it fills and CFF the other
            // way; the walls' normals need one way.
            if glyph.iter().map(|c| area(c)).sum::<f64>() < 0.0 {
                for c in &mut glyph {
                    c.reverse();
                }
            }
            out.extend(glyph);
        }
    }
    out
}

/// Draws an outline in font units, y up, the way the font has it.
struct Pen(BezPath);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((f64::from(x), f64::from(y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((f64::from(x), f64::from(y)));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(x), f64::from(y)),
        );
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(cx1), f64::from(cy1)),
            (f64::from(x), f64::from(y)),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

/// The outline of `glyph` at the run's size in pixels, y up, from the
/// glyph's origin, slanted as the run is; `None` without one. A run made
/// bold synthetically keeps its regular outline: it is not grown here.
fn glyph_path(run: &GlyphRun, glyph: u32) -> Option<BezPath> {
    let font = FontRef::from_index(run.font.data.as_ref(), run.font.index).ok()?;
    let units_per_em = f64::from(font.head().ok()?.units_per_em());
    if units_per_em <= 0.0 {
        return None;
    }
    let outline = font.outline_glyphs().get(GlyphId::new(glyph))?;
    let coords: Vec<NormalizedCoord> = run
        .normalized_coords
        .iter()
        .map(|c| NormalizedCoord::from_bits(*c))
        .collect();
    let mut pen = Pen(BezPath::new());
    outline
        .draw(
            DrawSettings::unhinted(Size::unscaled(), LocationRef::new(&coords)),
            &mut pen,
        )
        .ok()?;
    if pen.0.is_empty() {
        return None;
    }
    let k = f64::from(run.font_size) / units_per_em;
    let skew = f64::from(run.skew);
    Some(kurbo::Affine::new([k, 0.0, skew * k, k, 0.0, 0.0]) * pen.0)
}

/// The closed polygons `path` flattens into, without repeated points.
fn flatten(path: &BezPath, tolerance: f64) -> Vec<Contour> {
    let mut out: Vec<Contour> = Vec::new();
    let mut open: Contour = Vec::new();
    let close = |open: &mut Contour, out: &mut Vec<Contour>| {
        if open.len() > 1 && same(open[0], open[open.len() - 1]) {
            open.pop();
        }
        if open.len() >= 3 {
            out.push(std::mem::take(open));
        }
        open.clear();
    };
    kurbo::flatten(path.elements().iter().copied(), tolerance, |el| match el {
        PathEl::MoveTo(p) => {
            close(&mut open, &mut out);
            open.push([p.x, p.y]);
        }
        PathEl::LineTo(p) => {
            let p = [p.x, p.y];
            if p[0].is_finite() && p[1].is_finite() && open.last().is_none_or(|q| !same(*q, p)) {
                open.push(p);
            }
        }
        PathEl::ClosePath => close(&mut open, &mut out),
        _ => {}
    });
    close(&mut open, &mut out);
    out
}

fn same(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
}

/// Twice the signed area: positive counter-clockwise.
fn area(c: &[[f64; 2]]) -> f64 {
    (0..c.len())
        .map(|i| {
            let (p, q) = (c[i], c[(i + 1) % c.len()]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum()
}

/// The front at z = `depth` facing +z, the back at z = 0 facing -z and a
/// wall round every contour, an em `scale` units.
fn extrude(contours: &[Contour], scale: f32, depth: f32) -> Built {
    let mut built = Built::default();
    let Some(fill) = fill(contours) else {
        return built;
    };
    if fill.indices.is_empty() {
        return built;
    }
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in contours.iter().flatten() {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let span = [0, 1].map(|k| (hi[k] - lo[k]).max(1e-9));
    let uv = |p: [f64; 2]| {
        [
            ((p[0] - lo[0]) / span[0]) as f32,
            (1.0 - (p[1] - lo[1]) / span[1]) as f32,
        ]
    };
    let at = |p: [f64; 2], z: f32| [p[0] as f32 * scale, p[1] as f32 * scale, z];

    for (z, n) in [(depth, [0.0, 0.0, 1.0]), (0.0, [0.0, 0.0, -1.0])] {
        let first = built.positions.len() as u32;
        for v in &fill.vertices {
            let p = [f64::from(v[0]), f64::from(v[1])];
            built.vertex(at(p, z), n, uv(p));
        }
        for t in fill.indices.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| fill.vertices[i as usize]);
            let ccw = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) > 0.0;
            // Counter-clockwise seen from where the face looks.
            let (b_i, c_i) = if ccw == (n[2] > 0.0) {
                (t[1], t[2])
            } else {
                (t[2], t[1])
            };
            built
                .indices
                .extend([first + t[0], first + b_i, first + c_i]);
        }
    }

    for c in contours {
        let n = c.len();
        let normal = |i: usize| {
            let (p, q) = (c[i], c[(i + 1) % n]);
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let len = dx.hypot(dy).max(1e-12);
            [dy / len, -dx / len]
        };
        let normals: Vec<[f64; 2]> = (0..n).map(normal).collect();
        let smooth = |own: [f64; 2], other: [f64; 2]| {
            if own[0] * other[0] + own[1] * other[1] < SMOOTH_COS {
                return [own[0] as f32, own[1] as f32, 0.0];
            }
            let s = [own[0] + other[0], own[1] + other[1]];
            let len = s[0].hypot(s[1]).max(1e-12);
            [(s[0] / len) as f32, (s[1] / len) as f32, 0.0]
        };
        for i in 0..n {
            let j = (i + 1) % n;
            let own = normals[i];
            let start = smooth(own, normals[(i + n - 1) % n]);
            let end = smooth(own, normals[j]);
            let (u0, u1) = (uv(c[i])[0], uv(c[j])[0]);
            let a0 = built.vertex(at(c[i], 0.0), start, [u0, 1.0]);
            let b0 = built.vertex(at(c[j], 0.0), end, [u1, 1.0]);
            let b1 = built.vertex(at(c[j], depth), end, [u1, 0.0]);
            let a1 = built.vertex(at(c[i], depth), start, [u0, 0.0]);
            built.quad(a0, b0, b1, a1);
        }
    }
    built
}

/// The contours filled with triangles, by the non-zero rule as a font's
/// outlines are meant to be.
fn fill(contours: &[Contour]) -> Option<VertexBuffers<[f32; 2], u32>> {
    let mut b = LyonPath::builder();
    for c in contours {
        b.begin(point(c[0][0] as f32, c[0][1] as f32));
        for p in &c[1..] {
            b.line_to(point(p[0] as f32, p[1] as f32));
        }
        b.end(true);
    }
    let path = b.build();
    let mut out: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    FillTessellator::new()
        .tessellate_path(
            &path,
            &FillOptions::non_zero(),
            &mut BuffersBuilder::new(&mut out, |v: FillVertex<'_>| {
                let p = v.position();
                [p.x, p.y]
            }),
        )
        .ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64) -> Contour {
        vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s]]
    }

    #[test]
    fn a_square_is_two_caps_and_four_walls() {
        let built = extrude(&[square(0.0, 0.0, 1.0)], 10.0, 2.0);
        // Two triangles a cap, two a wall.
        assert_eq!(built.indices.len(), (2 + 2 + 4 * 2) * 3);
        let zs: Vec<f32> = built.positions.iter().map(|p| p[2]).collect();
        assert!(zs.iter().all(|z| *z == 0.0 || *z == 2.0));
        let xs = built.positions.iter().map(|p| p[0]);
        assert_eq!(xs.fold(f32::MIN, f32::max), 10.0);
        // The bottom wall looks down, the right one right.
        assert!(built.normals.contains(&[0.0, -1.0, 0.0]));
        assert!(built.normals.contains(&[1.0, 0.0, 0.0]));
    }

    #[test]
    fn every_face_winds_counter_clockwise_seen_from_its_normal() {
        let ring = [square(0.0, 0.0, 3.0), {
            let mut hole = square(1.0, 1.0, 1.0);
            hole.reverse();
            hole
        }];
        let built = extrude(&ring, 1.0, 1.0);
        for t in built.indices.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| built.positions[i as usize]);
            let n = built.normals[t[0] as usize];
            let (u, v) = (
                [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
                [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
            );
            let cross = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            assert!(cross[0] * n[0] + cross[1] * n[1] + cross[2] * n[2] > 0.0);
        }
        // The hole is not filled: no cap vertex lies inside it.
        let cap_inside = built
            .positions
            .iter()
            .zip(&built.normals)
            .any(|(p, n)| n[2] != 0.0 && p[0] > 1.0 && p[0] < 2.0 && p[1] > 1.0 && p[1] < 2.0);
        assert!(!cap_inside);
    }

    #[test]
    fn a_clockwise_outline_is_turned() {
        let mut cw = square(0.0, 0.0, 1.0);
        cw.reverse();
        assert!(area(&cw) < 0.0);
        let path = {
            let mut p = BezPath::new();
            p.move_to((0.0, 0.0));
            p.line_to((1.0, 0.0));
            p.line_to((1.0, 0.0));
            p.line_to((1.0, 1.0));
            p.close_path();
            p
        };
        let flat = flatten(&path, 0.01);
        assert_eq!(flat, vec![vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]]);
    }
}
