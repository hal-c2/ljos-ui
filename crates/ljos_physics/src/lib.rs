//! A world of rigid bodies in QtQuick3D.Physics's terms and units, over
//! rapier. It knows no engine, no QML and no GPU, and takes plain numbers,
//! so a native host and a web host share it (`docs/quick3d.md`, "web
//! embedding contract").
//!
//! Lengths are in whatever unit the host's scene is (Qt's default is the
//! centimetre) and rotations are quaternions as QML writes them, `[w, x, y,
//! z]`. The host says how long a typical thing is and how fast it moves
//! ([`Tuning`]): every tolerance of the solver follows from those two, as
//! PhysX's do, and they are not rapier's defaults, which assume metres.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::math::{Pose as RPose, Rotation, Vector};
use rapier3d::parry::shape::{HeightFieldFlags, TriMeshFlags};
use rapier3d::parry::utils::Array2;
use rapier3d::prelude::*;

/// Three numbers in the scene's units.
pub type Vec3 = [f32; 3];
/// A rotation as QML writes a quaternion: `[w, x, y, z]`.
pub type Quat = [f32; 4];

/// The rotation that turns nothing.
pub const NO_TURN: Quat = [1.0, 0.0, 0.0, 0.0];

fn v(a: Vec3) -> Vector {
    Vector::new(a[0], a[1], a[2])
}

fn q(a: Quat) -> Rotation {
    let r = Rotation::from_xyzw(a[1], a[2], a[3], a[0]);
    // A host's numbers come from a property an application can write.
    if r.length_squared() > 0.0 && r.is_finite() {
        r.normalize()
    } else {
        Rotation::IDENTITY
    }
}

/// Where a thing is and how it is turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub position: Vec3,
    pub rotation: Quat,
}

impl Default for Pose {
    fn default() -> Self {
        Pose {
            position: [0.0; 3],
            rotation: NO_TURN,
        }
    }
}

impl Pose {
    fn raw(self) -> RPose {
        RPose::from_parts(v(self.position), q(self.rotation))
    }

    fn of(p: &RPose) -> Pose {
        let t = p.translation;
        let r = p.rotation;
        Pose {
            position: [t.x, t.y, t.z],
            rotation: [r.w, r.x, r.y, r.z],
        }
    }
}

/// The form of one collision shape, already multiplied by the scale of the
/// node it is on: a shape in the world has no scale.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// `BoxShape`, by half its extents.
    Box { half: Vec3 },
    /// `SphereShape`.
    Sphere { radius: f32 },
    /// `CapsuleShape`, lying along X, as Qt's does.
    Capsule { radius: f32, half_height: f32 },
    /// `PlaneShape`: everything behind the XY plane is solid, its front
    /// being +Z, as a `Rectangle` model faces.
    Plane,
    /// `ConvexMeshShape`: the hull of these points.
    Convex { points: Vec<Vec3> },
    /// `TriangleMeshShape`.
    Triangles {
        positions: Vec<Vec3>,
        indices: Vec<[u32; 3]>,
    },
    /// `HeightFieldShape`: `heights` by row of `cols`, from -0.5 to 0.5 of
    /// `size.y`, centred on the node, `size.x` wide and `size.z` deep.
    HeightField {
        cols: usize,
        rows: usize,
        heights: Vec<f32>,
        size: Vec3,
    },
}

impl Shape {
    /// Whether the shape encloses nothing a mass can be worked out from: a
    /// body with one can only be static or kinematic, in Qt as here.
    pub fn is_fixed(&self) -> bool {
        matches!(
            self,
            Shape::Plane | Shape::Triangles { .. } | Shape::HeightField { .. }
        )
    }

    fn make(&self) -> Option<SharedShape> {
        Some(match self {
            Shape::Box { half } => SharedShape::cuboid(half[0], half[1], half[2]),
            Shape::Sphere { radius } => SharedShape::ball(*radius),
            Shape::Capsule {
                radius,
                half_height,
            } => SharedShape::capsule_x(*half_height, *radius),
            Shape::Plane => SharedShape::halfspace(Vector::Z),
            Shape::Convex { points } => {
                let points: Vec<Vector> = points.iter().copied().map(v).collect();
                SharedShape::convex_hull(&points)?
            }
            Shape::Triangles { positions, indices } => {
                if indices.is_empty() {
                    return None;
                }
                let positions: Vec<Vector> = positions.iter().copied().map(v).collect();
                SharedShape::trimesh_with_flags(
                    positions,
                    indices.clone(),
                    // Without the fix, a box sliding over a table's flat
                    // top catches on the edges between its triangles.
                    TriMeshFlags::FIX_INTERNAL_EDGES | TriMeshFlags::MERGE_DUPLICATE_VERTICES,
                )
                .ok()?
            }
            Shape::HeightField {
                cols,
                rows,
                heights,
                size,
            } => {
                if *cols < 2 || *rows < 2 || heights.len() != cols * rows {
                    return None;
                }
                SharedShape::heightfield_with_flags(
                    Array2::new(*rows, *cols, heights.clone()),
                    v(*size),
                    HeightFieldFlags::FIX_INTERNAL_EDGES,
                )
            }
        })
    }
}

/// A shape and where it is on its body.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub shape: Shape,
    pub pose: Pose,
}

/// `PhysicsMaterial`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Rapier has one coefficient of friction: this is the one it gets, so
    /// a body at rest holds as well as a sliding one slips.
    pub dynamic_friction: f32,
    pub restitution: f32,
}

impl Default for Material {
    fn default() -> Self {
        Material {
            dynamic_friction: 0.5,
            restitution: 0.5,
        }
    }
}

/// How a body of a world moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Does not move unless the host places it.
    Static,
    /// Moved by the world.
    Dynamic,
    /// Goes where the host says, pushing what is in the way.
    Kinematic,
    /// Goes where the host says, and only tells what is inside it.
    Trigger,
}

/// A body's weight (`DynamicRigidBody.massMode`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mass {
    /// Worked out from the shapes at this mass per cubic unit.
    Density(f32),
    /// This mass, its inertia and centre as the shapes have them.
    Total(f32),
    /// This mass, centre and inertia about the principal axes, which the
    /// rotation turns.
    Inertia {
        mass: f32,
        center: Vec3,
        center_rotation: Quat,
        diagonal: Vec3,
    },
    /// This mass, centre and inertia tensor, by column.
    Matrix {
        mass: f32,
        center: Vec3,
        matrix: [f32; 9],
    },
}

/// Which axes a body may not move or turn along, as bits of `X = 1, Y = 2,
/// Z = 4`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Locks {
    pub linear: u8,
    pub angular: u8,
}

/// Which bodies a body's shapes meet. `group` is one of 32, `ignored` a
/// mask of groups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub group: u8,
    pub ignored: u32,
}

/// What a host gives a world once: how it measures things.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// How long a typical thing is (`typicalLength`).
    pub length: f32,
    /// How fast a typical thing moves (`typicalSpeed`).
    pub speed: f32,
    /// Whether fast bodies stop at what they would pass through
    /// (`enableCCD`).
    pub ccd: bool,
    /// Whether a touch of two kinematic bodies is told.
    pub report_kinematic_kinematic: bool,
    /// Whether a touch of a static and a kinematic body is told.
    pub report_static_kinematic: bool,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning {
            length: 100.0,
            speed: 1000.0,
            ccd: false,
            report_kinematic_kinematic: false,
            report_static_kinematic: false,
        }
    }
}

/// A body of a [`World`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId(RigidBodyHandle);

/// What one body wants done, for the next step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// `applyCentralForce`: for the next step only.
    Force(Vec3),
    /// `applyForce`, at a point in the world.
    ForceAt(Vec3, Vec3),
    /// `applyTorque`.
    Torque(Vec3),
    /// `applyCentralImpulse`.
    Impulse(Vec3),
    /// `applyImpulse`, at a point in the world.
    ImpulseAt(Vec3, Vec3),
    /// `applyTorqueImpulse`.
    TorqueImpulse(Vec3),
    /// `setLinearVelocity`.
    LinearVelocity(Vec3),
    /// `setAngularVelocity`, in radians a second.
    AngularVelocity(Vec3),
    /// `reset`: here, and still.
    Reset(Pose),
}

/// Two bodies that began to touch, as seen by `a`: each list has an entry
/// per point of contact. A normal points from `b` to `a`, and an impulse is
/// what `a` was given.
#[derive(Clone, Debug, PartialEq)]
pub struct Contact {
    pub a: BodyId,
    pub b: BodyId,
    pub positions: Vec<Vec3>,
    pub impulses: Vec<Vec3>,
    pub normals: Vec<Vec3>,
}

/// A body that went into or out of a trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TriggerChange {
    pub trigger: BodyId,
    pub other: BodyId,
    pub entered: bool,
}

/// What one step found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stepped {
    pub contacts: Vec<Contact>,
    pub triggers: Vec<TriggerChange>,
}

/// What a character met on its way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub other: BodyId,
    pub position: Vec3,
    /// Pointing from what it met to the character.
    pub normal: Vec3,
}

/// Where a move took a character.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Moved {
    /// How far it really went.
    pub translation: Vec3,
    pub touches_side: bool,
    pub touches_up: bool,
    pub touches_down: bool,
    pub hits: Vec<Hit>,
}

/// A rapier event handler has to be `Sync`, though a step runs on one
/// thread.
#[derive(Default)]
struct Events(Mutex<Vec<CollisionEvent>>);

impl EventHandler for Events {
    fn handle_collision_event(
        &self,
        _: &RigidBodySet,
        _: &ColliderSet,
        event: CollisionEvent,
        _: Option<&ContactPair>,
    ) {
        if let Ok(mut events) = self.0.lock() {
            events.push(event);
        }
    }

    fn handle_contact_force_event(
        &self,
        _: Real,
        _: &RigidBodySet,
        _: &ColliderSet,
        _: &ContactPair,
        _: Real,
    ) {
    }

    fn handle_soft_body_tear_event(&self, _: &SoftBodySet, _: &SoftBodyTearEvent) {}
}

#[derive(Debug)]
struct Body {
    kind: Kind,
    material: Material,
    filter: Filter,
    parts: Vec<Part>,
    colliders: Vec<ColliderHandle>,
    mass: Mass,
}

/// The world.
pub struct World {
    raw: PhysicsWorld,
    tuning: Tuning,
    bodies: HashMap<RigidBodyHandle, Body>,
    /// Bodies a force was put on: it lasts one step.
    forced: HashSet<RigidBodyHandle>,
    /// How many shape pairs of two bodies touch: bodies touch once however
    /// many of their shapes do.
    touching: HashMap<(RigidBodyHandle, RigidBodyHandle), u32>,
}

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("World")
            .field("bodies", &self.bodies.len())
            .finish()
    }
}

impl World {
    pub fn new(tuning: Tuning, gravity: Vec3) -> World {
        let mut raw = PhysicsWorld::new();
        raw.gravity = v(gravity);
        let p = &mut raw.integration_parameters;
        p.length_unit = tuning.length.max(f32::EPSILON);
        // Rapier's limits are per length unit, with a unit that is one
        // typical thing long; speeds a host calls typical have to be far
        // inside them even when its things are small and its gravity is
        // measured in centimetres.
        let speed = tuning.speed / p.length_unit;
        p.normalized_max_linear_velocity = p.normalized_max_linear_velocity.max(speed * 100.0);
        p.normalized_max_corrective_velocity =
            p.normalized_max_corrective_velocity.max(speed * 0.05);
        World {
            raw,
            tuning,
            bodies: HashMap::new(),
            forced: HashSet::new(),
            touching: HashMap::new(),
        }
    }

    pub fn set_gravity(&mut self, gravity: Vec3) {
        self.raw.gravity = v(gravity);
        // A sleeping body does not feel it otherwise.
        for (h, body) in &self.bodies {
            if body.kind == Kind::Dynamic
                && let Some(b) = self.raw.bodies.get_mut(*h)
            {
                b.wake_up(true);
            }
        }
    }

    /// Adds a body, by default without shapes and with a mass of 1.
    pub fn add_body(&mut self, kind: Kind, pose: Pose) -> BodyId {
        // PhysX calls a body still when it goes slower than a hundredth of
        // the typical speed.
        let still = 0.01 * self.tuning.speed / self.raw.integration_parameters.length_unit;
        let builder = match kind {
            Kind::Static => RigidBodyBuilder::fixed(),
            Kind::Dynamic => RigidBodyBuilder::dynamic(),
            Kind::Kinematic | Kind::Trigger => RigidBodyBuilder::kinematic_position_based(),
        };
        let mut body = builder
            .pose(pose.raw())
            .ccd_enabled(self.tuning.ccd && kind == Kind::Dynamic)
            .additional_mass_properties(MassProperties::new(Vector::ZERO, 1.0, Vector::ONE))
            .build();
        body.activation_mut().normalized_linear_threshold = still;
        let h = self.raw.bodies.insert(body);
        self.bodies.insert(
            h,
            Body {
                kind,
                material: Material::default(),
                filter: Filter::default(),
                parts: Vec::new(),
                colliders: Vec::new(),
                mass: Mass::Total(1.0),
            },
        );
        BodyId(h)
    }

    pub fn remove_body(&mut self, id: BodyId) {
        if self.bodies.remove(&id.0).is_none() {
            return;
        }
        self.forced.remove(&id.0);
        self.touching.retain(|k, _| k.0 != id.0 && k.1 != id.0);
        let r = &mut self.raw;
        r.bodies.remove(
            id.0,
            &mut r.islands,
            &mut r.colliders,
            &mut r.impulse_joints,
            &mut r.multibody_joints,
            &mut r.soft_bodies,
            true,
        );
    }

    /// Makes `parts` the body's shapes, in place of those it had. A shape
    /// that cannot be made (a hull of too few points) is left out; the
    /// return says how many were.
    pub fn set_parts(&mut self, id: BodyId, parts: Vec<Part>) -> usize {
        let Some(body) = self.bodies.get_mut(&id.0) else {
            return 0;
        };
        let r = &mut self.raw;
        for c in body.colliders.drain(..) {
            r.colliders
                .remove(c, &mut r.islands, &mut r.bodies, &mut r.soft_bodies, true);
        }
        let mut missing = 0;
        let (kind, material, filter) = (body.kind, body.material, body.filter);
        let sensor_types = ActiveCollisionTypes::all();
        let mut types = ActiveCollisionTypes::default();
        if self.tuning.report_kinematic_kinematic {
            types |= ActiveCollisionTypes::KINEMATIC_KINEMATIC;
        }
        if self.tuning.report_static_kinematic {
            types |= ActiveCollisionTypes::KINEMATIC_FIXED;
        }
        for part in &parts {
            let Some(shape) = part.shape.make() else {
                missing += 1;
                continue;
            };
            let collider = ColliderBuilder::new(shape)
                .position(part.pose.raw())
                // Mass comes from `set_mass` for every mode alike.
                .density(0.0)
                .friction(material.dynamic_friction)
                .restitution(material.restitution)
                .collision_groups(groups(filter, self.tuning.ccd))
                .solver_groups(groups(filter, self.tuning.ccd))
                .sensor(kind == Kind::Trigger)
                .active_events(ActiveEvents::COLLISION_EVENTS)
                .active_collision_types(if kind == Kind::Trigger {
                    sensor_types
                } else {
                    types
                })
                .build();
            body.colliders.push(
                r.colliders
                    .insert_with_parent(collider, id.0, &mut r.bodies),
            );
        }
        body.parts = parts;
        let mass = body.mass;
        self.set_mass(id, mass);
        missing
    }

    pub fn set_material(&mut self, id: BodyId, material: Material) {
        let Some(body) = self.bodies.get_mut(&id.0) else {
            return;
        };
        body.material = material;
        for c in &body.colliders {
            if let Some(c) = self.raw.colliders.get_mut(*c) {
                c.set_friction(material.dynamic_friction);
                c.set_restitution(material.restitution);
            }
        }
    }

    pub fn set_filter(&mut self, id: BodyId, filter: Filter) {
        let Some(body) = self.bodies.get_mut(&id.0) else {
            return;
        };
        body.filter = filter;
        for c in &body.colliders {
            if let Some(c) = self.raw.colliders.get_mut(*c) {
                c.set_collision_groups(groups(filter, self.tuning.ccd));
                c.set_solver_groups(groups(filter, self.tuning.ccd));
            }
        }
    }

    /// Weighs the body again; its shapes are what a density goes by.
    pub fn set_mass(&mut self, id: BodyId, mass: Mass) {
        let Some(body) = self.bodies.get_mut(&id.0) else {
            return;
        };
        body.mass = mass;
        let props = |density: f32| -> MassProperties {
            body.parts
                .iter()
                .filter(|p| !p.shape.is_fixed())
                .filter_map(|p| {
                    let m = p.shape.make()?.mass_properties(density);
                    Some(m.transform_by(&p.pose.raw()))
                })
                .sum()
        };
        let made = match mass {
            Mass::Density(d) => props(d),
            Mass::Total(m) => {
                let mut p = props(1.0);
                if p.mass() > 0.0 {
                    p.set_mass(m, true);
                    p
                } else {
                    MassProperties::new(Vector::ZERO, m, Vector::ONE)
                }
            }
            Mass::Inertia {
                mass,
                center,
                center_rotation,
                diagonal,
            } => MassProperties::with_principal_inertia_frame(
                v(center),
                mass,
                v(diagonal),
                q(center_rotation),
            ),
            Mass::Matrix {
                mass,
                center,
                matrix,
            } => {
                MassProperties::with_inertia_matrix(v(center), mass, Mat3::from_cols_array(&matrix))
            }
        };
        if let Some(b) = self.raw.bodies.get_mut(id.0) {
            b.set_additional_mass_properties(made, true);
            // Rapier would only work this out in the next step, and an impulse
            // given before that would act on a body that weighs nothing.
            b.recompute_mass_properties_from_colliders(&self.raw.colliders);
        }
    }

    /// Switches a body between moving by the world and moving by the host.
    pub fn set_kinematic(&mut self, id: BodyId, kinematic: bool) {
        let Some(body) = self.bodies.get_mut(&id.0) else {
            return;
        };
        if !matches!(body.kind, Kind::Dynamic | Kind::Kinematic) {
            return;
        }
        body.kind = if kinematic {
            Kind::Kinematic
        } else {
            Kind::Dynamic
        };
        if let Some(b) = self.raw.bodies.get_mut(id.0) {
            b.set_body_type(
                if kinematic {
                    RigidBodyType::KinematicPositionBased
                } else {
                    RigidBodyType::Dynamic
                },
                true,
            );
        }
    }

    /// Whether the body falls.
    pub fn set_gravity_enabled(&mut self, id: BodyId, on: bool) {
        if let Some(b) = self.raw.bodies.get_mut(id.0) {
            b.set_gravity_scale(if on { 1.0 } else { 0.0 }, true);
        }
    }

    pub fn set_locks(&mut self, id: BodyId, locks: Locks) {
        let mut axes = LockedAxes::empty();
        for (bit, linear, angular) in [
            (
                1,
                LockedAxes::TRANSLATION_LOCKED_X,
                LockedAxes::ROTATION_LOCKED_X,
            ),
            (
                2,
                LockedAxes::TRANSLATION_LOCKED_Y,
                LockedAxes::ROTATION_LOCKED_Y,
            ),
            (
                4,
                LockedAxes::TRANSLATION_LOCKED_Z,
                LockedAxes::ROTATION_LOCKED_Z,
            ),
        ] {
            if locks.linear & bit != 0 {
                axes |= linear;
            }
            if locks.angular & bit != 0 {
                axes |= angular;
            }
        }
        if let Some(b) = self.raw.bodies.get_mut(id.0) {
            b.set_locked_axes(axes, true);
        }
    }

    /// Puts a body somewhere at once: a static body or a trigger whose node
    /// moved. Whatever rests on it wakes.
    pub fn place(&mut self, id: BodyId, pose: Pose) {
        if let Some(b) = self.raw.bodies.get_mut(id.0) {
            b.set_position(pose.raw(), true);
            self.wake_what_touches(id);
        }
    }

    /// A body asleep on one that is moved would otherwise hang in the air:
    /// waking a fixed body wakes nothing that rests on it.
    fn wake_what_touches(&mut self, id: BodyId) {
        let Some(b) = self.raw.bodies.get(id.0) else {
            return;
        };
        let mut others = Vec::new();
        for &c in b.colliders() {
            for pair in self.raw.narrow_phase.contact_pairs_with(c) {
                let other = if pair.collider1 == c {
                    pair.collider2
                } else {
                    pair.collider1
                };
                others.extend(self.raw.colliders.get(other).and_then(Collider::parent));
            }
        }
        for other in others {
            if let Some(b) = self.raw.bodies.get_mut(other) {
                b.wake_up(true);
            }
        }
    }

    /// Where a kinematic body is to be after the next step.
    pub fn set_target(&mut self, id: BodyId, pose: Pose) {
        if let Some(b) = self.raw.bodies.get_mut(id.0) {
            b.set_next_kinematic_position(pose.raw());
        }
    }

    /// Carries out what a body was asked to do.
    pub fn command(&mut self, id: BodyId, command: Command) {
        let Some(b) = self.raw.bodies.get_mut(id.0) else {
            return;
        };
        match command {
            Command::Force(f) => {
                b.add_force(v(f), true);
                self.forced.insert(id.0);
            }
            Command::ForceAt(f, at) => {
                b.add_force_at_point(v(f), v(at), true);
                self.forced.insert(id.0);
            }
            Command::Torque(t) => {
                b.add_torque(v(t), true);
                self.forced.insert(id.0);
            }
            Command::Impulse(i) => b.apply_impulse(v(i), true),
            Command::ImpulseAt(i, at) => b.apply_impulse_at_point(v(i), v(at), true),
            Command::TorqueImpulse(t) => b.apply_torque_impulse(v(t), true),
            Command::LinearVelocity(l) => b.set_linvel(v(l), true),
            Command::AngularVelocity(a) => b.set_angvel(v(a), true),
            Command::Reset(pose) => {
                b.set_position(pose.raw(), true);
                b.set_linvel(Vector::ZERO, true);
                b.set_angvel(Vector::ZERO, true);
                b.reset_forces(true);
                b.reset_torques(true);
            }
        }
    }

    /// Advances the world by `dt` seconds.
    pub fn step(&mut self, dt: f32) -> Stepped {
        self.raw.integration_parameters.dt = dt;
        let events = Events::default();
        self.raw.step_with_events(&(), &events);
        for h in self.forced.drain() {
            if let Some(b) = self.raw.bodies.get_mut(h) {
                b.reset_forces(false);
                b.reset_torques(false);
            }
        }
        let events = events.0.into_inner().unwrap_or_default();
        let mut out = Stepped::default();
        for event in events {
            self.tell(event, &mut out);
        }
        out
    }

    fn tell(&mut self, event: CollisionEvent, out: &mut Stepped) {
        let (c1, c2, started) = match event {
            CollisionEvent::Started(a, b, _) => (a, b, true),
            CollisionEvent::Stopped(a, b, _) => (a, b, false),
        };
        let owner = |c| {
            self.raw
                .colliders
                .get(c)
                .and_then(|c: &Collider| c.parent())
        };
        let (Some(b1), Some(b2)) = (owner(c1), owner(c2)) else {
            return;
        };
        let (t1, t2) = (self.is_trigger(b1), self.is_trigger(b2));
        // The two the same way round whichever collider rapier names first.
        let key = if b1.into_raw_parts() < b2.into_raw_parts() {
            (b1, b2)
        } else {
            (b2, b1)
        };
        let count = self.touching.entry(key).or_insert(0);
        if started {
            *count += 1;
            if *count > 1 {
                return;
            }
        } else {
            *count = count.saturating_sub(1);
            if *count > 0 {
                return;
            }
            self.touching.remove(&key);
        }
        if t1 || t2 {
            let (trigger, other) = if t1 { (b1, b2) } else { (b2, b1) };
            out.triggers.push(TriggerChange {
                trigger: BodyId(trigger),
                other: BodyId(other),
                entered: started,
            });
            if t1 && t2 {
                out.triggers.push(TriggerChange {
                    trigger: BodyId(b2),
                    other: BodyId(b1),
                    entered: started,
                });
            }
            return;
        }
        if started {
            out.contacts.push(self.contact(c1, c2, b1, b2));
        }
    }

    fn is_trigger(&self, h: RigidBodyHandle) -> bool {
        self.bodies.get(&h).is_some_and(|b| b.kind == Kind::Trigger)
    }

    fn contact(
        &self,
        c1: ColliderHandle,
        c2: ColliderHandle,
        b1: RigidBodyHandle,
        b2: RigidBodyHandle,
    ) -> Contact {
        let mut contact = Contact {
            a: BodyId(b1),
            b: BodyId(b2),
            positions: Vec::new(),
            impulses: Vec::new(),
            normals: Vec::new(),
        };
        let Some(pair) = self.raw.narrow_phase.contact_pair(c1, c2) else {
            return contact;
        };
        let at = self
            .raw
            .colliders
            .get(pair.collider1)
            .map_or(RPose::IDENTITY, |c| *c.position());
        // The pair may name its colliders the other way round from the
        // event; a normal says which way by the pair's own order.
        let sign = if pair.collider1 == c1 { -1.0 } else { 1.0 };
        for manifold in pair.manifolds() {
            let n = manifold.data.normal * sign;
            for point in &manifold.points {
                let p = at * point.local_p1;
                let i = n * point.data.impulse;
                contact.positions.push([p.x, p.y, p.z]);
                contact.impulses.push([i.x, i.y, i.z]);
                contact.normals.push([n.x, n.y, n.z]);
            }
        }
        contact
    }

    /// Where the body is.
    pub fn pose(&self, id: BodyId) -> Option<Pose> {
        self.raw.bodies.get(id.0).map(|b| Pose::of(b.position()))
    }

    /// Whether the body has come to rest.
    pub fn is_sleeping(&self, id: BodyId) -> bool {
        self.raw
            .bodies
            .get(id.0)
            .is_some_and(RigidBody::is_sleeping)
    }

    /// Whether a further step would move any body by itself: without one
    /// the host has no reason to step.
    pub fn has_motion(&self) -> bool {
        self.bodies.iter().any(|(h, body)| {
            body.kind == Kind::Dynamic && self.raw.bodies.get(*h).is_some_and(|b| !b.is_sleeping())
        })
    }

    /// How fast the body is moving, in units a second.
    pub fn linear_velocity(&self, id: BodyId) -> Option<Vec3> {
        let l = self.raw.bodies.get(id.0)?.linvel();
        Some([l.x, l.y, l.z])
    }

    /// Moves a kinematic body with a capsule upright on it by `by`, as far
    /// as what is in the way lets it, stepping over what is lower than
    /// `step_offset`. Takes effect in the next step.
    pub fn move_character(
        &mut self,
        id: BodyId,
        capsule: (f32, f32),
        step_offset: f32,
        by: Vec3,
        dt: f32,
    ) -> Moved {
        let Some(body) = self.raw.bodies.get(id.0) else {
            return Moved::default();
        };
        let at = *body.position();
        let (radius, half_height) = capsule;
        let shape = SharedShape::capsule_y(half_height, radius);
        let controller = KinematicCharacterController {
            offset: CharacterLength::Absolute(radius * 0.01),
            autostep: (step_offset > 0.0).then_some(CharacterAutostep {
                max_height: CharacterLength::Absolute(step_offset),
                min_width: CharacterLength::Absolute(radius * 0.5),
                include_dynamic_bodies: false,
            }),
            snap_to_ground: None,
            ..KinematicCharacterController::default()
        };
        let filter = QueryFilter::default()
            .exclude_rigid_body(id.0)
            .exclude_sensors();
        let queries = self.raw.query_pipeline_with_filter(filter);
        let mut out = Moved::default();
        let colliders = &self.raw.colliders;
        let moved = controller.move_shape(dt, &queries, &*shape, &at, v(by), |hit| {
            let n = hit.hit.normal1;
            // The normal looks away from what was hit: what it stands on
            // has a normal that looks up.
            if n.y > 0.5 {
                out.touches_down = true;
            } else if n.y < -0.5 {
                out.touches_up = true;
            } else {
                out.touches_side = true;
            }
            if let Some(other) = colliders.get(hit.handle).and_then(Collider::parent) {
                let p = hit.character_pos * hit.hit.witness1;
                out.hits.push(Hit {
                    other: BodyId(other),
                    position: [p.x, p.y, p.z],
                    normal: [n.x, n.y, n.z],
                });
            }
        });
        out.touches_down |= moved.grounded;
        let t = moved.translation;
        out.translation = [t.x, t.y, t.z];
        if let Some(b) = self.raw.bodies.get_mut(id.0) {
            let mut next = at;
            next.translation += t;
            b.set_next_kinematic_position(next);
        }
        out
    }
}

fn groups(filter: Filter, ccd: bool) -> InteractionGroups {
    // With CCD on, PhysX's own filter is what tells it which pairs to
    // sweep, and Qt leaves its groups out then.
    if ccd {
        return InteractionGroups::all();
    }
    InteractionGroups::new(
        Group::from_bits_truncate(1u32 << (filter.group & 31)),
        Group::from_bits_truncate(!filter.ignored),
        InteractionTestMode::And,
    )
}

#[cfg(test)]
mod tests;
