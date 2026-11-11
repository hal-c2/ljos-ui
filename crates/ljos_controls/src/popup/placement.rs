//! Where a popup goes when it does not fit, as arithmetic.
//!
//! One rule for both axes, read off qml6. A popup that may flip and runs
//! past the window, or past a margin it has, goes to the other side of
//! its parent when less of it runs past there (a tie stays). Then, if it
//! still runs past and is no larger than the room, it is pushed inside.
//! After that its margins push it back in, whether or not it flips (a
//! negative margin asks for nothing, and what does not fit keeps to the
//! start). A combo box's popup flips vertically and a cascading sub-menu
//! sideways; everything else only meets its margins. `position.rs` reads
//! the numbers and writes the answer; nothing here knows the engine.

/// The axes along which a popup goes to the other side of its parent
/// when it has more room there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Flips {
    pub(crate) horizontal: bool,
    pub(crate) vertical: bool,
}

impl Flips {
    /// It stays where it was put, pushed in by its margins.
    pub(crate) const NONE: Self = Self {
        horizontal: false,
        vertical: false,
    };
    /// Above its parent when it does not fit below: a combo box's popup.
    pub(crate) const VERTICAL: Self = Self {
        horizontal: false,
        vertical: true,
    };
    /// To the other side of its parent when it does not fit beside it: a
    /// cascading sub-menu.
    pub(crate) const HORIZONTAL: Self = Self {
        horizontal: true,
        vertical: false,
    };
}

/// A popup along one axis, in the coordinates of its overlay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Axis {
    /// Where it was put.
    pub(super) at: f64,
    /// Where it would be on the other side of its parent; `None` when it
    /// does not flip along this axis.
    pub(super) mirrored: Option<f64>,
    pub(super) size: f64,
    /// The extent of the overlay. Nothing moves without one.
    pub(super) bound: f64,
    /// The margins at the start and at the end; a negative one asks for
    /// nothing.
    pub(super) near: f64,
    pub(super) far: f64,
}

/// Where the popup goes along `axis`.
pub(super) fn settle(axis: Axis) -> f64 {
    if axis.bound <= 0.0 {
        return axis.at;
    }
    let at = match axis.mirrored {
        Some(mirrored) => flip(axis, mirrored),
        None => axis.at,
    };
    clamp(at, axis.size, axis.bound, axis.near, axis.far)
}

/// Where a popup that flips goes before its margins are met. Here a
/// margin it lacks counts as none, so the window's edge is the limit.
fn flip(a: Axis, mirrored: f64) -> f64 {
    let start = a.near.max(0.0);
    let end = a.bound - a.far.max(0.0);
    let over = |p: f64| (start - p).max(0.0) + (p + a.size - end).max(0.0);
    let mut at = a.at;
    if over(at) <= 0.0 {
        return at;
    }
    if over(mirrored) < over(at) {
        at = mirrored;
    }
    if over(at) > 0.0 && a.size <= end - start {
        at = at.min(end - a.size).max(start);
    }
    at
}

/// Moves `pos` so that `size` stays `near` from the start and `far` from
/// the end of `bound`; a negative margin asks for nothing. What does not
/// fit keeps to the start.
fn clamp(mut pos: f64, size: f64, bound: f64, near: f64, far: f64) -> f64 {
    if far >= 0.0 && pos + size > bound - far {
        pos = bound - far - size;
    }
    if near >= 0.0 && pos < near {
        pos = near;
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::{Axis, clamp, settle};

    /// A popup along an axis 400 long, with `m` as both its margins.
    fn axis(at: f64, mirrored: Option<f64>, size: f64, m: f64) -> Axis {
        Axis {
            at,
            mirrored,
            size,
            bound: 400.0,
            near: m,
            far: m,
        }
    }

    #[test]
    fn no_margin_leaves_the_popup_where_it_is() {
        assert_eq!(clamp(450.0, 100.0, 400.0, -1.0, -1.0), 450.0);
        assert_eq!(clamp(-50.0, 100.0, 400.0, -1.0, -1.0), -50.0);
    }

    #[test]
    fn a_margin_pushes_the_popup_back_in() {
        assert_eq!(clamp(350.0, 100.0, 400.0, 0.0, 0.0), 300.0);
        assert_eq!(clamp(350.0, 100.0, 400.0, 20.0, 20.0), 280.0);
        assert_eq!(clamp(-30.0, 100.0, 400.0, 5.0, 5.0), 5.0);
        // One side only.
        assert_eq!(clamp(350.0, 100.0, 400.0, -1.0, 0.0), 300.0);
        assert_eq!(clamp(-30.0, 100.0, 400.0, -1.0, 0.0), -30.0);
    }

    #[test]
    fn what_does_not_fit_keeps_to_the_start() {
        assert_eq!(clamp(10.0, 500.0, 400.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn without_an_overlay_nothing_moves() {
        let a = Axis {
            bound: 0.0,
            ..axis(350.0, Some(0.0), 100.0, 0.0)
        };
        assert_eq!(settle(a), 350.0);
    }

    /// (where it was put, where it would be mirrored, size, margins,
    /// where it goes). The sideways rows are the sub-menus of
    /// `features/controls/menu.feature` in the overlay's coordinates: a
    /// menu at `mx` 120 wide with an overlap of 1 puts a sub-menu at
    /// `mx + 119`, or `mx + 1 - width` on the other side; a mirrored one
    /// asks for the other side first.
    const FLIPS: &[(f64, f64, f64, f64, f64)] = &[
        // It fits: it stays.
        (129.0, -109.0, 120.0, 0.0, 129.0),
        // Less runs past on the other side, where it fits.
        (369.0, 131.0, 120.0, 0.0, 131.0),
        // Less runs past on the other side, and it is pushed in there.
        (319.0, -49.0, 250.0, 0.0, 0.0),
        (319.0, -49.0, 250.0, -1.0, 0.0),
        // More runs past on the other side: it stays, pushed in.
        (239.0, -129.0, 250.0, 0.0, 150.0),
        (239.0, -129.0, 250.0, -1.0, 150.0),
        // As much: it stays.
        (259.0, -109.0, 250.0, -1.0, 150.0),
        // Wider than the room: where it was put, then its margins.
        (219.0, -349.0, 450.0, -1.0, 219.0),
        (219.0, -349.0, 450.0, 0.0, 0.0),
        // A margin it has is an edge to flip from; inside it, it stays.
        (250.0, 50.0, 120.0, 40.0, 50.0),
        (250.0, 50.0, 120.0, 20.0, 250.0),
        // A mirrored menu asks for the left of its item first (`mx + 1 -
        // width`), and the right (`mx + 119`) is where it is mirrored to:
        // it stays on the left where it fits, and comes back to the right
        // where less runs past there.
        (131.0, 369.0, 120.0, 0.0, 131.0),
        (-109.0, 129.0, 120.0, 0.0, 129.0),
        (-109.0, 129.0, 120.0, -1.0, 129.0),
        (-149.0, 219.0, 250.0, -1.0, 150.0),
    ];

    #[test]
    fn a_popup_that_flips_goes_where_less_of_it_runs_past() {
        for &(at, mirrored, size, m, want) in FLIPS {
            let got = settle(axis(at, Some(mirrored), size, m));
            assert_eq!(
                got, want,
                "at {at}, mirrored {mirrored}, {size} long, margins {m}"
            );
        }
    }

    #[test]
    fn a_popup_that_does_not_flip_only_meets_its_margins() {
        for &(at, _, size, m, _) in FLIPS {
            let got = settle(axis(at, None, size, m));
            assert_eq!(
                got,
                clamp(at, size, 400.0, m, m),
                "at {at}, {size} long, margins {m}"
            );
        }
    }
}
