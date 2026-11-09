//! Ljós's own style (`Ljos.Controls`, docs/DIALECT.md §4.16): its `Theme`
//! derives what `ljos_design::roles` derives, every control in it passes
//! the accessibility audit in a light and a dark scheme, and every one that
//! takes the keyboard draws a focus ring that can be seen.
//!
//! The audit leaves the non-text contrast and the focus indicator of a
//! style's own parts to the style (`declared_by_app` in `ljos_audit`), so
//! those are held here: the ring is the `focus` role at 2 px, and the role
//! is 3:1 against every fill by `roles::derive`'s promises, which the
//! derivation's own tests hold over every palette.
//!
//! The audited files import `QtQuick` beside `Ljos.Controls`: `Ljos.Ui` has
//! no `Accessible` yet, and a control is named for the audit with it.

use ljos_a11y::{ColorScheme, SystemPrefs};
use ljos_design::color::Srgb;
use ljos_design::roles::{self, Base};
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::Time;
use ljos_test::a11y::{Options, audit};

const SCHEMES: [ColorScheme; 2] = [ColorScheme::Light, ColorScheme::Dark];

fn engine(scheme: ColorScheme) -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    ljos_quick::set_system_prefs(
        &e,
        SystemPrefs {
            color_scheme: scheme,
            ..SystemPrefs::default()
        },
    );
    e
}

fn load(e: &Engine, source: &str) -> ObjectId {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", source);
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let mut files = ljos_engine::MapSource::new();
    files.insert("Main", c.file);
    e.set_component_source(files);
    e.create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"))
}

fn scene(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = QuickScene::new(kurbo::Size::new(480.0, 400.0));
    s.set_root(e, root).unwrap();
    ljos_drive::settle_at(e, &mut s, Time::ZERO).unwrap();
    s
}

fn find(e: &Engine, root: ObjectId, name: &str) -> ObjectId {
    ljos_drive::find_below(e, root, name).unwrap_or_else(|| panic!("no object named {name}"))
}

fn hex(v: &Value) -> String {
    match v {
        Value::Color(c) => c.to_string(),
        other => panic!("not a colour: {other:?}"),
    }
}

/// The QML theme and the Rust derivation give the same colours, every role,
/// for the same three, in light and in dark.
#[test]
fn the_theme_derives_what_rust_derives() {
    let bases = [
        ("#ffffff", "#000000", "#0066cc"),
        ("#1e1e2e", "#e6e6f0", "#7c3aed"),
        ("#fafaf7", "#1b1b18", "#c2410c"),
        ("#0b0d10", "#f2f4f8", "#22c55e"),
    ];
    for (ground, text, accent) in bases {
        let props: String = roles::NAMES
            .iter()
            .map(|n| format!("    readonly property color r_{n}: Theme.{n}\n"))
            .collect();
        let source = format!(
            "import Ljos.Ui\nimport Ljos.Core\nimport Ljos.Controls\nItem {{\n    \
             Component.onCompleted: {{ Theme.canvas = \"{ground}\"; Theme.text = \"{text}\"; \
             Theme.accent = \"{accent}\" }}\n{props}}}\n"
        );
        let e = engine(ColorScheme::Light);
        let root = load(&e, &source);
        let _s = scene(&e, root);
        let want = roles::derive(Base {
            ground: Srgb::parse(ground).unwrap(),
            text: Srgb::parse(text).unwrap(),
            accent: Srgb::parse(accent).unwrap(),
        });
        for name in roles::NAMES {
            let got = hex(&e.read_by_name(root, &format!("r_{name}")));
            let expected = want.get(name).unwrap().to_hex();
            assert_eq!(got, expected, "{name} of {ground}/{text}/{accent}");
        }
        let warnings = e.take_warnings();
        assert!(warnings.is_empty(), "{warnings:?}");
    }
}

const ALL: &str = r##"
import QtQuick
import Ljos.Controls
Rectangle {
    width: 480; height: 400; color: Theme.canvas
    readonly property color theme_canvas: Theme.canvas
    readonly property color theme_text: Theme.text
    readonly property color theme_accent: Theme.accent
    Grid {
        x: 16; y: 16; columns: 2; spacing: 12
        Label { id: who; text: "Name" }
        Button { objectName: "button"; text: "Save" }
        Button { text: "Send"; primary: true }
        Button { text: "Delete"; destructive: true }
        Button { text: "More"; subtle: true; chevron: true }
        Button { text: "Off"; enabled: false; Accessible.description: "Turns on once there is a change" }
        Switch { objectName: "switch"; text: "Wrap lines" }
        CheckBox { objectName: "check"; text: "Show hidden files"; checked: true }
        ComboBox { objectName: "combo"; model: ["Small", "Medium", "Large"]; Accessible.name: "Size" }
        ComboBox { model: ["One", "Two"]; outline: true; Accessible.name: "Count" }
        TextField { objectName: "field"; placeholderText: "Ada"; Accessible.labelledBy: who }
        SpinBox { objectName: "spin"; value: 4; Accessible.name: "Columns" }
        Slider { objectName: "slider"; value: 0.5; Accessible.name: "Volume" }
    }
}
"##;

/// Every colour a control of the style draws is a role of the theme, so
/// the ratios `roles::PROMISES` holds are the ones drawn: a shade worked
/// out in a control (`Qt.darker` of the accent) or a colour written in
/// one would have none.
#[test]
fn the_style_draws_only_the_themes_roles() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/qml/Ljos/Controls");
    let mut seen = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".qml") || name == "Theme.qml" {
            continue;
        }
        seen += 1;
        let source = std::fs::read_to_string(&path).unwrap();
        for (n, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for worked in [
                "Qt.darker(",
                "Qt.lighter(",
                "Qt.tint(",
                "Qt.hsla(",
                "Qt.hsva(",
            ] {
                assert!(!code.contains(worked), "{name}:{}: {worked} {line}", n + 1);
            }
            // A shadow's rgba is decoration, drawn under the popup's edge.
            if code.contains("Qt.rgba(") {
                assert!(
                    code.contains("Qt.rgba(0, 0, 0,"),
                    "{name}:{}: {line}",
                    n + 1
                );
            }
            assert!(
                !code.contains("\"#"),
                "{name}:{}: a colour written in: {line}",
                n + 1
            );
        }
    }
    assert!(seen > 10, "{seen} files");
}

#[test]
fn every_control_passes_the_audit_in_both_schemes() {
    for scheme in SCHEMES {
        let e = engine(scheme);
        let root = load(&e, ALL);
        let mut s = scene(&e, root);
        let report = audit(&e, &mut s, Time::ZERO, &Options::desktop());
        assert!(report.is_clean(), "{scheme:?}: found:\n{report}");
        let warnings = e.take_warnings();
        assert!(warnings.is_empty(), "{scheme:?}: {warnings:?}");
    }
}

#[test]
fn an_open_menu_passes_the_audit_in_both_schemes() {
    const MENU: &str = r##"
import QtQuick
import Ljos.Controls
Rectangle {
    width: 480; height: 400; color: Theme.canvas
    Menu {
        Component.onCompleted: open()
        MenuItem { text: "Cut"; shortcut: "Ctrl+X"; iconName: "minus" }
        MenuItem { text: "Copy"; shortcut: "Ctrl+C"; enabled: false; Accessible.description: "Select something to copy" }
        MenuSeparator {}
        MenuItem { text: "Wrap"; checkable: true; checked: true }
    }
}
"##;
    for scheme in SCHEMES {
        let e = engine(scheme);
        let root = load(&e, MENU);
        let mut s = scene(&e, root);
        let report = audit(&e, &mut s, Time::ZERO, &Options::desktop());
        assert!(report.is_clean(), "{scheme:?}: found:\n{report}");
    }
}

/// Each control given the keyboard's focus draws a 2 px ring in the
/// `focus` role. A part filled with the accent has its ring outside it, on
/// the canvas; the others draw it as their own edge, on a raised fill. The
/// role is 3:1 against both.
#[test]
fn every_control_draws_a_focus_ring_that_can_be_seen() {
    // The control, and the part whose edge is the ring (`None`: a ring of
    // its own around the part).
    let rings = [
        ("button", None),
        ("switch", None),
        ("check", None),
        ("combo", Some("background")),
        ("field", Some("background")),
        ("spin", Some("background")),
        ("slider", Some("handle")),
    ];
    for scheme in SCHEMES {
        for (name, part) in rings {
            let e = engine(scheme);
            let root = load(&e, ALL);
            let mut s = scene(&e, root);
            let control = find(&e, root, name);
            let at = |what: &str| match part {
                Some(part) => {
                    let Value::Object(part) = e.read_by_name(control, part) else {
                        panic!("{name} has no {part}");
                    };
                    e.read_by_name(part, what)
                }
                None => e.read_by_name(find(&e, control, "focusRing"), what),
            };
            let before = at("border.width");
            // The keyboard's reason (Qt.TabFocusReason), as a Tab gives it.
            e.invoke_by_name(control, "forceActiveFocus", &[Value::Int(1)])
                .unwrap_or_else(|err| panic!("{name}: {err}"));
            ljos_drive::settle_at(&e, &mut s, Time::ZERO).unwrap();
            if part.is_none() {
                assert_eq!(
                    e.read_by_name(find(&e, control, "focusRing"), "visible"),
                    Value::Bool(true),
                    "{scheme:?} {name}: the ring is not shown"
                );
            }
            let width = at("border.width");
            assert_eq!(
                width.to_number(),
                2.0,
                "{scheme:?} {name}: {before:?} → {width:?}"
            );
            let ring = Srgb::parse(&hex(&at("border.color"))).unwrap();
            let r = derive_for(&e, root);
            assert_eq!(
                ring, r.focus,
                "{scheme:?} {name}: the ring is not the focus role"
            );
            for (on, fill) in [("canvas", r.canvas), ("surfaceRaised", r.surface_raised)] {
                let c = ljos_design::color::wcag_contrast(ring, fill);
                assert!(c >= 3.0, "{scheme:?} {name}: ring on {on} is {c:.2}:1");
            }
        }
    }
}

/// The roles the theme of `root`'s engine has, derived again from its three.
fn derive_for(e: &Engine, root: ObjectId) -> roles::Roles {
    let three = |n: &str| Srgb::parse(&hex(&e.read_by_name(root, &format!("theme_{n}")))).unwrap();
    roles::derive(Base {
        ground: three("canvas"),
        text: three("text"),
        accent: three("accent"),
    })
}
