//! The bodies of a world, and what each says of itself.
//!
//! A body's one effect reads all a world has to know of it into a [`Spec`].
//! A dynamic body does not track its own place: the world writes that, and
//! once it is in a world only `reset` moves it.

use std::cell::RefCell;
use std::rc::Rc;

use ljos::item::ljos_3d::math::{Mat4, Quat as MQuat, Vec3 as MVec3};
use ljos::prelude::*;
use ljos_physics::{Command, Filter, Kind, Locks, Mass, Material, Part, Pose, Quat, Vec3};
use ljos_quick3d::{Node, in_scene, parent_node, place};

use crate::shapes::{self, CapsuleShape};
use crate::world;

/// The bodies no world has yet, and those gone since a world last looked.
#[derive(Default)]
pub(crate) struct Registry {
    pub fresh: RefCell<Vec<ObjectId>>,
    pub gone: RefCell<Vec<ObjectId>>,
    pub worlds: RefCell<Vec<ObjectId>>,
}

/// What a body is, for a world.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Spec {
    pub kind: Kind,
    /// Dynamic by its type, but held by a shape a mass cannot be found of.
    pub forced: bool,
    pub parts: Vec<Part>,
    pub waiting: bool,
    pub enabled: bool,
    pub material: Material,
    pub filter: Filter,
    /// `None`: of the world's default density.
    pub mass: Option<Mass>,
    pub gravity: bool,
    pub locks: Locks,
    /// Where anything that is not dynamic is to be.
    pub target: Option<Pose>,
    pub character: Option<CharacterSpec>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CharacterSpec {
    pub radius: f32,
    pub half_height: f32,
    pub step_offset: f32,
    pub movement: MVec3,
    pub gravity: MVec3,
    pub mid_air_control: bool,
    pub turn: MQuat,
    pub tells_hits: bool,
}

/// What a body holds for its world.
#[derive(Debug, Default)]
pub(crate) struct BodyState {
    pub spec: Option<Rc<Spec>>,
    pub commands: Vec<Command>,
    pub teleport: Option<Vec3>,
    /// Bodies inside a trigger.
    pub inside: Vec<ObjectId>,
}

/// `PhysicsMaterial`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "PhysicsMaterial", extends = QtObject)]
pub struct PhysicsMaterial {
    #[qml(base)]
    pub object: QtObject,
    /// Kept: the engine has one friction, `dynamicFriction`.
    #[qml(notify)]
    pub static_friction: f64,
    #[qml(notify)]
    pub dynamic_friction: f64,
    #[qml(notify)]
    pub restitution: f64,
}

impl Default for PhysicsMaterial {
    fn default() -> Self {
        PhysicsMaterial {
            object: QtObject::default(),
            static_friction: 0.5,
            dynamic_friction: 0.5,
            restitution: 0.5,
        }
    }
}

/// `PhysicsNode`: what is in a world.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "PhysicsNode", extends = Node, uncreatable)]
pub struct PhysicsNode {
    #[qml(base)]
    pub node: Node,
    #[qml(notify, list)]
    pub collision_shapes: QmlList,
    #[qml(notify)]
    pub send_contact_reports: bool,
    #[qml(notify)]
    pub receive_contact_reports: bool,
    #[qml(notify)]
    pub send_trigger_reports: bool,
    #[qml(notify)]
    pub receive_trigger_reports: bool,
    #[qml(notify)]
    pub filter_group: i32,
    #[qml(notify)]
    pub filter_ignore_groups: i32,
    #[qml(skip)]
    pub(crate) state: BodyState,
}

#[qml_impl]
impl PhysicsNode {
    #[qml(signal, name = "bodyContact")]
    fn body_contact(body: Option<ObjectId>, positions: Value, impulses: Value, normals: Value);

    #[qml(signal, name = "enteredTriggerBody")]
    fn entered_trigger_body(body: Option<ObjectId>);

    #[qml(signal, name = "exitedTriggerBody")]
    fn exited_trigger_body(body: Option<ObjectId>);

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        install(cx.engine, cx.this)
    }
}

/// `PhysicsBody`: what stops other bodies.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "PhysicsBody", extends = PhysicsNode, uncreatable)]
pub struct PhysicsBody {
    #[qml(base)]
    pub physics_node: PhysicsNode,
    #[qml(notify, object = "PhysicsMaterial")]
    pub physics_material: Option<ObjectId>,
    #[qml(notify)]
    pub simulation_enabled: bool,
}

impl Default for PhysicsBody {
    fn default() -> Self {
        PhysicsBody {
            physics_node: PhysicsNode::default(),
            physics_material: None,
            simulation_enabled: true,
        }
    }
}

/// `StaticRigidBody`: stays where its node is.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "StaticRigidBody", extends = PhysicsBody)]
pub struct StaticRigidBody {
    #[qml(base)]
    pub body: PhysicsBody,
}

fn dynamic_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "MassMode",
            vec![
                ("DefaultDensity", 0),
                ("CustomDensity", 1),
                ("Mass", 2),
                ("MassAndInertiaTensor", 3),
                ("MassAndInertiaMatrix", 4),
            ],
        ),
        (
            "AxisLock",
            vec![("LockNone", 0), ("LockX", 1), ("LockY", 2), ("LockZ", 4)],
        ),
    ]
}

/// `DynamicRigidBody`: moved by the world.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Physics",
    name = "DynamicRigidBody",
    extends = PhysicsBody,
    enums = dynamic_enums
)]
pub struct DynamicRigidBody {
    #[qml(base)]
    pub body: PhysicsBody,
    #[qml(notify, enumeration = "MassMode")]
    pub mass_mode: i32,
    #[qml(notify)]
    pub density: f64,
    #[qml(notify, change = mass_changed)]
    pub mass: f64,
    #[qml(notify)]
    pub center_of_mass_position: Vector3d,
    #[qml(notify)]
    pub center_of_mass_rotation: Quaternion,
    #[qml(notify)]
    pub inertia_tensor: Vector3d,
    /// Nine numbers, row by row.
    #[qml(notify)]
    pub inertia_matrix: Value,
    #[qml(notify)]
    pub is_kinematic: bool,
    #[qml(notify)]
    pub kinematic_position: Vector3d,
    #[qml(notify)]
    pub kinematic_rotation: Quaternion,
    #[qml(notify, change = kinematic_euler_changed)]
    pub kinematic_euler_rotation: Vector3d,
    #[qml(notify)]
    pub kinematic_pivot: Vector3d,
    #[qml(notify)]
    pub gravity_enabled: bool,
    #[qml(notify, flags = "AxisLock")]
    pub linear_axis_lock: i32,
    #[qml(notify, flags = "AxisLock")]
    pub angular_axis_lock: i32,
    #[qml(notify, readonly)]
    pub is_sleeping: bool,
    /// The last mass that could be: a negative one is not taken.
    #[qml(skip)]
    good_mass: f64,
}

impl Default for DynamicRigidBody {
    fn default() -> Self {
        DynamicRigidBody {
            body: PhysicsBody::default(),
            mass_mode: 0,
            density: 0.001,
            mass: 1.0,
            center_of_mass_position: Vector3d::default(),
            center_of_mass_rotation: Quaternion::default(),
            inertia_tensor: Vector3d {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            inertia_matrix: Value::Undefined,
            is_kinematic: false,
            kinematic_position: Vector3d::default(),
            kinematic_rotation: Quaternion::default(),
            kinematic_euler_rotation: Vector3d::default(),
            kinematic_pivot: Vector3d::default(),
            gravity_enabled: true,
            linear_axis_lock: 0,
            angular_axis_lock: 0,
            is_sleeping: false,
            good_mass: 1.0,
        }
    }
}

fn mass_changed(cx: &Cx<'_>, _: PropIndex) {
    let (e, me) = (cx.engine, cx.this);
    let Ok(mass) = e.get(me, DynamicRigidBody::MASS) else {
        return;
    };
    if mass < 0.0 {
        let good = e
            .with(me, |b: &DynamicRigidBody| b.good_mass)
            .unwrap_or(1.0);
        ljos_quick3d::host_write(e, me, DynamicRigidBody::MASS, good);
    } else {
        let _ = e.with_mut(me, |b: &mut DynamicRigidBody| b.good_mass = mass);
    }
}

fn kinematic_euler_changed(cx: &Cx<'_>, _: PropIndex) {
    let (e, me) = (cx.engine, cx.this);
    let Ok(euler) = e.get(me, DynamicRigidBody::KINEMATIC_EULER_ROTATION) else {
        return;
    };
    let q = MQuat::from_euler_degrees(MVec3::new(euler.x, euler.y, euler.z));
    ljos_quick3d::host_write(
        e,
        me,
        DynamicRigidBody::KINEMATIC_ROTATION,
        Quaternion {
            scalar: q.w,
            x: q.x,
            y: q.y,
            z: q.z,
        },
    );
}

fn queue(cx: &Cx, command: Command) -> Result<(), EngineError> {
    cx.with_mut(|n: &mut PhysicsNode| n.state.commands.push(command))?;
    world::wake_all(cx.engine);
    Ok(())
}

fn v3(v: &Value) -> Vec3 {
    match v {
        Value::Vector3d(v) => [v.x, v.y, v.z],
        _ => [0.0; 3],
    }
}

#[qml_impl]
impl DynamicRigidBody {
    #[qml(invokable, name = "applyCentralForce")]
    fn apply_central_force(cx: &Cx, force: Value) -> Result<(), EngineError> {
        queue(cx, Command::Force(v3(&force)))
    }

    #[qml(invokable, name = "applyForce")]
    fn apply_force(cx: &Cx, force: Value, position: Value) -> Result<(), EngineError> {
        queue(cx, Command::ForceAt(v3(&force), v3(&position)))
    }

    #[qml(invokable, name = "applyTorque")]
    fn apply_torque(cx: &Cx, torque: Value) -> Result<(), EngineError> {
        queue(cx, Command::Torque(v3(&torque)))
    }

    #[qml(invokable, name = "applyCentralImpulse")]
    fn apply_central_impulse(cx: &Cx, impulse: Value) -> Result<(), EngineError> {
        queue(cx, Command::Impulse(v3(&impulse)))
    }

    #[qml(invokable, name = "applyImpulse")]
    fn apply_impulse(cx: &Cx, impulse: Value, position: Value) -> Result<(), EngineError> {
        queue(cx, Command::ImpulseAt(v3(&impulse), v3(&position)))
    }

    #[qml(invokable, name = "applyTorqueImpulse")]
    fn apply_torque_impulse(cx: &Cx, impulse: Value) -> Result<(), EngineError> {
        queue(cx, Command::TorqueImpulse(v3(&impulse)))
    }

    #[qml(invokable, name = "setLinearVelocity")]
    fn set_linear_velocity(cx: &Cx, velocity: Value) -> Result<(), EngineError> {
        queue(cx, Command::LinearVelocity(v3(&velocity)))
    }

    #[qml(invokable, name = "setAngularVelocity")]
    fn set_angular_velocity(cx: &Cx, velocity: Value) -> Result<(), EngineError> {
        queue(cx, Command::AngularVelocity(v3(&velocity)))
    }

    /// Back to `position` and `eulerRotation`, in the space of its parent,
    /// standing still.
    #[qml(invokable)]
    fn reset(cx: &Cx, position: Value, euler_rotation: Value) -> Result<(), EngineError> {
        let e = cx.engine;
        let p = v3(&position);
        let r = v3(&euler_rotation);
        let above = e.untrack(|| parent_node(e, cx.this).map(|n| place(e, n)));
        let (world, turn) =
            above.map_or((Mat4::IDENTITY, MQuat::IDENTITY), |a| (a.world, a.rotation));
        let at = world.transform_point(MVec3::new(p[0], p[1], p[2]));
        let q = turn * MQuat::from_euler_degrees(MVec3::new(r[0], r[1], r[2]));
        queue(
            cx,
            Command::Reset(Pose {
                position: [at.x, at.y, at.z],
                rotation: [q.w, q.x, q.y, q.z],
            }),
        )
    }
}

/// `TriggerBody`: stops nothing, and tells of what goes in and out of it.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "TriggerBody", extends = PhysicsNode)]
pub struct TriggerBody {
    #[qml(base)]
    pub physics_node: PhysicsNode,
    #[qml(notify, readonly)]
    pub collision_count: i32,
}

#[qml_impl]
impl TriggerBody {
    #[qml(signal, name = "bodyEntered")]
    fn body_entered(body: Option<ObjectId>);

    #[qml(signal, name = "bodyExited")]
    fn body_exited(body: Option<ObjectId>);
}

fn character_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Collision",
        vec![("None", 0), ("Side", 1), ("Up", 2), ("Down", 4)],
    )]
}

/// `CharacterController`: a capsule that walks.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Physics",
    name = "CharacterController",
    extends = PhysicsBody,
    enums = character_enums
)]
pub struct CharacterController {
    #[qml(base)]
    pub body: PhysicsBody,
    #[qml(notify)]
    pub movement: Vector3d,
    #[qml(notify)]
    pub gravity: Vector3d,
    #[qml(notify)]
    pub mid_air_control: bool,
    #[qml(notify, flags = "Collision", readonly)]
    pub collisions: i32,
    #[qml(notify)]
    pub enable_shape_hit_callback: bool,
}

impl Default for CharacterController {
    fn default() -> Self {
        CharacterController {
            body: PhysicsBody::default(),
            movement: Vector3d::default(),
            gravity: Vector3d::default(),
            mid_air_control: true,
            collisions: 0,
            enable_shape_hit_callback: false,
        }
    }
}

#[qml_impl]
impl CharacterController {
    #[qml(signal, name = "shapeHit")]
    fn shape_hit(body: Option<ObjectId>, position: Value, impulse: Value, normal: Value);

    /// Puts it at `position`, whatever is in the way, no longer falling.
    #[qml(invokable)]
    fn teleport(cx: &Cx, position: Value) -> Result<(), EngineError> {
        cx.with_mut(|n: &mut PhysicsNode| n.state.teleport = Some(v3(&position)))?;
        world::wake_all(cx.engine);
        Ok(())
    }
}

fn pose_of(e: &Engine, me: ObjectId) -> Pose {
    let p = place(e, me);
    let at = p.world.translation();
    Pose {
        position: [at.x, at.y, at.z],
        rotation: [p.rotation.w, p.rotation.x, p.rotation.y, p.rotation.z],
    }
}

/// Where a kinematic body wants to be in the scene.
fn kinematic_target(e: &Engine, me: ObjectId) -> Pose {
    let at = e
        .get(me, DynamicRigidBody::KINEMATIC_POSITION)
        .unwrap_or_default();
    let pivot = e
        .get(me, DynamicRigidBody::KINEMATIC_PIVOT)
        .unwrap_or_default();
    let q = e
        .get(me, DynamicRigidBody::KINEMATIC_ROTATION)
        .unwrap_or_default();
    let scale = e.get(me, Node::SCALE).unwrap_or_default();
    let own = MQuat {
        x: q.x,
        y: q.y,
        z: q.z,
        w: q.scalar,
    };
    let off = own.rotate(MVec3::new(
        -pivot.x * scale.x,
        -pivot.y * scale.y,
        -pivot.z * scale.z,
    ));
    let local = MVec3::new(at.x + off.x, at.y + off.y, at.z + off.z);
    let above = parent_node(e, me).map(|n| place(e, n));
    let (world, turn) = above.map_or((Mat4::IDENTITY, MQuat::IDENTITY), |a| (a.world, a.rotation));
    let p = world.transform_point(local);
    let r = turn * own;
    Pose {
        position: [p.x, p.y, p.z],
        rotation: [r.w, r.x, r.y, r.z],
    }
}

/// The scale of a node in the scene. Its parents' scales alone, so that a
/// body whose parent moves is not told.
fn scene_scale(e: &Engine, me: ObjectId) -> [f32; 3] {
    let mut s = [1.0, 1.0, 1.0];
    let mut at = Some(me);
    while let Some(n) = at {
        let own = e.get(n, Node::SCALE).unwrap_or_default();
        s = [s[0] * own.x, s[1] * own.y, s[2] * own.z];
        at = parent_node(e, n);
    }
    s
}

fn read_spec(e: &Engine, me: ObjectId) -> Spec {
    let dynamic = e.is::<DynamicRigidBody>(me);
    let trigger = e.is::<TriggerBody>(me);
    let character = e.is::<CharacterController>(me);
    let scale = scene_scale(e, me);

    let list = match e.read(me, PhysicsNode::COLLISION_SHAPES.index) {
        Value::List(l) => l.to_vec(),
        _ => Vec::new(),
    };
    let mut parts = Vec::with_capacity(list.len());
    let mut waiting = false;
    let mut character_spec = None;
    if character {
        character_spec = read_character(e, me, &list, scale, &mut parts);
    } else {
        for shape in list.iter().filter_map(Value::as_object) {
            match shapes::read(e, shape, scale) {
                shapes::Read::Part(part) => parts.push(part),
                shapes::Read::Waiting => waiting = true,
                shapes::Read::Nothing => {}
            }
        }
    }

    let is_body = e.is::<PhysicsBody>(me);
    let material = is_body
        .then(|| e.get(me, PhysicsBody::PHYSICS_MATERIAL).ok().flatten())
        .flatten()
        .map_or(Material::default(), |m| Material {
            dynamic_friction: e
                .get(m, PhysicsMaterial::DYNAMIC_FRICTION)
                .unwrap_or(0.5)
                .max(0.0) as f32,
            restitution: e
                .get(m, PhysicsMaterial::RESTITUTION)
                .unwrap_or(0.5)
                .clamp(0.0, 1.0) as f32,
        });
    let enabled = !is_body || e.get(me, PhysicsBody::SIMULATION_ENABLED).unwrap_or(true);
    let filter = Filter {
        group: e
            .get(me, PhysicsNode::FILTER_GROUP)
            .unwrap_or(0)
            .clamp(0, 31) as u8,
        ignored: e.get(me, PhysicsNode::FILTER_IGNORE_GROUPS).unwrap_or(0) as u32,
    };

    let fixed = parts.iter().any(|p| p.shape.is_fixed());
    let kinematic = dynamic && e.get(me, DynamicRigidBody::IS_KINEMATIC).unwrap_or(false);
    let forced = dynamic && !kinematic && fixed;
    let kind = if trigger {
        Kind::Trigger
    } else if character || kinematic || forced {
        Kind::Kinematic
    } else if dynamic {
        Kind::Dynamic
    } else {
        Kind::Static
    };
    let target = if character || kind == Kind::Dynamic {
        None
    } else if kinematic {
        Some(kinematic_target(e, me))
    } else {
        Some(pose_of(e, me))
    };

    Spec {
        kind,
        forced,
        parts,
        waiting,
        enabled,
        material,
        filter,
        mass: if dynamic { read_mass(e, me) } else { None },
        gravity: !dynamic || e.get(me, DynamicRigidBody::GRAVITY_ENABLED).unwrap_or(true),
        locks: if dynamic {
            Locks {
                linear: e.get(me, DynamicRigidBody::LINEAR_AXIS_LOCK).unwrap_or(0) as u8,
                angular: e.get(me, DynamicRigidBody::ANGULAR_AXIS_LOCK).unwrap_or(0) as u8,
            }
        } else {
            Locks::default()
        },
        target,
        character: character_spec,
    }
}

const LEAST_DENSITY: f64 = 1e-7;

fn read_mass(e: &Engine, me: ObjectId) -> Option<Mass> {
    let mode = e.get(me, DynamicRigidBody::MASS_MODE).unwrap_or(0);
    let mass = e.get(me, DynamicRigidBody::MASS).unwrap_or(1.0) as f32;
    let at = e
        .get(me, DynamicRigidBody::CENTER_OF_MASS_POSITION)
        .unwrap_or_default();
    let center = [at.x, at.y, at.z];
    match mode {
        1 => {
            let density = e.get(me, DynamicRigidBody::DENSITY).unwrap_or(0.001);
            if density < LEAST_DENSITY {
                e.warn(format!(
                    "PhysicsBody: Clamping density {density} to {LEAST_DENSITY}"
                ));
            }
            Some(Mass::Density(density.max(LEAST_DENSITY) as f32))
        }
        2 => Some(Mass::Total(mass)),
        3 => {
            let d = e
                .get(me, DynamicRigidBody::INERTIA_TENSOR)
                .unwrap_or_default();
            let q = e
                .get(me, DynamicRigidBody::CENTER_OF_MASS_ROTATION)
                .unwrap_or_default();
            let center_rotation: Quat = [q.scalar, q.x, q.y, q.z];
            Some(Mass::Inertia {
                mass,
                center,
                center_rotation,
                diagonal: [d.x, d.y, d.z],
            })
        }
        4 => {
            let mut matrix = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
            if let Value::List(l) = e.read(me, DynamicRigidBody::INERTIA_MATRIX.index)
                && l.len() == 9
            {
                for (m, v) in matrix.iter_mut().zip(l.iter()) {
                    *m = v.as_number().unwrap_or(0.0) as f32;
                }
            }
            Some(Mass::Matrix {
                mass,
                center,
                matrix,
            })
        }
        _ => None,
    }
}

fn read_character(
    e: &Engine,
    me: ObjectId,
    list: &[Value],
    scale: [f32; 3],
    parts: &mut Vec<Part>,
) -> Option<CharacterSpec> {
    let [shape] = list else {
        e.warn("CharacterController: invalid collision shapes list.".to_owned());
        return None;
    };
    let capsule = shape.as_object().filter(|s| e.is::<CapsuleShape>(*s));
    let Some(capsule) = capsule else {
        e.warn("CharacterController: collision shape is not a capsule.".to_owned());
        return None;
    };
    let diameter = e.get(capsule, CapsuleShape::DIAMETER).unwrap_or(100.0) as f32;
    let height = e.get(capsule, CapsuleShape::HEIGHT).unwrap_or(100.0) as f32 * scale[1];
    let radius = 0.5 * scale[0] * diameter;
    let half_height = height * 0.5;
    parts.push(shapes::upright_capsule(radius, half_height));
    let v = |p| {
        let v: Vector3d = e.get(me, p).unwrap_or_default();
        MVec3::new(v.x, v.y, v.z)
    };
    let turn = place(e, me).rotation;
    Some(CharacterSpec {
        radius,
        half_height,
        step_offset: 0.25 * height,
        movement: v(CharacterController::MOVEMENT),
        gravity: v(CharacterController::GRAVITY),
        mid_air_control: e
            .get(me, CharacterController::MID_AIR_CONTROL)
            .unwrap_or(true),
        turn,
        tells_hits: e
            .get(me, CharacterController::ENABLE_SHAPE_HIT_CALLBACK)
            .unwrap_or(false),
    })
}

/// Makes `me` known to the worlds there are and to come, and has its effect
/// say what it is from now on.
fn install(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let registry = e.extension::<Registry>();
    registry.fresh.borrow_mut().push(me);
    e.connect_by_name(me, "destroyed", move |e, _| {
        e.extension::<Registry>().gone.borrow_mut().push(me);
        world::wake_all(e);
    })?;
    e.effect_in(me, move |e| {
        let spec = read_spec(e, me);
        let changed = e
            .untrack(|| e.with(me, |n: &PhysicsNode| n.state.spec.as_deref() != Some(&spec)))
            .unwrap_or(false);
        if changed {
            let _ = e.with_mut(me, |n: &mut PhysicsNode| {
                n.state.spec = Some(Rc::new(spec));
            });
            e.untrack(|| world::wake_all(e));
        }
    })?;
    world::wake_all(e);
    Ok(())
}

/// The fresh bodies in `scene`, taken out of the list.
pub(crate) fn fresh_in(e: &Engine, scene: ObjectId) -> Vec<ObjectId> {
    let registry = e.extension::<Registry>();
    let mut fresh = registry.fresh.borrow_mut();
    let mut found = Vec::new();
    fresh.retain(|o| {
        if !e.is_alive(*o) {
            return false;
        }
        let inside = in_scene(e, *o, scene);
        if inside {
            found.push(*o);
        }
        !inside
    });
    found
}

/// Whether a body waits for `scene`'s world to take it.
pub(crate) fn any_fresh_in(e: &Engine, scene: ObjectId) -> bool {
    let registry = e.extension::<Registry>();
    let fresh = registry.fresh.borrow();
    fresh
        .iter()
        .any(|o| e.is_alive(*o) && in_scene(e, *o, scene))
}
