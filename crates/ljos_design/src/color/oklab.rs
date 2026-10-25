//! OKLab and OKLCH, with the matrices of Björn Ottosson's published
//! "A perceptual color space for image processing" (2020), and the sRGB
//! transfer function.

use super::srgb::Srgb;

/// sRGB decode: gamma-encoded 0..1 to linear light.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB encode: linear light to gamma-encoded.
pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Perceptual lightness `l` (0..1) and the green-red `a` and blue-yellow `b`
/// axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklab {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

/// OKLab in polar form: lightness, chroma and hue in degrees (0..360).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

impl Oklab {
    pub fn from_srgb(c: Srgb) -> Oklab {
        let (r, g, b) = (
            srgb_to_linear(c.r),
            srgb_to_linear(c.g),
            srgb_to_linear(c.b),
        );
        let l = (0.412_221_47 * r + 0.536_332_54 * g + 0.051_445_99 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_84 * g + 0.629_978_7 * b).cbrt();
        Oklab {
            l: 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            a: 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            b: 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        }
    }

    /// Linear-light sRGB, which may lie outside 0..1 when out of gamut.
    fn to_linear(self) -> [f32; 3] {
        let l = (self.l + 0.396_337_78 * self.a + 0.215_803_76 * self.b).powi(3);
        let m = (self.l - 0.105_561_346 * self.a - 0.063_854_17 * self.b).powi(3);
        let s = (self.l - 0.089_484_18 * self.a - 1.291_485_5 * self.b).powi(3);
        [
            4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
            -1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s,
            -0.004_196_086 * l - 0.703_418_6 * m + 1.707_614_7 * s,
        ]
    }

    /// Clamps into sRGB; use [`Oklch::to_srgb_mapped`] to keep hue and
    /// lightness instead.
    pub fn to_srgb(self) -> Srgb {
        let [r, g, b] = self.to_linear().map(|v| linear_to_srgb(v.clamp(0.0, 1.0)));
        Srgb::new(r, g, b)
    }

    pub fn to_oklch(self) -> Oklch {
        let c = self.a.hypot(self.b);
        let h = if c < 1e-6 {
            0.0
        } else {
            self.b.atan2(self.a).to_degrees().rem_euclid(360.0)
        };
        Oklch { l: self.l, c, h }
    }
}

impl Oklch {
    pub fn from_srgb(c: Srgb) -> Oklch {
        Oklab::from_srgb(c).to_oklch()
    }

    pub fn to_oklab(self) -> Oklab {
        let (s, c) = self.h.to_radians().sin_cos();
        Oklab {
            l: self.l,
            a: self.c * c,
            b: self.c * s,
        }
    }

    /// Whether this colour exists in sRGB (all linear channels in 0..=1,
    /// with a small epsilon for rounding).
    pub fn in_srgb_gamut(self) -> bool {
        const EPS: f32 = 1e-4;
        self.to_oklab()
            .to_linear()
            .iter()
            .all(|v| (-EPS..=1.0 + EPS).contains(v))
    }

    /// Maps into sRGB by lowering chroma at fixed lightness and hue (binary
    /// search for the largest chroma that fits), so the result is the same
    /// lightness and hue, only duller. Clipping channels instead would shift
    /// both.
    pub fn to_srgb_mapped(self) -> Srgb {
        let l = self.l.clamp(0.0, 1.0);
        let at = |c: f32| Oklch { l, c, h: self.h };
        if at(self.c).in_srgb_gamut() {
            return at(self.c).to_oklab().to_srgb();
        }
        let (mut lo, mut hi) = (0.0, self.c);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if at(mid).in_srgb_gamut() {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        at(lo).to_oklab().to_srgb()
    }
}

/// Perceptual distance between two colours: Euclidean distance in OKLab. A
/// just-noticeable difference is about 0.02.
pub fn delta_e_ok(a: Srgb, b: Srgb) -> f32 {
    let (x, y) = (Oklab::from_srgb(a), Oklab::from_srgb(b));
    ((x.l - y.l).powi(2) + (x.a - y.a).powi(2) + (x.b - y.b).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(got: Oklab, l: f32, a: f32, b: f32) {
        assert!(
            (got.l - l).abs() < 1e-3 && (got.a - a).abs() < 1e-3 && (got.b - b).abs() < 1e-3,
            "{got:?} vs {l} {a} {b}"
        );
    }

    #[test]
    fn ottosson_reference_values() {
        close(Oklab::from_srgb(Srgb::WHITE), 1.0, 0.0, 0.0);
        close(
            Oklab::from_srgb(Srgb::new(1.0, 0.0, 0.0)),
            0.628,
            0.2249,
            0.1258,
        );
        close(
            Oklab::from_srgb(Srgb::new(0.0, 1.0, 0.0)),
            0.8664,
            -0.2339,
            0.1795,
        );
        close(
            Oklab::from_srgb(Srgb::new(0.0, 0.0, 1.0)),
            0.4520,
            -0.0325,
            -0.3115,
        );
        close(Oklab::from_srgb(Srgb::BLACK), 0.0, 0.0, 0.0);
    }

    #[test]
    fn round_trips() {
        for hex in ["#336699", "#ff8800", "#123456", "#fedcba", "#808080"] {
            let c = Srgb::parse(hex).unwrap();
            let back = Oklab::from_srgb(c).to_srgb();
            assert_eq!(back.to_hex(), hex);
            let back = Oklch::from_srgb(c).to_oklab().to_srgb();
            assert_eq!(back.to_hex(), hex);
        }
        for v in [0.0, 0.01, 0.2, 0.5, 1.0] {
            assert!((linear_to_srgb(srgb_to_linear(v)) - v).abs() < 1e-5);
        }
    }

    #[test]
    fn gamut_test_and_mapping_keep_lightness_and_hue() {
        assert!(Oklch::from_srgb(Srgb::new(1.0, 0.0, 0.0)).in_srgb_gamut());
        let wild = Oklch {
            l: 0.7,
            c: 0.35,
            h: 150.0,
        };
        assert!(!wild.in_srgb_gamut());
        let mapped = wild.to_srgb_mapped();
        let back = Oklch::from_srgb(mapped);
        assert!((back.l - wild.l).abs() < 0.01, "{back:?}");
        assert!((back.h - wild.h).abs() < 1.0, "{back:?}");
        assert!(back.c < wild.c && back.c > 0.1);
        // In-gamut colours are untouched.
        let ok = Oklch {
            l: 0.6,
            c: 0.05,
            h: 30.0,
        };
        assert!(ok.in_srgb_gamut());
        let b = Oklch::from_srgb(ok.to_srgb_mapped());
        assert!((b.c - ok.c).abs() < 1e-3 && (b.h - ok.h).abs() < 0.5);
    }

    #[test]
    fn delta_e_is_a_metric_on_samples() {
        let (a, b) = (Srgb::BLACK, Srgb::WHITE);
        assert!((delta_e_ok(a, b) - 1.0).abs() < 1e-3);
        assert_eq!(delta_e_ok(a, a), 0.0);
        assert_eq!(delta_e_ok(a, b), delta_e_ok(b, a));
    }
}
