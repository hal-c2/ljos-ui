//! Tests of models and views: `ListModel`, `Repeater`, `ListView`.

use super::*;

/// The rows of a model of several roles, as a delegate reads them.
const ZONES: &str = "ListModel { ListElement { city: \"a\"; shift: 0 } }";

fn with_zones(body: &str) -> String {
    format!(
        "import QtQuick\nItem {{\n  {}\n  {body}\n}}\n",
        ZONES.replace("ListModel {", "ListModel { id: zones;")
    )
}

#[test]
fn a_role_of_a_model_of_several_roles_is_a_member_of_modeldata() {
    let text = lowered(&with_zones(
        "Repeater { model: zones; Text { text: model.city + model.index; property var row: model } }",
    ));
    assert!(
        text.contains("Text { text: modelData.city + index; property unknown row: modelData }"),
        "{text}"
    );
    assert!(text.starts_with("pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Models\n"));
}

#[test]
fn a_delegate_is_found_through_a_component_and_a_chooser() {
    let text = lowered(&with_zones(
        "ListView { model: zones; delegate: Component { Text { text: model.city } } }",
    ));
    assert!(text.contains("Text { text: modelData.city }"), "{text}");
    let text = lowered(&format!(
        "import QtQuick\nimport QtQml.Models\nItem {{\n  {}\n  ListView {{ model: zones; delegate: DelegateChooser {{ DelegateChoice {{ delegate: Text {{ text: model.city }} }} }} }}\n}}\n",
        ZONES.replace("ListModel {", "ListModel { id: zones;")
    ));
    assert!(
        text.contains("delegate: Text { text: modelData.city }"),
        "{text}"
    );
}

#[test]
fn a_nested_view_gives_its_delegates_its_own_rows() {
    let reason = kept_for(&with_zones(
        "Repeater { model: zones; Item { Repeater { model: 3; Text { text: model.city } } } }",
    ));
    assert!(reason.contains("a role of a model of values"), "{reason}");
    // The view's own `model` is its property, not a row.
    let text = lowered(&with_zones(
        "Repeater { model: zones; Item { ListView { model: 3; width: model } } }",
    ));
    assert!(
        text.contains("ListView { model: 3; width: model }"),
        "{text}"
    );
}

#[test]
fn a_declared_model_is_the_files_own() {
    let text = lowered(&with_zones(
        "Repeater { model: zones; Item { required property var model; Text { text: model.city } } }",
    ));
    assert!(
        text.contains("required property unknown model; Text { text: model.city }"),
        "{text}"
    );
}

#[test]
fn a_local_of_a_name_of_the_row_keeps_the_file() {
    let reason = kept_for(&with_zones(
        "Repeater { model: zones; Text { text: { var model = 1; return model.city } } }",
    ));
    assert!(reason.contains("a local is called `model`"), "{reason}");
    let reason = kept_for(&with_zones(
        "Repeater { model: zones; Text { text: { var index = 1; return model.index } } }",
    ));
    assert!(reason.contains("a local is called `index`"), "{reason}");
}

#[test]
fn an_array_of_objects_has_no_roles() {
    let reason = kept_for(
        "import QtQuick\nItem {\n  Repeater { model: [{ city: \"a\" }]; Text { text: model.city } }\n}\n",
    );
    assert!(reason.contains("read `modelData.city`"), "{reason}");
    let text = lowered(
        "import QtQuick\nItem {\n  Repeater { model: [{ city: \"a\" }]; Text { text: model.modelData.city } }\n}\n",
    );
    assert!(text.contains("Text { text: modelData.city }"), "{text}");
}

/// Outside any delegate the file shows, `model` is a row only if what
/// makes the file makes it a delegate, and of what the file does not say:
/// nothing of it is guessed, `model.index` and `model.modelData` either.
#[test]
fn a_file_outside_any_delegate_reads_nothing_of_a_row() {
    let reason = kept_for("import QtQuick\nText { text: model.index }\n");
    assert!(
        reason.contains("`model.index` reads the row of a view the file is not shown in; give the root `required property int index`"),
        "{reason}"
    );
    let reason = kept_for("import QtQuick\nText { text: model.modelData }\n");
    assert!(
        reason.contains("give the root `required property var modelData`"),
        "{reason}"
    );
    let reason = kept_for("import QtQuick\nText { text: model.city }\n");
    assert!(
        reason.contains("a view the file is not shown in"),
        "{reason}"
    );
}

#[test]
fn a_component_a_view_is_given_by_id_is_its_delegate() {
    let text = lowered(&with_zones(
        "Component { id: d; Text { text: model.city + model.index } }\n  ListView { model: zones; delegate: d }",
    ));
    assert!(
        text.contains("Component { id: d; Text { text: modelData.city + index } }"),
        "{text}"
    );
}

#[test]
fn a_component_given_by_id_and_named_elsewhere_is_no_known_delegate() {
    let reason = kept_for(&with_zones(
        "Component { id: d; Text { text: model.city } }\n  ListView { model: zones; delegate: d }\n  property Component other: d",
    ));
    assert!(
        reason.contains("the `Component` it is in is not written in the `delegate:` of, or given to by id alone; give the Component's root `required property var city`"),
        "{reason}"
    );
}

#[test]
fn a_component_given_by_id_to_views_of_other_rows_reads_no_role() {
    let reason = kept_for(&with_zones(
        "Component { id: d; Text { text: model.city } }\n  ListView { model: zones; delegate: d }\n  ListView { model: 3; delegate: d }",
    ));
    assert!(
        reason.contains("a model the file does not show"),
        "{reason}"
    );
}

#[test]
fn a_listed_model_with_an_id_and_one_role_may_be_given_more() {
    let reason = kept_for(
        "import QtQuick\nItem {\n  ListModel { id: m; ListElement { city: \"a\" } }\n  Repeater { model: m; Text { text: model.city } }\n}\n",
    );
    assert!(
        reason.contains("a model the file does not show"),
        "{reason}"
    );
}

#[test]
fn a_member_of_the_scope_called_index_keeps_the_file() {
    let reason = kept_for(&with_zones(
        "Repeater { model: zones; Item { property int index: 2; Text { text: model.index } } }",
    ));
    assert!(reason.contains("would be `index`"), "{reason}");
}

#[test]
fn qtqml_models_under_a_name_keeps_the_file() {
    let reason = kept_for("import QtQuick\nimport QtQml.Models as M\nItem { }\n");
    assert!(reason.contains("`QtQml.Models` under a name"), "{reason}");
}

/// A local or a method called as a member Ljós renames is kept only where
/// the member could be what the name reads: here no object is a ListView.
#[test]
fn a_name_ljos_renames_on_another_type_is_free_for_a_local() {
    let source = "import QtQuick\nItem {\n  ListModel { id: lm }\n  function remove(i) { lm.remove(i) }\n  function f() { let add = 1; return add }\n}\n";
    assert_eq!(
        lowered(source),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Models\nItem {\n  ListModel { id: lm }\n  function remove(i) { lm.remove(i) }\n  function f() { let add = 1; return add }\n}\n"
    );
}

#[test]
fn a_local_of_a_name_the_scope_renames_keeps_the_file() {
    assert!(
        kept_for("import QtQuick\nListView { function f() { let add = 1; return add } }\n")
            .contains("a local is called `add`")
    );
    assert!(
        kept_for(
            "import QtQuick\nItem { ListView { function f() { let remove = 1; return remove } } }\n"
        )
        .contains("a local is called `remove`")
    );
}
