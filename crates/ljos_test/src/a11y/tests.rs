//! Each check with a component that passes and one that fails, and the
//! audit over the Basic style.

use ljos_engine::{Engine, ObjectId};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::Time;

use super::{Code, Options, Report, audit};

fn engine() -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
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

fn mount(e: &Engine, source: &str) -> QuickScene {
    let root = load(e, source);
    let mut s = QuickScene::new(kurbo::Size::new(640.0, 480.0));
    s.set_root(e, root).unwrap();
    s.frame(e, Time::ZERO);
    s
}

fn audit_with(source: &str, options: &Options) -> Report {
    let e = engine();
    let mut s = mount(&e, source);
    let report = audit(&e, &mut s, Time::ZERO, options);
    let warnings = e.take_warnings();
    assert!(warnings.is_empty(), "{warnings:?}");
    report
}

fn run(source: &str) -> Report {
    audit_with(source, &Options::desktop())
}

/// The body in a window-sized white item, so that only the body is what
/// a check can find.
fn page(body: &str) -> String {
    format!(
        "import QtQuick\nimport QtQuick.Controls.Basic\nRectangle {{\n    id: root\n    \
         width: 640; height: 480\n    color: \"white\"\n{body}\n}}\n"
    )
}

#[track_caller]
fn clean(body: &str) {
    let report = run(&page(body));
    assert!(report.is_clean(), "expected nothing, found:\n{report}");
}

/// The one finding of a component that fails one check.
#[track_caller]
fn one(body: &str, code: Code) -> super::Finding {
    let report = run(&page(body));
    assert_eq!(report.codes(), [code], "\n{report}");
    assert_eq!(report.findings.len(), 1, "\n{report}");
    report.findings[0].clone()
}

#[test]
fn the_codes_are_stable_and_distinct() {
    let names: Vec<&str> = Code::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(
        names,
        [
            "a11y-unnamed-interactive",
            "a11y-unnamed-image",
            "a11y-duplicate-name",
            "a11y-target-size",
            "a11y-tab-trap",
            "a11y-tab-unreachable",
            "a11y-contrast",
            "a11y-set-size",
        ]
    );
    for code in Code::ALL {
        assert_eq!(Code::from_name(code.as_str()), Some(code));
    }
    assert_eq!(Code::from_name("a11y-nothing"), None);
}

const BUTTON: &str = r##"
    Rectangle {
        objectName: "save"
        width: 80; height: 32
        color: "#1565c0"
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        NAME
    }"##;

#[test]
fn something_to_act_on_has_a_name() {
    clean(&BUTTON.replace("NAME", "Accessible.name: \"Save\""));
    let found = one(&BUTTON.replace("NAME", ""), Code::UnnamedInteractive);
    assert!(
        found.message.starts_with("the button with no name"),
        "{found}"
    );
    // The file, and the line and column of the item in it.
    assert_eq!(
        found.place.as_deref(),
        Some("file:///Main.qml:8:5"),
        "{found}"
    );
    assert!(
        found
            .to_string()
            .starts_with("file:///Main.qml:8:5: [a11y-unnamed-interactive] ")
    );
}

#[test]
fn a_label_names_what_it_is_for() {
    clean(
        r##"
    Text { id: label; text: "Name"; color: "black" }
    Rectangle {
        y: 30; width: 80; height: 32
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.labelledBy: label
    }"##,
    );
}

const PICTURE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAIAAACQkWg2AAAAIUlEQVR42mN4n8OAFf3/8RArYhjVQBMNuCRwGTSqgSYaAMXJGZ+uT/rTAAAAAElFTkSuQmCC";

#[test]
fn a_picture_is_named_or_left_out() {
    let image = |more: &str| {
        format!(
            "    Image {{ objectName: \"logo\"; width: 48; height: 48; source: \"{PICTURE}\"; {more} }}"
        )
    };
    clean(&image(
        "Accessible.role: Accessible.Graphic; Accessible.name: \"Ljós\"",
    ));
    clean(&image("Accessible.name: \"Ljós\""));
    clean(&image("Accessible.ignored: true"));
    let found = one(&image(""), Code::UnnamedImage);
    assert!(found.message.contains("\"logo\""), "{found}");
    assert!(
        found
            .place
            .as_deref()
            .is_some_and(|p| p.starts_with("file:///Main.qml:7:"))
    );
    // A role and no name is no better.
    one(
        &image("Accessible.role: Accessible.Graphic"),
        Code::UnnamedImage,
    );
}

#[test]
fn the_icon_of_a_named_button_is_the_button_s() {
    clean(&format!(
        r##"
    Rectangle {{
        width: 48; height: 48
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: "Home"
        Item {{
            anchors.fill: parent
            Image {{ anchors.fill: parent; source: "{PICTURE}" }}
        }}
    }}"##
    ));
}

#[test]
fn something_that_says_it_is_a_picture_needs_a_name_too() {
    let chart = |more: &str| {
        format!(
            "    Rectangle {{ width: 100; height: 60; Accessible.role: Accessible.Chart; {more} }}"
        )
    };
    clean(&chart("Accessible.name: \"Sales by month\""));
    one(&chart(""), Code::UnnamedImage);
}

#[test]
fn two_things_to_act_on_side_by_side_have_names_of_their_own() {
    let two = |a: &str, b: &str| {
        format!(
            r##"
    Row {{
        Rectangle {{
            width: 80; height: 32; activeFocusOnTab: true
            Accessible.role: Accessible.Button; Accessible.name: "{a}"
        }}
        Rectangle {{
            objectName: "second"
            width: 80; height: 32; activeFocusOnTab: true
            Accessible.role: Accessible.Button; Accessible.name: "{b}"
        }}
    }}"##
        )
    };
    clean(&two("Delete draft", "Delete all"));
    let found = one(&two("Delete", "Delete"), Code::DuplicateName);
    assert!(
        found.message.starts_with("the button \"Delete\""),
        "{found}"
    );
    // It names the second and points at the first.
    assert_eq!(found.place.as_deref(), Some("file:///Main.qml:13:9"));
    assert!(found.message.contains("file:///Main.qml:9:9"), "{found}");
}

#[test]
fn text_may_repeat() {
    clean(
        r##"
    Column {
        Text { text: "0"; color: "black" }
        Text { text: "0"; color: "black" }
    }"##,
    );
}

fn sized(w: u32, h: u32) -> String {
    format!(
        r##"
    Rectangle {{
        width: {w}; height: {h}; activeFocusOnTab: true
        Accessible.role: Accessible.Button; Accessible.name: "Close"
    }}"##
    )
}

#[test]
fn something_to_press_is_24_wide_and_high_on_a_desktop() {
    clean(&sized(24, 24));
    let found = one(&sized(30, 20), Code::TargetSize);
    assert!(found.message.contains("is 30 by 20"), "{found}");
    assert!(found.message.contains("24 by 24"), "{found}");
    one(&sized(16, 40), Code::TargetSize);
}

#[test]
fn something_to_press_is_44_wide_and_high_under_a_finger() {
    let report = audit_with(&page(&sized(30, 30)), &Options::mobile());
    assert_eq!(report.codes(), [Code::TargetSize], "\n{report}");
    assert!(report.findings[0].message.contains("44 by 44"));
    let report = audit_with(&page(&sized(44, 44)), &Options::mobile());
    assert!(report.is_clean(), "\n{report}");
}

#[test]
fn what_cannot_be_pressed_may_be_small() {
    clean(
        r##"
    Rectangle {
        width: 30; height: 12
        Accessible.role: Accessible.ProgressBar; Accessible.name: "Done"
    }
    Rectangle {
        y: 20; width: 16; height: 16; enabled: false
        Accessible.role: Accessible.Button; Accessible.name: "Later"
    }"##,
    );
}

/// Three stops; `MORE` goes into the last.
const STOPS: &str = r##"
    Row {
        Rectangle {
            id: a
            width: 60; height: 30; activeFocusOnTab: true
            Accessible.role: Accessible.Button; Accessible.name: "One"
        }
        Rectangle {
            id: b
            width: 60; height: 30; activeFocusOnTab: true
            Accessible.role: Accessible.Button; Accessible.name: "Two"
            SECOND
        }
        Rectangle {
            id: c
            width: 60; height: 30; activeFocusOnTab: true
            Accessible.role: Accessible.Button; Accessible.name: "Three"
            THIRD
        }
    }"##;

fn stops(second: &str, third: &str) -> String {
    STOPS.replace("SECOND", second).replace("THIRD", third)
}

#[test]
fn tab_goes_round_every_stop_once() {
    clean(&stops("", ""));
    // Said in so many words, the same round.
    clean(&stops("KeyNavigation.tab: c", "KeyNavigation.tab: a"));
}

#[test]
fn a_round_that_does_not_come_back_is_a_trap() {
    let found = one(&stops("", "KeyNavigation.tab: b"), Code::TabTrap);
    assert!(
        found
            .message
            .starts_with("Tab from the button \"Three\" goes back to the button \"Two\""),
        "{found}"
    );
    assert!(found.message.contains("the button \"One\""), "{found}");
    assert_eq!(found.place.as_deref(), Some("file:///Main.qml:20:9"));

    let found = one(&stops("KeyNavigation.tab: b", ""), Code::TabTrap);
    assert!(
        found
            .message
            .starts_with("Tab does not leave the button \"Two\""),
        "{found}"
    );
}

#[test]
fn a_stop_that_tab_passes_by_is_never_reached() {
    // One, Two, and back to One: Three is in the order and not on the way.
    let found = one(&stops("KeyNavigation.tab: a", ""), Code::TabUnreachable);
    assert!(found.message.contains("the button \"Three\""), "{found}");
}

#[test]
fn what_takes_the_keyboard_is_a_stop() {
    let button = |more: &str| {
        format!(
            r##"
    Rectangle {{
        width: 60; height: 30; {more}
        Accessible.role: Accessible.Button; Accessible.name: "Go"
    }}"##
        )
    };
    clean(&button("activeFocusOnTab: true"));
    clean(&button("Accessible.focusable: false"));
    let found = one(&button(""), Code::TabUnreachable);
    assert!(
        found
            .message
            .starts_with("the button \"Go\" takes the keyboard"),
        "{found}"
    );
}

#[test]
fn what_is_inside_a_stop_is_reached_through_it() {
    clean(
        r##"
    Rectangle {
        width: 200; height: 40; activeFocusOnTab: true
        Accessible.role: Accessible.ToolBar; Accessible.name: "Tools"
        Rectangle {
            width: 40; height: 40
            Accessible.role: Accessible.Button; Accessible.name: "Cut"
        }
    }"##,
    );
}

#[test]
fn a_stop_says_something_of_itself() {
    let found = one(
        "    Item { objectName: \"well\"; width: 60; height: 30; activeFocusOnTab: true }",
        Code::UnnamedInteractive,
    );
    assert!(
        found.message.starts_with("Tab stops at the Item \"well\""),
        "{found}"
    );
}

fn text_on(colour: &str, behind: &str, more: &str) -> String {
    format!(
        r##"
    Rectangle {{
        width: 200; height: 60; color: "{behind}"
        Text {{ anchors.centerIn: parent; text: "Reading"; color: "{colour}"; {more} }}
    }}"##
    )
}

#[test]
fn text_stands_out_from_what_is_behind_it() {
    clean(&text_on("#212121", "white", ""));
    clean(&text_on("white", "#1565c0", ""));
    // #767676 on white is the lightest grey that passes.
    clean(&text_on("#767676", "white", ""));
    let found = one(&text_on("#999999", "white", ""), Code::Contrast);
    assert!(
        found
            .message
            .starts_with("the text \"Reading\" is #999999 on #ffffff, a contrast of 2.85:1"),
        "{found}"
    );
    assert!(found.message.ends_with("4.5:1 is the least"), "{found}");
    assert_eq!(found.place.as_deref(), Some("file:///Main.qml:10:9"));
}

#[test]
fn large_text_needs_less() {
    // 3.54:1.
    clean(&text_on("#888888", "white", "font.pixelSize: 24"));
    let found = one(
        &text_on("#888888", "white", "font.pixelSize: 23"),
        Code::Contrast,
    );
    assert!(found.message.contains("3.54:1"), "{found}");
    let found = one(
        &text_on("#aaaaaa", "white", "font.pixelSize: 30"),
        Code::Contrast,
    );
    assert!(found.message.ends_with("3:1 is the least"), "{found}");
}

#[test]
fn colours_are_the_ones_shown() {
    // Half-clear black over white is a grey of 3.95:1; black behind it,
    // nearly nothing.
    one(&text_on("#80000000", "white", ""), Code::Contrast);
    // What is painted last is what is behind.
    clean(
        r##"
    Rectangle {
        width: 200; height: 60; color: "white"
        Rectangle { anchors.fill: parent; color: "#1b1b1b" }
        Text { anchors.centerIn: parent; text: "Reading"; color: "white" }
    }"##,
    );
    // The opacity of what the text is in is its own too.
    one(
        r##"
    Item {
        width: 200; height: 60; opacity: 0.4
        Text { anchors.centerIn: parent; text: "Reading"; color: "black" }
    }"##,
        Code::Contrast,
    );
}

#[test]
fn text_that_is_switched_off_is_not_held_to_it() {
    clean(
        r##"
    Item {
        width: 200; height: 60; enabled: false
        Text { anchors.centerIn: parent; text: "Reading"; color: "#bbbbbb" }
    }"##,
    );
}

#[test]
fn where_nothing_is_painted_the_window_shows() {
    let bare = "import QtQuick\nItem { width: 200; height: 60\n    Text { text: \"Reading\"; color: \"white\" }\n}\n";
    let report = run(bare);
    assert_eq!(report.codes(), [Code::Contrast], "\n{report}");
    assert!(report.findings[0].message.contains("#ffffff on #ffffff"));
    let dark = Options {
        backdrop: Some([18, 18, 18]),
        ..Options::desktop()
    };
    assert!(audit_with(bare, &dark).is_clean());
}

const ROWS: &str = r##"
    Column {
        Accessible.role: Accessible.List; Accessible.name: "Files"
        Repeater {
            model: 3
            Rectangle {
                width: 100; height: 30
                Accessible.role: Accessible.ListItem; Accessible.name: "File " + (index + 1)
                Accessible.focusable: false
            }
        }
        MORE
    }"##;

#[test]
fn rows_say_which_row_they_are_or_none_does() {
    clean(&ROWS.replace("MORE", ""));
    // By hand, none of them says: an assistive technology counts them.
    clean(
        r##"
    Column {
        Accessible.role: Accessible.List; Accessible.name: "Files"
        Rectangle {
            width: 100; height: 30
            Accessible.role: Accessible.ListItem; Accessible.name: "One"
            Accessible.focusable: false
        }
        Rectangle {
            width: 100; height: 30
            Accessible.role: Accessible.ListItem; Accessible.name: "Two"
            Accessible.focusable: false
        }
    }"##,
    );
    let found = one(
        &ROWS.replace(
            "MORE",
            r##"Rectangle {
            width: 100; height: 30
            Accessible.role: Accessible.ListItem; Accessible.name: "More"
            Accessible.focusable: false
        }"##,
        ),
        Code::SetSize,
    );
    assert!(
        found
            .message
            .starts_with("the listitem \"More\" does not say its place among rows that do"),
        "{found}"
    );
}

#[test]
fn the_audit_leaves_the_scene_as_it_was() {
    let e = engine();
    let mut s = mount(&e, &page(&stops("", "")));
    assert!(!s.accessibility_active());
    let focus = s.active_focus_item(&e);
    let report = audit(&e, &mut s, Time::ZERO, &Options::desktop());
    assert!(report.is_clean(), "\n{report}");
    assert!(!s.accessibility_active());
    assert_eq!(s.active_focus_item(&e), focus);
    assert!(!ljos_quick::system_prefs(&e).screen_reader_active);

    // And as it was when an assistive technology listens.
    s.set_accessibility_active(&e, true);
    s.frame(&e, Time::ZERO);
    let nodes = s.accessibility_tree().len();
    assert!(audit(&e, &mut s, Time::ZERO, &Options::desktop()).is_clean());
    assert!(s.accessibility_active());
    assert_eq!(s.accessibility_tree().len(), nodes);
}

#[test]
fn a_report_reads_one_finding_to_a_line() {
    let report = run(&page(&sized(10, 10)));
    let text = report.to_string();
    assert_eq!(text.lines().count(), 1);
    assert!(
        text.starts_with(
            "file:///Main.qml:8:5: [a11y-target-size] the button \"Close\" is 10 by 10"
        ),
        "{text}"
    );
    assert!(report.clone().without(&[Code::TargetSize]).is_clean());
    assert_eq!(report.of(Code::TargetSize).count(), 1);
}

/// Every control of the Basic style that there is, named as an author
/// would name it.
const STYLE: &str = r##"
    Column {
        x: 8; y: 8; spacing: 8
        ToolBar {
            width: 600
            Row {
                ToolButton { text: "Back" }
                ToolButton { text: "Forward" }
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
            ScrollView {
                width: 160; height: 80
                TextArea { Accessible.name: "Notes"; text: "one\ntwo\nthree\nfour\nfive\nsix" }
            }
        }
    }"##;

#[test]
fn the_basic_style_is_clean() {
    clean(STYLE);
}

/// The one thing of the style's the audit holds against it. The colour
/// is the one Qt's Basic style has (`features/controls/control-palette.feature`
/// pins the palette to it), so it stays until a style of our own says
/// otherwise; this test is where that is written down.
#[test]
fn the_placeholder_of_the_basic_style_is_fainter_than_the_audit_allows() {
    let report = run(&page(
        r##"
    TextField { Accessible.name: "Name"; placeholderText: "Given name" }"##,
    ));
    assert_eq!(report.codes(), [Code::Contrast], "\n{report}");
    let f = &report.findings[0];
    assert!(
        f.place.as_deref().unwrap().contains("Basic/TextField.qml:"),
        "{f}"
    );
    assert!(f.message.contains("#939494 on #ffffff"), "{f}");
}

/// What is in a popup is audited while the popup is open.
#[test]
fn the_popups_of_the_basic_style_are_clean() {
    for (what, body) in [
        (
            "combo box",
            r##"
    ComboBox {
        Accessible.name: "Paper"
        model: ["A4", "A5", "Letter"]
        Component.onCompleted: popup.open()
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
    ] {
        let report = run(&page(body));
        assert!(
            report.is_clean(),
            "{what}: expected nothing, found:\n{report}"
        );
    }
}

#[test]
fn what_is_wrong_in_an_open_popup_is_found() {
    let f = one(
        r##"
    Dialog {
        title: "Discard the changes?"
        Component.onCompleted: open()
        Button { width: 60 }
    }"##,
        Code::UnnamedInteractive,
    );
    assert_eq!(f.place.as_deref(), Some("file:///Main.qml:11:9"));
}

#[test]
fn tab_goes_round_inside_an_open_popup() {
    // The stops of a popup are a round of their own: one that Tab is sent
    // past is found there as it is in the window.
    let f = one(
        r##"
    Button { text: "Under" }
    Dialog {
        title: "Three"
        Component.onCompleted: open()
        Row {
            Button { id: a; text: "First"; KeyNavigation.tab: c }
            Button { text: "Second" }
            Button { id: c; text: "Third" }
        }
    }"##,
        Code::TabUnreachable,
    );
    assert!(f.message.contains("\"Second\""), "{f}");
}

/// The demo of the shell is the application the style is tried on: what
/// the audit finds there is the demo's own to name, and nothing of the
/// style's.
#[test]
fn the_demo_of_the_shell_has_nothing_of_the_styles() {
    let e = engine();
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../ljos_shell/examples/qml/Main.qml"
    ));
    let root = load(&e, source);
    let mut s = QuickScene::new(kurbo::Size::new(900.0, 600.0));
    s.set_root(&e, root).unwrap();
    s.frame(&e, Time::ZERO);
    let report = audit(&e, &mut s, Time::ZERO, &Options::desktop());
    for f in &report.findings {
        let place = f.place.as_deref().unwrap_or_default();
        assert!(
            place.contains("Main.qml:"),
            "a finding of the style's:\n{report}"
        );
        assert!(
            matches!(f.code, Code::UnnamedInteractive | Code::UnnamedImage),
            "the demo is missing names and nothing else:\n{report}"
        );
    }
    eprintln!("the demo of the shell:\n{report}");
}
