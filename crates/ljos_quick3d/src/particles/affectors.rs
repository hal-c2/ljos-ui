//! What bends a particle's way once it is under way.

use ljos::item::ljos_3d::math::{Mat4, Vec3};
use ljos::item::ljos_3d::particles::{Affector, CustomAffect, ScaleCurve};
use ljos::prelude::*;

use super::system::{self, Member};
use super::{floats, shapes};
use crate::Node;
use crate::node::vec3;
use crate::space;

/// `Affector3D`: what the affectors have. `particles` names the kinds it is
/// for; none named, it is for all of its system's.
///
/// An application's own affector is a type that extends this one and hands
/// what it does to [`Affector3D::set_custom`].
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "Affector3D",
    extends = Node,
    uncreatable
)]
pub struct Affector3D {
    #[qml(base)]
    pub node: Node,
    #[qml(notify, object = "ParticleSystem3D")]
    pub system: Option<ObjectId>,
    #[qml(notify, list)]
    pub particles: QmlList,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(skip)]
    custom: Option<CustomAffect>,
}

impl Default for Affector3D {
    fn default() -> Self {
        Affector3D {
            node: Node::default(),
            system: None,
            particles: QmlList::default(),
            enabled: true,
            custom: None,
        }
    }
}

impl Affector3D {
    /// Makes the affector `me` do what `affect` does; a type's effect calls
    /// it again with a new one when the type's properties change.
    pub fn set_custom(e: &Engine, me: ObjectId, affect: CustomAffect) {
        let _ = e.with_mut(me, |a: &mut Affector3D| a.custom = Some(affect));
        let _ = e.notify(me, Affector3D::ENABLED.index);
    }
}

#[qml_impl]
impl Affector3D {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        system::enrol(cx.engine, cx.this, Affector3D::SYSTEM, Member::Affector)
    }
}

const UP: Vector3d = Vector3d {
    x: 0.0,
    y: 1.0,
    z: 0.0,
};

/// `Gravity3D`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Particles3D", name = "Gravity3D", extends = Affector3D)]
pub struct Gravity3D {
    #[qml(base)]
    pub affector: Affector3D,
    #[qml(notify)]
    pub magnitude: f64,
    #[qml(notify)]
    pub direction: Vector3d,
}

impl Default for Gravity3D {
    fn default() -> Self {
        Gravity3D {
            affector: Affector3D::default(),
            magnitude: 100.0,
            direction: Vector3d {
                x: 0.0,
                y: -1.0,
                z: 0.0,
            },
        }
    }
}

/// `PointRotator3D`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Particles3D", name = "PointRotator3D", extends = Affector3D)]
pub struct PointRotator3D {
    #[qml(base)]
    pub affector: Affector3D,
    #[qml(notify)]
    pub magnitude: f64,
    #[qml(notify)]
    pub direction: Vector3d,
    #[qml(notify)]
    pub pivot_point: Vector3d,
}

impl Default for PointRotator3D {
    fn default() -> Self {
        PointRotator3D {
            affector: Affector3D::default(),
            magnitude: 10.0,
            direction: UP,
            pivot_point: Vector3d::default(),
        }
    }
}

/// `Wander3D`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Particles3D", name = "Wander3D", extends = Affector3D)]
pub struct Wander3D {
    #[qml(base)]
    pub affector: Affector3D,
    #[qml(notify)]
    pub global_amount: Vector3d,
    #[qml(notify)]
    pub global_pace: Vector3d,
    #[qml(notify)]
    pub global_pace_start: Vector3d,
    #[qml(notify)]
    pub unique_amount: Vector3d,
    #[qml(notify)]
    pub unique_pace: Vector3d,
    #[qml(notify)]
    pub unique_amount_variation: f64,
    #[qml(notify)]
    pub unique_pace_variation: f64,
    #[qml(notify)]
    pub fade_in_duration: i32,
    #[qml(notify)]
    pub fade_out_duration: i32,
}

/// `Attractor3D`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Particles3D", name = "Attractor3D", extends = Affector3D)]
pub struct Attractor3D {
    #[qml(base)]
    pub affector: Affector3D,
    #[qml(notify)]
    pub position_variation: Vector3d,
    #[qml(notify, object = "ParticleAbstractShape3D")]
    pub shape: Option<ObjectId>,
    #[qml(notify)]
    pub duration: i32,
    #[qml(notify)]
    pub duration_variation: i32,
    #[qml(notify)]
    pub hide_at_end: bool,
    /// Kept: a particle's place in the shape is worked out whenever it is
    /// asked for, so there is nothing to cache.
    #[qml(notify)]
    pub use_cached_positions: bool,
    #[qml(notify)]
    pub positions_amount: i32,
}

impl Default for Attractor3D {
    fn default() -> Self {
        Attractor3D {
            affector: Affector3D::default(),
            position_variation: Vector3d::default(),
            shape: None,
            duration: -1,
            duration_variation: 0,
            hide_at_end: false,
            use_cached_positions: true,
            positions_amount: 0,
        }
    }
}

/// `Repeller3D`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Particles3D", name = "Repeller3D", extends = Affector3D)]
pub struct Repeller3D {
    #[qml(base)]
    pub affector: Affector3D,
    #[qml(notify)]
    pub radius: f64,
    #[qml(notify)]
    pub outer_radius: f64,
    #[qml(notify)]
    pub strength: f64,
}

impl Default for Repeller3D {
    fn default() -> Self {
        Repeller3D {
            affector: Affector3D::default(),
            radius: 0.0,
            outer_radius: 50.0,
            strength: 50.0,
        }
    }
}

fn scale_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "ScalingType",
        vec![
            ("Linear", 0),
            ("SewSaw", 1),
            ("SineWave", 2),
            ("AbsSineWave", 3),
            ("Step", 4),
            ("SmoothStep", 5),
        ],
    )]
}

/// `ScaleAffector3D`. `easingCurve` is kept and does nothing.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "ScaleAffector3D",
    extends = Affector3D,
    enums = scale_enums
)]
pub struct ScaleAffector3D {
    #[qml(base)]
    pub affector: Affector3D,
    #[qml(notify)]
    pub min_size: f64,
    #[qml(notify)]
    pub max_size: f64,
    #[qml(notify)]
    pub duration: i32,
    #[qml(notify, rename = "type", enumeration = "ScalingType")]
    pub scaling: i32,
    #[qml(notify)]
    pub easing_curve: Value,
}

impl Default for ScaleAffector3D {
    fn default() -> Self {
        ScaleAffector3D {
            affector: Affector3D::default(),
            min_size: 1.0,
            max_size: 1.0,
            duration: 1000,
            scaling: 0,
            easing_curve: Value::Undefined,
        }
    }
}

fn normal(v: Vector3d) -> Vec3 {
    vec3(v).normalized()
}

/// What the affector `obj` does now, seen from its system, whose world is
/// inverted in `from`. Tracked.
pub(super) fn read(e: &Engine, obj: ObjectId, from: Mat4) -> Option<Affector> {
    if !e.get(obj, Affector3D::ENABLED).unwrap_or(true) {
        return None;
    }
    let seen = || from * space::place(e, obj).world;
    if e.is::<Gravity3D>(obj) {
        return Some(Affector::Gravity {
            direction: normal(e.get(obj, Gravity3D::DIRECTION).unwrap_or_default()),
            magnitude: e.get(obj, Gravity3D::MAGNITUDE).unwrap_or(0.0) as f32,
        });
    }
    if e.is::<PointRotator3D>(obj) {
        return Some(Affector::Rotator {
            axis: normal(e.get(obj, PointRotator3D::DIRECTION).unwrap_or_default()),
            pivot: vec3(e.get(obj, PointRotator3D::PIVOT_POINT).unwrap_or_default()),
            magnitude: e.get(obj, PointRotator3D::MAGNITUDE).unwrap_or(0.0) as f32,
        });
    }
    if e.is::<Wander3D>(obj) {
        let v = |p| floats(e.get(obj, p).unwrap_or_default());
        return Some(Affector::Wander {
            amount: v(Wander3D::GLOBAL_AMOUNT),
            pace: v(Wander3D::GLOBAL_PACE),
            start: v(Wander3D::GLOBAL_PACE_START),
            own_amount: v(Wander3D::UNIQUE_AMOUNT),
            own_pace: v(Wander3D::UNIQUE_PACE),
            amount_variation: e.get(obj, Wander3D::UNIQUE_AMOUNT_VARIATION).unwrap_or(0.0) as f32,
            pace_variation: e.get(obj, Wander3D::UNIQUE_PACE_VARIATION).unwrap_or(0.0) as f32,
            fade_in: e.get(obj, Wander3D::FADE_IN_DURATION).unwrap_or(0) as f32,
            fade_out: e.get(obj, Wander3D::FADE_OUT_DURATION).unwrap_or(0) as f32,
        });
    }
    if e.is::<Attractor3D>(obj) {
        return Some(Affector::Attract {
            place: seen(),
            shape: shapes::read(e, e.get(obj, Attractor3D::SHAPE).ok().flatten()),
            variation: floats(
                e.get(obj, Attractor3D::POSITION_VARIATION)
                    .unwrap_or_default(),
            ),
            duration: e.get(obj, Attractor3D::DURATION).unwrap_or(-1) as f32,
            duration_variation: e.get(obj, Attractor3D::DURATION_VARIATION).unwrap_or(0) as f32,
            hide_at_end: e.get(obj, Attractor3D::HIDE_AT_END).unwrap_or(false),
        });
    }
    if e.is::<Repeller3D>(obj) {
        return Some(Affector::Repel {
            at: seen().translation(),
            radius: e.get(obj, Repeller3D::RADIUS).unwrap_or(0.0) as f32,
            outer: e.get(obj, Repeller3D::OUTER_RADIUS).unwrap_or(0.0) as f32,
            strength: e.get(obj, Repeller3D::STRENGTH).unwrap_or(0.0) as f32,
        });
    }
    if e.is::<ScaleAffector3D>(obj) {
        return Some(Affector::Scale {
            min: e.get(obj, ScaleAffector3D::MIN_SIZE).unwrap_or(1.0) as f32,
            max: e.get(obj, ScaleAffector3D::MAX_SIZE).unwrap_or(1.0) as f32,
            duration: e.get(obj, ScaleAffector3D::DURATION).unwrap_or(0) as f32,
            curve: match e.get(obj, ScaleAffector3D::SCALING).unwrap_or(0) {
                1 => ScaleCurve::SewSaw,
                2 => ScaleCurve::SineWave,
                3 => ScaleCurve::AbsSineWave,
                4 => ScaleCurve::Step,
                5 => ScaleCurve::SmoothStep,
                _ => ScaleCurve::Linear,
            },
        });
    }
    e.with(obj, |a: &Affector3D| a.custom.clone())
        .ok()
        .flatten()
        .map(Affector::Custom)
}

/// Whether the affector `obj` is for the kind `kind`. Tracked.
pub(super) fn affects(e: &Engine, obj: ObjectId, kind: ObjectId) -> bool {
    match e.read(obj, Affector3D::PARTICLES.index) {
        Value::List(named) if !named.is_empty() => {
            named.iter().any(|v| v.as_object() == Some(kind))
        }
        _ => true,
    }
}
