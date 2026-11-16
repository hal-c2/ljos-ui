//! `t` messages of `Ljos.Core`: what a binding lowers to, the text a
//! translator is given, and where a message that cannot be translated is
//! reported.

mod oracle;

use ljos_compiler::{Diagnostic, Project, compile_file};
use ljos_ir::{Arg, CompiledFile, Param, TranslationKind};
use oracle::Runtime;

fn compile(source: &str) -> (CompiledFile, Vec<Diagnostic>) {
    let mut project = Project::new(Runtime::new());
    project.add("Page", source);
    let compiled = compile_file(&project, "Page").expect("compiles");
    (compiled.file, compiled.diagnostics)
}

fn place(d: &Diagnostic) -> String {
    format!("{}:{} {}", d.span.line, d.span.column, d.message)
}

#[test]
fn a_message_with_values_is_one_translation_and_a_call() {
    let (file, diagnostics) = compile(
        r##"import QtQuick
import Ljos.Core
Item {
    property string name: "Ada"
    property int count: 2
    property string shown: t`${name} has ${plural(count, { one: "# file", other: "# files" })}`
}
"##,
    );
    assert_eq!(diagnostics, []);
    let message = file
        .translations
        .iter()
        .find(|t| t.kind == TranslationKind::Message)
        .expect("a message");
    assert_eq!(
        message.source,
        "{name} has {count, plural, one {# file} other {# files}}"
    );
    assert_eq!(message.placeholders, ["name", "count"]);
    assert!(
        file.js.source.contains("__ljos_message("),
        "{}",
        file.js.source
    );
}

#[test]
fn a_form_that_is_not_written_out_is_reported_at_its_value() {
    let (_, diagnostics) = compile(
        r##"import QtQuick
import Ljos.Core
Item {
    property int count: 2
    property string many: "# files"
    property string files: plural(count, { one: "# file", other: many })
}
"##,
    );
    let places: Vec<String> = diagnostics.iter().map(place).collect();
    assert!(
        places
            .iter()
            .any(|p| p.starts_with("6:66 a form of `plural`")),
        "{places:?}"
    );
}

#[test]
fn forms_without_other_are_reported_at_their_brace() {
    let (_, diagnostics) = compile(
        r##"import QtQuick
import Ljos.Core
Item {
    property string role: "admin"
    property string label: select(role, { admin: "Administrator" })
}
"##,
    );
    let places: Vec<String> = diagnostics.iter().map(place).collect();
    assert!(
        places
            .iter()
            .any(|p| p.starts_with("5:41 the forms of `select` need `other`")),
        "{places:?}"
    );
}

#[test]
fn a_choice_the_document_binds_itself_is_a_value_the_message_shows() {
    let (file, diagnostics) = compile(
        r##"import QtQuick
import Ljos.Core
Item {
    function select(role, forms) { return forms.other + " (" + role + ")" }
    property string label: t`${select("admin", { other: "Member" })}`
}
"##,
    );
    assert_eq!(diagnostics, []);
    let message = file
        .translations
        .iter()
        .find(|t| t.kind == TranslationKind::Message)
        .expect("a message");
    assert_eq!(message.source, "{0}");
    assert!(file.js.source.contains("select("), "{}", file.js.source);
}

#[test]
fn a_choice_in_a_delegate_is_a_form_though_a_model_role_could_have_its_name() {
    let (file, diagnostics) = compile(
        r##"import QtQuick
import Ljos.Core
ListView {
    model: 3
    delegate: Text {
        property int n: 2
        text: t`${plural(n, { one: "# file", other: "# files" })}`
        property string note: t /* a comment */ `Note`
        property string more: t // and one
            ({ message: "More" })
    }
}
"##,
    );
    assert_eq!(diagnostics, []);
    let message = file
        .translations
        .iter()
        .find(|t| t.kind == TranslationKind::Message)
        .unwrap_or_else(|| panic!("{:?}\n{}", file.translations, file.js.source));
    assert_eq!(message.source, "{n, plural, one {# file} other {# files}}");
    let mut sources: Vec<&str> = file
        .translations
        .iter()
        .filter(|t| t.kind == TranslationKind::Message)
        .map(|t| t.source.as_str())
        .collect();
    sources.sort_unstable();
    assert_eq!(
        sources,
        ["More", "Note", "{n, plural, one {# file} other {# files}}"]
    );
}

#[test]
fn a_call_of_a_choice_the_document_binds_is_no_message_and_hides_none() {
    let (file, diagnostics) = compile(
        r##"import QtQuick
import Ljos.Core
Item {
    function select(text) { return text }
    property string label: select(t`Hi`)
}
"##,
    );
    assert_eq!(diagnostics, []);
    let sources: Vec<&str> = file
        .translations
        .iter()
        .filter(|t| t.kind == TranslationKind::Message)
        .map(|t| t.source.as_str())
        .collect();
    assert_eq!(sources, ["Hi"]);
}

#[test]
fn a_role_named_as_a_macro_read_as_a_value_is_the_role() {
    let (file, _) = compile(
        r##"import QtQuick
import Ljos.Core
ListView {
    model: ListModel { ListElement { t: 1.5 } }
    delegate: Text { text: t }
}
"##,
    );
    // A role of the model comes before what `Ljos.Core` gives, and the
    // macro is what is there when the model has no role of the name.
    let params = &file.exprs[0].params;
    assert!(
        matches!(
            &params[..],
            [Param { name, arg: Arg::Context { fallback: Some(fallback), .. } }]
                if name == "t" && matches!(&**fallback, Arg::Core(core) if core == "t")
        ),
        "{params:?}"
    );
}

#[test]
fn a_message_given_to_t_in_a_delegate_is_the_role_or_the_macro() {
    let (file, diagnostics) = compile(
        r##"import QtQuick
import Ljos.Core
ListView {
    model: 3
    delegate: Text {
        property var message: msg`Open`
        text: t(message)
    }
}
"##,
    );
    assert_eq!(diagnostics, []);
    let given = file
        .exprs
        .iter()
        .flat_map(|e| &e.params)
        .find(|p| p.name == "t")
        .unwrap_or_else(|| panic!("{:?}", file.exprs));
    assert!(
        matches!(
            &given.arg,
            Arg::Context { fallback: Some(fallback), .. }
                if matches!(&**fallback, Arg::Core(core) if core == "t")
        ),
        "{given:?}"
    );
    // The message written out is lowered whatever the model has.
    assert!(
        file.translations.iter().any(|t| t.source == "Open"),
        "{:?}",
        file.translations
    );
}

#[test]
fn a_message_written_out_in_a_delegate_is_the_macro_though_the_model_has_the_role() {
    let (file, diagnostics) = compile(
        r##"import QtQuick
import Ljos.Core
ListView {
    // A model whose roles only the running program knows: `t` may be one.
    model: 3
    delegate: Text {
        text: t`Open` + t
    }
}
"##,
    );
    assert_eq!(diagnostics, []);
    assert!(
        file.translations.iter().any(|t| t.source == "Open"),
        "{:?}",
        file.translations
    );
    assert!(
        file.js.source.contains("__ljos_message("),
        "{}",
        file.js.source
    );
    // The `t` read as a value is still the role.
    let params = &file.exprs[0].params;
    assert!(
        matches!(&params[..], [Param { name, arg: Arg::Context { .. } }] if name == "t"),
        "{params:?}"
    );
}
