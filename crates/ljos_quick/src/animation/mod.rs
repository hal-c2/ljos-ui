//! Animations: `Animation` and its types, `Behavior`,
//! `AnimationController`, and the driver that runs them once per frame.
//!
//! - [`types`]: the QML objects. They hold what QML wrote and nothing else.
//! - [`timeline`]: what an animation does over time, worked out when it
//!   starts. It takes the time since the start and puts every property
//!   where it belongs at that moment, so the result does not depend on how
//!   many frames there were.
//! - [`driver`]: what is running, the per-frame tick, and when the next
//!   frame is needed (none while nothing runs).
//! - [`frame`]: `FrameAnimation`, a script once a frame.
//! - [`easing`]: the curves.
//! - [`smooth`]: the way a `SmoothedAnimation` takes.
//! - [`spring`]: the way a `SpringAnimation` takes.
//! - [`shown`]: what an Animator shows while its property keeps the old
//!   value.
//!
//! - [`transition`]: `Transition`. Its animations run on an object it is
//!   given (a popup's `enter` and `exit`, a view's `add` and `remove`), or
//!   between two states, on what the change of state changed.
//!
//! States are in [`crate::states`]. A change of state hands the timeline
//! what changed ([`timeline::Changes`]) and the driver runs the result
//! under the transition ([`driver::start_timeline`]).

pub(crate) mod behavior;
pub(crate) mod controller;
pub(crate) mod driver;
pub mod easing;
pub(crate) mod frame;
pub(crate) mod motion;
pub(crate) mod shown;
pub(crate) mod smooth;
pub(crate) mod spring;
pub(crate) mod timeline;
pub(crate) mod transition;
pub(crate) mod types;

use std::time::Duration;

use ljos_engine::{Engine, ObjectId, QmlType};
use ljos_runtime::Time;

use crate::item::is_a;

pub use behavior::{Behavior, BehaviorTarget};
pub use controller::AnimationController;
pub use frame::FrameAnimation;
pub use transition::{
    Passage, Transition, run_passage, run_transition, run_transition_to, stop_transition,
};
pub use types::{
    AnchorAnimation, Animation, Animator, ColorAnimation, Easing, EasingGroup, NumberAnimation,
    OpacityAnimator, ParallelAnimation, ParentAnimation, PauseAnimation, PropertyAction,
    PropertyAnimation, QuaternionAnimation, RotationAnimation, RotationAnimator, ScaleAnimator,
    ScriptAction, SequentialAnimation, SmoothedAnimation, SpringAnimation, UniformAnimator,
    Vector3dAnimation, XAnimator, YAnimator,
};

/// `NumberAnimation on eulerRotation.y { }`: the animation moves a
/// number of its own, kept equal to the member and written into the
/// whole value as it moves, the way a Behavior on a member does.
fn animate_member(
    e: &Engine,
    anim: ObjectId,
    target: ObjectId,
    prop: ljos_engine::PropIndex,
    member: &str,
) -> bool {
    use behavior::MemberValue;
    use ljos_engine::{Atom, Value};
    use ljos_reactive::WriteKind;
    let Some(ty) = e
        .meta_of(target)
        .and_then(|m| m.property(prop).map(|p| p.ty.clone()))
    else {
        return false;
    };
    let member = Atom::new(member);
    let now = |e: &Engine| e.peek(target, prop).member(&member).map(|v| v.to_number());
    let start = now(e).unwrap_or(0.0);
    let Ok(proxy) = e.create_native(
        MemberValue {
            value: start,
            ..Default::default()
        },
        Some(anim),
    ) else {
        return false;
    };
    let m = member.clone();
    let _ = e.watch(proxy, MemberValue::VALUE.index, move |e, v| {
        let whole = e.peek(target, prop);
        if let Some(w) = whole.with_member(&ty, &m, v)
            && w != whole
        {
            let _ = e.write_internal(target, prop, w, WriteKind::Animation);
        }
    });
    // Anything else that moves the member moves the number too, so an
    // animation without `from` starts where the member is.
    let _ = e.watch(target, prop, move |e, v| {
        if let Some(n) = v.member(&member).map(|v| v.to_number())
            && e.peek(proxy, MemberValue::VALUE.index).to_number() != n
        {
            let _ = e.write_internal(
                proxy,
                MemberValue::VALUE.index,
                Value::Real(n),
                WriteKind::Animation,
            );
        }
    });
    e.with_mut::<Animation, _>(anim, |a| a.on = Some((proxy, MemberValue::VALUE.index)))
        .is_ok()
}

/// Hooks the animation types into the engine. Once per engine; more calls
/// do nothing.
pub(crate) fn register(engine: &Engine) {
    let driver = engine.extension::<driver::Driver>();
    if driver.registered.replace(true) {
        return;
    }
    // Its own list, merged into the module by the registry: the crate's
    // list of items is not this module's to keep.
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [
        Animation, PropertyAnimation, NumberAnimation, ColorAnimation, SequentialAnimation,
        ParallelAnimation, PauseAnimation, ScriptAction, PropertyAction, Behavior,
        AnimationController, FrameAnimation, Easing, RotationAnimation, SmoothedAnimation, Transition,
        AnchorAnimation, ParentAnimation, SpringAnimation, Animator, OpacityAnimator, XAnimator,
        YAnimator, ScaleAnimator, RotationAnimator, UniformAnimator, Vector3dAnimation,
    ]));
    // A transition's animations are made when it first runs, in Qt: what
    // they are given is not worked out before then.
    engine.register_deferred_contents(Transition::static_meta());
    // `script: ...` is a statement to run when its turn comes.
    engine.register_script_property(ScriptAction::static_meta(), ScriptAction::SCRIPT.index);
    // `NumberAnimation on x { }` and `Behavior on x { }`.
    engine.on_value_source(|e, source, target, prop| {
        if is_a::<Behavior>(e, source) {
            behavior::attach(e, source, target, prop);
            true
        } else if is_a::<Animation>(e, source) {
            e.with_mut::<Animation, _>(source, |a| a.on = Some((target, prop)))
                .is_ok()
        } else {
            false
        }
    });
    // `Behavior on font.pixelSize { }`.
    engine.on_member_value_source(|e, source, target, prop, member| {
        if is_a::<Behavior>(e, source) {
            behavior::attach_member(e, source, target, prop, member)
        } else {
            is_a::<Animation>(e, source) && animate_member(e, source, target, prop, member)
        }
    });
}

/// Frame step 5: every running animation is brought to `now`.
pub(crate) fn tick(engine: &Engine, now: Time) {
    driver::tick(engine, now);
}

/// When the running animations next need a frame; `None` while none does.
pub(crate) fn next_wake(engine: &Engine, now: Time) -> Option<Time> {
    driver::next_wake(engine, now)
}

/// Whether the next tick steps an animation; it changes nothing, unlike
/// [`next_wake`], which plans the frames it asks for.
pub(crate) fn any_stepping(engine: &Engine) -> bool {
    driver::any_stepping(engine)
}

/// Tells the animations the display's refresh period (60 Hz until told):
/// a visible animation asks for one frame per period, and a `Qt.callLater`
/// queued while the queue runs waits one.
pub fn set_animation_frame_period(engine: &Engine, period: Duration) {
    driver::set_frame_period(engine, period);
    engine.set_frame_period(period);
}

/// The display's refresh period, as [`set_animation_frame_period`] gave it.
pub(crate) fn frame_period(engine: &Engine) -> Duration {
    driver::frame_period(engine)
}

/// How many animations are under way in `engine`, paused ones included.
/// Zero means the animations ask for no frames.
pub fn running_animation_count(engine: &Engine) -> usize {
    driver::run_count(engine)
}

/// The animations under way in `engine`, each with whether it is paused,
/// in the order they started. For an observer.
pub fn running_animations(engine: &Engine) -> Vec<(ObjectId, bool)> {
    driver::runs(engine)
}
