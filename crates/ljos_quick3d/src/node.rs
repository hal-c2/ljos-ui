//! The scene's objects: `Object3D`, `Node` and the nodes and resources
//! derived from them.

use ljos::item::ljos_3d::math::{Quat, Vec3};
use ljos::ljos_engine::WriteKind;
use ljos::prelude::*;

/// `Object3D`: what every object in a scene is. Its default property
/// holds the objects declared in it: a node's child nodes.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "Object3D",
    extends = QtObject,
    default = "data",
    uncreatable
)]
pub struct Object3D {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, list)]
    pub data: QmlList,
}

/// `Node`: a place in the scene, relative to its parent node.
///
/// `x`, `y` and `z` are `position`'s parts and `eulerRotation` is
/// `rotation` in degrees, as in Qt: writing one sets the other. The
/// counterpart is written as the toolkit's own value
/// ([`WriteKind::Host`]), so a binding on it stays.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "Node", extends = Object3D)]
pub struct Node {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify, change = x_changed)]
    pub x: f64,
    #[qml(notify, change = y_changed)]
    pub y: f64,
    #[qml(notify, change = z_changed)]
    pub z: f64,
    #[qml(notify, change = position_changed)]
    pub position: Vector3d,
    #[qml(notify, change = rotation_changed)]
    pub rotation: Quaternion,
    #[qml(notify, change = euler_changed)]
    pub euler_rotation: Vector3d,
    #[qml(notify)]
    pub scale: Vector3d,
    /// Kept; the drawing does not use it yet.
    #[qml(notify)]
    pub pivot: Vector3d,
    /// Kept; the drawing does not use it yet.
    #[qml(notify)]
    pub opacity: f64,
    #[qml(notify)]
    pub visible: bool,
}

impl Default for Node {
    fn default() -> Self {
        Node {
            object3d: Object3D::default(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            position: Vector3d::default(),
            rotation: IDENTITY,
            euler_rotation: Vector3d::default(),
            scale: Vector3d {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            pivot: Vector3d::default(),
            opacity: 1.0,
            visible: true,
        }
    }
}

const IDENTITY: Quaternion = Quaternion {
    scalar: 1.0,
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

fn host_write<T: QmlValue>(e: &Engine, me: ObjectId, p: Prop<T>, v: T) {
    if let Err(err) = e.write(me, p.index, v.to_value(), WriteKind::Host) {
        e.warn(format!("QtQuick3D Node: {err}"));
    }
}

// One axis at a time: `position` writing `x` must not see the `y` it has
// not written yet and write an old `position` back.
fn axis_changed(
    cx: &Cx<'_>,
    axis: Prop<f64>,
    set: fn(&mut Vector3d, f32),
    get: fn(&Vector3d) -> f32,
) {
    let (e, me) = (cx.engine, cx.this);
    let (Ok(v), Ok(mut p)) = (e.get(me, axis), e.get(me, Node::POSITION)) else {
        return;
    };
    let v = v as f32;
    if get(&p) != v {
        set(&mut p, v);
        host_write(e, me, Node::POSITION, p);
    }
}

fn x_changed(cx: &Cx<'_>, _: PropIndex) {
    axis_changed(cx, Node::X, |p, v| p.x = v, |p| p.x);
}

fn y_changed(cx: &Cx<'_>, _: PropIndex) {
    axis_changed(cx, Node::Y, |p, v| p.y = v, |p| p.y);
}

fn z_changed(cx: &Cx<'_>, _: PropIndex) {
    axis_changed(cx, Node::Z, |p, v| p.z = v, |p| p.z);
}

fn position_changed(cx: &Cx<'_>, _: PropIndex) {
    let (e, me) = (cx.engine, cx.this);
    let Ok(p) = e.get(me, Node::POSITION) else {
        return;
    };
    for (axis, v) in [(Node::X, p.x), (Node::Y, p.y), (Node::Z, p.z)] {
        if e.get(me, axis).is_ok_and(|a| a as f32 != v) {
            host_write(e, me, axis, f64::from(v));
        }
    }
}

pub(crate) fn quat(q: Quaternion) -> Quat {
    let q = Quat {
        x: q.x,
        y: q.y,
        z: q.z,
        w: q.scalar,
    };
    let len = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt();
    if len > f32::EPSILON {
        Quat {
            x: q.x / len,
            y: q.y / len,
            z: q.z / len,
            w: q.w / len,
        }
    } else {
        Quat::IDENTITY
    }
}

pub(crate) fn vec3(v: Vector3d) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

// Euler angles name a rotation many ways (and a quaternion and its
// negation are the same turn), so the two are compared as rotations: one
// side's change must not write the other back in a form of its own.
fn same_turn(a: Quat, b: Quat) -> bool {
    (a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w).abs() > 1.0 - 1e-6
}

fn rotation_changed(cx: &Cx<'_>, _: PropIndex) {
    let (e, me) = (cx.engine, cx.this);
    let (Ok(q), Ok(euler)) = (e.get(me, Node::ROTATION), e.get(me, Node::EULER_ROTATION)) else {
        return;
    };
    let q = quat(q);
    if !same_turn(q, Quat::from_euler_degrees(vec3(euler))) {
        let d = q.to_euler_degrees();
        host_write(
            e,
            me,
            Node::EULER_ROTATION,
            Vector3d {
                x: d.x,
                y: d.y,
                z: d.z,
            },
        );
    }
}

fn euler_changed(cx: &Cx<'_>, _: PropIndex) {
    let (e, me) = (cx.engine, cx.this);
    let (Ok(q), Ok(euler)) = (e.get(me, Node::ROTATION), e.get(me, Node::EULER_ROTATION)) else {
        return;
    };
    let turn = Quat::from_euler_degrees(vec3(euler));
    if !same_turn(quat(q), turn) {
        host_write(
            e,
            me,
            Node::ROTATION,
            Quaternion {
                scalar: turn.w,
                x: turn.x,
                y: turn.y,
                z: turn.z,
            },
        );
    }
}

/// `Model`: a mesh drawn with its materials.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "Model", extends = Node)]
pub struct Model {
    #[qml(base)]
    pub node: Node,
    /// A file, or a built-in mesh: `#Cube`, `#Rectangle` (and, not drawn
    /// yet, `#Sphere`, `#Cylinder`, `#Cone`).
    #[qml(notify)]
    pub source: Url,
    #[qml(notify, list)]
    pub materials: QmlList,
    #[qml(notify)]
    pub casts_shadows: bool,
    #[qml(notify)]
    pub receives_shadows: bool,
    #[qml(notify)]
    pub pickable: bool,
}

impl Default for Model {
    fn default() -> Self {
        Model {
            node: Node::default(),
            source: Url::default(),
            materials: QmlList::default(),
            casts_shadows: true,
            receives_shadows: true,
            pickable: false,
        }
    }
}

/// `Camera`: what a view is seen through. It looks down its local -Z axis.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Camera", extends = Node, uncreatable)]
pub struct Camera {
    #[qml(base)]
    pub node: Node,
}

fn perspective_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "FieldOfViewOrientation",
        vec![("Vertical", 0), ("Horizontal", 1)],
    )]
}

/// `PerspectiveCamera`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "PerspectiveCamera",
    extends = Camera,
    enums = perspective_enums
)]
pub struct PerspectiveCamera {
    #[qml(base)]
    pub camera: Camera,
    /// Degrees.
    #[qml(notify)]
    pub field_of_view: f64,
    #[qml(notify, enumeration = "FieldOfViewOrientation")]
    pub field_of_view_orientation: i32,
    #[qml(notify)]
    pub clip_near: f64,
    #[qml(notify)]
    pub clip_far: f64,
}

impl Default for PerspectiveCamera {
    fn default() -> Self {
        PerspectiveCamera {
            camera: Camera::default(),
            field_of_view: 60.0,
            field_of_view_orientation: 0,
            clip_near: 10.0,
            clip_far: 10000.0,
        }
    }
}

/// `OrthographicCamera`: at a magnification of 1, one scene unit is one
/// pixel of the view.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "OrthographicCamera", extends = Camera)]
pub struct OrthographicCamera {
    #[qml(base)]
    pub camera: Camera,
    #[qml(notify)]
    pub clip_near: f64,
    #[qml(notify)]
    pub clip_far: f64,
    #[qml(notify)]
    pub horizontal_magnification: f64,
    #[qml(notify)]
    pub vertical_magnification: f64,
}

impl Default for OrthographicCamera {
    fn default() -> Self {
        OrthographicCamera {
            camera: Camera::default(),
            clip_near: 10.0,
            clip_far: 10000.0,
            horizontal_magnification: 1.0,
            vertical_magnification: 1.0,
        }
    }
}

/// `Light`: what every light has.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "Light", extends = Node, uncreatable)]
pub struct Light {
    #[qml(base)]
    pub node: Node,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify)]
    pub ambient_color: Color,
    #[qml(notify)]
    pub brightness: f64,
    /// Kept; shadows are M5.
    #[qml(notify)]
    pub casts_shadow: bool,
}

impl Default for Light {
    fn default() -> Self {
        Light {
            node: Node::default(),
            color: Color::WHITE,
            ambient_color: Color::BLACK,
            brightness: 1.0,
            casts_shadow: false,
        }
    }
}

/// `DirectionalLight`: light from far away, along the light's own axis.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "DirectionalLight", extends = Light)]
pub struct DirectionalLight {
    #[qml(base)]
    pub light: Light,
}

/// `Material`: what every material has.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Material", extends = Object3D, uncreatable)]
pub struct Material {
    #[qml(base)]
    pub object3d: Object3D,
}

pub(crate) mod lighting {
    pub const NO_LIGHTING: i32 = 0;
    pub const FRAGMENT_LIGHTING: i32 = 1;
}

fn default_material_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Lighting",
        vec![
            ("NoLighting", i64::from(lighting::NO_LIGHTING)),
            ("FragmentLighting", i64::from(lighting::FRAGMENT_LIGHTING)),
        ],
    )]
}

/// `DefaultMaterial`: a diffuse colour, lit by the scene's lights unless
/// `lighting` is `NoLighting`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "DefaultMaterial",
    extends = Material,
    enums = default_material_enums
)]
pub struct DefaultMaterial {
    #[qml(base)]
    pub material: Material,
    #[qml(notify)]
    pub diffuse_color: Color,
    #[qml(notify, enumeration = "Lighting")]
    pub lighting: i32,
}

impl Default for DefaultMaterial {
    fn default() -> Self {
        DefaultMaterial {
            material: Material::default(),
            diffuse_color: Color::WHITE,
            lighting: lighting::FRAGMENT_LIGHTING,
        }
    }
}

pub(crate) mod background {
    pub const TRANSPARENT: i32 = 0;
    pub const COLOR: i32 = 1;
}

fn environment_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "BackgroundMode",
            vec![
                ("Transparent", i64::from(background::TRANSPARENT)),
                ("Color", i64::from(background::COLOR)),
                ("SkyBox", 2),
                ("SkyBoxCubeMap", 3),
                ("SkyMaterial", 4),
            ],
        ),
        (
            "TonemapMode",
            vec![
                ("TonemapModeNone", 0),
                ("TonemapModeLinear", 1),
                ("TonemapModeAces", 2),
                ("TonemapModeHejlDawson", 3),
                ("TonemapModeFilmic", 4),
                ("TonemapModeCustom", 5),
            ],
        ),
        (
            "AntialiasingMode",
            vec![("NoAA", 0), ("SSAA", 1), ("MSAA", 2), ("ProgressiveAA", 3)],
        ),
    ]
}

/// `SceneEnvironment`: what is behind the scene and how it is finished.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "SceneEnvironment",
    extends = Object3D,
    enums = environment_enums
)]
pub struct SceneEnvironment {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify)]
    pub clear_color: Color,
    /// Transparent and Color are drawn; the sky boxes are M4.
    #[qml(notify, enumeration = "BackgroundMode")]
    pub background_mode: i32,
    /// Kept; tone mapping is M3.
    #[qml(notify, enumeration = "TonemapMode")]
    pub tonemap_mode: i32,
    /// Kept; anti-aliasing is M3.
    #[qml(notify, enumeration = "AntialiasingMode")]
    pub antialiasing_mode: i32,
}

impl Default for SceneEnvironment {
    fn default() -> Self {
        SceneEnvironment {
            object3d: Object3D::default(),
            clear_color: Color::BLACK,
            background_mode: background::TRANSPARENT,
            tonemap_mode: 1,
            antialiasing_mode: 0,
        }
    }
}
