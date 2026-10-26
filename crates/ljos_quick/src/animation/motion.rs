//! What reduced motion does to an animated property (ARCHITECTURE §14,
//! docs/design/motion.md). It is there to spare people for whom movement
//! on the screen brings on dizziness or nausea (WCAG 2.2 SC 2.3.3): what
//! slides, grows or turns is at its end at once. A fade or a change of
//! colour moves nothing and tells that something changed, so it stays, at
//! most [`REDUCED_FADE_MAX_MS`] long.
//!
//! The one place that says which property is which, so that the driver,
//! the entrances and exits of popups and pages, and their tests agree.

use ljos_engine::TypeTag;

pub(crate) use ljos_design::limits::motion::REDUCED_FADE_MAX_MS;

/// What a property's change is to a person who asked for less motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Motion {
    /// It moves something: a place, a size, a scale, a turn, a scroll.
    Moves,
    /// It fades something or changes its colour.
    Fades,
    /// Neither, as far as its name and type tell: a number of the
    /// application's own.
    Other,
}

/// The properties whose change moves something on the screen: an item's
/// place and size, its scale and rotation, where a Flickable's content is,
/// how far a Drawer is in, and what the Translate, Scale and Rotation
/// transforms hold.
const MOVES: &[&str] = &[
    "x",
    "y",
    "z",
    "width",
    "height",
    "implicitWidth",
    "implicitHeight",
    "scale",
    "rotation",
    "contentX",
    "contentY",
    "originX",
    "originY",
    "position",
    "angle",
    "xScale",
    "yScale",
    "origin",
    "axis",
    "currentIndex",
    "offset",
    "progress",
];

/// What a change of the property `name`, of type `ty`, is.
pub(crate) fn motion_of(name: &str, ty: &TypeTag) -> Motion {
    if name == "opacity" || matches!(ty, TypeTag::Color) {
        Motion::Fades
    } else if MOVES.contains(&name) {
        Motion::Moves
    } else {
        Motion::Other
    }
}

/// Whether reduced motion takes the property to its end at once. A number
/// of the application's own may well place something (a binding turns it
/// into an `x`), and a value shown at once hides nothing, so it is treated
/// as one that moves.
pub(crate) fn jumps(name: &str, ty: &TypeTag) -> bool {
    motion_of(name, ty) != Motion::Fades
}

/// A pass of `micros` that fades, as reduced motion has it.
pub(crate) fn capped_fade(micros: i64) -> i64 {
    micros.min(i64::from(REDUCED_FADE_MAX_MS) * 1000)
}

#[cfg(test)]
mod tests {
    use ljos_engine::TypeTag;

    use super::{Motion, capped_fade, jumps, motion_of};

    #[test]
    fn a_place_a_size_a_scale_and_a_turn_move() {
        for name in [
            "x",
            "y",
            "width",
            "height",
            "implicitWidth",
            "implicitHeight",
            "scale",
            "rotation",
            "contentX",
            "contentY",
            "position",
            "angle",
            "xScale",
            "yScale",
        ] {
            assert_eq!(motion_of(name, &TypeTag::Real), Motion::Moves, "{name}");
            assert!(jumps(name, &TypeTag::Real), "{name}");
        }
    }

    #[test]
    fn opacity_and_any_colour_fade() {
        assert_eq!(motion_of("opacity", &TypeTag::Real), Motion::Fades);
        assert_eq!(motion_of("color", &TypeTag::Color), Motion::Fades);
        assert_eq!(motion_of("borderTint", &TypeTag::Color), Motion::Fades);
        assert!(!jumps("opacity", &TypeTag::Real));
    }

    #[test]
    fn a_number_of_the_applications_own_is_at_its_end_at_once() {
        assert_eq!(motion_of("level", &TypeTag::Real), Motion::Other);
        assert!(jumps("level", &TypeTag::Real));
    }

    #[test]
    fn a_fade_is_cut_to_the_limit_and_a_shorter_one_kept() {
        assert_eq!(capped_fade(1_000_000), 150_000);
        assert_eq!(capped_fade(100_000), 100_000);
        assert_eq!(capped_fade(0), 0);
    }
}
