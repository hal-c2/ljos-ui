//! Geometry an application provides: Qt Quick 3D's `QQuick3DGeometry`.
//!
//! The application describes its vertices as bytes: a stride, and
//! attributes that say where in each vertex the position, normal and
//! texture coordinates are. The scene carries the description in an `Arc`
//! and the renderer decodes and uploads it once per revision.

use std::fmt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::math::Vec3;
use crate::mesh::Mesh;

/// What an attribute of a vertex is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Semantic {
    /// The indices, in the index data rather than in each vertex.
    Index,
    /// Three `F32`: where the vertex is. Every geometry needs it.
    Position,
    /// Three `F32`, unit length. Without it the faces are flat.
    Normal,
    /// Two `F32`.
    TexCoord0,
    /// Two `F32`: the second UV set, which a texture's `indexUV` of 1
    /// reads.
    TexCoord1,
    /// Kept; nothing is drawn with it yet.
    Tangent,
    /// Kept; nothing is drawn with it yet.
    Binormal,
    /// Kept; nothing is drawn with it yet.
    Color,
    /// Four joint indices the vertex follows, as `U16`, `U32` or whole
    /// `F32`; drawn when the model has a skin.
    Joint,
    /// Four `F32`: how much of each of the vertex's joints it follows.
    Weight,
}

/// How an attribute's numbers are stored, little endian.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentType {
    U16,
    U32,
    F32,
}

impl ComponentType {
    pub fn size(self) -> usize {
        match self {
            ComponentType::U16 => 2,
            ComponentType::U32 | ComponentType::F32 => 4,
        }
    }
}

/// One attribute of a vertex, `offset` bytes into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub semantic: Semantic,
    pub offset: usize,
    pub component: ComponentType,
}

/// How the vertices, or the indices, make shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrimitiveType {
    /// Not drawn yet.
    Points,
    /// Not drawn yet.
    LineStrip,
    /// Not drawn yet.
    Lines,
    TriangleStrip,
    TriangleFan,
    #[default]
    Triangles,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GeometryError {
    #[error("the geometry has no position attribute")]
    NoPosition,
    #[error("the {0:?} attribute must be {1}")]
    WrongType(Semantic, &'static str),
    #[error("the stride is zero")]
    NoStride,
    #[error("the {0:?} attribute does not fit in a stride of {1} bytes")]
    OutsideStride(Semantic, usize),
    #[error("index {index} is past the {vertices} vertices")]
    IndexOutOfRange { index: u32, vertices: usize },
    #[error("{0:?} are not drawn yet")]
    NotYet(PrimitiveType),
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A geometry as the application last committed it. One object keeps its
/// `id` through every revision, so a renderer can tell a changed geometry
/// from another one.
#[derive(Clone)]
pub struct CustomGeometry {
    id: u64,
    revision: u64,
    vertex_data: Vec<u8>,
    index_data: Vec<u8>,
    stride: usize,
    attributes: Vec<Attribute>,
    primitive: PrimitiveType,
    bounds: Option<(Vec3, Vec3)>,
    // What culling asks for every frame, found once.
    sphere: OnceLock<(Vec3, f32)>,
}

impl Default for CustomGeometry {
    fn default() -> Self {
        CustomGeometry {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            revision: 0,
            vertex_data: Vec::new(),
            index_data: Vec::new(),
            stride: 0,
            attributes: Vec::new(),
            primitive: PrimitiveType::default(),
            bounds: None,
            sphere: OnceLock::new(),
        }
    }
}

impl fmt::Debug for CustomGeometry {
    // The bytes are large; their lengths say enough.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomGeometry")
            .field("id", &self.id)
            .field("revision", &self.revision)
            .field("vertex_bytes", &self.vertex_data.len())
            .field("index_bytes", &self.index_data.len())
            .field("stride", &self.stride)
            .field("primitive", &self.primitive)
            .finish_non_exhaustive()
    }
}

/// Two geometries are the same when they are one object at one revision:
/// comparing bytes would cost a scene its cheap "did anything change".
impl PartialEq for CustomGeometry {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.revision == other.revision
    }
}

impl CustomGeometry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Commits what was set: a renderer uploads the geometry again.
    pub fn update(&mut self) {
        self.revision += 1;
    }

    pub fn set_vertex_data(&mut self, data: Vec<u8>) {
        self.vertex_data = data;
        self.sphere = OnceLock::new();
    }

    pub fn set_index_data(&mut self, data: Vec<u8>) {
        self.index_data = data;
    }

    pub fn set_stride(&mut self, stride: usize) {
        self.stride = stride;
        self.sphere = OnceLock::new();
    }

    pub fn add_attribute(&mut self, semantic: Semantic, offset: usize, component: ComponentType) {
        self.attributes.push(Attribute {
            semantic,
            offset,
            component,
        });
        self.sphere = OnceLock::new();
    }

    pub fn set_primitive_type(&mut self, primitive: PrimitiveType) {
        self.primitive = primitive;
    }

    /// The box the geometry lies in, which culling uses; without it the
    /// positions are searched.
    pub fn set_bounds(&mut self, min: Vec3, max: Vec3) {
        self.bounds = Some((min, max));
        self.sphere = OnceLock::new();
    }

    /// Back to nothing: no data, no attributes, no bounds. The `id` stays.
    pub fn clear(&mut self) {
        let (id, revision) = (self.id, self.revision);
        *self = CustomGeometry {
            id,
            revision,
            ..CustomGeometry::default()
        };
    }

    pub fn vertex_data(&self) -> &[u8] {
        &self.vertex_data
    }

    pub fn index_data(&self) -> &[u8] {
        &self.index_data
    }

    pub fn stride(&self) -> usize {
        self.stride
    }

    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }

    pub fn primitive_type(&self) -> PrimitiveType {
        self.primitive
    }

    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        self.bounds
    }

    fn attribute(&self, semantic: Semantic) -> Option<Attribute> {
        self.attributes
            .iter()
            .rev()
            .find(|a| a.semantic == semantic)
            .copied()
    }

    /// Whether [`CustomGeometry::to_mesh`] can build it.
    pub fn validate(&self) -> Result<(), GeometryError> {
        self.to_mesh().map(drop)
    }

    /// A sphere that holds the geometry, centre and radius in model space:
    /// what frustum culling tests. An empty or broken geometry has none
    /// to speak of and is not drawn anyway.
    pub fn bounding_sphere(&self) -> (Vec3, f32) {
        *self.sphere.get_or_init(|| {
            let (min, max) = self
                .bounds
                .or_else(|| self.position_box())
                .unwrap_or_default();
            let centre = Vec3::new(
                (min.x + max.x) * 0.5,
                (min.y + max.y) * 0.5,
                (min.z + max.z) * 0.5,
            );
            let half = Vec3::new(max.x - centre.x, max.y - centre.y, max.z - centre.z);
            (centre, half.length())
        })
    }

    fn position_box(&self) -> Option<(Vec3, Vec3)> {
        let mut found: Option<(Vec3, Vec3)> = None;
        for p in self.read_vec::<3>(Semantic::Position).ok()? {
            let (min, max) =
                found.unwrap_or((Vec3::new(p[0], p[1], p[2]), Vec3::new(p[0], p[1], p[2])));
            found = Some((
                Vec3::new(min.x.min(p[0]), min.y.min(p[1]), min.z.min(p[2])),
                Vec3::new(max.x.max(p[0]), max.y.max(p[1]), max.z.max(p[2])),
            ));
        }
        found
    }

    /// The vertices' `N` floats of `semantic`.
    fn read_vec<const N: usize>(&self, semantic: Semantic) -> Result<Vec<[f32; N]>, GeometryError> {
        let a = self.attribute(semantic).ok_or(GeometryError::NoPosition)?;
        if a.component != ComponentType::F32 {
            return Err(GeometryError::WrongType(semantic, "F32"));
        }
        if self.stride == 0 {
            return Err(GeometryError::NoStride);
        }
        if a.offset + 4 * N > self.stride {
            return Err(GeometryError::OutsideStride(semantic, self.stride));
        }
        Ok(self
            .vertex_data
            .chunks_exact(self.stride)
            .map(|v| {
                let mut out = [0.0; N];
                for (i, f) in out.iter_mut().enumerate() {
                    let at = a.offset + 4 * i;
                    *f = f32::from_le_bytes([v[at], v[at + 1], v[at + 2], v[at + 3]]);
                }
                out
            })
            .collect())
    }

    /// The vertices' four joint indices. Qt's own geometries give them as
    /// integers or as floats holding whole numbers; both are read.
    fn joints(&self, a: Attribute) -> Result<Vec<[u16; 4]>, GeometryError> {
        if self.stride == 0 {
            return Err(GeometryError::NoStride);
        }
        if a.offset + 4 * a.component.size() > self.stride {
            return Err(GeometryError::OutsideStride(Semantic::Joint, self.stride));
        }
        let size = a.component.size();
        Ok(self
            .vertex_data
            .chunks_exact(self.stride)
            .map(|v| {
                std::array::from_fn(|i| {
                    let b = &v[a.offset + i * size..];
                    match a.component {
                        ComponentType::U16 => u16::from_le_bytes([b[0], b[1]]),
                        ComponentType::U32 => {
                            u32::from_le_bytes([b[0], b[1], b[2], b[3]]).min(u32::from(u16::MAX))
                                as u16
                        }
                        ComponentType::F32 => {
                            f32::from_le_bytes([b[0], b[1], b[2], b[3]]).clamp(0.0, 65535.0) as u16
                        }
                    }
                })
            })
            .collect())
    }

    fn indices(&self, vertices: usize) -> Result<Vec<u32>, GeometryError> {
        let indices: Vec<u32> = match self.attribute(Semantic::Index) {
            None => (0..vertices as u32).collect(),
            Some(a) => match a.component {
                ComponentType::U16 => self
                    .index_data
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u32::from(u16::from_le_bytes(*b)))
                    .collect(),
                ComponentType::U32 => self
                    .index_data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| u32::from_le_bytes(*b))
                    .collect(),
                ComponentType::F32 => {
                    return Err(GeometryError::WrongType(Semantic::Index, "U16 or U32"));
                }
            },
        };
        match indices.iter().find(|i| **i as usize >= vertices) {
            Some(&index) => Err(GeometryError::IndexOutOfRange { index, vertices }),
            None => Ok(indices),
        }
    }

    /// The triangles of the geometry, as three vertex indices each.
    fn triangles(&self, vertices: usize) -> Result<Vec<u32>, GeometryError> {
        let i = self.indices(vertices)?;
        let mut out = Vec::new();
        match self.primitive {
            PrimitiveType::Triangles => {
                out.extend(i.as_chunks::<3>().0.iter().flatten());
            }
            // Every other triangle is flipped so that all face one way.
            PrimitiveType::TriangleStrip => {
                for (k, w) in i.windows(3).enumerate() {
                    if k % 2 == 0 {
                        out.extend([w[0], w[1], w[2]]);
                    } else {
                        out.extend([w[1], w[0], w[2]]);
                    }
                }
            }
            PrimitiveType::TriangleFan => {
                for w in i.windows(2).skip(1) {
                    out.extend([i[0], w[0], w[1]]);
                }
            }
            other => return Err(GeometryError::NotYet(other)),
        }
        Ok(out)
    }

    /// Decodes the geometry for upload. Without normals each triangle gets
    /// its own vertices and the normal of its face, counter-clockwise
    /// being the front, so the geometry is lit flat.
    pub fn to_mesh(&self) -> Result<Mesh, GeometryError> {
        let positions = self.read_vec::<3>(Semantic::Position)?;
        let triangles = self.triangles(positions.len())?;
        let normals = self
            .attribute(Semantic::Normal)
            .map(|_| self.read_vec::<3>(Semantic::Normal))
            .transpose()?;
        let uvs = self
            .attribute(Semantic::TexCoord0)
            .map(|_| self.read_vec::<2>(Semantic::TexCoord0))
            .transpose()?;
        let uv_of = |v: usize| uvs.as_ref().map_or([0.0; 2], |u| u[v]);
        let uv1 = self
            .attribute(Semantic::TexCoord1)
            .map(|_| self.read_vec::<2>(Semantic::TexCoord1))
            .transpose()?;
        let skin = match (self.attribute(Semantic::Joint), self.attribute(Semantic::Weight)) {
            (Some(joints), Some(_)) => {
                Some((self.joints(joints)?, self.read_vec::<4>(Semantic::Weight)?))
            }
            _ => None,
        };
        // Where each vertex of the mesh came from in the geometry.
        let mut from: Vec<u32> = Vec::new();
        let mut mesh = Mesh::default();
        match normals {
            Some(normals) => {
                mesh.positions = positions;
                mesh.normals = normals;
                mesh.uvs = (0..mesh.positions.len()).map(uv_of).collect();
                mesh.indices = triangles;
                from = (0..mesh.positions.len() as u32).collect();
            }
            None => {
                for t in triangles.as_chunks::<3>().0 {
                    let p = [t[0], t[1], t[2]].map(|v| positions[v as usize]);
                    let a = Vec3::new(p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]);
                    let b = Vec3::new(p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]);
                    let n = a.cross(b).normalized();
                    for (k, v) in t.iter().enumerate() {
                        mesh.positions.push(p[k]);
                        mesh.normals.push([n.x, n.y, n.z]);
                        mesh.uvs.push(uv_of(*v as usize));
                        mesh.indices.push(mesh.positions.len() as u32 - 1);
                        from.push(*v);
                    }
                }
            }
        }
        if let Some(uv1) = uv1 {
            mesh.uv1 = from.iter().map(|v| uv1[*v as usize]).collect();
        }
        if let Some((joints, weights)) = skin {
            mesh.joints = from.iter().map(|v| joints[*v as usize]).collect();
            mesh.weights = from.iter().map(|v| weights[*v as usize]).collect();
        }
        Ok(mesh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floats(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    fn quad() -> CustomGeometry {
        let mut g = CustomGeometry::new();
        g.set_vertex_data(floats(&[
            0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 10.0, 10.0, 0.0, 0.0, 10.0, 0.0,
        ]));
        g.set_stride(12);
        g.add_attribute(Semantic::Position, 0, ComponentType::F32);
        g
    }

    #[test]
    fn a_missing_normal_makes_flat_faces_that_face_the_front() {
        let mut g = quad();
        g.set_index_data(
            [0u16, 1, 2, 0, 2, 3]
                .iter()
                .flat_map(|i| i.to_le_bytes())
                .collect(),
        );
        g.add_attribute(Semantic::Index, 0, ComponentType::U16);
        let m = g.to_mesh().unwrap();
        assert_eq!(m.positions.len(), 6, "one vertex for each corner of a face");
        assert_eq!(m.indices.len(), 6);
        assert!(m.normals.iter().all(|n| *n == [0.0, 0.0, 1.0]));
    }

    #[test]
    fn a_normal_attribute_is_kept_and_vertices_are_shared() {
        let mut g = CustomGeometry::new();
        g.set_vertex_data(floats(&[
            0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
            0.0,
        ]));
        g.set_stride(24);
        g.add_attribute(Semantic::Position, 0, ComponentType::F32);
        g.add_attribute(Semantic::Normal, 12, ComponentType::F32);
        let m = g.to_mesh().unwrap();
        assert_eq!(m.positions.len(), 3);
        assert_eq!(m.normals[0], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn a_fan_and_a_strip_make_triangles_that_face_one_way() {
        let mut fan = quad();
        fan.set_primitive_type(PrimitiveType::TriangleFan);
        let m = fan.to_mesh().unwrap();
        assert_eq!(m.indices, [0, 1, 2, 3, 4, 5]);
        assert!(m.normals.iter().all(|n| *n == [0.0, 0.0, 1.0]));
        // A strip turns every other triangle round so all face one way:
        // the corners (0, 1, 2, 3) of a zigzag make 012 and 213.
        let mut strip = CustomGeometry::new();
        strip.set_vertex_data(floats(&[
            0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0,
        ]));
        strip.set_stride(12);
        strip.add_attribute(Semantic::Position, 0, ComponentType::F32);
        strip.set_primitive_type(PrimitiveType::TriangleStrip);
        let m = strip.to_mesh().unwrap();
        assert_eq!(m.positions.len(), 6);
        assert!(
            m.normals.iter().all(|n| *n == [0.0, 0.0, 1.0]),
            "{:?}",
            m.normals
        );
    }

    #[test]
    fn what_cannot_be_drawn_is_an_error() {
        let mut g = quad();
        g.set_primitive_type(PrimitiveType::Lines);
        assert_eq!(
            g.validate(),
            Err(GeometryError::NotYet(PrimitiveType::Lines))
        );
        assert_eq!(
            CustomGeometry::new().validate(),
            Err(GeometryError::NoPosition)
        );
        let mut g = quad();
        g.set_stride(8);
        assert!(matches!(
            g.validate(),
            Err(GeometryError::OutsideStride(..))
        ));
        let mut g = quad();
        g.set_index_data(vec![9, 0, 0, 0]);
        g.add_attribute(Semantic::Index, 0, ComponentType::U32);
        assert!(matches!(
            g.validate(),
            Err(GeometryError::IndexOutOfRange {
                index: 9,
                vertices: 4
            })
        ));
    }

    #[test]
    fn the_bounding_sphere_holds_the_bounds_or_the_positions() {
        let mut g = quad();
        let (c, r) = g.bounding_sphere();
        assert_eq!((c.x, c.y, c.z), (5.0, 5.0, 0.0));
        assert!((r - 50f32.sqrt()).abs() < 1e-4);
        g.set_bounds(
            Vec3::new(-100.0, -100.0, -100.0),
            Vec3::new(100.0, 100.0, 100.0),
        );
        let (c, r) = g.bounding_sphere();
        assert_eq!((c.x, c.y, c.z), (0.0, 0.0, 0.0));
        assert!((r - 300f32.sqrt() * 10.0 * 1.0).abs() < 1e-2);
    }

    #[test]
    fn a_revision_is_a_new_geometry_to_compare_and_clear_keeps_the_id() {
        let mut g = quad();
        let before = g.clone();
        assert_eq!(g, before);
        g.update();
        assert_ne!(g, before);
        assert_eq!(g.id(), before.id());
        g.clear();
        assert_eq!(g.id(), before.id());
        assert!(g.vertex_data().is_empty() && g.attributes().is_empty());
        assert_ne!(CustomGeometry::new().id(), g.id());
    }
}
