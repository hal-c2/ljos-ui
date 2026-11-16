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
    // A role of the model comes before what `Ljos.Core` gives; only a
    // call of a macro is the macro whatever the model has.
    let params = &file.exprs[0].params;
    assert!(
        matches!(&params[..], [Param { name, arg: Arg::Context { .. } }] if name == "t"),
        "{params:?}"
    );
}
