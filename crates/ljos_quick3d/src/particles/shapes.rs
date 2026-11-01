//! Where a particle starts: `ParticleShape3D` and its like.

use std::sync::Arc;

use ljos::item::ljos_3d::math::{Mat4, Vec3};
use ljos::item::ljos_3d::mesh::Mesh;
use ljos::item::ljos_3d::particles::{Shape, Surface};
use ljos::item::ljos_3d::scene::Primitive;
use ljos::prelude::*;

use super::floats;
use crate::mesh_source;
use crate::node::Model;
use crate::space;

/// `ParticleAbstractShape3D`: what the shapes have.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "ParticleAbstractShape3D",
    extends = QtObject,
    uncreatable
)]
pub struct ParticleAbstractShape3D {
    #[qml(base)]
    pub object: QtObject,
}

fn shape_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "ShapeType",
        vec![("Cube", 0), ("Sphere", 1), ("Cylinder", 2)],
    )]
}

/// `ParticleShape3D`: a box, a ball or a tube, `extents` from its middle to
/// its sides.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "ParticleShape3D",
    extends = ParticleAbstractShape3D,
    enums = shape_enums
)]
pub struct ParticleShape3D {
    #[qml(base)]
    pub shape: ParticleAbstractShape3D,
    #[qml(notify)]
    pub fill: bool,
    #[qml(notify, rename = "type", enumeration = "ShapeType")]
    pub kind: i32,
    #[qml(notify)]
    pub extents: Vector3d,
}

impl Default for ParticleShape3D {
    fn default() -> Self {
        ParticleShape3D {
            shape: ParticleAbstractShape3D::default(),
            fill: true,
            kind: 0,
            extents: Vector3d {
                x: 50.0,
                y: 50.0,
                z: 50.0,
            },
        }
    }
}

/// `ParticleModelShape3D`: the surface of a model, or anywhere between it
/// and its middle. The model is made from `delegate` and is not drawn.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "ParticleModelShape3D",
    extends = ParticleAbstractShape3D
)]
pub struct ParticleModelShape3D {
    #[qml(base)]
    pub shape: ParticleAbstractShape3D,
    #[qml(notify)]
    pub fill: bool,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(skip)]
    surface: Option<(Arc<Surface>, Mat4)>,
    #[qml(skip)]
    made: Option<ObjectId>,
}

impl Default for ParticleModelShape3D {
    fn default() -> Self {
        ParticleModelShape3D {
            shape: ParticleAbstractShape3D::default(),
            fill: true,
            delegate: None,
            surface: None,
            made: None,
        }
    }
}

const CATEGORY: &str = "qt.quick3d.particles";

#[qml_impl]
impl ParticleModelShape3D {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.effect_in(me, move |e| {
            let component = e.get(me, ParticleModelShape3D::DELEGATE).ok().flatten();
            e.untrack(|| make(e, me, component));
        })?;
        Ok(())
    }
}

fn make(e: &Engine, me: ObjectId, component: Option<ObjectId>) {
    let old = e
        .with_mut(me, |s: &mut ParticleModelShape3D| {
            s.surface = None;
            s.made.take()
        })
        .ok()
        .flatten();
    if let Some(old) = old {
        let _ = e.destroy(old);
    }
    let Some(component) = component else {
        return;
    };
    // Owned by the shape, which is no node: so the model is in no scene.
    let model = match e.component_create_owned(component, me, |_, _| {}) {
        Ok(model) => model,
        Err(why) => {
            e.log(
                LogLevel::Warning,
                CATEGORY,
                format!("ParticleModelShape3D: {why}"),
            );
            return;
        }
    };
    let _ = e.with_mut(me, |s: &mut ParticleModelShape3D| s.made = Some(model));
    if !e.is::<Model>(model) {
        return;
    }
    let place = space::place(e, model).world;
    let source = e.read(model, Model::SOURCE.index).to_js_string();
    if let Some(primitive) = Primitive::from_source(&source) {
        let mesh = Mesh::primitive(primitive);
        let corners: Vec<Vec3> = mesh
            .positions
            .iter()
            .map(|p| Vec3::new(p[0], p[1], p[2]))
            .collect();
        keep(e, me, Surface::new(&corners, Some(&mesh.indices)), place);
        return;
    }
    if source.is_empty() {
        return;
    }
    let url = e.resolve_url(model, &source);
    mesh_source::load(e, &url, move |e, file| match file {
        Ok(file) => {
            let (corners, triangles) = file.triangles();
            let corners: Vec<Vec3> = corners
                .iter()
                .map(|p| Vec3::new(p[0], p[1], p[2]))
                .collect();
            let indices: Vec<u32> = triangles.iter().flatten().copied().collect();
            if e.is_alive(me) {
                keep(e, me, Surface::new(&corners, Some(&indices)), place);
            }
        }
        Err(why) => e.log(LogLevel::Warning, CATEGORY, format!("{source}: {why}")),
    });
}

fn keep(e: &Engine, me: ObjectId, surface: Surface, place: Mat4) {
    let _ = e.with_mut(me, |s: &mut ParticleModelShape3D| {
        s.surface = Some((Arc::new(surface), place));
    });
    let _ = e.notify(me, ParticleModelShape3D::FILL.index);
}

/// `ParticleCustomShape3D`: registered so a scene with one loads; it reads no
/// file, and its particles start where the emitter is.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "ParticleCustomShape3D",
    extends = ParticleAbstractShape3D
)]
pub struct ParticleCustomShape3D {
    #[qml(base)]
    pub shape: ParticleAbstractShape3D,
    #[qml(notify)]
    pub source: Url,
    #[qml(notify)]
    pub randomize_data: bool,
}

/// `ParticleSceneShape3D`: registered so a scene with one loads; it does not
/// look at the scene, and its particles start where the emitter is.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "ParticleSceneShape3D",
    extends = ParticleAbstractShape3D
)]
pub struct ParticleSceneShape3D {
    #[qml(base)]
    pub shape: ParticleAbstractShape3D,
    #[qml(notify, object = "Node")]
    pub scene: Option<ObjectId>,
    #[qml(notify)]
    pub scene_center: Vector3d,
    #[qml(notify)]
    pub scene_extents: Vector3d,
    #[qml(notify)]
    pub shape_resolution: f64,
    #[qml(notify, list)]
    pub excluded_nodes: QmlList,
    #[qml(notify, object = "Geometry")]
    pub geometry: Option<ObjectId>,
}

impl Default for ParticleSceneShape3D {
    fn default() -> Self {
        ParticleSceneShape3D {
            shape: ParticleAbstractShape3D::default(),
            scene: None,
            scene_center: Vector3d::default(),
            scene_extents: Vector3d::default(),
            shape_resolution: 10.0,
            excluded_nodes: QmlList::default(),
            geometry: None,
        }
    }
}

/// What the shape `obj` is now. Tracked.
pub(super) fn read(e: &Engine, obj: Option<ObjectId>) -> Shape {
    let Some(obj) = obj else {
        return Shape::Point;
    };
    if e.is::<ParticleShape3D>(obj) {
        let fill = e.get(obj, ParticleShape3D::FILL).unwrap_or(true);
        let extents = floats(e.get(obj, ParticleShape3D::EXTENTS).unwrap_or_default());
        return match e.get(obj, ParticleShape3D::KIND).unwrap_or(0) {
            1 => Shape::Sphere { extents, fill },
            2 => Shape::Cylinder { extents, fill },
            _ => Shape::Cube { extents, fill },
        };
    }
    if e.is::<ParticleModelShape3D>(obj) {
        let fill = e.get(obj, ParticleModelShape3D::FILL).unwrap_or(true);
        let found = e
            .with(obj, |s: &ParticleModelShape3D| s.surface.clone())
            .ok()
            .flatten();
        if let Some((surface, place)) = found {
            return Shape::Mesh {
                surface,
                place,
                fill,
            };
        }
    }
    Shape::Point
}
