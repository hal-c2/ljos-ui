//! Each check with a component that passes and one that fails, and the
//! audit over the Basic style.

use ljos_engine::{Engine, ObjectId};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::Time;

use super::{Options, Report, audit};

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

/// What the hand-made stops here leave out on purpose: they draw no
/// focus, which `focus-indicator-missing` reports, and its own fixtures
/// test.
const BARE: [&str; 1] = ["focus-indicator-missing"];

fn audit_with(source: &str, options: &Options) -> Report {
    let e = engine();
    let mut s = mount(&e, source);
    let report = audit(&e, &mut s, Time::ZERO, options).without(&BARE);
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
fn one(body: &str, code: &str) -> super::Finding {
    let report = run(&page(body));
    assert_eq!(report.codes(), [code], "\n{report}");
    assert_eq!(report.findings.len(), 1, "\n{report}");
    report.findings[0].clone()
}

#[test]
fn every_code_is_a_rule_of_ljos_design_checked_at_run_time() {
    for code in crate::rules::implemented() {
        let rule = ljos_design::rule(code).unwrap_or_else(|| panic!("{code} is no rule"));
        assert_ne!(rule.check, ljos_design::Check::Static, "{code}");
    }
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
    let found = one(&BUTTON.replace("NAME", ""), "a11y-unnamed-interactive");
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
    let found = one(&image(""), "a11y-unnamed-image");
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
        "a11y-unnamed-image",
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
    one(&chart(""), "a11y-unnamed-image");
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
    let found = one(&two("Delete", "Delete"), "a11y-duplicate-name");
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
    let found = one(&sized(30, 20), "a11y-target-size");
    assert!(found.message.contains("is 30 by 20"), "{found}");
    assert!(found.message.contains("24 by 24"), "{found}");
    one(&sized(16, 40), "a11y-target-size");
}

#[test]
fn something_to_press_is_44_wide_and_high_under_a_finger() {
    let report = audit_with(&page(&sized(30, 30)), &Options::mobile());
    assert_eq!(report.codes(), ["a11y-target-size"], "\n{report}");
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
        Accessible.description: "Comes on once the upload is done"
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
    let found = one(&stops("", "KeyNavigation.tab: b"), "a11y-tab-trap");
    assert!(
        found
            .message
            .starts_with("Tab from the button \"Three\" goes back to the button \"Two\""),
        "{found}"
    );
    assert!(found.message.contains("the button \"One\""), "{found}");
    assert_eq!(found.place.as_deref(), Some("file:///Main.qml:20:9"));

    let found = one(&stops("KeyNavigation.tab: b", ""), "a11y-tab-trap");
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
    let found = one(&stops("KeyNavigation.tab: a", ""), "a11y-tab-unreachable");
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
    let found = one(&button(""), "a11y-tab-unreachable");
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
        "a11y-unnamed-interactive",
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
    let found = one(&text_on("#999999", "white", ""), "color-text-contrast");
    assert!(
        found
            .message
            .starts_with("the text \"Reading\" is #999999 on #ffffff, a contrast of 2.85:1"),
        "{found}"
    );
    assert!(
        found.message.contains("4.5:1 is the least (APCA Lc "),
        "{found}"
    );
    assert_eq!(found.place.as_deref(), Some("file:///Main.qml:10:9"));
}

#[test]
fn large_text_needs_less() {
    // 3.54:1.
    clean(&text_on("#888888", "white", "font.pixelSize: 24"));
    let found = one(
        &text_on("#888888", "white", "font.pixelSize: 23"),
        "color-text-contrast",
    );
    assert!(found.message.contains("3.54:1"), "{found}");
    let found = one(
        &text_on("#aaaaaa", "white", "font.pixelSize: 30"),
        "color-text-contrast",
    );
    assert!(
        found.message.contains("3:1 is the least (APCA Lc "),
        "{found}"
    );
}

#[test]
fn colours_are_the_ones_shown() {
    // Half-clear black over white is a grey of 3.95:1; black behind it,
    // nearly nothing.
    one(&text_on("#80000000", "white", ""), "color-text-contrast");
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
        "color-text-contrast",
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
    assert_eq!(report.codes(), ["color-text-contrast"], "\n{report}");
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
        "a11y-set-size",
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
    let report = audit(&e, &mut s, Time::ZERO, &Options::desktop()).without(&BARE);
    assert!(report.is_clean(), "\n{report}");
    assert!(!s.accessibility_active());
    assert_eq!(s.active_focus_item(&e), focus);
    assert!(!ljos_quick::system_prefs(&e).screen_reader_active);

    // And as it was when an assistive technology listens.
    s.set_accessibility_active(&e, true);
    s.frame(&e, Time::ZERO);
    let nodes = s.accessibility_tree().len();
    assert!(
        audit(&e, &mut s, Time::ZERO, &Options::desktop())
            .without(&BARE)
            .is_clean()
    );
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
    assert!(report.clone().without(&["a11y-target-size"]).is_clean());
    assert_eq!(report.of("a11y-target-size").count(), 1);
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
            Button { text: "Off"; enabled: false; Accessible.description: "Nothing to turn off" }
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
    let e = engine();
    let mut s = mount(
        &e,
        &page(
            r##"
    TextField { Accessible.name: "Name"; placeholderText: "Given name" }"##,
        ),
    );
    let report = audit(&e, &mut s, Time::ZERO, &Options::desktop());
    assert_eq!(report.codes(), ["color-text-contrast"], "\n{report}");
    let f = &report.findings[0];
    // The style's text, found where the application can fix it: its
    // field, with `placeholderTextColor`.
    let own = f.item.and_then(|i| crate::cx::place_of(&e, i)).unwrap();
    assert!(own.contains("Basic/TextField.qml:"), "{own}");
    assert!(f.place.as_deref().unwrap().contains("Main.qml:8:"), "{f}");
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
        MenuItem { text: "Copy"; enabled: false; Accessible.description: "Select something first" }
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
        Button {}
    }"##,
        "a11y-unnamed-interactive",
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
        "a11y-tab-unreachable",
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
    // As a viewer has it: the root fills the window.
    e.set(root, ljos_quick::Item::WIDTH, 900.0).unwrap();
    e.set(root, ljos_quick::Item::HEIGHT, 600.0).unwrap();
    let mut s = QuickScene::new(kurbo::Size::new(900.0, 600.0));
    s.set_root(&e, root).unwrap();
    s.frame(&e, Time::ZERO);
    let report = audit(&e, &mut s, Time::ZERO, &Options::desktop());
    for f in &report.findings {
        let place = f.place.as_deref().unwrap_or_default();
        let own = f.item.and_then(|i| crate::cx::place_of(&e, i));
        // Known (ARCHITECTURE.md section 14): the Basic style's placeholder
        // grey is Qt's palette value, 3.04:1 on white.
        let style = own.as_deref().unwrap_or_default();
        if f.code == "color-text-contrast"
            && (style.contains("Basic/TextField.qml") || style.contains("Basic/TextArea.qml"))
        {
            assert!(f.message.contains("3.04:1"), "{f}");
            continue;
        }
        assert!(
            place.contains("Main.qml:") && own.as_deref() == Some(place),
            "a finding of the style's:\n{report}"
        );
        // The demo's icon-only "Add" button has no tooltip, and its "Go"
        // menu is sixteen lines with no group: the demo's too.
        assert!(
            matches!(
                f.code,
                "a11y-unnamed-interactive"
                    | "a11y-unnamed-image"
                    | "a11y-icon-button-no-tooltip"
                    | "layout-menu-ungrouped"
            )
                // The demo's button draws its own background, and no focus.
                || (f.code == "focus-indicator-missing" && f.message.contains("Click me")),
            "the demo is missing names and nothing else:\n{report}"
        );
    }
    eprintln!("the demo of the shell:\n{report}");
}

/// Mounted as a viewer mounts it: the root fills the window.
fn hosted(e: &Engine, source: &str, w: f64, h: f64) -> QuickScene {
    let root = load(e, source);
    e.set(root, ljos_quick::Item::WIDTH, w).unwrap();
    e.set(root, ljos_quick::Item::HEIGHT, h).unwrap();
    let mut s = QuickScene::new(kurbo::Size::new(w, h));
    s.set_root(e, root).unwrap();
    s.frame(e, Time::ZERO);
    s
}

const ROW_OF_FOUR: &str = r##"import QtQuick
import QtQuick.Controls.Basic
Rectangle {
    color: "white"
    Row {
        objectName: "row"
        spacing: 8
        Button { text: "Open the file" }
        Button { text: "Save the file" }
        Button { text: "Print the file" }
        Button { objectName: "last"; text: "Close the file" }
    }
}
"##;

#[test]
fn what_shows_only_at_a_larger_font_shows_once_with_its_points() {
    let e = engine();
    // The row is 424 px wide, and 621 px at font scale 2.
    let mut s = hosted(&e, ROW_OF_FOUR, 560.0, 420.0);
    let now = audit(&e, &mut s, Time::ZERO, &Options::desktop());
    assert!(now.is_clean(), "\n{now}");
    let options = Options {
        matrix: crate::Matrix::Full,
        ..Options::desktop()
    }
    .only(&["layout-offscreen"])
    .unwrap();
    let report = audit(&e, &mut s, Time::ZERO, &options);
    let last: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.locator.as_deref() == Some("#last"))
        .collect();
    assert_eq!(last.len(), 1, "one finding however many points:\n{report}");
    let points: Vec<String> = last[0].points.iter().map(ToString::to_string).collect();
    assert!(!points.iter().any(|p| p == "current"), "{points:?}");
    assert!(points.iter().any(|p| p == "font-scale=2"), "{points:?}");
    assert!(
        points.iter().any(|p| p.starts_with("size=small")),
        "{points:?}"
    );
    assert_eq!(report.points.len(), crate::Matrix::full_points().len());
}

#[test]
fn a_larger_font_is_not_a_shift() {
    let e = engine();
    let mut s = hosted(&e, ROW_OF_FOUR, 560.0, 420.0);
    let options = Options {
        matrix: crate::Matrix::Full,
        before: Some(std::sync::Arc::new(crate::Placement::of(&e, &s))),
        ..Options::desktop()
    }
    .only(&["layout-shift"])
    .unwrap();
    let report = audit(&e, &mut s, Time::ZERO, &options);
    assert!(report.is_clean(), "\n{report}");
}

#[test]
fn a_full_audit_leaves_the_scene_as_it_was() {
    let e = engine();
    let source = r##"import QtQuick
import QtQuick.Controls.Basic
Rectangle {
    id: root
    color: palette.window
    property real wide: label.contentWidth
    Label { id: label; text: "Settings"; width: root.width / 2 }
    Rectangle {
        objectName: "fading"
        width: 40; height: 40
        color: root.width > 600 ? "red" : "blue"
        Behavior on color { ColorAnimation { duration: 400 } }
    }
}
"##;
    let mut s = hosted(&e, source, 640.0, 480.0);
    // A setting the application made, which a later report of the host's
    // does not undo.
    ljos_quick::update_system_prefs(&e, |p| p.contrast = ljos_a11y::Contrast::High);
    s.frame(&e, Time::ZERO);
    let root = s.root().unwrap();
    let read = |name: &str| e.read_by_name(root, name).to_number();
    let before = (
        ljos_quick::system_prefs(&e),
        ljos_quick::hold_system_prefs(&e),
        s.viewport(),
        read("width"),
        read("height"),
        read("wide"),
    );
    let current = audit(&e, &mut s, Time::ZERO, &Options::desktop());
    let options = Options {
        matrix: crate::Matrix::Full,
        ..Options::desktop()
    };
    audit(&e, &mut s, Time::ZERO, &options);
    let after = (
        ljos_quick::system_prefs(&e),
        ljos_quick::hold_system_prefs(&e),
        s.viewport(),
        read("width"),
        read("height"),
        read("wide"),
    );
    assert_eq!(before, after);
    // Turning the accessibility tree back off asks for one frame; after it
    // nothing is left animating.
    s.frame(&e, Time::ZERO);
    assert!(!s.needs_frame());
    let again = audit(&e, &mut s, Time::ZERO, &Options::desktop());
    assert_eq!(current.to_string(), again.to_string());
    // The label still follows the root: its binding was not broken.
    e.set(root, ljos_quick::Item::WIDTH, 400.0).unwrap();
    s.frame(&e, Time::ZERO);
    let label = ljos_quick::child_items(&e, root)[0];
    assert_eq!(e.read_by_name(label, "width").to_number(), 200.0);
}

/// An outline sets text off from what is behind it: the text is held to
/// the outline's colour, and the outline is no text of its own.
#[test]
fn a_styled_text_is_held_to_the_colour_about_its_glyphs() {
    clean(
        r##"
    Text { text: "Set off"; color: "white"; style: Text.Outline; styleColor: "black" }"##,
    );
    let report = run(&page(
        r##"
    Text { text: "Faint"; color: "white"; style: Text.Raised; styleColor: "#eeeeee" }"##,
    ));
    assert_eq!(report.codes(), ["color-text-contrast"], "\n{report}");
    assert!(
        report.findings[0].message.contains("on #eeeeee"),
        "{report}"
    );
}

/// What a module Ljós does not have would show is not known, so text on
/// nothing but the window is not held to the window's colour, and the
/// report says what it did not see.
#[test]
fn text_over_a_view_ljos_does_not_have_is_left_alone() {
    let e = engine();
    let mut s = mount(
        &e,
        r##"
import QtQuick
import QtQuick3D
Item {
    width: 400; height: 300
    View3D { anchors.fill: parent }
    Text { text: "Tap to throw"; color: "white" }
}
"##,
    );
    let report = audit(&e, &mut s, Time::ZERO, &Options::desktop()).without(&BARE);
    assert!(report.is_clean(), "\n{report}");
    assert_eq!(report.unseen, ["QtQuick3D"]);
    assert!(
        report
            .by_rule("")
            .contains("Not audited: what the items of QtQuick3D show")
    );
}

#[test]
fn a_page_of_a_swipe_view_that_is_not_shown_is_not_judged() {
    // The buttons of the pages past the current one are on the Tab round
    // and outside the view's clip, where Ljós describes nothing.
    let report = run(
        "import QtQuick\nimport QtQuick.Controls.Basic\nimport QtQuick.Layouts\n\
         Rectangle {\n    width: 400; height: 300\n    color: \"white\"\n    \
         SwipeView {\n        anchors.fill: parent\n        clip: true\n        \
         Repeater {\n            model: 3\n            Pane {\n                \
         RowLayout {\n                    anchors.fill: parent\n                    \
         Repeater {\n                        model: [\"Heat\", \"Cool\"]\n                        \
         Button { text: modelData; Layout.fillWidth: true }\n                    }\n                \
         }\n            }\n        }\n    }\n}\n",
    );
    assert!(report.is_clean(), "{report}");
}

#[test]
fn a_page_is_a_pane_and_a_tab_button_a_tab() {
    // Both have the role of a tab; only the button is pressed.
    let report = run("import QtQuick\nimport QtQuick.Controls.Basic\n\
         Page {\n    width: 400; height: 300\n    \
         Button { x: 20; y: 20; text: \"Save\" }\n}\n");
    assert!(report.is_clean(), "{report}");
    let f = one(
        "    TabBar { x: 20; y: 20; width: 200\n        TabButton { icon.source: \"\" }\n    }",
        "a11y-unnamed-interactive",
    );
    assert!(f.message.contains("tab"), "{}", f.message);
}

#[test]
fn what_cannot_be_seen_is_not_in_the_accessibility_tree() {
    let f = one(
        "    Button { x: 20; y: 20; text: \"Delete\"; opacity: 0 }",
        "a11y-hidden-exposed",
    );
    assert!(f.message.contains("opacity of 0"), "{}", f.message);
    // Squeezed to nothing: hidden, not a small target.
    let f = one(
        "    Button { x: 20; y: 20; text: \"Settings\"; width: 0; height: 0 }",
        "a11y-hidden-exposed",
    );
    assert!(
        f.message.contains("no area") || f.message.contains("an area"),
        "{}",
        f.message
    );
    // Under a transparent parent, once, for the outer node.
    one(
        "    Rectangle { x: 20; y: 20; width: 200; height: 60; opacity: 0\n        \
         Accessible.role: Accessible.Grouping; Accessible.name: \"Tools\"\n        \
         Button { text: \"Cut\" }\n    }",
        "a11y-hidden-exposed",
    );
    clean("    Button { x: 20; y: 20; text: \"Delete\"; visible: false }");
    // Left out of the tree, it is still a stop of the Tab round, which
    // the rule of an ignored stop says.
    let report = run(&page(
        "    Button { x: 20; y: 20; text: \"Delete\"; opacity: 0; Accessible.ignored: true }",
    ));
    assert_eq!(report.codes(), ["a11y-ignored-focusable"], "\n{report}");
    // A box of no size whose content hangs out of it is seen.
    clean("    Item { x: 20; y: 20\n        Button { text: \"Open\" }\n    }");
}

#[test]
fn a_tap_handler_is_a_click_an_assistive_technology_must_find() {
    let f = one(
        "    Rectangle { x: 20; y: 20; width: 160; height: 40; color: \"#eeeeee\"\n        \
         TapHandler { onTapped: console.log(\"open\") }\n    }",
        "a11y-unnamed-interactive",
    );
    assert!(f.message.contains("TapHandler"), "{}", f.message);
    assert!(f.bounds.is_some_and(|b| b.width == 160.0));
    // Only the secondary button: a second way to what is offered.
    clean(
        "    Rectangle { x: 20; y: 20; width: 160; height: 40; color: \"#eeeeee\"\n        \
         TapHandler { acceptedButtons: Qt.RightButton; onTapped: console.log(\"menu\") }\n    }",
    );
}

/// Each flip puts back every button: checking one of an exclusive group
/// unchecked another, and that one is checked again.
#[test]
fn an_exclusive_group_keeps_its_choice_after_the_audit() {
    let e = engine();
    let mut s = mount(
        &e,
        &page(
            "    Column { x: 20; y: 20\n        \
             RadioButton { objectName: \"first\"; text: \"First\"; checked: true }\n        \
             RadioButton { objectName: \"second\"; text: \"Second\" }\n    }",
        ),
    );
    let root = s.root().unwrap();
    let column = ljos_quick::child_items(&e, root)[0];
    let radios = ljos_quick::child_items(&e, column);
    let checked = |i: usize| e.read_by_name(radios[i], "checked").to_boolean();
    for code in ["motion-duration", "color-state-only"] {
        let options = Options::desktop().only(&[code]).unwrap();
        audit(&e, &mut s, Time::ZERO, &options);
        assert!(checked(0) && !checked(1), "after {code}");
    }
}

/// Focus is put back as the focus chain gives it, not assigned: a binding
/// on `focus` still decides afterwards.
#[test]
fn a_binding_on_focus_outlives_the_audit() {
    let e = engine();
    let mut s = mount(
        &e,
        &page(
            "    property bool editing: true\n    \
             TextField { objectName: \"field\"; x: 20; y: 20; width: 200; focus: root.editing }\n    \
             Button { x: 20; y: 80; text: \"Save\" }",
        ),
    );
    let root = s.root().unwrap();
    let field = ljos_quick::child_items(&e, root)[0];
    let focus = || e.read_by_name(field, "focus").to_boolean();
    assert!(focus());
    audit(&e, &mut s, Time::ZERO, &Options::desktop());
    assert!(focus());
    let editing = e.property_index(root, "editing").unwrap();
    for on in [false, true, false] {
        let _ = e.write(
            root,
            editing,
            ljos_engine::Value::Bool(on),
            ljos_engine::WriteKind::Imperative,
        );
        s.frame(&e, Time::ZERO);
        assert_eq!(focus(), on, "editing: {on}");
    }
}

/// The rules that time a popup's ways out and in share one closing and
/// opening of it.
#[test]
fn an_audit_opens_each_popup_again_once() {
    let e = engine();
    let mut s = mount(
        &e,
        &page(
            "    property int opens: 0\n    \
             Popup {\n        x: 20; y: 20; width: 200; height: 100; visible: true\n        \
             onOpened: root.opens += 1\n        \
             enter: Transition { NumberAnimation { property: \"opacity\"; from: 0; to: 1; duration: 100 } }\n    \
             }",
        ),
    );
    let root = s.root().unwrap();
    let opens = || e.read_by_name(root, "opens").to_number();
    let options = Options::desktop().only(&["motion"]).unwrap();
    audit(&e, &mut s, Time::ZERO, &options);
    let first = opens();
    assert!(first >= 1.0, "the popup opened when the scene began");
    audit(&e, &mut s, Time::ZERO, &options);
    assert_eq!(opens() - first, 1.0, "one audit opens the popup once more");
}

/// A node of an item's that draws nothing is passed by, and its fill is
/// found on the next.
#[test]
fn an_item_whose_first_node_draws_nothing_has_its_fill_found() {
    let e = engine();
    let s = mount(
        &e,
        &page("    Rectangle { x: 20; y: 20; width: 100; height: 60; color: \"#336699\" }"),
    );
    let root = s.root().unwrap();
    let panel = ljos_quick::child_items(&e, root)[0];
    let snapshot = s.snapshot().unwrap();
    let at = snapshot
        .items
        .iter()
        .position(|p| s.item_of(p.node) == Some(panel))
        .unwrap();
    let mut items = snapshot.items.clone();
    let mut bare = items[at].clone();
    bare.list = None;
    items.insert(at, bare);
    let two_nodes = ljos_render::FrameSnapshot {
        frame: snapshot.frame,
        viewport: snapshot.viewport,
        items,
        clips: snapshot.clips.clone(),
        stats: snapshot.stats,
    };
    let options = Options::desktop();
    let cx = crate::cx::Cx::new(&e, &s, &options);
    let (fill, _) = crate::surfaces::own_rect(&cx, &two_nodes, panel).expect("its fill");
    assert_eq!(
        [fill.r, fill.g, fill.b, fill.a].map(|c| (c * 255.0).round() as u8),
        [0x33, 0x66, 0x99, 0xff]
    );
}

/// A check box, so that the acting checks frame the scene later than the
/// audit's `now`, and a bar that grows over a second once the scheme is
/// dark. `grown` is how far it got while the scheme was dark.
const GROWS_WHEN_DARK: &str = r#"
import QtQuick
import QtQuick.Controls
Rectangle {
    id: root
    width: 640; height: 480
    color: "white"
    property bool dark: Qt.styleHints.colorScheme === Qt.ColorScheme.Dark
    property real grown: 0
    CheckBox { text: "Remember me" }
    Rectangle {
        y: 100
        height: 20
        color: "black"
        width: root.dark ? 600 : 100
        Behavior on width { NumberAnimation { duration: 1000 } }
        onWidthChanged: if (root.dark) root.grown = Math.max(root.grown, width)
    }
}
"#;

#[test]
fn what_a_later_point_starts_runs_on_from_where_the_checks_left_the_clock() {
    use std::{cell::RefCell, rc::Rc};
    let e = engine();
    let mut s = mount(&e, GROWS_WHEN_DARK);
    // Every time the scene is framed at, in order.
    let times = Rc::new(RefCell::new(Vec::new()));
    let seen = times.clone();
    s.set_timer_hook(Box::new(move |_, now| {
        seen.borrow_mut().push(now);
        None
    }));
    let options = Options {
        matrix: crate::Matrix::Points(vec![
            crate::MatrixPoint::CURRENT,
            "scheme=dark reduced-motion=false".parse().unwrap(),
        ]),
        ..Options::desktop()
    };
    let report = audit(&e, &mut s, Time::ZERO, &options);
    let times = times.borrow();
    assert!(report.until > Time::ZERO, "the acting checks let time pass");
    assert!(
        times.windows(2).all(|w| w[0] <= w[1]),
        "the scene's clock never goes back: {times:?}"
    );
    assert_eq!(times.last(), Some(&report.until), "{times:?}");
    let grown = e.read_by_name(s.root().unwrap(), "grown").to_number();
    assert!(
        grown > 100.0,
        "the bar grew while the scheme was dark: {grown}"
    );
}

/// Not let act, the audit runs none of the application's handlers, and
/// the report says which codes that left unchecked instead of finding
/// nothing of them.
#[test]
fn an_audit_not_let_act_runs_no_handler_and_says_what_it_did_not_check() {
    let e = engine();
    let mut s = mount(
        &e,
        &page(
            "    property int ran: 0\n    \
             CheckBox { x: 20; y: 20; text: \"Sync\"; onCheckedChanged: root.ran++ }",
        ),
    );
    let root = s.root().unwrap();
    let ran = || e.read_by_name(root, "ran").to_number();
    let options = Options {
        act: false,
        ..Options::desktop()
    };
    let report = audit(&e, &mut s, Time::ZERO, &options);
    assert_eq!(ran(), 0.0, "a handler ran");
    assert!(
        report
            .not_checked
            .iter()
            .all(|c| !report.codes().contains(c)),
        "\n{report}"
    );
    let mut acting: Vec<&str> = crate::rules::ACTS
        .iter()
        .flat_map(|c| c.codes.iter().copied())
        .collect();
    acting.sort_unstable();
    assert_eq!(report.not_checked, acting);
    assert!(
        report.by_rule("").contains("Not checked: "),
        "{}",
        report.by_rule("")
    );
    assert_eq!(
        report.json()["notChecked"].as_array().map(Vec::len),
        Some(acting.len())
    );

    let only = Options {
        act: false,
        ..Options::desktop()
            .only(&["motion-duration", "a11y-unnamed-interactive"])
            .unwrap()
    };
    assert_eq!(
        audit(&e, &mut s, Time::ZERO, &only).not_checked,
        ["motion-duration"]
    );

    let report = audit(&e, &mut s, Time::ZERO, &Options::desktop());
    assert!(ran() > 0.0, "the acting checks toggled the box");
    assert!(report.not_checked.is_empty());
}
