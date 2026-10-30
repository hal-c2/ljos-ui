//! The scene's objects: `Object3D`, `Node` and the nodes and resources
//! derived from them.

use std::sync::Arc;

use ljos::item::ljos_3d::math::{Quat, Vec3};
use ljos::item::ljos_3d::{ComponentType, CustomGeometry, PrimitiveType, Semantic};
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
#[qml(module = "QtQuick3D", name = "Node", extends = Object3D, enums = node_enums)]
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
    /// Where the node is in the scene: its own place through its parents'
    /// transforms. This and the five below are made when first read
    /// (`space.rs`), so a node nobody asks holds no binding for them.
    #[qml(notify, readonly)]
    pub scene_position: Vector3d,
    #[qml(notify, readonly)]
    pub scene_rotation: Quaternion,
    #[qml(notify, readonly)]
    pub scene_scale: Vector3d,
    /// Unit length, in scene space: where the node's -Z, +Y and +X point.
    #[qml(notify, readonly)]
    pub forward: Vector3d,
    #[qml(notify, readonly)]
    pub up: Vector3d,
    #[qml(notify, readonly)]
    pub right: Vector3d,
}

fn node_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "TransformSpace",
        vec![("LocalSpace", 0), ("ParentSpace", 1), ("SceneSpace", 2)],
    )]
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
            scene_position: Vector3d::default(),
            scene_rotation: IDENTITY,
            scene_scale: Vector3d {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            forward: Vector3d {
                x: 0.0,
                y: 0.0,
                z: -1.0,
            },
            up: Vector3d {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
            right: Vector3d {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
        }
    }
}

pub(crate) const IDENTITY: Quaternion = Quaternion {
    scalar: 1.0,
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

pub(crate) fn host_write<T: QmlValue>(e: &Engine, me: ObjectId, p: Prop<T>, v: T) {
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
    /// A file (not drawn yet), or a built-in mesh: `#Cube`, `#Rectangle`,
    /// `#Sphere`, `#Cylinder` or `#Cone`.
    #[qml(notify)]
    pub source: Url,
    #[qml(notify, list)]
    pub materials: QmlList,
    /// An application's own geometry, drawn instead of `source`'s mesh.
    #[qml(notify, object = "Geometry")]
    pub geometry: Option<ObjectId>,
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
            geometry: None,
            casts_shadows: true,
            receives_shadows: true,
            pickable: false,
        }
    }
}

/// `Geometry`: vertices an application provides, as `QQuick3DGeometry`
/// is in Qt. An application's type embeds it as its base, sets the data
/// from Rust and calls [`Geometry::update`] to have a `Model` whose
/// `geometry` it is draw the new data:
///
/// ```ignore
/// #[derive(Default, QmlObject)]
/// #[qml(module = "App", name = "Triangle", extends = Geometry)]
/// struct Triangle {
///     #[qml(base)]
///     geometry: Geometry,
/// }
///
/// let (e, t) = (&engine, triangle_id);
/// e.with_mut(t, |t: &mut Triangle| {
///     let g = &mut t.geometry;
///     g.set_vertex_data(bytes);
///     g.set_stride(12);
///     g.add_attribute(Semantic::Position, 0, ComponentType::F32);
/// })?;
/// Geometry::update(e, t);
/// ```
///
/// Vertex data is little endian. Without a normal attribute the faces
/// are flat. Triangles, strips and fans are drawn; lines and points are
/// not yet.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Geometry", extends = Object3D, uncreatable)]
pub struct Geometry {
    #[qml(base)]
    pub object3d: Object3D,
    /// How many times the data was committed: what a model's drawing
    /// tracks.
    #[qml(notify, readonly)]
    pub revision: i32,
    #[qml(skip)]
    data: CustomGeometry,
    /// The data as last committed, which the scene shares with the
    /// renderer without copying it again.
    #[qml(skip)]
    snapshot: Option<Arc<CustomGeometry>>,
}

impl Geometry {
    pub fn set_vertex_data(&mut self, data: Vec<u8>) {
        self.data.set_vertex_data(data);
    }

    pub fn set_index_data(&mut self, data: Vec<u8>) {
        self.data.set_index_data(data);
    }

    pub fn set_stride(&mut self, stride: usize) {
        self.data.set_stride(stride);
    }

    pub fn add_attribute(&mut self, semantic: Semantic, offset: usize, component: ComponentType) {
        self.data.add_attribute(semantic, offset, component);
    }

    pub fn set_primitive_type(&mut self, primitive: PrimitiveType) {
        self.data.set_primitive_type(primitive);
    }

    /// The box the geometry lies in; culling uses it. Without it the
    /// positions are searched.
    pub fn set_bounds(&mut self, min: Vec3, max: Vec3) {
        self.data.set_bounds(min, max);
    }

    /// Removes the data, the attributes and the bounds. Nothing is drawn
    /// differently until [`Geometry::update`].
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Commits what was set to the geometry `obj` (an object whose base
    /// is a `Geometry`): the models that use it are drawn again. A
    /// geometry that cannot be drawn is reported and draws nothing.
    pub fn update(e: &Engine, obj: ObjectId) {
        let committed = e.with_mut(obj, |g: &mut Geometry| {
            g.data.update();
            g.snapshot = Some(Arc::new(g.data.clone()));
            g.revision = g.revision.wrapping_add(1);
            g.data.validate()
        });
        match committed {
            Ok(checked) => {
                if let Err(err) = checked {
                    e.warn(format!("QtQuick3D Geometry: {err}"));
                }
                if let Err(err) = e.notify(obj, Geometry::REVISION.index) {
                    e.warn(format!("QtQuick3D Geometry: {err}"));
                }
            }
            Err(err) => e.warn(format!("QtQuick3D Geometry: {err}")),
        }
    }

    /// The data as last committed; `None` before the first update.
    pub(crate) fn committed(&self) -> Option<Arc<CustomGeometry>> {
        self.snapshot.clone()
    }
}

/// `Camera`: what a view is seen through. It looks down its local -Z axis.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Camera", extends = Node, uncreatable)]
pub struct Camera {
    #[qml(base)]
    pub node: Node,
    /// Leaves out the models wholly outside what the camera sees; off by
    /// default, as in Qt.
    #[qml(notify)]
    pub frustum_culling_enabled: bool,
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

pub(crate) mod cull {
    pub const BACK: i32 = 0;
    pub const FRONT: i32 = 1;
    pub const NONE: i32 = 2;
}

fn material_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "CullMode",
        vec![
            ("BackFaceCulling", i64::from(cull::BACK)),
            ("FrontFaceCulling", i64::from(cull::FRONT)),
            ("NoCulling", i64::from(cull::NONE)),
        ],
    )]
}

/// `Material`: what every material has.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "Material",
    extends = Object3D,
    uncreatable,
    enums = material_enums
)]
pub struct Material {
    #[qml(base)]
    pub object3d: Object3D,
    /// Which faces are left out: a face is a front face where it winds
    /// counter-clockwise on the screen.
    #[qml(notify, enumeration = "CullMode")]
    pub cull_mode: i32,
}

/// `PointLight`: light from one place in every direction, fading with
/// the distance.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "PointLight", extends = Light)]
pub struct PointLight {
    #[qml(base)]
    pub light: Light,
    #[qml(notify)]
    pub constant_fade: f64,
    /// Multiplied by 0.01 before use.
    #[qml(notify)]
    pub linear_fade: f64,
    /// Multiplied by 0.0001 before use.
    #[qml(notify)]
    pub quadratic_fade: f64,
}

impl Default for PointLight {
    fn default() -> Self {
        PointLight {
            light: Light::default(),
            constant_fade: 1.0,
            linear_fade: 0.0,
            quadratic_fade: 1.0,
        }
    }
}

/// `SpotLight`: a point light that shines only inside a cone along its
/// own axis.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "SpotLight", extends = Light)]
pub struct SpotLight {
    #[qml(base)]
    pub light: Light,
    #[qml(notify)]
    pub constant_fade: f64,
    #[qml(notify)]
    pub linear_fade: f64,
    #[qml(notify)]
    pub quadratic_fade: f64,
    /// Degrees, edge to edge.
    #[qml(notify)]
    pub cone_angle: f64,
    /// Degrees; where the light starts to fall off towards the cone's edge.
    #[qml(notify)]
    pub inner_cone_angle: f64,
}

impl Default for SpotLight {
    fn default() -> Self {
        SpotLight {
            light: Light::default(),
            constant_fade: 1.0,
            linear_fade: 0.0,
            quadratic_fade: 1.0,
            cone_angle: 40.0,
            inner_cone_angle: 30.0,
        }
    }
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
/// `lighting` is `NoLighting`, with a white highlight of
/// `specularAmount`.
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
    /// 0 to 1.
    #[qml(notify)]
    pub specular_amount: f64,
    /// 0 to 1: how far the highlight spreads.
    #[qml(notify)]
    pub specular_roughness: f64,
}

impl Default for DefaultMaterial {
    fn default() -> Self {
        DefaultMaterial {
            material: Material::default(),
            diffuse_color: Color::WHITE,
            lighting: lighting::FRAGMENT_LIGHTING,
            specular_amount: 0.0,
            specular_roughness: 0.0,
        }
    }
}

pub(crate) mod alpha {
    pub const DEFAULT: i32 = 0;
    pub const BLEND: i32 = 1;
    pub const OPAQUE: i32 = 2;
    pub const MASK: i32 = 3;
}

fn principled_material_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "Lighting",
            vec![
                ("NoLighting", i64::from(lighting::NO_LIGHTING)),
                ("FragmentLighting", i64::from(lighting::FRAGMENT_LIGHTING)),
            ],
        ),
        (
            "AlphaMode",
            vec![
                ("Default", i64::from(alpha::DEFAULT)),
                ("Blend", i64::from(alpha::BLEND)),
                ("Opaque", i64::from(alpha::OPAQUE)),
                ("Mask", i64::from(alpha::MASK)),
            ],
        ),
    ]
}

/// `PrincipledMaterial`: the metal-roughness model.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "PrincipledMaterial",
    extends = Material,
    enums = principled_material_enums
)]
pub struct PrincipledMaterial {
    #[qml(base)]
    pub material: Material,
    /// Qt does not document a default; white, as `DefaultMaterial`'s
    /// diffuse colour.
    #[qml(notify)]
    pub base_color: Color,
    /// 0 a dielectric, 1 a metal.
    #[qml(notify)]
    pub metalness: f64,
    /// 0 to 1.
    #[qml(notify)]
    pub roughness: f64,
    /// Light the surface gives by itself, red, green and blue.
    #[qml(notify)]
    pub emissive_factor: Vector3d,
    /// Qt does not document a default; 1, wholly opaque.
    #[qml(notify)]
    pub opacity: f64,
    #[qml(notify, enumeration = "AlphaMode")]
    pub alpha_mode: i32,
    /// Where `Mask` cuts: below it nothing is drawn.
    #[qml(notify)]
    pub alpha_cutoff: f64,
    #[qml(notify, enumeration = "Lighting")]
    pub lighting: i32,
}

impl Default for PrincipledMaterial {
    fn default() -> Self {
        PrincipledMaterial {
            material: Material::default(),
            base_color: Color::WHITE,
            metalness: 0.0,
            roughness: 0.0,
            emissive_factor: Vector3d::default(),
            opacity: 1.0,
            alpha_mode: alpha::DEFAULT,
            alpha_cutoff: 0.5,
            lighting: lighting::FRAGMENT_LIGHTING,
        }
    }
}

pub(crate) mod antialiasing {
    pub const MSAA: i32 = 2;
    pub const MEDIUM: i32 = 0;
    pub const HIGH: i32 = 1;
    pub const VERY_HIGH: i32 = 2;
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
            vec![
                ("NoAA", 0),
                ("SSAA", 1),
                ("MSAA", i64::from(antialiasing::MSAA)),
                ("ProgressiveAA", 3),
            ],
        ),
        (
            "AntialiasingQuality",
            vec![
                ("Medium", i64::from(antialiasing::MEDIUM)),
                ("High", i64::from(antialiasing::HIGH)),
                ("VeryHigh", i64::from(antialiasing::VERY_HIGH)),
            ],
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
    /// MSAA is drawn; SSAA and ProgressiveAA are not built and draw
    /// as NoAA.
    #[qml(notify, enumeration = "AntialiasingMode")]
    pub antialiasing_mode: i32,
    /// For MSAA: 2, 4 or 8 samples a pixel, or as near as the device has.
    #[qml(notify, enumeration = "AntialiasingQuality")]
    pub antialiasing_quality: i32,
}

impl Default for SceneEnvironment {
    fn default() -> Self {
        SceneEnvironment {
            object3d: Object3D::default(),
            clear_color: Color::BLACK,
            background_mode: background::TRANSPARENT,
            tonemap_mode: 1,
            antialiasing_mode: 0,
            antialiasing_quality: antialiasing::HIGH,
        }
    }
}
