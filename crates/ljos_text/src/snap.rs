//! Glyph advances as Qt counts them.
//!
//! A shaper hands out advances as exact fractions of a pixel. Qt keeps them
//! in 1/64 pixel, and how it gets there decides the width of every text, so
//! this follows what Qt 6.11 reports (measured as a black box, at whole pixel
//! sizes, which is what a QML font always has):
//!
//! - the advance a font gives a glyph is cut down to 1/64 pixel, after the
//!   font's scale and the advance have been through the 16.16 fixed-point
//!   steps a FreeType-based font engine takes;
//! - what shaping adds or takes (kerning, letter and word spacing) is rounded
//!   to the nearest 1/64 pixel on its own;
//! - a glyph the shaper left without an advance (a combining mark) has none.

use parley::FontData;

/// 1/64 pixel, as a float.
const UNIT: f64 = 64.0;
/// Float noise in a shaped advance, in 1/64 pixel: far below what moves a
/// rounding, far above the error of an `f32` advance.
const NOISE: f64 = 1.0 / 256.0;

/// A font's own advance for `units` font units at `size` pixels, in whole
/// 1/64 pixels.
fn nominal(units: f64, size: f64, units_per_em: f64) -> f64 {
    // The scale is a 16.16 number of 1/64 pixels per font unit, the advance a
    // 16.16 number of pixels; both are rounded, then the advance is cut.
    let scale = (size * UNIT * 65536.0 / units_per_em).round();
    let linear = (units.round() * scale / UNIT).round();
    (linear / 1024.0).floor()
}

/// Qt's advance for every glyph of a run. `glyphs` are `(id, advance)` as
/// shaped, in pixels. Without the font's own advances (data that is not a
/// font) the shaped ones are returned as they are.
pub(crate) fn advances(
    font: &FontData,
    size: f32,
    normalized_coords: &[i16],
    glyphs: impl Iterator<Item = (u32, f32)> + Clone,
) -> Vec<f32> {
    let ids = glyphs.clone().map(|(id, _)| id);
    let Some((units_per_em, mut out)) = crate::measure::advances(font, normalized_coords, ids)
    else {
        return glyphs.map(|(_, advance)| advance).collect();
    };
    let (size, units_per_em) = (f64::from(size), f64::from(units_per_em));
    // The font's advances become Qt's where they are: every text that is
    // made counts its glyphs here, and a list for each step cost more than
    // the sums.
    for (slot, (_, shaped)) in out.iter_mut().zip(glyphs) {
        let units = f64::from(*slot);
        *slot = if shaped == 0.0 {
            0.0
        } else {
            let own = nominal(units, size, units_per_em);
            // Against the exact advance, not the cut one: this is what the
            // shaper added, which Qt rounds separately.
            let exact = units * size * UNIT / units_per_em;
            let added = (f64::from(shaped) * UNIT - exact + 0.5 + NOISE).floor();
            ((own + added) / UNIT) as f32
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::nominal;

    #[test]
    fn advances_are_cut_to_a_64th() {
        // Liberation Sans at 14 px, checked on Qt: H, e, l and the space.
        for (units, want) in [
            (1479.0, 647.0),
            (1139.0, 498.0),
            (455.0, 199.0),
            (569.0, 248.0),
        ] {
            assert_eq!(nominal(units, 14.0, 2048.0), want);
        }
        // Exactly on the grid stays there.
        assert_eq!(nominal(1024.0, 14.0, 2048.0), 448.0);
    }

    #[test]
    fn the_scale_is_rounded_before_the_advance() {
        // 1000 units per em: 14 px is 58720.256 as a 16.16 scale, which is
        // rounded down, so half an em comes out a 64th under 7 px.
        assert_eq!(nominal(500.0, 14.0, 1000.0), 447.0);
        assert_eq!(nominal(500.0, 16.0, 1000.0), 512.0);
    }
}
