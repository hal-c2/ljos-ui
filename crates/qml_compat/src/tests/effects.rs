//! Tests of Qt's effects: `MultiEffect` and `RectangularShadow`.

use super::*;

/// `Ljos.Ui` has Qt's effects as they are: the import's line is left
/// empty, and nothing of the effects changes.
#[test]
fn a_file_of_effects_lowers_with_its_effects_as_they_are() {
    let source = "import QtQuick\n\
import QtQuick.Effects\n\
Item {\n\
    Image { id: photo; source: \"a.png\"; visible: false }\n\
    MultiEffect { source: photo; anchors.fill: photo; blurEnabled: true; blur: 0.5 }\n\
    RectangularShadow { width: 10; height: 10; radius: 4; color: \"black\" }\n\
}\n";
    assert_eq!(
        lowered(source),
        format!(
            "pragma Scripts: JavaScript; import Ljos.Ui\n\n{}",
            source.split_once("Effects\n").unwrap().1
        )
    );
    assert_eq!(
        kept_for("import QtQuick\nimport QtQuick.Effects as E\nE.MultiEffect { }\n"),
        "it imports `QtQuick.Effects` under a name"
    );
}

/// Every type of `QtQuick.Effects` the table describes is in `Ljos.Ui`, or
/// keeps a file that makes one.
#[test]
fn a_type_of_qt_effects_ljos_ui_has_not_keeps_the_file() {
    let ui = qt_types::module(MODULE).expect("Ljos.Ui is described");
    let types = qt_types::native_types("QtQuick.Effects").expect("QtQuick.Effects is described");
    assert!(types.contains(&"MultiEffect"), "{types:?}");
    for name in types {
        let source = format!("import QtQuick\nimport QtQuick.Effects\nItem {{ {name} {{ }} }}\n");
        match (ui.type_named(name).is_some(), lower(&source, &no_locals)) {
            (true, Outcome::Lowered(_)) => {}
            (false, Outcome::Kept(kept)) => {
                assert_eq!(kept.to_string(), format!("`{name}` is not in {MODULE} yet"));
            }
            (has, other) => panic!("{name} (in Ljos.Ui: {has}): {other:?}"),
        }
    }
}
