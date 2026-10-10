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

use ljos_engine::{
    Color, Engine, LogLevel, ObjectId, Point, PropIndex, Rect, Size, TypeTag, Value,
};
use ljos_reactive::WriteKind;

use super::easing::Curve;
use super::smooth::{Params, Reversing, Track};
use super::types::{
    Animation, ColorAnimation, NumberAnimation, ParallelAnimation, PauseAnimation, PropertyAction,
    PropertyAnimation, RotationAnimation, ScriptAction, SequentialAnimation, SmoothedAnimation,
};
use crate::item::{Item, is_a, is_item};

/// "Never": a duration or a loop count without end. Far from `i64::MAX`, so
/// sums of a few of them do not overflow.
pub(crate) const FOREVER: i64 = i64::MAX / 16;

/// More passes than this of a group that takes no time are not run: a
/// huge `loops` on a script must not hang the frame.
const MAX_INSTANT_PASSES: i64 = 10_000;

/// What a `Behavior` (or a property the animation is declared `on`) gives
/// the animations that name no target of their own.
#[derive(Clone, Debug)]
pub(crate) struct Defaults {
    pub obj: ObjectId,
    pub prop: PropIndex,
    /// The value a Behavior animates to, for animations without `to`.
    pub to: Option<Value>,
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
    /// One pass.
    pass: i64,
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
    Pause,
    Script { action: ObjectId, fired: bool },
    Action { node: ActionNode, fired: bool },
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
}

/// Which values an animation type moves between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Number,
    Color,
    /// An angle in degrees, turning the way [`Turn`] says.
    Rotation(Turn),
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
    curve: Curve,
    /// Per slot, from the first moment of the node. `None`: not begun.
    lerps: Option<Vec<Lerp>>,
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

#[derive(Debug)]
struct ActionNode {
    slots: Vec<Slot>,
    value: Value,
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
            (Kind::Color, _) | (Kind::Any, TypeTag::Color) => colors(),
            (Kind::Number, _) | (Kind::Any, TypeTag::Int | TypeTag::Real) => numbers(),
            (Kind::Any, _) => match (from, to) {
                (Value::Point(a), Value::Point(b)) => Lerp::Point { from: **a, to: **b },
                (Value::Size(a), Value::Size(b)) => Lerp::Size { from: **a, to: **b },
                (Value::Rect(a), Value::Rect(b)) => Lerp::Rect { from: **a, to: **b },
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
            Lerp::Jump { to } => to.clone(),
        })
    }
}

impl Slot {
    fn shown(&self, engine: &Engine) -> bool {
        !self.item
            || engine
                .untrack(|| engine.read(self.obj, Item::VISIBLE.index))
                .to_boolean()
    }

    fn write(&self, engine: &Engine, v: Value) {
        // The target may be gone, or refuse the value; either way the
        // animation goes on.
        let _ = engine.write_internal(self.obj, self.prop, v, WriteKind::Animation);
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
    match engine.read_by_name(obj, name) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| engine.is_alive(*o))
            .collect(),
        _ => Vec::new(),
    }
}

fn duration_of(engine: &Engine, anim: ObjectId) -> i64 {
    let ms = engine.read_by_name(anim, "duration").to_number();
    if ms.is_finite() && ms > 0.0 {
        (ms * 1000.0) as i64
    } else {
        0
    }
}

/// The properties `target`, `targets`, `property` and `properties` name,
/// with the defaults filling in for what is not named.
fn slots(engine: &Engine, anim: ObjectId, defaults: Option<&Defaults>, quiet: bool) -> Vec<Slot> {
    let mut targets = objects(engine, anim, "targets");
    if let Some(t) = engine.read_by_name(anim, "target").as_object()
        && !targets.contains(&t)
    {
        targets.insert(0, t);
    }
    let mut names: Vec<String> = Vec::new();
    let one = engine.read_by_name(anim, "property").to_js_string();
    let many = engine.read_by_name(anim, "properties").to_js_string();
    for n in std::iter::once(&*one).chain(many.split(',')) {
        let n = n.trim();
        if !n.is_empty() && !names.iter().any(|x| x == n) {
            names.push(n.to_owned());
        }
    }
    let slot = |obj: ObjectId, prop: PropIndex| {
        let ty = engine
            .meta_of(obj)
            .and_then(|m| m.property(prop).map(|p| p.ty.clone()))
            .unwrap_or(TypeTag::Var);
        Slot {
            obj,
            prop,
            ty,
            item: is_item(engine, obj),
        }
    };
    if let Some(d) = defaults {
        if targets.is_empty() && names.is_empty() {
            return vec![slot(d.obj, d.prop)];
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
    for t in &targets {
        for n in &names {
            match engine.property_index(*t, n) {
                Ok(p) => out.push(slot(*t, p)),
                Err(_) if quiet || assumed => {}
                Err(_) => warn(
                    engine,
                    anim,
                    format_args!("Cannot animate non-existent property \"{n}\""),
                ),
            }
        }
    }
    out
}

/// Whether `anim`, run with these defaults, takes the defaults' property
/// somewhere: one of its parts animates or sets it. A `Behavior` whose
/// animation does not lets the value through.
pub(crate) fn moves(engine: &Engine, anim: ObjectId, d: &Defaults) -> bool {
    if is_a::<SequentialAnimation>(engine, anim) || is_a::<ParallelAnimation>(engine, anim) {
        objects(engine, anim, "animations")
            .into_iter()
            .any(|c| moves(engine, c, d))
    } else if is_a::<PropertyAnimation>(engine, anim) || is_a::<PropertyAction>(engine, anim) {
        slots(engine, anim, Some(d), true)
            .iter()
            .any(|s| s.obj == d.obj && s.prop == d.prop)
    } else {
        false
    }
}

fn curve_of(engine: &Engine, anim: ObjectId) -> Curve {
    let num = |name: &str| engine.read_by_name(anim, name).to_number();
    let bezier = match engine.read_by_name(anim, "easing.bezierCurve") {
        Value::List(l) => l.iter().map(Value::to_number).collect(),
        _ => Vec::new(),
    };
    Curve {
        ty: num("easing.type") as i32,
        amplitude: num("easing.amplitude"),
        overshoot: num("easing.overshoot"),
        period: num("easing.period"),
        bezier,
    }
}

impl Timed {
    /// The timeline of `anim` and what is inside it, read from the objects
    /// as they are now.
    pub(crate) fn build(engine: &Engine, anim: ObjectId, defaults: Option<&Defaults>) -> Timed {
        let set = |name: &str| match engine.read_by_name(anim, name) {
            Value::Undefined => None,
            v => Some(v),
        };
        let children = |defaults: Option<&Defaults>| -> Vec<Timed> {
            objects(engine, anim, "animations")
                .into_iter()
                .filter(|c| is_a::<Animation>(engine, *c))
                .map(|c| Timed::build(engine, c, defaults))
                .collect()
        };
        let (node, pass) = if is_a::<SequentialAnimation>(engine, anim) {
            let mut list = children(defaults);
            let mut at = 0i64;
            for c in &mut list {
                c.start = at;
                at = at.saturating_add(c.total()).min(FOREVER);
            }
            (Node::Seq(list), at)
        } else if is_a::<ParallelAnimation>(engine, anim) {
            let list = children(defaults);
            let longest = list.iter().map(Timed::total).max().unwrap_or(0);
            (Node::Par(list), longest)
        } else if is_a::<SmoothedAnimation>(engine, anim) {
            let node = SmoothNode::build(
                engine,
                anim,
                slots(engine, anim, defaults, false),
                set("to").or_else(|| defaults.and_then(|d| d.to.clone())),
            );
            let longest = node.tracks.iter().flatten().map(Track::micros).max();
            (Node::Smooth(node), longest.unwrap_or(0))
        } else if is_a::<PropertyAnimation>(engine, anim) {
            let kind = if is_a::<RotationAnimation>(engine, anim) {
                Kind::Rotation(Turn::of(engine.read_by_name(anim, "direction").to_number()))
            } else if is_a::<NumberAnimation>(engine, anim) {
                Kind::Number
            } else if is_a::<ColorAnimation>(engine, anim) {
                Kind::Color
            } else {
                Kind::Any
            };
            let node = PropNode {
                kind,
                slots: slots(engine, anim, defaults, false),
                from: set("from"),
                to: set("to").or_else(|| defaults.and_then(|d| d.to.clone())),
                curve: curve_of(engine, anim),
                lerps: None,
            };
            (Node::Prop(node), duration_of(engine, anim))
        } else if is_a::<PauseAnimation>(engine, anim) {
            (Node::Pause, duration_of(engine, anim))
        } else if is_a::<ScriptAction>(engine, anim) {
            (
                Node::Script {
                    action: anim,
                    fired: false,
                },
                0,
            )
        } else if is_a::<PropertyAction>(engine, anim) {
            let node = ActionNode {
                slots: slots(engine, anim, defaults, false),
                // Without a value of its own it sets what the Behavior it
                // is in was given.
                value: set("value")
                    .or_else(|| defaults.and_then(|d| d.to.clone()))
                    .unwrap_or_default(),
            };
            (Node::Action { node, fired: false }, 0)
        } else {
            // An animation type this module does not know stands still.
            (Node::Pause, 0)
        };
        let loops = engine.read(anim, Animation::LOOPS.index).to_number();
        let loops = if loops < 0.0 { FOREVER } else { loops as i64 };
        Timed {
            // Measured on Qt: a SmoothedAnimation is over when it is
            // there, whatever `loops` says.
            loops: if matches!(node, Node::Smooth(_)) {
                loops.min(1)
            } else {
                loops
            },
            node,
            start: 0,
            pass,
            iter: 0,
            state: State::Idle,
        }
    }

    /// A timeline that begins and then does nothing, without end: what
    /// `loops: 0` is.
    pub(crate) fn inert() -> Timed {
        Timed {
            node: Node::Pause,
            start: 0,
            pass: 0,
            loops: FOREVER,
            iter: 0,
            state: State::Idle,
        }
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
    /// SmoothedAnimation, which has no duration to tell Qt when it ends.
    pub(crate) fn finishes_when_stopped(&self) -> bool {
        self.is_endless() || matches!(self.node, Node::Smooth(_))
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
            }
            Node::Script { fired, .. } | Node::Action { fired, .. } => *fired = false,
            Node::Seq(c) | Node::Par(c) => c.iter_mut().for_each(|c| c.reset(forget)),
            Node::Smooth(_) | Node::Pause => {}
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
        let k = if d == 0 {
            if n >= FOREVER {
                0
            } else {
                (n - 1).min(MAX_INSTANT_PASSES)
            }
        } else {
            (t / d).min(n - 1)
        };
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
        }
    }

    /// Brings the current pass to `local`.
    fn pass_to(&mut self, local: i64, cx: &Ctx<'_>) {
        let pass = self.pass;
        match &mut self.node {
            Node::Prop(p) => {
                if p.lerps.is_none() {
                    p.begin(cx.engine);
                }
                let progress = if pass == 0 {
                    1.0
                } else {
                    local as f64 / pass as f64
                };
                let eased = p.curve.value(progress);
                for (slot, lerp) in p.slots.iter().zip(p.lerps.iter().flatten()) {
                    if let Some(v) = lerp.at(eased) {
                        slot.write(cx.engine, v);
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
            Node::Pause => {}
            Node::Script { action, fired } => {
                if !*fired {
                    *fired = true;
                    if cx.scripts {
                        cx.engine.run_script(*action, ScriptAction::SCRIPT.index);
                    }
                }
            }
            Node::Action { node, fired } => {
                if !*fired {
                    *fired = true;
                    for slot in &node.slots {
                        // Like an animated value, it leaves the property's
                        // binding in place.
                        let v = slot.ty.coerce(&node.value).unwrap_or(node.value.clone());
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
                    c.advance(local, cx);
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
}

impl PropNode {
    /// Takes the start values: the property's own where `from` is not set.
    fn begin(&mut self, engine: &Engine) {
        let lerps = self
            .slots
            .iter()
            .map(|slot| {
                let convert = |v: &Value| match self.kind {
                    Kind::Color => TypeTag::Color.coerce(v).ok(),
                    // Kept as written: a whole-number property passes
                    // through the fractions on its way.
                    Kind::Number | Kind::Rotation(_) => Some(v.clone()),
                    Kind::Any => slot.ty.coerce(v).ok(),
                };
                let to = self.to.as_ref().and_then(convert);
                let from = match self.from.as_ref().and_then(convert) {
                    Some(v) => v,
                    None => engine.untrack(|| engine.read(slot.obj, slot.prop)),
                };
                Lerp::new(self.kind, &slot.ty, &from, to.as_ref())
            })
            .collect();
        self.lerps = Some(lerps);
    }
}

impl SmoothNode {
    fn params(engine: &Engine, anim: ObjectId) -> Params {
        let num = |name: &str| engine.read_by_name(anim, name).to_number();
        Params {
            velocity: num("velocity"),
            duration: num("duration"),
            max_easing: num("maximumEasingTime"),
            reversing: Reversing::of(num("reversingMode")),
        }
    }

    /// Plans each property's way from where it is now, at the speed this
    /// animation last gave it. `from` plays no part: measured on Qt, a
    /// SmoothedAnimation always sets out from the property's own value.
    fn build(engine: &Engine, anim: ObjectId, slots: Vec<Slot>, to: Option<Value>) -> SmoothNode {
        let params = Self::params(engine, anim);
        let to = to.map(|v| v.to_number()).filter(|n| !n.is_nan());
        let tracks = slots
            .iter()
            .map(|slot| {
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
            if local == 0 && !track.is_over(t) {
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
