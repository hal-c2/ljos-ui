//! What a native method gives in a file of Ljós's API: the type Ljós's
//! table says, from the nearest type that declares the method, else
//! `unknown`, which a typed property does not take as it is.

mod oracle;

use ljos_compiler::{Code, Project, Severity, compile_file};
use oracle::Runtime;

/// What the checker says, as errors, of a Ljós file whose root is a
/// `root` with the id `g` and holds `binding`.
fn errors(root: &str, binding: &str) -> Vec<(Code, String)> {
    let mut project = Project::new(Runtime::new());
    let source = format!("import Ljos.Ui\n{root} {{\n    id: g\n    {binding}\n}}\n");
    project.add("Main", &source);
    let diagnostics = match compile_file(&project, "Main") {
        Ok(compiled) => compiled.diagnostics,
        Err(diagnostics) => diagnostics,
    };
    diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code, d.message))
        .collect()
}

fn gives(property: &str, it: &str) -> Vec<(Code, String)> {
    vec![(
        Code::TsAssignType,
        format!("`{property}` is {}; this gives it {it}", article(property)),
    )]
}

fn article(property: &str) -> &'static str {
    if property == "s" {
        "a string"
    } else {
        "a real"
    }
}

#[test]
fn a_method_that_gives_a_number_is_not_a_string() {
    assert_eq!(
        errors("Gauge", "readonly property string s: g.level()"),
        gives("s", "a number")
    );
    assert_eq!(errors("Gauge", "readonly property real r: g.level()"), []);
}

#[test]
fn a_method_the_table_says_nothing_of_gives_unknown() {
    assert_eq!(
        errors("Gauge", "readonly property string s: g.spin()"),
        gives("s", "an unknown value")
    );
}

/// `Dial` redeclares `label` and says nothing of what it gives, so its
/// `label` is `unknown`, not the `string` of `Gauge`'s; the `level` it
/// does not redeclare is `Gauge`'s number.
#[test]
fn the_nearest_declaration_says_what_a_method_gives() {
    assert_eq!(
        errors("Gauge", "readonly property real r: g.label()"),
        gives("r", "a string")
    );
    assert_eq!(
        errors("Dial", "readonly property real r: g.label()"),
        gives("r", "an unknown value")
    );
    assert_eq!(
        errors("Dial", "readonly property string s: g.level()"),
        gives("s", "a number")
    );
}
