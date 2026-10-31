//! The shapes a body is made of.
//!
//! [`read`] says what a shape is now, as big as the scale of its body makes
//! it. It is called in the effect of the body, so a change of anything it
//! reads is what makes the world take the body's shapes again.

use std::rc::Rc;
use std::sync::Arc;

use ljos::item;
use ljos::item::ljos_3d::mesh_file::MeshFile;
use ljos::ljos_engine::Provenance;
use ljos::prelude::*;
use ljos_physics::{Part, Pose, Shape};
use ljos_quick3d::{Geometry, Node, mesh_source};

/// `CollisionShape`: what all the shapes have.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "CollisionShape", extends = Node, uncreatable)]
pub struct CollisionShape {
    #[qml(base)]
    pub node: Node,
    /// Kept: nothing of a world is drawn.
    #[qml(notify)]
    pub enable_debug_draw: bool,
}

/// `BoxShape`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "BoxShape", extends = CollisionShape)]
pub struct BoxShape {
    #[qml(base)]
    pub shape: CollisionShape,
    #[qml(notify)]
    pub extents: Vector3d,
}

const HUNDREDS: Vector3d = Vector3d {
    x: 100.0,
    y: 100.0,
    z: 100.0,
};

impl Default for BoxShape {
    fn default() -> Self {
        BoxShape {
            shape: CollisionShape::default(),
            extents: HUNDREDS,
        }
    }
}

/// `SphereShape`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "SphereShape", extends = CollisionShape)]
pub struct SphereShape {
    #[qml(base)]
    pub shape: CollisionShape,
    #[qml(notify)]
    pub diameter: f64,
}

impl Default for SphereShape {
    fn default() -> Self {
        SphereShape {
            shape: CollisionShape::default(),
            diameter: 100.0,
        }
    }
}

/// `CapsuleShape`: a cylinder with half a sphere at each end, along X.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "CapsuleShape", extends = CollisionShape)]
pub struct CapsuleShape {
    #[qml(base)]
    pub shape: CollisionShape,
    #[qml(notify)]
    pub diameter: f64,
    /// Of the cylinder, without the ends.
    #[qml(notify)]
    pub height: f64,
}

impl Default for CapsuleShape {
    fn default() -> Self {
        CapsuleShape {
            shape: CollisionShape::default(),
            diameter: 100.0,
            height: 100.0,
        }
    }
}

/// `PlaneShape`: what is behind the node's XY plane is solid.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "PlaneShape", extends = CollisionShape)]
pub struct PlaneShape {
    #[qml(base)]
    pub shape: CollisionShape,
}

/// `MeshShape`: a shape from a `.mesh` file or a `Geometry`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "MeshShape", extends = CollisionShape, uncreatable)]
pub struct MeshShape {
    #[qml(base)]
    pub shape: CollisionShape,
    #[qml(notify)]
    pub source: Url,
    /// Wins over `source`.
    #[qml(notify, object = "Geometry")]
    pub geometry: Option<ObjectId>,
    /// How many times a file has arrived: what the body's effect tracks.
    #[qml(notify, readonly)]
    pub revision: i32,
    #[qml(skip)]
    file: Option<Arc<MeshFile>>,
    /// The source a read has answered for, with or without a mesh.
    #[qml(skip)]
    answered: String,
}

/// `ConvexMeshShape`: the hull of the corners of a mesh.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "ConvexMeshShape", extends = MeshShape)]
pub struct ConvexMeshShape {
    #[qml(base)]
    pub mesh: MeshShape,
}

/// `TriangleMeshShape`: the triangles of a mesh themselves, which only a
/// static or kinematic body can be made of.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "TriangleMeshShape", extends = MeshShape)]
pub struct TriangleMeshShape {
    #[qml(base)]
    pub mesh: MeshShape,
}

/// `HeightFieldShape`: ground whose height is the brightness of a picture.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "HeightFieldShape", extends = CollisionShape)]
pub struct HeightFieldShape {
    #[qml(base)]
    pub shape: CollisionShape,
    #[qml(notify)]
    pub source: Url,
    /// Unset, the longer side is 100, the other as the picture is, and it
    /// is 100 high.
    #[qml(notify)]
    pub extents: Vector3d,
    #[qml(notify, readonly)]
    pub revision: i32,
    #[qml(skip)]
    field: Option<Rc<Field>>,
    #[qml(skip)]
    answered: String,
}

impl Default for HeightFieldShape {
    fn default() -> Self {
        HeightFieldShape {
            shape: CollisionShape::default(),
            source: Url::default(),
            extents: HUNDREDS,
            revision: 0,
            field: None,
            answered: String::new(),
        }
    }
}

#[derive(Debug)]
struct Field {
    cols: usize,
    rows: usize,
    heights: Vec<f32>,
}

#[qml_impl]
impl MeshShape {
    /// Reads the file its `source` names, and again whenever that changes.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        cx.engine.effect_in(me, move |e| {
            let source = e.get(me, MeshShape::SOURCE).unwrap_or_default();
            let url = if source.0.is_empty() {
                String::new()
            } else {
                e.resolve_url(me, source.0.as_str())
            };
            e.untrack(|| read_mesh(e, me, url));
        })?;
        Ok(())
    }
}

fn read_mesh(e: &Engine, me: ObjectId, url: String) {
    if url.is_empty() {
        return;
    }
    let wanted = url.clone();
    mesh_source::load(e, &url, move |e, got| {
        if !e.is_alive(me) {
            return;
        }
        let now = e
            .untrack(|| e.get(me, MeshShape::SOURCE))
            .unwrap_or_default();
        if e.resolve_url(me, now.0.as_str()) != wanted {
            return;
        }
        let file = got
            .inspect_err(|err| e.warn(format!("QtQuick3D.Physics: cannot read {wanted}: {err}")))
            .ok();
        let revision = e.with_mut(me, |m: &mut MeshShape| {
            m.file = file;
            m.answered.clone_from(&wanted);
            m.revision.wrapping_add(1)
        });
        if let Ok(revision) = revision {
            let _ = e.set_internal(me, MeshShape::REVISION, revision);
        }
    });
}

#[qml_impl]
impl HeightFieldShape {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        cx.engine.effect_in(me, move |e| {
            let source = e.get(me, HeightFieldShape::SOURCE).unwrap_or_default();
            if source.0.is_empty() {
                return;
            }
            let url = e.resolve_url(me, source.0.as_str());
            e.untrack(|| read_field(e, me, url));
        })?;
        Ok(())
    }
}

fn read_field(e: &Engine, me: ObjectId, url: String) {
    if let Some(now) = item::texture_now(e, &url) {
        return take_field(e, me, &url, now.map_err(|err| err.to_string()));
    }
    let wanted = url.clone();
    item::load_texture(e, &url, move |e, got| {
        if e.is_alive(me) {
            take_field(e, me, &wanted, got.map_err(|err| err.to_string()));
        }
    });
}

fn take_field(e: &Engine, me: ObjectId, url: &str, got: Result<item::ImageHandle, String>) {
    let field = match got {
        Ok(image) => {
            let (cols, rows) = (image.width(), image.height());
            let mut heights = Vec::with_capacity((cols * rows) as usize);
            for y in 0..rows {
                for x in 0..cols {
                    let [r, g, b, _] = image.pixel(x, y);
                    let brightest = r.max(g).max(b);
                    heights.push(f32::from(brightest) / 255.0 - 0.5);
                }
            }
            Some(Rc::new(Field {
                cols: cols as usize,
                rows: rows as usize,
                heights,
            }))
        }
        Err(err) => {
            e.warn(format!("QtQuick3D.Physics: cannot read {url}: {err}"));
            None
        }
    };
    let revision = e.with_mut(me, |h: &mut HeightFieldShape| {
        h.field = field;
        h.answered = url.to_owned();
        h.revision.wrapping_add(1)
    });
    if let Ok(revision) = revision {
        let _ = e.set_internal(me, HeightFieldShape::REVISION, revision);
    }
}

/// What a shape is now.
#[derive(Debug)]
pub(crate) enum Read {
    Part(Part),
    /// Its file has not been read yet: nothing falls until it has.
    Waiting,
    /// There is nothing of it: no source, or one that could not be read.
    Nothing,
}

fn scaled(v: Vector3d, s: [f32; 3]) -> [f32; 3] {
    [v.x * s[0], v.y * s[1], v.z * s[2]]
}

/// What `shape` is, in a body scaled by `scale`.
pub(crate) fn read(e: &Engine, shape: ObjectId, scale: [f32; 3]) -> Read {
    let own = e.get(shape, Node::SCALE).unwrap_or(Vector3d {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    });
    let s = [scale[0] * own.x, scale[1] * own.y, scale[2] * own.z];
    let at = e.get(shape, Node::POSITION).unwrap_or_default();
    let turn = e.get(shape, Node::ROTATION).unwrap_or_default();
    let mut pose = Pose {
        position: scaled(at, scale),
        rotation: [turn.scalar, turn.x, turn.y, turn.z],
    };
    let shape = if e.is::<BoxShape>(shape) {
        let extents = e.get(shape, BoxShape::EXTENTS).unwrap_or(HUNDREDS);
        Shape::Box {
            half: scaled(extents, s).map(|v| v * 0.5),
        }
    } else if e.is::<SphereShape>(shape) {
        let diameter = e.get(shape, SphereShape::DIAMETER).unwrap_or(100.0) as f32;
        Shape::Sphere {
            radius: diameter * s[0] * 0.5,
        }
    } else if e.is::<CapsuleShape>(shape) {
        let diameter = e.get(shape, CapsuleShape::DIAMETER).unwrap_or(100.0) as f32;
        let height = e.get(shape, CapsuleShape::HEIGHT).unwrap_or(100.0) as f32;
        Shape::Capsule {
            radius: s[1] * diameter * 0.5,
            half_height: s[0] * height * 0.5,
        }
    } else if e.is::<PlaneShape>(shape) {
        // A plane has no size, and where it is is not scaled.
        pose.position = [at.x, at.y, at.z];
        Shape::Plane
    } else if e.is::<MeshShape>(shape) {
        match mesh(e, shape, s) {
            Ok(shape) => shape,
            Err(read) => return read,
        }
    } else if e.is::<HeightFieldShape>(shape) {
        match height_field(e, shape, s) {
            Ok(shape) => shape,
            Err(read) => return read,
        }
    } else {
        return Read::Nothing;
    };
    Read::Part(Part { shape, pose })
}

fn mesh(e: &Engine, shape: ObjectId, s: [f32; 3]) -> Result<Shape, Read> {
    let triangles = e.is::<TriangleMeshShape>(shape);
    let geometry = e.get(shape, MeshShape::GEOMETRY).ok().flatten();
    let (positions, indices) = if let Some(g) = geometry {
        e.read(g, Geometry::REVISION.index);
        let committed = e.with(g, Geometry::committed).ok().flatten();
        let Some(mesh) = committed.and_then(|c| c.to_mesh().ok()) else {
            return Err(Read::Nothing);
        };
        let indices = mesh.indices.as_chunks::<3>().0.to_vec();
        (mesh.positions, indices)
    } else {
        e.read(shape, MeshShape::REVISION.index);
        let source = e.get(shape, MeshShape::SOURCE).unwrap_or_default();
        if source.0.is_empty() {
            return Err(Read::Nothing);
        }
        let url = e.resolve_url(shape, source.0.as_str());
        let held = e
            .with(shape, |m: &MeshShape| (m.answered == url, m.file.clone()))
            .unwrap_or((false, None));
        match held {
            (true, Some(file)) => file.triangles(),
            (true, None) => return Err(Read::Nothing),
            (false, _) => return Err(Read::Waiting),
        }
    };
    let positions: Vec<_> = positions
        .into_iter()
        .map(|p| [p[0] * s[0], p[1] * s[1], p[2] * s[2]])
        .collect();
    Ok(if triangles {
        Shape::Triangles { positions, indices }
    } else {
        Shape::Convex { points: positions }
    })
}

fn height_field(e: &Engine, shape: ObjectId, s: [f32; 3]) -> Result<Shape, Read> {
    e.read(shape, HeightFieldShape::REVISION.index);
    let source = e.get(shape, HeightFieldShape::SOURCE).unwrap_or_default();
    if source.0.is_empty() {
        return Err(Read::Nothing);
    }
    let url = e.resolve_url(shape, source.0.as_str());
    let (answered, field) = e
        .with(shape, |h: &HeightFieldShape| {
            (h.answered == url, h.field.clone())
        })
        .unwrap_or((false, None));
    if !answered {
        return Err(Read::Waiting);
    }
    let Some(field) = field else {
        return Err(Read::Nothing);
    };
    let extents = e.get(shape, HeightFieldShape::EXTENTS).unwrap_or(HUNDREDS);
    let given = matches!(
        e.provenance_tracked(shape, HeightFieldShape::EXTENTS.index),
        Some(Provenance::Assigned | Provenance::Bound)
    );
    let extents = if given {
        extents
    } else {
        let longer = field.cols.max(field.rows).max(1) as f32;
        Vector3d {
            x: 100.0 * field.cols as f32 / longer,
            y: 100.0,
            z: 100.0 * field.rows as f32 / longer,
        }
    };
    Ok(Shape::HeightField {
        cols: field.cols,
        rows: field.rows,
        heights: field.heights.clone(),
        size: scaled(extents, s),
    })
}

/// The capsule of a character, standing up: what its controller moves.
pub(crate) fn upright_capsule(radius: f32, half_height: f32) -> Part {
    let half_turn = std::f32::consts::FRAC_1_SQRT_2;
    Part {
        shape: Shape::Capsule {
            radius,
            half_height,
        },
        // The shape lies along X; a quarter turn about Z stands it up.
        pose: Pose {
            position: [0.0; 3],
            rotation: [half_turn, 0.0, 0.0, half_turn],
        },
    }
}
