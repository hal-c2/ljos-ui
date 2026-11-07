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
