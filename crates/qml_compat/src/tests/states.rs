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

#[test]
fn a_behavior_is_on_the_member_by_its_ljos_name() {
    let text = lowered(
        "import QtQuick\nRectangle {\n    color: \"red\"\n    Behavior on color { ColorAnimation { duration: 200 } }\n}\n",
    );
    assert_eq!(
        text,
        "pragma Scripts: JavaScript; import Ljos.Ui\nRectangle {\n    fill: \"red\"\n    Behavior on fill { ColorAnimation { duration: 200 } }\n}\n"
    );
}

#[test]
fn a_value_source_animation_names_the_member_it_is_on() {
    let text = lowered(
        "import QtQuick\nRectangle {\n    ColorAnimation on color { to: \"red\"; duration: 200 }\n    Behavior on color { PropertyAnimation { property: \"color\" } }\n}\n",
    );
    assert!(text.contains("ColorAnimation on fill {"), "{text}");
    assert!(
        text.contains("PropertyAnimation { property: \"fill\" }"),
        "{text}"
    );
}

#[test]
fn an_animation_s_names_are_its_target_s() {
    let text = lowered(
        "import QtQuick\nItem {\n    Rectangle { id: r }\n    Text { id: t }\n    SequentialAnimation {\n        NumberAnimation { target: r; property: \"color\"; duration: 100 }\n        ColorAnimation { targets: [r]; properties: \"x, color,opacity\" }\n        PropertyAction { target: r; property: \"color\"; value: \"red\" }\n        ColorAnimation { target: t; property: \"color\" }\n        NumberAnimation { target: r; properties: \"x,y\" }\n    }\n}\n",
    );
    assert!(
        text.contains("{ target: r; property: \"fill\"; duration: 100 }"),
        "{text}"
    );
    assert!(text.contains("properties: \"x, fill,opacity\""), "{text}");
    assert!(
        text.contains("PropertyAction { target: r; property: \"fill\"; value: \"red\" }"),
        "{text}"
    );
    // A `Text`'s `color` is `color` in Ljós too.
    assert!(
        text.contains("{ target: t; property: \"color\" }"),
        "{text}"
    );
    assert!(text.contains("properties: \"x,y\""), "{text}");
}

#[test]
fn a_state_transition_names_what_the_states_change() {
    let text = lowered(
        "import QtQuick\nItem {\n    Rectangle { id: r }\n    states: [\n        State { name: \"on\"; PropertyChanges { target: r; color: \"red\" } }\n    ]\n    transitions: Transition {\n        ColorAnimation { properties: \"color\"; duration: 300 }\n    }\n}\n",
    );
    assert!(text.contains("r.fill: \"red\""), "{text}");
    assert!(
        text.contains("ColorAnimation { properties: \"fill\"; duration: 300 }"),
        "{text}"
    );
}

#[test]
fn a_name_that_is_one_member_on_some_and_another_on_the_rest_keeps_the_file() {
    let source = "import QtQuick\nItem {\n    Rectangle { id: r }\n    Text { id: t }\n    ColorAnimation { targets: [r, t]; property: \"color\" }\n}\n";
    assert!(matches!(
        cause_of(source),
        Cause::Animated(Animated::Split { name, new }) if name == "color" && new == "fill"
    ));
    let source = "import QtQuick\nItem {\n    Rectangle { id: r }\n    Text { id: t }\n    states: State {\n        PropertyChanges { r.color: \"red\" }\n        PropertyChanges { t.color: \"red\" }\n    }\n    transitions: Transition { ColorAnimation { property: \"color\" } }\n}\n";
    assert!(
        kept_for(source).contains("is `fill` on some of what it moves"),
        "{}",
        kept_for(source)
    );
}

#[test]
fn an_animation_whose_target_is_not_known_keeps_the_file() {
    for source in [
        // A target that is not an id of the file.
        "import QtQuick\nItem { property Item box; NumberAnimation { target: box; property: \"color\" } }\n",
        // No target, and no `Behavior` or state says what it moves.
        "import QtQuick\nItem { ColorAnimation { property: \"color\" } }\n",
        // A view's `add` moves the view's delegates, which the file does
        // not type here.
        "import QtQuick\nListView { add: Transition { ColorAnimation { property: \"color\" } } }\n",
        // On a member of a group, a name is the group's.
        "import QtQuick\nRectangle { Behavior on border.color { ColorAnimation { property: \"color\" } } }\n",
    ] {
        assert!(
            matches!(cause_of(source), Cause::Animated(Animated::Unknown { ref name }) if name == "color"),
            "{source}: {:?}",
            cause_of(source)
        );
    }
    assert!(
        kept_for("import QtQuick\nItem { ColorAnimation { property: \"color\" } }\n")
            .contains("names a member Ljós renames, on objects whose type is not known")
    );
}

#[test]
fn an_animation_whose_names_are_made_as_it_runs_keeps_the_file() {
    let source = "import QtQuick\nItem {\n    Rectangle { id: r }\n    property string which: \"color\"\n    NumberAnimation { target: r; property: which }\n}\n";
    assert!(matches!(
        cause_of(source),
        Cause::Animated(Animated::Made { ref binding }) if binding == "property"
    ));
}

#[test]
fn names_no_type_renames_are_left_as_they_are_wherever_they_point() {
    let text = lowered(
        "import QtQuick\nItem {\n    property Item box\n    NumberAnimation { target: box; properties: \"x,opacity\" }\n    ListView { add: Transition { NumberAnimation { property: \"opacity\"; from: 0 } } }\n}\n",
    );
    assert!(text.contains("properties: \"x,opacity\""), "{text}");
    assert!(text.contains("property: \"opacity\"; from: 0"), "{text}");
}
