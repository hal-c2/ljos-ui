//! What makes particles: emitters, their bursts and their directions.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use ljos::item::ljos_3d::math::{Mat4, Quat};
use ljos::item::ljos_3d::particles::{Burst, Direction, Emitter, Trigger};
use ljos::prelude::*;

use super::system::{self, Member};
use super::{floats, shapes};
use crate::node::{IDENTITY, Object3D, quat, vec3};
use crate::{Node, space};

/// `Direction3D`: what the directions have.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "Direction3D",
    extends = QtObject,
    uncreatable
)]
pub struct Direction3D {
    #[qml(base)]
    pub object: QtObject,
}

/// `VectorDirection3D`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Particles3D", name = "VectorDirection3D", extends = Direction3D)]
pub struct VectorDirection3D {
    #[qml(base)]
    pub base: Direction3D,
    #[qml(notify)]
    pub direction: Vector3d,
    #[qml(notify)]
    pub direction_variation: Vector3d,
    #[qml(notify)]
    pub normalized: bool,
}

impl Default for VectorDirection3D {
    fn default() -> Self {
        VectorDirection3D {
            base: Direction3D::default(),
            direction: Vector3d {
                x: 0.0,
                y: 100.0,
                z: 0.0,
            },
            direction_variation: Vector3d::default(),
            normalized: false,
        }
    }
}

/// `TargetDirection3D`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Particles3D", name = "TargetDirection3D", extends = Direction3D)]
pub struct TargetDirection3D {
    #[qml(base)]
    pub base: Direction3D,
    #[qml(notify)]
    pub position: Vector3d,
    #[qml(notify)]
    pub position_variation: Vector3d,
    #[qml(notify)]
    pub normalized: bool,
    #[qml(notify)]
    pub magnitude: f64,
    #[qml(notify)]
    pub magnitude_variation: f64,
}

impl Default for TargetDirection3D {
    fn default() -> Self {
        TargetDirection3D {
            base: Direction3D::default(),
            position: Vector3d::default(),
            position_variation: Vector3d::default(),
            normalized: false,
            magnitude: 1.0,
            magnitude_variation: 0.0,
        }
    }
}

/// `EmitBurst3D`: so many particles there from the start of the system.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Particles3D", name = "EmitBurst3D")]
pub struct EmitBurst3D {
    #[qml(notify)]
    pub time: i32,
    #[qml(notify)]
    pub amount: i32,
    #[qml(notify)]
    pub duration: i32,
}

fn trigger_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "TriggerMode",
        vec![("TriggerTime", 0), ("TriggerStart", 1), ("TriggerEnd", 2)],
    )]
}

/// `DynamicBurst3D`: the same, when the system's time comes past `time`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "DynamicBurst3D",
    extends = EmitBurst3D,
    enums = trigger_enums
)]
pub struct DynamicBurst3D {
    #[qml(base)]
    pub burst: EmitBurst3D,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub amount_variation: i32,
    #[qml(notify, enumeration = "TriggerMode")]
    pub trigger_mode: i32,
}

impl Default for DynamicBurst3D {
    fn default() -> Self {
        DynamicBurst3D {
            burst: EmitBurst3D::default(),
            enabled: true,
            amount_variation: 0,
            trigger_mode: 0,
        }
    }
}

fn emit_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "EmitMode",
        vec![
            ("Default", 0),
            ("SurfaceNormal", 1),
            ("SurfaceReflected", 2),
        ],
    )]
}

/// `ParticleEmitter3D`. `emitMode` and `depthBias` are kept and do nothing.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "ParticleEmitter3D",
    extends = Node,
    enums = emit_enums
)]
pub struct ParticleEmitter3D {
    #[qml(base)]
    pub node: Node,
    #[qml(notify, object = "ParticleSystem3D")]
    pub system: Option<ObjectId>,
    #[qml(notify, list)]
    pub emit_bursts: QmlList,
    #[qml(notify, object = "Direction3D")]
    pub velocity: Option<ObjectId>,
    #[qml(notify, object = "Particle3D")]
    pub particle: Option<ObjectId>,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify, object = "ParticleAbstractShape3D")]
    pub shape: Option<ObjectId>,
    #[qml(notify)]
    pub emit_rate: f64,
    #[qml(notify)]
    pub life_span: i32,
    #[qml(notify)]
    pub life_span_variation: i32,
    #[qml(notify)]
    pub particle_scale: f64,
    #[qml(notify)]
    pub particle_end_scale: f64,
    #[qml(notify)]
    pub particle_scale_variation: f64,
    #[qml(notify)]
    pub particle_end_scale_variation: f64,
    #[qml(notify)]
    pub particle_rotation: Vector3d,
    #[qml(notify)]
    pub particle_rotation_variation: Vector3d,
    #[qml(notify)]
    pub particle_rotation_velocity: Vector3d,
    #[qml(notify)]
    pub particle_rotation_velocity_variation: Vector3d,
    #[qml(notify)]
    pub depth_bias: f64,
    #[qml(notify)]
    pub reversed: bool,
    #[qml(notify, enumeration = "EmitMode")]
    pub emit_mode: i32,
}

impl Default for ParticleEmitter3D {
    fn default() -> Self {
        ParticleEmitter3D {
            node: Node::default(),
            system: None,
            emit_bursts: QmlList::default(),
            velocity: None,
            particle: None,
            enabled: true,
            shape: None,
            emit_rate: 0.0,
            life_span: 1000,
            life_span_variation: 0,
            particle_scale: 1.0,
            particle_end_scale: -1.0,
            particle_scale_variation: 0.0,
            particle_end_scale_variation: -1.0,
            particle_rotation: Vector3d::default(),
            particle_rotation_variation: Vector3d::default(),
            particle_rotation_velocity: Vector3d::default(),
            particle_rotation_velocity_variation: Vector3d::default(),
            depth_bias: 0.0,
            reversed: false,
            emit_mode: 0,
        }
    }
}

#[qml_impl]
impl ParticleEmitter3D {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        system::enrol(
            cx.engine,
            cx.this,
            ParticleEmitter3D::SYSTEM,
            Member::Emitter,
        )
    }

    /// So many particles now, or spread over `duration` from now, and with
    /// `position` that far from where the emitter would start them.
    #[qml(invokable, name = "burst")]
    fn burst(cx: &Cx, count: Value, duration: Value, position: Value) {
        let (e, me) = (cx.engine, cx.this);
        let count = count.to_number().max(0.0) as u32;
        let duration = if matches!(duration, Value::Undefined) {
            0.0
        } else {
            duration.to_number().max(0.0)
        };
        let position = match position {
            Value::Vector3d(v) => Some(vec3(v)),
            _ => None,
        };
        system::burst(e, me, count, duration, position);
    }
}

/// `TrailEmitter3D`: emits where the particles of another kind are.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Particles3D", name = "TrailEmitter3D", extends = ParticleEmitter3D)]
pub struct TrailEmitter3D {
    #[qml(base)]
    pub emitter: ParticleEmitter3D,
    #[qml(notify, object = "Particle3D")]
    pub follow: Option<ObjectId>,
    /// What `burst` asked for since the system last moved, with when.
    #[qml(skip)]
    pub(super) asked: Vec<(u32, f64)>,
}

#[qml_impl]
impl TrailEmitter3D {
    /// So many for each of the particles followed, when the time next moves.
    #[qml(invokable, name = "burst")]
    fn burst(cx: &Cx, count: Value) {
        system::ask(cx.engine, cx.this, count.to_number().max(0.0) as u32);
    }
}

fn id_of(obj: ObjectId) -> u64 {
    let mut h = DefaultHasher::new();
    obj.hash(&mut h);
    h.finish()
}

/// An emitter's bursts: those `emitBursts` names and those declared in it,
/// the last first. Tracked.
fn bursts(e: &Engine, obj: ObjectId) -> Vec<Burst> {
    let mut found: Vec<ObjectId> = Vec::new();
    for list in [ParticleEmitter3D::EMIT_BURSTS.index, Object3D::DATA.index] {
        if let Value::List(l) = e.read(obj, list) {
            for b in l.iter().filter_map(Value::as_object) {
                if e.is::<EmitBurst3D>(b) && !found.contains(&b) {
                    found.push(b);
                }
            }
        }
    }
    found.reverse();
    found
        .into_iter()
        .map(|b| {
            let dynamic = e.is::<DynamicBurst3D>(b);
            Burst {
                id: id_of(b),
                time: f64::from(e.get(b, EmitBurst3D::TIME).unwrap_or(0)),
                amount: e.get(b, EmitBurst3D::AMOUNT).unwrap_or(0) as f32,
                duration: f64::from(e.get(b, EmitBurst3D::DURATION).unwrap_or(0)),
                dynamic,
                enabled: !dynamic || e.get(b, DynamicBurst3D::ENABLED).unwrap_or(true),
                amount_variation: if dynamic {
                    e.get(b, DynamicBurst3D::AMOUNT_VARIATION).unwrap_or(0) as f32
                } else {
                    0.0
                },
                trigger: match dynamic.then(|| e.get(b, DynamicBurst3D::TRIGGER_MODE)) {
                    Some(Ok(1)) => Trigger::Start,
                    Some(Ok(2)) => Trigger::End,
                    _ => Trigger::Time,
                },
            }
        })
        .collect()
}

fn direction(e: &Engine, obj: Option<ObjectId>) -> Direction {
    let Some(obj) = obj else {
        return Direction::default();
    };
    if e.is::<VectorDirection3D>(obj) {
        return Direction::Vector {
            direction: floats(e.get(obj, VectorDirection3D::DIRECTION).unwrap_or_default()),
            variation: floats(
                e.get(obj, VectorDirection3D::DIRECTION_VARIATION)
                    .unwrap_or_default(),
            ),
            normalized: e.get(obj, VectorDirection3D::NORMALIZED).unwrap_or(false),
        };
    }
    if e.is::<TargetDirection3D>(obj) {
        return Direction::Target {
            position: floats(e.get(obj, TargetDirection3D::POSITION).unwrap_or_default()),
            variation: floats(
                e.get(obj, TargetDirection3D::POSITION_VARIATION)
                    .unwrap_or_default(),
            ),
            normalized: e.get(obj, TargetDirection3D::NORMALIZED).unwrap_or(false),
            magnitude: e.get(obj, TargetDirection3D::MAGNITUDE).unwrap_or(1.0) as f32,
            magnitude_variation: e
                .get(obj, TargetDirection3D::MAGNITUDE_VARIATION)
                .unwrap_or(0.0) as f32,
        };
    }
    Direction::default()
}

/// What the emitter `obj` says now, seen from its system: `from` is the
/// system's world inverted, `system_turn` the system's turn. Tracked.
pub(super) fn read(e: &Engine, obj: ObjectId, from: Mat4, system_turn: Quat) -> Emitter {
    use ParticleEmitter3D as P;
    let at = space::place(e, obj);
    let v = |p| floats(e.get(obj, p).unwrap_or_default());
    Emitter {
        enabled: e.get(obj, P::ENABLED).unwrap_or(true),
        emit_rate: e.get(obj, P::EMIT_RATE).unwrap_or(0.0),
        life_span: e.get(obj, P::LIFE_SPAN).unwrap_or(0) as f32,
        life_span_variation: e.get(obj, P::LIFE_SPAN_VARIATION).unwrap_or(0) as f32,
        scale: e.get(obj, P::PARTICLE_SCALE).unwrap_or(1.0) as f32,
        end_scale: e.get(obj, P::PARTICLE_END_SCALE).unwrap_or(-1.0) as f32,
        scale_variation: e.get(obj, P::PARTICLE_SCALE_VARIATION).unwrap_or(0.0) as f32,
        end_scale_variation: e.get(obj, P::PARTICLE_END_SCALE_VARIATION).unwrap_or(-1.0) as f32,
        rotation: v(P::PARTICLE_ROTATION),
        rotation_variation: v(P::PARTICLE_ROTATION_VARIATION),
        spin: v(P::PARTICLE_ROTATION_VELOCITY),
        spin_variation: v(P::PARTICLE_ROTATION_VELOCITY_VARIATION),
        reversed: e.get(obj, P::REVERSED).unwrap_or(false),
        shape: shapes::read(e, e.get(obj, P::SHAPE).ok().flatten()),
        direction: direction(e, e.get(obj, P::VELOCITY).ok().flatten()),
        place: from * at.world,
        turn: system_turn.conjugate() * at.rotation,
        own_turn: quat(e.get(obj, Node::ROTATION).unwrap_or(IDENTITY)),
        bursts: bursts(e, obj),
    }
}
