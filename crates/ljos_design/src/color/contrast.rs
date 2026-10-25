//! Contrast: WCAG 2.2's ratio (what the rules fail on) and APCA's lightness
//! contrast (advice only).

use super::oklab::srgb_to_linear;
use super::srgb::Srgb;
use crate::limits;

/// Relative luminance, WCAG 2.2 "relative luminance" definition: linearise
/// each channel, then weight by the Rec. 709 primaries.
pub fn luminance(c: Srgb) -> f32 {
    0.2126 * srgb_to_linear(c.r) + 0.7152 * srgb_to_linear(c.g) + 0.0722 * srgb_to_linear(c.b)
}

/// WCAG 2.2 contrast ratio, 1 (identical) to 21 (black on white); symmetric.
pub fn wcag_contrast(a: Srgb, b: Srgb) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Whether text of this pixel size and weight is "large" under WCAG 2.2
/// SC 1.4.3 and so needs only the lower ratio.
pub fn is_large_text(px: f32, weight: u16) -> bool {
    px >= limits::LARGE_TEXT_PX
        || (weight >= limits::BOLD_WEIGHT && px >= limits::LARGE_BOLD_TEXT_PX)
}

/// APCA lightness contrast `Lc`, roughly -108..=106. Positive is dark text
/// on a light background, negative light text on dark: polarity matters
/// because the same pair reads differently reversed. Advisory only.
///
/// Implemented from the formula and constants published in the APCA
/// description "SAPC-APCA 0.0.98G-4g" (the WCAG 3 working-draft
/// materials, github.com/Myndex/SAPC-APCA documentation): a simple 2.4
/// exponent for luminance, soft clamp of near-black, power-curve lightness
/// difference per polarity, a low-contrast clip and an offset.
pub fn apca_lc(text: Srgb, background: Srgb) -> f32 {
    const NORM_BG: f32 = 0.56;
    const NORM_TXT: f32 = 0.57;
    const REV_TXT: f32 = 0.62;
    const REV_BG: f32 = 0.65;
    const BLK_THRS: f32 = 0.022;
    const BLK_CLMP: f32 = 1.414;
    const SCALE: f32 = 1.14;
    const LO_OFFSET: f32 = 0.027;
    const DELTA_Y_MIN: f32 = 0.0005;
    const LO_CLIP: f32 = 0.1;

    // APCA's luminance is not WCAG's: plain 2.4 power, not the piecewise
    // sRGB curve, with its own coefficients.
    let y = |c: Srgb| {
        0.212_672_9 * c.r.clamp(0.0, 1.0).powf(2.4)
            + 0.715_152_2 * c.g.clamp(0.0, 1.0).powf(2.4)
            + 0.072_175 * c.b.clamp(0.0, 1.0).powf(2.4)
    };
    let soft = |y: f32| {
        if y < BLK_THRS {
            y + (BLK_THRS - y).powf(BLK_CLMP)
        } else {
            y
        }
    };
    let (yt, yb) = (soft(y(text)), soft(y(background)));
    if (yb - yt).abs() < DELTA_Y_MIN {
        return 0.0;
    }
    let lc = if yb > yt {
        let s = (yb.powf(NORM_BG) - yt.powf(NORM_TXT)) * SCALE;
        if s < LO_CLIP { 0.0 } else { s - LO_OFFSET }
    } else {
        let s = (yb.powf(REV_BG) - yt.powf(REV_TXT)) * SCALE;
        if s > -LO_CLIP { 0.0 } else { s + LO_OFFSET }
    };
    lc * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Srgb {
        Srgb::parse(s).unwrap()
    }

    #[test]
    fn black_on_white_is_21() {
        assert!((wcag_contrast(Srgb::BLACK, Srgb::WHITE) - 21.0).abs() < 1e-3);
        assert!((wcag_contrast(Srgb::WHITE, Srgb::WHITE) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_aa_boundary_grey() {
        let w = Srgb::WHITE;
        assert!((wcag_contrast(hex("#777777"), w) - 4.48).abs() < 0.01);
        assert!(wcag_contrast(hex("#767676"), w) >= limits::TEXT_CONTRAST);
        assert!(wcag_contrast(hex("#777777"), w) < limits::TEXT_CONTRAST);
        assert_eq!(wcag_contrast(hex("#777"), w), wcag_contrast(w, hex("#777")));
    }

    #[test]
    fn large_text_by_size_and_weight() {
        assert!(is_large_text(24.0, 400));
        assert!(!is_large_text(23.0, 400));
        assert!(is_large_text(18.67, 700));
        assert!(!is_large_text(18.0, 700));
        assert!(!is_large_text(18.67, 600));
    }

    #[test]
    fn apca_published_pairs() {
        let (g, w) = (hex("#888888"), Srgb::WHITE);
        assert!((apca_lc(g, w) - 63.1).abs() < 0.2, "{}", apca_lc(g, w));
        assert!((apca_lc(w, g) + 68.5).abs() < 0.2, "{}", apca_lc(w, g));
        // Polarity: dark on light is positive, the reverse negative.
        assert!(apca_lc(Srgb::BLACK, w) > 100.0);
        assert!(apca_lc(w, Srgb::BLACK) < -100.0);
        assert_eq!(apca_lc(g, g), 0.0);
    }
}
