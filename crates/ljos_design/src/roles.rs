//! The roles of `Ljos.Controls`' own style, derived from three colours.
//!
//! [`crate::palette`] grows a whole palette from a hue; a control style
//! needs fewer colours, and an app that brings its own ground, text and
//! accent (from a brand, or from `Ljos.Design`'s palette) should get every
//! other colour from those three and know each one is legible. So each
//! role is a mix of the three, and each role something is read on is then
//! moved toward the text colour, a step at a time, until it meets its WCAG
//! ratio against every fill it is drawn on ([`reach`]). A fill that
//! something is drawn on in a state (a button hovered or pressed) is moved
//! away from what is drawn on it, so a state can only add contrast.
//! [`PROMISES`] lists every pair of roles the style draws one on the
//! other, and [`UNPROMISED`] the few it draws with no ratio and why; a
//! test holds every role to one of the two.
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
pub const NAMES: [&str; 23] = [
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
    "accentHover",
    "accentPressed",
    "focus",
    "link",
    "errorForeground",
    "warningForeground",
    "destructive",
    "destructiveForeground",
    "destructiveHover",
    "destructivePressed",
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
    pub accent_hover: Srgb,
    pub accent_pressed: Srgb,
    pub focus: Srgb,
    pub link: Srgb,
    pub error_foreground: Srgb,
    pub warning_foreground: Srgb,
    pub destructive: Srgb,
    pub destructive_foreground: Srgb,
    pub destructive_hover: Srgb,
    pub destructive_pressed: Srgb,
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

/// How far a hovered and a pressed fill move from their colour, away from
/// what is drawn on them.
pub const HOVER_SHADE: f32 = 0.08;
pub const PRESSED_SHADE: f32 = 0.16;

/// `fill` moved `t` of the way away from `ink`: toward black under a light
/// ink, toward white under a dark one. Every channel moves the same way, so
/// the luminance moves away from the ink's and the ratio only grows.
pub fn shade(fill: Srgb, ink: Srgb, t: f32) -> Srgb {
    mix(fill, on(ink), t)
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
    let accent_foreground = on(a);
    let error_foreground = reach(ERROR_SEED, t, &fills, TEXT_CONTRAST);
    // A destructive button is filled with the error colour; what is on it is
    // chosen against that fill, as the accent's is.
    let destructive_foreground = on(error_foreground);
    Roles {
        canvas: g,
        surface,
        surface_raised,
        surface_overlay,
        // The edge of a field and a button, a switch's track and a slider's
        // handle, on whatever ground the control is put.
        input: reach(border, t, &grounds, NONTEXT_CONTRAST),
        border,
        text: t,
        text_muted: reach(mix(g, t, 0.62), t, &fills, TEXT_CONTRAST),
        // Only in a field, and fields are raised.
        placeholder: reach(mix(g, t, 0.5), t, &[surface_raised], TEXT_CONTRAST),
        icon_muted: reach(mix(g, t, 0.45), t, &fills, NONTEXT_CONTRAST),
        accent: a,
        accent_foreground,
        accent_surface,
        accent_hover: shade(a, accent_foreground, HOVER_SHADE),
        accent_pressed: shade(a, accent_foreground, PRESSED_SHADE),
        // A ring around a control on any ground, and the check of a row
        // that is highlighted.
        focus: reach(a, t, &fills, NONTEXT_CONTRAST),
        // Text in a label or a pane, which are not highlighted.
        link: reach(a, t, &grounds, TEXT_CONTRAST),
        error_foreground,
        warning_foreground: reach(WARNING_SEED, t, &fills, TEXT_CONTRAST),
        destructive: error_foreground,
        destructive_foreground,
        destructive_hover: shade(error_foreground, destructive_foreground, HOVER_SHADE),
        destructive_pressed: shade(error_foreground, destructive_foreground, PRESSED_SHADE),
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

/// What [`derive`] promises: every pair of roles the style draws one on
/// the other. `text` on the fills holds when the text and ground given
/// have it; the rest hold for any three colours whose text is legible on
/// the ground.
pub const PROMISES: &[Promise] = &[
    Promise {
        role: "text",
        against: FILLS,
        ratio: TEXT_CONTRAST,
    },
    Promise {
        role: "input",
        against: GROUNDS,
        ratio: NONTEXT_CONTRAST,
    },
    Promise {
        role: "focus",
        against: FILLS,
        ratio: NONTEXT_CONTRAST,
    },
    Promise {
        role: "link",
        against: GROUNDS,
        ratio: TEXT_CONTRAST,
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
    // A filled button's text, icon and checked mark in each of its states.
    Promise {
        role: "accentForeground",
        against: &["accent", "accentHover", "accentPressed"],
        ratio: TEXT_CONTRAST,
    },
    Promise {
        role: "destructiveForeground",
        against: &["destructive", "destructiveHover", "destructivePressed"],
        ratio: TEXT_CONTRAST,
    },
];

/// The pairs the style draws with no ratio promised, and why none is
/// needed. Every role is in a promise or here.
pub const UNPROMISED: &[(&str, &str)] = &[
    (
        "border",
        "a separator and a popup's edge: a popup is set apart by its fill and shadow, and a separator is decoration",
    ),
    (
        "accent",
        "a checked switch's track and a slider's filled groove: the thumb's place shows the value, and the thumb has its own edge",
    ),
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
            "accentHover" => self.accent_hover,
            "accentPressed" => self.accent_pressed,
            "focus" => self.focus,
            "link" => self.link,
            "errorForeground" => self.error_foreground,
            "warningForeground" => self.warning_foreground,
            "destructive" => self.destructive,
            "destructiveForeground" => self.destructive_foreground,
            "destructiveHover" => self.destructive_hover,
            "destructivePressed" => self.destructive_pressed,
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

    /// Every role is drawn on or under another in a promise, or is listed
    /// as drawn with none, so a role added later cannot skip the check.
    #[test]
    fn every_role_is_promised_or_said_why_not() {
        for n in NAMES {
            let promised = PROMISES
                .iter()
                .any(|p| p.role == n || p.against.contains(&n));
            let unpromised = UNPROMISED.iter().any(|&(r, _)| r == n);
            assert!(promised || unpromised, "{n} is in no promise");
        }
        for p in PROMISES {
            assert!(NAMES.contains(&p.role), "{}", p.role);
            for b in p.against {
                assert!(NAMES.contains(b), "{b}");
            }
        }
    }

    /// The three colours of every palette of `Ljos.Design` (every hue,
    /// light and dark, normal and high contrast) and of apps' own (near-white
    /// to near-black grounds, accents from yellow to mid-grey) keep every
    /// promise: every pair the style draws, in every state of a fill.
    #[test]
    fn every_pair_the_style_draws_keeps_its_ratio() {
        let mut bases = Vec::new();
        for hue in (0..360).step_by(15) {
            for scheme in Scheme::ALL {
                for contrast in Contrast::ALL {
                    let p = palette(Seed::hue(hue as f32), scheme, contrast);
                    let name = format!("hue {hue} {} {contrast:?}", scheme.name());
                    bases.push((name, Base::from_palette(&p).unwrap()));
                }
            }
        }
        let grounds = [
            "#ffffff", "#f4f1ea", "#e8eef7", "#101014", "#1e1e2e", "#000000",
        ];
        let accents = [
            "#0066cc", "#ffcc00", "#e5484d", "#22c55e", "#7c3aed", "#777777", "#888888", "#ffffff",
            "#000000",
        ];
        for g in grounds {
            let ground = hex(g);
            let text = mix(on(ground), ground, 0.1);
            for a in accents {
                let accent = hex(a);
                bases.push((
                    format!("{g} {a}"),
                    Base {
                        ground,
                        text,
                        accent,
                    },
                ));
            }
        }
        for (name, base) in bases {
            let broken = derive(base).broken();
            assert!(broken.is_empty(), "{name}: {broken:?}");
        }
    }

    /// A state fill only gains contrast with what is drawn on it.
    #[test]
    fn a_shaded_fill_moves_away_from_its_ink() {
        for a in ["#777777", "#888888", "#0066cc", "#ffcc00", "#e5484d"] {
            let fill = hex(a);
            let ink = on(fill);
            let at_rest = wcag_contrast(ink, fill);
            let hover = wcag_contrast(ink, shade(fill, ink, HOVER_SHADE));
            let pressed = wcag_contrast(ink, shade(fill, ink, PRESSED_SHADE));
            assert!(
                at_rest <= hover && hover <= pressed,
                "{a}: {at_rest} {hover} {pressed}"
            );
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
