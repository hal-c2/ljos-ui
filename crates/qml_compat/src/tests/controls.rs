//! Tests of the controls: `QtQuick.Controls` lowered to `Ljos.Controls.Basic`.

use super::*;

#[test]
fn qt_controls_are_lowered_to_ljos_controls() {
    let lowered_text = "pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Controls.Basic\nItem {\n  Button { text: \"Save\"; ToolTip.text: \"Writes\" }\n  Label { color: \"red\" }\n}\n";
    for uri in QT_CONTROLS {
        let source = format!(
            "import QtQuick\nimport {uri} 2.15\nItem {{\n  Button {{ text: \"Save\"; ToolTip.text: \"Writes\" }}\n  Label {{ color: \"red\" }}\n}}\n"
        );
        let Outcome::Lowered(done) = in_style(&source, "Basic") else {
            panic!("{uri} is not lowered");
        };
        assert_eq!(done.text, lowered_text, "{uri}");
    }
}

#[test]
fn the_basic_style_is_lowered_whatever_style_the_program_starts_with() {
    let source = "import QtQuick\nimport QtQuick.Controls.Basic\nLabel { }\n";
    assert!(matches!(in_style(source, "Fusion"), Outcome::Lowered(_)));
    assert!(matches!(lower(source, &no_locals), Outcome::Lowered(_)));
}

#[test]
fn the_controls_of_another_style_or_of_one_not_known_keep_the_file() {
    let source = "import QtQuick\nimport QtQuick.Controls\nLabel { }\n";
    let Outcome::Kept(kept) = in_style(source, "Fusion") else {
        panic!("lowered in Fusion");
    };
    assert_eq!(
        kept.to_string(),
        "it imports `QtQuick.Controls`, which is the Fusion style where it runs, and Ljos.Controls.Basic is Basic"
    );
    assert_eq!(kept.at, "import QtQuick\n".len());
    assert_eq!(
        kept_for(source),
        "it imports `QtQuick.Controls`, whose style is chosen as the program starts"
    );
}

#[test]
fn both_imports_of_qt_controls_are_one_of_ljos_controls() {
    let Outcome::Lowered(done) = in_style(
        "import QtQuick\nimport QtQuick.Controls\nimport QtQuick.Controls.Basic\nLabel { }\n",
        "Basic",
    ) else {
        panic!("not lowered");
    };
    assert_eq!(
        done.text,
        "pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Controls.Basic\n\nLabel { }\n"
    );
}

#[test]
fn a_control_ljos_controls_has_not_taken_keeps_the_file() {
    assert_eq!(
        kept_for("import QtQuick\nimport QtQuick.Controls.Basic\nItem { Dial { } }\n"),
        "`Dial` is not in Ljos.Controls.Basic yet"
    );
    assert_eq!(
        kept_for(
            "import QtQuick\nimport QtQuick.Controls.Basic\nLabel { property int m: Dial.SnapAlways }\n"
        ),
        "`Dial` is not in Ljos.Controls.Basic yet"
    );
}

#[test]
fn the_controls_under_a_name_or_a_style_of_their_own_keep_the_file() {
    assert_eq!(
        kept_for("import QtQuick\nimport QtQuick.Controls.Basic as C\nC.Button { }\n"),
        "it imports `QtQuick.Controls.Basic` under a name"
    );
    assert_eq!(
        kept_for("import QtQuick\nimport QtQuick.Controls.Material\nButton { }\n"),
        "it imports `QtQuick.Controls.Material`, which is not lowered yet"
    );
}

#[test]
fn a_rectangle_a_control_is_given_is_still_a_fill() {
    assert_eq!(
        lowered(
            "import QtQuick\nimport QtQuick.Controls.Basic\nButton { background: Rectangle { color: \"red\" } }\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Controls.Basic\nButton { background: Rectangle { fill: \"red\" } }\n"
    );
    // What the style gave is an Item: it may be a Rectangle.
    assert_eq!(
        kept_for(
            "import QtQuick\nimport QtQuick.Controls.Basic\nButton { id: b; onClicked: b.background.color = \"red\" }\n"
        ),
        "`.color` is read off something whose type is not known"
    );
}

/// Every control a lowered file may name is one `Ljos.Controls.Basic` has, as
/// Qt's controls describe it.
#[test]
fn ljos_controls_has_what_qt_controls_have() {
    let ljos = qt_types::module(CONTROLS).expect("Ljos.Controls.Basic is described");
    let qt = qt_types::module("QtQuick.Controls.Basic").expect("described");
    for name in [
        "AbstractButton",
        "Button",
        "CheckBox",
        "ComboBox",
        "Dialog",
        "ItemDelegate",
        "Label",
        "Menu",
        "MenuItem",
        "MenuSeparator",
        "Popup",
        "ScrollBar",
        "ScrollView",
        "Slider",
        "SpinBox",
        "Switch",
        "TextArea",
        "TextField",
        "ToolTip",
        "Control",
        "Pane",
        "Frame",
        "GroupBox",
        "Page",
        "ToolBar",
        "ToolButton",
        "RoundButton",
        "DelayButton",
    ] {
        assert!(ljos.type_named(name).is_some(), "{name}");
        assert!(qt.type_named(name).is_some(), "{name}");
    }
}

/// What a lowered file of the controls is: the Basic imports become
/// Ljós's, and every line under them stays as written.
fn lowered_unchanged(source: &str) {
    let Outcome::Lowered(done) = in_style(source, "Basic") else {
        panic!("not lowered: {:?}", in_style(source, "Basic"));
    };
    let body = source
        .split_once("import QtQuick.Controls")
        .map(|(_, rest)| rest);
    let body = body
        .and_then(|rest| rest.split_once('\n'))
        .map(|(_, rest)| rest);
    assert_eq!(
        done.text,
        format!(
            "pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Controls.Basic\n{}",
            body.expect("a controls import")
        )
    );
}

#[test]
fn panes_frames_pages_and_bare_controls_are_lowered() {
    // The `Rectangle` a control is given is still a fill.
    assert_eq!(
        lowered(
            "import QtQuick\nimport QtQuick.Controls.Basic\nPage {\n  title: \"Notes\"\n  \
             header: Label { text: title }\n  \
             Pane { padding: 12; Frame { GroupBox { title: \"Size\" } } }\n  \
             Control { contentItem: Rectangle { color: \"red\" } }\n}\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Controls.Basic\nPage {\n  title: \"Notes\"\n  \
         header: Label { text: title }\n  \
         Pane { padding: 12; Frame { GroupBox { title: \"Size\" } } }\n  \
         Control { contentItem: Rectangle { fill: \"red\" } }\n}\n"
    );
}

#[test]
fn tool_round_and_delay_buttons_are_lowered() {
    lowered_unchanged(
        "import QtQuick\nimport QtQuick.Controls.Basic\nItem {\n  \
         ToolButton { text: \"Close\"; display: AbstractButton.IconOnly; icon.name: \"window-close\" }\n  \
         RoundButton { text: \"+\"; radius: 8 }\n  \
         DelayButton { text: \"Hold\"; delay: 800; onActivated: text = \"Done\" }\n}\n",
    );
}
