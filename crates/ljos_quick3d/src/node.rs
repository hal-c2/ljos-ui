//! The scene's objects: `Object3D`, `Node` and the nodes and resources
//! derived from them.

use std::cell::RefCell;
use std::sync::Arc;

use ljos::item::ljos_3d::math::{Quat, Vec3};
use ljos::item::ljos_3d::{ComponentType, CustomGeometry, PrimitiveType, Semantic};
use ljos::ljos_engine::WriteKind;
use ljos::prelude::*;

use ljos::item::ljos_3d::qml::defaults as d;
pub(crate) use ljos::item::ljos_3d::qml::enums::{
    alpha, antialiasing, background, blend, channel, cull, depth_draw, fov_orientation, lighting,
    morph, reflection_probe, shadow_map, soft_shadow, tonemap,
};

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
    #[qml(notify, list, append = crate::items::data_append)]
    pub data: QmlList,
    /// The object this one is in: the one it was declared in until it is
    /// set. Setting it moves the object into the new parent's `data`
    /// (`space.rs`).
    #[qml(notify, object = "Object3D", change = crate::space::parent_written)]
    pub parent: Option<ObjectId>,
    /// The object's states, as an item's: `State`s whose changes apply to it,
    /// the one it is in and the transitions between them.
    #[qml(notify, list)]
    pub states: QmlList,
    #[qml(notify)]
    pub state: Atom,
    #[qml(notify, list)]
    pub transitions: QmlList,
    /// Whose `data` holds it, once it completed; `parent` before that is
    /// only what was declared.
    #[qml(skip)]
    pub(crate) held: Option<Held>,
}

/// Where a completed object is held. `None` inside: set to no parent.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Held(pub(crate) Option<ObjectId>);

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
    /// The point in the node's own space it scales and turns about, and
    /// that `position` places.
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
            scale: vector(d::node::SCALE),
            pivot: Vector3d::default(),
            opacity: d::node::OPACITY,
            visible: d::node::VISIBLE,
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

pub fn host_write<T: QmlValue>(e: &Engine, me: ObjectId, p: Prop<T>, v: T) {
    if let Err(err) = e.write(me, p.index, v.to_value(), WriteKind::Host) {
        e.warn(format!("QtQuick3D Node: {err}"));
    }
}

thread_local! {
    /// The nodes whose `position` is being written into its parts.
    static SPLITTING: RefCell<Vec<ObjectId>> = const { RefCell::new(Vec::new()) };
}

fn splitting(me: ObjectId) -> bool {
    SPLITTING.with_borrow(|s| s.contains(&me))
}

// A part changed: the parts are the truth, all three of them. Taking the
// other two from `position` would lose a part whose binding changed with
// this one and has not been written into `position` yet. While `position`
// is being split into its parts it is the truth instead, and its parts not
// yet written must not write their old values back into it.
fn axis_changed(cx: &Cx<'_>) {
    let (e, me) = (cx.engine, cx.this);
    if splitting(me) {
        return;
    }
    let (Ok(x), Ok(y), Ok(z), Ok(p)) = (
        e.get(me, Node::X),
        e.get(me, Node::Y),
        e.get(me, Node::Z),
        e.get(me, Node::POSITION),
    ) else {
        return;
    };
    let parts = Vector3d {
        x: x as f32,
        y: y as f32,
        z: z as f32,
    };
    if parts != p {
        host_write(e, me, Node::POSITION, parts);
    }
}

fn x_changed(cx: &Cx<'_>, _: PropIndex) {
    axis_changed(cx);
}

fn y_changed(cx: &Cx<'_>, _: PropIndex) {
    axis_changed(cx);
}

fn z_changed(cx: &Cx<'_>, _: PropIndex) {
    axis_changed(cx);
}

fn position_changed(cx: &Cx<'_>, _: PropIndex) {
    let (e, me) = (cx.engine, cx.this);
    let Ok(p) = e.get(me, Node::POSITION) else {
        return;
    };
    SPLITTING.with_borrow_mut(|s| s.push(me));
    for (axis, v) in [(Node::X, p.x), (Node::Y, p.y), (Node::Z, p.z)] {
        if e.get(me, axis).is_ok_and(|a| a as f32 != v) {
            host_write(e, me, axis, f64::from(v));
        }
    }
    SPLITTING.with_borrow_mut(|s| {
        if let Some(i) = s.iter().rposition(|n| *n == me) {
            s.swap_remove(i);
        }
    });
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

/// A default of [`d`] as the member's value.
pub(crate) fn vector([x, y, z]: [f32; 3]) -> Vector3d {
    Vector3d { x, y, z }
}

/// A default colour of [`d`] as the member's value.
pub(crate) fn rgba([r, g, b, a]: [u8; 4]) -> Color {
    Color::from_rgba8(r, g, b, a)
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
    /// A `.mesh` file, read in the background, or a built-in mesh: `#Cube`,
    /// `#Rectangle`, `#Sphere`, `#Cylinder` or `#Cone`.
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
    /// The table of copies drawn in place of the model itself.
    #[qml(notify, object = "Instancing")]
    pub instancing: Option<ObjectId>,
    /// The node whose space the copies' entries are in; the model itself
    /// when unset.
    #[qml(notify, object = "Node")]
    pub instance_root: Option<ObjectId>,
    /// The joints that bend the mesh; wins over `skeleton`.
    #[qml(notify, object = "Skin")]
    pub skin: Option<ObjectId>,
    #[qml(notify, object = "Skeleton")]
    pub skeleton: Option<ObjectId>,
    /// `skeleton`'s inverse bind poses, as `Qt.matrix4x4` values.
    #[qml(notify)]
    pub inverse_bind_poses: Value,
    /// Added to the model's distance from the camera, which orders what is
    /// blended.
    #[qml(notify)]
    pub depth_bias: f64,
    /// What a file's mesh morphs by; each target's `weight` says how far.
    #[qml(notify, list)]
    pub morph_targets: QmlList,
    /// Kept: `ReflectionProbe`s do not capture yet (see docs/quick3d.md).
    #[qml(notify)]
    pub casts_reflections: bool,
    #[qml(notify)]
    pub receives_reflections: bool,
    /// Kept: a mesh's levels of detail are not read.
    #[qml(notify)]
    pub level_of_detail_bias: f64,
    #[qml(skip)]
    pub(crate) mesh: RefCell<crate::mesh_source::Loaded>,
}

impl Default for Model {
    fn default() -> Self {
        Model {
            node: Node::default(),
            source: Url::default(),
            materials: QmlList::default(),
            geometry: None,
            casts_shadows: d::model::CASTS_SHADOWS,
            receives_shadows: d::model::RECEIVES_SHADOWS,
            pickable: false,
            instancing: None,
            instance_root: None,
            skin: None,
            skeleton: None,
            inverse_bind_poses: Value::Undefined,
            depth_bias: d::model::DEPTH_BIAS,
            morph_targets: QmlList::default(),
            casts_reflections: d::model::CASTS_REFLECTIONS,
            receives_reflections: d::model::RECEIVES_REFLECTIONS,
            level_of_detail_bias: d::model::LEVEL_OF_DETAIL_BIAS,
            mesh: RefCell::default(),
        }
    }
}

fn morph_target_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Attributes",
        vec![
            ("Position", i64::from(morph::POSITION)),
            ("Normal", i64::from(morph::NORMAL)),
            ("Tangent", i64::from(morph::TANGENT)),
            ("Binormal", i64::from(morph::BINORMAL)),
            ("TexCoord0", i64::from(morph::TEX_COORD_0)),
            ("TexCoord1", i64::from(morph::TEX_COORD_1)),
            ("Color", i64::from(morph::COLOR)),
        ],
    )]
}

/// `MorphTarget`: one of a mesh's shapes, and how far towards it the model is.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "MorphTarget",
    extends = Object3D,
    enums = morph_target_enums
)]
pub struct MorphTarget {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify)]
    pub weight: f64,
    /// Which attributes it moves, flags or-ed together.
    #[qml(notify)]
    pub attributes: i32,
}

impl Default for MorphTarget {
    fn default() -> Self {
        MorphTarget {
            object3d: Object3D::default(),
            weight: d::morph_target::WEIGHT,
            attributes: d::morph_target::ATTRIBUTES,
        }
    }
}

fn reflection_probe_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "ReflectionQuality",
            vec![
                ("VeryLow", i64::from(reflection_probe::VERY_LOW)),
                ("Low", i64::from(reflection_probe::LOW)),
                ("Medium", i64::from(reflection_probe::MEDIUM)),
                ("High", i64::from(reflection_probe::HIGH)),
                ("VeryHigh", i64::from(reflection_probe::VERY_HIGH)),
            ],
        ),
        (
            "ReflectionRefreshMode",
            vec![
                ("FirstFrame", i64::from(reflection_probe::FIRST_FRAME)),
                ("EveryFrame", i64::from(reflection_probe::EVERY_FRAME)),
            ],
        ),
        (
            "ReflectionTimeSlicing",
            vec![
                ("None", i64::from(reflection_probe::NO_TIME_SLICING)),
                (
                    "AllFacesAtOnce",
                    i64::from(reflection_probe::ALL_FACES_AT_ONCE),
                ),
                (
                    "IndividualFaces",
                    i64::from(reflection_probe::INDIVIDUAL_FACES),
                ),
            ],
        ),
    ]
}

/// `ReflectionProbe`: every member is kept, nothing is captured yet: models
/// keep lighting by the environment's light probe (docs/quick3d.md).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "ReflectionProbe",
    extends = Node,
    enums = reflection_probe_enums
)]
pub struct ReflectionProbe {
    #[qml(base)]
    pub node: Node,
    #[qml(notify, enumeration = "ReflectionQuality")]
    pub quality: i32,
    #[qml(notify)]
    pub clear_color: Color,
    #[qml(notify, enumeration = "ReflectionRefreshMode")]
    pub refresh_mode: i32,
    #[qml(notify, enumeration = "ReflectionTimeSlicing")]
    pub time_slicing: i32,
    #[qml(notify)]
    pub parallax_correction: bool,
    #[qml(notify)]
    pub box_size: Vector3d,
    #[qml(notify)]
    pub box_offset: Vector3d,
    #[qml(notify)]
    pub debug_view: bool,
    #[qml(notify, object = "Texture")]
    pub texture: Option<ObjectId>,
}

impl Default for ReflectionProbe {
    fn default() -> Self {
        use d::reflection_probe as r;
        ReflectionProbe {
            node: Node::default(),
            quality: r::QUALITY,
            clear_color: rgba(r::CLEAR_COLOR),
            refresh_mode: r::REFRESH_MODE,
            time_slicing: r::TIME_SLICING,
            parallax_correction: r::PARALLAX_CORRECTION,
            box_size: vector(r::BOX_SIZE),
            box_offset: vector(r::BOX_OFFSET),
            debug_view: r::DEBUG_VIEW,
            texture: None,
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
///
/// QML can declare one and call the same functions, which Qt's QML cannot:
/// what stands in for an application's C++ subclass has its data to give.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Geometry", extends = Object3D, enums = geometry_enums)]
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

    /// This geometry's data, committed: for a mesh that belongs to an
    /// object that is no `Geometry` itself, and keeps what it gets.
    pub(crate) fn into_snapshot(mut self) -> Arc<CustomGeometry> {
        self.data.update();
        Arc::new(self.data)
    }

    /// The data as last committed; `None` before the first update.
    pub fn committed(&self) -> Option<Arc<CustomGeometry>> {
        self.snapshot.clone()
    }
}

/// The enums of `QQuick3DGeometry` Qt documents, as many as are drawn here;
/// `QQuick3DGeometry::Attribute`'s are named on `Geometry` too, as QML has no
/// nested class to name them on.
fn geometry_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "PrimitiveType",
            vec![
                ("Points", 0),
                ("LineStrip", 1),
                ("Lines", 2),
                ("TriangleStrip", 3),
                ("TriangleFan", 4),
                ("Triangles", 5),
            ],
        ),
        (
            "Semantic",
            vec![
                ("IndexSemantic", 0),
                ("PositionSemantic", 1),
                ("NormalSemantic", 2),
                ("TexCoord0Semantic", 3),
                ("TexCoordSemantic", 3),
                ("TexCoord1Semantic", 4),
                ("TangentSemantic", 5),
                ("BinormalSemantic", 6),
                ("ColorSemantic", 7),
                ("JointSemantic", 8),
                ("WeightSemantic", 9),
            ],
        ),
        (
            "ComponentType",
            vec![("U16Type", 0), ("U32Type", 1), ("F32Type", 2)],
        ),
    ]
}

fn primitive_of(value: i32) -> Option<PrimitiveType> {
    Some(match value {
        0 => PrimitiveType::Points,
        1 => PrimitiveType::LineStrip,
        2 => PrimitiveType::Lines,
        3 => PrimitiveType::TriangleStrip,
        4 => PrimitiveType::TriangleFan,
        5 => PrimitiveType::Triangles,
        _ => return None,
    })
}

fn semantic_of(value: i32) -> Option<Semantic> {
    Some(match value {
        0 => Semantic::Index,
        1 => Semantic::Position,
        2 => Semantic::Normal,
        3 => Semantic::TexCoord0,
        4 => Semantic::TexCoord1,
        5 => Semantic::Tangent,
        6 => Semantic::Binormal,
        7 => Semantic::Color,
        8 => Semantic::Joint,
        9 => Semantic::Weight,
        _ => return None,
    })
}

fn component_of(value: i32) -> Option<ComponentType> {
    Some(match value {
        0 => ComponentType::U16,
        1 => ComponentType::U32,
        2 => ComponentType::F32,
        _ => return None,
    })
}

#[qml_impl]
impl Geometry {
    #[qml(invokable, name = "clear")]
    fn clear_in_qml(cx: &Cx) {
        let _ = cx.engine.with_mut(cx.this, Geometry::clear);
    }

    #[qml(invokable, name = "setVertexData")]
    fn set_vertex_data_in_qml(cx: &Cx, data: Value) {
        let bytes = cx.engine.js_bytes(&data).unwrap_or_default();
        let _ = cx
            .engine
            .with_mut(cx.this, |g: &mut Geometry| g.set_vertex_data(bytes));
    }

    #[qml(invokable, name = "setIndexData")]
    fn set_index_data_in_qml(cx: &Cx, data: Value) {
        let bytes = cx.engine.js_bytes(&data).unwrap_or_default();
        let _ = cx
            .engine
            .with_mut(cx.this, |g: &mut Geometry| g.set_index_data(bytes));
    }

    #[qml(invokable, name = "setStride")]
    fn set_stride_in_qml(cx: &Cx, stride: i32) {
        let stride = usize::try_from(stride).unwrap_or(0);
        let _ = cx
            .engine
            .with_mut(cx.this, |g: &mut Geometry| g.set_stride(stride));
    }

    #[qml(invokable, name = "setBounds")]
    fn set_bounds_in_qml(cx: &Cx, min: Value, max: Value) {
        let (min, max) = (
            crate::space::from_value(&min),
            crate::space::from_value(&max),
        );
        let _ = cx
            .engine
            .with_mut(cx.this, |g: &mut Geometry| g.set_bounds(min, max));
    }

    #[qml(invokable, name = "setPrimitiveType")]
    fn set_primitive_type_in_qml(cx: &Cx, primitive: i32) {
        let Some(primitive) = primitive_of(primitive) else {
            return cx.engine.warn(format!(
                "QtQuick3D Geometry: {primitive} is not a primitive type"
            ));
        };
        let _ = cx
            .engine
            .with_mut(cx.this, |g: &mut Geometry| g.set_primitive_type(primitive));
    }

    #[qml(invokable, name = "addAttribute")]
    fn add_attribute_in_qml(cx: &Cx, semantic: i32, offset: i32, component: i32) {
        let (Some(semantic), Some(component)) = (semantic_of(semantic), component_of(component))
        else {
            return cx.engine.warn(format!(
                "QtQuick3D Geometry: no attribute of semantic {semantic} and component type {component}"
            ));
        };
        let offset = usize::try_from(offset).unwrap_or(0);
        let _ = cx.engine.with_mut(cx.this, |g: &mut Geometry| {
            g.add_attribute(semantic, offset, component)
        });
    }

    #[qml(invokable, name = "update")]
    fn update_in_qml(cx: &Cx) {
        Geometry::update(cx.engine, cx.this);
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
        vec![
            ("Vertical", i64::from(fov_orientation::VERTICAL)),
            ("Horizontal", i64::from(fov_orientation::HORIZONTAL)),
        ],
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
            field_of_view: d::perspective_camera::FIELD_OF_VIEW,
            field_of_view_orientation: d::perspective_camera::FIELD_OF_VIEW_ORIENTATION,
            clip_near: d::perspective_camera::CLIP_NEAR,
            clip_far: d::perspective_camera::CLIP_FAR,
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
            clip_near: d::orthographic_camera::CLIP_NEAR,
            clip_far: d::orthographic_camera::CLIP_FAR,
            horizontal_magnification: d::orthographic_camera::HORIZONTAL_MAGNIFICATION,
            vertical_magnification: d::orthographic_camera::VERTICAL_MAGNIFICATION,
        }
    }
}

fn light_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "ShadowMapQuality",
            vec![
                ("ShadowMapQualityLow", i64::from(shadow_map::LOW)),
                ("ShadowMapQualityMedium", i64::from(shadow_map::MEDIUM)),
                ("ShadowMapQualityHigh", i64::from(shadow_map::HIGH)),
                ("ShadowMapQualityVeryHigh", i64::from(shadow_map::VERY_HIGH)),
                ("ShadowMapQualityUltra", i64::from(shadow_map::ULTRA)),
            ],
        ),
        (
            "SoftShadowQuality",
            vec![
                ("Hard", i64::from(soft_shadow::HARD)),
                ("PCF4", i64::from(soft_shadow::PCF4)),
                ("PCF8", i64::from(soft_shadow::PCF8)),
                ("PCF16", i64::from(soft_shadow::PCF16)),
                ("PCF32", i64::from(soft_shadow::PCF32)),
                ("PCF64", i64::from(soft_shadow::PCF64)),
            ],
        ),
    ]
}

/// `Light`: what every light has.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "Light",
    extends = Node,
    uncreatable,
    enums = light_enums
)]
pub struct Light {
    #[qml(base)]
    pub node: Node,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify)]
    pub ambient_color: Color,
    #[qml(notify)]
    pub brightness: f64,
    #[qml(notify)]
    pub casts_shadow: bool,
    /// 0 to 100: how much of the light a shadow keeps away.
    #[qml(notify)]
    pub shadow_factor: f64,
    /// Scene units a surface is moved towards the light before it is
    /// compared with the shadow map.
    #[qml(notify)]
    pub shadow_bias: f64,
    #[qml(notify, enumeration = "ShadowMapQuality")]
    pub shadow_map_quality: i32,
    #[qml(notify)]
    pub shadow_map_far: f64,
    #[qml(notify, enumeration = "SoftShadowQuality")]
    pub soft_shadow_quality: i32,
    #[qml(notify)]
    pub pcf_factor: f64,
    /// Kept for compatibility; the documentation says it has no effect.
    #[qml(notify)]
    pub shadow_filter: f64,
    /// Kept; the maps are always 32-bit depth.
    #[qml(notify)]
    pub use32_bit_shadowmap: bool,
    /// Lights, and adds its ambient to, only this node and what is in it.
    #[qml(notify, object = "Node")]
    pub scope: Option<ObjectId>,
}

impl Default for Light {
    fn default() -> Self {
        Light {
            node: Node::default(),
            color: rgba(d::light::COLOR),
            ambient_color: rgba(d::light::AMBIENT_COLOR),
            brightness: d::light::BRIGHTNESS,
            casts_shadow: d::light::CASTS_SHADOW,
            shadow_factor: d::light::SHADOW_FACTOR,
            shadow_bias: d::light::SHADOW_BIAS,
            shadow_map_quality: d::light::SHADOW_MAP_QUALITY,
            shadow_map_far: d::light::SHADOW_MAP_FAR,
            soft_shadow_quality: d::light::SOFT_SHADOW_QUALITY,
            pcf_factor: d::light::PCF_FACTOR,
            shadow_filter: d::light::SHADOW_FILTER,
            use32_bit_shadowmap: false,
            scope: None,
        }
    }
}

/// `DirectionalLight`: light from far away, along the light's own axis.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "DirectionalLight", extends = Light)]
pub struct DirectionalLight {
    #[qml(base)]
    pub light: Light,
    /// 0 to 3: how many times the shadow map is split by distance from
    /// the camera.
    #[qml(notify)]
    pub csm_num_splits: i32,
    #[qml(notify)]
    pub csm_split1: f64,
    #[qml(notify)]
    pub csm_split2: f64,
    #[qml(notify)]
    pub csm_split3: f64,
    #[qml(notify)]
    pub csm_blend_ratio: f64,
    #[qml(notify)]
    pub lock_shadowmap_texels: bool,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        DirectionalLight {
            light: Light::default(),
            csm_num_splits: d::directional_light::CSM_NUM_SPLITS,
            csm_split1: d::directional_light::CSM_SPLIT1,
            csm_split2: d::directional_light::CSM_SPLIT2,
            csm_split3: d::directional_light::CSM_SPLIT3,
            csm_blend_ratio: d::directional_light::CSM_BLEND_RATIO,
            lock_shadowmap_texels: d::directional_light::LOCK_SHADOWMAP_TEXELS,
        }
    }
}

fn material_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "CullMode",
            vec![
                ("BackFaceCulling", i64::from(cull::BACK)),
                ("FrontFaceCulling", i64::from(cull::FRONT)),
                ("NoCulling", i64::from(cull::NONE)),
            ],
        ),
        (
            "DepthDrawMode",
            vec![
                ("OpaqueOnlyDepthDraw", i64::from(depth_draw::OPAQUE_ONLY)),
                ("AlwaysDepthDraw", i64::from(depth_draw::ALWAYS)),
                ("NeverDepthDraw", i64::from(depth_draw::NEVER)),
                (
                    "OpaquePrePassDepthDraw",
                    i64::from(depth_draw::OPAQUE_PRE_PASS),
                ),
            ],
        ),
        (
            "TextureChannelMapping",
            vec![
                ("R", i64::from(channel::R)),
                ("G", i64::from(channel::G)),
                ("B", i64::from(channel::B)),
                ("A", i64::from(channel::A)),
            ],
        ),
    ]
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
    /// Whether the material writes depth. A CustomMaterial follows it;
    /// the built-in materials keep their own rule (an opaque one writes,
    /// a blended one does not).
    #[qml(notify, enumeration = "DepthDrawMode")]
    pub depth_draw_mode: i32,
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
            constant_fade: d::fade::CONSTANT,
            linear_fade: d::fade::LINEAR,
            quadratic_fade: d::fade::QUADRATIC,
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
            constant_fade: d::fade::CONSTANT,
            linear_fade: d::fade::LINEAR,
            quadratic_fade: d::fade::QUADRATIC,
            cone_angle: d::spot_light::CONE_ANGLE,
            inner_cone_angle: d::spot_light::INNER_CONE_ANGLE,
        }
    }
}

fn blend_modes() -> (&'static str, Vec<(&'static str, i64)>) {
    (
        "BlendMode",
        vec![
            ("SourceOver", i64::from(blend::SOURCE_OVER)),
            ("Screen", i64::from(blend::SCREEN)),
            ("Multiply", i64::from(blend::MULTIPLY)),
        ],
    )
}

fn default_material_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "Lighting",
            vec![
                ("NoLighting", i64::from(lighting::NO_LIGHTING)),
                ("FragmentLighting", i64::from(lighting::FRAGMENT_LIGHTING)),
            ],
        ),
        blend_modes(),
        ("SpecularModel", vec![("Default", 0), ("KGGX", 1)]),
    ]
}

/// `DefaultMaterial`: a diffuse colour, lit by the scene's lights unless
/// `lighting` is `NoLighting`, with a highlight of `specularAmount` and
/// `specularTint`.
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
    /// Multiplies `diffuseColor`.
    #[qml(notify, object = "Texture")]
    pub diffuse_map: Option<ObjectId>,
    #[qml(notify, enumeration = "Lighting")]
    pub lighting: i32,
    /// 0 to 1.
    #[qml(notify)]
    pub specular_amount: f64,
    /// 0 to 1: how far the highlight spreads.
    #[qml(notify)]
    pub specular_roughness: f64,
    /// Multiplies `diffuseColor`'s alpha.
    #[qml(notify)]
    pub opacity: f64,
    #[qml(notify, object = "Texture")]
    pub opacity_map: Option<ObjectId>,
    #[qml(notify, enumeration = "TextureChannelMapping")]
    pub opacity_channel: i32,
    #[qml(notify)]
    pub emissive_factor: Vector3d,
    #[qml(notify, object = "Texture")]
    pub emissive_map: Option<ObjectId>,
    #[qml(notify, object = "Texture")]
    pub normal_map: Option<ObjectId>,
    #[qml(notify, object = "Texture")]
    pub roughness_map: Option<ObjectId>,
    #[qml(notify, object = "Texture")]
    pub specular_map: Option<ObjectId>,
    #[qml(notify)]
    pub specular_tint: Color,
    /// 0 has the highlight the same from every side.
    #[qml(notify)]
    pub fresnel_power: f64,
    #[qml(notify, enumeration = "BlendMode")]
    pub blend_mode: i32,
    #[qml(notify)]
    pub vertex_colors_enabled: bool,
    /// Kept: this model's highlight has one shape.
    #[qml(notify, enumeration = "SpecularModel")]
    pub specular_model: i32,
    /// Kept: `specularTint` and `fresnelPower` make the highlight here.
    #[qml(notify)]
    pub index_of_refraction: f64,
    /// Kept: the environment's probe does not light a `DefaultMaterial`.
    #[qml(notify, object = "Texture")]
    pub specular_reflection_map: Option<ObjectId>,
    /// Kept: normals come from `normalMap`.
    #[qml(notify, object = "Texture")]
    pub bump_map: Option<ObjectId>,
    #[qml(notify)]
    pub bump_amount: f64,
    /// Kept: no light passes through a `DefaultMaterial`.
    #[qml(notify, object = "Texture")]
    pub translucency_map: Option<ObjectId>,
    #[qml(notify)]
    pub translucent_falloff: f64,
    /// Kept: light stops at the edge of the lit side.
    #[qml(notify)]
    pub diffuse_light_wrap: f64,
    /// Kept, as lines and points are not drawn wider than one pixel.
    #[qml(notify)]
    pub point_size: f64,
    #[qml(notify)]
    pub line_width: f64,
}

impl Default for DefaultMaterial {
    fn default() -> Self {
        DefaultMaterial {
            material: Material::default(),
            diffuse_color: rgba(d::default_material::DIFFUSE_COLOR),
            diffuse_map: None,
            lighting: d::default_material::LIGHTING,
            specular_amount: d::default_material::SPECULAR_AMOUNT,
            specular_roughness: d::default_material::SPECULAR_ROUGHNESS,
            opacity: d::default_material::OPACITY,
            opacity_map: None,
            opacity_channel: d::default_material::OPACITY_CHANNEL,
            emissive_factor: vector(d::default_material::EMISSIVE_FACTOR),
            emissive_map: None,
            normal_map: None,
            roughness_map: None,
            specular_map: None,
            specular_tint: rgba(d::default_material::SPECULAR_TINT),
            fresnel_power: d::default_material::FRESNEL_POWER,
            blend_mode: d::default_material::BLEND_MODE,
            vertex_colors_enabled: d::default_material::VERTEX_COLORS_ENABLED,
            specular_model: d::default_material::SPECULAR_MODEL,
            index_of_refraction: d::default_material::INDEX_OF_REFRACTION,
            specular_reflection_map: None,
            bump_map: None,
            bump_amount: d::default_material::BUMP_AMOUNT,
            translucency_map: None,
            translucent_falloff: d::default_material::TRANSLUCENT_FALLOFF,
            diffuse_light_wrap: d::default_material::DIFFUSE_LIGHT_WRAP,
            point_size: d::default_material::POINT_SIZE,
            line_width: d::default_material::LINE_WIDTH,
        }
    }
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
        blend_modes(),
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
    /// Multiplies `baseColor`, its alpha the opacity.
    #[qml(notify, object = "Texture")]
    pub base_color_map: Option<ObjectId>,
    #[qml(notify, object = "Texture")]
    pub metalness_map: Option<ObjectId>,
    #[qml(notify, enumeration = "TextureChannelMapping")]
    pub metalness_channel: i32,
    #[qml(notify, object = "Texture")]
    pub roughness_map: Option<ObjectId>,
    #[qml(notify, enumeration = "TextureChannelMapping")]
    pub roughness_channel: i32,
    /// Tangent-space normals.
    #[qml(notify, object = "Texture")]
    pub normal_map: Option<ObjectId>,
    /// 0 keeps the mesh's normals, 1 takes the map's.
    #[qml(notify)]
    pub normal_strength: f64,
    /// Multiplies `emissiveFactor`.
    #[qml(notify, object = "Texture")]
    pub emissive_map: Option<ObjectId>,
    /// Shades the light the light probe gives.
    #[qml(notify, object = "Texture")]
    pub occlusion_map: Option<ObjectId>,
    #[qml(notify, enumeration = "TextureChannelMapping")]
    pub occlusion_channel: i32,
    #[qml(notify)]
    pub occlusion_amount: f64,
    #[qml(notify, object = "Texture")]
    pub opacity_map: Option<ObjectId>,
    #[qml(notify, enumeration = "TextureChannelMapping")]
    pub opacity_channel: i32,
    #[qml(notify, enumeration = "BlendMode")]
    pub blend_mode: i32,
    #[qml(notify)]
    pub vertex_colors_enabled: bool,
    /// How much of a dielectric's reflection there is; metals keep theirs.
    #[qml(notify)]
    pub specular_amount: f64,
    /// 0 reflects white, 1 the base colour.
    #[qml(notify)]
    pub specular_tint: f64,
    /// What a dielectric reflects head on comes from it.
    #[qml(notify)]
    pub index_of_refraction: f64,
    /// How fast the reflection grows towards the edges.
    #[qml(notify)]
    pub fresnel_power: f64,
    #[qml(notify)]
    pub fresnel_scale_bias_enabled: bool,
    #[qml(notify)]
    pub fresnel_scale: f64,
    #[qml(notify)]
    pub fresnel_bias: f64,
    /// A second, dielectric layer over the surface.
    #[qml(notify)]
    pub clearcoat_amount: f64,
    #[qml(notify)]
    pub clearcoat_roughness_amount: f64,
    #[qml(notify)]
    pub clearcoat_fresnel_power: f64,
    #[qml(notify)]
    pub clearcoat_fresnel_scale_bias_enabled: bool,
    #[qml(notify)]
    pub clearcoat_fresnel_scale: f64,
    #[qml(notify)]
    pub clearcoat_fresnel_bias: f64,
    /// How much of the background shows through the diffuse light; not bent.
    #[qml(notify)]
    pub transmission_factor: f64,
    /// Multiplies `transmissionFactor`.
    #[qml(notify, object = "Texture")]
    pub transmission_map: Option<ObjectId>,
    #[qml(notify, enumeration = "TextureChannelMapping")]
    pub transmission_channel: i32,
    /// Moves what the other maps read, so that the surface looks carved.
    #[qml(notify, object = "Texture")]
    pub height_map: Option<ObjectId>,
    #[qml(notify)]
    pub height_amount: f64,
    #[qml(notify, enumeration = "TextureChannelMapping")]
    pub height_channel: i32,
    #[qml(notify)]
    pub min_height_map_samples: i32,
    #[qml(notify)]
    pub max_height_map_samples: i32,
    /// Lights this material in place of the environment's probe.
    #[qml(notify, object = "Texture")]
    pub light_probe: Option<ObjectId>,
    /// Kept, as the rest of this block: a fragment shader has only so many samplers. See docs/quick3d.md.
    #[qml(notify, object = "Texture")]
    pub specular_map: Option<ObjectId>,
    #[qml(notify, object = "Texture")]
    pub specular_reflection_map: Option<ObjectId>,
    #[qml(notify, object = "Texture")]
    pub clearcoat_map: Option<ObjectId>,
    #[qml(notify, enumeration = "TextureChannelMapping")]
    pub clearcoat_channel: i32,
    #[qml(notify, object = "Texture")]
    pub clearcoat_roughness_map: Option<ObjectId>,
    #[qml(notify, enumeration = "TextureChannelMapping")]
    pub clearcoat_roughness_channel: i32,
    #[qml(notify, object = "Texture")]
    pub clearcoat_normal_map: Option<ObjectId>,
    #[qml(notify)]
    pub clearcoat_normal_strength: f64,
    #[qml(notify, object = "Texture")]
    pub thickness_map: Option<ObjectId>,
    #[qml(notify, enumeration = "TextureChannelMapping")]
    pub thickness_channel: i32,
    #[qml(notify)]
    pub thickness_factor: f64,
    #[qml(notify)]
    pub attenuation_distance: f64,
    #[qml(notify)]
    pub attenuation_color: Color,
    #[qml(notify)]
    pub point_size: f64,
    #[qml(notify)]
    pub line_width: f64,
}

impl Default for PrincipledMaterial {
    fn default() -> Self {
        PrincipledMaterial {
            material: Material::default(),
            base_color: rgba(d::principled_material::BASE_COLOR),
            metalness: d::principled_material::METALNESS,
            roughness: d::principled_material::ROUGHNESS,
            emissive_factor: vector(d::principled_material::EMISSIVE_FACTOR),
            opacity: d::principled_material::OPACITY,
            alpha_mode: d::principled_material::ALPHA_MODE,
            alpha_cutoff: d::principled_material::ALPHA_CUTOFF,
            lighting: d::principled_material::LIGHTING,
            base_color_map: None,
            metalness_map: None,
            metalness_channel: d::principled_material::METALNESS_CHANNEL,
            roughness_map: None,
            roughness_channel: d::principled_material::ROUGHNESS_CHANNEL,
            normal_map: None,
            normal_strength: d::principled_material::NORMAL_STRENGTH,
            emissive_map: None,
            occlusion_map: None,
            occlusion_channel: d::principled_material::OCCLUSION_CHANNEL,
            occlusion_amount: d::principled_material::OCCLUSION_AMOUNT,
            opacity_map: None,
            opacity_channel: d::principled_material::OPACITY_CHANNEL,
            blend_mode: d::principled_material::BLEND_MODE,
            vertex_colors_enabled: d::principled_material::VERTEX_COLORS_ENABLED,
            specular_amount: d::principled_material::SPECULAR_AMOUNT,
            specular_tint: d::principled_material::SPECULAR_TINT,
            index_of_refraction: d::principled_material::INDEX_OF_REFRACTION,
            fresnel_power: d::principled_material::FRESNEL_POWER,
            fresnel_scale_bias_enabled: d::principled_material::FRESNEL_SCALE_BIAS_ENABLED,
            fresnel_scale: d::principled_material::FRESNEL_SCALE,
            fresnel_bias: d::principled_material::FRESNEL_BIAS,
            clearcoat_amount: d::principled_material::CLEARCOAT_AMOUNT,
            clearcoat_roughness_amount: d::principled_material::CLEARCOAT_ROUGHNESS_AMOUNT,
            clearcoat_fresnel_power: d::principled_material::CLEARCOAT_FRESNEL_POWER,
            clearcoat_fresnel_scale_bias_enabled: false,
            clearcoat_fresnel_scale: d::principled_material::CLEARCOAT_FRESNEL_SCALE,
            clearcoat_fresnel_bias: d::principled_material::CLEARCOAT_FRESNEL_BIAS,
            transmission_factor: d::principled_material::TRANSMISSION_FACTOR,
            transmission_map: None,
            transmission_channel: d::principled_material::TRANSMISSION_CHANNEL,
            height_map: None,
            height_amount: d::principled_material::HEIGHT_AMOUNT,
            height_channel: d::principled_material::HEIGHT_CHANNEL,
            min_height_map_samples: d::principled_material::MIN_HEIGHT_MAP_SAMPLES,
            max_height_map_samples: d::principled_material::MAX_HEIGHT_MAP_SAMPLES,
            light_probe: None,
            specular_map: None,
            specular_reflection_map: None,
            clearcoat_map: None,
            clearcoat_channel: d::principled_material::CLEARCOAT_CHANNEL,
            clearcoat_roughness_map: None,
            clearcoat_roughness_channel: d::principled_material::CLEARCOAT_ROUGHNESS_CHANNEL,
            clearcoat_normal_map: None,
            clearcoat_normal_strength: d::principled_material::CLEARCOAT_NORMAL_STRENGTH,
            thickness_map: None,
            thickness_channel: d::principled_material::THICKNESS_CHANNEL,
            thickness_factor: d::principled_material::THICKNESS_FACTOR,
            attenuation_distance: d::principled_material::ATTENUATION_DISTANCE,
            attenuation_color: rgba(d::principled_material::ATTENUATION_COLOR),
            point_size: d::principled_material::POINT_SIZE,
            line_width: d::principled_material::LINE_WIDTH,
        }
    }
}

fn environment_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "BackgroundMode",
            vec![
                ("Transparent", i64::from(background::TRANSPARENT)),
                ("Color", i64::from(background::COLOR)),
                ("SkyBox", i64::from(background::SKY_BOX)),
                ("SkyBoxCubeMap", i64::from(background::SKY_BOX_CUBE_MAP)),
                ("SkyMaterial", i64::from(background::SKY_MATERIAL)),
            ],
        ),
        (
            "TonemapMode",
            vec![
                ("TonemapModeNone", i64::from(tonemap::NONE)),
                ("TonemapModeLinear", i64::from(tonemap::LINEAR)),
                ("TonemapModeAces", i64::from(tonemap::ACES)),
                ("TonemapModeHejlDawson", i64::from(tonemap::HEJL_DAWSON)),
                ("TonemapModeFilmic", i64::from(tonemap::FILMIC)),
                ("TonemapModeCustom", i64::from(tonemap::CUSTOM)),
            ],
        ),
        (
            "AntialiasingMode",
            vec![
                ("NoAA", i64::from(antialiasing::NO_AA)),
                ("SSAA", i64::from(antialiasing::SSAA)),
                ("MSAA", i64::from(antialiasing::MSAA)),
                ("ProgressiveAA", i64::from(antialiasing::PROGRESSIVE_AA)),
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
    /// Transparent, Color and SkyBox are drawn; SkyBoxCubeMap and
    /// SkyMaterial are not built and draw as Transparent.
    #[qml(notify, enumeration = "BackgroundMode")]
    pub background_mode: i32,
    /// Linear by default, as the documentation says. TonemapModeCustom is
    /// not built and draws as Linear.
    #[qml(notify, enumeration = "TonemapMode")]
    pub tonemap_mode: i32,
    /// An equirectangular picture of what is around the scene: it lights
    /// every PrincipledMaterial and is the sky box.
    #[qml(notify, object = "Texture")]
    pub light_probe: Option<ObjectId>,
    #[qml(notify)]
    pub probe_exposure: f64,
    /// 0 to 1: how much darker the probe's light from below the horizon
    /// is on the materials it lights.
    #[qml(notify)]
    pub probe_horizon: f64,
    /// Euler angles in degrees.
    #[qml(notify)]
    pub probe_orientation: Vector3d,
    /// 0 to 1.
    #[qml(notify)]
    pub skybox_blur_amount: f64,
    /// MSAA is drawn; SSAA and ProgressiveAA are not built and draw
    /// as NoAA.
    #[qml(notify, enumeration = "AntialiasingMode")]
    pub antialiasing_mode: i32,
    /// For MSAA: 2, 4 or 8 samples a pixel, or as near as the device has.
    #[qml(notify, enumeration = "AntialiasingQuality")]
    pub antialiasing_quality: i32,
    /// `Effect`s run over the picture in the list's order.
    #[qml(notify, list)]
    pub effects: QmlList,
    /// Turning it on gives a strength or distance that is 0 a value
    /// that shows; once given, it says whether ambient occlusion shows.
    #[qml(notify, change = ao_enabled_changed)]
    pub ao_enabled: bool,
    /// 0 to 100; 0 is none.
    #[qml(notify)]
    pub ao_strength: f64,
    #[qml(notify)]
    pub ao_distance: f64,
    /// 0 to 50.
    #[qml(notify)]
    pub ao_softness: f64,
    /// 2 to 4.
    #[qml(notify)]
    pub ao_sample_rate: i32,
    #[qml(notify)]
    pub ao_bias: f64,
    #[qml(notify)]
    pub ao_dither: bool,
    /// What lit surfaces fade into.
    #[qml(notify, object = "Fog")]
    pub fog: Option<ObjectId>,
    /// Widens highlights where normals change fast across a pixel.
    #[qml(notify, rename = "specularAAEnabled")]
    pub specular_aa_enabled: bool,
    /// Off, a surface hides nothing and nothing hides it.
    #[qml(notify)]
    pub depth_test_enabled: bool,
    /// Kept: a depth pre-pass changes the time taken, not the picture.
    #[qml(notify)]
    pub depth_pre_pass_enabled: bool,
    /// Kept: frames are not blended over time (docs/quick3d.md).
    #[qml(notify, rename = "temporalAAEnabled")]
    pub temporal_aa_enabled: bool,
    #[qml(notify, rename = "temporalAAStrength")]
    pub temporal_aa_strength: f64,
}

/// `Fog`: lit surfaces fade into a colour, with their distance from the
/// camera and their height.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "Fog", extends = Object3D)]
pub struct Fog {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub color: Color,
    /// 0 to 1.
    #[qml(notify)]
    pub density: f64,
    #[qml(notify)]
    pub depth_enabled: bool,
    #[qml(notify)]
    pub depth_near: f64,
    #[qml(notify)]
    pub depth_far: f64,
    #[qml(notify)]
    pub depth_curve: f64,
    #[qml(notify)]
    pub height_enabled: bool,
    #[qml(notify)]
    pub least_intense_y: f64,
    #[qml(notify)]
    pub most_intense_y: f64,
    #[qml(notify)]
    pub height_curve: f64,
    #[qml(notify)]
    pub transmit_enabled: bool,
    #[qml(notify)]
    pub transmit_curve: f64,
}

impl Default for Fog {
    fn default() -> Self {
        use d::fog as f;
        Fog {
            object3d: Object3D::default(),
            enabled: f::ENABLED,
            color: rgba(f::COLOR),
            density: f::DENSITY,
            depth_enabled: f::DEPTH_ENABLED,
            depth_near: f::DEPTH_NEAR,
            depth_far: f::DEPTH_FAR,
            depth_curve: f::DEPTH_CURVE,
            height_enabled: f::HEIGHT_ENABLED,
            least_intense_y: f::LEAST_INTENSE_Y,
            most_intense_y: f::MOST_INTENSE_Y,
            height_curve: f::HEIGHT_CURVE,
            transmit_enabled: f::TRANSMIT_ENABLED,
            transmit_curve: f::TRANSMIT_CURVE,
        }
    }
}

/// The values the documentation's note promises: strong enough to see.
fn ao_enabled_changed(cx: &Cx<'_>, _: PropIndex) {
    let (e, me) = (cx.engine, cx.this);
    if !e.get(me, SceneEnvironment::AO_ENABLED).unwrap_or(false) {
        return;
    }
    if e.get(me, SceneEnvironment::AO_STRENGTH)
        .is_ok_and(|s| s == 0.0)
    {
        host_write(e, me, SceneEnvironment::AO_STRENGTH, 100.0);
    }
    if e.get(me, SceneEnvironment::AO_DISTANCE)
        .is_ok_and(|d| d == 0.0)
    {
        host_write(e, me, SceneEnvironment::AO_DISTANCE, 5.0);
    }
}

impl Default for SceneEnvironment {
    fn default() -> Self {
        SceneEnvironment {
            object3d: Object3D::default(),
            clear_color: rgba(d::scene_environment::CLEAR_COLOR),
            background_mode: d::scene_environment::BACKGROUND_MODE,
            tonemap_mode: d::scene_environment::TONEMAP_MODE,
            light_probe: None,
            probe_exposure: d::scene_environment::PROBE_EXPOSURE,
            probe_horizon: d::scene_environment::PROBE_HORIZON,
            probe_orientation: vector(d::scene_environment::PROBE_ORIENTATION),
            skybox_blur_amount: d::scene_environment::SKYBOX_BLUR_AMOUNT,
            antialiasing_mode: d::scene_environment::ANTIALIASING_MODE,
            antialiasing_quality: d::scene_environment::ANTIALIASING_QUALITY,
            effects: QmlList::default(),
            ao_enabled: d::scene_environment::AO_ENABLED,
            ao_strength: d::scene_environment::AO_STRENGTH,
            ao_distance: d::scene_environment::AO_DISTANCE,
            ao_softness: d::scene_environment::AO_SOFTNESS,
            ao_sample_rate: d::scene_environment::AO_SAMPLE_RATE,
            ao_bias: d::scene_environment::AO_BIAS,
            ao_dither: d::scene_environment::AO_DITHER,
            fog: None,
            specular_aa_enabled: d::scene_environment::SPECULAR_AA_ENABLED,
            depth_test_enabled: d::scene_environment::DEPTH_TEST_ENABLED,
            depth_pre_pass_enabled: d::scene_environment::DEPTH_PRE_PASS_ENABLED,
            temporal_aa_enabled: d::scene_environment::TEMPORAL_AA_ENABLED,
            temporal_aa_strength: d::scene_environment::TEMPORAL_AA_STRENGTH,
        }
    }
}
