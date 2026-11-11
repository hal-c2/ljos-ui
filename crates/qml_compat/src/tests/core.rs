//! Tests of the non-visual core: `QtObject`, `Timer`, `Connections`, `Binding`.

use super::*;

#[test]
fn a_file_of_qtqml_alone_imports_ljos_core() {
    assert_eq!(
        lowered("import QtQml\nQtObject { }\n"),
        "pragma Scripts: JavaScript; import Ljos.Core\nQtObject { }\n"
    );
}

#[test]
fn a_type_of_qtqml_brings_ljos_core_onto_the_import_line() {
    assert_eq!(
        lowered("import QtQuick\nItem { Timer { } }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\nItem { Timer { } }\n"
    );
    // An attached object of one is naming it too.
    assert_eq!(
        lowered("import QtQuick\nItem { Component.onCompleted: {} }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\n\
         Item { Component.onCompleted: {} }\n"
    );
    // `Qt` is a global, of every file: what stays Qt's needs no import.
    assert_eq!(
        lowered("import QtQuick\nItem { property color c: Qt.darker(\"red\") }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui\nItem { property color c: Qt.darker(\"red\") }\n"
    );
}

#[test]
fn an_import_of_qtqml_beside_qtquick_becomes_ljos_core() {
    assert_eq!(
        lowered("import QtQuick\nimport QtQml\nItem { }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Core\nItem { }\n"
    );
}

#[test]
fn a_model_of_qtqml_is_had_from_ljos_models_not_ljos_core() {
    assert_eq!(
        lowered("import QtQuick\nItem { ListModel { } }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Models\nItem { ListModel { } }\n"
    );
    // Beside a type of Ljos.Core, the file imports both.
    assert_eq!(
        lowered("import QtQuick\nItem { ListModel { } Timer { } }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core; import Ljos.Models\nItem { ListModel { } Timer { } }\n"
    );
}

#[test]
fn a_file_of_qtqml_that_names_a_type_of_ljos_ui_imports_it_too() {
    // `QtQuick.Layouts` without `QtQuick`: its layout is Ljós.Ui's.
    let text = lowered(
        "import QtQml\nimport QtQuick.Layouts\n\
         RowLayout { Item { implicitWidth: 8; Layout.fillWidth: true } }\n",
    );
    assert!(
        text.starts_with("pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\n"),
        "{text}"
    );
}

/// Only Qt's table has a type of `QtQml` that `Ljos.Core` has not: Ljós's
/// own describes no more of `QtQml` than `Ljos.Core` brings.
#[test]
fn a_type_of_qtqml_that_ljos_core_has_not_keeps_the_file() {
    if !qt_types::has_qt_table() {
        return;
    }
    let reason = kept_for("import QtQml\nLoggingCategory { name: \"a\" }\n");
    assert_eq!(reason, "`LoggingCategory` is not in Ljos.Core yet");
}

/// A type the file writes, of a property or a signal's argument, is one
/// it names as much as one it makes.
#[test]
fn a_type_of_qtqml_a_file_writes_brings_ljos_core() {
    assert_eq!(
        lowered(
            "import QtQuick\n\
             Item { property QtObject o; property Component c; property list<QtObject> xs }\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\n\
         Item { property QtObject o; property Component c; property list<QtObject> xs }\n"
    );
    assert_eq!(
        lowered("import QtQuick\nItem { signal picked(QtObject o) }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\n\
         Item { signal picked(QtObject o) }\n"
    );
    // A type of Ljos.Ui's is renamed where it is written too.
    assert_eq!(
        lowered("import QtQuick\nItem { property MouseArea area; property Item at }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui\n\
         Item { property PointerArea area; property Item at }\n"
    );
}

#[test]
fn a_member_of_qt_that_ljos_core_has_is_lowered_where_its_name_is_free() {
    assert_eq!(
        lowered("import QtQuick\nItem { x: Qt.platform.os.length; y: Qt.point(1, 2).y }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\n\
         Item { x: App.platform.os.length; y: point(1, 2).y }\n"
    );
    // A property of that name, a local of it, or a file beside
    // it of that name, is what the name is.
    assert_eq!(
        lowered("import QtQuick\nItem { property int size: 1; width: Qt.size(1, 2).width }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui\n\
         Item { property int size: 1; width: Qt.size(1, 2).width }\n"
    );
    assert_eq!(
        lowered("import QtQuick\nItem { function f(later) { Qt.callLater(later) } }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui\n\
         Item { function f(later) { Qt.callLater(later) } }\n"
    );
    // A parameter called `Qt` is no global: its members are its own.
    assert_eq!(
        lowered("import QtQuick\nItem { function f(Qt) { return Qt.point(1, 2) } }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui\n\
         Item { function f(Qt) { return Qt.point(1, 2) } }\n"
    );
    let app = |name: &str| (name == "App").then(|| root_of("Item", &[]));
    match lower("import QtQuick\nItem { x: Qt.platform.os.length }\n", &app) {
        Outcome::Lowered(lowered) => assert_eq!(
            lowered.text,
            "pragma Scripts: JavaScript; import Ljos.Ui\nItem { x: Qt.platform.os.length }\n"
        ),
        other => panic!("not lowered: {other:?}"),
    }
    // What a file it is built on declares is in sight too.
    let base = |name: &str| {
        (name == "Base").then(|| Root {
            declares: vec!["rgba".into()],
            ..root_of("Item", &[])
        })
    };
    match lower(
        "import QtQuick\nBase { property color c: Qt.rgba(1, 0, 0, 1) }\n",
        &base,
    ) {
        Outcome::Lowered(lowered) => assert_eq!(
            lowered.text,
            "pragma Scripts: JavaScript; import Ljos.Ui\nBase { property color c: Qt.rgba(1, 0, 0, 1) }\n"
        ),
        other => panic!("not lowered: {other:?}"),
    }
}

/// A member of a type the file's objects are is in sight, though no other
/// word of the file is its name: in a `Component`'s handler, `url` is the
/// component's, so `Qt.resolvedUrl` stays Qt's. (`Component` is what
/// `Ljos.Core` is imported for.)
#[test]
fn a_member_of_qt_stays_qt_s_where_a_type_of_the_file_s_objects_has_its_name() {
    assert_eq!(
        lowered(
            "import QtQuick\nItem { Component { Component.onCompleted: console.log(Qt.resolvedUrl(\"a.png\")) } }\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\n\
         Item { Component { Component.onCompleted: console.log(Qt.resolvedUrl(\"a.png\")) } }\n"
    );
    // Where no object's type has it, the name is free.
    assert_eq!(
        lowered("import QtQuick\nItem { Item { x: Qt.point(1, 2).x } }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\n\
         Item { Item { x: point(1, 2).x } }\n"
    );
}

#[test]
fn a_member_of_qt_stays_qt_s_where_a_directory_imported_by_path_may_name_it() {
    // `qml/App.qml` is not beside the file: what a directory imported by
    // path has is not in sight of the pass.
    assert_eq!(
        lowered(
            "import QtQuick\nimport \"qml\"\nItem { x: Qt.platform.os.length; y: Qt.point(1, 2).y }\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\n\
         import \"qml\"\nItem { x: Qt.platform.os.length; y: point(1, 2).y }\n"
    );
    // Under a name, the directory's types are that name's.
    assert_eq!(
        lowered("import QtQuick\nimport \"qml\" as Q\nItem { x: Qt.platform.os.length }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\n\
         import \"qml\" as Q\nItem { x: App.platform.os.length }\n"
    );
}

#[test]
fn qt_resolved_url_given_a_context_stays_qt_s() {
    // Qt 6 resolves against the file of the second argument's component;
    // `url` takes one.
    assert_eq!(
        lowered(
            "import QtQuick\nItem { property url a: Qt.resolvedUrl(\"a.png\"); property url b: Qt.resolvedUrl(\"b.png\", this) }\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\n\
         Item { property url a: url(\"a.png\"); property url b: Qt.resolvedUrl(\"b.png\", this) }\n"
    );
}

#[test]
fn qt_resolved_url_given_a_spread_stays_qt_s() {
    // A spread may carry the context too, so it counts as more than one.
    assert_eq!(
        lowered("import QtQuick\nItem { property url c: Qt.resolvedUrl(...[\"c.png\", this]) }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui\n\
         Item { property url c: Qt.resolvedUrl(...[\"c.png\", this]) }\n"
    );
}

#[test]
fn qt_resolved_url_not_called_with_one_argument_stays_qt_s() {
    // `.call`, `.apply` and the bare function may pass a context `url` drops.
    assert_eq!(
        lowered(
            "import QtQuick\nItem { property url c: Qt.resolvedUrl.call(null, \"c.png\", this); property url d: Qt.resolvedUrl.apply(null, [\"d.png\", this]); property var f: Qt.resolvedUrl }\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui\n\
         Item { property url c: Qt.resolvedUrl.call(null, \"c.png\", this); property url d: Qt.resolvedUrl.apply(null, [\"d.png\", this]); property unknown f: Qt.resolvedUrl }\n"
    );
}

/// A `Connections` is aimed at its target only through its handlers: one of
/// an id's renamed signal is renamed, its own members are not, and one it
/// cannot follow keeps the file.
#[test]
fn a_connections_renames_the_handlers_of_its_target() {
    let at = |connections: &str| {
        format!("import QtQuick\nItem {{ Rectangle {{ id: r }} {connections} }}\n")
    };
    for (handler, lowered_as) in [
        (
            "function onColorChanged() { }",
            "function onFillChanged() { }",
        ),
        ("onColorChanged: { }", "onFillChanged: { }"),
        (
            "function onWidthChanged() { }",
            "function onWidthChanged() { }",
        ),
    ] {
        let text = lowered(&at(&format!(
            "Connections {{ target: r; enabled: true; {handler} }}"
        )));
        assert!(
            text.contains(&format!(
                "Connections {{ target: r; enabled: true; {lowered_as} }}"
            )),
            "{text}"
        );
    }
    // A target that is not an id, or none, is not followed.
    for connections in [
        "Connections { target: parent; function onColorChanged() { } }",
        "Connections { function onColorChanged() { } }",
    ] {
        assert_eq!(
            cause_of(&at(connections)),
            Cause::HandlerOnUnknown {
                handler: "onColorChanged".into()
            },
            "{connections}"
        );
    }
}
