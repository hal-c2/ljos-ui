//! The meshes of the Helpers' shape geometries, as plain functions of
//! their parameters so they are tested without an engine. A parameter that
//! cannot make a shape (a radius of zero, fewer than three segments) makes
//! an empty mesh, which draws nothing, as the documentation says.

use std::f32::consts::{PI, TAU};

use ljos::item::ljos_3d::PrimitiveType;

use super::mesh::Built;

/// A cell count from a size's part: at least one cell.
fn cells(n: f64) -> u32 {
    n.round().max(1.0) as u32
}

/// One face of a cuboid: a grid of `cols` by `rows` cells in the plane
/// through `centre`, spanned by `u` (along columns) and `v` (along rows),
/// facing `normal`. The winding is counter-clockwise seen from `normal`.
fn face(
    b: &mut Built,
    centre: [f32; 3],
    u: [f32; 3],
    v: [f32; 3],
    normal: [f32; 3],
    c: u32,
    r: u32,
) {
    let first = b.positions.len() as u32;
    for j in 0..=r {
        for i in 0..=c {
            let (s, t) = (i as f32 / c as f32, j as f32 / r as f32);
            let p = std::array::from_fn(|k| centre[k] + u[k] * (s - 0.5) + v[k] * (t - 0.5));
            b.vertex(p, normal, [s, 1.0 - t]);
        }
    }
    // u x v points along the normal for the faces below; flip the other way.
    let along = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let dot: f32 = along.iter().zip(normal).map(|(a, n)| a * n).sum();
    b.grid(first, c, r, dot < 0.0);
}

/// A box centred on the origin. Each resolution is the cell count in the
/// two directions named, as `(width, height)`.
pub(crate) fn cuboid(extent: [f32; 3], xy: (f64, f64), xz: (f64, f64), yz: (f64, f64)) -> Built {
    let [x, y, z] = extent;
    let mut b = Built::default();
    if x <= 0.0 || y <= 0.0 || z <= 0.0 {
        return b;
    }
    let (xy, xz, yz) = (
        (cells(xy.0), cells(xy.1)),
        (cells(xz.0), cells(xz.1)),
        (cells(yz.0), cells(yz.1)),
    );
    let h = [x / 2.0, y / 2.0, z / 2.0];
    face(
        &mut b,
        [0.0, 0.0, h[2]],
        [x, 0.0, 0.0],
        [0.0, y, 0.0],
        [0.0, 0.0, 1.0],
        xy.0,
        xy.1,
    );
    face(
        &mut b,
        [0.0, 0.0, -h[2]],
        [x, 0.0, 0.0],
        [0.0, y, 0.0],
        [0.0, 0.0, -1.0],
        xy.0,
        xy.1,
    );
    face(
        &mut b,
        [0.0, h[1], 0.0],
        [x, 0.0, 0.0],
        [0.0, 0.0, z],
        [0.0, 1.0, 0.0],
        xz.0,
        xz.1,
    );
    face(
        &mut b,
        [0.0, -h[1], 0.0],
        [x, 0.0, 0.0],
        [0.0, 0.0, z],
        [0.0, -1.0, 0.0],
        xz.0,
        xz.1,
    );
    face(
        &mut b,
        [h[0], 0.0, 0.0],
        [0.0, y, 0.0],
        [0.0, 0.0, z],
        [1.0, 0.0, 0.0],
        yz.0,
        yz.1,
    );
    face(
        &mut b,
        [-h[0], 0.0, 0.0],
        [0.0, y, 0.0],
        [0.0, 0.0, z],
        [-1.0, 0.0, 0.0],
        yz.0,
        yz.1,
    );
    b
}

/// Which plane a flat grid lies in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plane {
    Xy,
    Xz,
    Zy,
}

/// A flat grid of `width` by `height` centred on the origin. The normal is
/// along the axis the plane leaves out, positive unless `reversed`.
pub(crate) fn plane(
    width: f32,
    height: f32,
    resolution: (f64, f64),
    plane: Plane,
    reversed: bool,
    mirrored: bool,
) -> Built {
    let mut b = Built::default();
    if width <= 0.0 || height <= 0.0 {
        return b;
    }
    let (c, r) = (cells(resolution.0), cells(resolution.1));
    // Columns run along `u`, rows along `v`, so that u x v is the +normal.
    let (u, v, n) = match plane {
        Plane::Xy => ([width, 0.0, 0.0], [0.0, height, 0.0], [0.0, 0.0, 1.0]),
        Plane::Xz => ([0.0, 0.0, width], [height, 0.0, 0.0], [0.0, 1.0, 0.0]),
        Plane::Zy => ([0.0, 0.0, -width], [0.0, height, 0.0], [1.0, 0.0, 0.0]),
    };
    let first = b.positions.len() as u32;
    let n = if reversed { n.map(|f| -f) } else { n };
    for j in 0..=r {
        for i in 0..=c {
            let (s, t) = (i as f32 / c as f32, j as f32 / r as f32);
            let p = std::array::from_fn(|k| u[k] * (s - 0.5) + v[k] * (t - 0.5));
            let vt = if mirrored { t } else { 1.0 - t };
            b.vertex(p, n, [s, vt]);
        }
    }
    b.grid(first, c, r, reversed);
    b
}

/// A sphere of `radius` with its poles on the Y axis.
pub(crate) fn sphere(radius: f32, rings: i32, segments: i32) -> Built {
    let mut b = Built::default();
    if radius <= 0.0 || rings < 2 || segments < 3 {
        return b;
    }
    let (rings, segments) = (rings as u32, segments as u32);
    for j in 0..=rings {
        let phi = PI * j as f32 / rings as f32;
        let (sp, cp) = phi.sin_cos();
        for i in 0..=segments {
            let theta = TAU * i as f32 / segments as f32;
            let (st, ct) = theta.sin_cos();
            let n = [sp * ct, cp, -sp * st];
            b.vertex(
                n.map(|f| f * radius),
                n,
                [i as f32 / segments as f32, j as f32 / rings as f32],
            );
        }
    }
    b.grid(0, segments, rings, true);
    b
}

/// A cone or cylinder along the Y axis, centred on the origin, from
/// `bottom_radius` at -length/2 to `top_radius` at +length/2, with
/// `rings` extra rings between the ends. A cap is made for each end whose
/// radius is not zero.
pub(crate) fn cone(
    top_radius: f32,
    bottom_radius: f32,
    length: f32,
    rings: i32,
    segments: i32,
) -> Built {
    let mut b = Built::default();
    if length <= 0.0
        || segments < 3
        || top_radius < 0.0
        || bottom_radius < 0.0
        || (top_radius == 0.0 && bottom_radius == 0.0)
    {
        return b;
    }
    let (spans, segments) = (rings.max(0) as u32 + 1, segments as u32);
    // The surface leans by the difference of the radii over the length.
    let lean = (bottom_radius - top_radius) / length;
    let norm = (1.0 + lean * lean).sqrt();
    let side = |theta: f32| {
        let (st, ct) = theta.sin_cos();
        [ct / norm, lean / norm, -st / norm]
    };
    for j in 0..=spans {
        let t = j as f32 / spans as f32;
        let (y, r) = (
            -length / 2.0 + length * t,
            bottom_radius + (top_radius - bottom_radius) * t,
        );
        for i in 0..=segments {
            let theta = TAU * i as f32 / segments as f32;
            let n = side(theta);
            let (st, ct) = theta.sin_cos();
            b.vertex(
                [r * ct, y, -r * st],
                n,
                [i as f32 / segments as f32, 1.0 - t],
            );
        }
    }
    b.grid(0, segments, spans, false);
    for (y, r, up) in [
        (length / 2.0, top_radius, 1.0f32),
        (-length / 2.0, bottom_radius, -1.0),
    ] {
        if r <= 0.0 {
            continue;
        }
        let normal = [0.0, up, 0.0];
        let centre = b.vertex([0.0, y, 0.0], normal, [0.5, 0.5]);
        let first = b.positions.len() as u32;
        for i in 0..=segments {
            let theta = TAU * i as f32 / segments as f32;
            let (st, ct) = theta.sin_cos();
            b.vertex(
                [r * ct, y, -r * st],
                normal,
                [0.5 + 0.5 * ct, 0.5 - 0.5 * st * up],
            );
        }
        for i in 0..segments {
            // The rim runs counter-clockwise seen from +Y, so the top
            // takes it as it is and the bottom the other way round.
            if up > 0.0 {
                b.indices.extend([centre, first + i, first + i + 1]);
            } else {
                b.indices.extend([centre, first + i + 1, first + i]);
            }
        }
    }
    b
}

/// A torus in the XY plane around the Z axis: `radius` to the middle of
/// the tube, whose own radius is `tube_radius`.
pub(crate) fn torus(radius: f32, tube_radius: f32, rings: i32, segments: i32) -> Built {
    let mut b = Built::default();
    if radius <= 0.0 || tube_radius <= 0.0 || rings < 3 || segments < 3 {
        return b;
    }
    let (rings, segments) = (rings as u32, segments as u32);
    for j in 0..=rings {
        let u = TAU * j as f32 / rings as f32;
        let (su, cu) = u.sin_cos();
        for i in 0..=segments {
            let v = TAU * i as f32 / segments as f32;
            let (sv, cv) = v.sin_cos();
            let n = [cv * cu, cv * su, sv];
            b.vertex(
                [
                    (radius + tube_radius * cv) * cu,
                    (radius + tube_radius * cv) * su,
                    tube_radius * sv,
                ],
                n,
                [j as f32 / rings as f32, i as f32 / segments as f32],
            );
        }
    }
    b.grid(0, segments, rings, true);
    b
}

/// How a capsule's UVs run along its length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UvProfile {
    Fixed,
    Aspect,
    Uniform,
}

/// A capsule along the X axis: a cylinder of `height` between two
/// hemispheres, `diameter` across in the YZ plane.
pub(crate) struct Capsule {
    pub height: f32,
    pub diameter: f32,
    pub latitudes: i32,
    pub longitudes: i32,
    pub rings: i32,
    pub normals: bool,
    pub uv: bool,
    pub profile: UvProfile,
}

impl Capsule {
    pub fn build(&self) -> Built {
        let mut b = Built::default();
        let r = self.diameter / 2.0;
        if r <= 0.0 || self.height < 0.0 || self.longitudes < 3 || self.latitudes < 2 {
            return b;
        }
        // Each hemisphere takes half the latitudes; an odd count is
        // rounded up to even.
        let half = (self.latitudes + 1) / 2;
        let (half, longs, mid) = (
            half as u32,
            self.longitudes as u32,
            self.rings.max(1) as u32,
        );
        let h = self.height / 2.0;
        // Rows from the +X pole to the -X pole; each has its axial
        // position and its cross-section.
        let mut rows: Vec<(f32, f32, [f32; 2])> = Vec::new();
        for k in 0..=half {
            let a = PI / 2.0 * k as f32 / half as f32;
            rows.push((h + r * a.cos(), r * a.sin(), [a.cos(), a.sin()]));
        }
        for k in 1..mid {
            let t = k as f32 / mid as f32;
            rows.push((h - self.height * t, r, [0.0, 1.0]));
        }
        for k in 0..=half {
            let a = PI / 2.0 * k as f32 / half as f32;
            rows.push((-h - r * a.sin(), r * a.cos(), [-a.sin(), a.cos()]));
        }
        let total = rows.len() as u32 - 1;
        let v_of = |row: usize, x: f32| match self.profile {
            UvProfile::Fixed => {
                if x > h {
                    (row as f32 / half as f32) / 3.0
                } else if x < -h {
                    2.0 / 3.0 + ((row as u32 - (total - half)) as f32 / half as f32) / 3.0
                } else {
                    1.0 / 3.0 + ((h - x) / self.height.max(f32::EPSILON)) / 3.0
                }
            }
            UvProfile::Aspect => (h + r - x) / (self.height + 2.0 * r).max(f32::EPSILON),
            UvProfile::Uniform => row as f32 / total as f32,
        };
        for (row, (x, ring, dir)) in rows.iter().enumerate() {
            for i in 0..=longs {
                let theta = TAU * i as f32 / longs as f32;
                let (st, ct) = theta.sin_cos();
                let n = [dir[0], dir[1] * ct, -dir[1] * st];
                b.vertex(
                    [*x, ring * ct, -ring * st],
                    if self.normals { n } else { [0.0; 3] },
                    if self.uv {
                        [i as f32 / longs as f32, v_of(row, *x)]
                    } else {
                        [0.0; 2]
                    },
                );
            }
        }
        b.grid(0, longs, total, false);
        b
    }
}

/// A grid of line segments in the XY plane, centred on the origin:
/// `horizontal` lines parallel to X, `horizontal_step` apart, and
/// `vertical` lines parallel to Y, `vertical_step` apart. Each line spans
/// the extent of the lines across it.
pub(crate) fn grid(
    horizontal: i32,
    horizontal_step: f32,
    vertical: i32,
    vertical_step: f32,
) -> Built {
    let mut b = Built {
        primitive: Some(PrimitiveType::Lines),
        ..Built::default()
    };
    let (h, v) = (horizontal.max(0), vertical.max(0));
    let width = v.saturating_sub(1) as f32 * vertical_step;
    let height = h.saturating_sub(1) as f32 * horizontal_step;
    let n = [0.0, 0.0, 1.0];
    for j in 0..h {
        let y = -height / 2.0 + j as f32 * horizontal_step;
        let (a, c) = (
            b.vertex([-width / 2.0, y, 0.0], n, [0.0, 0.0]),
            b.vertex([width / 2.0, y, 0.0], n, [1.0, 0.0]),
        );
        b.indices.extend([a, c]);
    }
    for i in 0..v {
        let x = -width / 2.0 + i as f32 * vertical_step;
        let (a, c) = (
            b.vertex([x, -height / 2.0, 0.0], n, [0.0, 0.0]),
            b.vertex([x, height / 2.0, 0.0], n, [0.0, 1.0]),
        );
        b.indices.extend([a, c]);
    }
    b
}

/// A mesh over a grid of `cols` by `rows` heights (each 0 to 1, row by row
/// from the far edge, as a picture is read). It spans `extents.x` by
/// `extents.z`, centred on the origin, and `extents.y` up from the plane.
/// `smooth` gives the normals of the surface at each vertex; without, every
/// triangle has its own vertices and its face's normal.
pub(super) fn height_field(
    heights: &[f32],
    cols: u32,
    rows: u32,
    extents: [f32; 3],
    smooth: bool,
) -> Built {
    let mut b = Built::default();
    if cols < 2 || rows < 2 || heights.len() < (cols * rows) as usize {
        return b;
    }
    let h = |i: u32, j: u32| heights[(j * cols + i) as usize] * extents[1];
    let point = |i: u32, j: u32| {
        [
            (i as f32 / (cols - 1) as f32 - 0.5) * extents[0],
            h(i, j),
            (j as f32 / (rows - 1) as f32 - 0.5) * extents[2],
        ]
    };
    let uv = |i: u32, j: u32| [i as f32 / (cols - 1) as f32, j as f32 / (rows - 1) as f32];
    // Counter-clockwise seen from above (+Y), where +Z is towards the viewer.
    let quads = || (0..rows - 1).flat_map(|j| (0..cols - 1).map(move |i| (i, j)));
    if smooth {
        let dx = extents[0] / (cols - 1) as f32;
        let dz = extents[2] / (rows - 1) as f32;
        for j in 0..rows {
            for i in 0..cols {
                let (l, r) = (i.saturating_sub(1), (i + 1).min(cols - 1));
                let (u, d) = (j.saturating_sub(1), (j + 1).min(rows - 1));
                let sx = (h(r, j) - h(l, j)) / ((r - l) as f32 * dx);
                let sz = (h(i, d) - h(i, u)) / ((d - u) as f32 * dz);
                let n = normalise([-sx, 1.0, -sz]);
                b.vertex(point(i, j), n, uv(i, j));
            }
        }
        for (i, j) in quads() {
            let at = |i: u32, j: u32| j * cols + i;
            b.quad(at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j));
        }
    } else {
        for (i, j) in quads() {
            let (a, c, d, e) = ((i, j), (i, j + 1), (i + 1, j + 1), (i + 1, j));
            for tri in [[a, c, d], [a, d, e]] {
                let p: Vec<[f32; 3]> = tri.iter().map(|&(i, j)| point(i, j)).collect();
                let n = face_normal(p[0], p[1], p[2]);
                for (k, &(i, j)) in tri.iter().enumerate() {
                    let v = b.vertex(p[k], n, uv(i, j));
                    b.indices.push(v);
                }
            }
        }
    }
    b
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    normalise([
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ])
}

fn normalise(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-9 {
        [0.0, 1.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangles(b: &Built) -> Vec<[[f32; 3]; 3]> {
        b.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| [0, 1, 2].map(|k| b.positions[t[k] as usize]))
            .collect()
    }

    /// The normal of a triangle by its winding.
    fn facing(t: &[[f32; 3]; 3]) -> [f32; 3] {
        let a = std::array::from_fn::<f32, 3, _>(|k| t[1][k] - t[0][k]);
        let c = std::array::from_fn::<f32, 3, _>(|k| t[2][k] - t[0][k]);
        [
            a[1] * c[2] - a[2] * c[1],
            a[2] * c[0] - a[0] * c[2],
            a[0] * c[1] - a[1] * c[0],
        ]
    }

    fn agrees(b: &Built) -> bool {
        b.indices.as_chunks::<3>().0.iter().all(|t| {
            let tri = [0, 1, 2].map(|k| b.positions[t[k] as usize]);
            let f = facing(&tri);
            // A pole's triangles have no area; rounding gives them any side.
            if f.iter().map(|c| c * c).sum::<f32>() < 1e-6 {
                return true;
            }
            let n = b.normals[t[0] as usize];
            f.iter().zip(n).map(|(a, b)| a * b).sum::<f32>() > 0.0
        })
    }

    #[test]
    fn a_cuboid_has_six_faces_that_face_outwards() {
        let b = cuboid([2.0, 4.0, 6.0], (1.0, 1.0), (1.0, 1.0), (1.0, 1.0));
        assert_eq!((b.positions.len(), triangles(&b).len()), (24, 12));
        assert!(agrees(&b));
        let top = b.positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        assert_eq!(top, 2.0);
    }

    #[test]
    fn a_cuboid_resolution_is_cells_per_face() {
        let b = cuboid([1.0; 3], (2.0, 3.0), (1.0, 1.0), (1.0, 1.0));
        // xy faces: 2x3 cells each, two triangles a cell, two faces.
        assert_eq!(triangles(&b).len(), 2 * 2 * 6 + 4 * 2);
    }

    #[test]
    fn planes_face_the_axis_they_leave_out() {
        for (p, axis) in [(Plane::Xy, 2), (Plane::Xz, 1), (Plane::Zy, 0)] {
            for reversed in [false, true] {
                let b = plane(10.0, 20.0, (2.0, 2.0), p, reversed, false);
                assert!(agrees(&b), "{p:?} {reversed}");
                let n = b.normals[0];
                assert_eq!(n[axis], if reversed { -1.0 } else { 1.0 });
            }
        }
    }

    #[test]
    fn a_sphere_lies_on_its_radius_and_faces_outwards() {
        let b = sphere(50.0, 8, 12);
        assert!(agrees(&b));
        for p in &b.positions {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((r - 50.0).abs() < 1e-3);
        }
        assert!(sphere(50.0, 1, 12).is_empty() && sphere(50.0, 8, 2).is_empty());
    }

    #[test]
    fn a_cylinder_and_a_cone_are_capped_where_they_have_a_radius() {
        let cyl = cone(10.0, 10.0, 40.0, 0, 8);
        assert!(agrees(&cyl));
        let lone = cone(0.0, 10.0, 40.0, 0, 8);
        assert!(agrees(&lone));
        // The cone's tip has no cap: one cap's fan fewer than the cylinder.
        assert_eq!(triangles(&cyl).len() - triangles(&lone).len(), 8);
        assert!(cone(0.0, 0.0, 40.0, 0, 8).is_empty() && cone(1.0, 1.0, 0.0, 0, 8).is_empty());
        assert!(cone(1.0, 1.0, 1.0, 0, 2).is_empty());
    }

    #[test]
    fn a_torus_keeps_its_tube_round_its_ring() {
        let b = torus(100.0, 10.0, 12, 8);
        assert!(agrees(&b));
        for p in &b.positions {
            let ring = (p[0] * p[0] + p[1] * p[1]).sqrt();
            let d = ((ring - 100.0).powi(2) + p[2] * p[2]).sqrt();
            assert!((d - 10.0).abs() < 1e-3);
        }
    }

    #[test]
    fn a_capsule_runs_along_x_and_reaches_past_its_height() {
        let c = Capsule {
            height: 100.0,
            diameter: 40.0,
            latitudes: 8,
            longitudes: 12,
            rings: 2,
            normals: true,
            uv: true,
            profile: UvProfile::Fixed,
        };
        let b = c.build();
        assert!(agrees(&b));
        let (lo, hi) = b
            .positions
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(p[0]), h.max(p[0])));
        assert!((hi - 70.0).abs() < 1e-3 && (lo + 70.0).abs() < 1e-3);
        for p in &b.positions {
            assert!((p[1] * p[1] + p[2] * p[2]).sqrt() <= 20.0 + 1e-3);
        }
    }

    #[test]
    fn a_grid_is_a_pair_of_vertices_per_line() {
        let b = grid(3, 2.0, 5, 1.0);
        assert_eq!(b.indices.len(), 2 * (3 + 5));
        assert_eq!(b.primitive, Some(PrimitiveType::Lines));
        // 5 vertical lines 1 apart span 4; 3 horizontal lines 2 apart span 4.
        let xs: Vec<f32> = b.positions.iter().map(|p| p[0]).collect();
        assert_eq!(xs.iter().cloned().fold(f32::MIN, f32::max), 2.0);
        assert!(grid(0, 1.0, 0, 1.0).is_empty());
    }

    #[test]
    fn a_height_field_faces_up_with_one_vertex_per_pixel() {
        let b = height_field(&[0.0, 0.5, 1.0, 0.25], 2, 2, [10.0, 8.0, 10.0], true);
        assert_eq!(b.positions.len(), 4);
        assert_eq!(b.indices.len(), 6);
        assert!(triangles(&b).iter().all(|t| facing(t)[1] > 0.0));
        // Heights scale by extents.y; the field is centred on the origin.
        assert_eq!(b.positions[2], [-5.0, 8.0, 5.0]);
    }

    #[test]
    fn a_flat_shaded_height_field_has_a_normal_for_each_face() {
        let b = height_field(&[0.0, 0.0, 1.0, 0.0], 2, 2, [10.0, 10.0, 10.0], false);
        assert_eq!(b.positions.len(), 6);
        assert!(b.normals[0] != b.normals[3]);
    }

    #[test]
    fn a_height_field_needs_two_pixels_each_way() {
        assert!(height_field(&[1.0], 1, 1, [1.0; 3], true).is_empty());
    }
}
