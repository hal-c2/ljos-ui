//! Where a stand-in is recorded as written: the span a warning and a
//! reload point at covers the name in the source, spaces and all.

use ljos_compiler::{Project, compile_file};
use qml_front::QtTypeOracle;

/// The source text each stand-in of `text` is recorded at.
fn stood_in(text: &str) -> Vec<String> {
    let mut project = Project::new(QtTypeOracle);
    project.add("Main", text);
    let compiled = compile_file(&project, "Main").expect("compiles");
    compiled
        .file
        .imports
        .iter()
        .flat_map(|import| import.stood_in.iter().flatten())
        .map(|s| text[s.span.start as usize..s.span.end as usize].to_string())
        .collect()
}

#[test]
fn a_key_read_through_a_qualifier_is_recorded_at_the_qualified_name_as_written() {
    let text = "import QtQuick\nimport QtQuick3D\nimport QtQuick3D.Helpers as H\n\
                Item { property int origin: H . SceneGizmo.Center }\n";
    assert_eq!(stood_in(text), ["H . SceneGizmo"]);
}

#[test]
fn a_key_read_through_a_qualifier_without_spaces_is_recorded_at_it() {
    let text = "import QtQuick\nimport QtQuick3D\nimport QtQuick3D.Helpers as H\n\
                Item { property int origin: H.SceneGizmo.Center }\n";
    assert_eq!(stood_in(text), ["H.SceneGizmo"]);
}
