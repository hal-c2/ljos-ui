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
fn a_model_of_qtqml_is_had_from_ljos_core_until_ljos_models_has_it() {
    assert_eq!(
        lowered("import QtQuick\nItem { ListModel { } }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\nItem { ListModel { } }\n"
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
