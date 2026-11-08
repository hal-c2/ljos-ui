//! Tests of dialogs: `QtQuick.Dialogs`, popups.

use super::*;

#[test]
fn qt_file_and_folder_dialogs_are_lowered_to_ljos_controls() {
    let body = "Item {\n  FileDialog { fileMode: FileDialog.OpenFiles; onAccepted: console.log(selectedFiles) }\n  FolderDialog { options: FolderDialog.ShowDirsOnly }\n}\n";
    assert_eq!(
        lowered(&format!("import QtQuick\nimport QtQuick.Dialogs\n{body}")),
        format!("pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Controls\n{body}")
    );
    // Imported with the controls, the dialogs' line is the second import of
    // Ljos.Controls, and stays empty.
    assert_eq!(
        lowered(&format!(
            "import QtQuick\nimport QtQuick.Dialogs\nimport QtQuick.Controls.Basic\n{body}"
        )),
        format!("pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Controls\n\n{body}")
    );
}

#[test]
fn a_dialog_ljos_controls_has_not_taken_or_the_dialogs_under_a_name_keep_the_file() {
    for dialog in ["ColorDialog", "FontDialog", "MessageDialog"] {
        assert_eq!(
            kept_for(&format!(
                "import QtQuick\nimport QtQuick.Dialogs\nItem {{ {dialog} {{ }} }}\n"
            )),
            format!("`{dialog}` is not in Ljos.Controls yet")
        );
    }
    assert_eq!(
        kept_for(
            "import QtQuick\nimport QtQuick.Dialogs\nItem { property int b: MessageDialog.Ok }\n"
        ),
        "`MessageDialog` is not in Ljos.Controls yet"
    );
    assert_eq!(
        kept_for("import QtQuick\nimport QtQuick.Dialogs as D\nItem { D.FileDialog { } }\n"),
        "it imports `QtQuick.Dialogs` under a name"
    );
}
