//! The checker's rules of translated texts in a file of Ljós's API
//! (docs/DIALECT.md §4.13): Qt's `qsTr` family is `qt-tr`, and a
//! translated text joined to another value is `i18n-concat`.

mod oracle;

use ljos_compiler::{Code, Project, Severity, compile_file};
use oracle::Runtime;

fn found(source: &str) -> Vec<(Code, Severity, String)> {
    let mut project = Project::new(Runtime::new());
    project.add("Main", source);
    let diagnostics = match compile_file(&project, "Main") {
        Ok(compiled) => compiled.diagnostics,
        Err(diagnostics) => diagnostics,
    };
    diagnostics
        .into_iter()
        .map(|d| (d.code, d.severity, d.message))
        .collect()
}

#[test]
fn qt_s_translation_functions_are_qt_s() {
    for (call, instead) in [
        ("qsTr(\"Open\")", "`qsTr` is Qt's: use t`…` (Ljos.Core)"),
        (
            "qsTranslate(\"C\", \"Open\")",
            "`qsTranslate` is Qt's: use t({ message, context }) (Ljos.Core)",
        ),
        (
            "qsTrId(\"open\")",
            "`qsTrId` is Qt's: use t({ id }) (Ljos.Core)",
        ),
        (
            "QT_TR_NOOP(\"Open\")",
            "`QT_TR_NOOP` is Qt's: write msg`…` (Ljos.Core) and translate it with t(…) where it is shown",
        ),
    ] {
        let source = format!("import Ljos.Ui\nItem {{\n    property string a: {call}\n}}\n");
        let found = found(&source);
        assert!(
            found
                .iter()
                .any(|(code, severity, message)| *code == Code::QtTr
                    && *severity == Severity::Error
                    && message == instead),
            "{call}: {found:?}"
        );
    }
}

#[test]
fn a_local_named_qs_tr_is_the_script_s() {
    let found =
        found("import Ljos.Ui\nItem {\n    function f(qsTr: string): string { return qsTr }\n}\n");
    assert!(
        !found.iter().any(|(code, ..)| *code == Code::QtTr),
        "{found:?}"
    );
}

#[test]
fn qt_s_qml_keeps_qs_tr() {
    let found = found("import QtQuick\nItem {\n    property string a: qsTr(\"Open\")\n}\n");
    assert!(
        !found.iter().any(|(code, ..)| *code == Code::QtTr),
        "{found:?}"
    );
}

#[test]
fn a_list_element_s_qs_tr_stays_qt_s() {
    let found = found(
        "import Ljos.Ui\nimport Ljos.Models\nItem {\n    Repeater { model: ListModel { ListElement { city: qsTr(\"Reykjav\u{ed}k\") } } }\n}\n",
    );
    assert!(
        !found.iter().any(|(code, ..)| *code == Code::QtTr),
        "{found:?}"
    );
}

#[test]
fn a_translated_text_joined_to_a_value_is_two_messages() {
    let found = found(
        "import Ljos.Ui\nimport Ljos.Core\nItem {\n    property int minutes: 5\n    property string ago: minutes + \" \" + t`minutes ago`\n    property string one: t`${minutes} minutes ago`\n    property string sum: \"a\" + (minutes + \"b\")\n}\n",
    );
    let concat: Vec<_> = found
        .iter()
        .filter(|(code, ..)| *code == Code::I18nConcat)
        .collect();
    assert_eq!(concat.len(), 1, "{found:?}");
    assert_eq!(concat[0].1, Severity::Warning);
    assert_eq!(
        concat[0].2,
        "a translated text joined to another value is two messages: write one, t`${minutes} minutes ago`"
    );
}

#[test]
fn a_local_named_plural_is_the_script_s() {
    let found = found(
        "import Ljos.Ui\nimport Ljos.Core\nItem {\n    function f(plural: (n: number) => string): string { return plural(1) + \"x\" }\n}\n",
    );
    assert!(
        !found.iter().any(|(code, ..)| *code == Code::I18nConcat),
        "{found:?}"
    );
}
