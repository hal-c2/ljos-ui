//! The roles of `Ljos.Controls`' own style, derived from three colours.
//!
//! [`crate::palette`] grows a whole palette from a hue; a control style
//! needs fewer colours, and an app that brings its own ground, text and
//! accent (from a brand, or from `Ljos.Design`'s palette) should get every
//! other colour from those three and know each one is legible. So each
//! role is a mix of the three, and each role something is read on is then
//! moved toward the text colour, a step at a time, until it meets its WCAG
//! ratio against every fill it is drawn on ([`reach`]).
//!
//! The style's `Theme` singleton (`crates/ljos_controls/qml/Ljos/Controls/
//! Theme.qml`) does the same arithmetic in a binding, so a theme set while
//! the app runs re-derives; a test in `ljos_controls` holds the two to the
//! same colours.
//!
//! DECISION: a fill is lifted toward the text colour in both schemes (a
//! darker grey in light, a lighter one in dark). One rule for both keeps
//! the derivation three inputs and no scheme flag; `Ljos.Design`'s palette,
//! whose raised light surface is white, is the place for a look that
//! differs by scheme.

use crate::color::{Srgb, wcag_contrast};
use crate::limits::{NONTEXT_CONTRAST, TEXT_CONTRAST};
use crate::palette::Palette;

/// How many steps [`reach`] takes from its start to its end.
pub const STEPS: u32 = 40;

/// The colours a style is derived from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Base {
    /// The window's ground: `canvas`.
    pub ground: Srgb,
    pub text: Srgb,
    pub accent: Srgb,
}

impl Base {
    /// `Ljos.Design`'s canvas, text and accent for one palette.
    pub fn from_palette(p: &Palette) -> Option<Base> {
        let get = |n: &str| p.role(n).and_then(|c| Srgb::parse(c).ok());
        Some(Base {
            ground: get("canvas")?,
            text: get("text")?,
            accent: get("accent")?,
        })
    }
}

/// Where the error and warning colours start before they are made
/// legible: a red and an amber that read as such on light and dark.
pub const ERROR_SEED: Srgb = Srgb::new(229.0 / 255.0, 72.0 / 255.0, 77.0 / 255.0);
pub const WARNING_SEED: Srgb = Srgb::new(217.0 / 255.0, 119.0 / 255.0, 6.0 / 255.0);

/// The roles, by the names the `Theme` singleton has.
pub const NAMES: [&str; 16] = [
    "canvas",
    "surface",
    "surfaceRaised",
    "surfaceOverlay",
    "input",
    "border",
    "text",
    "textMuted",
    "placeholder",
    "iconMuted",
    "accent",
    "accentForeground",
    "accentSurface",
    "focus",
    "errorForeground",
    "warningForeground",
];

/// Every role's colour. Each field may be set after [`derive`], as an app
/// overrides one role in the singleton.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roles {
    pub canvas: Srgb,
    pub surface: Srgb,
    pub surface_raised: Srgb,
    pub surface_overlay: Srgb,
    pub input: Srgb,
    pub border: Srgb,
    pub text: Srgb,
    pub text_muted: Srgb,
    pub placeholder: Srgb,
    pub icon_muted: Srgb,
    pub accent: Srgb,
    pub accent_foreground: Srgb,
    pub accent_surface: Srgb,
    pub focus: Srgb,
    pub error_foreground: Srgb,
    pub warning_foreground: Srgb,
}

/// `a` moved `t` of the way to `b`, each channel rounded to 8 bits as the
/// colour is stored.
pub fn mix(a: Srgb, b: Srgb, t: f32) -> Srgb {
    let ch = |x: f32, y: f32| ((x + (y - x) * t) * 255.0).round().clamp(0.0, 255.0) / 255.0;
    Srgb::new(ch(a.r, b.r), ch(a.g, b.g), ch(a.b, b.b))
}

/// The lowest contrast `c` has against any of `against`.
pub fn least_contrast(c: Srgb, against: &[Srgb]) -> f32 {
    against
        .iter()
        .map(|&b| wcag_contrast(c, b))
        .fold(f32::INFINITY, f32::min)
}

/// The first colour from `from` toward `to`, in [`STEPS`] steps, that has
/// `ratio` against every colour in `against`; `to` when none does, which is
/// the best the inputs allow (and the text colour, where it is used).
pub fn reach(from: Srgb, to: Srgb, against: &[Srgb], ratio: f32) -> Srgb {
    (0..=STEPS)
        .map(|i| mix(from, to, i as f32 / STEPS as f32))
        .find(|&c| least_contrast(c, against) >= ratio)
        .unwrap_or(to)
}

/// Black or white, whichever has more contrast with `c`.
pub fn on(c: Srgb) -> Srgb {
    if wcag_contrast(Srgb::WHITE, c) >= wcag_contrast(Srgb::BLACK, c) {
        Srgb::WHITE
    } else {
        Srgb::BLACK
    }
}

/// Every role from the three colours.
pub fn derive(base: Base) -> Roles {
    let Base {
        ground: g,
        text: t,
        accent: a,
    } = base;
    let surface = mix(g, t, 0.04);
    let surface_raised = mix(g, t, 0.07);
    let surface_overlay = mix(g, t, 0.10);
    let border = mix(g, t, 0.16);
    let accent_surface = mix(surface_overlay, a, 0.18);
    // Muted text, icons and the semantic colours sit on any fill, a
    // highlighted menu row included; the most lifted one is the hardest.
    let fills = [g, surface, surface_raised, surface_overlay, accent_surface];
    let grounds = [g, surface, surface_raised, surface_overlay];
    Roles {
        canvas: g,
        surface,
        surface_raised,
        surface_overlay,
        input: reach(border, t, &[surface_raised], NONTEXT_CONTRAST),
        border,
        text: t,
        text_muted: reach(mix(g, t, 0.62), t, &fills, TEXT_CONTRAST),
        // Only in a field, and fields are raised.
        placeholder: reach(mix(g, t, 0.5), t, &[surface_raised], TEXT_CONTRAST),
        icon_muted: reach(mix(g, t, 0.45), t, &fills, NONTEXT_CONTRAST),
        accent: a,
        accent_foreground: on(a),
        accent_surface,
        // A ring is drawn around a control on any ground.
        focus: reach(a, t, &grounds, NONTEXT_CONTRAST),
        error_foreground: reach(ERROR_SEED, t, &fills, TEXT_CONTRAST),
        warning_foreground: reach(WARNING_SEED, t, &fills, TEXT_CONTRAST),
    }
}

/// A ratio a role is promised against the fills it is drawn on.
#[derive(Clone, Debug, PartialEq)]
pub struct Promise {
    pub role: &'static str,
    pub against: &'static [&'static str],
    pub ratio: f32,
}

const FILLS: &[&str] = &[
    "canvas",
    "surface",
    "surfaceRaised",
    "surfaceOverlay",
    "accentSurface",
];
const GROUNDS: &[&str] = &["canvas", "surface", "surfaceRaised", "surfaceOverlay"];

/// What [`derive`] promises. `text` on the fills holds when the text and
/// ground given have it; the rest hold for any three colours whose text
/// is legible on the ground.
pub const PROMISES: &[Promise] = &[
    Promise {
        role: "text",
        against: FILLS,
        ratio: TEXT_CONTRAST,
    },
    Promise {
        role: "input",
        against: &["surfaceRaised"],
        ratio: NONTEXT_CONTRAST,
    },
    Promise {
        role: "focus",
        against: GROUNDS,
        ratio: NONTEXT_CONTRAST,
    },
    Promise {
        role: "textMuted",
        against: FILLS,
        ratio: TEXT_CONTRAST,
    },
    Promise {
        role: "placeholder",
        against: &["surfaceRaised"],
        ratio: TEXT_CONTRAST,
    },
    Promise {
        role: "iconMuted",
        against: FILLS,
        ratio: NONTEXT_CONTRAST,
    },
    Promise {
        role: "errorForeground",
        against: FILLS,
        ratio: TEXT_CONTRAST,
    },
    Promise {
        role: "warningForeground",
        against: FILLS,
        ratio: TEXT_CONTRAST,
    },
    Promise {
        role: "accentForeground",
        against: &["accent"],
        ratio: TEXT_CONTRAST,
    },
];

impl Roles {
    /// A role by its `Theme` name.
    pub fn get(&self, name: &str) -> Option<Srgb> {
        Some(match name {
            "canvas" => self.canvas,
            "surface" => self.surface,
            "surfaceRaised" => self.surface_raised,
            "surfaceOverlay" => self.surface_overlay,
            "input" => self.input,
            "border" => self.border,
            "text" => self.text,
            "textMuted" => self.text_muted,
            "placeholder" => self.placeholder,
            "iconMuted" => self.icon_muted,
            "accent" => self.accent,
            "accentForeground" => self.accent_foreground,
            "accentSurface" => self.accent_surface,
            "focus" => self.focus,
            "errorForeground" => self.error_foreground,
            "warningForeground" => self.warning_foreground,
            _ => return None,
        })
    }

    /// The promises these colours break, each with the ratio it has.
    pub fn broken(&self) -> Vec<(&'static str, &'static str, f32)> {
        let mut out = Vec::new();
        for p in PROMISES {
            let Some(fg) = self.get(p.role) else { continue };
            for &bg in p.against {
                let Some(b) = self.get(bg) else { continue };
                let ratio = wcag_contrast(fg, b);
                if ratio < p.ratio {
                    out.push((p.role, bg, ratio));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::{Contrast, Scheme, Seed, palette};

    fn hex(s: &str) -> Srgb {
        Srgb::parse(s).unwrap()
    }

    #[test]
    fn every_name_is_a_role() {
        let r = derive(Base {
            ground: Srgb::WHITE,
            text: Srgb::BLACK,
            accent: hex("#0066cc"),
        });
        for n in NAMES {
            assert!(r.get(n).is_some(), "{n}");
        }
        assert_eq!(r.get("nothing"), None);
    }

    #[test]
    fn the_fills_are_mixes_of_ground_and_text() {
        let r = derive(Base {
            ground: Srgb::WHITE,
            text: Srgb::BLACK,
            accent: hex("#0066cc"),
        });
        assert_eq!(r.canvas.to_hex(), "#ffffff");
        assert_eq!(r.surface.to_hex(), "#f5f5f5");
        assert_eq!(r.surface_raised.to_hex(), "#ededed");
        assert_eq!(r.surface_overlay.to_hex(), "#e6e6e6");
        assert_eq!(r.accent_foreground, Srgb::WHITE);
    }

    #[test]
    fn reach_stops_at_the_first_colour_that_passes() {
        let bg = Srgb::WHITE;
        let c = reach(hex("#cccccc"), Srgb::BLACK, &[bg], 4.5);
        assert!(wcag_contrast(c, bg) >= 4.5);
        let before = mix(hex("#cccccc"), Srgb::BLACK, {
            let i = (0..=STEPS)
                .find(|&i| mix(hex("#cccccc"), Srgb::BLACK, i as f32 / STEPS as f32) == c)
                .unwrap();
            (i - 1) as f32 / STEPS as f32
        });
        assert!(wcag_contrast(before, bg) < 4.5);
        // Already legible: kept as it is.
        assert_eq!(reach(Srgb::BLACK, Srgb::WHITE, &[bg], 4.5), Srgb::BLACK);
    }

    /// Every palette of `Ljos.Design`, light and dark, gives three colours
    /// whose derived roles keep every promise.
    #[test]
    fn the_roles_of_every_palette_keep_their_ratios() {
        for hue in (0..360).step_by(30) {
            for scheme in Scheme::ALL {
                let p = palette(Seed::hue(hue as f32), scheme, Contrast::Normal);
                let roles = derive(Base::from_palette(&p).unwrap());
                let broken = roles.broken();
                assert!(broken.is_empty(), "hue {hue} {}: {broken:?}", scheme.name());
            }
        }
    }

    /// Three colours of an app's own, from near-black to near-white
    /// grounds with a legible text, keep the promises too.
    #[test]
    fn the_roles_of_any_legible_three_keep_their_ratios() {
        let grounds = [
            "#ffffff", "#f4f1ea", "#e8eef7", "#101014", "#1e1e2e", "#000000",
        ];
        let accents = [
            "#0066cc", "#ffcc00", "#e5484d", "#22c55e", "#7c3aed", "#888888",
        ];
        for g in grounds {
            let ground = hex(g);
            let text = on(ground);
            let text = mix(text, ground, 0.1);
            for a in accents {
                let roles = derive(Base {
                    ground,
                    text,
                    accent: hex(a),
                });
                let broken = roles.broken();
                assert!(broken.is_empty(), "{g} {a}: {broken:?}");
            }
        }
    }

    #[test]
    fn a_role_set_after_derive_is_checked_as_set() {
        let mut roles = derive(Base {
            ground: Srgb::WHITE,
            text: Srgb::BLACK,
            accent: hex("#0066cc"),
        });
        roles.placeholder = hex("#dddddd");
        let broken = roles.broken();
        assert_eq!(broken.len(), 1, "{broken:?}");
        assert_eq!((broken[0].0, broken[0].1), ("placeholder", "surfaceRaised"));
    }
}
