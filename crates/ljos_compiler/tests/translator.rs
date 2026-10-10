//! The context `qsTr` looks its text up in: the file, or what
//! `pragma Translator` names. Literal calls carry it in the IR; scripts get
//! it from one line of the file's module.

mod oracle;

use ljos_compiler::{Project, compile_file};
use ljos_ir::CompiledFile;
use oracle::Runtime;

fn compile(key: &str, source: &str) -> CompiledFile {
    let mut project = Project::new(Runtime::new());
    project.add(key, source);
    let compiled = compile_file(&project, key).expect("compiles");
    assert_eq!(compiled.diagnostics, [], "{:?}", compiled.diagnostics);
    compiled.file
}

#[test]
fn the_context_is_the_name_of_the_file() {
    let file = compile(
        "ui/Page",
        r#"
import QtQuick
Item {
    property int count: 2
    Text { text: qsTr("Open") }
    Text { text: qsTr("%n file(s)", "", count) }
}
"#,
    );
    assert_eq!(file.translations[0].context, "Page");
    assert!(
        file.js
            .source
            .contains("const qsTr = __ljos_qsTr(\"Page\");\n"),
        "{}",
        file.js.source
    );
}

#[test]
fn pragma_translator_names_another_context() {
    let file = compile(
        "Page",
        r#"
pragma Translator: Shared
import QtQuick
Item {
    property int count: 2
    Text { text: qsTr("Open") }
    function label() { return qsTr("%n file(s)", "", count) }
}
"#,
    );
    assert_eq!(file.translations[0].context, "Shared");
    assert!(
        file.js
            .source
            .contains("const qsTr = __ljos_qsTr(\"Shared\");\n"),
        "{}",
        file.js.source
    );
}

#[test]
fn a_file_whose_scripts_do_not_translate_binds_nothing() {
    let file = compile(
        "Page",
        r#"
import QtQuick
Item {
    property int count: 2
    Text { text: qsTr("Open") }
    Text { text: "n: " + count }
}
"#,
    );
    assert_eq!(file.translations.len(), 1);
    assert!(
        !file.js.source.contains("__ljos_qsTr"),
        "{}",
        file.js.source
    );
}

#[test]
fn a_name_of_the_file_that_hides_qstr_is_left_alone() {
    let file = compile(
        "Page",
        r#"
import QtQuick
Item {
    function qsTr(text) { return text.toUpperCase() }
    property string shout: qsTr("open")
}
"#,
    );
    assert!(
        !file.js.source.contains("__ljos_qsTr"),
        "{}",
        file.js.source
    );
}

#[test]
fn a_delegate_translates_with_the_files_qstr_whatever_roles_the_model_has() {
    let file = compile(
        "Rows",
        r#"
import QtQuick
ListView {
    model: 3
    delegate: Text {
        property string label: qsTr("Row")
        text: qsTr("Row %1").arg(index)
    }
}
"#,
    );
    // Not a name to look for among the roles at run time: that would find
    // the global `qsTr`, which has no context.
    assert!(
        file.js.source.contains("function f0(index)"),
        "{}",
        file.js.source
    );
    assert!(
        file.js
            .source
            .contains("const qsTr = __ljos_qsTr(\"Rows\");\n"),
        "{}",
        file.js.source
    );
    assert_eq!(file.translations.len(), 2);
    assert!(file.translations.iter().all(|t| t.context == "Rows"));
}
