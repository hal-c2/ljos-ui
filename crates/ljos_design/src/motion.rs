//! The motion vocabulary as data: what kind of change something makes, how
//! large the thing is and how often a user meets it decide how long it
//! takes and which curve it follows. The static lints, the audit, the MCP
//! tools and the `Ljos.Design` QML module all read these tokens, so a
//! suggestion and a check never disagree.
//!
//! Sources, paraphrased:
//!
//! - animations.dev (Emil Kowalski), the easing blueprint: ease-out for
//!   anything that enters or answers the user, because it starts fast and
//!   so feels immediate; ease-in-out for something that moves from one
//!   place on screen to another; linear only for constant motion such as a
//!   spinner or a progress bar; and the frequency rule: the more often an
//!   animation is seen, the shorter it is, down to none at all.
//! - Material 3, "Easing and duration" (m3.material.io/styles/motion): the
//!   standard curve, durations by the size of the area that changes, and
//!   exits shorter than entrances.
//! - Apple Human Interface Guidelines, "Motion": brief, purposeful motion,
//!   springs for what follows a gesture, and a calm replacement when the
//!   user asks for reduced motion.
//! - web.dev, "prefers-reduced-motion": reduced motion means no movement,
//!   not no feedback, so a short cross-fade stays.
//!
//! The cubic Bézier curves below are the commonly published control points
//! of the named families (the "quint" and "quart" Penner curves, Material
//! 3's standard curve, the iOS sheet curve as web write-ups measure it).

use serde::Serialize;

use crate::limits::motion as lim;

/// What a change does on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// Something appears: a popup, a menu, a toast, a row.
    Enter,
    /// Something goes away. Always shorter than its entrance.
    Exit,
    /// Something already on screen moves to another place.
    Move,
    /// Something opens up in place: a section, a row's details.
    Expand,
    /// The reverse of [`Kind::Expand`].
    Collapse,
    /// A drawer or a sheet slides in from an edge.
    SheetIn,
    /// A drawer or a sheet slides back out.
    SheetOut,
    /// Motion that follows a finger and may be taken over at any moment;
    /// a spring, so a change of target keeps the speed it has.
    Gesture,
    /// Constant motion that repeats or measures progress: a spinner, an
    /// indeterminate bar. Its duration is one cycle.
    Loop,
}

impl Kind {
    pub const ALL: [Kind; 9] = [
        Kind::Enter,
        Kind::Exit,
        Kind::Move,
        Kind::Expand,
        Kind::Collapse,
        Kind::SheetIn,
        Kind::SheetOut,
        Kind::Gesture,
        Kind::Loop,
    ];

    /// The name the MCP tools and the QML module use.
    pub const fn name(self) -> &'static str {
        match self {
            Kind::Enter => "enter",
            Kind::Exit => "exit",
            Kind::Move => "move",
            Kind::Expand => "expand",
            Kind::Collapse => "collapse",
            Kind::SheetIn => "sheet-in",
            Kind::SheetOut => "sheet-out",
            Kind::Gesture => "gesture",
            Kind::Loop => "loop",
        }
    }

    pub fn parse(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// The way back of an entrance: what has to be the shorter of the two.
    pub const fn reverse(self) -> Option<Kind> {
        match self {
            Kind::Enter => Some(Kind::Exit),
            Kind::Expand => Some(Kind::Collapse),
            Kind::SheetIn => Some(Kind::SheetOut),
            _ => None,
        }
    }

    /// The entrance a way back reverses.
    pub const fn way_in(self) -> Option<Kind> {
        match self {
            Kind::Exit => Some(Kind::Enter),
            Kind::Collapse => Some(Kind::Expand),
            Kind::SheetOut => Some(Kind::SheetIn),
            _ => None,
        }
    }

    /// Whether something leaves (or closes) with this kind.
    pub const fn leaves(self) -> bool {
        matches!(self, Kind::Exit | Kind::Collapse | Kind::SheetOut)
    }
}

/// How much of the window changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Size {
    /// A control or its feedback: a check, a tooltip, a dropdown.
    Small,
    /// A panel, a dialog, a drawer, a card.
    Panel,
    /// The whole window: a page change.
    Screen,
}

impl Size {
    pub const ALL: [Size; 3] = [Size::Small, Size::Panel, Size::Screen];

    pub const fn name(self) -> &'static str {
        match self {
            Size::Small => "small",
            Size::Panel => "panel",
            Size::Screen => "screen",
        }
    }

    pub fn parse(name: &str) -> Option<Size> {
        Size::ALL.into_iter().find(|s| s.name() == name)
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// How often a user meets the motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Frequency {
    /// Rare moments that may be watched: onboarding, a first launch.
    Rare,
    /// What most interface motion is: a dialog, a menu, a page.
    Occasional,
    /// Done many times a day: selection, a tab switch, focus, a shortcut.
    Frequent,
}

impl Frequency {
    pub const ALL: [Frequency; 3] = [Frequency::Rare, Frequency::Occasional, Frequency::Frequent];

    pub const fn name(self) -> &'static str {
        match self {
            Frequency::Rare => "rare",
            Frequency::Occasional => "occasional",
            Frequency::Frequent => "frequent",
        }
    }

    pub fn parse(name: &str) -> Option<Frequency> {
        Frequency::ALL.into_iter().find(|f| f.name() == name)
    }
}

/// A cubic Bézier easing curve from (0, 0) to (1, 1), as CSS writes it and
/// as QML takes it in `easing.bezierCurve`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Bezier {
    /// What the module names it: `Motion.easeOut`.
    pub name: &'static str,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl Bezier {
    const fn new(name: &'static str, x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Bezier {
            name,
            x1,
            y1,
            x2,
            y2,
        }
    }

    /// `easing.bezierCurve` for `easing.type: Easing.BezierSpline`: the two
    /// control points and the end point of the one segment.
    pub fn qml_curve(&self) -> [f32; 6] {
        [self.x1, self.y1, self.x2, self.y2, 1.0, 1.0]
    }

    /// `[0.23, 1, 0.32, 1, 1, 1]`, as QML is written.
    pub fn qml_list(&self) -> String {
        let parts: Vec<String> = self.qml_curve().iter().map(|v| number(*v)).collect();
        format!("[{}]", parts.join(", "))
    }

    /// The slope the curve leaves (0, 0) with. Above 1 it starts faster
    /// than linear (ease-out), below 1 slower (ease-in). A curve whose
    /// first control point is at x = 0 leaves straight up.
    pub fn start_slope(&self) -> f32 {
        if self.x1 <= 0.0 {
            f32::INFINITY
        } else {
            self.y1 / self.x1
        }
    }

    /// The slope it arrives at (1, 1) with: below 1 it settles (ease-out),
    /// above 1 it is still speeding up when it stops (ease-in).
    pub fn end_slope(&self) -> f32 {
        let dx = 1.0 - self.x2;
        if dx <= 0.0 {
            f32::INFINITY
        } else {
            (1.0 - self.y2) / dx
        }
    }

    /// Whether the curve starts slow: what an entrance must not do
    /// (`motion-ease-in-enter`).
    pub fn starts_slow(&self) -> bool {
        self.start_slope() < 1.0
    }

    /// Whether it goes outside 0..1 on the way: an overshoot or a dip
    /// (`motion-bounce`).
    pub fn overshoots(&self) -> bool {
        !(0.0..=1.0).contains(&self.y1) || !(0.0..=1.0).contains(&self.y2)
    }

    /// The eased progress at `t` in 0..1.
    pub fn at(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        // Solve x(s) = t for the curve parameter s by bisection: x is
        // monotonic for control points in 0..1, which all of these have.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..40 {
            let mid = (lo + hi) / 2.0;
            if cubic(self.x1, self.x2, mid) < t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        cubic(self.y1, self.y2, (lo + hi) / 2.0)
    }
}

fn cubic(p1: f32, p2: f32, s: f32) -> f32 {
    let u = 1.0 - s;
    3.0 * u * u * s * p1 + 3.0 * u * s * s * p2 + s * s * s
}

/// Fast start, long settle: entrances and answers to the user. The
/// "out-quint"-like curve animations.dev recommends for interface motion.
pub const EASE_OUT: Bezier = Bezier::new("easeOut", 0.23, 1.0, 0.32, 1.0);
/// A softer ease-out ("out-quart") for exits: still quick to react, and
/// done before the user waits on it. DECISION: exits ease out too; an exit
/// is also an answer to the user, and an ease-in exit feels late.
pub const EASE_OUT_SOFT: Bezier = Bezier::new("easeOutSoft", 0.165, 0.84, 0.44, 1.0);
/// Speeds up and slows down: something that moves from one place on the
/// screen to another ("in-out-quart").
pub const EASE_IN_OUT: Bezier = Bezier::new("easeInOut", 0.77, 0.0, 0.175, 1.0);
/// Material 3's emphasized-decelerate curve: a quick start that settles
/// gently, for content that opens or closes in place. (Its plain standard
/// curve leaves at a slope of 0, which reads as a late start.)
pub const DECELERATE: Bezier = Bezier::new("decelerate", 0.05, 0.7, 0.1, 1.0);
/// The curve of a sheet or a drawer: a near-instant start that glides in.
pub const DRAWER: Bezier = Bezier::new("drawer", 0.32, 0.72, 0.0, 1.0);
/// Constant speed: loops and progress only.
pub const LINEAR: Bezier = Bezier::new("linear", 0.0, 0.0, 1.0, 1.0);

/// Every named curve, in the order the module lists them.
pub const CURVES: [Bezier; 6] = [
    EASE_OUT,
    EASE_OUT_SOFT,
    EASE_IN_OUT,
    DECELERATE,
    DRAWER,
    LINEAR,
];

/// One step of QML's `SpringAnimation`, in seconds. Measured on qml6: the
/// property moves in steps of 16 milliseconds, and each step changes its
/// speed (units per second) by `(spring * distance - damping * speed) /
/// mass` (see `ljos_quick`'s spring, which is held to recorded qml6 runs).
pub const QML_SPRING_STEP_S: f32 = 0.016;

/// A damped spring as physics writes it: `mass * a = -stiffness * x -
/// damping * v`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Spring {
    /// Newtons per unit: how hard it pulls.
    pub stiffness: f32,
    /// How much speed it loses: `c` in `-c * v`.
    pub damping: f32,
    pub mass: f32,
}

/// What a value of `SpringAnimation` is measured in; it decides `epsilon`,
/// the distance and speed below which the spring is at rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpringUnit {
    /// `x`, `y`, `width`: a quarter of a pixel is invisible.
    Pixels,
    /// `opacity`, `scale`: 0..1.
    Fraction,
    /// `rotation`.
    Degrees,
}

impl SpringUnit {
    pub const fn epsilon(self) -> f32 {
        match self {
            SpringUnit::Pixels => 0.25,
            SpringUnit::Fraction => 0.002,
            SpringUnit::Degrees => 0.1,
        }
    }
}

/// The spring as `SpringAnimation`'s properties.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct QmlSpring {
    pub spring: f32,
    pub damping: f32,
    pub mass: f32,
    pub epsilon: f32,
}

impl Spring {
    /// The spring that answers in `response` seconds (the period it would
    /// swing with undamped) at `ratio` of critical damping, for a mass of
    /// 1: how Apple's guidelines and SwiftUI describe a spring.
    pub fn from_response(response: f32, ratio: f32) -> Spring {
        let omega = std::f32::consts::TAU / response;
        Spring {
            stiffness: omega * omega,
            damping: 2.0 * ratio * omega,
            mass: 1.0,
        }
    }

    /// The share of critical damping: 1 settles without overshoot, below 1
    /// overshoots, and interface motion stays at or above
    /// [`lim::SPRING_MIN_DAMPING_RATIO`].
    pub fn damping_ratio(&self) -> f32 {
        self.damping / (2.0 * (self.stiffness * self.mass).sqrt())
    }

    /// How far past its target it swings, as a share of the distance (0
    /// for a critically or over-damped spring).
    pub fn overshoot(&self) -> f32 {
        let z = self.damping_ratio();
        if z >= 1.0 {
            0.0
        } else {
            (-std::f32::consts::PI * z / (1.0 - z * z).sqrt()).exp()
        }
    }

    /// The same spring in `SpringAnimation`'s terms. Its step adds
    /// `(spring * d - damping * v) / mass` to the speed every
    /// [`QML_SPRING_STEP_S`], where physics adds `(k * d - c * v) / m`
    /// times the step, so `spring = k * step` and `damping = c * step`
    /// with the mass as it is. Checked against qml6 in
    /// `features/design/tokens-motion.feature`.
    pub fn qml(&self, unit: SpringUnit) -> QmlSpring {
        QmlSpring {
            spring: round3(self.stiffness * QML_SPRING_STEP_S),
            damping: round3(self.damping * QML_SPRING_STEP_S),
            mass: self.mass,
            epsilon: unit.epsilon(),
        }
    }
}

/// The spring for something of `size` that follows a gesture: a little
/// life (0.85 of critical damping, a 0.6 % overshoot) and quicker for a
/// small thing than for a whole screen.
pub fn spring(size: Size) -> Spring {
    let response = match size {
        Size::Small => 0.3,
        Size::Panel => 0.4,
        Size::Screen => 0.5,
    };
    Spring::from_response(response, 0.85)
}

/// What to do instead when the user asks for reduced motion.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Reduced {
    /// Milliseconds of the cross-fade; 0 for none (a loop stops and shows
    /// its state still).
    pub duration_ms: u32,
    /// The only property that changes: nothing moves or scales.
    pub property: &'static str,
    pub easing: Bezier,
}

/// One token: the motion for a kind, a size and a frequency.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Token {
    pub kind: Kind,
    pub size: Size,
    pub frequency: Frequency,
    /// Milliseconds; for [`Kind::Loop`], one cycle.
    pub duration_ms: u32,
    pub easing: Bezier,
    /// For [`Kind::Gesture`]: what follows the finger.
    pub spring: Option<Spring>,
    pub reduced: Reduced,
}

/// Milliseconds at [`Frequency::Occasional`], by kind and then by size
/// (small, panel, screen). Within the limits of `limits::motion` for each
/// size, and every way back shorter than its way in.
const OCCASIONAL_MS: [(Kind, [u32; 3]); 9] = [
    (Kind::Enter, [150, 250, 300]),
    (Kind::Exit, [100, 200, 250]),
    (Kind::Move, [150, 250, 300]),
    (Kind::Expand, [150, 250, 300]),
    (Kind::Collapse, [100, 200, 250]),
    (Kind::SheetIn, [200, 300, 350]),
    (Kind::SheetOut, [150, 250, 300]),
    (Kind::Gesture, [150, 250, 300]),
    (Kind::Loop, [1000, 1200, 1500]),
];

/// A rare moment may take this much longer, up to its limit.
const RARE_FACTOR: f32 = 1.4;
/// What a frequent way back takes of the frequent way in, so that it is
/// still the shorter.
const FREQUENT_EXIT_FACTOR: f32 = 0.7;

/// The most a way back takes of its way in.
const WAY_BACK_SHARE: f32 = 0.8;

/// The longest a motion of `size` met at `frequency` may take (loops have
/// no limit: they run while their state lasts).
pub const fn limit_ms(size: Size, frequency: Frequency) -> u32 {
    match (frequency, size) {
        (Frequency::Frequent, _) => lim::FREQUENT_MAX_MS,
        (_, Size::Small) => lim::MICRO_MAX_MS,
        (Frequency::Rare, _) => lim::EMPHASIS_MAX_MS,
        (Frequency::Occasional, _) => lim::STANDARD_MAX_MS,
    }
}

fn occasional_ms(kind: Kind, size: Size) -> u32 {
    let (_, row) = OCCASIONAL_MS
        .iter()
        .find(|(k, _)| *k == kind)
        .expect("every kind has a row");
    row[size.index()]
}

fn round_to_10(ms: f32) -> u32 {
    ((ms / 10.0).round() as u32) * 10
}

fn duration_ms(kind: Kind, size: Size, frequency: Frequency) -> u32 {
    let base = occasional_ms(kind, size);
    if kind == Kind::Loop {
        return base;
    }
    let limit = limit_ms(size, frequency);
    let ms = match frequency {
        Frequency::Occasional => base.min(limit),
        Frequency::Rare => round_to_10(base as f32 * RARE_FACTOR).min(limit),
        Frequency::Frequent => {
            let ms = if kind.leaves() {
                round_to_10(lim::FREQUENT_MAX_MS as f32 * FREQUENT_EXIT_FACTOR)
            } else {
                lim::FREQUENT_MAX_MS
            };
            base.min(ms).max(lim::MIN_VISIBLE_MS)
        }
    };
    // A limit can bring the way in down to the way back; the way back
    // stays the shorter.
    match kind.way_in() {
        Some(way_in) => {
            let forward = duration_ms(way_in, size, frequency) as f32;
            ms.min(round_to_10(forward * WAY_BACK_SHARE))
                .max(lim::MIN_VISIBLE_MS)
        }
        None => ms,
    }
}

/// The curve a kind follows.
pub const fn easing(kind: Kind) -> Bezier {
    match kind {
        Kind::Enter | Kind::Gesture => EASE_OUT,
        Kind::Exit => EASE_OUT_SOFT,
        Kind::Move => EASE_IN_OUT,
        Kind::Expand | Kind::Collapse => DECELERATE,
        Kind::SheetIn | Kind::SheetOut => DRAWER,
        Kind::Loop => LINEAR,
    }
}

/// The token for a kind, a size and a frequency.
pub fn token(kind: Kind, size: Size, frequency: Frequency) -> Token {
    let duration = duration_ms(kind, size, frequency);
    let reduced = if kind == Kind::Loop {
        Reduced {
            duration_ms: 0,
            property: "opacity",
            easing: LINEAR,
        }
    } else {
        Reduced {
            duration_ms: duration.min(lim::REDUCED_FADE_MAX_MS),
            property: "opacity",
            easing: EASE_OUT_SOFT,
        }
    };
    Token {
        kind,
        size,
        frequency,
        duration_ms: duration,
        easing: easing(kind),
        spring: (kind == Kind::Gesture).then(|| spring(size)),
        reduced,
    }
}

/// Every token, kind by kind, then size, then frequency.
pub fn tokens() -> Vec<Token> {
    let mut out = Vec::new();
    for kind in Kind::ALL {
        for size in Size::ALL {
            for frequency in Frequency::ALL {
                out.push(token(kind, size, frequency));
            }
        }
    }
    out
}

/// The scale an entrance starts from (and an exit ends at), with a fade.
pub const ENTER_SCALE: f32 = lim::ENTER_SCALE_RECOMMENDED;

/// The scale a control is pressed down to: enough to be felt under a
/// pointer, too little to move its neighbours' edges by more than a pixel
/// or two at control sizes.
pub const PRESS_SCALE: f32 = 0.97;

/// The token a press follows: met all the time, on a small thing, and an
/// answer to the user, so it eases out.
pub fn press() -> Token {
    Token {
        easing: EASE_OUT,
        ..token(Kind::Move, Size::Small, Frequency::Frequent)
    }
}

impl Token {
    /// A QML animation that does this, with literal numbers, for a file
    /// that does not use the module: `NumberAnimation { … }`, or a
    /// `SpringAnimation` for a gesture.
    pub fn qml(&self, properties: &str) -> String {
        if let Some(spring) = self.spring {
            let q = spring.qml(SpringUnit::Pixels);
            return format!(
                "SpringAnimation {{ properties: \"{properties}\"; spring: {}; damping: {}; mass: {}; epsilon: {} }}",
                number(q.spring),
                number(q.damping),
                number(q.mass),
                number(q.epsilon)
            );
        }
        let loops = if self.kind == Kind::Loop {
            "; loops: Animation.Infinite"
        } else {
            ""
        };
        format!(
            "NumberAnimation {{ properties: \"{properties}\"; duration: {}; easing.type: Easing.BezierSpline; easing.bezierCurve: {}{loops} }}",
            self.duration_ms,
            self.easing.qml_list()
        )
    }
}

/// A number as QML is written: no trailing zeros, at most three decimals.
pub(crate) fn number(v: f32) -> String {
    let s = format!("{:.3}", round3(v));
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}

fn round3(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_duration_is_within_the_limit_of_its_size_and_frequency() {
        for t in tokens() {
            if t.kind == Kind::Loop {
                continue;
            }
            let limit = limit_ms(t.size, t.frequency);
            assert!(
                t.duration_ms <= limit && t.duration_ms >= lim::MIN_VISIBLE_MS,
                "{t:?}: {} ms, limit {limit}",
                t.duration_ms
            );
            assert!(t.reduced.duration_ms <= lim::REDUCED_FADE_MAX_MS, "{t:?}");
        }
    }

    #[test]
    fn a_way_back_is_shorter_than_its_way_in() {
        for kind in Kind::ALL {
            let Some(back) = kind.reverse() else { continue };
            for size in Size::ALL {
                for frequency in Frequency::ALL {
                    let (a, b) = (token(kind, size, frequency), token(back, size, frequency));
                    assert!(b.duration_ms < a.duration_ms, "{a:?} vs {b:?}");
                }
            }
        }
    }

    #[test]
    fn nothing_that_enters_or_answers_starts_slow_and_nothing_bounces() {
        for t in tokens() {
            assert!(!t.easing.overshoots(), "{t:?}");
            if t.kind != Kind::Move && t.kind != Kind::Loop {
                assert!(!t.easing.starts_slow(), "{t:?} starts slow");
                assert!(t.easing.end_slope() < 1.0, "{t:?} does not settle");
            }
        }
        // Linear only where motion is constant.
        for t in tokens().iter().filter(|t| t.easing == LINEAR) {
            assert_eq!(t.kind, Kind::Loop);
        }
    }

    #[test]
    fn a_curve_runs_from_0_to_1() {
        for c in CURVES {
            assert!(c.at(0.0).abs() < 1e-4, "{c:?}");
            assert!((c.at(1.0) - 1.0).abs() < 1e-4, "{c:?}");
            assert!(c.at(0.5) > 0.0 && c.at(0.5) < 1.0, "{c:?}");
        }
        // Ease-out is past half way at a third of the time.
        assert!(EASE_OUT.at(1.0 / 3.0) > 0.5);
        assert!((LINEAR.at(0.3) - 0.3).abs() < 1e-4);
    }

    #[test]
    fn springs_are_damped_enough_and_settle() {
        for size in Size::ALL {
            let s = spring(size);
            assert!(s.damping_ratio() >= lim::SPRING_MIN_DAMPING_RATIO, "{s:?}");
            assert!((s.damping_ratio() - 0.85).abs() < 1e-4);
            assert!(s.overshoot() < 0.01);
            // Run QML's step on the mapped values: it rests within a
            // second, never swings further than physics says (with the
            // step's own error), and never runs away.
            let q = s.qml(SpringUnit::Pixels);
            let (to, mut x, mut v) = (100.0f32, 0.0f32, 0.0f32);
            let mut peak = 0.0f32;
            let mut rested = None;
            for n in 1..=200 {
                v += (q.spring * (to - x) - q.damping * v) / q.mass;
                x += v * QML_SPRING_STEP_S;
                peak = peak.max(x);
                if (to - x).abs() < q.epsilon && v.abs() < q.epsilon {
                    rested = Some(n);
                    break;
                }
            }
            let steps = rested.unwrap_or_else(|| panic!("{size:?} never rests"));
            assert!(
                steps as f32 * QML_SPRING_STEP_S <= 1.0,
                "{size:?}: {steps} steps"
            );
            assert!(
                peak <= to * (1.0 + s.overshoot() + 0.02),
                "{size:?}: {peak}"
            );
        }
    }

    #[test]
    fn the_qml_mapping_is_the_step_times_the_physics() {
        let s = Spring {
            stiffness: 170.0,
            damping: 26.0,
            mass: 1.0,
        };
        let q = s.qml(SpringUnit::Fraction);
        assert_eq!((q.spring, q.damping, q.mass), (2.72, 0.416, 1.0));
        assert_eq!(q.epsilon, 0.002);
    }

    #[test]
    fn a_token_is_written_as_qml() {
        let t = token(Kind::Enter, Size::Panel, Frequency::Occasional);
        assert_eq!(
            t.qml("opacity,scale"),
            "NumberAnimation { properties: \"opacity,scale\"; duration: 250; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.23, 1, 0.32, 1, 1, 1] }"
        );
        let g = token(Kind::Gesture, Size::Small, Frequency::Occasional);
        assert!(
            g.qml("x")
                .starts_with("SpringAnimation { properties: \"x\"; spring: 7.018;")
        );
        let l = token(Kind::Loop, Size::Small, Frequency::Frequent);
        assert!(l.qml("rotation").ends_with("loops: Animation.Infinite }"));
        assert_eq!(l.reduced.duration_ms, 0);
    }

    #[test]
    fn names_parse_back() {
        for k in Kind::ALL {
            assert_eq!(Kind::parse(k.name()), Some(k));
        }
        for s in Size::ALL {
            assert_eq!(Size::parse(s.name()), Some(s));
        }
        for f in Frequency::ALL {
            assert_eq!(Frequency::parse(f.name()), Some(f));
        }
    }
}
