//! `PhysicsWorld`: steps the bodies of its scene from the animation clock.
//!
//! A frame first carries out what the step before it came to (who touched
//! what is told, where each body got to is written back) and only then
//! starts the next step, as Qt's does: QML reads a world in which one step
//! has finished, and what it asks in reply goes into the next. When nothing
//! can move the frame animation stops, and the application asks for no more
//! frames until something changes: a body, a command, gravity.

use std::collections::HashMap;
use std::rc::Rc;

use ljos::item::FrameAnimation;
use ljos::item::ljos_3d::math::{Mat4, Quat as MQuat, Vec3 as MVec3};
use ljos::ljos_engine::WriteKind;
use ljos::prelude::*;
use ljos_physics::{BodyId, Kind, Mass, Moved, Pose, Stepped, Tuning, Vec3, World};
use ljos_quick3d::{Node, parent_node, place};

use crate::bodies::{self, CharacterController, CharacterSpec, DynamicRigidBody, PhysicsNode};
use crate::bodies::{Registry, Spec, TriggerBody};

/// `PhysicsWorld`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Physics", name = "PhysicsWorld", extends = QtObject)]
pub struct PhysicsWorld {
    #[qml(base)]
    pub object: QtObject,
    /// The top node of what is simulated: only the bodies in it are.
    #[qml(notify, object = "Node")]
    pub scene: Option<ObjectId>,
    #[qml(notify)]
    pub gravity: Vector3d,
    #[qml(notify)]
    pub running: bool,
    /// Kept: nothing of a world is drawn.
    #[qml(notify)]
    pub force_debug_draw: bool,
    #[qml(notify, rename = "enableCCD", change = fixed_changed)]
    pub enable_ccd: bool,
    #[qml(notify, change = fixed_changed)]
    pub typical_length: f64,
    #[qml(notify, change = fixed_changed)]
    pub typical_speed: f64,
    #[qml(notify)]
    pub default_density: f64,
    #[qml(notify)]
    pub minimum_timestep: f64,
    #[qml(notify)]
    pub maximum_timestep: f64,
    /// Kept: nothing of a world is drawn.
    #[qml(notify, object = "Node")]
    pub viewport: Option<ObjectId>,
    /// Kept: a world steps on the application's thread.
    #[qml(notify)]
    pub num_threads: i32,
    #[qml(notify, change = fixed_changed)]
    pub report_kinematic_kinematic_collisions: bool,
    #[qml(notify, change = fixed_changed)]
    pub report_static_kinematic_collisions: bool,
    #[qml(skip)]
    sim: Option<Box<Sim>>,
    #[qml(skip)]
    clock: Option<ObjectId>,
    /// Something changed that the next frame has to look at.
    #[qml(skip)]
    woken: bool,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        PhysicsWorld {
            object: QtObject::default(),
            scene: None,
            gravity: Vector3d {
                x: 0.0,
                y: -981.0,
                z: 0.0,
            },
            running: true,
            force_debug_draw: false,
            enable_ccd: false,
            typical_length: 100.0,
            typical_speed: 1000.0,
            default_density: 0.001,
            minimum_timestep: 1.0,
            maximum_timestep: 33.333,
            viewport: None,
            num_threads: -1,
            report_kinematic_kinematic_collisions: false,
            report_static_kinematic_collisions: false,
            sim: None,
            clock: None,
            woken: false,
        }
    }
}

/// What a world keeps of a body it has.
struct Entry {
    body: BodyId,
    spec: Rc<Spec>,
    /// The density it was last weighed by, for one of the world's default.
    density: f64,
    last: Pose,
    /// How fast a character is falling, in the scene's space.
    falling: MVec3,
    /// What a character touched when it last moved.
    hit: Moved,
    sleeping: bool,
}

impl std::fmt::Debug for Sim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sim")
            .field("bodies", &self.entries.len())
            .finish()
    }
}

struct Sim {
    world: World,
    entries: HashMap<ObjectId, Entry>,
    owners: HashMap<BodyId, ObjectId>,
    waited: f64,
    /// A step's outcome, told at the next frame.
    stepped: Option<(Stepped, f64)>,
}

fn fixed_changed(cx: &Cx<'_>, _: PropIndex) {
    let begun = cx.with(|w: &PhysicsWorld| w.sim.is_some()).unwrap_or(false);
    if begun {
        cx.engine.warn(
            "PhysicsWorld: enableCCD, typicalLength, typicalSpeed and the collision reports \
             cannot change once it has started"
                .to_owned(),
        );
    }
}

#[qml_impl]
impl PhysicsWorld {
    // `timestep`, all lower case: the name a handler that declares no
    // parameters reads on Qt.
    #[qml(signal, name = "frameDone")]
    fn frame_done(timestep: f64);

    /// Its clock is a hidden `FrameAnimation`: the application asks for a
    /// frame only while that runs.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let clock = e.create_native(FrameAnimation::default(), Some(me))?;
        cx.with_mut(|w: &mut PhysicsWorld| w.clock = Some(clock))?;
        e.connect_by_name(clock, "triggered", move |e, _| frame(e, me))?;
        e.extension::<Registry>().worlds.borrow_mut().push(me);
        e.connect_by_name(me, "destroyed", move |e, _| {
            e.extension::<Registry>()
                .worlds
                .borrow_mut()
                .retain(|w| *w != me);
        })?;
        let mut was_running = true;
        e.effect_in(me, move |e| {
            let running = e.get(me, PhysicsWorld::RUNNING).unwrap_or(true);
            let gravity = e.get(me, PhysicsWorld::GRAVITY).unwrap_or_default();
            let _ = e.get(me, PhysicsWorld::SCENE);
            let _ = e.get(me, PhysicsWorld::DEFAULT_DENSITY);
            let restarted = running && !was_running;
            was_running = running;
            e.untrack(|| {
                let maximum = e.get(me, PhysicsWorld::MAXIMUM_TIMESTEP).unwrap_or(33.0);
                let _ = e.with_mut(me, |w: &mut PhysicsWorld| {
                    if let Some(sim) = &mut w.sim {
                        sim.world.set_gravity([gravity.x, gravity.y, gravity.z]);
                        // A step's worth of time, not all the time it stood.
                        if restarted {
                            sim.waited = maximum;
                        }
                    }
                });
                wake(e, me);
            });
        })?;
        Ok(())
    }
}

/// Starts the clock of every world: which of them something is for is worked
/// out in their next frame, by those that find nothing stopping again.
pub(crate) fn wake_all(e: &Engine) {
    let worlds = e.extension::<Registry>().worlds.borrow().clone();
    for world in worlds {
        wake(e, world);
    }
}

fn set_clock(e: &Engine, me: ObjectId, on: bool) {
    let clock = e.with(me, |w: &PhysicsWorld| w.clock).ok().flatten();
    if let Some(clock) = clock {
        let _ = e.write(
            clock,
            FrameAnimation::RUNNING.index,
            Value::Bool(on),
            WriteKind::Host,
        );
    }
}

fn wake(e: &Engine, me: ObjectId) {
    let (running, scene) = e.untrack(|| {
        (
            e.get(me, PhysicsWorld::RUNNING).unwrap_or(false),
            e.get(me, PhysicsWorld::SCENE).ok().flatten(),
        )
    });
    let _ = e.with_mut(me, |w: &mut PhysicsWorld| w.woken = true);
    set_clock(e, me, running && scene.is_some());
}

fn new_sim(e: &Engine, me: ObjectId) -> Sim {
    let gravity = e.get(me, PhysicsWorld::GRAVITY).unwrap_or_default();
    let tuning = Tuning {
        length: e.get(me, PhysicsWorld::TYPICAL_LENGTH).unwrap_or(100.0) as f32,
        speed: e.get(me, PhysicsWorld::TYPICAL_SPEED).unwrap_or(1000.0) as f32,
        ccd: e.get(me, PhysicsWorld::ENABLE_CCD).unwrap_or(false),
        report_kinematic_kinematic: e
            .get(me, PhysicsWorld::REPORT_KINEMATIC_KINEMATIC_COLLISIONS)
            .unwrap_or(false),
        report_static_kinematic: e
            .get(me, PhysicsWorld::REPORT_STATIC_KINEMATIC_COLLISIONS)
            .unwrap_or(false),
    };
    Sim {
        world: World::new(tuning, [gravity.x, gravity.y, gravity.z]),
        entries: HashMap::new(),
        owners: HashMap::new(),
        waited: 0.0,
        stepped: None,
    }
}

/// The sim, out of the world while it is worked on: what is told to QML may
/// come back to this world, and finds it there without a borrow held.
fn take_sim(e: &Engine, me: ObjectId) -> Option<Box<Sim>> {
    e.with_mut(me, |w: &mut PhysicsWorld| w.sim.take())
        .ok()
        .flatten()
}

fn put_sim(e: &Engine, me: ObjectId, sim: Box<Sim>) {
    let _ = e.with_mut(me, |w: &mut PhysicsWorld| w.sim = Some(sim));
}

fn frame(e: &Engine, me: ObjectId) {
    if !e.is_alive(me) {
        return;
    }
    e.untrack(|| e.batch(|| frame_inner(e, me)));
}

fn frame_inner(e: &Engine, me: ObjectId) {
    let Ok(Some(scene)) = e.get(me, PhysicsWorld::SCENE) else {
        return set_clock(e, me, false);
    };
    let fresh_here = bodies::any_fresh_in(e, scene);
    let first = e
        .with(me, |w: &PhysicsWorld| w.sim.is_none())
        .unwrap_or(false);
    if first && !fresh_here {
        // Nothing to simulate yet, and a body coming wakes this.
        return set_clock(e, me, false);
    }
    // Nothing falls until all it may land on is there.
    if waiting_for_files(e, me, scene) {
        return set_clock(e, me, false);
    }
    let minimum = e
        .get(me, PhysicsWorld::MINIMUM_TIMESTEP)
        .unwrap_or(1.0)
        .max(0.0);
    let maximum = e
        .get(me, PhysicsWorld::MAXIMUM_TIMESTEP)
        .unwrap_or(33.333)
        .max(minimum);
    let mut sim = take_sim(e, me).unwrap_or_else(|| Box::new(new_sim(e, me)));
    let frame_ms = clock_ms(e, me);
    let _ = e.with_mut(me, |w: &mut PhysicsWorld| w.woken = false);
    finish(e, me, scene, &mut sim);

    if !can_move(e, &sim) && !bodies::any_fresh_in(e, scene) {
        sim.waited = 0.0;
        put_sim(e, me, sim);
        // What it told may have asked for something.
        let woken = e.with(me, |w: &PhysicsWorld| w.woken).unwrap_or(false);
        if !woken {
            set_clock(e, me, false);
        }
        return;
    }

    sim.waited += frame_ms;
    if sim.waited >= minimum {
        let ms = sim.waited.min(maximum);
        sim.waited = 0.0;
        let seconds = ms as f32 * 0.001;
        move_characters(e, &mut sim, seconds);
        let stepped = sim.world.step(seconds);
        sim.stepped = Some((stepped, ms));
    }
    put_sim(e, me, sim);
}

/// How long since the last frame, in milliseconds.
fn clock_ms(e: &Engine, me: ObjectId) -> f64 {
    let clock = e.with(me, |w: &PhysicsWorld| w.clock).ok().flatten();
    clock
        .and_then(|c| e.get(c, FrameAnimation::FRAME_TIME).ok())
        .unwrap_or(0.0)
        * 1000.0
}

fn waiting_for_files(e: &Engine, me: ObjectId, scene: ObjectId) -> bool {
    let registry = e.extension::<Registry>();
    let fresh = registry.fresh.borrow();
    let waits = |o: &ObjectId| {
        e.with(*o, |n: &PhysicsNode| {
            n.state.spec.as_ref().is_some_and(|s| s.waiting)
        })
        .unwrap_or(false)
    };
    if fresh
        .iter()
        .any(|o| e.is_alive(*o) && ljos_quick3d::in_scene(e, *o, scene) && waits(o))
    {
        return true;
    }
    e.with(me, |w: &PhysicsWorld| {
        w.sim
            .as_ref()
            .is_some_and(|s| s.entries.keys().any(|o| e.is_alive(*o) && waits(o)))
    })
    .unwrap_or(false)
}

/// Whether anything will change if this world goes on stepping.
fn can_move(e: &Engine, sim: &Sim) -> bool {
    if sim.world.has_motion() || sim.stepped.is_some() {
        return true;
    }
    sim.entries.iter().any(|(o, entry)| {
        let asked = e
            .with(*o, |n: &PhysicsNode| {
                !n.state.commands.is_empty() || n.state.teleport.is_some()
            })
            .unwrap_or(false);
        asked
            || entry.spec.character.is_some_and(|c| {
                let walks = c.movement != MVec3::ZERO;
                let falls = c.gravity != MVec3::ZERO && !grounded(&c, &entry.hit, entry.falling);
                walks || falls
            })
            || (entry.spec.kind == Kind::Kinematic
                && entry.spec.character.is_none()
                && entry.spec.target.is_some_and(|t| t != entry.last))
    })
}

/// What a step came to, then what is new since: QML hears of the step in a
/// world that is between steps.
fn finish(e: &Engine, me: ObjectId, scene: ObjectId, sim: &mut Sim) {
    let stepped = sim.stepped.take();
    let density = e.get(me, PhysicsWorld::DEFAULT_DENSITY).unwrap_or(0.001);
    if let Some((stepped, _)) = &stepped {
        tell_triggers(e, sim, stepped);
    }
    adopt(e, scene, sim, density);
    if let Some((stepped, _)) = &stepped {
        tell_contacts(e, sim, stepped);
    }
    remove_gone(e, sim);
    apply_changes(e, sim, density);
    write_back(e, sim);
    tell_hits(e, sim);
    if let Some((_, ms)) = stepped {
        let _ = e.emit_by_name(me, "frameDone", &[Value::Real(ms)]);
    }
}

fn adopt(e: &Engine, scene: ObjectId, sim: &mut Sim, density: f64) {
    for o in bodies::fresh_in(e, scene) {
        let Ok(Some(spec)) = e.with(o, |n: &PhysicsNode| n.state.spec.clone()) else {
            continue;
        };
        if !spec.enabled {
            // Taken when it is enabled: its spec changing wakes the world.
            e.extension::<Registry>().fresh.borrow_mut().push(o);
            continue;
        }
        add(e, sim, o, spec, density);
    }
}

fn scene_pose(e: &Engine, o: ObjectId) -> Pose {
    let p = place(e, o);
    let at = p.world.translation();
    Pose {
        position: [at.x, at.y, at.z],
        rotation: [p.rotation.w, p.rotation.x, p.rotation.y, p.rotation.z],
    }
}

fn add(e: &Engine, sim: &mut Sim, o: ObjectId, spec: Rc<Spec>, density: f64) {
    if spec.forced {
        e.warn(
            "Cannot make body containing trimesh/heightfield/plane non-kinematic, \
             forcing kinematic."
                .to_owned(),
        );
    }
    let pose = scene_pose(e, o);
    let body = sim.world.add_body(spec.kind, pose);
    sim.owners.insert(body, o);
    configure(sim, body, &spec, density);
    sim.entries.insert(
        o,
        Entry {
            body,
            spec,
            density,
            last: pose,
            falling: MVec3::ZERO,
            hit: Moved::default(),
            sleeping: false,
        },
    );
}

fn configure(sim: &mut Sim, body: BodyId, spec: &Spec, density: f64) {
    let world = &mut sim.world;
    world.set_material(body, spec.material);
    world.set_filter(body, spec.filter);
    world.set_parts(body, spec.parts.clone());
    world.set_gravity_enabled(body, spec.gravity);
    world.set_locks(body, spec.locks);
    weigh(sim, body, spec, density);
    if let Some(target) = spec.target
        && spec.kind == Kind::Kinematic
    {
        sim.world.set_target(body, target);
    }
}

fn weigh(sim: &mut Sim, body: BodyId, spec: &Spec, density: f64) {
    if spec.kind == Kind::Dynamic {
        let mass = spec.mass.unwrap_or(Mass::Density(density as f32));
        sim.world.set_mass(body, mass);
    }
}

/// Takes what changed of each body since the world last looked.
fn apply_changes(e: &Engine, sim: &mut Sim, density: f64) {
    let objects: Vec<ObjectId> = sim.entries.keys().copied().collect();
    for o in objects {
        let Ok((Some(now), commands, teleport)) = e.with_mut(o, |n: &mut PhysicsNode| {
            (
                n.state.spec.clone(),
                std::mem::take(&mut n.state.commands),
                n.state.teleport.take(),
            )
        }) else {
            continue;
        };
        let Some(entry) = sim.entries.get(&o) else {
            continue;
        };
        let body = entry.body;
        if !Rc::ptr_eq(&now, &entry.spec) || (now.mass.is_none() && entry.density != density) {
            let was = entry.spec.clone();
            if !now.enabled {
                remove(sim, o);
                e.extension::<Registry>().fresh.borrow_mut().push(o);
                continue;
            }
            update(e, sim, body, &was, &now, density);
            if let Some(entry) = sim.entries.get_mut(&o) {
                entry.spec = now.clone();
                entry.density = density;
            }
        }
        for command in commands {
            sim.world.command(body, command);
        }
        if let Some(position) = teleport {
            let pose = Pose {
                position,
                rotation: now.target.map_or(ljos_physics::NO_TURN, |t| t.rotation),
            };
            sim.world.place(body, pose);
            if let Some(entry) = sim.entries.get_mut(&o) {
                entry.falling = MVec3::ZERO;
                entry.hit = Moved::default();
                entry.last = pose;
            }
            write_pose(e, o, pose);
        }
    }
}

fn update(e: &Engine, sim: &mut Sim, body: BodyId, was: &Spec, now: &Spec, density: f64) {
    if now.material != was.material {
        sim.world.set_material(body, now.material);
    }
    if now.filter != was.filter {
        sim.world.set_filter(body, now.filter);
    }
    if now.parts != was.parts {
        sim.world.set_parts(body, now.parts.clone());
    }
    if now.gravity != was.gravity {
        sim.world.set_gravity_enabled(body, now.gravity);
    }
    if now.locks != was.locks {
        sim.world.set_locks(body, now.locks);
    }
    if now.kind != was.kind
        && [now.kind, was.kind]
            .iter()
            .all(|k| matches!(k, Kind::Dynamic | Kind::Kinematic))
    {
        if now.forced {
            e.warn(
                "Cannot make a body containing trimesh/heightfield/plane non-kinematic, \
                 ignoring."
                    .to_owned(),
            );
        }
        sim.world.set_kinematic(body, now.kind == Kind::Kinematic);
    }
    weigh(sim, body, now, density);
    match (now.kind, now.target) {
        (Kind::Kinematic, Some(target)) => sim.world.set_target(body, target),
        (Kind::Static | Kind::Trigger, Some(target)) => sim.world.place(body, target),
        _ => {}
    }
}

fn remove(sim: &mut Sim, o: ObjectId) {
    if let Some(entry) = sim.entries.remove(&o) {
        sim.owners.remove(&entry.body);
        sim.world.remove_body(entry.body);
    }
}

fn remove_gone(e: &Engine, sim: &mut Sim) {
    let gone: Vec<ObjectId> = e
        .extension::<Registry>()
        .gone
        .borrow()
        .iter()
        .copied()
        .collect();
    for o in gone {
        remove(sim, o);
    }
    // Every world looks through the list, so none is emptied by one: what is
    // dead is gone for all of them by the next frame anyway.
    let dead: Vec<ObjectId> = sim
        .entries
        .keys()
        .copied()
        .filter(|o| !e.is_alive(*o))
        .collect();
    for o in dead {
        remove(sim, o);
    }
    e.extension::<Registry>().gone.borrow_mut().clear();
}

fn write_pose(e: &Engine, o: ObjectId, pose: Pose) {
    let above = parent_node(e, o).map(|n| place(e, n));
    let (position, rotation) = match above {
        Some(a) => {
            let inverse = a.world.inverse().unwrap_or(Mat4::IDENTITY);
            let p = inverse.transform_point(MVec3::new(
                pose.position[0],
                pose.position[1],
                pose.position[2],
            ));
            let q = a.rotation.conjugate() * quat(pose.rotation);
            (p, q)
        }
        None => (
            MVec3::new(pose.position[0], pose.position[1], pose.position[2]),
            quat(pose.rotation),
        ),
    };
    let at = Vector3d {
        x: position.x,
        y: position.y,
        z: position.z,
    };
    if e.get(o, Node::POSITION).ok() != Some(at) {
        ljos_quick3d::host_write(e, o, Node::POSITION, at);
    }
    let turn = Quaternion {
        scalar: rotation.w,
        x: rotation.x,
        y: rotation.y,
        z: rotation.z,
    };
    if e.get(o, Node::ROTATION).ok() != Some(turn) {
        ljos_quick3d::host_write(e, o, Node::ROTATION, turn);
    }
}

fn quat(q: ljos_physics::Quat) -> MQuat {
    MQuat {
        w: q[0],
        x: q[1],
        y: q[2],
        z: q[3],
    }
}

/// Writes where each body got to back to its node.
fn write_back(e: &Engine, sim: &mut Sim) {
    let objects: Vec<ObjectId> = sim.entries.keys().copied().collect();
    for o in objects {
        let Some(entry) = sim.entries.get(&o) else {
            continue;
        };
        if matches!(entry.spec.kind, Kind::Static | Kind::Trigger) {
            continue;
        }
        let body = entry.body;
        let sleeping = sim.world.is_sleeping(body);
        let Some(pose) = sim.world.pose(body) else {
            continue;
        };
        let moved = pose != entry.last;
        let was_sleeping = entry.sleeping;
        if moved {
            write_pose(e, o, pose);
        }
        if sleeping != was_sleeping && e.is::<DynamicRigidBody>(o) {
            let _ = e.set_internal(o, DynamicRigidBody::IS_SLEEPING, sleeping);
        }
        if let Some(entry) = sim.entries.get_mut(&o) {
            entry.last = pose;
            entry.sleeping = sleeping;
        }
    }
}

fn vectors(list: &[Vec3], sign: f32) -> Value {
    Value::list(
        list.iter()
            .map(|v| {
                Value::Vector3d(Vector3d {
                    x: v[0] * sign,
                    y: v[1] * sign,
                    z: v[2] * sign,
                })
            })
            .collect(),
    )
}

fn flag(e: &Engine, o: ObjectId, p: Prop<bool>) -> bool {
    e.is_alive(o) && e.get(o, p).unwrap_or(false)
}

fn tell_contacts(e: &Engine, sim: &Sim, stepped: &Stepped) {
    for contact in &stepped.contacts {
        let (Some(a), Some(b)) = (sim.owners.get(&contact.a), sim.owners.get(&contact.b)) else {
            continue;
        };
        for (to, from, sign) in [(*a, *b, 1.0), (*b, *a, -1.0)] {
            if flag(e, to, PhysicsNode::RECEIVE_CONTACT_REPORTS)
                && flag(e, from, PhysicsNode::SEND_CONTACT_REPORTS)
            {
                let _ = e.emit_by_name(
                    to,
                    "bodyContact",
                    &[
                        Value::Object(from),
                        vectors(&contact.positions, 1.0),
                        vectors(&contact.impulses, sign),
                        vectors(&contact.normals, sign),
                    ],
                );
            }
        }
    }
}

fn tell_triggers(e: &Engine, sim: &Sim, stepped: &Stepped) {
    for change in &stepped.triggers {
        let (Some(trigger), Some(other)) = (
            sim.owners.get(&change.trigger).copied(),
            sim.owners.get(&change.other).copied(),
        ) else {
            continue;
        };
        if flag(e, other, PhysicsNode::SEND_TRIGGER_REPORTS) && e.is::<TriggerBody>(trigger) {
            let told = e
                .with_mut(trigger, |n: &mut PhysicsNode| {
                    let at = n.state.inside.iter().position(|o| *o == other);
                    match (change.entered, at) {
                        (true, None) => n.state.inside.push(other),
                        (false, Some(at)) => {
                            n.state.inside.swap_remove(at);
                        }
                        _ => return None,
                    }
                    Some(n.state.inside.len() as i32)
                })
                .ok()
                .flatten();
            if let Some(count) = told {
                let _ = e.set_internal(trigger, TriggerBody::COLLISION_COUNT, count);
                let name = if change.entered {
                    "bodyEntered"
                } else {
                    "bodyExited"
                };
                let _ = e.emit_by_name(trigger, name, &[Value::Object(other)]);
            }
        }
        if flag(e, other, PhysicsNode::RECEIVE_TRIGGER_REPORTS) {
            let name = if change.entered {
                "enteredTriggerBody"
            } else {
                "exitedTriggerBody"
            };
            let _ = e.emit_by_name(other, name, &[Value::Object(trigger)]);
        }
    }
}

fn tell_hits(e: &Engine, sim: &mut Sim) {
    let mut told = Vec::new();
    for (o, entry) in &mut sim.entries {
        let Some(c) = entry.spec.character else {
            continue;
        };
        let hit = &mut entry.hit;
        let collisions = i32::from(hit.touches_side)
            + 2 * i32::from(hit.touches_up)
            + 4 * i32::from(hit.touches_down);
        let _ = e.set_internal(*o, CharacterController::COLLISIONS, collisions);
        if c.tells_hits {
            for h in hit.hits.drain(..) {
                told.push((*o, h));
            }
        }
    }
    for (o, hit) in told {
        let Some(other) = sim.owners.get(&hit.other) else {
            continue;
        };
        let v = |v: Vec3| {
            Value::Vector3d(Vector3d {
                x: v[0],
                y: v[1],
                z: v[2],
            })
        };
        let _ = e.emit_by_name(
            o,
            "shapeHit",
            &[
                Value::Object(*other),
                v(hit.position),
                v([0.0; 3]),
                v(hit.normal),
            ],
        );
    }
}

fn grounded(c: &CharacterSpec, hit: &Moved, falling: MVec3) -> bool {
    let pull = c.gravity;
    let (up, down, side) = (hit.touches_up, hit.touches_down, hit.touches_side);
    (pull.y < 0.0 && (down || (up && falling.y > 0.0)))
        || (pull.y > 0.0 && (up || (down && falling.y < 0.0)))
        || ((pull.x != 0.0 || pull.z != 0.0) && side)
}

fn scaled(v: MVec3, by: f32) -> MVec3 {
    MVec3::new(v.x * by, v.y * by, v.z * by)
}

fn plus(a: MVec3, b: MVec3) -> MVec3 {
    MVec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

/// How far a character goes this step: where it is told to walk, turned as
/// it is, and what gravity has done to it.
fn displacement(c: &CharacterSpec, hit: &Moved, falling: &mut MVec3, seconds: f32) -> MVec3 {
    let mut moved = scaled(c.turn.rotate(c.movement), seconds);
    let pull = c.gravity;
    if pull == MVec3::ZERO {
        return moved;
    }
    if grounded(c, hit, *falling) {
        *falling = plus(scaled(moved, 1.0 / seconds), scaled(pull, seconds));
        if c.mid_air_control {
            // Only along the pull: walking in the air is by `movement`.
            let length = pull.length();
            let along = scaled(pull, falling.dot(pull) / (length * length));
            *falling = along;
        }
    } else {
        if !c.mid_air_control {
            moved = MVec3::ZERO;
        }
        moved = plus(moved, scaled(*falling, seconds));
        *falling = plus(*falling, scaled(pull, seconds));
    }
    plus(moved, scaled(pull, 0.5 * seconds * seconds))
}

fn move_characters(_: &Engine, sim: &mut Sim, seconds: f32) {
    let Sim { world, entries, .. } = sim;
    for entry in entries.values_mut() {
        let Some(c) = entry.spec.character else {
            continue;
        };
        let by = displacement(&c, &entry.hit, &mut entry.falling, seconds);
        entry.hit = world.move_character(
            entry.body,
            (c.radius, c.half_height),
            c.step_offset,
            [by.x, by.y, by.z],
            seconds,
        );
    }
}
