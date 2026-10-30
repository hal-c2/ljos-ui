//! Geometry on the CPU: what a renderer uploads once per mesh.

use crate::scene::Primitive;

/// Triangles, counter-clockwise when seen from outside.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
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

    fn vertex(&mut self, position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> u32 {
        self.positions.push(position);
        self.normals.push(normal);
        self.uvs.push(uv);
        self.positions.len() as u32 - 1
    }

    /// A flat disc of `sides` corners at height `y`, facing up or down,
    /// as a fan: `sides - 2` triangles.
    fn cap(&mut self, sides: u32, radius: f32, y: f32, up: bool) {
        let normal = [0.0, if up { 1.0 } else { -1.0 }, 0.0];
        let base = self.positions.len() as u32;
        for k in 0..sides {
            let (s, c) = around(k, sides);
            self.vertex(
                [radius * s, y, radius * c],
                normal,
                [0.5 + 0.5 * s, 0.5 - 0.5 * c],
            );
        }
        for k in 1..sides - 1 {
            // Seen from above, the angle turns counter-clockwise.
            if up {
                self.indices.extend([base, base + k, base + k + 1]);
            } else {
                self.indices.extend([base, base + k + 1, base + k]);
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
    m.face([0.0, 50.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
    m.face([0.0, -50.0, 0.0], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0]);
    m
}

/// `#Rectangle`: 100 by 100 in the XY plane, facing +Z.
fn rectangle() -> Mesh {
    let mut m = Mesh::default();
    m.face([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    m
}

/// The sine and cosine of the `k`th of `n` steps around Y. Angle 0 is +Z
/// and a quarter turn is +X, so a point is `(r sin, y, r cos)`; turning
/// that way is counter-clockwise from outside a side wall.
fn around(k: u32, n: u32) -> (f32, f32) {
    let a = std::f32::consts::TAU * k as f32 / n as f32;
    a.sin_cos()
}

/// Qt documents the face counts of its built-in meshes; these meet them.
const SPHERE_RINGS: u32 = 50;
const SPHERE_SEGMENTS: u32 = 50;
const CYLINDER_SIDES: u32 = 80;
const CONE_SIDES: u32 = 40;

/// `#Sphere`: diameter 100, centred on the origin, smooth.
fn sphere() -> Mesh {
    let mut m = Mesh::default();
    let (rings, segments) = (SPHERE_RINGS, SPHERE_SEGMENTS);
    for i in 0..=rings {
        let (sp, cp) = (std::f32::consts::PI * i as f32 / rings as f32).sin_cos();
        for j in 0..=segments {
            let (s, c) = around(j, segments);
            let n = [sp * s, cp, sp * c];
            m.vertex(
                [50.0 * n[0], 50.0 * n[1], 50.0 * n[2]],
                n,
                [j as f32 / segments as f32, i as f32 / rings as f32],
            );
        }
    }
    let at = |i: u32, j: u32| i * (segments + 1) + j;
    for i in 0..rings {
        for j in 0..segments {
            let (tl, tr) = (at(i, j), at(i, j + 1));
            let (bl, br) = (at(i + 1, j), at(i + 1, j + 1));
            // At the poles one edge of the quad is a point.
            if i != rings - 1 {
                m.indices.extend([bl, br, tr]);
            }
            if i != 0 {
                m.indices.extend([bl, tr, tl]);
            }
        }
    }
    m
}

/// `#Cylinder`: 100 tall along Y and 100 across, centred on the origin;
/// smooth sides and flat caps.
fn cylinder() -> Mesh {
    let mut m = Mesh::default();
    let n = CYLINDER_SIDES;
    let base = m.positions.len() as u32;
    for k in 0..=n {
        let (s, c) = around(k, n);
        let u = k as f32 / n as f32;
        m.vertex([50.0 * s, -50.0, 50.0 * c], [s, 0.0, c], [u, 1.0]);
        m.vertex([50.0 * s, 50.0, 50.0 * c], [s, 0.0, c], [u, 0.0]);
    }
    for k in 0..n {
        let (bl, tl) = (base + 2 * k, base + 2 * k + 1);
        let (br, tr) = (bl + 2, tl + 2);
        m.indices.extend([bl, br, tr, bl, tr, tl]);
    }
    m.cap(n, 50.0, 50.0, true);
    m.cap(n, 50.0, -50.0, false);
    m
}

/// `#Cone`: base 100 across on the origin, apex 100 up Y.
fn cone() -> Mesh {
    let mut m = Mesh::default();
    let n = CONE_SIDES;
    // The slope's normal leans up by the ratio of radius to height.
    let normal = |s: f32, c: f32| {
        let v = [100.0 * s, 50.0, 100.0 * c];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l]
    };
    for k in 0..n {
        let (s0, c0) = around(k, n);
        let (s1, c1) = around(k + 1, n);
        let (sm, cm) = (std::f32::consts::TAU * (k as f32 + 0.5) / n as f32).sin_cos();
        let (u0, u1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
        let a = m.vertex([50.0 * s0, 0.0, 50.0 * c0], normal(s0, c0), [u0, 1.0]);
        let b = m.vertex([50.0 * s1, 0.0, 50.0 * c1], normal(s1, c1), [u1, 1.0]);
        // The apex gets the normal of the middle of its face, one copy
        // per face, so the point does not shade as a single direction.
        let t = m.vertex([0.0, 100.0, 0.0], normal(sm, cm), [(u0 + u1) * 0.5, 0.0]);
        m.indices.extend([a, b, t]);
    }
    m.cap(n, 50.0, 0.0, false);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_sphere_has_qts_face_count_and_a_diameter_of_100() {
        let m = sphere();
        assert_eq!(m.indices.len() / 3, 4900);
        winds_outwards(&m, [0.0; 3]);
        // The poles are on Y; across, the rings' corners fall a little
        // inside the true circle.
        for axis in 0..3 {
            let (lo, hi) = extent(&m, axis);
            assert!((-50.001..-49.8).contains(&lo), "{axis}: {lo}");
            assert!(hi > 49.8 && hi <= 50.001, "{axis}: {hi}");
        }
    }

    #[test]
    fn the_cylinder_has_qts_face_count_and_stands_along_y() {
        let m = cylinder();
        assert_eq!(m.indices.len() / 3, 316);
        winds_outwards(&m, [0.0; 3]);
        assert_eq!(extent(&m, 1), (-50.0, 50.0));
    }

    #[test]
    fn the_cone_has_qts_face_count_and_points_up_from_its_base() {
        let m = cone();
        assert_eq!(m.indices.len() / 3, 78);
        winds_outwards(&m, [0.0, 25.0, 0.0]);
        assert_eq!(extent(&m, 1), (0.0, 100.0));
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
