//! Where a node is in the scene, and the mappings between spaces: a
//! node's own, the scene's, and a camera's viewport.
//!
//! A node's parent is the node that owns it, as a node declared in
//! another is its child (and a `Repeater3D`'s nodes are its children).
//! The transforms are read, and so tracked, all the way up: a binding on
//! `scenePosition` follows every parent's move.

use ljos::item::ljos_3d::math::{Mat4, Quat, Vec3};
use ljos::item::ljos_3d::pick::Viewport;
use ljos::prelude::*;

use crate::node::{IDENTITY, host_write, quat, vec3};
use crate::{Camera, Node, View3D};

/// A node's transform in scene space.
#[derive(Clone, Copy)]
pub(crate) struct Place {
    pub world: Mat4,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Place {
    const SCENE: Place = Place {
        world: Mat4::IDENTITY,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };
}

/// The node that owns `obj`, if a node does: its parent in the scene.
pub(crate) fn parent_node(e: &Engine, obj: ObjectId) -> Option<ObjectId> {
    e.owner(obj).filter(|p| e.is::<Node>(*p))
}

/// `obj`'s place in the scene; the scene's own for an object that is not
/// a node. Tracked.
pub(crate) fn place(e: &Engine, obj: ObjectId) -> Place {
    if !e.is::<Node>(obj) {
        return Place::SCENE;
    }
    let parent = parent_node(e, obj).map_or(Place::SCENE, |p| place(e, p));
    let position = vec3(e.get(obj, Node::POSITION).unwrap_or_default());
    let rotation = quat(e.get(obj, Node::ROTATION).unwrap_or(IDENTITY));
    let scale = vec3(e.get(obj, Node::SCALE).unwrap_or(ONE));
    Place {
        world: parent.world * Mat4::from_trs(position, rotation, scale),
        rotation: parent.rotation * rotation,
        scale: Vec3::new(
            parent.scale.x * scale.x,
            parent.scale.y * scale.y,
            parent.scale.z * scale.z,
        ),
    }
}

const ONE: Vector3d = Vector3d {
    x: 1.0,
    y: 1.0,
    z: 1.0,
};

pub(crate) fn to_value(v: Vec3) -> Value {
    Value::Vector3d(Vector3d {
        x: v.x,
        y: v.y,
        z: v.z,
    })
}

/// A `vector3d` argument; anything else is the origin.
pub(crate) fn from_value(v: &Value) -> Vec3 {
    match v {
        Value::Vector3d(v) => vec3(*v),
        _ => Vec3::ZERO,
    }
}

fn quaternion(q: Quat) -> Quaternion {
    Quaternion {
        scalar: q.w,
        x: q.x,
        y: q.y,
        z: q.z,
    }
}

/// The scene-space properties, and how each is read off a [`Place`].
const SCENE_PROPS: [PropIndex; 6] = [
    Node::SCENE_POSITION.index,
    Node::SCENE_ROTATION.index,
    Node::SCENE_SCALE.index,
    Node::FORWARD.index,
    Node::UP.index,
    Node::RIGHT.index,
];

const SCENE_PARTS: [fn(&Place) -> Value; 6] = [
    |p| to_value(p.world.translation()),
    |p| Value::Quaternion(std::sync::Arc::new(quaternion(p.rotation))),
    |p| to_value(p.scale),
    |p| to_value(p.rotation.rotate(Vec3::new(0.0, 0.0, -1.0))),
    |p| to_value(p.rotation.rotate(Vec3::new(0.0, 1.0, 0.0))),
    |p| to_value(p.rotation.rotate(Vec3::new(1.0, 0.0, 0.0))),
];

/// Binds the scene-space properties of the node `obj` when each is first
/// read: most nodes are never asked where they are.
fn bind_scene_properties(e: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    for prop in SCENE_PROPS {
        e.bind_on_read(obj, prop, |e, obj, prop| {
            let i = SCENE_PROPS.iter().position(|p| *p == prop).unwrap_or(0);
            let make = SCENE_PARTS[i];
            e.bind_initial(obj, prop, move |e| make(&place(e, obj)))
        })?;
    }
    Ok(())
}

fn inverse(m: Mat4) -> Mat4 {
    m.inverse().unwrap_or(Mat4::IDENTITY)
}

#[qml_impl]
impl Node {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        bind_scene_properties(cx.engine, cx.this)
    }

    #[qml(invokable, name = "mapPositionToScene")]
    fn map_position_to_scene(cx: &Cx, local: Value) -> Value {
        let world = place(cx.engine, cx.this).world;
        to_value(world.transform_point(from_value(&local)))
    }

    #[qml(invokable, name = "mapPositionFromScene")]
    fn map_position_from_scene(cx: &Cx, scene: Value) -> Value {
        let world = place(cx.engine, cx.this).world;
        to_value(inverse(world).transform_point(from_value(&scene)))
    }

    /// Into `node`'s space; the scene's when `node` is null.
    #[qml(invokable, name = "mapPositionToNode")]
    fn map_position_to_node(cx: &Cx, node: Option<ObjectId>, local: Value) -> Value {
        let e = cx.engine;
        let to = node.map_or(Mat4::IDENTITY, |n| inverse(place(e, n).world));
        to_value((to * place(e, cx.this).world).transform_point(from_value(&local)))
    }

    #[qml(invokable, name = "mapPositionFromNode")]
    fn map_position_from_node(cx: &Cx, node: Option<ObjectId>, position: Value) -> Value {
        let e = cx.engine;
        let from = node.map_or(Mat4::IDENTITY, |n| place(e, n).world);
        let to = inverse(place(e, cx.this).world);
        to_value((to * from).transform_point(from_value(&position)))
    }

    /// Turned only: a direction is not moved by a node's place or
    /// stretched by its scale, and keeps its length.
    #[qml(invokable, name = "mapDirectionToScene")]
    fn map_direction_to_scene(cx: &Cx, local: Value) -> Value {
        let turn = place(cx.engine, cx.this).rotation;
        to_value(turn.rotate(from_value(&local)))
    }

    #[qml(invokable, name = "mapDirectionFromScene")]
    fn map_direction_from_scene(cx: &Cx, scene: Value) -> Value {
        let turn = place(cx.engine, cx.this).rotation;
        to_value(turn.conjugate().rotate(from_value(&scene)))
    }

    #[qml(invokable, name = "mapDirectionToNode")]
    fn map_direction_to_node(cx: &Cx, node: Option<ObjectId>, local: Value) -> Value {
        let e = cx.engine;
        let to = node.map_or(Quat::IDENTITY, |n| place(e, n).rotation.conjugate());
        to_value(to.rotate(place(e, cx.this).rotation.rotate(from_value(&local))))
    }

    #[qml(invokable, name = "mapDirectionFromNode")]
    fn map_direction_from_node(cx: &Cx, node: Option<ObjectId>, direction: Value) -> Value {
        let e = cx.engine;
        let from = node.map_or(Quat::IDENTITY, |n| place(e, n).rotation);
        let to = place(e, cx.this).rotation.conjugate();
        to_value(to.rotate(from.rotate(from_value(&direction))))
    }

    /// Adds a turn of `degrees` about `axis`, an axis of this node's own
    /// space, its parent's or the scene's.
    #[qml(invokable)]
    fn rotate(cx: &Cx, degrees: f64, axis: Value, space: i32) {
        let (e, me) = (cx.engine, cx.this);
        let axis = from_value(&axis);
        let Ok(current) = e.get(me, Node::ROTATION) else {
            return;
        };
        let current = quat(current);
        let axis = match space {
            // Turns are composed in the parent's space: a scene axis is
            // turned back by what the parents turn, a local one by what
            // the node turns.
            2 => {
                let parent = parent_node(e, me).map_or(Quat::IDENTITY, |p| place(e, p).rotation);
                parent.conjugate().rotate(axis)
            }
            0 => current.rotate(axis),
            _ => axis,
        };
        let turn = Quat::from_axis_angle(axis, (degrees as f32).to_radians());
        host_write(e, me, Node::ROTATION, quaternion(turn * current));
    }
}

/// The view a camera is declared in, walking up its owners.
fn view_of(e: &Engine, camera: ObjectId) -> Option<ObjectId> {
    let mut at = e.owner(camera);
    while let Some(o) = at {
        if e.is::<View3D>(o) {
            return Some(o);
        }
        at = e.owner(o);
    }
    None
}

/// The camera `me` over the view it is declared in. A camera outside any
/// view maps onto a square viewport one unit across.
fn viewport(e: &Engine, me: ObjectId) -> Option<Viewport> {
    let camera = crate::scene::camera(e, me)?;
    let (w, h) = view_of(e, me).map_or((1.0, 1.0), |v| crate::view::size(e, v));
    Viewport::new(&camera, place(e, me).world, w, h)
}

#[qml_impl]
impl Camera {
    /// A scene position in the viewport: 0 to 1 from its top left, and the
    /// distance past the near clip plane. The origin where it cannot be
    /// mapped.
    #[qml(invokable, name = "mapToViewport")]
    fn map_to_viewport(cx: &Cx, scene: Value) -> Value {
        let Some(v) = viewport(cx.engine, cx.this) else {
            return to_value(Vec3::ZERO);
        };
        let (w, h) = v.size();
        let at = v.from_scene(from_value(&scene));
        to_value(at.map_or(Vec3::ZERO, |p| Vec3::new(p.x / w, p.y / h, p.z)))
    }

    #[qml(invokable, name = "mapFromViewport")]
    fn map_from_viewport(cx: &Cx, viewport_position: Value) -> Value {
        let Some(v) = viewport(cx.engine, cx.this) else {
            return to_value(Vec3::ZERO);
        };
        let (w, h) = v.size();
        let p = from_value(&viewport_position);
        to_value(
            v.to_scene(Vec3::new(p.x * w, p.y * h, p.z))
                .unwrap_or(Vec3::ZERO),
        )
    }

    /// Turns the camera to look at a node or a scene position, keeping
    /// its up towards the scene's +Y.
    #[qml(invokable, name = "lookAt")]
    fn look_at(cx: &Cx, target: Value) {
        let (e, me) = (cx.engine, cx.this);
        let at = match &target {
            Value::Object(o) => place(e, *o).world.translation(),
            v => from_value(v),
        };
        let parent = parent_node(e, me).map_or(Place::SCENE, |p| place(e, p));
        let from = place(e, me).world.translation();
        let d = parent.rotation.conjugate().rotate(at - from);
        if d.length() <= f32::EPSILON {
            return;
        }
        let yaw = (-d.x).atan2(-d.z).to_degrees();
        let pitch = d.y.atan2((d.x * d.x + d.z * d.z).sqrt()).to_degrees();
        host_write(
            e,
            me,
            Node::EULER_ROTATION,
            Vector3d {
                x: pitch,
                y: yaw,
                z: 0.0,
            },
        );
    }
}
