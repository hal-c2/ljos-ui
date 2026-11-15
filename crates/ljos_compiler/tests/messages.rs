//! `t` messages of `Ljos.Core`: what a binding lowers to, the text a
//! translator is given, and where a message that cannot be translated is
//! reported.

mod oracle;

use ljos_compiler::{Diagnostic, Project, compile_file};
use ljos_ir::{CompiledFile, TranslationKind};
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
