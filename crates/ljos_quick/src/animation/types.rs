//! The animation types: `Animation` and what derives from it.
//!
//! The objects only hold what QML wrote. What an animation does is worked
//! out when it starts ([`super::timeline`]) and carried out by the
//! [`driver`](super::driver).

use std::rc::Rc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, Interception, ObjectId, PropIndex, QmlList, QmlObject, QtObject,
    Value, qml_impl,
};

use super::driver;
use super::easing::easing_enums;
use crate::enums::EnumTable;
use crate::item::{is_a, is_unset};

pub(crate) fn animation_enums() -> EnumTable {
    vec![("Loops", vec![("Infinite", -2)])]
}

/// `Animation` (QtQuick): what every animation type has.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Animation",
    extends = QtObject,
    uncreatable,
    enums = animation_enums
)]
pub struct Animation {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub running: bool,
    #[qml(notify)]
    pub paused: bool,
    /// How many passes; negative ([`Animation.Infinite`](animation_enums))
    /// for no end.
    #[qml(notify)]
    pub loops: i32,
    #[qml(notify)]
    pub always_run_to_end: bool,
    /// The property it was declared `on` (`NumberAnimation on x`): it
    /// starts by itself and animates that property unless told otherwise.
    #[qml(skip)]
    pub(crate) on: Option<(ObjectId, PropIndex)>,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            running: false,
            paused: false,
            loops: 1,
            always_run_to_end: false,
            on: None,
        }
    }
}

#[qml_impl]
impl Animation {
    #[qml(signal)]
    fn started();

    #[qml(signal)]
    fn stopped();

    #[qml(signal)]
    fn finished();

    #[qml(invokable)]
    fn start(cx: &Cx) -> Result<(), EngineError> {
        driver::start(cx.engine, cx.this);
        Ok(())
    }

    #[qml(invokable)]
    fn stop(cx: &Cx) -> Result<(), EngineError> {
        driver::stop(cx.engine, cx.this);
        Ok(())
    }

    #[qml(invokable)]
    fn pause(cx: &Cx) -> Result<(), EngineError> {
        driver::set_paused(cx.engine, cx.this, true);
        Ok(())
    }

    #[qml(invokable)]
    fn resume(cx: &Cx) -> Result<(), EngineError> {
        driver::set_paused(cx.engine, cx.this, false);
        Ok(())
    }

    #[qml(invokable)]
    fn restart(cx: &Cx) -> Result<(), EngineError> {
        driver::stop(cx.engine, cx.this);
        driver::start(cx.engine, cx.this);
        Ok(())
    }

    #[qml(invokable)]
    fn complete(cx: &Cx) -> Result<(), EngineError> {
        driver::complete(cx.engine, cx.this);
        Ok(())
    }

    /// From here on `running` and `paused` do what the methods do,
    /// whoever sets them.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        let engine = cx.engine;
        // A write is the method call, with the method's order of signals;
        // the animation stores the value itself.
        engine.set_interceptor(
            me,
            Animation::RUNNING.index,
            Some(Rc::new(move |e, v, how| match how {
                Interception::Write => {
                    if v.to_boolean() {
                        driver::start(e, me);
                    } else {
                        driver::stop(e, me);
                    }
                    true
                }
                // Stored first; the watch below follows it.
                Interception::Binding => false,
            })),
        );
        engine.set_interceptor(
            me,
            Animation::PAUSED.index,
            Some(Rc::new(move |e, v, how| match how {
                Interception::Write => {
                    driver::set_paused(e, me, v.to_boolean());
                    true
                }
                Interception::Binding => false,
            })),
        );
        // A binding's new value arrives here. So does every value the
        // driver writes, which by then is what the animation is doing.
        engine.watch(me, Animation::RUNNING.index, move |e, v| {
            let want = v.to_boolean();
            if want != driver::is_running(e, me) {
                if want {
                    driver::start(e, me);
                } else {
                    driver::stop(e, me);
                }
            }
        })?;
        engine.watch(me, Animation::PAUSED.index, move |e, v| {
            let want = v.to_boolean();
            if want != driver::is_paused(e, me) {
                driver::set_paused(e, me, want);
            }
        })?;
        if is_managed(engine, me) {
            return Ok(());
        }
        let source = cx.with(|a: &Animation| a.on.is_some())?;
        let running = cx.get(Animation::RUNNING)?;
        if running || (source && is_unset(engine, me, Animation::RUNNING.index)) {
            driver::start(engine, me);
        }
        Ok(())
    }
}

/// Whether something else runs this animation: the group or the
/// `Transition` it is in, or the `Behavior` it belongs to. Such an
/// animation never starts by itself.
pub(crate) fn is_managed(engine: &Engine, anim: ObjectId) -> bool {
    let Some(owner) = engine.owner(anim) else {
        return false;
    };
    if is_a::<SequentialAnimation>(engine, owner)
        || is_a::<ParallelAnimation>(engine, owner)
        || is_a::<super::transition::Transition>(engine, owner)
    {
        return matches!(
            engine.untrack(|| engine.read_by_name(owner, "animations")),
            Value::List(l) if l.iter().any(|v| v.as_object() == Some(anim))
        );
    }
    is_a::<super::behavior::Behavior>(engine, owner)
        && engine
            .untrack(|| engine.read_by_name(owner, "animation"))
            .as_object()
            == Some(anim)
}

/// `Easing` (QtQuick): the namespace of the curve names.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Easing",
    extends = QtObject,
    uncreatable,
    enums = easing_enums
)]
pub struct Easing {
    #[qml(base)]
    pub object: QtObject,
}

/// The `easing` group of a [`PropertyAnimation`].
#[derive(Debug, QmlObject)]
#[qml(name = "EasingCurve", uncreatable, enums = easing_enums)]
pub struct EasingGroup {
    /// `Easing.Type`.
    #[qml(notify, rename = "type", enumeration = "Type")]
    pub ty: i32,
    #[qml(notify)]
    pub amplitude: f64,
    #[qml(notify)]
    pub overshoot: f64,
    #[qml(notify)]
    pub period: f64,
    /// Control points of `Easing.BezierSpline`.
    #[qml(notify)]
    pub bezier_curve: Vec<Value>,
}

impl Default for EasingGroup {
    fn default() -> Self {
        let c = super::easing::Curve::default();
        Self {
            ty: c.ty,
            amplitude: c.amplitude,
            overshoot: c.overshoot,
            period: c.period,
            bezier_curve: Vec::new(),
        }
    }
}

/// `PropertyAnimation` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "PropertyAnimation", extends = Animation)]
pub struct PropertyAnimation {
    #[qml(base)]
    pub animation: Animation,
    #[qml(notify)]
    pub target: Option<ObjectId>,
    #[qml(list)]
    pub targets: QmlList,
    #[qml(notify)]
    pub property: Atom,
    #[qml(notify)]
    pub properties: Atom,
    /// `undefined`: the value the property has when the animation begins.
    #[qml(notify)]
    pub from: Value,
    /// `undefined`: the property is left alone.
    #[qml(notify)]
    pub to: Value,
    /// Milliseconds.
    #[qml(notify)]
    pub duration: i32,
    #[qml(group)]
    pub easing: EasingGroup,
}

impl Default for PropertyAnimation {
    fn default() -> Self {
        Self {
            animation: Animation::default(),
            target: None,
            targets: QmlList::new(),
            property: Atom::EMPTY,
            properties: Atom::EMPTY,
            from: Value::Undefined,
            to: Value::Undefined,
            duration: 250,
            easing: EasingGroup::default(),
        }
    }
}

/// `NumberAnimation` (QtQuick).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "NumberAnimation", extends = PropertyAnimation)]
pub struct NumberAnimation {
    #[qml(base)]
    pub base: PropertyAnimation,
}

/// `ColorAnimation` (QtQuick).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "ColorAnimation", extends = PropertyAnimation)]
pub struct ColorAnimation {
    #[qml(base)]
    pub base: PropertyAnimation,
}

pub(crate) fn rotation_enums() -> EnumTable {
    vec![(
        "RotationDirection",
        vec![
            ("Numerical", 0),
            ("Shortest", 1),
            ("Clockwise", 2),
            ("Counterclockwise", 3),
        ],
    )]
}

/// `RotationAnimation` (QtQuick): an angle, and which way round it turns.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "RotationAnimation",
    extends = PropertyAnimation,
    enums = rotation_enums
)]
pub struct RotationAnimation {
    #[qml(base)]
    pub base: PropertyAnimation,
    /// `RotationAnimation.RotationDirection`.
    #[qml(notify, enumeration = "RotationDirection")]
    pub direction: i32,
}

pub(crate) fn smoothed_enums() -> EnumTable {
    vec![(
        "ReversingMode",
        vec![("Eased", 0), ("Immediate", 1), ("Sync", 2)],
    )]
}

/// `SmoothedAnimation` (QtQuick): to a value at a velocity, speeding up
/// and slowing down on the way ([`super::smooth`]).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "SmoothedAnimation",
    extends = NumberAnimation,
    enums = smoothed_enums
)]
pub struct SmoothedAnimation {
    #[qml(base)]
    pub base: NumberAnimation,
    /// Units per second; -1 for none.
    #[qml(notify)]
    pub velocity: f64,
    /// `SmoothedAnimation.ReversingMode`.
    #[qml(notify, enumeration = "ReversingMode")]
    pub reversing_mode: i32,
    /// Milliseconds; -1 for none.
    #[qml(notify)]
    pub maximum_easing_time: f64,
    /// The speed each property it moved had when it was last looked at.
    /// Kept after the run: the next one sets out at that speed, which is
    /// what makes a new value on the way a smooth change of course.
    #[qml(skip)]
    pub(crate) speeds: Vec<((ObjectId, PropIndex), f64)>,
}

impl Default for SmoothedAnimation {
    fn default() -> Self {
        let mut base = NumberAnimation::default();
        // No duration: the velocity says how long it takes.
        base.base.duration = -1;
        Self {
            base,
            velocity: 200.0,
            reversing_mode: 0,
            maximum_easing_time: -1.0,
            speeds: Vec::new(),
        }
    }
}

impl SmoothedAnimation {
    pub(crate) fn speed(&self, of: (ObjectId, PropIndex)) -> f64 {
        self.speeds
            .iter()
            .find(|(k, _)| *k == of)
            .map_or(0.0, |(_, v)| *v)
    }

    pub(crate) fn set_speed(&mut self, of: (ObjectId, PropIndex), speed: f64) {
        match self.speeds.iter_mut().find(|(k, _)| *k == of) {
            Some((_, v)) => *v = speed,
            None => self.speeds.push((of, speed)),
        }
    }
}

/// `SpringAnimation` (QtQuick): pulled to a value the way a spring would
/// pull it, past it and back ([`super::spring`]).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "SpringAnimation", extends = NumberAnimation)]
pub struct SpringAnimation {
    #[qml(base)]
    pub base: NumberAnimation,
    /// How hard it pulls; 0 for no spring, which leaves the velocity.
    #[qml(notify)]
    pub spring: f64,
    #[qml(notify)]
    pub damping: f64,
    #[qml(notify)]
    pub mass: f64,
    /// Nearer to the value and slower than this, it is there.
    #[qml(notify)]
    pub epsilon: f64,
    /// 0 for none.
    #[qml(notify)]
    pub modulus: f64,
    /// Units per second, the most it may move at; 0 for no limit.
    #[qml(notify)]
    pub velocity: f64,
    /// Where each property it moved was when it was last looked at, and
    /// how fast. Kept after the run: the next one sets out at that speed
    /// (a new value on the way does not stop the swing), and from that
    /// place where the property could only show it rounded.
    #[qml(skip)]
    pub(crate) last: Vec<((ObjectId, PropIndex), (f64, f64))>,
}

impl Default for SpringAnimation {
    fn default() -> Self {
        Self {
            base: NumberAnimation::default(),
            spring: 0.0,
            damping: 0.0,
            mass: 1.0,
            epsilon: 0.01,
            modulus: 0.0,
            velocity: 0.0,
            last: Vec::new(),
        }
    }
}

impl SpringAnimation {
    /// The value and the speed `of` had when this animation last moved it.
    pub(crate) fn last(&self, of: (ObjectId, PropIndex)) -> Option<(f64, f64)> {
        self.last.iter().find(|(k, _)| *k == of).map(|(_, v)| *v)
    }

    pub(crate) fn set_last(&mut self, of: (ObjectId, PropIndex), state: (f64, f64)) {
        match self.last.iter_mut().find(|(k, _)| *k == of) {
            Some((_, v)) => *v = state,
            None => self.last.push((of, state)),
        }
    }
}

/// `SequentialAnimation` (QtQuick): one child after the other.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "SequentialAnimation",
    extends = Animation,
    default = "animations"
)]
pub struct SequentialAnimation {
    #[qml(base)]
    pub animation: Animation,
    #[qml(list)]
    pub animations: QmlList,
}

/// `ParallelAnimation` (QtQuick): all children at once.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "ParallelAnimation",
    extends = Animation,
    default = "animations"
)]
pub struct ParallelAnimation {
    #[qml(base)]
    pub animation: Animation,
    #[qml(list)]
    pub animations: QmlList,
}

/// `PauseAnimation` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "PauseAnimation", extends = Animation)]
pub struct PauseAnimation {
    #[qml(base)]
    pub animation: Animation,
    /// Milliseconds.
    #[qml(notify)]
    pub duration: i32,
}

impl Default for PauseAnimation {
    fn default() -> Self {
        Self {
            animation: Animation::default(),
            duration: 250,
        }
    }
}

/// `ScriptAction` (QtQuick): a statement that runs when its turn comes.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "ScriptAction", extends = Animation)]
pub struct ScriptAction {
    #[qml(base)]
    pub animation: Animation,
    /// Kept by the engine as a statement ([`Engine::run_script`]); the
    /// field holds nothing.
    #[qml(notify)]
    pub script: Value,
    /// The `StateChangeScript` to run instead, inside a transition.
    #[qml(notify)]
    pub script_name: Atom,
}

/// `PropertyAction` (QtQuick): sets properties when its turn comes.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "PropertyAction", extends = Animation)]
pub struct PropertyAction {
    #[qml(base)]
    pub animation: Animation,
    #[qml(notify)]
    pub target: Option<ObjectId>,
    #[qml(list)]
    pub targets: QmlList,
    #[qml(notify)]
    pub property: Atom,
    #[qml(notify)]
    pub properties: Atom,
    #[qml(notify)]
    pub value: Value,
}

/// `AnchorAnimation` (QtQuick): in a transition, moves the items a state
/// anchored otherwise from where they were to where they now belong.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "AnchorAnimation", extends = Animation)]
pub struct AnchorAnimation {
    #[qml(base)]
    pub animation: Animation,
    /// The items it moves; none: every item the state anchored otherwise.
    #[qml(list)]
    pub targets: QmlList,
    /// Milliseconds.
    #[qml(notify)]
    pub duration: i32,
    #[qml(group)]
    pub easing: EasingGroup,
}

impl Default for AnchorAnimation {
    fn default() -> Self {
        Self {
            animation: Animation::default(),
            targets: QmlList::new(),
            duration: 250,
            easing: EasingGroup::default(),
        }
    }
}

/// `ParentAnimation` (QtQuick): in a transition, what is inside it moves
/// an item a state gave another parent from where it was on screen to
/// its place there.
///
/// It runs its animations all at once, which is what a
/// `ParallelAnimation` does: so it is one here.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "ParentAnimation",
    extends = ParallelAnimation,
    default = "animations"
)]
pub struct ParentAnimation {
    #[qml(base)]
    pub base: ParallelAnimation,
    /// Stored: the item is the one the state reparents.
    #[qml(notify)]
    pub target: Option<ObjectId>,
    /// Stored: the parent is the one the state gives.
    #[qml(notify)]
    pub new_parent: Option<ObjectId>,
    /// Stored.
    #[qml(notify)]
    pub via: Option<ObjectId>,
}
