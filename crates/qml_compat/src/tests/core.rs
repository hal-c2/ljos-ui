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
    // `Qt` is a global, of every file.
    assert_eq!(
        lowered("import QtQuick\nItem { x: Qt.platform.os.length }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui\nItem { x: Qt.platform.os.length }\n"
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
