//! `QtQuick.Timeline`: keyframes on any property of any object, driven by a
//! frame number instead of the clock.
//!
//! A [`Timeline`] that is enabled holds the properties its groups name, as an
//! override: what the property was, bound or not, comes back when it is
//! disabled. It has a frame number and writes what that frame says, and does
//! nothing else, so a timeline nothing moves costs nothing; a
//! [`TimelineAnimation`] is a plain animation of the frame number.

mod animation;
mod keyframes;
mod timeline;

pub use animation::TimelineAnimation;
pub use timeline::{Keyframe, KeyframeGroup, Timeline};

/// Registers the `QtQuick.Timeline` module with an engine.
pub fn register(engine: &ljos::Engine) {
    engine.register_module(ljos::module!("QtQuick.Timeline" 1.0 => [
        Timeline, KeyframeGroup, Keyframe, TimelineAnimation,
    ]));
}
