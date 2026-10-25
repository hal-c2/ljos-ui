//! Alpha compositing the way the scene graph blends: in gamma-encoded sRGB
//! space, not linear light. Qt Quick and egui both blend the encoded values
//! (`out = top*a + bottom*(1-a)` per channel), so an audit that composites
//! layers must do the same or it computes contrasts the user never sees.

use super::srgb::{Srgb, Srgba};

/// `top` painted over an opaque `bottom`.
pub fn over(top: Srgba, bottom: Srgb) -> Srgb {
    let a = top.a.clamp(0.0, 1.0);
    let mix = |t: f32, b: f32| t * a + b * (1.0 - a);
    Srgb::new(
        mix(top.r, bottom.r),
        mix(top.g, bottom.g),
        mix(top.b, bottom.b),
    )
}

/// A colour under an item's `opacity`: the item's alpha is multiplied in,
/// and a parent's opacity multiplies again, so a stack folds with repeated
/// calls.
pub fn opacity(c: Srgba, item_opacity: f32) -> Srgba {
    Srgba {
        a: c.a * item_opacity.clamp(0.0, 1.0),
        ..c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_black_over_white_is_mid_grey_in_gamma_space() {
        let c = over(Srgb::BLACK.with_alpha(0.5), Srgb::WHITE);
        assert!((c.r - 0.5).abs() < 1e-6);
    }

    #[test]
    fn opaque_and_transparent_extremes() {
        let red = Srgb::new(1.0, 0.0, 0.0);
        assert_eq!(over(red.with_alpha(1.0), Srgb::WHITE), red);
        assert_eq!(over(red.with_alpha(0.0), Srgb::WHITE), Srgb::WHITE);
    }

    #[test]
    fn opacity_multiplies_and_clamps() {
        let c = opacity(opacity(Srgb::WHITE.with_alpha(0.8), 0.5), 0.5);
        assert!((c.a - 0.2).abs() < 1e-6);
        assert_eq!(opacity(Srgb::WHITE.with_alpha(1.0), 2.0).a, 1.0);
    }
}
