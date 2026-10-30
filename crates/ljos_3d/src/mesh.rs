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
}
