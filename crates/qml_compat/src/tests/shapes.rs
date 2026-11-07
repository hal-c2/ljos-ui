//! Tests of shapes and paths: `Shape`, `Canvas`, gradients.

use super::*;

/// `Ljos.Ui` has Qt's shapes as they are: the import's line is left empty,
/// and nothing of the shapes changes, their enums read through the types
/// included.
#[test]
fn a_file_of_shapes_lowers_with_its_shapes_as_they_are() {
    let source = "import QtQuick\n\
import QtQuick.Shapes\n\
Shape {\n\
    preferredRendererType: Shape.CurveRenderer\n\
    ShapePath {\n\
        fillColor: \"red\"; capStyle: ShapePath.RoundCap\n\
        fillGradient: LinearGradient { GradientStop { position: 1; color: \"blue\" } }\n\
        PathSvg { path: \"M 0 0 L 10 10\" }\n\
        PathLine { x: 0; y: 10 }\n\
    }\n\
}\n";
    assert_eq!(
        lowered(source),
        format!(
            "pragma Scripts: JavaScript; import Ljos.Ui\n\n{}",
            source.split_once("Shapes\n").unwrap().1
        )
    );
    assert_eq!(
        kept_for("import QtQuick\nimport QtQuick.Shapes as S\nS.Shape { }\n"),
        "it imports `QtQuick.Shapes` under a name"
    );
}

/// Every type of `QtQuick.Shapes` the table describes is in `Ljos.Ui`, or
/// keeps a file that makes one, or reads an enum through it.
#[test]
fn a_type_of_qt_shapes_ljos_ui_has_not_keeps_the_file() {
    let ui = qt_types::module(MODULE).expect("Ljos.Ui is described");
    let types = qt_types::native_types("QtQuick.Shapes").expect("QtQuick.Shapes is described");
    assert!(types.contains(&"ShapePath"), "{types:?}");
    for name in types {
        let made = format!("import QtQuick\nimport QtQuick.Shapes\nItem {{ {name} {{ }} }}\n");
        let read =
            format!("import QtQuick\nimport QtQuick.Shapes\nItem {{ property int a: {name}.X }}\n");
        for source in [made, read] {
            let outcome = lower(&source, &no_locals);
            match (ui.type_named(name).is_some(), outcome) {
                (true, Outcome::Lowered(_)) => {}
                (false, Outcome::Kept(kept)) => {
                    assert_eq!(kept.reason, format!("`{name}` is not in {MODULE} yet"));
                }
                (has, other) => panic!("{name} (in Ljos.Ui: {has}): {other:?}"),
            }
        }
    }
}
