//! What an animation does over time, worked out when it starts.
//!
//! A [`Timed`] is one animation object: when it begins inside its parent,
//! how long one pass takes, how often it repeats, and its [`Node`] (a
//! property moving, a pause, a script, a group). Durations and offsets are
//! fixed when the timeline is built, which is why a change to `to` or
//! `duration` does not reach a run that is under way.
//!
//! [`Timed::advance`] takes the time since the animation began, not a step.
//! That makes the result independent of how many frames there were in
//! between: an animation nobody watched for a second is where a watched
//! one would be, with every script run and every property action applied
//! in order.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{
    Atom, Color, Engine, LogLevel, NumberSlot, ObjectId, Point, PropIndex, Rect, Size, TypeTag,
    Value,
};
use ljos_reactive::WriteKind;

use super::easing::Curve;
use super::motion;
use super::shown;
use super::smooth::{Params, Reversing, Track};
use super::spring;
use super::types::{
    AnchorAnimation, Animation, Animator, ColorAnimation, EasingGroup, NumberAnimation,
    OpacityAnimator, ParallelAnimation, ParentAnimation, PauseAnimation, PropertyAction,
    PropertyAnimation, QuaternionAnimation, RotationAnimation, RotationAnimator, ScaleAnimator,
    ScriptAction, SequentialAnimation, SmoothedAnimation, SpringAnimation, UniformAnimator,
    XAnimator, YAnimator,
};
use crate::item::{Item, is_a, is_item, is_unset};

/// "Never": a duration or a loop count without end. Far from `i64::MAX`, so
/// sums of a few of them do not overflow.
pub(crate) const FOREVER: i64 = i64::MAX / 16;

/// What a `Behavior` (or a property the animation is declared `on`) gives
/// the animations that name no target of their own.
#[derive(Clone, Debug)]
pub(crate) struct Defaults {
    pub obj: ObjectId,
    pub prop: PropIndex,
    /// The value a Behavior animates to, for animations without `to`.
    pub to: Option<Value>,
    /// What [`moves`] found the animation it was asked of moves: the
    /// Behavior asks at every value it hears, and the start that follows
    /// would look it all up again.
    pub aimed: Option<Rc<Aimed>>,
}

/// The slots of one animation, as [`slots`] gives them.
#[derive(Debug)]
pub(crate) struct Aimed {
    anim: ObjectId,
    slots: Vec<Slot>,
}

/// [`Defaults::prop`] of a run that has an object to move and no property
/// of its own: the animations of a `Transition` move what they name.
pub(crate) const NO_PROPERTY: PropIndex = PropIndex(u32::MAX);

/// One property a change of state gave another value.
#[derive(Clone, Debug)]
pub(crate) struct Change {
    pub obj: ObjectId,
    pub prop: PropIndex,
    /// The value it has once the transition is over.
    pub to: Value,
    pub kind: ChangeKind,
}

/// Which animations of a transition a [`Change`] is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChangeKind {
    /// A property the state names: any animation of properties.
    Property,
    /// Where other anchors put an item: an `AnchorAnimation`.
    Anchor,
    /// Where an item is in the parent the state gave it: what is inside a
    /// `ParentAnimation`.
    Parent,
}

/// What a change of state did, for the transition that shows it. Its
/// animations move what changed, to the values the state gives, unless
/// they say otherwise.
#[derive(Debug, Default)]
pub(crate) struct Changes {
    pub list: Vec<Change>,
    /// The transition runs for the opposite change: what came last comes
    /// first, and an animation's own `from` and `to` change places.
    pub reversed: bool,
    /// The named scripts of the state entered, each with the property its
    /// statement is kept under: a `ScriptAction` runs one by `scriptName`.
    pub scripts: Vec<(Atom, ObjectId, PropIndex)>,
    /// The value each `PropertyAction` was given, read before the state's
    /// changes were made ([`action_values`]); one not here reads its own.
    pub values: Vec<(ObjectId, Value)>,
    /// What the state gives the value it already had: no animation moves
    /// it, but a `PropertyAction` without a value of its own still sets
    /// it (qml6).
    pub kept: Vec<Change>,
}

/// What an animation is built in: the property of a `Behavior`, or the
/// changes of a transition.
#[derive(Clone, Copy)]
struct Env<'a> {
    defaults: Option<&'a Defaults>,
    changes: Option<&'a Changes>,
    /// Inside a `ParentAnimation`.
    reparented: bool,
}

/// What an advancing timeline needs from its run.
pub(crate) struct Ctx<'a> {
    pub engine: &'a Engine,
    /// Set when the run was stopped from inside (a script that calls
    /// `stop()`): nothing more of it may happen.
    pub cancelled: &'a Cell<bool>,
    /// False while an `AnimationController` seeks: showing a moment of an
    /// animation does not run its scripts.
    pub scripts: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    Active,
    Done,
}

#[derive(Debug)]
pub(crate) struct Timed {
    node: Node,
    /// Where it begins in its parent's pass, in microseconds.
    start: i64,
    /// One pass, as it is timed now.
    pass: i64,
    /// One pass as it was built: what [`Timed::retime`] times from.
    base: i64,
    /// Timed for reduced motion.
    calm: bool,
    /// [`FOREVER`] for `Animation.Infinite`.
    loops: i64,
    /// The pass it is in.
    iter: i64,
    state: State,
}

#[derive(Debug)]
enum Node {
    Prop(PropNode),
    Smooth(SmoothNode),
    Spring(SpringNode),
    Pause,
    /// The statement kept under `prop` of `action`.
    Script {
        action: ObjectId,
        prop: PropIndex,
        fired: bool,
    },
    Action {
        node: ActionNode,
        fired: bool,
    },
    Seq(Vec<Timed>),
    Par(Vec<Timed>),
}

/// One property of one object.
#[derive(Clone, Debug)]
struct Slot {
    obj: ObjectId,
    prop: PropIndex,
    ty: TypeTag,
    /// An item's property shows only while the item does.
    item: bool,
    /// Where a number goes, once the property has a node to take it.
    numbers: Cell<Option<NumberSlot>>,
}

/// Which values an animation type moves between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Number,
    Color,
    /// An angle in degrees, turning the way [`Turn`] says.
    Rotation(Turn),
    /// `QuaternionAnimation`: a turn along the shortest arc, at an even
    /// speed, or along the straight line between the ends, normalized.
    Quaternion {
        nlerp: bool,
    },
    /// `PropertyAnimation`: whatever the property holds.
    Any,
}

/// `RotationAnimation.direction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Turn {
    Numerical,
    Shortest,
    Clockwise,
    Counterclockwise,
}

impl Turn {
    fn of(direction: f64) -> Turn {
        match direction as i32 {
            1 => Turn::Shortest,
            2 => Turn::Clockwise,
            3 => Turn::Counterclockwise,
            _ => Turn::Numerical,
        }
    }

    /// How far to turn to get from `from` to `to`, in degrees. Measured
    /// on Qt: clockwise never counts down and counterclockwise never
    /// counts up, but neither takes whole turns off a way that already
    /// goes in its direction; the shortest way is never more than half a
    /// turn.
    fn way(self, from: f64, to: f64) -> f64 {
        let diff = to - from;
        if !diff.is_finite() {
            return diff;
        }
        match self {
            Turn::Numerical => diff,
            Turn::Clockwise if diff < 0.0 => diff.rem_euclid(360.0),
            Turn::Counterclockwise if diff > 0.0 => match diff.rem_euclid(360.0) {
                0.0 => 0.0,
                rest => rest - 360.0,
            },
            Turn::Clockwise | Turn::Counterclockwise => diff,
            Turn::Shortest => {
                let d = diff % 360.0;
                if d > 180.0 {
                    d - 360.0
                } else if d < -180.0 {
                    d + 360.0
                } else {
                    d
                }
            }
        }
    }
}

#[derive(Debug)]
struct PropNode {
    kind: Kind,
    slots: Vec<Slot>,
    from: Option<Value>,
    to: Option<Value>,
    /// Per slot, the value a state gives it: where it goes without a `to`
    /// of its own. Empty outside a transition.
    tos: Vec<Option<Value>>,
    curve: Curve,
    /// Per slot, from the first moment of the node. `None`: not begun.
    lerps: Option<Vec<Lerp>>,
    /// An Animator's: per slot, the value shown and not yet written. The
    /// property is written once, when the node ends or the run does.
    held: Option<Vec<Option<f64>>>,
    /// Per slot, whether it shows its end value from the first moment:
    /// what reduced motion does to a property that moves. `None` while
    /// motion is not reduced.
    jumps: Option<Vec<bool>>,
    /// Motion is no longer reduced, but this pass keeps `jumps`: what is
    /// at its end does not go back.
    pinned: bool,
}

/// A `SmoothedAnimation`: each property on a way of its own.
#[derive(Debug)]
struct SmoothNode {
    anim: ObjectId,
    slots: Vec<Slot>,
    params: Params,
    /// Per slot; `None` where there is nothing to do. Planned when the
    /// timeline is built, because the distance gives the duration.
    tracks: Vec<Option<Track>>,
    begun: bool,
}

/// A `SpringAnimation`: each property on a way of its own.
#[derive(Debug)]
struct SpringNode {
    anim: ObjectId,
    slots: Vec<Slot>,
    params: spring::Params,
    /// Per slot; `None` where there is nothing to do. Planned when the
    /// timeline is built, because the way gives the duration.
    tracks: Vec<Option<spring::Track>>,
    /// The animation says where the way begins (`from`), so it does not
    /// begin where the property happens to be.
    own_from: bool,
    begun: bool,
}

#[derive(Debug)]
struct ActionNode {
    slots: Vec<Slot>,
    /// Per slot; `None` where there is nothing to set.
    values: Vec<Option<Value>>,
    /// In a transition, the action was given a value of its own: the
    /// state's change of what it sets ends there ([`Timed::given`]).
    own: bool,
}

/// The way from one value to another.
#[derive(Clone, Debug)]
enum Lerp {
    /// Nothing to do: no `to`, or values that cannot be told apart by
    /// degrees.
    None,
    Number {
        from: f64,
        to: f64,
        whole: bool,
    },
    /// An angle that goes `way` degrees from `from` and then shows `to`,
    /// which need not be where the way ends (350 clockwise to 10 passes
    /// 360).
    Angle {
        from: f64,
        way: f64,
        to: f64,
        whole: bool,
    },
    /// Channels in the 0 to 255 range.
    Color {
        from: [f64; 4],
        to: [f64; 4],
    },
    Point {
        from: Point,
        to: Point,
    },
    Size {
        from: Size,
        to: Size,
    },
    Rect {
        from: Rect,
        to: Rect,
    },
    /// A `vector2d`, `vector3d` or `vector4d`, component by component;
    /// `make` builds the value back from the four.
    Vector {
        from: [f32; 4],
        to: [f32; 4],
        make: fn([f32; 4]) -> Value,
    },
    /// A `quaternion`, scalar first, `to` on the near side of `from`.
    Quaternion {
        from: [f32; 4],
        to: [f32; 4],
        nlerp: bool,
    },
    /// Values without anything in between (a string, a bool): the end
    /// value, from the first moment.
    Jump {
        to: Value,
    },
}

impl Lerp {
    fn new(kind: Kind, ty: &TypeTag, from: &Value, to: Option<&Value>) -> Lerp {
        let Some(to) = to else {
            return Lerp::None;
        };
        let color = |v: &Value| match TypeTag::Color.coerce(v) {
            Ok(Value::Color(c)) => Some(c.to_rgba8().map(f64::from)),
            _ => None,
        };
        let number = |v: &Value| {
            let n = v.to_number();
            (!n.is_nan()).then_some(n)
        };
        let colors = || match (color(from), color(to)) {
            (Some(from), Some(to)) => Lerp::Color { from, to },
            _ => Lerp::None,
        };
        let numbers = || match (number(from), number(to)) {
            (Some(from), Some(to)) => Lerp::Number {
                from,
                to,
                whole: matches!(ty, TypeTag::Int),
            },
            _ => Lerp::None,
        };
        match (kind, ty) {
            (Kind::Rotation(turn), _) => match (number(from), number(to)) {
                (Some(from), Some(to)) => Lerp::Angle {
                    from,
                    way: turn.way(from, to),
                    to,
                    whole: matches!(ty, TypeTag::Int),
                },
                _ => Lerp::None,
            },
            (Kind::Quaternion { nlerp }, _) => quaternions(from, to, nlerp),
            (Kind::Color, _) | (Kind::Any, TypeTag::Color) => colors(),
            (Kind::Number, _) | (Kind::Any, TypeTag::Int | TypeTag::Real) => numbers(),
            (Kind::Any, _) => match (from, to) {
                (Value::Point(a), Value::Point(b)) => Lerp::Point { from: **a, to: **b },
                (Value::Size(a), Value::Size(b)) => Lerp::Size { from: **a, to: **b },
                (Value::Rect(a), Value::Rect(b)) => Lerp::Rect { from: **a, to: **b },
                (Value::Quaternion(_), Value::Quaternion(_)) => quaternions(from, to, false),
                (Value::Vector2d(_) | Value::Vector3d(_) | Value::Vector4d(_), _) => {
                    match (vector(from), vector(to)) {
                        (Some((from, make)), Some((to, _))) => Lerp::Vector { from, to, make },
                        _ => Lerp::Jump { to: to.clone() },
                    }
                }
                (Value::Color(_), _) | (_, Value::Color(_)) => colors(),
                (Value::Int(_) | Value::Real(_), Value::Int(_) | Value::Real(_)) => numbers(),
                _ => Lerp::Jump { to: to.clone() },
            },
        }
    }

    /// The value at eased progress `p`.
    fn at(&self, p: f64) -> Option<Value> {
        let mix = |a: f64, b: f64| a + (b - a) * p;
        let number = |v: f64, whole: bool| {
            if whole {
                // Half goes away from zero, as Qt rounds.
                Value::Int(v.round().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32)
            } else {
                Value::Real(v)
            }
        };
        Some(match self {
            Lerp::None => return None,
            Lerp::Number { from, to, whole } => number(mix(*from, *to), *whole),
            Lerp::Angle {
                from,
                way,
                to,
                whole,
            } => {
                // Wherever the curve says "there", the value is `to`.
                let v = if p == 1.0 { *to } else { from + way * p };
                number(v, *whole)
            }
            Lerp::Color { from, to } => {
                // Qt cuts each channel to a whole number.
                let c = |i: usize| mix(from[i], to[i]).floor().clamp(0.0, 255.0) as u8;
                Value::Color(Color::from_rgba8(c(0), c(1), c(2), c(3)))
            }
            Lerp::Point { from, to } => Value::Point(std::sync::Arc::new(Point {
                x: mix(from.x, to.x),
                y: mix(from.y, to.y),
            })),
            Lerp::Size { from, to } => Value::Size(std::sync::Arc::new(Size {
                width: mix(from.width, to.width),
                height: mix(from.height, to.height),
            })),
            Lerp::Rect { from, to } => Value::Rect(std::sync::Arc::new(Rect {
                x: mix(from.x, to.x),
                y: mix(from.y, to.y),
                width: mix(from.width, to.width),
                height: mix(from.height, to.height),
            })),
            Lerp::Vector { from, to, make } => {
                let p = p as f32;
                make(std::array::from_fn(|i| from[i] + (to[i] - from[i]) * p))
            }
            Lerp::Quaternion { from, to, nlerp } => {
                quaternion_value(turn_between(*from, *to, p as f32, *nlerp))
            }
            Lerp::Jump { to } => to.clone(),
        })
    }
}

/// A vector's components, padded to four, and how to make the vector
/// again from four.
fn vector(v: &Value) -> Option<([f32; 4], fn([f32; 4]) -> Value)> {
    use ljos_engine::{Vector2d, Vector3d, Vector4d};
    Some(match v {
        Value::Vector2d(v) => ([v.x, v.y, 0.0, 0.0], |[x, y, ..]| {
            Value::Vector2d(Vector2d { x, y })
        }),
        Value::Vector3d(v) => ([v.x, v.y, v.z, 0.0], |[x, y, z, _]| {
            Value::Vector3d(Vector3d { x, y, z })
        }),
        Value::Vector4d(v) => ([v.x, v.y, v.z, v.w], |[x, y, z, w]| {
            Value::Vector4d(std::sync::Arc::new(Vector4d { x, y, z, w }))
        }),
        _ => return None,
    })
}

fn quaternion_parts(v: &Value) -> Option<[f32; 4]> {
    match TypeTag::Quaternion.coerce(v) {
        Ok(Value::Quaternion(q)) => Some([q.scalar, q.x, q.y, q.z]),
        _ => None,
    }
}

fn quaternion_value([scalar, x, y, z]: [f32; 4]) -> Value {
    Value::Quaternion(std::sync::Arc::new(ljos_engine::Quaternion {
        scalar,
        x,
        y,
        z,
    }))
}

fn quaternions(from: &Value, to: &Value, nlerp: bool) -> Lerp {
    let (Some(from), Some(mut to)) = (quaternion_parts(from), quaternion_parts(to)) else {
        return Lerp::None;
    };
    // q and -q are the same turn: the one nearer `from` is the short way.
    if dot(from, to) < 0.0 {
        to = to.map(|c| -c);
    }
    Lerp::Quaternion { from, to, nlerp }
}

fn dot(a: [f32; 4], b: [f32; 4]) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

fn normalized(q: [f32; 4]) -> [f32; 4] {
    let n = dot(q, q).sqrt();
    if n > 0.0 { q.map(|c| c / n) } else { q }
}

/// The turn a fraction `p` of the way from `a` to `b`, which are on the
/// same side.
fn turn_between(a: [f32; 4], b: [f32; 4], p: f32, nlerp: bool) -> [f32; 4] {
    let (a, b) = (normalized(a), normalized(b));
    let cos = dot(a, b).min(1.0);
    // Nearly the same turn: the arc is the line, and its sine is too
    // small to divide by.
    let (wa, wb) = if nlerp || cos > 0.9995 {
        (1.0 - p, p)
    } else {
        let angle = cos.acos();
        let sin = angle.sin();
        (((1.0 - p) * angle).sin() / sin, (p * angle).sin() / sin)
    };
    normalized(std::array::from_fn(|i| a[i] * wa + b[i] * wb))
}

/// The turn of `degrees` about X, Y and Z, as `eulerRotation` gives it:
/// Z first, then X, then Y.
fn euler_turn([x, y, z]: [f64; 3]) -> [f32; 4] {
    let about = |axis: usize, degrees: f64| {
        let half = (degrees.to_radians() / 2.0) as f32;
        let mut q = [half.cos(), 0.0, 0.0, 0.0];
        q[axis + 1] = half.sin();
        q
    };
    let times = |[aw, ax, ay, az]: [f32; 4], [bw, bx, by, bz]: [f32; 4]| {
        [
            aw * bw - ax * bx - ay * by - az * bz,
            aw * bx + ax * bw + ay * bz - az * by,
            aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw,
        ]
    };
    times(times(about(1, y), about(0, x)), about(2, z))
}

impl Slot {
    fn of(engine: &Engine, obj: ObjectId, prop: PropIndex) -> Slot {
        let ty = engine
            .meta_of(obj)
            .and_then(|m| m.property(prop).map(|p| p.ty.clone()))
            .unwrap_or(TypeTag::Var);
        Slot {
            obj,
            prop,
            ty,
            item: is_item(engine, obj),
            numbers: Cell::new(None),
        }
    }

    /// Whether reduced motion takes it to its end at once.
    fn jumps(&self, engine: &Engine) -> bool {
        engine
            .meta_of(self.obj)
            .and_then(|m| {
                m.property(self.prop)
                    .map(|p| motion::jumps(&p.name, &self.ty))
            })
            .unwrap_or(true)
    }

    fn shown(&self, engine: &Engine) -> bool {
        !self.item
            || engine
                .untrack(|| engine.read(self.obj, Item::VISIBLE.index))
                .to_boolean()
    }

    fn write(&self, engine: &Engine, v: Value) {
        let v = match self.numbers.get() {
            Some(slot) => match engine.write_number(&slot, v) {
                Ok(()) => return,
                Err(v) => {
                    self.numbers.set(None);
                    v
                }
            },
            None => v,
        };
        // The target may be gone, or refuse the value; either way the
        // animation goes on.
        let _ = engine.write_internal(self.obj, self.prop, v, WriteKind::Animation);
        if matches!(self.ty, TypeTag::Real | TypeTag::Int) {
            self.numbers.set(engine.number_slot(self.obj, self.prop));
        }
    }
}

pub(crate) fn warn(engine: &Engine, anim: ObjectId, what: std::fmt::Arguments<'_>) {
    let ty = engine
        .meta_of(anim)
        .map(|m| m.name.to_string())
        .unwrap_or_default();
    engine.log(LogLevel::Warning, "qml", format!("QML {ty}: {what}"));
}

/// The objects in a list property.
fn objects(engine: &Engine, obj: ObjectId, name: &str) -> Vec<ObjectId> {
    objects_of(engine, engine.read_by_name(obj, name))
}

fn objects_of(engine: &Engine, list: Value) -> Vec<ObjectId> {
    match list {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| engine.is_alive(*o))
            .collect(),
        _ => Vec::new(),
    }
}

/// What `anim` holds in `p`, or nothing when nothing ever gave it a value
/// and it holds its type's default. Asking whether it was given one costs
/// less than the read, and the animation a Behavior starts at every write
/// sets few of the properties a start goes through.
fn given(engine: &Engine, anim: ObjectId, p: PropIndex) -> Option<Value> {
    (!is_unset(engine, anim, p)).then(|| engine.read(anim, p))
}

/// `from` and `to` of a PropertyAnimation, `to` from `defaults` if it
/// has none of its own.
fn property_ends(
    engine: &Engine,
    anim: ObjectId,
    defaults: Option<&Defaults>,
) -> (Option<Value>, Option<Value>) {
    let at = |p: PropIndex| match given(engine, anim, p)? {
        Value::Undefined => None,
        v => Some(v),
    };
    let (mut from, mut to) = (
        at(PropertyAnimation::FROM.index),
        at(PropertyAnimation::TO.index),
    );
    if is_a::<QuaternionAnimation>(engine, anim) {
        let euler = |props: [PropIndex; 3]| {
            let parts = props.map(|p| given(engine, anim, p).map(|v| v.to_number()));
            // Turns about the axes not given are none.
            parts
                .iter()
                .any(Option::is_some)
                .then(|| quaternion_value(euler_turn(parts.map(|p| p.unwrap_or(0.0)))))
        };
        from = from.or_else(|| {
            euler([
                QuaternionAnimation::FROM_X_ROTATION.index,
                QuaternionAnimation::FROM_Y_ROTATION.index,
                QuaternionAnimation::FROM_Z_ROTATION.index,
            ])
        });
        to = to.or_else(|| {
            euler([
                QuaternionAnimation::TO_X_ROTATION.index,
                QuaternionAnimation::TO_Y_ROTATION.index,
                QuaternionAnimation::TO_Z_ROTATION.index,
            ])
        });
    }
    (from, to.or_else(|| defaults.and_then(|d| d.to.clone())))
}

/// What a PropertyAnimation moves between.
fn property_kind(engine: &Engine, anim: ObjectId) -> Kind {
    if is_a::<RotationAnimation>(engine, anim) {
        Kind::Rotation(Turn::of(engine.read_by_name(anim, "direction").to_number()))
    } else if is_a::<NumberAnimation>(engine, anim) {
        Kind::Number
    } else if is_a::<ColorAnimation>(engine, anim) {
        Kind::Color
    } else if is_a::<QuaternionAnimation>(engine, anim) {
        let nlerp = given(engine, anim, QuaternionAnimation::KIND.index)
            .is_some_and(|v| v.to_number() == 1.0);
        Kind::Quaternion { nlerp }
    } else {
        Kind::Any
    }
}

/// `loops`, [`FOREVER`] for no end.
fn loops_of(engine: &Engine, anim: ObjectId) -> i64 {
    let loops = given(engine, anim, Animation::LOOPS.index).map_or(1.0, |v| v.to_number());
    if loops < 0.0 { FOREVER } else { loops as i64 }
}

fn duration_of(engine: &Engine, anim: ObjectId) -> i64 {
    let ms = if is_a::<PropertyAnimation>(engine, anim) {
        engine.read(anim, PropertyAnimation::DURATION.index)
    } else {
        engine.read_by_name(anim, "duration")
    }
    .to_number();
    if ms.is_finite() && ms > 0.0 {
        (ms * 1000.0) as i64
    } else {
        0
    }
}

/// The properties `target`, `targets`, `property` and `properties` name,
/// with the defaults filling in for what is not named.
fn slots(engine: &Engine, anim: ObjectId, defaults: Option<&Defaults>, quiet: bool) -> Vec<Slot> {
    if let Some(a) = defaults.and_then(|d| d.aimed.as_ref())
        && a.anim == anim
    {
        return a.slots.clone();
    }
    slots_of(engine, anim, defaults, quiet).0
}

/// [`slots`], and whether every property named was there: one that is not
/// is warned of unless `quiet`.
fn slots_of(
    engine: &Engine,
    anim: ObjectId,
    defaults: Option<&Defaults>,
    quiet: bool,
) -> (Vec<Slot>, bool) {
    let (mut targets, mut names) = named(engine, anim);
    let slot = |obj: ObjectId, prop: PropIndex| Slot::of(engine, obj, prop);
    if let Some(d) = defaults {
        if targets.is_empty() && names.is_empty() && d.prop != NO_PROPERTY {
            return (vec![slot(d.obj, d.prop)], true);
        }
        if targets.is_empty() {
            targets.push(d.obj);
        }
        if names.is_empty()
            && let Some(name) = engine
                .meta_of(d.obj)
                .and_then(|m| m.property(d.prop).map(|p| p.name.to_string()))
        {
            names.push(name);
        }
    }
    // A RotationAnimation that names no property turns these, where the
    // target has them.
    let assumed = names.is_empty() && is_a::<RotationAnimation>(engine, anim);
    if assumed {
        names.extend(["rotation".to_owned(), "angle".to_owned()]);
    }
    let mut out = Vec::new();
    let mut all = true;
    for t in &targets {
        for n in &names {
            match engine.property_index(*t, n) {
                Ok(p) => out.push(slot(*t, p)),
                Err(_) if assumed => {}
                Err(_) if quiet => all = false,
                Err(_) => warn(
                    engine,
                    anim,
                    format_args!("Cannot animate non-existent property \"{n}\""),
                ),
            }
        }
    }
    (out, all)
}

/// The objects and the property names an animation names itself.
fn named(engine: &Engine, anim: ObjectId) -> (Vec<ObjectId>, Vec<String>) {
    if is_a::<Animator>(engine, anim) {
        let target = engine.read(anim, Animator::TARGET.index).as_object();
        return (target.into_iter().collect(), animated(engine, anim));
    }
    // Asked of every Behavior's animation at every write it hears: a
    // PropertyAnimation's own are read by index, with no name to look up.
    let typed = is_a::<PropertyAnimation>(engine, anim);
    let get = |p: PropIndex, name: &str| {
        if typed {
            given(engine, anim, p)
        } else {
            Some(engine.read_by_name(anim, name))
        }
    };
    let mut targets = get(PropertyAnimation::TARGETS.index, "targets")
        .map_or_else(Vec::new, |v| objects_of(engine, v));
    if let Some(t) = get(PropertyAnimation::TARGET.index, "target").and_then(|v| v.as_object())
        && !targets.contains(&t)
    {
        targets.insert(0, t);
    }
    let mut names: Vec<String> = Vec::new();
    let text =
        |p: PropIndex, name: &str| get(p, name).map(|v| v.to_js_string()).unwrap_or_default();
    let one = text(PropertyAnimation::PROPERTY.index, "property");
    let many = text(PropertyAnimation::PROPERTIES.index, "properties");
    for n in std::iter::once(&*one).chain(many.split(',')) {
        let n = n.trim();
        if !n.is_empty() && !names.iter().any(|x| x == n) {
            names.push(n.to_owned());
        }
    }
    (targets, names)
}

/// The property an Animator moves, which its type says.
fn animated(engine: &Engine, anim: ObjectId) -> Vec<String> {
    let name = if is_a::<OpacityAnimator>(engine, anim) {
        "opacity"
    } else if is_a::<XAnimator>(engine, anim) {
        "x"
    } else if is_a::<YAnimator>(engine, anim) {
        "y"
    } else if is_a::<ScaleAnimator>(engine, anim) {
        "scale"
    } else if is_a::<RotationAnimator>(engine, anim) {
        "rotation"
    } else if is_a::<UniformAnimator>(engine, anim) {
        return match engine
            .read(anim, UniformAnimator::UNIFORM.index)
            .to_js_string()
        {
            n if n.is_empty() => Vec::new(),
            n => vec![n.to_string()],
        };
    } else {
        return Vec::new();
    };
    vec![name.to_owned()]
}

/// What an animation of properties moves in a transition, and per slot
/// the value the state gives it. Of what changed it takes what it names:
/// every changed object when it names none, and without a property name
/// every changed property of the kind it animates. A property it names
/// outright that no state changed is its own to move, with its own `to`.
fn of_changes(
    engine: &Engine,
    anim: ObjectId,
    list: &[Change],
    reparented: bool,
    kind: Kind,
) -> (Vec<Slot>, Vec<Option<Value>>) {
    let (targets, mut names) = named(engine, anim);
    let outright = !names.is_empty();
    if !outright && matches!(kind, Kind::Rotation(_)) {
        names.extend(["rotation".to_owned(), "angle".to_owned()]);
    }
    let mut slots: Vec<Slot> = Vec::new();
    let mut tos = Vec::new();
    for c in list {
        let mine = match c.kind {
            ChangeKind::Property => true,
            ChangeKind::Parent => reparented,
            ChangeKind::Anchor => false,
        };
        if !mine || !(targets.is_empty() || targets.contains(&c.obj)) {
            continue;
        }
        let slot = Slot::of(engine, c.obj, c.prop);
        let wanted = if names.is_empty() {
            match kind {
                Kind::Number => matches!(slot.ty, TypeTag::Int | TypeTag::Real),
                Kind::Color => matches!(slot.ty, TypeTag::Color),
                Kind::Quaternion { .. } => matches!(slot.ty, TypeTag::Quaternion),
                Kind::Rotation(_) | Kind::Any => true,
            }
        } else {
            names
                .iter()
                .any(|n| engine.property_index(c.obj, n).ok() == Some(c.prop))
        };
        if wanted && !slots.iter().any(|s| s.obj == c.obj && s.prop == c.prop) {
            slots.push(slot);
            tos.push(Some(c.to.clone()));
        }
    }
    if outright {
        for t in &targets {
            for n in &names {
                if let Ok(p) = engine.property_index(*t, n)
                    && !slots.iter().any(|s| s.obj == *t && s.prop == p)
                {
                    slots.push(Slot::of(engine, *t, p));
                    tos.push(None);
                }
            }
        }
    }
    (slots, tos)
}

/// What an `AnchorAnimation` moves: the place and size of the items a
/// state anchored otherwise, of those it names or of all of them.
fn of_anchors(
    engine: &Engine,
    anim: ObjectId,
    changes: &Changes,
) -> (Vec<Slot>, Vec<Option<Value>>) {
    let targets = objects(engine, anim, "targets");
    changes
        .list
        .iter()
        .filter(|c| c.kind == ChangeKind::Anchor)
        .filter(|c| targets.is_empty() || targets.contains(&c.obj))
        .map(|c| (Slot::of(engine, c.obj, c.prop), Some(c.to.clone())))
        .unzip()
}

/// The names of the state scripts the `ScriptAction`s in `anims` run in
/// their own time (`scriptName`).
pub(crate) fn script_names(engine: &Engine, anims: &[ObjectId]) -> Vec<Atom> {
    let mut out = Vec::new();
    for anim in anims {
        if is_a::<ScriptAction>(engine, *anim) {
            let name = engine.read_by_name(*anim, "scriptName").to_js_string();
            if !name.is_empty() {
                out.push(Atom::from(&*name));
            }
        } else if is_a::<SequentialAnimation>(engine, *anim)
            || is_a::<ParallelAnimation>(engine, *anim)
        {
            out.extend(script_names(engine, &objects(engine, *anim, "animations")));
        }
    }
    out
}

/// The value of each `PropertyAction` in `anims` that was given one, as it
/// is now: a transition's actions read theirs before the state's changes
/// are made (qml6), so one that reads what the state changes sets the old
/// value.
pub(crate) fn action_values(engine: &Engine, anims: &[ObjectId]) -> Vec<(ObjectId, Value)> {
    let mut out = Vec::new();
    for anim in anims {
        if is_a::<PropertyAction>(engine, *anim) {
            if is_unset(engine, *anim, PropertyAction::VALUE.index) {
                continue;
            }
            out.push((
                *anim,
                engine.untrack(|| engine.read_by_name(*anim, "value")),
            ));
        } else if is_a::<SequentialAnimation>(engine, *anim)
            || is_a::<ParallelAnimation>(engine, *anim)
        {
            out.extend(action_values(engine, &objects(engine, *anim, "animations")));
        }
    }
    out
}

/// Whether `anim`, run with these defaults, takes the defaults' property
/// somewhere: one of its parts animates or sets it. A `Behavior` whose
/// animation does not lets the value through.
///
/// What a single animation of properties moves is kept in `d`, for its
/// start to take, unless a name it gives is missing: the start warns of
/// that.
pub(crate) fn moves(engine: &Engine, anim: ObjectId, d: &mut Defaults) -> bool {
    if is_a::<SequentialAnimation>(engine, anim) || is_a::<ParallelAnimation>(engine, anim) {
        let found = |c| {
            moves(
                engine,
                c,
                &mut Defaults {
                    aimed: None,
                    ..d.clone()
                },
            )
        };
        objects(engine, anim, "animations").into_iter().any(found)
    } else if is_a::<PropertyAnimation>(engine, anim)
        || is_a::<PropertyAction>(engine, anim)
        || is_a::<Animator>(engine, anim)
    {
        d.aimed = None;
        let (slots, all) = slots_of(engine, anim, Some(d), true);
        let found = slots.iter().any(|s| s.obj == d.obj && s.prop == d.prop);
        if all {
            d.aimed = Some(Rc::new(Aimed { anim, slots }));
        }
        found
    } else {
        false
    }
}

/// What the animations of a transition run on `target` do, as reduced
/// motion sees them: whether one of them moves something
/// ([`motion::jumps`]), and whether one fades `target` itself.
pub(crate) fn passage_of(engine: &Engine, anims: &[ObjectId], target: ObjectId) -> (bool, bool) {
    let defaults = Defaults {
        obj: target,
        prop: NO_PROPERTY,
        to: None,
        aimed: None,
    };
    let (mut moves, mut fades) = (false, false);
    for anim in anims {
        if is_a::<SequentialAnimation>(engine, *anim) || is_a::<ParallelAnimation>(engine, *anim) {
            let (m, f) = passage_of(engine, &objects(engine, *anim, "animations"), target);
            moves |= m;
            fades |= f;
        } else if is_a::<PropertyAnimation>(engine, *anim) || is_a::<Animator>(engine, *anim) {
            for slot in slots_of(engine, *anim, Some(&defaults), true).0 {
                if slot.jumps(engine) {
                    moves = true;
                } else if slot.obj == target
                    && engine.property_index(target, "opacity").ok() == Some(slot.prop)
                {
                    fades = true;
                }
            }
        } else if is_a::<AnchorAnimation>(engine, *anim) {
            moves = true;
        }
    }
    (moves, fades)
}

fn curve_of(engine: &Engine, anim: ObjectId) -> Curve {
    // Read at every start: the types that have the group are read by
    // index, with no names to look up.
    let group = if is_a::<PropertyAnimation>(engine, anim) {
        Some(PropertyAnimation::EASING)
    } else if is_a::<Animator>(engine, anim) {
        Some(Animator::EASING)
    } else if is_a::<AnchorAnimation>(engine, anim) {
        Some(AnchorAnimation::EASING)
    } else {
        None
    };
    let read = |leaf: PropIndex, name: &str| match group {
        Some(g) => given(engine, anim, PropIndex(g.index.0 + 1 + leaf.0)),
        None => Some(engine.read_by_name(anim, name)),
    };
    let default = Curve::default();
    let num = |leaf: PropIndex, name: &str, or: f64| read(leaf, name).map_or(or, |v| v.to_number());
    let ty = num(EasingGroup::TY.index, "easing.type", default.ty.into()) as i32;
    // Only a spline goes by its points, and the list is made anew at
    // each read.
    let bezier = match ty {
        super::easing::BEZIER_SPLINE => {
            match read(EasingGroup::BEZIER_CURVE.index, "easing.bezierCurve") {
                Some(Value::List(l)) => l.iter().map(Value::to_number).collect(),
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    };
    Curve {
        ty,
        amplitude: num(
            EasingGroup::AMPLITUDE.index,
            "easing.amplitude",
            default.amplitude,
        ),
        overshoot: num(
            EasingGroup::OVERSHOOT.index,
            "easing.overshoot",
            default.overshoot,
        ),
        period: num(EasingGroup::PERIOD.index, "easing.period", default.period),
        bezier,
    }
}

impl Timed {
    /// The timeline of `anim` and what is inside it, read from the objects
    /// as they are now.
    pub(crate) fn build(engine: &Engine, anim: ObjectId, defaults: Option<&Defaults>) -> Timed {
        let env = Env {
            defaults,
            changes: None,
            reparented: false,
        };
        Timed::build_in(engine, anim, env)
    }

    /// The timeline of a transition between states: its animations, all
    /// at once, moving what `changes` says changed.
    pub(crate) fn of_transition(engine: &Engine, anims: &[ObjectId], changes: &Changes) -> Timed {
        let env = Env {
            defaults: None,
            changes: Some(changes),
            reparented: false,
        };
        let list: Vec<Timed> = anims
            .iter()
            .filter(|a| is_a::<Animation>(engine, **a))
            .map(|a| Timed::build_in(engine, *a, env))
            .collect();
        let longest = list.iter().map(Timed::total).max().unwrap_or(0);
        Timed {
            node: Node::Par(list),
            start: 0,
            pass: longest,
            base: longest,
            calm: false,
            loops: 1,
            iter: 0,
            state: State::Idle,
        }
    }

    fn build_in(engine: &Engine, anim: ObjectId, env: Env<'_>) -> Timed {
        let defaults = env.defaults;
        let ends = || property_ends(engine, anim, defaults);
        let children = |env: Env<'_>| -> Vec<Timed> {
            objects(engine, anim, "animations")
                .into_iter()
                .filter(|c| is_a::<Animation>(engine, *c))
                .map(|c| Timed::build_in(engine, c, env))
                .collect()
        };
        // What it moves, and where a state takes each of those.
        let aimed = |kind: Kind| match env.changes {
            Some(changes) => of_changes(engine, anim, &changes.list, env.reparented, kind),
            None => (slots(engine, anim, defaults, false), Vec::new()),
        };
        let reversed = env.changes.is_some_and(|c| c.reversed);
        let (node, pass) = if is_a::<SequentialAnimation>(engine, anim) {
            let mut list = children(env);
            if reversed {
                list.reverse();
            }
            let mut at = 0i64;
            for c in &mut list {
                c.start = at;
                at = at.saturating_add(c.total()).min(FOREVER);
            }
            (Node::Seq(list), at)
        } else if is_a::<ParallelAnimation>(engine, anim) {
            let list = children(Env {
                reparented: env.reparented || is_a::<ParentAnimation>(engine, anim),
                ..env
            });
            let longest = list.iter().map(Timed::total).max().unwrap_or(0);
            (Node::Par(list), longest)
        } else if is_a::<SpringAnimation>(engine, anim) {
            let (slots, tos) = aimed(Kind::Number);
            let (mut from, mut to) = ends();
            if reversed {
                std::mem::swap(&mut from, &mut to);
            }
            let node = SpringNode::build(engine, anim, slots, from, to, &tos);
            let longest = node
                .tracks
                .iter()
                .flatten()
                .map(spring::Track::micros)
                .max();
            (Node::Spring(node), longest.unwrap_or(0))
        } else if is_a::<SmoothedAnimation>(engine, anim) {
            let (slots, tos) = aimed(Kind::Number);
            let node = SmoothNode::build(engine, anim, slots, ends().1, &tos);
            let longest = node.tracks.iter().flatten().map(Track::micros).max();
            (Node::Smooth(node), longest.unwrap_or(0))
        } else if is_a::<Animator>(engine, anim) {
            let kind = if is_a::<RotationAnimator>(engine, anim) {
                Kind::Rotation(Turn::of(engine.read_by_name(anim, "direction").to_number()))
            } else {
                Kind::Number
            };
            let (slots, tos) = aimed(kind);
            // `from` and `to` are numbers: one never written is not there.
            let own = |p: PropIndex| {
                (!is_unset(engine, anim, p)).then(|| Value::Real(engine.read(anim, p).to_number()))
            };
            let (mut from, mut to) = (
                own(Animator::FROM.index),
                own(Animator::TO.index)
                    .or_else(|| defaults.and_then(|d| d.to.clone()))
                    .or_else(|| env.changes.is_none().then_some(Value::Real(0.0))),
            );
            if reversed {
                std::mem::swap(&mut from, &mut to);
            }
            let held = Some(vec![None; slots.len()]);
            let node = PropNode {
                kind,
                slots,
                from,
                to,
                tos,
                curve: curve_of(engine, anim),
                lerps: None,
                held,
                jumps: None,
                pinned: false,
            };
            (Node::Prop(node), duration_of(engine, anim))
        } else if is_a::<PropertyAnimation>(engine, anim) {
            let kind = property_kind(engine, anim);
            let (slots, tos) = aimed(kind);
            let (mut from, mut to) = ends();
            if reversed {
                std::mem::swap(&mut from, &mut to);
            }
            let node = PropNode {
                kind,
                slots,
                from,
                to,
                tos,
                curve: curve_of(engine, anim),
                lerps: None,
                held: None,
                jumps: None,
                pinned: false,
            };
            (Node::Prop(node), duration_of(engine, anim))
        } else if is_a::<AnchorAnimation>(engine, anim) {
            // Outside a transition there are no anchors that changed.
            let (slots, tos) = match env.changes {
                Some(changes) => of_anchors(engine, anim, changes),
                None => (Vec::new(), Vec::new()),
            };
            let node = PropNode {
                kind: Kind::Number,
                slots,
                from: None,
                to: None,
                tos,
                curve: curve_of(engine, anim),
                lerps: None,
                held: None,
                jumps: None,
                pinned: false,
            };
            (Node::Prop(node), duration_of(engine, anim))
        } else if is_a::<PauseAnimation>(engine, anim) {
            (Node::Pause, duration_of(engine, anim))
        } else if is_a::<ScriptAction>(engine, anim) {
            // In a transition `scriptName` names a script of the state
            // entered, which then runs here instead of on entry.
            let name = engine.read_by_name(anim, "scriptName").to_js_string();
            let named = env
                .changes
                .filter(|_| !name.is_empty())
                .and_then(|c| c.scripts.iter().find(|(n, ..)| **n == *name));
            let (action, prop) = match named {
                Some((_, script, prop)) => (*script, *prop),
                None => (anim, ScriptAction::SCRIPT.index),
            };
            (
                Node::Script {
                    action,
                    prop,
                    fired: false,
                },
                0,
            )
        } else if is_a::<PropertyAction>(engine, anim) {
            let (mut slots, mut tos) = aimed(Kind::Any);
            if let Some(changes) = env.changes {
                let (more, ends) =
                    of_changes(engine, anim, &changes.kept, env.reparented, Kind::Any);
                for (slot, to) in more.into_iter().zip(ends) {
                    match slots
                        .iter()
                        .position(|s| s.obj == slot.obj && s.prop == slot.prop)
                    {
                        Some(i) => {
                            if tos[i].is_none() {
                                tos[i] = to;
                            }
                        }
                        None => {
                            slots.push(slot);
                            tos.push(to);
                        }
                    }
                }
            }
            // Without a value of its own it sets what the Behavior it is
            // in was given, or what the state gives the property.
            // One given `undefined` sets the property to what its type has
            // before anything is assigned (qml6).
            let read = match env.changes {
                Some(c) => c
                    .values
                    .iter()
                    .find(|(a, _)| *a == anim)
                    .map(|(_, v)| v.clone()),
                None => given(engine, anim, PropertyAction::VALUE.index),
            };
            let blank = matches!(read, Some(Value::Undefined));
            let own = read.filter(|_| !blank);
            let given = env.changes.is_some() && (own.is_some() || blank);
            let own = own.or_else(|| defaults.and_then(|d| d.to.clone()).filter(|_| !blank));
            let values = match env.changes {
                Some(_) if blank => slots.iter().map(|s| Some(s.ty.default_value())).collect(),
                Some(_) => tos.into_iter().map(|to| own.clone().or(to)).collect(),
                None if blank => slots.iter().map(|s| Some(s.ty.default_value())).collect(),
                None => slots
                    .iter()
                    .map(|_| Some(own.clone().unwrap_or_default()))
                    .collect(),
            };
            let node = ActionNode {
                slots,
                values,
                own: given,
            };
            (Node::Action { node, fired: false }, 0)
        } else {
            // An animation type this module does not know stands still.
            (Node::Pause, 0)
        };
        let loops = loops_of(engine, anim);
        Timed {
            // Measured on Qt: a SmoothedAnimation and a SpringAnimation
            // are over when they are there, whatever `loops` says.
            // Measured on Qt: one that takes no time is over after one
            // pass too, so an endless one never keeps the application busy.
            loops: if matches!(node, Node::Smooth(_) | Node::Spring(_)) || pass == 0 {
                loops.min(1)
            } else {
                loops
            },
            node,
            start: 0,
            pass,
            base: pass,
            calm: false,
            iter: 0,
            state: State::Idle,
        }
    }

    /// Takes the timeline [`Timed::build`] made of `anim` back to its
    /// beginning, towards what `defaults` says now, as a build would: the
    /// property's value then is where it begins. Only for one property
    /// animation whose kind, properties, ends, duration, easing and loops
    /// are still those it was built with; false, and nothing changed,
    /// for any other, which is built anew.
    pub(crate) fn restart_towards(
        &mut self,
        engine: &Engine,
        anim: ObjectId,
        defaults: &Defaults,
    ) -> bool {
        let Node::Prop(p) = &mut self.node else {
            return false;
        };
        // An Animator holds what it shows; an AnchorAnimation and a
        // transition's animation have what a state gives.
        if p.held.is_some() || !p.tos.is_empty() || !is_a::<PropertyAnimation>(engine, anim) {
            return false;
        }
        let Some(aimed) = defaults.aimed.as_ref().filter(|a| a.anim == anim) else {
            return false;
        };
        let same_slots = aimed.slots.len() == p.slots.len()
            && aimed
                .slots
                .iter()
                .zip(&p.slots)
                .all(|(a, b)| a.obj == b.obj && a.prop == b.prop);
        if !same_slots || property_kind(engine, anim) != p.kind {
            return false;
        }
        let (from, to) = property_ends(engine, anim, Some(defaults));
        let pass = duration_of(engine, anim);
        let loops = loops_of(engine, anim);
        let loops = if pass == 0 { loops.min(1) } else { loops };
        if from != p.from || pass != self.base || loops != self.loops {
            return false;
        }
        if curve_of(engine, anim) != p.curve {
            return false;
        }
        p.to = to;
        self.start = 0;
        self.reset(true);
        true
    }

    /// A timeline that begins and then does nothing, without end: what
    /// `loops: 0` is.
    pub(crate) fn inert() -> Timed {
        Timed {
            node: Node::Pause,
            start: 0,
            pass: 0,
            base: 0,
            calm: false,
            loops: FOREVER,
            iter: 0,
            state: State::Idle,
        }
    }

    /// A fade of `obj`'s `opacity` from `from` to `to` in `micros`, at the
    /// default easing: what reduced motion puts in place of an entrance or
    /// an exit that would move. `None` if `obj` has no `opacity`.
    pub(crate) fn fade(
        engine: &Engine,
        obj: ObjectId,
        from: f64,
        to: f64,
        micros: i64,
    ) -> Option<Timed> {
        let prop = engine.property_index(obj, "opacity").ok()?;
        let node = PropNode {
            kind: Kind::Number,
            slots: vec![Slot::of(engine, obj, prop)],
            from: Some(Value::Real(from)),
            to: Some(Value::Real(to)),
            tos: Vec::new(),
            curve: Curve::default(),
            lerps: None,
            held: None,
            jumps: None,
            pinned: false,
        };
        Some(Timed {
            node: Node::Prop(node),
            start: 0,
            pass: micros,
            base: micros,
            calm: false,
            loops: 1,
            iter: 0,
            state: State::Idle,
        })
    }

    pub(crate) fn loops(&self) -> i64 {
        self.loops
    }

    /// All passes together; [`FOREVER`] when there is no end.
    pub(crate) fn total(&self) -> i64 {
        if self.loops >= FOREVER {
            FOREVER
        } else {
            self.pass.saturating_mul(self.loops).min(FOREVER)
        }
    }

    pub(crate) fn is_endless(&self) -> bool {
        self.total() >= FOREVER
    }

    pub(crate) fn is_done(&self) -> bool {
        self.state == State::Done
    }

    /// Whether `stop()` on the way emits `finished` as well. Measured on
    /// Qt: it does for an animation without end, and for a
    /// SmoothedAnimation and a SpringAnimation, which have no duration to
    /// tell Qt when they end.
    pub(crate) fn finishes_when_stopped(&self) -> bool {
        self.is_endless()
            || matches!(self.node, Node::Smooth(_) | Node::Spring(_))
            || matches!(&self.node, Node::Prop(p) if p.held.is_some())
    }

    /// When the pass it is in ends. For `complete()` on an endless
    /// animation and for `alwaysRunToEnd`.
    pub(crate) fn end_of_pass(&self, t: i64) -> i64 {
        if self.pass == 0 || self.loops == 0 {
            return 0;
        }
        let k = (t / self.pass).min(self.loops - 1);
        (k + 1).saturating_mul(self.pass).min(FOREVER)
    }

    /// Makes the pass that contains `t` the last one.
    pub(crate) fn end_after_pass(&mut self, t: i64) {
        if self.pass > 0 && self.loops > 0 {
            self.loops = (t / self.pass).min(self.loops - 1) + 1;
        } else if self.loops >= FOREVER {
            self.loops = 1;
        }
    }

    /// The properties its parts move or set, now or later.
    pub(crate) fn moved(&self, out: &mut Vec<(ObjectId, PropIndex)>) {
        let slots = match &self.node {
            Node::Prop(p) => &p.slots,
            Node::Smooth(s) => &s.slots,
            Node::Spring(s) => &s.slots,
            Node::Action { node, .. } => &node.slots,
            Node::Seq(c) | Node::Par(c) => {
                c.iter().for_each(|c| c.moved(out));
                return;
            }
            Node::Pause | Node::Script { .. } => return,
        };
        out.extend(slots.iter().map(|s| (s.obj, s.prop)));
    }

    /// The properties a transition's `PropertyAction` sets to a value of
    /// its own: the change the state makes of each ends at that value,
    /// not at the state's (qml6).
    pub(crate) fn given(&self, out: &mut Vec<(ObjectId, PropIndex)>) {
        match &self.node {
            Node::Action { node, .. } if node.own => {
                out.extend(node.slots.iter().map(|s| (s.obj, s.prop)));
            }
            Node::Seq(c) | Node::Par(c) => c.iter().for_each(|c| c.given(out)),
            _ => {}
        }
    }

    /// Writes what Animators show and have not written yet, and shows the
    /// properties again: at their end, or when their run ends early.
    pub(crate) fn release(&mut self, engine: &Engine) {
        match &mut self.node {
            Node::Prop(p) => {
                let Some(held) = &mut p.held else {
                    return;
                };
                for (slot, v) in p.slots.iter().zip(held.iter_mut()) {
                    if let Some(v) = v.take() {
                        slot.write(engine, as_shown(slot, v));
                        shown::show(engine, slot.obj, slot.prop, None);
                    }
                }
            }
            Node::Seq(c) | Node::Par(c) => c.iter_mut().for_each(|c| c.release(engine)),
            _ => {}
        }
    }

    fn has_scripts(&self) -> bool {
        match &self.node {
            Node::Script { .. } => true,
            Node::Seq(c) | Node::Par(c) => c.iter().any(Timed::has_scripts),
            _ => false,
        }
    }

    /// Back to before the first pass.
    fn reset(&mut self, forget: bool) {
        self.state = State::Idle;
        self.iter = 0;
        self.rewind(forget);
    }

    /// Ready for another pass. `forget`: start values are taken anew.
    fn rewind(&mut self, forget: bool) {
        match &mut self.node {
            Node::Prop(p) => {
                if forget {
                    p.lerps = None;
                }
                if std::mem::take(&mut p.pinned) {
                    p.jumps = None;
                }
            }
            Node::Script { fired, .. } | Node::Action { fired, .. } => *fired = false,
            Node::Seq(c) | Node::Par(c) => c.iter_mut().for_each(|c| c.reset(forget)),
            Node::Smooth(_) | Node::Spring(_) | Node::Pause => {}
        }
    }

    /// Shows the moment `t` whatever was shown before, for a controller
    /// that moves back and forth. Start values taken once are kept.
    pub(crate) fn seek(&mut self, t: i64, cx: &Ctx<'_>) {
        self.reset(false);
        self.advance(t, cx);
    }

    /// Brings the animation to `t` microseconds after its beginning: runs
    /// what became due since the last call, in order, and writes the
    /// values of that moment.
    pub(crate) fn advance(&mut self, t: i64, cx: &Ctx<'_>) {
        if self.state == State::Done || cx.cancelled.get() {
            return;
        }
        if self.loops == 0 {
            self.state = State::Done;
            return;
        }
        self.state = State::Active;
        let t = t.max(0);
        let (d, n) = (self.pass, self.loops);
        // The pass `t` is in; the very end belongs to the last one.
        // One that takes no time has a single pass (see `build_in`).
        let k = if d == 0 { 0 } else { (t / d).min(n - 1) };
        while self.iter < k {
            self.pass_to(d, cx);
            if cx.cancelled.get() {
                return;
            }
            self.iter += 1;
            // Passes that only move properties leave things where every
            // pass leaves them: one is enough.
            if !self.has_scripts() {
                self.iter = k;
            }
            // A looping animation keeps its own start value; what is
            // inside a looping group starts each pass from where it is.
            self.rewind(!matches!(self.node, Node::Prop(_)));
        }
        let local = if d == 0 { 0 } else { (t - k * d).min(d) };
        self.pass_to(local, cx);
        if n < FOREVER && t >= self.total() && !cx.cancelled.get() {
            self.state = State::Done;
            self.release(cx.engine);
        }
    }

    /// Brings the current pass to `local`.
    fn pass_to(&mut self, local: i64, cx: &Ctx<'_>) {
        let pass = self.pass;
        match &mut self.node {
            Node::Prop(p) => {
                let resting = match p.lerps {
                    None => p.begin(cx.engine),
                    Some(_) => Vec::new(),
                };
                let progress = if pass == 0 {
                    1.0
                } else {
                    local as f64 / pass as f64
                };
                let curved = p.curve.value(progress);
                for (i, (slot, lerp)) in p.slots.iter().zip(p.lerps.iter().flatten()).enumerate() {
                    let jumps = p.jumps.as_ref().is_some_and(|j| j.get(i) == Some(&true));
                    let eased = if jumps { 1.0 } else { curved };
                    // A Behavior's start shows the property where it is: a
                    // write of the value it was given changes nothing.
                    if eased == 0.0 && p.held.is_none() && resting.get(i) == Some(&true) {
                        continue;
                    }
                    let Some(v) = lerp.at(eased) else {
                        continue;
                    };
                    match &mut p.held {
                        Some(held) => {
                            let n = v.to_number();
                            held[i] = Some(n);
                            shown::show(cx.engine, slot.obj, slot.prop, Some(n));
                        }
                        None => slot.write(cx.engine, v),
                    }
                }
            }
            Node::Smooth(s) => {
                // Measured on Qt: an AnimationController does not move a
                // SmoothedAnimation.
                if cx.scripts {
                    s.show(cx.engine, pass, local);
                }
            }
            Node::Spring(s) => {
                // Like a SmoothedAnimation it has no duration for a
                // controller's progress to be a part of.
                if cx.scripts {
                    s.show(cx.engine, pass, local);
                }
            }
            Node::Pause => {}
            Node::Script {
                action,
                prop,
                fired,
            } => {
                if !*fired {
                    *fired = true;
                    if cx.scripts {
                        cx.engine.run_script(*action, *prop);
                    }
                }
            }
            Node::Action { node, fired } => {
                if !*fired {
                    *fired = true;
                    for (slot, value) in node.slots.iter().zip(&node.values) {
                        let Some(value) = value else {
                            continue;
                        };
                        // Like an animated value, it leaves the property's
                        // binding in place.
                        let v = slot.ty.coerce(value).unwrap_or(value.clone());
                        slot.write(cx.engine, v);
                    }
                }
            }
            Node::Seq(children) => {
                for c in children {
                    if c.state == State::Done {
                        continue;
                    }
                    if local < c.start {
                        break;
                    }
                    c.advance(local - c.start, cx);
                    if c.state != State::Done || cx.cancelled.get() {
                        break;
                    }
                }
            }
            Node::Par(children) => {
                for c in children {
                    // Reduced motion, turned on or off on the way, may
                    // have one begin later (see `retime`).
                    if local < c.start {
                        continue;
                    }
                    c.advance(local - c.start, cx);
                    if cx.cancelled.get() {
                        break;
                    }
                }
            }
        }
    }

    /// Whether a frame would show something new now: a property of
    /// something that shows is on its way.
    pub(crate) fn wants_frames(&self, engine: &Engine) -> bool {
        if self.state != State::Active {
            return false;
        }
        match &self.node {
            Node::Prop(p) => p.lerps.as_ref().is_some_and(|lerps| {
                p.slots
                    .iter()
                    .zip(lerps)
                    .any(|(s, l)| !matches!(l, Lerp::None | Lerp::Jump { .. }) && s.shown(engine))
            }),
            Node::Smooth(s) => s
                .slots
                .iter()
                .zip(&s.tracks)
                .any(|(slot, track)| track.is_some() && slot.shown(engine)),
            Node::Spring(s) => s
                .slots
                .iter()
                .zip(&s.tracks)
                .any(|(slot, track)| track.is_some() && slot.shown(engine)),
            Node::Seq(c) | Node::Par(c) => c.iter().any(|c| c.wants_frames(engine)),
            _ => false,
        }
    }

    /// Whether running it does more than move values: a script, or a
    /// property set by an action.
    fn has_effects(&self) -> bool {
        match &self.node {
            Node::Script { .. } | Node::Action { .. } => true,
            Node::Seq(c) | Node::Par(c) => c.iter().any(Timed::has_effects),
            _ => false,
        }
    }

    /// Whether any property it moves, now or later, is one that shows.
    fn any_shown(&self, engine: &Engine) -> bool {
        match &self.node {
            Node::Prop(p) => p.slots.iter().any(|s| s.shown(engine)),
            Node::Smooth(s) => s.slots.iter().any(|s| s.shown(engine)),
            Node::Spring(s) => s.slots.iter().any(|s| s.shown(engine)),
            Node::Seq(c) | Node::Par(c) => c.iter().any(|c| c.any_shown(engine)),
            _ => false,
        }
    }

    /// When an animation that needs no frames now must next be looked at.
    /// One that only moves values nobody sees is looked at when it ends,
    /// and never if it has no end: whatever makes it show again brings a
    /// frame, and that frame puts it where it would have been.
    pub(crate) fn next_event(&self, engine: &Engine, t: i64) -> Option<i64> {
        if self.has_effects() || self.any_shown(engine) {
            self.next_boundary(t)
        } else if self.is_endless() {
            None
        } else {
            (t < self.total()).then(|| self.total())
        }
    }

    /// The first moment after `t` at which something other than a value
    /// on its way happens: a part begins or ends, a pass ends.
    pub(crate) fn next_boundary(&self, t: i64) -> Option<i64> {
        if t < 0 {
            return Some(0);
        }
        if t >= self.total() || self.pass == 0 {
            return None;
        }
        let d = self.pass;
        let k = t / d;
        let local = t - k * d;
        let inner = match &self.node {
            Node::Seq(c) | Node::Par(c) => c
                .iter()
                .filter_map(|c| c.next_boundary(local - c.start).map(|b| b + c.start))
                .min(),
            _ => None,
        };
        Some(k * d + inner.map_or(d, |i| i.min(d)))
    }

    /// Whether it is timed for reduced motion.
    pub(crate) fn calm(&self) -> bool {
        self.calm
    }

    /// Times it for reduced motion (`on`), or by its own durations again,
    /// and gives the moment that `t` (as it was timed) is now. What
    /// [`motion::jumps`] says moves is at its end at once, a fade lasts at
    /// most [`motion::REDUCED_FADE_MAX_MS`], a pause keeps its length.
    /// Each part keeps how far along it is, so a value under way goes on
    /// from where it is instead of jumping back or ahead. Only for a
    /// timeline with an end: one without stays as it is.
    pub(crate) fn retime(&mut self, engine: &Engine, on: bool, t: i64) -> i64 {
        self.calm = on;
        let under_way = self.state == State::Active && t >= 0;
        let (d, n) = (self.pass, self.loops);
        let k = if d == 0 { 0 } else { (t.max(0) / d).min(n - 1) };
        let local = if d == 0 { 0 } else { (t.max(0) - k * d).min(d) };
        let local = self.retime_pass(engine, on, under_way.then_some(local));
        if !under_way {
            return t;
        }
        k.saturating_mul(self.pass)
            .saturating_add(local)
            .min(FOREVER)
    }

    /// [`Timed::retime`] of one pass: sets `pass`, and gives where `local`
    /// is in it now (`None`: it is not under way).
    fn retime_pass(&mut self, engine: &Engine, on: bool, local: Option<i64>) -> i64 {
        let old = self.pass;
        let state = self.state;
        let base = self.base;
        let pass = match &mut self.node {
            Node::Prop(p) => p.retime(engine, on, state == State::Active, base),
            Node::Smooth(s) => calm_pass(on, base, s.slots.iter().all(|s| s.jumps(engine))),
            Node::Spring(s) => calm_pass(on, base, s.slots.iter().all(|s| s.jumps(engine))),
            Node::Pause | Node::Script { .. } | Node::Action { .. } => base,
            Node::Seq(children) => {
                // Only the first part not over is under way.
                let active = local.and_then(|l| {
                    children
                        .iter()
                        .position(|c| c.state != State::Done)
                        .filter(|i| l >= children[*i].start)
                });
                let mut at = 0i64;
                let mut mapped = None;
                for (i, c) in children.iter_mut().enumerate() {
                    let l = match (active == Some(i), local) {
                        (true, Some(l)) => l - c.start,
                        _ => -1,
                    };
                    let l = c.retime(engine, on, l);
                    c.start = at;
                    if active == Some(i) {
                        mapped = Some(at.saturating_add(l));
                    }
                    at = at.saturating_add(c.total()).min(FOREVER);
                }
                self.pass = at;
                return match local {
                    Some(_) => mapped.unwrap_or(at),
                    None => 0,
                };
            }
            Node::Par(children) => {
                let mapped: Vec<Option<i64>> = children
                    .iter_mut()
                    .map(|c| match local {
                        Some(l) if c.state != State::Done => {
                            Some(c.retime(engine, on, l - c.start))
                        }
                        _ => {
                            c.retime(engine, on, -1);
                            None
                        }
                    })
                    .collect();
                // Each part that is under way begins where it is as far
                // along as before: the parts no longer keep step, so some
                // begin later. The parts that are over end where they did.
                let at = mapped.iter().flatten().copied().max().unwrap_or(0);
                for (c, m) in children.iter_mut().zip(&mapped) {
                    c.start = m.map_or(0, |l| at - l);
                }
                self.pass = children
                    .iter()
                    .map(|c| c.start.saturating_add(c.total()).min(FOREVER))
                    .max()
                    .unwrap_or(0);
                return if local.is_some() { at } else { 0 };
            }
        };
        self.pass = pass;
        match local {
            Some(l) if old > 0 => {
                i64::try_from(i128::from(l) * i128::from(pass) / i128::from(old)).unwrap_or(pass)
            }
            _ => 0,
        }
    }
}

/// The pass of a SmoothedAnimation or a SpringAnimation. One that moves
/// is at its end at once; one that fades keeps its length, since cutting
/// its way short would only end it with a jump.
fn calm_pass(on: bool, base: i64, moves: bool) -> i64 {
    if on && moves { 0 } else { base }
}

impl PropNode {
    /// The pass for reduced motion (`on`) or without it, from the one it
    /// was built with. With reduced motion what moves shows its end from
    /// the first moment, and the pass is only as long as a fade may be;
    /// without, what is at its end already stays there for the rest of a
    /// pass that is `under_way`.
    fn retime(&mut self, engine: &Engine, on: bool, under_way: bool, base: i64) -> i64 {
        if !on {
            self.pinned = self.jumps.is_some() && under_way && self.lerps.is_some();
            if !self.pinned {
                self.jumps = None;
            }
            return base;
        }
        let jumps: Vec<bool> = self
            .slots
            .iter()
            .map(|s| !matches!(self.kind, Kind::Color) && s.jumps(engine))
            .collect();
        let pass = if jumps.is_empty() {
            // Moves nothing: a pause by another name.
            base
        } else if jumps.iter().all(|j| *j) {
            0
        } else {
            motion::capped_fade(base)
        };
        self.jumps = Some(jumps);
        self.pinned = false;
        pass
    }

    /// Takes the start values: the property's own where `from` is not set.
    /// Per slot, whether the start value is the one the property holds.
    fn begin(&mut self, engine: &Engine) -> Vec<bool> {
        let mut resting = Vec::with_capacity(self.slots.len());
        let lerps = self
            .slots
            .iter()
            .enumerate()
            .map(|(i, slot)| {
                let convert = |v: &Value| match self.kind {
                    Kind::Color => TypeTag::Color.coerce(v).ok(),
                    // Kept as written: a whole-number property passes
                    // through the fractions on its way.
                    Kind::Number | Kind::Rotation(_) => Some(v.clone()),
                    Kind::Any | Kind::Quaternion { .. } => slot.ty.coerce(v).ok(),
                };
                let own = self.to.as_ref();
                let to = own
                    .or_else(|| self.tos.get(i).and_then(Option::as_ref))
                    .and_then(convert);
                let (from, own) = match self.from.as_ref().and_then(convert) {
                    Some(v) => (v, false),
                    None => (engine.untrack(|| engine.read(slot.obj, slot.prop)), true),
                };
                let lerp = Lerp::new(self.kind, &slot.ty, &from, to.as_ref());
                // One never given a value is given its own by this write,
                // as on Qt, and no longer goes by its default.
                resting.push(
                    own && lerp.at(0.0).is_some_and(|v| v == from)
                        && !is_unset(engine, slot.obj, slot.prop),
                );
                lerp
            })
            .collect();
        self.lerps = Some(lerps);
        resting
    }
}

impl SmoothNode {
    fn params(engine: &Engine, anim: ObjectId) -> Params {
        let num = |p: PropIndex| engine.read(anim, p).to_number();
        Params {
            velocity: num(SmoothedAnimation::VELOCITY.index),
            duration: num(PropertyAnimation::DURATION.index),
            max_easing: num(SmoothedAnimation::MAXIMUM_EASING_TIME.index),
            reversing: Reversing::of(num(SmoothedAnimation::REVERSING_MODE.index)),
        }
    }

    /// Plans each property's way from where it is now, at the speed this
    /// animation last gave it. `from` plays no part: measured on Qt, a
    /// SmoothedAnimation always sets out from the property's own value.
    ///
    /// `to` is its own end value; `tos` has, per slot, the one a state
    /// gives (empty outside a transition).
    fn build(
        engine: &Engine,
        anim: ObjectId,
        slots: Vec<Slot>,
        to: Option<Value>,
        tos: &[Option<Value>],
    ) -> SmoothNode {
        let params = Self::params(engine, anim);
        let number = |v: &Value| Some(v.to_number()).filter(|n| !n.is_nan());
        let own = to.as_ref().and_then(number);
        let tracks = slots
            .iter()
            .enumerate()
            .map(|(i, slot)| {
                let to = own.or_else(|| tos.get(i).and_then(Option::as_ref).and_then(number));
                let from = engine
                    .untrack(|| engine.read(slot.obj, slot.prop))
                    .to_number();
                let speed = engine
                    .with::<SmoothedAnimation, _>(anim, |a| a.speed((slot.obj, slot.prop)))
                    .unwrap_or(0.0);
                Track::plan(from, to?, speed, &params, None)
            })
            .collect();
        SmoothNode {
            anim,
            slots,
            params,
            tracks,
            begun: false,
        }
    }

    /// Inside a group its turn may come after something else moved the
    /// property. The way is then planned again from where the property
    /// is, in the time the group set aside for it when it started.
    // TODO(smoothed-in-group): Qt gives it the time the new distance asks
    // for; that needs timelines whose durations change while they run.
    fn begin(&mut self, engine: &Engine, pass: i64) {
        for (slot, track) in self.slots.iter().zip(&mut self.tracks) {
            let Some(planned) = track else {
                continue;
            };
            let from = engine
                .untrack(|| engine.read(slot.obj, slot.prop))
                .to_number();
            if from != planned.from {
                let secs = pass as f64 / 1e6;
                *track = Track::plan(from, planned.to, 0.0, &self.params, Some(secs));
            }
        }
    }

    /// Puts every property where it is `local` microseconds into the
    /// pass, and notes the speeds.
    fn show(&mut self, engine: &Engine, pass: i64, local: i64) {
        if !std::mem::replace(&mut self.begun, true) {
            self.begin(engine, pass);
        }
        let over = local >= pass;
        let t = local as f64 / 1e6;
        let mut speeds = Vec::with_capacity(self.slots.len());
        for (slot, track) in self.slots.iter().zip(&self.tracks) {
            let Some(track) = track else {
                continue;
            };
            let (value, speed) = if over || track.is_over(t) {
                (track.to, 0.0)
            } else {
                track.at(t)
            };
            speeds.push(((slot.obj, slot.prop), speed));
            // At the very start the property stays as it is: the way
            // begins with the first step.
            if local == 0 && !over && !track.is_over(t) {
                continue;
            }
            let v = if matches!(slot.ty, TypeTag::Int) {
                Value::Int(
                    value
                        .round()
                        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
                )
            } else {
                Value::Real(value)
            };
            slot.write(engine, v);
        }
        let _ = engine.with_mut::<SmoothedAnimation, _>(self.anim, |a| {
            for (of, speed) in speeds {
                a.set_speed(of, speed);
            }
        });
    }
}

impl SpringNode {
    fn params(engine: &Engine, anim: ObjectId) -> spring::Params {
        let num = |p: PropIndex| engine.read(anim, p).to_number();
        spring::Params {
            spring: num(SpringAnimation::SPRING.index),
            damping: num(SpringAnimation::DAMPING.index),
            mass: num(SpringAnimation::MASS.index),
            velocity: num(SpringAnimation::VELOCITY.index),
            epsilon: num(SpringAnimation::EPSILON.index),
            modulus: num(SpringAnimation::MODULUS.index),
        }
    }

    /// Where `slot` is, as a spring sees it: a whole-number property shows
    /// the place rounded, and the spring goes on from the place itself.
    fn place(engine: &Engine, anim: ObjectId, slot: &Slot) -> (f64, f64) {
        let shown = engine
            .untrack(|| engine.read(slot.obj, slot.prop))
            .to_number();
        let last = engine
            .with::<SpringAnimation, _>(anim, |a| a.last((slot.obj, slot.prop)))
            .ok()
            .flatten();
        match last {
            Some((value, speed))
                if value == shown || as_shown(slot, value).to_number() == shown =>
            {
                (value, speed)
            }
            // Something else moved it since: the speed is still its own.
            Some((_, speed)) => (shown, speed),
            None => (shown, 0.0),
        }
    }

    /// Plans each property's way, at the speed this animation last gave
    /// it: from `from`, or from where the property is now.
    ///
    /// `to` is its own end value; `tos` has, per slot, the one a state
    /// gives (empty outside a transition).
    fn build(
        engine: &Engine,
        anim: ObjectId,
        slots: Vec<Slot>,
        from: Option<Value>,
        to: Option<Value>,
        tos: &[Option<Value>],
    ) -> SpringNode {
        let params = Self::params(engine, anim);
        let number = |v: &Value| Some(v.to_number()).filter(|n| !n.is_nan());
        let own_from = from.as_ref().and_then(number);
        let own = to.as_ref().and_then(number);
        let tracks = slots
            .iter()
            .enumerate()
            .map(|(i, slot)| {
                let to = own.or_else(|| tos.get(i).and_then(Option::as_ref).and_then(number));
                let (place, speed) = Self::place(engine, anim, slot);
                spring::Track::plan(own_from.unwrap_or(place), to?, speed, &params)
            })
            .collect();
        SpringNode {
            anim,
            slots,
            params,
            tracks,
            own_from: own_from.is_some(),
            begun: false,
        }
    }

    /// Inside a group its turn may come after something else moved the
    /// property. The way is then planned again from where the property
    /// is; it ends, at the value, when the time the group set aside for
    /// it when it started is over.
    // TODO(spring-in-group): Qt gives it the time the new way asks for;
    // that needs timelines whose durations change while they run.
    fn begin(&mut self, engine: &Engine) {
        if self.own_from {
            return;
        }
        for (slot, track) in self.slots.iter().zip(&mut self.tracks) {
            let Some(planned) = track else {
                continue;
            };
            let (place, speed) = Self::place(engine, self.anim, slot);
            if place != planned.from {
                *track = spring::Track::plan(place, planned.to, speed, &self.params);
            }
        }
    }

    /// Puts every property where it is `local` microseconds into the
    /// pass, and notes the places and speeds.
    fn show(&mut self, engine: &Engine, pass: i64, local: i64) {
        if !std::mem::replace(&mut self.begun, true) {
            self.begin(engine);
        }
        let over = local >= pass;
        let mut last = Vec::with_capacity(self.slots.len());
        for (slot, track) in self.slots.iter().zip(&mut self.tracks) {
            let Some(track) = track else {
                continue;
            };
            let there = over || local >= track.micros();
            let (value, speed) = if there {
                (track.rest(), 0.0)
            } else {
                track.at(local)
            };
            last.push(((slot.obj, slot.prop), (value, speed)));
            // Measured on Qt: at the very start the property stays as it
            // is; the way begins with the first step.
            if local == 0 && !there {
                continue;
            }
            slot.write(engine, as_shown(slot, value));
        }
        let _ = engine.with_mut::<SpringAnimation, _>(self.anim, |a| {
            for (of, state) in last {
                a.set_last(of, state);
            }
        });
    }
}

/// A number as `slot` holds it: whole for a whole-number property.
fn as_shown(slot: &Slot, value: f64) -> Value {
    if matches!(slot.ty, TypeTag::Int) {
        Value::Int(
            value
                .round()
                .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
        )
    } else {
        Value::Real(value)
    }
}
