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
    for source in [
        "import Ljos.Ui\nimport Ljos.Models\nItem {\n    Repeater { model: ListModel { ListElement { city: qsTr(\"Reykjav\u{ed}k\") } } }\n}\n",
        // The type, by whatever name the file gives it.
        "import Ljos.Ui\nimport Ljos.Models as Models\nItem {\n    Repeater { model: Models.ListModel { Models.ListElement { city: qsTr(\"Reykjav\u{ed}k\") } } }\n}\n",
    ] {
        let found = found(source);
        assert!(
            !found.iter().any(|(code, ..)| *code == Code::QtTr),
            "{source}: {found:?}"
        );
    }
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
fn a_sum_that_starts_with_a_translated_text_is_two_messages_too() {
    // The sum starts where its first value does, which may have a
    // finding of its own; both are said.
    for (value, one) in [
        ("t`x` + \"y\"", "t`xy`"),
        (
            "plural(1, { other: \"x\" }) + \"y\"",
            "t`${plural(1, { other: \"x\" })}y`",
        ),
    ] {
        let found = found(&format!(
            "import Ljos.Ui\nimport Ljos.Core\nItem {{\n    property string a: {value}\n    function f(): string {{ return {value} }}\n}}\n"
        ));
        let concat: Vec<_> = found
            .iter()
            .filter(|(code, ..)| *code == Code::I18nConcat)
            .collect();
        assert_eq!(concat.len(), 2, "{value}: {found:?}");
        assert!(
            concat[0].2.ends_with(&format!("write one, {one}")),
            "{value}: {found:?}"
        );
    }
}

#[test]
fn numbers_added_before_the_text_stay_one_value() {
    // `n + 1 + t`x`` is "3x" for n = 2, not "21x".
    let found = found(
        "import Ljos.Ui\nimport Ljos.Core\nItem {\n    property int n: 2\n    property string a: n + 1 + t`x`\n}\n",
    );
    let concat: Vec<_> = found
        .iter()
        .filter(|(code, ..)| *code == Code::I18nConcat)
        .collect();
    assert_eq!(concat.len(), 1, "{found:?}");
    assert_eq!(
        concat[0].2,
        "a translated text joined to another value is two messages: write one, t`${n + 1}x`"
    );
}

#[test]
fn a_group_in_a_sum_is_one_value_and_keeps_its_parentheses() {
    // `+` adds a parenthesised group first: `n + (1 + 2) + t`x`` is "5x"
    // for n = 2, `t`x` + (1 + 2)` is "x3", and a group on the left adds as
    // its parts do but its text keeps the parentheses it opens.
    for (value, one) in [
        ("n + (1 + 2) + t`x`", "t`${n + (1 + 2)}x`"),
        ("((n + 1) + 2) + t`x`", "t`${((n + 1) + 2)}x`"),
        ("t`x` + (1 + 2)", "t`x${1 + 2}`"),
    ] {
        let found = found(&format!(
            "import Ljos.Ui\nimport Ljos.Core\nItem {{\n    property int n: 2\n    property string a: {value}\n}}\n"
        ));
        let concat: Vec<_> = found
            .iter()
            .filter(|(code, ..)| *code == Code::I18nConcat)
            .collect();
        assert_eq!(concat.len(), 1, "{value}: {found:?}");
        assert!(
            concat[0].2.ends_with(&format!("write one, {one}")),
            "{value}: {found:?}"
        );
    }
}

#[test]
fn a_text_inside_a_group_is_the_group_s_sum() {
    // `n + (1 + t`x`)` is "21x" for n = 2: the group joins first, so the
    // message is the group's, and `n` is joined to it after.
    let found = found(
        "import Ljos.Ui\nimport Ljos.Core\nItem {\n    property int n: 2\n    property string a: n + (1 + t`x`)\n}\n",
    );
    let concat: Vec<_> = found
        .iter()
        .filter(|(code, ..)| *code == Code::I18nConcat)
        .collect();
    assert_eq!(concat.len(), 1, "{found:?}");
    assert_eq!(
        concat[0].2,
        "a translated text joined to another value is two messages: write one, t`${1}x`"
    );
}

#[test]
fn a_group_that_holds_a_text_is_said_alone() {
    // One message cannot hold another (`t`a${t`b` + c}``): the group is
    // said first, and the sum around it once the group is one text.
    let found = found(
        "import Ljos.Ui\nimport Ljos.Core\nItem {\n    property string c: \"y\"\n    property string a: t`a` + (t`b` + c)\n}\n",
    );
    let concat: Vec<_> = found
        .iter()
        .filter(|(code, ..)| *code == Code::I18nConcat)
        .collect();
    assert_eq!(concat.len(), 1, "{found:?}");
    assert_eq!(
        concat[0].2,
        "a translated text joined to another value is two messages: write one, t`b${c}`"
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
