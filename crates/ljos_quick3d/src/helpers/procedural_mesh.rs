//! `ProceduralMesh`: a mesh a script fills in from lists of numbers, and
//! `ProceduralMeshSubset`, a range of it.

use ljos::item::ljos_3d::PrimitiveType;
use ljos::ljos_engine::Vector2d;
use ljos::prelude::*;

use super::geometry::{Generated, changed, complete};
use super::mesh::Built;
use crate::node::Geometry;

fn mode_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "PrimitiveMode",
        vec![
            ("Points", 0),
            ("LineStrip", 1),
            ("Lines", 2),
            ("TriangleStrip", 3),
            ("TriangleFan", 4),
            ("Triangles", 5),
        ],
    )]
}

/// A mesh made of the lists it is given: `positions` are the vertices and
/// `indexes` (all of them in order when left out) say how the primitives of
/// `primitiveMode` join them. A list of normals or texture coordinates that
/// is not as long as `positions` is left out, as an attribute of a mesh that
/// does not give one.
///
/// Normals not given are worked out for triangles from their faces. Vertex
/// colours, tangents, binormals and the second texture coordinates are kept
/// but not drawn.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "ProceduralMesh",
    extends = Geometry,
    enums = mode_enums
)]
pub struct ProceduralMesh {
    #[qml(base)]
    geometry: Geometry,
    #[qml(skip)]
    complete: bool,
    #[qml(notify, change = changed::<ProceduralMesh>)]
    positions: Vec<Vector3d>,
    #[qml(notify, change = changed::<ProceduralMesh>)]
    normals: Vec<Vector3d>,
    #[qml(notify)]
    tangents: Vec<Vector3d>,
    #[qml(notify)]
    binormals: Vec<Vector3d>,
    #[qml(notify, change = changed::<ProceduralMesh>)]
    uv0: Vec<Vector2d>,
    #[qml(notify)]
    uv1: Vec<Vector2d>,
    #[qml(notify)]
    colors: Vec<Color>,
    #[qml(notify, change = changed::<ProceduralMesh>)]
    indexes: Vec<i32>,
    #[qml(notify, change = changed::<ProceduralMesh>, enumeration = "PrimitiveMode")]
    primitive_mode: i32,
    #[qml(notify, list)]
    subsets: QmlList,
}

impl Default for ProceduralMesh {
    fn default() -> Self {
        ProceduralMesh {
            geometry: Geometry::default(),
            complete: false,
            positions: Vec::new(),
            normals: Vec::new(),
            tangents: Vec::new(),
            binormals: Vec::new(),
            uv0: Vec::new(),
            uv1: Vec::new(),
            colors: Vec::new(),
            indexes: Vec::new(),
            primitive_mode: TRIANGLES,
            subsets: QmlList::default(),
        }
    }
}

const TRIANGLES: i32 = 5;

impl Generated for ProceduralMesh {
    // A mesh a script fills has no state to report: it is empty or it is not.
    const STATUS: Option<Prop<i32>> = None;
    fn geometry(&mut self) -> &mut Geometry {
        &mut self.geometry
    }
    fn complete(&mut self) -> &mut bool {
        &mut self.complete
    }
    fn built(cx: &Cx) -> Option<Built> {
        let positions: Vec<Vector3d> = cx.get(ProceduralMesh::POSITIONS).ok()?;
        let normals: Vec<Vector3d> = cx.get(ProceduralMesh::NORMALS).ok()?;
        let uvs: Vec<Vector2d> = cx.get(ProceduralMesh::UV0).ok()?;
        let indexes: Vec<i32> = cx.get(ProceduralMesh::INDEXES).ok()?;
        let mode: i32 = cx.get(ProceduralMesh::PRIMITIVE_MODE).ok()?;
        Some(assemble(&positions, &normals, &uvs, &indexes, mode))
    }
}

#[qml_impl]
impl ProceduralMesh {
    #[qml(on_complete)]
    fn completed(cx: &Cx) {
        complete::<ProceduralMesh>(cx);
    }
}

/// The mesh the lists make. An index past the vertices ends the mesh
/// there: what follows is not a primitive of it.
fn assemble(
    positions: &[Vector3d],
    normals: &[Vector3d],
    uvs: &[Vector2d],
    indexes: &[i32],
    mode: i32,
) -> Built {
    let mut b = Built::default();
    let n = positions.len();
    let primitive = match mode {
        0 => PrimitiveType::Points,
        1 => PrimitiveType::LineStrip,
        2 => PrimitiveType::Lines,
        3 => PrimitiveType::TriangleStrip,
        4 => PrimitiveType::TriangleFan,
        _ => PrimitiveType::Triangles,
    };
    b.primitive = Some(primitive);
    b.indices = if indexes.is_empty() {
        (0..n as u32).collect()
    } else {
        indexes
            .iter()
            .map_while(|&i| u32::try_from(i).ok().filter(|&i| (i as usize) < n))
            .collect()
    };
    let given = normals.len() == n;
    let worked = if !given && primitive == PrimitiveType::Triangles {
        face_normals(positions, &b.indices)
    } else {
        vec![[0.0, 1.0, 0.0]; n]
    };
    for (i, p) in positions.iter().enumerate() {
        let normal = if given {
            [normals[i].x, normals[i].y, normals[i].z]
        } else {
            worked[i]
        };
        let uv = if uvs.len() == n {
            [uvs[i].x, uvs[i].y]
        } else {
            [0.0, 0.0]
        };
        b.vertex([p.x, p.y, p.z], normal, uv);
    }
    b
}

/// Each vertex's normal: its faces' normals, added up by area and made
/// unit length.
fn face_normals(positions: &[Vector3d], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut sum = vec![[0.0f32; 3]; positions.len()];
    for t in indices.as_chunks::<3>().0 {
        let at = |k: usize| {
            let p = positions[t[k] as usize];
            [p.x, p.y, p.z]
        };
        let (a, b, c) = (at(0), at(1), at(2));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for &i in t {
            for k in 0..3 {
                sum[i as usize][k] += n[k];
            }
        }
    }
    sum.into_iter()
        .map(|n| {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len < 1e-12 {
                [0.0, 1.0, 0.0]
            } else {
                [n[0] / len, n[1] / len, n[2] / len]
            }
        })
        .collect()
}

/// `ProceduralMeshSubset`: a part of a mesh, `length` indexes from `offset`.
/// Kept: the mesh is drawn whole, with the model's first material.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Helpers", name = "ProceduralMeshSubset", extends = QtObject)]
pub struct ProceduralMeshSubset {
    #[qml(base)]
    object: QtObject,
    #[qml(notify)]
    name: String,
    #[qml(notify)]
    length: i32,
    #[qml(notify)]
    offset: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32, z: f32) -> Vector3d {
        Vector3d { x, y, z }
    }

    #[test]
    fn a_triangle_without_normals_faces_where_its_winding_does() {
        let b = assemble(
            &[v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)],
            &[],
            &[],
            &[],
            TRIANGLES,
        );
        assert_eq!(b.indices, [0, 1, 2]);
        assert_eq!(b.normals[0], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn indexes_past_the_vertices_end_the_mesh() {
        let b = assemble(
            &[v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0)],
            &[],
            &[],
            &[0, 1, 7, 0],
            2,
        );
        assert_eq!(b.indices, [0, 1]);
        assert_eq!(b.primitive, Some(PrimitiveType::Lines));
    }

    #[test]
    fn a_list_of_normals_of_another_length_is_left_out() {
        let b = assemble(
            &[v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0)],
            &[v(1.0, 0.0, 0.0)],
            &[],
            &[],
            0,
        );
        assert_eq!(b.normals[0], [0.0, 1.0, 0.0]);
    }
}
