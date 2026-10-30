//! Meshes the Helpers' geometry types generate, and how they become the
//! `Geometry` a `Model` draws.

use ljos::item::ljos_3d::math::Vec3;
use ljos::item::ljos_3d::{ComponentType, PrimitiveType, Semantic};

use crate::node::Geometry;

/// A mesh in the layout every generated geometry shares: position, normal
/// and UV per vertex, and indices.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Built {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub primitive: Option<PrimitiveType>,
}

/// Bytes a vertex takes: three floats each of position and normal, two of
/// UV.
const STRIDE: usize = 32;

impl Built {
    pub fn vertex(&mut self, p: [f32; 3], n: [f32; 3], uv: [f32; 2]) -> u32 {
        self.positions.push(p);
        self.normals.push(n);
        self.uvs.push(uv);
        (self.positions.len() - 1) as u32
    }

    /// Two triangles, counter-clockwise, over the corners `a b c d` taken
    /// round the quad.
    pub fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.indices.extend([a, b, c, a, c, d]);
    }

    /// A grid of `cols` by `rows` cells over the vertices `first`, where
    /// the vertex at column `i` and row `j` is `first + j * (cols + 1) + i`.
    /// `flip` turns the winding round.
    pub fn grid(&mut self, first: u32, cols: u32, rows: u32, flip: bool) {
        let at = |i: u32, j: u32| first + j * (cols + 1) + i;
        for j in 0..rows {
            for i in 0..cols {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                if flip {
                    self.quad(a, d, c, b);
                } else {
                    self.quad(a, b, c, d);
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Replaces what `g` holds with this mesh, ready for
    /// [`Geometry::update`].
    pub fn into_geometry(self, g: &mut Geometry) {
        g.clear();
        if self.positions.is_empty() {
            return;
        }
        let mut data = Vec::with_capacity(self.positions.len() * STRIDE);
        for ((p, n), uv) in self.positions.iter().zip(&self.normals).zip(&self.uvs) {
            for f in p.iter().chain(n).chain(uv) {
                data.extend(f.to_le_bytes());
            }
        }
        g.set_vertex_data(data);
        g.set_stride(STRIDE);
        g.add_attribute(Semantic::Position, 0, ComponentType::F32);
        g.add_attribute(Semantic::Normal, 12, ComponentType::F32);
        g.add_attribute(Semantic::TexCoord0, 24, ComponentType::F32);
        if !self.indices.is_empty() {
            g.set_index_data(self.indices.iter().flat_map(|i| i.to_le_bytes()).collect());
            g.add_attribute(Semantic::Index, 0, ComponentType::U32);
        }
        g.set_primitive_type(self.primitive.unwrap_or(PrimitiveType::Triangles));
        let (mut lo, mut hi) = (self.positions[0], self.positions[0]);
        for p in &self.positions {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        g.set_bounds(
            Vec3::new(lo[0], lo[1], lo[2]),
            Vec3::new(hi[0], hi[1], hi[2]),
        );
    }
}
