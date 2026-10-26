//! The design tools: `ljos_design`'s rules, motion tokens, palettes and
//! contrast maths, for an agent that is choosing a colour or a duration
//! instead of guessing one. Every number comes from `ljos_design`, so an
//! answer here agrees with what `ljos check` and `ljos audit` hold an
//! interface to.

use ljos_design::{
    color::{Srgb, Srgba, apca_lc, is_large_text, over, wcag_contrast},
    limits,
    motion::{self, Frequency, Kind, Size},
    palette::{self, Contrast, Scheme, Seed},
};
use ljos_mcp_core::{CallCtx, SurfaceError, ToolHandler, typed};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::tools::{BUDGET, page};

pub(crate) const RULES: &str = "The design rules every check names: code, area, severity, whether the source or the running scene tells it, what is wrong, the fix and its guideline; and the limits they hold to. Filter by `code` or `area`; paginated.";
pub(crate) const MOTION: &str = "Motion tokens by kind (enter, exit, move, sheet-in, gesture…), size and how often it is met: duration within the limits, easing as cubic-bezier points, a spring for gestures, what reduced motion does, and the QML to paste. Paginated.";
pub(crate) const PALETTE: &str = "A palette from a seed (`#rrggbb`, a colour name or a hue): twelve-step OKLCH ramps, the roles (surface, text, accent, onAccent, danger…), every contrast pair they pass, and hue harmonies at the seed's lightness.";
pub(crate) const CONTRAST: &str = "WCAG 2.2 ratio and APCA Lc of a foreground on a background, through translucent layers and opacity as the scene blends them; when it fails, the nearest foreground of the same hue that passes.";

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RulesArgs {
    /// One rule, for example `motion-ease-in-enter`.
    code: Option<String>,
    /// `a11y`, `color`, `text`, `layout`, `focus`, `motion` or `content`.
    area: Option<String>,
    cursor: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct RuleOut {
    code: String,
    area: String,
    /// `error` fails `ljos check` and `ljos audit`; `warning` needs a
    /// reason to keep; `hint` is advice.
    severity: String,
    /// `static`, `runtime` or `both`.
    check: String,
    rule: String,
    fix: String,
    /// `docs/design/<page>#<code>`.
    guide: String,
}

#[derive(Serialize, JsonSchema)]
struct LimitOut {
    name: &'static str,
    value: f32,
    unit: &'static str,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct RulesOut {
    rules: Vec<RuleOut>,
    limits: Vec<LimitOut>,
    next_cursor: Option<String>,
}

/// A serde enum as its name.
/// An f32 as the decimal it was written as: `0.23`, not `0.23000000417232513`.
fn tidy(x: f32) -> f64 {
    (f64::from(x) * 1e4).round() / 1e4
}

fn name_of(v: impl Serialize) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn limit_table() -> Vec<LimitOut> {
    use limits::motion as m;
    let l = |name, value, unit| LimitOut { name, value, unit };
    vec![
        l("text_contrast", limits::TEXT_CONTRAST, "ratio"),
        l("large_text_contrast", limits::LARGE_TEXT_CONTRAST, "ratio"),
        l("high_text_contrast", palette::HIGH_TEXT_CONTRAST, "ratio"),
        l("nontext_contrast", limits::NONTEXT_CONTRAST, "ratio"),
        l("large_text_px", limits::LARGE_TEXT_PX, "px"),
        l("large_bold_text_px", limits::LARGE_BOLD_TEXT_PX, "px"),
        l("apca_body_lc", limits::APCA_BODY_LC, "Lc"),
        l("apca_large_lc", limits::APCA_LARGE_LC, "Lc"),
        l("apca_nontext_lc", limits::APCA_NONTEXT_LC, "Lc"),
        l("surface_min_alpha", limits::SURFACE_MIN_ALPHA, "alpha"),
        l("target_min_desktop", limits::TARGET_MIN_DESKTOP, "px"),
        l("target_min_touch", limits::TARGET_MIN_TOUCH, "px"),
        l("spacing_unit_px", limits::SPACING_UNIT_PX, "px"),
        l("motion_frequent_max", m::FREQUENT_MAX_MS as f32, "ms"),
        l("motion_micro_max", m::MICRO_MAX_MS as f32, "ms"),
        l("motion_standard_max", m::STANDARD_MAX_MS as f32, "ms"),
        l("motion_emphasis_max", m::EMPHASIS_MAX_MS as f32, "ms"),
        l("motion_min_visible", m::MIN_VISIBLE_MS as f32, "ms"),
        l("motion_enter_scale", m::ENTER_SCALE_RECOMMENDED, "scale"),
        l(
            "spring_min_damping_ratio",
            m::SPRING_MIN_DAMPING_RATIO,
            "ratio",
        ),
    ]
}

fn rules(a: RulesArgs) -> Result<RulesOut, SurfaceError> {
    let rules: Vec<RuleOut> = ljos_design::RULES
        .iter()
        .filter(|r| a.code.as_deref().is_none_or(|c| r.code == c))
        .filter(|r| a.area.as_deref().is_none_or(|x| name_of(r.area) == x))
        .map(|r| RuleOut {
            code: r.code.to_owned(),
            area: name_of(r.area),
            severity: name_of(r.severity),
            check: name_of(r.check),
            rule: r.rule.to_owned(),
            fix: r.fix.to_owned(),
            guide: r.guide(),
        })
        .collect();
    if rules.is_empty() {
        return Err(SurfaceError::not_found(format!(
            "no design rule matches{}{}",
            a.code.map(|c| format!(" code `{c}`")).unwrap_or_default(),
            a.area.map(|x| format!(" area `{x}`")).unwrap_or_default(),
        )));
    }
    let limits = limit_table();
    let budget = BUDGET - serde_json::to_vec(&limits).map_or(0, |b| b.len());
    let (rules, next_cursor) = page(rules, a.cursor.as_deref(), usize::MAX, budget)?;
    Ok(RulesOut {
        rules,
        limits,
        next_cursor,
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MotionArgs {
    /// `enter`, `exit`, `move`, `expand`, `collapse`, `sheet-in`,
    /// `sheet-out`, `gesture` or `loop`.
    kind: Option<String>,
    /// `small` (a button, a tooltip), `panel` (a menu, a dialog) or
    /// `screen` (a page, a drawer).
    size: Option<String>,
    /// `rare`, `occasional` (the default for most things) or `frequent`
    /// (many times a day: tabs, selection, focus).
    frequency: Option<String>,
    /// What the QML snippet animates; `opacity,scale` when not given.
    properties: Option<String>,
    cursor: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct CurveOut {
    name: &'static str,
    /// `x1, y1, x2, y2`.
    bezier: [f64; 4],
    /// For `easing.bezierCurve` with `easing.type: Easing.BezierSpline`.
    qml: String,
}

impl CurveOut {
    fn of(b: motion::Bezier) -> CurveOut {
        CurveOut {
            name: b.name,
            bezier: [b.x1, b.y1, b.x2, b.y2].map(tidy),
            qml: b.qml_list(),
        }
    }
}

#[derive(Serialize, JsonSchema)]
struct SpringOut {
    stiffness: f32,
    damping: f32,
    mass: f32,
    damping_ratio: f32,
    /// `SpringAnimation`'s `spring`, `damping`, `mass` and `epsilon` for
    /// a value in pixels.
    qml_spring: f32,
    qml_damping: f32,
    qml_mass: f32,
    qml_epsilon: f32,
}

#[derive(Serialize, JsonSchema)]
struct ReducedOut {
    /// 0: nothing plays, the end state shows at once.
    duration_ms: u32,
    property: &'static str,
    easing: &'static str,
}

#[derive(Serialize, JsonSchema)]
struct TokenOut {
    kind: &'static str,
    size: &'static str,
    frequency: &'static str,
    duration_ms: u32,
    /// What `motion-duration` allows here.
    limit_ms: u32,
    easing: &'static str,
    spring: Option<SpringOut>,
    reduced: ReducedOut,
    qml: String,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct MotionOut {
    tokens: Vec<TokenOut>,
    curves: Vec<CurveOut>,
    /// The scale an entrance starts from, with a fade.
    enter_scale: f32,
    next_cursor: Option<String>,
}

fn parsed<T>(
    what: &str,
    v: Option<&str>,
    parse: fn(&str) -> Option<T>,
    all: &str,
) -> Result<Option<T>, SurfaceError> {
    v.map(|s| {
        parse(s).ok_or_else(|| {
            SurfaceError::invalid_argument(format!("unknown {what} `{s}`: use one of {all}"))
        })
    })
    .transpose()
}

fn names<T: Copy>(all: &[T], name: fn(T) -> &'static str) -> String {
    all.iter().map(|&v| name(v)).collect::<Vec<_>>().join(", ")
}

fn motion_tokens(a: MotionArgs) -> Result<MotionOut, SurfaceError> {
    let kind = parsed(
        "kind",
        a.kind.as_deref(),
        Kind::parse,
        &names(&Kind::ALL, Kind::name),
    )?;
    let size = parsed(
        "size",
        a.size.as_deref(),
        Size::parse,
        &names(&Size::ALL, Size::name),
    )?;
    let frequency = parsed(
        "frequency",
        a.frequency.as_deref(),
        Frequency::parse,
        &names(&Frequency::ALL, Frequency::name),
    )?;
    let properties = a.properties.as_deref().unwrap_or("opacity,scale");
    let tokens: Vec<TokenOut> = motion::tokens()
        .into_iter()
        .filter(|t| kind.is_none_or(|k| t.kind == k))
        .filter(|t| size.is_none_or(|s| t.size == s))
        .filter(|t| frequency.is_none_or(|f| t.frequency == f))
        .map(|t| TokenOut {
            kind: t.kind.name(),
            size: t.size.name(),
            frequency: t.frequency.name(),
            duration_ms: t.duration_ms,
            limit_ms: motion::limit_ms(t.size, t.frequency),
            easing: t.easing.name,
            spring: t.spring.map(|s| {
                let q = s.qml(motion::SpringUnit::Pixels);
                SpringOut {
                    stiffness: s.stiffness,
                    damping: s.damping,
                    mass: s.mass,
                    damping_ratio: s.damping_ratio(),
                    qml_spring: q.spring,
                    qml_damping: q.damping,
                    qml_mass: q.mass,
                    qml_epsilon: q.epsilon,
                }
            }),
            reduced: ReducedOut {
                duration_ms: t.reduced.duration_ms,
                property: t.reduced.property,
                easing: t.reduced.easing.name,
            },
            qml: t.qml(properties),
        })
        .collect();
    let (tokens, next_cursor) = page(tokens, a.cursor.as_deref(), usize::MAX, BUDGET / 2)?;
    Ok(MotionOut {
        tokens,
        curves: motion::CURVES.into_iter().map(CurveOut::of).collect(),
        enter_scale: motion::ENTER_SCALE,
        next_cursor,
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PaletteArgs {
    /// `#rrggbb`, a colour name, or an OKLCH hue in degrees (`250`).
    seed: String,
    /// `light` (the default) or `dark`.
    scheme: Option<String>,
    /// `normal` (the default) or `high`: text at 7:1.
    contrast: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct RoleOut {
    name: &'static str,
    color: String,
}

#[derive(Serialize, JsonSchema)]
struct RampOut {
    name: String,
    /// Steps 1 to 12: backgrounds, fills by state, borders, solid, text.
    steps: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
struct PairOut {
    foreground: &'static str,
    background: &'static str,
    /// `text` or `nontext`.
    kind: &'static str,
    ratio: f32,
    required: f32,
    apca: f32,
}

#[derive(Serialize, JsonSchema)]
struct HarmonyOut {
    name: &'static str,
    colors: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct PaletteOut {
    scheme: &'static str,
    contrast: &'static str,
    seed_hue: f32,
    seed_chroma: f32,
    roles: Vec<RoleOut>,
    ramps: Vec<RampOut>,
    /// Every pair the roles promise, each passing.
    checks: Vec<PairOut>,
    /// Pairs below what they need; empty unless a seed could not be
    /// brought to pass.
    failing: usize,
    harmonies: Vec<HarmonyOut>,
}

fn design_palette(a: PaletteArgs) -> Result<PaletteOut, SurfaceError> {
    let seed = Seed::parse(&a.seed).ok_or_else(|| {
        SurfaceError::invalid_argument(format!(
            "`{}` is not a colour (`#rrggbb`, a name) or a hue in degrees",
            a.seed
        ))
    })?;
    let scheme = parsed("scheme", a.scheme.as_deref(), Scheme::parse, "light, dark")?
        .unwrap_or(Scheme::Light);
    let contrast = parsed(
        "contrast",
        a.contrast.as_deref(),
        Contrast::parse,
        "normal, high",
    )?
    .unwrap_or(Contrast::Normal);
    let p = palette::palette(seed, scheme, contrast);
    Ok(PaletteOut {
        scheme: scheme.name(),
        contrast: contrast.name(),
        seed_hue: (seed.hue * 10.0).round() / 10.0,
        seed_chroma: (seed.chroma * 1000.0).round() / 1000.0,
        failing: p.checks.iter().filter(|c| !c.passes()).count(),
        roles: p
            .roles
            .into_iter()
            .map(|r| RoleOut {
                name: r.name,
                color: r.color,
            })
            .collect(),
        ramps: p
            .ramps
            .into_iter()
            .map(|r| RampOut {
                name: r.name,
                steps: r.steps,
            })
            .collect(),
        checks: p
            .checks
            .into_iter()
            .map(|c| PairOut {
                foreground: c.foreground,
                background: c.background,
                kind: c.kind,
                ratio: c.ratio,
                required: c.required,
                apca: c.apca,
            })
            .collect(),
        harmonies: palette::harmonies(seed)
            .into_iter()
            .map(|h| HarmonyOut {
                name: h.name,
                colors: h.colors,
            })
            .collect(),
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContrastArgs {
    /// The text or mark: `#rrggbb`, `#aarrggbb` or a colour name.
    foreground: String,
    /// The opaque colour at the bottom: the window, a page.
    background: String,
    /// What lies between, bottom first: a card, a hover tint.
    #[serde(default)]
    layers: Vec<Layer>,
    /// The foreground item's `opacity` (and its parents'), 1 when not given.
    opacity: Option<f32>,
    /// `text` (the default) or `nontext` (borders, icons, focus rings).
    kind: Option<String>,
    /// Text size in logical pixels; large text needs less.
    pixel_size: Option<f32>,
    /// `font.weight` (400 when not given); bold text is large sooner.
    weight: Option<u16>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Layer {
    /// The layer's colour, translucent or not.
    color: String,
    /// The layer item's `opacity`, 1 when not given.
    opacity: Option<f32>,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct ContrastOut {
    /// What reaches the eye after blending.
    foreground: String,
    background: String,
    ratio: f64,
    required: f32,
    passes: bool,
    /// APCA Lc and what it advises; never failed on.
    apca: f32,
    apca_advised: f32,
    /// The rule a failure is reported under.
    rule: &'static str,
    /// The opaque colour of the foreground's hue, nearest in lightness,
    /// that passes here; absent when it passes already or nothing does.
    nearest_passing: Option<String>,
}

fn color(what: &str, s: &str) -> Result<Srgba, SurfaceError> {
    Srgba::parse(s).map_err(|e| SurfaceError::invalid_argument(format!("{what}: {e}")))
}

fn design_contrast(a: ContrastArgs) -> Result<ContrastOut, SurfaceError> {
    let bottom = color("background", &a.background)?;
    if bottom.a < 1.0 {
        return Err(SurfaceError::invalid_argument(
            "background must be opaque; put translucent colours in `layers`",
        ));
    }
    let mut ground = bottom.rgb();
    for (i, layer) in a.layers.iter().enumerate() {
        let c = color(&format!("layers[{i}]"), &layer.color)?;
        ground = over(
            ljos_design::color::opacity(c, layer.opacity.unwrap_or(1.0)),
            ground,
        );
    }
    let fg = color("foreground", &a.foreground)?;
    let seen = over(
        ljos_design::color::opacity(fg, a.opacity.unwrap_or(1.0)),
        ground,
    );
    let text = match a.kind.as_deref() {
        None | Some("text") => true,
        Some("nontext") => false,
        Some(k) => {
            return Err(SurfaceError::invalid_argument(format!(
                "unknown kind `{k}`: use text or nontext"
            )));
        }
    };
    let large = text && is_large_text(a.pixel_size.unwrap_or(0.0), a.weight.unwrap_or(400));
    let (required, apca_advised, rule) = match (text, large) {
        (false, _) => (
            limits::NONTEXT_CONTRAST,
            limits::APCA_NONTEXT_LC,
            "color-nontext-contrast",
        ),
        (true, true) => (
            limits::LARGE_TEXT_CONTRAST,
            limits::APCA_LARGE_LC,
            "color-text-contrast",
        ),
        (true, false) => (
            limits::TEXT_CONTRAST,
            limits::APCA_BODY_LC,
            "color-text-contrast",
        ),
    };
    let ratio = wcag_contrast(seen, ground);
    let passes = ratio >= required;
    let nearest_passing = (!passes)
        .then(|| palette::nearest_passing(fg.rgb(), &[ground], required))
        .flatten()
        .map(Srgb::to_hex);
    Ok(ContrastOut {
        foreground: seen.to_hex(),
        background: ground.to_hex(),
        // Rounded down, so a ratio never claims a pass it does not have;
        // the f32 error is taken off first, or black on white says 20.99.
        ratio: (f64::from(ratio) * 100.0 + 1e-3).floor() / 100.0,
        required,
        passes,
        apca: (apca_lc(seen, ground) * 10.0).round() / 10.0,
        apca_advised,
        rule,
        nearest_passing,
    })
}

pub(crate) fn rules_handler() -> ToolHandler {
    typed(|_: &CallCtx, a: RulesArgs| rules(a))
}

pub(crate) fn motion_handler() -> ToolHandler {
    typed(|_: &CallCtx, a: MotionArgs| motion_tokens(a))
}

pub(crate) fn palette_handler() -> ToolHandler {
    typed(|_: &CallCtx, a: PaletteArgs| design_palette(a))
}

pub(crate) fn contrast_handler() -> ToolHandler {
    typed(|_: &CallCtx, a: ContrastArgs| design_contrast(a))
}
