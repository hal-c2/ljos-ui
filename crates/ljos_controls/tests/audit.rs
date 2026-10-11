//! The accessibility audit over the Basic style, in a light and in a dark
//! colour scheme: what the style draws is to be found, named and read
//! against what is behind it in both.

use ljos_a11y::{ColorScheme, SystemPrefs};
use ljos_engine::{Engine, ObjectId};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::Time;
use ljos_test::a11y::{Code, Options, Report, audit};

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

/// Compiles `source` as `Main.qml` and creates it.
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

/// The body in a window, which is as dark as the scheme.
fn run(scheme: ColorScheme, body: &str) -> Report {
    let source = format!(
        "import QtQuick\nimport QtQuick.Controls.Basic\nApplicationWindow {{\n    \
         width: 640; height: 480; visible: true\n{body}\n}}\n"
    );
    let e = engine(scheme);
    let root = load(&e, &source);
    let mut s = QuickScene::new(kurbo::Size::new(640.0, 480.0));
    s.set_root(&e, root).unwrap();
    s.frame(&e, Time::ZERO);
    let report = audit(&e, &mut s, Time::ZERO, &Options::desktop());
    let warnings = e.take_warnings();
    assert!(warnings.is_empty(), "{warnings:?}");
    report
}

const SCHEMES: [ColorScheme; 2] = [ColorScheme::Light, ColorScheme::Dark];

const GALLERY: &str = r##"
    Column {
        x: 8; y: 8; spacing: 8
        ToolBar {
            width: 600
            Row {
                ToolButton { text: "Back" }
                ToolButton { text: "Forward"; enabled: false }
            }
        }
        Row {
            spacing: 8
            Button { text: "Save" }
            Button { text: "Off"; enabled: false }
            Button { text: "Bold"; checkable: true; checked: true }
            Button { text: "Lit"; highlighted: true }
            Button { text: "Flat"; flat: true }
        }
        Row {
            spacing: 8
            CheckBox { text: "Wrap" }
            CheckBox { text: "Some"; tristate: true; checkState: Qt.PartiallyChecked }
            RadioButton { text: "One"; checked: true }
            RadioButton { text: "Two" }
            Switch { text: "Sound" }
            Switch { text: "On"; checked: true }
        }
        Row {
            spacing: 8
            Label { id: who; text: "Name"; height: 40; verticalAlignment: Text.AlignVCenter }
            TextField { Accessible.labelledBy: who; text: "Ada" }
            SpinBox { Accessible.name: "Copies"; value: 2 }
            ComboBox { Accessible.name: "Paper"; model: ["A4", "A5", "Letter"] }
        }
        Row {
            spacing: 8
            Frame { Label { text: "In a frame" } }
            GroupBox { title: "Margins"; Label { text: "In a group" } }
            Pane { Label { text: "In a pane" } }
            ItemDelegate { text: "A row" }
            ScrollView {
                width: 160; height: 80
                TextArea { Accessible.name: "Notes"; text: "one\ntwo\nthree\nfour\nfive\nsix" }
            }
        }
    }"##;

#[test]
fn the_style_is_clean_in_both_schemes() {
    for scheme in SCHEMES {
        let report = run(scheme, GALLERY);
        assert!(
            report.is_clean(),
            "{scheme:?}: expected nothing, found:\n{report}"
        );
    }
}

/// What is in a popup is audited while the popup is open.
#[test]
fn the_popups_of_the_style_are_clean_in_both_schemes() {
    for (what, body) in POPUPS {
        for scheme in SCHEMES {
            let report = run(scheme, body);
            assert!(
                report.is_clean(),
                "{what}, {scheme:?}: expected nothing, found:\n{report}"
            );
        }
    }
}

const POPUPS: [(&str, &str); 4] = [
    (
        "popup that fades in",
        r##"
    Popup {
        enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1 } }
        Component.onCompleted: open()
        Label { text: "Saved" }
    }"##,
    ),
    (
        "menu",
        r##"
    Menu {
        Component.onCompleted: open()
        MenuItem { text: "Cut" }
        MenuItem { text: "Copy"; enabled: false }
        MenuSeparator {}
        MenuItem { text: "Wrap"; checkable: true; checked: true }
        Menu { title: "More"; MenuItem { text: "Deep" } }
    }"##,
    ),
    (
        "dialog",
        r##"
    Dialog {
        title: "Discard the changes?"
        standardButtons: Dialog.Ok | Dialog.Cancel
        Component.onCompleted: open()
        Label { text: "They cannot be brought back." }
    }"##,
    ),
    (
        "tool tip",
        r##"
    Button {
        text: "Save"
        ToolTip.visible: true
        ToolTip.text: "Writes the file"
    }"##,
    ),
];

// What the audit holds against the style. Each colour is the one Qt's
// Basic style has (`features/controls/control-palette.feature` and
// `control-palette-dark.feature` pin them), so it stays until a style of
// our own says otherwise; these tests are where the findings are written
// down.

/// The one finding of `body` in `scheme`: text of too little contrast,
/// drawn as `colours` says.
#[track_caller]
fn too_faint(scheme: ColorScheme, body: &str, colours: &str) {
    let report = run(scheme, body);
    assert_eq!(report.codes(), [Code::Contrast], "{scheme:?}\n{report}");
    assert_eq!(report.findings.len(), 1, "{scheme:?}\n{report}");
    let f = &report.findings[0];
    assert!(f.message.contains(colours), "{scheme:?}: {f}");
}

/// 3.04:1 in a light scheme and 3.95:1 in a dark one, where 4.5:1 is the
/// least.
#[test]
fn the_placeholder_is_too_faint_in_both_schemes() {
    let body = r##"
    TextField { Accessible.name: "Name"; placeholderText: "Given name" }"##;
    too_faint(ColorScheme::Light, body, "#939494 on #ffffff");
    too_faint(ColorScheme::Dark, body, "#6b6b6c on #000000");
}

/// A combo box is drawn pressed while its popup is open, which in a dark
/// scheme is `#d4d6d8` on `#626262`, 4.19:1. The audit does not say so:
/// the text is drawn by the field of the style, which is switched off
/// while the combo box is not editable (as Qt's is), and what is switched
/// off is not held to the contrast. An editable one is held to it.
#[test]
fn the_text_of_a_combo_box_that_is_not_editable_is_not_audited() {
    let body = r##"
    ComboBox {
        Accessible.name: "Paper"
        model: ["A4", "A5", "Letter"]
        Component.onCompleted: popup.open()
    }"##;
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        let report = run(scheme, body);
        assert!(report.is_clean(), "{scheme:?}\n{report}");
    }
}

/// The item of a menu bar whose menu is open has the button's text on the
/// palette's `mid`: clean in a light scheme, 4.19:1 in a dark one.
#[test]
fn the_item_of_an_open_menu_is_too_faint_in_a_dark_scheme() {
    let body = r##"
    menuBar: MenuBar {
        Menu { title: "File"; MenuItem { text: "New" }; MenuItem { text: "Open" } }
        Menu { title: "Edit"; MenuItem { text: "Cut"; enabled: false } }
    }
    Component.onCompleted: menuBar.itemAt(0).triggered()"##;
    let report = run(ColorScheme::Light, body);
    assert!(report.is_clean(), "{report}");
    too_faint(ColorScheme::Dark, body, "#d4d6d8 on #626262");
}

/// The demo of the shell is the style as an application uses it. Its text
/// field shows a placeholder, which is the one thing held against it.
#[test]
fn the_demo_has_only_its_placeholder_against_it() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../ljos_shell/examples/qml/Main.qml"
    );
    let source = std::fs::read_to_string(path).unwrap();
    for (scheme, colours) in [
        (ColorScheme::Light, "#939494 on #ffffff"),
        (ColorScheme::Dark, "#6b6b6c on #000000"),
    ] {
        let e = engine(scheme);
        let root = load(&e, &source);
        let mut s = QuickScene::new(kurbo::Size::new(640.0, 480.0));
        s.set_root(&e, root).unwrap();
        // A shell gives the root its window's size.
        for (name, v) in [("width", 640.0), ("height", 480.0)] {
            e.write_by_name(root, name, ljos_engine::Value::Real(v))
                .unwrap();
        }
        s.frame(&e, Time::ZERO);
        let report = audit(&e, &mut s, Time::ZERO, &Options::desktop());
        assert_eq!(report.findings.len(), 1, "{scheme:?}\n{report}");
        assert_eq!(report.codes(), [Code::Contrast], "{scheme:?}\n{report}");
        let f = &report.findings[0];
        assert!(f.message.contains(colours), "{scheme:?}: {f}");
        let warnings = e.take_warnings();
        assert!(warnings.is_empty(), "{warnings:?}");
    }
}

#[test]
fn an_editable_combo_box_is_clean_in_both_schemes() {
    let body = r##"
    ComboBox {
        Accessible.name: "Paper"
        editable: true
        model: ["A4", "A5", "Letter"]
    }"##;
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        let report = run(scheme, body);
        assert!(report.is_clean(), "{scheme:?}\n{report}");
    }
}
