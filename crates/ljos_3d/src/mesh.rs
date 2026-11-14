//! Geometry on the CPU: what a renderer uploads once per mesh.

use std::sync::OnceLock;

use crate::math::Vec3;
use crate::scene::Primitive;

/// How a mesh's indices make shapes once decoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Topology {
    /// Three indices a triangle.
    #[default]
    Triangles,
    /// Two indices a segment, one pixel wide.
    Lines,
    /// One index a point, one pixel wide.
    Points,
}

/// Triangles, counter-clockwise when seen from outside; or the segments or
/// points of a custom geometry, per [`Mesh::topology`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    /// The second UV set; empty where the mesh has one, which then serves
    /// as both.
    pub uv1: Vec<[f32; 2]>,
    /// Up to four joints a vertex follows, and how much of each; empty
    /// for a mesh that is not skinned.
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    /// Linear colours that multiply the material's; empty for a mesh that
    /// has none.
    pub colors: Vec<[f32; 4]>,
    pub topology: Topology,
}

/// The box that holds a primitive's mesh in model space, least corner
/// then most: its positions', found once.
pub(crate) fn primitive_box(p: Primitive) -> (Vec3, Vec3) {
    static BOXES: OnceLock<[(Vec3, Vec3); 5]> = OnceLock::new();
    let all = [
        Primitive::Cube,
        Primitive::Rectangle,
        Primitive::Sphere,
        Primitive::Cylinder,
        Primitive::Cone,
    ];
    let boxes = BOXES.get_or_init(|| {
        all.map(|p| {
            let found = Mesh::primitive(p).positions.iter().fold(
                ([f32::MAX; 3], [f32::MIN; 3]),
                |(lo, hi), v| {
                    (
                        [lo[0].min(v[0]), lo[1].min(v[1]), lo[2].min(v[2])],
                        [hi[0].max(v[0]), hi[1].max(v[1]), hi[2].max(v[2])],
                    )
                },
            );
            let ([a, b, c], [d, e, f]) = found;
            (Vec3::new(a, b, c), Vec3::new(d, e, f))
        })
    });
    boxes[all.iter().position(|q| *q == p).unwrap_or(0)]
}

impl Mesh {
    pub fn primitive(p: Primitive) -> Mesh {
        match p {
            Primitive::Cube => cube(),
            Primitive::Rectangle => rectangle(),
            Primitive::Sphere => sphere(),
            Primitive::Cylinder => cylinder(),
            Primitive::Cone => cone(),
        }
    }

    /// A tangent a vertex, for normal maps: `xyz` along increasing u,
    /// `w` the sign that makes `w × (normal × tangent)` point up the image
    /// (decreasing v, as v runs down from the image's top row).
    pub fn tangents(&self) -> Vec<[f32; 4]> {
        let n = self.positions.len();
        let mut t = vec![[0.0f32; 3]; n];
        let mut b = vec![[0.0f32; 3]; n];
        if self.uvs.len() != n {
            return vec![[1.0, 0.0, 0.0, 1.0]; n];
        }
        let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        for &[i0, i1, i2] in self.indices.as_chunks::<3>().0 {
            let [i0, i1, i2] = [i0 as usize, i1 as usize, i2 as usize];
            if i0.max(i1).max(i2) >= n {
                continue;
            }
            let (e1, e2) = (
                sub(self.positions[i1], self.positions[i0]),
                sub(self.positions[i2], self.positions[i0]),
            );
            let (du1, dv1) = (
                self.uvs[i1][0] - self.uvs[i0][0],
                self.uvs[i1][1] - self.uvs[i0][1],
            );
            let (du2, dv2) = (
                self.uvs[i2][0] - self.uvs[i0][0],
                self.uvs[i2][1] - self.uvs[i0][1],
            );
            let det = du1 * dv2 - du2 * dv1;
            if det.abs() < 1e-12 {
                continue;
            }
            let r = 1.0 / det;
            for k in [i0, i1, i2] {
                for c in 0..3 {
                    t[k][c] += (e1[c] * dv2 - e2[c] * dv1) * r;
                    b[k][c] += (e2[c] * du1 - e1[c] * du2) * r;
                }
            }
        }
        (0..n)
            .map(|i| {
                let nm = self.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]);
                let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                // Gram-Schmidt against the normal; a vertex with no u
                // direction (a pole) takes any direction across the normal.
                let d = dot(nm, t[i]);
                let mut tt = [
                    t[i][0] - nm[0] * d,
                    t[i][1] - nm[1] * d,
                    t[i][2] - nm[2] * d,
                ];
                let mut len = dot(tt, tt).sqrt();
                if len < 1e-6 {
                    let any = if nm[0].abs() < 0.9 {
                        [1.0, 0.0, 0.0]
                    } else {
                        [0.0, 1.0, 0.0]
                    };
                    let d = dot(nm, any);
                    tt = [any[0] - nm[0] * d, any[1] - nm[1] * d, any[2] - nm[2] * d];
                    len = dot(tt, tt).sqrt();
                }
                let tt = [tt[0] / len, tt[1] / len, tt[2] / len];
                let cross = [
                    nm[1] * tt[2] - nm[2] * tt[1],
                    nm[2] * tt[0] - nm[0] * tt[2],
                    nm[0] * tt[1] - nm[1] * tt[0],
                ];
                // b runs down the image; up is its opposite.
                let w = if dot(cross, b[i]) > 0.0 { -1.0 } else { 1.0 };
                [tt[0], tt[1], tt[2], w]
            })
            .collect()
    }

    fn vertex(&mut self, position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> u32 {
        self.positions.push(position);
        self.normals.push(normal);
        self.uvs.push(uv);
        self.positions.len() as u32 - 1
    }

    /// A round end of [`SIDES`] corners at height `y`, facing up or down,
    /// each corner where `pictured` says it is in the picture.
    fn disc(&mut self, y: f32, radius: f32, up: bool, pictured: impl Fn(f32, f32) -> [f32; 2]) {
        let normal = [0.0, if up { 1.0 } else { -1.0 }, 0.0];
        let first = self.positions.len() as u32;
        for step in 0..SIDES {
            let [x, z] = round(step, SIDES);
            self.vertex([x * radius, y, z * radius], normal, pictured(x, z));
        }
        for step in 1..SIDES - 1 {
            if up {
                self.indices.extend([first, first + step, first + step + 1]);
            } else {
                self.indices.extend([first, first + step + 1, first + step]);
            }
        }
    }

    /// Adds a square face of side 100 centred on `centre`, facing `normal`,
    /// with `u` along its first side; its corners get their own normals so
    /// the cube's edges stay hard.
    fn face(&mut self, centre: [f32; 3], normal: [f32; 3], u: [f32; 3]) {
        let v = [
            normal[1] * u[2] - normal[2] * u[1],
            normal[2] * u[0] - normal[0] * u[2],
            normal[0] * u[1] - normal[1] * u[0],
        ];
        let base = self.positions.len() as u32;
        for (su, sv) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            self.positions.push([
                centre[0] + 50.0 * (su * u[0] + sv * v[0]),
                centre[1] + 50.0 * (su * u[1] + sv * v[1]),
                centre[2] + 50.0 * (su * u[2] + sv * v[2]),
            ]);
            self.normals.push(normal);
            self.uvs.push([(su + 1.0) * 0.5, 1.0 - (sv + 1.0) * 0.5]);
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// `#Cube`: 100 units a side, centred on the origin.
fn cube() -> Mesh {
    let mut m = Mesh::default();
    m.face([0.0, 0.0, 50.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    m.face([0.0, 0.0, -50.0], [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0]);
    m.face([50.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
    m.face([-50.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    m.face([0.0, 50.0, 0.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]);
    m.face([0.0, -50.0, 0.0], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0]);
    m
}

/// `#Rectangle`: 100 by 100 in the XY plane, facing +Z.
fn rectangle() -> Mesh {
    let mut m = Mesh::default();
    m.face([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    m
}

// The sizes, the number of sides and the places in a picture below are those
// of the mesh files Qt has for these shapes, measured from what it draws; a
// round one is a little under a hundred across. A picture's up is Qt's here
// and goes in as `1 - v`, as v runs down from the image's top row in a mesh.

/// A place in a picture, with up as Qt has it.
fn up_the_picture(u: f32, v: f32) -> [f32; 2] {
    [u, 1.0 - v]
}

/// Where a step of `steps` round the upright is, as x and z: Qt goes from
/// the back by the left to the front and the right.
fn round(step: u32, steps: u32) -> [f32; 2] {
    let (s, c) = (std::f32::consts::TAU * step as f32 / steps as f32).sin_cos();
    [-s, -c]
}

const BALL: f32 = 49.475_61;
const SLICES: u32 = 50;
const SIDES: u32 = 40;
const TUBE: f32 = 49.979_98;
const HEIGHTS: [f32; 4] = [-TUBE, -16.659_912, 16.659_912, TUBE];
/// How far up the picture each of [`HEIGHTS`] is: the side of a cylinder
/// takes a strip of it.
const STRIP: [f32; 4] = [0.066_25, 0.188_75, 0.311_25, 0.433_75];
const FOOT: f32 = 49.974_61;
const GROUND: f32 = 0.025_390_625;
const POINT: f32 = 99.943_36;

/// `#Sphere`: fifty slices of fifty rings, a picture going once round it
/// from the back and from its foot to its top. Each pole is a corner to a
/// slice, at the middle of the slice in the picture.
fn sphere() -> Mesh {
    let mut m = Mesh::default();
    let slices = SLICES as f32;
    for step in 0..SLICES {
        m.vertex(
            [0.0, -BALL, 0.0],
            [0.0, -1.0, 0.0],
            up_the_picture((step as f32 + 0.5) / slices, 0.0),
        );
    }
    for ring in 1..SLICES {
        let (out, down) = (std::f32::consts::PI * ring as f32 / slices).sin_cos();
        let y = -down;
        for step in 0..=SLICES {
            let [x, z] = round(step, SLICES);
            m.vertex(
                [x * out * BALL, y * BALL, z * out * BALL],
                [x * out, y, z * out],
                up_the_picture(step as f32 / slices, ring as f32 / slices),
            );
        }
    }
    let top = m.positions.len() as u32;
    for step in 0..SLICES {
        m.vertex(
            [0.0, BALL, 0.0],
            [0.0, 1.0, 0.0],
            up_the_picture((step as f32 + 0.5) / slices, 1.0),
        );
    }
    let at = |ring: u32, step: u32| SLICES + (ring - 1) * (SLICES + 1) + step;
    for step in 0..SLICES {
        m.indices.extend([at(1, step), step, at(1, step + 1)]);
    }
    for ring in 1..SLICES - 1 {
        for step in 0..SLICES {
            let (a, b) = (at(ring, step), at(ring, step + 1));
            let (c, d) = (at(ring + 1, step + 1), at(ring + 1, step));
            // A square is cut one way in every other eighth of the way
            // round, and the other way above the middle.
            let even = (((step as f32 + 0.5) / (slices / 8.0)).floor() as u32).is_multiple_of(2);
            if even == (ring < SLICES / 2) {
                m.indices.extend([d, b, c, a, b, d]);
            } else {
                m.indices.extend([a, b, c, a, c, d]);
            }
        }
    }
    for step in 0..SLICES {
        m.indices
            .extend([at(SLICES - 1, step), at(SLICES - 1, step + 1), top + step]);
    }
    m
}

/// `#Cylinder`: a side taking a strip of a picture, from the left of the
/// shape round by its front, and each end a round piece of the picture's
/// upper left quarter, turned by half a side; the top's is the other way
/// across.
fn cylinder() -> Mesh {
    let mut m = Mesh::default();
    for (height, strip) in HEIGHTS.into_iter().zip(STRIP) {
        for step in 0..=SIDES {
            let [x, z] = round(step + SIDES / 4, SIDES);
            m.vertex(
                [x * TUBE, height, z * TUBE],
                [x, 0.0, z],
                up_the_picture(step as f32 / SIDES as f32, strip),
            );
        }
    }
    for ring in 0..HEIGHTS.len() as u32 - 1 {
        for step in 0..SIDES {
            let at = ring * (SIDES + 1) + step;
            m.indices.extend([
                at + 1,
                at + SIDES + 2,
                at + SIDES + 1,
                at + 1,
                at + SIDES + 1,
                at,
            ]);
        }
    }
    let (half_s, half_c) = (std::f32::consts::PI / SIDES as f32).sin_cos();
    let end = move |across: f32| {
        move |x: f32, z: f32| {
            up_the_picture(
                0.25 + 0.25 * across * (x * half_c + z * half_s),
                0.75 + 0.25 * (z * half_c - x * half_s),
            )
        }
    };
    m.disc(TUBE, TUBE, true, end(-1.0));
    m.disc(-TUBE, TUBE, false, end(1.0));
    m
}

/// `#Cone`: stands on the origin, with a picture laid on it from above, on
/// its side as on its foot, and one corner at its point, which faces up.
fn cone() -> Mesh {
    let mut m = Mesh::default();
    let pictured = |x: f32, z: f32| up_the_picture(0.5 + x / 2.0, 0.5 - z / 2.0);
    m.disc(GROUND, FOOT, false, pictured);
    let first = m.positions.len() as u32;
    let lean = FOOT / (POINT - GROUND);
    let length = (1.0 + lean * lean).sqrt();
    for step in 0..SIDES {
        let [x, z] = round(step, SIDES);
        m.vertex(
            [x * FOOT, GROUND, z * FOOT],
            [x / length, lean / length, z / length],
            pictured(x, z),
        );
    }
    let point = m.vertex([0.0, POINT, 0.0], [0.0, 1.0, 0.0], [0.5, 0.5]);
    for step in 0..SIDES {
        m.indices
            .extend([first + step, first + (step + 1) % SIDES, point]);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangles_tangent_runs_along_x_and_up_the_image_is_up_y() {
        let m = Mesh::primitive(Primitive::Rectangle);
        for (t, n) in m.tangents().iter().zip(&m.normals) {
            assert!((t[0] - 1.0).abs() < 1e-5 && t[1].abs() < 1e-5, "{t:?}");
            // w (n × t) is +Y, where the image's top row lies.
            let up = n[2] * t[0] - n[0] * t[2];
            assert!((t[3] * up - 1.0).abs() < 1e-5, "{t:?}");
        }
    }

    #[test]
    fn every_cube_face_winds_outwards() {
        let m = cube();
        for t in m.indices.chunks(3) {
            let p = |i: u32| m.positions[i as usize];
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let want = m.normals[t[0] as usize];
            let dot = n[0] * want[0] + n[1] * want[1] + n[2] * want[2];
            assert!(dot > 0.0, "triangle {t:?} winds inwards");
        }
        assert_eq!(m.positions.len(), 24);
        assert!(
            m.positions
                .iter()
                .all(|p| p.iter().all(|c| c.abs() == 50.0))
        );
    }

    /// Every triangle of a closed convex mesh faces away from a point
    /// inside it, and every corner's normal agrees with its face.
    fn winds_outwards(m: &Mesh, inside: [f32; 3]) {
        for t in m.indices.chunks(3) {
            let p = |i: u32| m.positions[i as usize];
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let out = [a[0] - inside[0], a[1] - inside[1], a[2] - inside[2]];
            let dot = n[0] * out[0] + n[1] * out[1] + n[2] * out[2];
            assert!(dot > 0.0, "triangle {t:?} winds inwards");
            for &i in t {
                let v = m.normals[i as usize];
                assert!(v[0] * n[0] + v[1] * n[1] + v[2] * n[2] > 0.0);
            }
        }
    }

    fn extent(m: &Mesh, axis: usize) -> (f32, f32) {
        m.positions
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                (lo.min(p[axis]), hi.max(p[axis]))
            })
    }

    #[test]
    fn the_sphere_has_qts_face_count_and_is_about_100_across() {
        let m = sphere();
        assert_eq!(m.indices.len() / 3, 4900);
        winds_outwards(&m, [0.0; 3]);
        // The poles are on Y; a round one of Qt's is a little under a
        // hundred across.
        for axis in 0..3 {
            let (lo, hi) = extent(&m, axis);
            assert!((-50.0..-49.3).contains(&lo), "{axis}: {lo}");
            assert!(hi > 49.3 && hi <= 50.0, "{axis}: {hi}");
        }
    }

    #[test]
    fn the_cylinder_has_qts_face_count_and_stands_along_y() {
        let m = cylinder();
        assert_eq!(m.indices.len() / 3, 316);
        winds_outwards(&m, [0.0; 3]);
        assert_eq!(extent(&m, 1), (-TUBE, TUBE));
    }

    #[test]
    fn the_cone_has_qts_face_count_and_points_up_from_its_base() {
        let m = cone();
        assert_eq!(m.indices.len() / 3, 78);
        winds_outwards(&m, [0.0, 25.0, 0.0]);
        assert_eq!(extent(&m, 1), (GROUND, POINT));
    }

    #[test]
    fn every_primitive_box_is_its_mesh_s() {
        for p in [
            Primitive::Cube,
            Primitive::Rectangle,
            Primitive::Sphere,
            Primitive::Cylinder,
            Primitive::Cone,
        ] {
            let (min, max) = primitive_box(p);
            let m = Mesh::primitive(p);
            assert_eq!(extent(&m, 0), (min.x, max.x), "{p:?}");
            assert_eq!(extent(&m, 1), (min.y, max.y), "{p:?}");
            assert_eq!(extent(&m, 2), (min.z, max.z), "{p:?}");
        }
        // A rectangle is flat: no deeper than its plane.
        let (min, max) = primitive_box(Primitive::Rectangle);
        assert_eq!((min.z, max.z), (0.0, 0.0));
    }

    #[test]
    fn every_bounding_sphere_holds_its_mesh() {
        for p in [
            Primitive::Cube,
            Primitive::Rectangle,
            Primitive::Sphere,
            Primitive::Cylinder,
            Primitive::Cone,
        ] {
            let (c, r) = p.bounds();
            for v in Mesh::primitive(p).positions {
                let d = ((v[0] - c.x).powi(2) + (v[1] - c.y).powi(2) + (v[2] - c.z).powi(2)).sqrt();
                assert!(d <= r + 1e-3, "{p:?} pokes out at {v:?}");
            }
        }
    }
}
