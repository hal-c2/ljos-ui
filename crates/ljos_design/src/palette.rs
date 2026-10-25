//! Palettes from one seed colour: twelve-step OKLCH ramps, the roles an
//! interface paints with, and the proof that every pair a user reads
//! passes [`limits`].
//!
//! The ramps follow the twelve-step layout Radix Colors documents (1–2
//! backgrounds, 3–5 component fills by state, 6–8 borders, 9–10 solid
//! fills, 11–12 text), built in OKLCH so a step is the same lightness in
//! every hue. Neutrals carry a trace of the seed's hue, as Material 3's
//! tonal palettes do, so greys sit with the accent instead of against it.
//! Danger, warning, success and info have hues of their own (about 25°,
//! 80°, 150° and 250°: red, amber, green, blue) that no seed moves, so a
//! red accent never makes an error look like a brand colour.
//!
//! The ramps are a starting point, not a guarantee: a fixed lightness
//! table cannot know that yellow at L 0.6 is paler to the eye's contrast
//! maths than blue at L 0.6. So every role that is read is generated, then
//! its lightness is moved (hue and chroma kept, chroma only lowered to fit
//! sRGB) until it passes against everything it sits on, in the 8-bit
//! colour that is written out. [`Palette::checks`] lists each pair.

use serde::Serialize;

use crate::color::{Oklch, Srgb, Srgba, apca_lc, wcag_contrast};
use crate::limits;

/// Light or dark, as `Application.styleHints.colorScheme` tells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Light,
    Dark,
}

/// Normal contrast, or the high contrast a user asks the platform for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Contrast {
    Normal,
    High,
}

impl Scheme {
    pub const ALL: [Scheme; 2] = [Scheme::Light, Scheme::Dark];
    pub fn name(self) -> &'static str {
        match self {
            Scheme::Light => "light",
            Scheme::Dark => "dark",
        }
    }
    pub fn parse(s: &str) -> Option<Scheme> {
        Scheme::ALL.into_iter().find(|v| v.name() == s)
    }
}

impl Contrast {
    pub const ALL: [Contrast; 2] = [Contrast::Normal, Contrast::High];
    pub fn name(self) -> &'static str {
        match self {
            Contrast::Normal => "normal",
            Contrast::High => "high",
        }
    }
    pub fn parse(s: &str) -> Option<Contrast> {
        Contrast::ALL.into_iter().find(|v| v.name() == s)
    }
    /// What text must reach: WCAG AA, or AAA's 7:1 (SC 1.4.6) when the
    /// user asked for more.
    pub fn text(self) -> f32 {
        match self {
            Contrast::Normal => limits::TEXT_CONTRAST,
            Contrast::High => HIGH_TEXT_CONTRAST,
        }
    }
}

/// WCAG 2.2 SC 1.4.6 (AAA), for the high-contrast variants.
pub const HIGH_TEXT_CONTRAST: f32 = 7.0;

/// The hue and colourfulness a palette grows from.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Seed {
    /// OKLCH hue in degrees.
    pub hue: f32,
    /// OKLCH chroma of the solid accent; 0 gives a grey accent.
    pub chroma: f32,
    /// The lightness of a seed colour, kept for the solid accent so a
    /// brand colour comes back as itself when it can.
    pub lightness: Option<f32>,
}

/// Chroma of an accent grown from a bare hue.
pub const DEFAULT_CHROMA: f32 = 0.14;
/// Neutrals take this share of the seed's chroma, up to [`NEUTRAL_TINT_MAX`].
const NEUTRAL_TINT: f32 = 0.08;
const NEUTRAL_TINT_MAX: f32 = 0.012;
/// Semantic fills are this colourful, whatever the seed.
const SEMANTIC_CHROMA: f32 = 0.15;

impl Seed {
    pub fn hue(hue: f32) -> Seed {
        Seed {
            hue: hue.rem_euclid(360.0),
            chroma: DEFAULT_CHROMA,
            lightness: None,
        }
    }

    pub fn color(c: Srgb) -> Seed {
        let o = Oklch::from_srgb(c);
        Seed {
            hue: o.h,
            chroma: o.c,
            lightness: Some(o.l.clamp(0.4, 0.8)),
        }
    }

    /// `#rrggbb`, a colour keyword, or a hue in degrees.
    pub fn parse(s: &str) -> Option<Seed> {
        if let Ok(h) = s.trim().trim_end_matches('°').parse::<f32>() {
            return h.is_finite().then(|| Seed::hue(h));
        }
        Srgb::parse(s).ok().map(Seed::color)
    }
}

/// The reserved semantic hues.
pub const SEMANTIC_HUES: [(&str, f32); 4] = [
    ("danger", 25.0),
    ("warning", 80.0),
    ("success", 150.0),
    ("info", 250.0),
];

/// Lightness of steps 1..=12.
const LIGHT_L: [f32; 12] = [
    0.993, 0.982, 0.962, 0.940, 0.915, 0.885, 0.840, 0.760, 0.600, 0.560, 0.490, 0.270,
];
const DARK_L: [f32; 12] = [
    0.155, 0.185, 0.225, 0.255, 0.285, 0.325, 0.385, 0.475, 0.600, 0.650, 0.790, 0.945,
];
/// Share of the ramp's chroma each step carries: pale backgrounds and
/// near-black text are nearly grey, the solid step is the full colour.
const CHROMA_SHARE: [f32; 12] = [
    0.04, 0.08, 0.16, 0.24, 0.32, 0.42, 0.52, 0.68, 1.0, 0.96, 0.86, 0.45,
];

/// Twelve steps of one hue.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Ramp {
    pub name: String,
    pub steps: Vec<String>,
}

fn ramp_colors(scheme: Scheme, hue: f32, chroma: f32, solid_l: Option<f32>) -> [Oklch; 12] {
    let ls = match scheme {
        Scheme::Light => LIGHT_L,
        Scheme::Dark => DARK_L,
    };
    std::array::from_fn(|i| {
        let l = match (i, solid_l) {
            (8, Some(l)) => l,
            (9, Some(l)) => l + (ls[9] - ls[8]),
            _ => ls[i],
        };
        Oklch {
            l,
            c: chroma * CHROMA_SHARE[i],
            h: hue,
        }
    })
}

/// The colour as it is written out, so a check measures what ships.
fn emit(c: Oklch) -> Srgb {
    let [r, g, b] = c.to_srgb_mapped().to_u8();
    Srgb::from_u8(r, g, b)
}

fn min_contrast(fg: Srgb, bgs: &[Srgb]) -> f32 {
    bgs.iter()
        .map(|&bg| wcag_contrast(fg, bg))
        .fold(f32::INFINITY, f32::min)
}

const L_STEP: f32 = 0.005;

/// Moves `c`'s lightness one way until it reaches `target` against every
/// colour in `bgs`; `None` if it runs out of lightness first.
fn reach(c: Oklch, bgs: &[Srgb], target: f32, darker: bool) -> Option<Oklch> {
    let mut c = c;
    loop {
        if min_contrast(emit(c), bgs) >= target {
            return Some(c);
        }
        if (darker && c.l <= 0.0) || (!darker && c.l >= 1.0) {
            return None;
        }
        c.l = if darker {
            (c.l - L_STEP).max(0.0)
        } else {
            (c.l + L_STEP).min(1.0)
        };
    }
}

/// The colour nearest `fg` in lightness, with its hue and as much of its
/// chroma as fits, that reaches `target` against every colour in `bgs`.
/// `fg` itself when it already passes; `None` when no lightness does.
pub fn nearest_passing(fg: Srgb, bgs: &[Srgb], target: f32) -> Option<Srgb> {
    if min_contrast(fg, bgs) >= target {
        return Some(fg);
    }
    let c = Oklch::from_srgb(fg);
    let options = [reach(c, bgs, target, true), reach(c, bgs, target, false)];
    options
        .into_iter()
        .flatten()
        .min_by(|a, b| (a.l - c.l).abs().total_cmp(&(b.l - c.l).abs()))
        .map(emit)
}

/// One colour an interface paints with.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Role {
    /// The property name in the QML module: `surfaceRaised`.
    pub name: &'static str,
    /// `#rrggbb`, or `#aarrggbb` for the translucent scrim and shadow.
    pub color: String,
}

/// A pair the palette promises.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Check {
    pub foreground: &'static str,
    pub background: &'static str,
    /// `text` or `nontext`.
    pub kind: &'static str,
    pub ratio: f32,
    pub required: f32,
    /// APCA Lc, as advice beside the ratio.
    pub apca: f32,
}

impl Check {
    pub fn passes(&self) -> bool {
        self.ratio >= self.required
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Palette {
    pub scheme: Scheme,
    pub contrast: Contrast,
    pub seed: Seed,
    pub ramps: Vec<Ramp>,
    pub roles: Vec<Role>,
    pub checks: Vec<Check>,
}

impl Palette {
    pub fn role(&self, name: &str) -> Option<&str> {
        self.roles
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.color.as_str())
    }
}

/// Every role, in the order the module lists them.
pub const ROLES: [&str; 27] = [
    "canvas",
    "surface",
    "surfaceRaised",
    "surfaceHover",
    "surfacePressed",
    "surfaceSelected",
    "border",
    "borderStrong",
    "focusRing",
    "accent",
    "accentHover",
    "accentPressed",
    "onAccent",
    "text",
    "textMuted",
    "textDisabled",
    "link",
    "danger",
    "onDanger",
    "warning",
    "onWarning",
    "success",
    "onSuccess",
    "info",
    "onInfo",
    "scrim",
    "shadow",
];

/// How far a hover and a pressed fill step from their rest colour.
const STATE_L: [f32; 2] = [0.05, 0.10];

struct Fill {
    rest: Oklch,
    on: Oklch,
}

/// How much further, in OKLCH lightness, a fill may move to keep white
/// text on it.
const WHITE_TEXT_BIAS: f32 = 0.1;

/// A solid fill and the text on it. The text is white or near-black; the
/// fill moves away from it until the pair passes, and must still stand
/// out from the surfaces it sits on (a checked box, a badge). White text
/// is the convention on a solid fill, so it wins unless near-black passes
/// by moving the fill [`WHITE_TEXT_BIAS`] less: amber and lime keep dark
/// text, blue does not, and a brand colour stays itself when it can.
fn fill(start: Oklch, target: f32, grounds: &[Srgb]) -> Fill {
    let light_on = Oklch {
        l: 1.0,
        c: 0.0,
        h: start.h,
    };
    let dark_on = Oklch {
        l: 0.2,
        c: start.c * 0.3,
        h: start.h,
    };
    let candidates = [light_on, dark_on].map(|on| {
        let darker = on.l > start.l;
        reach(start, &[emit(on)], target, darker)
            .filter(|rest| min_contrast(emit(*rest), grounds) >= limits::NONTEXT_CONTRAST)
            .map(|rest| Fill { rest, on })
    });
    let [a, b] = candidates;
    match (a, b) {
        (Some(a), Some(b))
            if (b.rest.l - start.l).abs() + WHITE_TEXT_BIAS < (a.rest.l - start.l).abs() =>
        {
            b
        }
        (Some(a), _) => a,
        (None, Some(b)) => b,
        (None, None) => Fill {
            rest: start,
            on: light_on,
        },
    }
}

fn shift(c: Oklch, by: f32, darker: bool) -> Oklch {
    Oklch {
        l: if darker { c.l - by } else { c.l + by }.clamp(0.0, 1.0),
        ..c
    }
}

/// The palette for one seed, scheme and contrast.
pub fn palette(seed: Seed, scheme: Scheme, contrast: Contrast) -> Palette {
    let light = scheme == Scheme::Light;
    let text_target = contrast.text();
    let nontext = limits::NONTEXT_CONTRAST;
    let tint = (seed.chroma * NEUTRAL_TINT).min(NEUTRAL_TINT_MAX);
    let n = ramp_colors(scheme, seed.hue, tint, None);
    let a = ramp_colors(scheme, seed.hue, seed.chroma, seed.lightness);
    let sem: Vec<[Oklch; 12]> = SEMANTIC_HUES
        .iter()
        .map(|&(_, h)| ramp_colors(scheme, h, SEMANTIC_CHROMA, None))
        .collect();

    let canvas = if light { n[1] } else { n[0] };
    let surface = if light { n[0] } else { n[1] };
    let raised = if light {
        Oklch { l: 1.0, ..n[0] }
    } else {
        n[2]
    };
    let (hover, pressed, selected) = (n[3], n[4], a[3]);
    let grounds = [canvas, surface, raised].map(emit);
    let all_grounds = [canvas, surface, raised, hover, pressed, selected].map(emit);

    // Text and marks move away from the surfaces: darker on light.
    let toward = light;
    let fix = |c: Oklch, bgs: &[Srgb], target: f32| reach(c, bgs, target, toward).unwrap_or(c);
    let border = fix(n[6], &grounds, nontext);
    let border_strong = fix(n[7], &grounds, limits::TEXT_CONTRAST);
    let focus = fix(a[8], &all_grounds, nontext);
    let text = fix(n[11], &all_grounds, text_target);
    let muted_start = if contrast == Contrast::High {
        n[11]
    } else {
        n[10]
    };
    let muted = fix(muted_start, &all_grounds, text_target);
    let link = fix(a[10], &grounds, text_target);

    let accent = fill(a[8], text_target, &grounds);
    let away = accent.on.l > accent.rest.l;
    let accent_states = STATE_L.map(|d| shift(accent.rest, d, away));
    let accent_states =
        accent_states.map(|c| reach(c, &[emit(accent.on)], text_target, away).unwrap_or(c));
    let semantic: Vec<Fill> = sem
        .iter()
        .map(|r| fill(r[8], text_target, &grounds))
        .collect();

    let black = |a: f32| Srgba::new(0.0, 0.0, 0.0, a).to_hex();
    let (scrim, shadow) = if light {
        (black(0.32), black(0.16))
    } else {
        (black(0.6), black(0.5))
    };
    let hex = |c: Oklch| emit(c).to_hex();
    let mut colors: Vec<(&'static str, String)> = vec![
        ("canvas", hex(canvas)),
        ("surface", hex(surface)),
        ("surfaceRaised", hex(raised)),
        ("surfaceHover", hex(hover)),
        ("surfacePressed", hex(pressed)),
        ("surfaceSelected", hex(selected)),
        ("border", hex(border)),
        ("borderStrong", hex(border_strong)),
        ("focusRing", hex(focus)),
        ("accent", hex(accent.rest)),
        ("accentHover", hex(accent_states[0])),
        ("accentPressed", hex(accent_states[1])),
        ("onAccent", hex(accent.on)),
        ("text", hex(text)),
        ("textMuted", hex(muted)),
        ("textDisabled", hex(n[8])),
        ("link", hex(link)),
    ];
    for (i, &(name, _)) in SEMANTIC_HUES.iter().enumerate() {
        colors.push((name, hex(semantic[i].rest)));
        colors.push((ON_NAMES[i], hex(semantic[i].on)));
    }
    colors.push(("scrim", scrim));
    colors.push(("shadow", shadow));
    let roles: Vec<Role> = ROLES
        .iter()
        .map(|&name| Role {
            name,
            color: colors
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, c)| c.clone())
                .unwrap_or_default(),
        })
        .collect();

    let mut ramps = vec![ramp("neutral", &n), ramp("accent", &a)];
    for (i, &(name, _)) in SEMANTIC_HUES.iter().enumerate() {
        ramps.push(ramp(name, &sem[i]));
    }
    let mut p = Palette {
        scheme,
        contrast,
        seed,
        ramps,
        roles,
        checks: Vec::new(),
    };
    p.checks = checks(&p, text_target);
    p
}

const ON_NAMES: [&str; 4] = ["onDanger", "onWarning", "onSuccess", "onInfo"];

fn ramp(name: &str, steps: &[Oklch; 12]) -> Ramp {
    Ramp {
        name: name.to_owned(),
        steps: steps.iter().map(|&c| emit(c).to_hex()).collect(),
    }
}

const GROUNDS: [&str; 3] = ["canvas", "surface", "surfaceRaised"];
const ALL_GROUNDS: [&str; 6] = [
    "canvas",
    "surface",
    "surfaceRaised",
    "surfaceHover",
    "surfacePressed",
    "surfaceSelected",
];

/// The pairs a palette promises: (foreground, backgrounds, text or not,
/// and for non-text whether the stronger text limit applies).
fn pairs() -> Vec<(&'static str, &'static [&'static str], &'static str)> {
    vec![
        ("text", &ALL_GROUNDS, "text"),
        ("textMuted", &ALL_GROUNDS, "text"),
        ("link", &GROUNDS, "text"),
        (
            "onAccent",
            &["accent", "accentHover", "accentPressed"],
            "text",
        ),
        ("onDanger", &["danger"], "text"),
        ("onWarning", &["warning"], "text"),
        ("onSuccess", &["success"], "text"),
        ("onInfo", &["info"], "text"),
        ("border", &GROUNDS, "nontext"),
        ("borderStrong", &GROUNDS, "nontext"),
        ("focusRing", &ALL_GROUNDS, "nontext"),
        ("accent", &GROUNDS, "nontext"),
        ("danger", &GROUNDS, "nontext"),
        ("warning", &GROUNDS, "nontext"),
        ("success", &GROUNDS, "nontext"),
        ("info", &GROUNDS, "nontext"),
    ]
}

fn checks(p: &Palette, text_target: f32) -> Vec<Check> {
    let color = |name: &str| Srgb::parse(p.role(name).unwrap_or("#000")).unwrap_or(Srgb::BLACK);
    let mut out = Vec::new();
    for (fg, bgs, kind) in pairs() {
        for &bg in bgs {
            let (f, b) = (color(fg), color(bg));
            let required = match (kind, fg) {
                ("text", _) => text_target,
                (_, "borderStrong") => limits::TEXT_CONTRAST,
                _ => limits::NONTEXT_CONTRAST,
            };
            out.push(Check {
                foreground: fg,
                background: bg,
                kind,
                ratio: round2(wcag_contrast(f, b)),
                required,
                apca: round2(apca_lc(f, b)),
            });
        }
    }
    out
}

/// Rounded down, so a reported ratio never claims more than it has.
fn round2(v: f32) -> f32 {
    (v * 100.0).floor() / 100.0
}

/// A harmony: hues at fixed offsets from the seed's, at the seed's
/// lightness and chroma, so the colours differ only in hue.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Harmony {
    pub name: &'static str,
    pub colors: Vec<String>,
}

pub const HARMONIES: [(&str, &[f32]); 4] = [
    ("analogous", &[-30.0, 0.0, 30.0]),
    ("complementary", &[0.0, 180.0]),
    ("splitComplementary", &[0.0, 150.0, 210.0]),
    ("triadic", &[0.0, 120.0, 240.0]),
];

pub fn harmonies(seed: Seed) -> Vec<Harmony> {
    let l = seed.lightness.unwrap_or(LIGHT_L[8]);
    HARMONIES
        .iter()
        .map(|&(name, offsets)| Harmony {
            name,
            colors: offsets
                .iter()
                .map(|o| {
                    emit(Oklch {
                        l,
                        c: seed.chroma,
                        h: (seed.hue + o).rem_euclid(360.0),
                    })
                    .to_hex()
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failures(p: &Palette) -> Vec<String> {
        p.checks
            .iter()
            .filter(|c| !c.passes())
            .map(|c| {
                format!(
                    "{}/{} {} on {}: {} < {}",
                    p.scheme.name(),
                    p.contrast.name(),
                    c.foreground,
                    c.background,
                    c.ratio,
                    c.required
                )
            })
            .collect()
    }

    #[test]
    fn every_hue_and_chroma_passes_in_all_four_variants() {
        let mut bad = Vec::new();
        for hue in (0..360).step_by(5) {
            for chroma in [0.0, 0.05, 0.1, 0.15, 0.2] {
                let seed = Seed {
                    hue: hue as f32,
                    chroma,
                    lightness: None,
                };
                for scheme in Scheme::ALL {
                    for contrast in Contrast::ALL {
                        let p = palette(seed, scheme, contrast);
                        bad.extend(
                            failures(&p)
                                .into_iter()
                                .map(|f| format!("{hue}° c{chroma} {f}")),
                        );
                    }
                }
            }
        }
        assert!(
            bad.is_empty(),
            "{} failures:\n{}",
            bad.len(),
            bad[..bad.len().min(30)].join("\n")
        );
    }

    #[test]
    fn brand_colours_pass_too() {
        for hex in [
            "#ffcc00", "#00ff00", "#0000ff", "#ff0000", "#7f7f7f", "#000000", "#ffffff",
        ] {
            let seed = Seed::parse(hex).unwrap();
            for scheme in Scheme::ALL {
                for contrast in Contrast::ALL {
                    let f = failures(&palette(seed, scheme, contrast));
                    assert!(f.is_empty(), "{hex}: {f:?}");
                }
            }
        }
    }

    #[test]
    fn a_seed_that_already_passes_comes_back_as_itself() {
        let p = palette(
            Seed::parse("#1d4ed8").unwrap(),
            Scheme::Light,
            Contrast::Normal,
        );
        assert_eq!(p.role("accent"), Some("#1d4ed8"));
        assert_eq!(p.role("onAccent"), Some("#ffffff"));
    }

    #[test]
    fn every_role_is_there_and_ramps_run_one_way() {
        let p = palette(Seed::hue(250.0), Scheme::Dark, Contrast::Normal);
        assert_eq!(p.roles.len(), ROLES.len());
        assert!(
            p.roles.iter().all(|r| r.color.starts_with('#')),
            "{:?}",
            p.roles
        );
        assert_eq!(p.role("scrim"), Some("#99000000"));
        for scheme in Scheme::ALL {
            let p = palette(Seed::hue(250.0), scheme, Contrast::Normal);
            for r in &p.ramps {
                let ls: Vec<f32> = r
                    .steps
                    .iter()
                    .map(|h| Oklch::from_srgb(Srgb::parse(h).unwrap()).l)
                    .collect();
                let rising = scheme == Scheme::Dark;
                assert!(
                    ls.windows(2).all(|w| (w[1] > w[0]) == rising),
                    "{} {}: {ls:?}",
                    scheme.name(),
                    r.name
                );
            }
        }
    }

    #[test]
    fn semantic_hues_stay_put_whatever_the_seed() {
        let red = palette(Seed::hue(25.0), Scheme::Light, Contrast::Normal);
        let blue = palette(Seed::hue(250.0), Scheme::Light, Contrast::Normal);
        assert_eq!(red.role("danger"), blue.role("danger"));
        let danger = Oklch::from_srgb(Srgb::parse(red.role("danger").unwrap()).unwrap());
        assert!((danger.h - 25.0).abs() < 5.0, "{danger:?}");
    }

    #[test]
    fn high_contrast_reaches_seven_to_one() {
        let p = palette(Seed::hue(140.0), Scheme::Light, Contrast::High);
        let text = p
            .checks
            .iter()
            .find(|c| c.foreground == "textMuted" && c.background == "surfacePressed")
            .unwrap();
        assert!(text.required == 7.0 && text.ratio >= 7.0, "{text:?}");
    }

    #[test]
    fn nearest_passing_keeps_the_hue_and_moves_the_least() {
        let grey = Srgb::parse("#999999").unwrap();
        let fixed = nearest_passing(grey, &[Srgb::WHITE], limits::TEXT_CONTRAST).unwrap();
        assert!(wcag_contrast(fixed, Srgb::WHITE) >= 4.5);
        assert!(
            wcag_contrast(fixed, Srgb::WHITE) < 4.7,
            "{}",
            fixed.to_hex()
        );
        let blue = Srgb::parse("#6699ff").unwrap();
        let fixed = nearest_passing(blue, &[Srgb::WHITE], 4.5).unwrap();
        let (h0, h1) = (Oklch::from_srgb(blue).h, Oklch::from_srgb(fixed).h);
        assert!((h0 - h1).abs() < 3.0, "{h0} {h1}");
        assert_eq!(
            nearest_passing(Srgb::BLACK, &[Srgb::WHITE], 4.5),
            Some(Srgb::BLACK)
        );
        assert_eq!(nearest_passing(grey, &[Srgb::WHITE], 22.0), None);
    }

    #[test]
    fn harmonies_keep_lightness_and_turn_hue() {
        let h = harmonies(Seed::hue(30.0));
        let triadic = h.iter().find(|h| h.name == "triadic").unwrap();
        let ls: Vec<Oklch> = triadic
            .colors
            .iter()
            .map(|c| Oklch::from_srgb(Srgb::parse(c).unwrap()))
            .collect();
        assert!(ls.iter().all(|c| (c.l - ls[0].l).abs() < 0.01), "{ls:?}");
        assert!((ls[1].h - 150.0).abs() < 3.0, "{ls:?}");
        assert_eq!(h.len(), 4);
    }

    #[test]
    fn seeds_parse() {
        assert_eq!(Seed::parse("250").unwrap().hue, 250.0);
        assert_eq!(Seed::parse("-30").unwrap().hue, 330.0);
        assert!(Seed::parse("#1d4ed8").unwrap().lightness.is_some());
        assert!(Seed::parse("royalblue").is_some());
        assert!(Seed::parse("nope").is_none());
        assert!(Seed::parse("NaN").is_none());
    }
}
