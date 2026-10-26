//! The QML module `Ljos.Design`, written from the tokens of this crate: the
//! palette as singletons that follow the platform's colour scheme and
//! contrast, motion that follows its reduced-motion preference, the spacing
//! and type scales, and transitions and behaviours that use them.
//!
//! The text is plain QML that Qt runs as well: no colour maths in a
//! binding, only tables to pick from. `ljos design export` writes these
//! files into a directory an application imports; Ljós builds the same
//! files in (`ljos_design_qml`), and a test there fails when what is
//! committed is not what this writes.

use std::fmt::Write as _;

use crate::motion::{self, Frequency, Kind, Size, SpringUnit, number};
use crate::palette::{self, Contrast, ROLES, Scheme, Seed};
use crate::scale;

/// What an application imports.
pub const URI: &str = "Ljos.Design";
/// Where the files are, below a directory on the import path.
pub const DIRECTORY: &str = "Ljos/Design";
/// The module's one-object types.
pub const SINGLETONS: [&str; 4] = ["Colors", "Motion", "Spacing", "Typography"];
/// The types an application makes objects of.
pub const TYPES: [&str; 4] = [
    "EnterTransition",
    "ExitTransition",
    "MoveBehavior",
    "PressBehavior",
];
/// The built-in palette has a seed every this many degrees of hue; a seed
/// between two takes the nearer.
pub const HUE_STEP: u32 = 30;
/// The hue of the palette nothing set a seed for: a calm blue.
pub const DEFAULT_HUE: u32 = 240;

/// One file of the module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    /// The name in [`DIRECTORY`]: `Colors.qml`, `qmldir`.
    pub name: String,
    pub text: String,
}

/// The palettes a module carries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seeds {
    /// A palette every [`HUE_STEP`] degrees; `Colors.seedHue` picks one.
    Hues,
    /// The palette of one seed: what an application with a brand colour
    /// exports.
    One(Seed),
}

/// Every file of the module, `qmldir` first.
pub fn files(seeds: Seeds) -> Vec<File> {
    let mut out = vec![
        file("qmldir", qmldir()),
        file("Colors.qml", colors(seeds)),
        file("Motion.qml", motion_singleton()),
        file("Spacing.qml", spacing()),
        file("Typography.qml", typography()),
    ];
    out.extend([
        file("EnterTransition.qml", enter_transition()),
        file("ExitTransition.qml", exit_transition()),
        file("MoveBehavior.qml", move_behavior()),
        file("PressBehavior.qml", press_behavior()),
    ]);
    out
}

fn file(name: &str, text: String) -> File {
    File {
        name: name.to_owned(),
        text,
    }
}

const HEADER: &str = "// Written by ljos_design (crates/ljos_design/src/module.rs); \
                      `ljos design export` writes it again.\n";

/// Reduced motion, read where Qt has it (6.12) and false where it has not.
const REDUCED: &str = "Qt.styleHints.accessibility !== undefined \
                       && Qt.styleHints.accessibility.motionPreference === 1";
/// The platform's high contrast (Qt 6.10).
const HIGH_CONTRAST: &str = "Qt.styleHints.accessibility !== undefined \
                             && Qt.styleHints.accessibility.contrastPreference === 1";

fn qmldir() -> String {
    let mut s = format!("module {URI}\n");
    for name in SINGLETONS {
        let _ = writeln!(s, "singleton {name} 1.0 {name}.qml");
    }
    for name in TYPES {
        let _ = writeln!(s, "{name} 1.0 {name}.qml");
    }
    s
}

/// A role's property name in `Colors`. QML takes a member named `on` and
/// a capital for a signal handler, and Qt reads such a property as black,
/// so `onAccent` (the text and icons on an accent fill) is `textOnAccent`.
pub fn qml_role(role: &str) -> String {
    match role.strip_prefix("on") {
        Some(rest) if rest.starts_with(|c: char| c.is_ascii_uppercase()) => format!("textOn{rest}"),
        _ => role.to_owned(),
    }
}

/// `sheet-in` as a property name: `sheetIn`.
fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in name.chars() {
        if c == '-' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn capital(name: &str) -> String {
    let mut c = name.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// The palette's four variants, as the table names them.
const VARIANTS: [(&str, Scheme, Contrast); 4] = [
    ("light", Scheme::Light, Contrast::Normal),
    ("dark", Scheme::Dark, Contrast::Normal),
    ("lightHigh", Scheme::Light, Contrast::High),
    ("darkHigh", Scheme::Dark, Contrast::High),
];

fn colors(seeds: Seeds) -> String {
    let (list, pick, seed_line) = match seeds {
        Seeds::Hues => {
            let hues: Vec<Seed> = (0..360 / HUE_STEP)
                .map(|i| Seed::hue((i * HUE_STEP) as f32))
                .collect();
            let n = hues.len();
            (
                hues,
                format!("Math.round(((seedHue % 360) + 360) % 360 / {HUE_STEP}) % {n}"),
                format!(
                    "    // The OKLCH hue the palette grows from, in degrees: the nearest\n    \
                     // palette of one every {HUE_STEP}° is used.\n    \
                     property real seedHue: {DEFAULT_HUE}\n"
                ),
            )
        }
        Seeds::One(seed) => (
            vec![seed],
            "0".to_owned(),
            format!(
                "    // The OKLCH hue of the seed this module was exported for.\n    \
                 readonly property real seedHue: {}\n",
                number(seed.hue)
            ),
        ),
    };

    let mut s = format!("{HEADER}pragma Singleton\nimport QtQuick\n\n");
    s.push_str(
        "// The palette's colours by what they are for, light or dark as the\n\
         // platform's colour scheme is, and with the contrast a user asks for.\n\
         // Every text, mark and focus ring passes WCAG against the grounds it\n\
         // is meant for: 4.5:1 for text (7:1 in high contrast), 3:1 for the rest.\n\
         QtObject {\n",
    );
    s.push_str(&seed_line);
    let _ = writeln!(
        s,
        "    property bool dark: Application.styleHints.colorScheme === Qt.ColorScheme.Dark"
    );
    let _ = writeln!(s, "    property bool highContrast: {HIGH_CONTRAST}\n");
    s.push_str("    // One entry for each seed, a list of the roles below for each variant.\n");
    s.push_str("    readonly property var table: [\n");
    for (i, seed) in list.iter().enumerate() {
        let _ = writeln!(s, "        {{ // hue {}", number(seed.hue));
        for (j, (variant, scheme, contrast)) in VARIANTS.iter().enumerate() {
            let p = palette::palette(*seed, *scheme, *contrast);
            let colours: Vec<String> = p.roles.iter().map(|r| format!("\"{}\"", r.color)).collect();
            let comma = if j + 1 < VARIANTS.len() { "," } else { "" };
            let _ = writeln!(
                s,
                "            \"{variant}\": [{}]{comma}",
                colours.join(", ")
            );
        }
        let comma = if i + 1 < list.len() { "," } else { "" };
        let _ = writeln!(s, "        }}{comma}");
    }
    s.push_str("    ]\n");
    let _ = writeln!(
        s,
        "    readonly property var roles: table[{pick}][(dark ? \"dark\" : \"light\") + (highContrast ? \"High\" : \"\")]\n"
    );
    for (i, role) in ROLES.iter().enumerate() {
        let _ = writeln!(
            s,
            "    readonly property color {}: roles[{i}]",
            qml_role(role)
        );
    }
    s.push_str("}\n");
    s
}

fn motion_singleton() -> String {
    let mut s = format!("{HEADER}pragma Singleton\nimport QtQuick\n\n");
    s.push_str(
        "// How long a change takes and the curve it follows, by what it does,\n\
         // how much of the window it moves and how often a user meets it.\n\
         QtObject {\n",
    );
    let _ = writeln!(
        s,
        "    // The user asked for less motion: move nothing, cross-fade instead.\n    \
         readonly property bool reduced: {REDUCED}\n"
    );
    s.push_str(
        "    // Milliseconds: enterPanel is met now and then, enterPanelFrequent\n    \
         // many times a day, enterPanelRare once in a while. A loop's is one cycle.\n",
    );
    for t in motion::tokens() {
        let suffix = match t.frequency {
            Frequency::Occasional => "",
            Frequency::Rare => "Rare",
            Frequency::Frequent => "Frequent",
        };
        let _ = writeln!(
            s,
            "    readonly property int {}{}{suffix}: {}",
            camel(t.kind.name()),
            capital(t.size.name()),
            t.duration_ms
        );
    }
    let press = motion::press();
    let _ = writeln!(
        s,
        "    readonly property int press: {}\n",
        press.duration_ms
    );
    s.push_str("    // easing.bezierCurve lists, for easing.type: Easing.BezierSpline.\n");
    for c in motion::CURVES {
        let _ = writeln!(s, "    readonly property var {}: {}", c.name, c.qml_list());
    }
    let _ = writeln!(
        s,
        "\n    // What an entrance grows from and an exit shrinks to.\n    \
         readonly property real enterScale: {}\n    \
         readonly property real pressScale: {}\n",
        number(motion::ENTER_SCALE),
        number(motion::PRESS_SCALE)
    );
    s.push_str("    // SpringAnimation numbers for what follows a finger, in pixels.\n");
    for size in Size::ALL {
        let q = motion::spring(size).qml(SpringUnit::Pixels);
        let name = format!("gesture{}", capital(size.name()));
        let _ = writeln!(
            s,
            "    readonly property real {name}Spring: {}",
            number(q.spring)
        );
        let _ = writeln!(
            s,
            "    readonly property real {name}Damping: {}",
            number(q.damping)
        );
        let _ = writeln!(
            s,
            "    readonly property real {name}Mass: {}",
            number(q.mass)
        );
        let _ = writeln!(
            s,
            "    readonly property real {name}Epsilon: {}",
            number(q.epsilon)
        );
    }
    s.push_str("}\n");
    s
}

fn spacing() -> String {
    let mut s = format!("{HEADER}pragma Singleton\nimport QtQuick\n\n");
    let _ = writeln!(
        s,
        "// Gaps and padding in logical pixels, steps of {} px.\nQtObject {{",
        number(crate::limits::SPACING_UNIT_PX)
    );
    let _ = writeln!(
        s,
        "    readonly property int unit: {}",
        number(crate::limits::SPACING_UNIT_PX)
    );
    for step in scale::spacing() {
        let _ = writeln!(
            s,
            "    readonly property int {}: {}",
            step.name,
            number(step.px)
        );
    }
    s.push_str("}\n");
    s
}

fn typography() -> String {
    let mut s = format!("{HEADER}pragma Singleton\nimport QtQuick\n\n");
    let _ = writeln!(
        s,
        "// Text sizes a ratio of {} apart from a body of {} px: font.pixelSize,\n\
         // lineHeight (with lineHeightMode: Text.FixedHeight) and font.weight.\nQtObject {{",
        number(scale::RATIO),
        number(scale::BODY_PX)
    );
    for step in scale::type_scale() {
        let _ = writeln!(
            s,
            "    readonly property int {}Size: {}",
            step.name, step.px
        );
        let _ = writeln!(
            s,
            "    readonly property int {}LineHeight: {}",
            step.name, step.line_height
        );
        let _ = writeln!(
            s,
            "    readonly property int {}Weight: {}",
            step.name, step.weight
        );
    }
    s.push_str("}\n");
    s
}

fn easing(curve: motion::Bezier) -> String {
    format!(
        "easing.type: Easing.BezierSpline; easing.bezierCurve: {}",
        curve.qml_list()
    )
}

/// A popup's entrance or exit: a fade and a scale between `ENTER_SCALE`
/// and 1, from the side `origin` names.
fn popup_transition(kind: Kind) -> String {
    let t = motion::token(kind, Size::Panel, Frequency::Occasional);
    let (from, to, what) = if kind == Kind::Enter {
        ("0", "1", "appears: it fades in and grows from")
    } else {
        ("1", "0", "goes: it fades out and shrinks to")
    };
    let scale = number(motion::ENTER_SCALE);
    let (scale_from, scale_to) = if kind == Kind::Enter {
        (format!("root.reduced ? 1 : {scale}"), "1".to_owned())
    } else {
        ("1".to_owned(), format!("root.reduced ? 1 : {scale}"))
    };
    let mut s = format!("{HEADER}import QtQuick\nimport QtQuick.Controls\n\n");
    let _ = write!(
        s,
        "// How a popup, a dialog or a menu {what} {}% of its size,\n\
         // from the side its anchor is on. With reduced motion it only fades.\n\
         Transition {{\n    \
             id: root\n    \
             // The point it scales about: Popup.Top for a menu below its button.\n    \
             property int origin: Popup.Center\n    \
             property int duration: {}\n    \
             readonly property bool reduced: {REDUCED}\n\n    \
             PropertyAction {{ property: \"transformOrigin\"; value: root.origin }}\n    \
             NumberAnimation {{ property: \"opacity\"; from: {from}; to: {to}; duration: root.reduced ? Math.min(root.duration, {}) : root.duration; {} }}\n    \
             NumberAnimation {{ property: \"scale\"; from: {scale_from}; to: {scale_to}; duration: root.reduced ? 0 : root.duration; {} }}\n\
         }}\n",
        number(motion::ENTER_SCALE * 100.0),
        t.duration_ms,
        t.reduced.duration_ms,
        easing(t.easing),
        easing(t.easing),
    );
    s
}

fn enter_transition() -> String {
    popup_transition(Kind::Enter)
}

fn exit_transition() -> String {
    popup_transition(Kind::Exit)
}

fn behavior(what: &str, t: motion::Token) -> String {
    let mut s = format!("{HEADER}import QtQuick\n\n");
    let _ = write!(
        s,
        "{what}\
         Behavior {{\n    \
             id: root\n    \
             property int duration: {}\n    \
             readonly property bool reduced: {REDUCED}\n    \
             enabled: !reduced\n\n    \
             NumberAnimation {{ duration: root.duration; {} }}\n\
         }}\n",
        t.duration_ms,
        easing(t.easing),
    );
    s
}

fn move_behavior() -> String {
    behavior(
        "// Something on screen that moves to another place: `MoveBehavior on x {}`.\n\
         // With reduced motion it is there at once.\n",
        motion::token(Kind::Move, Size::Panel, Frequency::Occasional),
    )
}

fn press_behavior() -> String {
    behavior(
        "// A control giving under a press: `PressBehavior on scale {}` with\n\
         // `scale: pressed ? Motion.pressScale : 1`. With reduced motion it is\n\
         // there at once.\n",
        motion::press(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_qmldir_names_every_file() {
        let files = files(Seeds::Hues);
        let qmldir = &files[0].text;
        for f in &files[1..] {
            let name = f.name.trim_end_matches(".qml");
            assert!(
                qmldir.contains(&format!("{name} 1.0 {}\n", f.name)),
                "{}",
                f.name
            );
        }
        assert!(qmldir.starts_with("module Ljos.Design\n"));
    }

    #[test]
    fn colours_have_a_palette_for_every_hue_step_and_variant() {
        let text = colors(Seeds::Hues);
        assert_eq!(
            text.matches("\"lightHigh\": [").count(),
            (360 / HUE_STEP) as usize
        );
        for role in ROLES {
            let name = qml_role(role);
            assert!(text.contains(&format!("property color {name}: ")), "{name}");
        }
        assert!(!text.contains("property color on"));
        assert_eq!(qml_role("onAccent"), "textOnAccent");
        assert_eq!(qml_role("canvas"), "canvas");
        let one = colors(Seeds::One(Seed::parse("#1d4ed8").unwrap()));
        assert_eq!(one.matches("\"darkHigh\": [").count(), 1);
        assert!(one.contains("\"#1d4ed8\""));
    }

    #[test]
    fn suggested_motion_eases_out_and_keeps_to_its_limits() {
        for kind in [Kind::Enter, Kind::Exit] {
            let t = motion::token(kind, Size::Panel, Frequency::Occasional);
            assert!(!t.easing.starts_slow());
            assert!(t.duration_ms <= motion::limit_ms(t.size, t.frequency));
            assert!(popup_transition(kind).contains(&t.easing.qml_list()));
        }
        let press = motion::press();
        assert!(!press.easing.starts_slow());
        assert!(press.duration_ms <= motion::limit_ms(Size::Small, Frequency::Frequent));
        const { assert!(motion::PRESS_SCALE >= motion::ENTER_SCALE && motion::PRESS_SCALE < 1.0) };
    }

    #[test]
    fn names_are_camel_case() {
        assert_eq!(camel("sheet-in"), "sheetIn");
        assert_eq!(capital("panel"), "Panel");
    }
}
