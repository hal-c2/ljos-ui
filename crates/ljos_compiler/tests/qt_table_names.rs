//! A property Qt's table writes with a C++ name: what the checker lets it
//! be given. PR CI builds without Qt's table, where such a property is
//! `var`, so only the oracle's names hold this on every change.

mod oracle;

use ljos_compiler::{Project, Severity, compile_file};
use oracle::Runtime;

/// The errors the checker says of a `Text` that holds `binding`.
fn errors(binding: &str) -> Vec<String> {
    let mut project = Project::new(Runtime::new());
    let source = format!("import QtQuick\nText {{\n    {binding}\n}}\n");
    project.add("Main", &source);
    let diagnostics = match compile_file(&project, "Main") {
        Ok(compiled) => compiled.diagnostics,
        Err(diagnostics) => diagnostics,
    };
    diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message)
        .collect()
}

#[test]
fn a_qvariantmap_takes_an_object() {
    assert_eq!(
        errors(r#"font.features: { "tnum": 1 }"#),
        Vec::<String>::new()
    );
}

#[test]
fn a_qvariantlist_takes_an_array_of_anything() {
    assert_eq!(
        errors(r#"font.fallbacks: [1, "two"]"#),
        Vec::<String>::new()
    );
}

/// The control: a property of a primitive the table names still refuses
/// what it is not, so the two above pass for their names, not because
/// nothing is checked.
#[test]
fn a_bool_still_refuses_a_string() {
    assert_eq!(errors(r#"font.bold: "yes""#).len(), 1);
}
