//! Tests of states, transitions and animations.

use super::*;

#[test]
fn a_property_changes_target_is_written_into_its_changes() {
    let text = lowered(
        "import QtQuick\nItem {\n    Rectangle { id: face }\n    states: State {\n        PropertyChanges {\n            target: face\n            color: \"red\"; border.width: 2\n            onColorChanged: face.color = \"blue\"\n        }\n        PropertyChanges { target: face; explicit: true; width: 3 }\n        PropertyChanges { face.height: 4 }\n    }\n}\n",
    );
    assert_eq!(
        text,
        "pragma Scripts: JavaScript; import Ljos.Ui\nItem {\n    Rectangle { id: face }\n    states: State {\n        PropertyChanges {\n\n            face.fill: \"red\"; face.border.width: 2\n            face.onFillChanged: face.fill = \"blue\"\n        }\n        PropertyChanges { explicit: true; face.width: 3 }\n        PropertyChanges { face.height: 4 }\n    }\n}\n"
    );
}

#[test]
fn a_property_changes_target_that_is_no_id_stays_a_target() {
    for (source, target) in [
        (
            "import QtQuick\nItem { states: State { PropertyChanges { target: parent; opacity: 0 } } }\n",
            "{ target: parent; opacity: 0 }",
        ),
        (
            "import QtQuick\nItem { id: a; property Item b; states: State { PropertyChanges { target: a.b; opacity: 0 } } }\n",
            "{ target: a.b; opacity: 0 }",
        ),
    ] {
        assert!(lowered(source).contains(target), "{source}");
    }
}

#[test]
fn what_the_id_form_has_no_spelling_for_keeps_the_file() {
    for source in [
        "import QtQuick\nItem { Text { id: t }\n states: State { PropertyChanges { target: t; font { bold: true } } } }\n",
        "import QtQuick\nimport QtQuick.Layouts\nItem { Item { id: t }\n states: State { PropertyChanges { target: t; Layout.fillWidth: true } } }\n",
    ] {
        assert!(
            kept_for(source).contains("Ljós's API names in each change"),
            "{source}"
        );
    }
}
