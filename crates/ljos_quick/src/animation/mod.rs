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
//! - [`easing`]: the curves.
//! - [`smooth`]: the way a `SmoothedAnimation` takes.
//!
//! - [`transition`]: `Transition`, as far as a popup's `enter` and `exit`
//!   need it: its animations run on an object it is given.
//!
//! States (M2) are not here. A Transition between states will run its
//! animations the way a `Behavior` does: through
//! [`driver::start_driven`] with the property and end value as defaults.

pub(crate) mod behavior;
pub(crate) mod controller;
pub(crate) mod driver;
pub mod easing;
pub(crate) mod smooth;
pub(crate) mod timeline;
pub(crate) mod transition;
pub(crate) mod types;

use std::time::Duration;

use ljos_engine::{Engine, QmlType};
use ljos_runtime::Time;

use crate::item::is_a;

pub use behavior::{Behavior, BehaviorTarget};
pub use controller::AnimationController;
pub use transition::{Transition, run_transition, stop_transition};
pub use types::{
    Animation, ColorAnimation, Easing, EasingGroup, NumberAnimation, ParallelAnimation,
    PauseAnimation, PropertyAction, PropertyAnimation, RotationAnimation, ScriptAction,
    SequentialAnimation, SmoothedAnimation,
};

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
        AnimationController, Easing, RotationAnimation, SmoothedAnimation, Transition,
    ]));
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
}

/// Frame step 5: every running animation is brought to `now`.
pub(crate) fn tick(engine: &Engine, now: Time) {
    driver::tick(engine, now);
}

/// When the running animations next need a frame; `None` while none does.
pub(crate) fn next_wake(engine: &Engine, now: Time) -> Option<Time> {
    driver::next_wake(engine, now)
}

/// Tells the animations the display's refresh period (60 Hz until told):
/// a visible animation asks for one frame per period.
pub fn set_animation_frame_period(engine: &Engine, period: Duration) {
    driver::set_frame_period(engine, period);
}

/// How many animations are under way in `engine`, paused ones included.
/// Zero means the animations ask for no frames.
pub fn running_animation_count(engine: &Engine) -> usize {
    driver::run_count(engine)
}
