//! Tests of loading: `Loader`, components, QML text made at run time.

use super::*;

#[test]
fn qml_text_made_while_the_file_runs_keeps_it() {
    assert!(
        kept_for("import QtQuick\nItem { function f() { Qt.createQmlObject(\"import QtQuick; Rectangle {}\", this) } }\n")
            .contains("QML text")
    );
}
