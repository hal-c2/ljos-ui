//! Tests of dialogs: `QtQuick.Dialogs`, popups.

use super::*;

#[test]
fn qt_file_and_folder_dialogs_are_lowered_to_ljos_controls() {
    let body = "Item {\n  FileDialog { fileMode: FileDialog.OpenFiles; onAccepted: console.log(selectedFiles) }\n  FolderDialog { options: FolderDialog.ShowDirsOnly }\n}\n";
    assert_eq!(
        lowered(&format!("import QtQuick\nimport QtQuick.Dialogs\n{body}")),
        format!("pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Controls.Basic\n{body}")
    );
    // Imported with the controls, the dialogs' line is the second import of
    // Ljos.Controls.Basic, and stays empty.
    assert_eq!(
        lowered(&format!(
            "import QtQuick\nimport QtQuick.Dialogs\nimport QtQuick.Controls.Basic\n{body}"
        )),
        format!("pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Controls.Basic\n\n{body}")
    );
}

#[test]
fn a_dialog_ljos_controls_has_not_taken_or_the_dialogs_under_a_name_keep_the_file() {
    for dialog in ["ColorDialog", "FontDialog", "MessageDialog"] {
        assert_eq!(
            kept_for(&format!(
                "import QtQuick\nimport QtQuick.Dialogs\nItem {{ {dialog} {{ }} }}\n"
            )),
            format!("`{dialog}` is not in Ljos.Controls.Basic yet")
        );
    }
    assert_eq!(
        kept_for(
            "import QtQuick\nimport QtQuick.Dialogs\nItem { property int b: MessageDialog.Ok }\n"
        ),
        "`MessageDialog` is not in Ljos.Controls.Basic yet"
    );
    assert_eq!(
        kept_for("import QtQuick\nimport QtQuick.Dialogs as D\nItem { D.FileDialog { } }\n"),
        "it imports `QtQuick.Dialogs` under a name"
    );
}

/// A Qt file finds a name its imports lack among the files beside it. The
/// lowered file imports Ljós's modules instead, and an import is found
/// before the files beside it: a name one of those has and the Qt imports
/// had not would be Ljós's type, not the project's.
#[test]
fn a_file_of_the_project_a_ljos_module_would_shadow_keeps_the_file() {
    let only =
        |file: &'static str| move |name: &str| (name == file).then(|| root_of("Rectangle", &[]));
    let kept = |source: &str, file: &'static str| match lower(source, &only(file)) {
        Outcome::Kept(kept) => kept.to_string(),
        other => panic!("lowered: {other:?}"),
    };
    assert_eq!(
        kept(
            "import QtQuick\nimport QtQuick.Dialogs\nItem { Button { } FileDialog { } }\n",
            "Button"
        ),
        "`Button` is a file of the project, and Ljos.Controls.Basic, which the lowered file imports, has a `Button` that would be found first"
    );
    assert_eq!(
        kept(
            "import QtQuick\nimport QtQuick.Controls.Basic\nItem { Label { } FileDialog { } }\n",
            "FileDialog"
        ),
        "`FileDialog` is a file of the project, and Ljos.Controls.Basic, which the lowered file imports, has a `FileDialog` that would be found first"
    );
    // Ljós's modules have names QtQuick has not (`Box`, `PointerArea`),
    // so what a lowered file imports can shadow a file too.
    assert_eq!(
        kept("import QtQuick\nItem { Box { } }\n", "Box"),
        "`Box` is a file of the project, and Ljos.Layout, which the lowered file imports, has a `Box` that would be found first"
    );
    // Read in a script, as an enum of the project's file, the same.
    assert_eq!(
        kept("import QtQuick\nItem { property int d: Box.Row }\n", "Box"),
        "`Box` is a file of the project, and Ljos.Layout, which the lowered file imports, has a `Box` that would be found first"
    );
    // A name the file does not import at all, which a Qt file finds in the
    // context, is shadowed as well.
    assert_eq!(
        kept_for("import QtQuick\nItem { property var d: PointerArea }\n"),
        "`PointerArea` is not a type of the modules the file imports, and Ljos.Ui, which the lowered file imports, has a `PointerArea` that would be found first"
    );
    // A name no module the lowered file imports has is still the project's.
    assert!(matches!(
        lower(
            "import QtQuick\nimport QtQuick.Dialogs\nItem { Card { } }\n",
            &only("Card")
        ),
        Outcome::Lowered(_)
    ));
    // Without the controls, Ljos.Controls.Basic is not imported, and its `Button`
    // shadows nothing.
    assert!(matches!(
        lower("import QtQuick\nItem { Button { } }\n", &only("Button")),
        Outcome::Lowered(_)
    ));
}

#[test]
fn the_dialogs_with_the_controls_of_another_style_keep_the_file_for_the_style() {
    let Outcome::Kept(kept) = in_style(
        "import QtQuick\nimport QtQuick.Controls\nimport QtQuick.Dialogs\nItem { FileDialog { } }\n",
        "Fusion",
    ) else {
        panic!("lowered in Fusion");
    };
    assert_eq!(
        kept.to_string(),
        "it imports `QtQuick.Controls`, which is the Fusion style where it runs, and Ljos.Controls.Basic is Basic"
    );
}

#[test]
fn the_dialogs_imported_before_the_controls_carry_the_import_of_ljos_controls() {
    assert_eq!(
        lowered(
            "import QtQuick\nimport QtQuick.Dialogs\nimport QtQuick.Controls.Basic\nLabel { }\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Controls.Basic\n\nLabel { }\n"
    );
}

#[test]
fn an_import_qualifier_read_in_a_script_is_the_files_own() {
    // Qt and Ljós alike find a qualifier before a type of an import, so
    // `Box` stays the script's, not Ljos.Ui's `Box`.
    assert_eq!(
        lowered(
            "import QtQuick\nimport \"box.js\" as Box\nItem { property int d: Box.helper() }\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui\nimport \"box.js\" as Box\nItem { property int d: Box.helper() }\n"
    );
    // Named as a type Ljós renames, or one Ljos.Ui has not taken, the
    // qualifier is neither renamed nor a reason to keep the file.
    assert_eq!(
        lowered(
            "import QtQuick\nimport \"area.js\" as MouseArea\nItem { property int d: MouseArea.helper() }\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui\nimport \"area.js\" as MouseArea\nItem { property int d: MouseArea.helper() }\n"
    );
    assert_eq!(
        lowered(
            "import QtQuick\nimport \"paint.js\" as Canvas\nItem { property int d: Canvas.helper() }\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui\nimport \"paint.js\" as Canvas\nItem { property int d: Canvas.helper() }\n"
    );
}

/// A module of the project with a `Label`, a `FileDialog`, a `RowLayout`
/// and a `Row` of its own, each a Rectangle.
struct Gauges;

impl Locals for Gauges {
    fn root(&self, _: &str) -> Option<Root> {
        None
    }

    fn controls_style(&self) -> Option<&str> {
        Some(CONTROLS_STYLE)
    }

    fn module(&self, uri: &str) -> Option<ProjectModule> {
        (uri == "Gauges").then_some(ProjectModule::Files)
    }

    fn imported(&self, from: Imported<'_>, name: &str) -> Option<Root> {
        (from == Imported::Module("Gauges")
            && matches!(name, "Label" | "FileDialog" | "RowLayout" | "Row"))
        .then(|| root_of("Rectangle", &[]))
    }
}

/// Qt finds a name in the last import that has it. The lowered file has
/// one `import Ljos.Controls.Basic`, on the first line of the controls and the
/// dialogs, `Ljos.Ui` on `QtQuick`'s line and `Ljos.Layout` on
/// `QtQuick.Layouts`'. An import of the project between the line a Qt
/// type is found on and the one Ljós's takes means one type in Qt's file
/// and another in the lowered one.
#[test]
fn an_import_of_the_project_between_qts_line_and_ljos_line_keeps_the_file() {
    let kept = |source: &str| match lower(source, &Gauges) {
        Outcome::Kept(kept) => kept.to_string(),
        other => panic!("lowered: {other:?}"),
    };
    // Basic's `Label` in Qt; lowered, `Gauges` is after `Ljos.Controls.Basic`.
    assert_eq!(
        kept(
            "import QtQuick\nimport QtQuick.Dialogs\nimport Gauges\nimport QtQuick.Controls.Basic\nItem { Label { color: \"red\" } }\n"
        ),
        "`Label` is Qt's, but in the lowered file the project's `Label`, imported after the line Ljos.Controls.Basic takes, would be found first"
    );
    // The dialogs' `FileDialog` in Qt, as they are imported last.
    assert_eq!(
        kept(
            "import QtQuick\nimport QtQuick.Controls.Basic\nimport Gauges\nimport QtQuick.Dialogs\nItem { FileDialog { } }\n"
        ),
        "`FileDialog` is Qt's, but in the lowered file the project's `FileDialog`, imported after the line Ljos.Controls.Basic takes, would be found first"
    );
    // The project's `Row` in Qt, imported after `QtQuick`; lowered,
    // `Ljos.Layout` takes `QtQuick.Layouts`' line, after `Gauges`.
    assert_eq!(
        kept("import QtQuick\nimport Gauges\nimport QtQuick.Layouts\nItem { Row { } }\n"),
        "`Row` is a file of the project, and Ljos.Layout, which the lowered file imports, has a `Row` that would be found first"
    );
    // `QtQuick.Layouts`' `RowLayout`, in Qt and lowered alike: its line is
    // `Ljos.Layout`'s, after `Gauges`.
    assert!(matches!(
        lower(
            "import QtQuick\nimport Gauges\nimport QtQuick.Layouts\nItem { RowLayout { } }\n",
            &Gauges
        ),
        Outcome::Lowered(_)
    ));
}

#[test]
fn an_import_of_the_project_after_the_dialogs_is_the_projects_wherever_the_controls_are() {
    // In Qt and lowered alike, `Gauges` is after the dialogs and the line
    // `Ljos.Controls.Basic` takes: its `FileDialog` is a Rectangle, whose
    // `color` is a fill.
    let Outcome::Lowered(lowered) = lower(
        "import QtQuick\nimport QtQuick.Dialogs\nimport Gauges\nimport QtQuick.Controls.Basic\nItem { FileDialog { color: \"red\" } }\n",
        &Gauges,
    ) else {
        panic!("kept");
    };
    assert!(
        lowered.text.contains("FileDialog { fill: \"red\" }"),
        "{}",
        lowered.text
    );
}

/// A name a script binds itself is the script's, in the lowered file as in
/// Qt's, whatever module has a type of that name.
#[test]
fn a_name_a_script_binds_itself_is_the_scripts() {
    for body in [
        "function f() { const Row = 2; return Row }",
        "function f() { var Box = { w: 1 }; return Box.w }",
        "function f(Box) { return Box }",
        "function f() { return [1].map(PointerArea => PointerArea + 1) }",
        "function f() { try { return 1 } catch (Box) { return Box } }",
        "function f() { function Box() { return 1 } return Box() }",
        "function f() { for (let Box of [1]) { return Box } return 0 }",
    ] {
        let source = format!("import QtQuick\nItem {{ {body} }}\n");
        assert_eq!(
            lowered(&source),
            format!("pragma Scripts: JavaScript; import Ljos.Ui\nItem {{ {body} }}\n")
        );
    }
    // A local named as a type Ljós renames is not renamed: its uses would
    // be the new name's, its declaration the old one's.
    let body = "function f() { let MouseArea = 1; return MouseArea }";
    assert_eq!(
        lowered(&format!("import QtQuick\nItem {{ {body} }}\n")),
        format!("pragma Scripts: JavaScript; import Ljos.Ui\nItem {{ {body} }}\n")
    );
    // Outside the scope of its declaration, the name is the type's again.
    assert_eq!(
        kept_for("import QtQuick\nItem { function f() { { let Box = 1 } return Box } }\n"),
        "`Box` is not a type of the modules the file imports, and Ljos.Layout, which the lowered file imports, has a `Box` that would be found first"
    );
    assert_eq!(
        lowered(
            "import QtQuick\nItem { function f() { if (true) { let MouseArea = 1 } return MouseArea } }\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui\nItem { function f() { if (true) { let MouseArea = 1 } return PointerArea } }\n"
    );
}
