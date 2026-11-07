//! Tests of groups: files of a project that name one another.

use super::*;

#[test]
fn a_file_of_the_project_is_the_type_its_root_is() {
    let locals = |name: &str| (name == "Swatch").then(|| root_of("Rectangle", &[]));
    let Outcome::Lowered(lowered) = lower(
        "import QtQuick\nItem { Swatch { id: s; color: \"red\" } Text { color: s.color } }\n",
        &locals,
    ) else {
        panic!("not lowered");
    };
    assert!(
        lowered
            .text
            .contains("Swatch { id: s; fill: \"red\" } Text { color: s.fill }"),
        "{}",
        lowered.text
    );
}

#[test]
fn a_file_names_the_files_of_the_project_it_may_use() {
    let refs = references(
        "import QtQuick\nimport \"parts\"\nimport \"lib.js\" as Lib\nItem {\n    property Swatch s\n    \
         Button { }\n    Loader { source: \"panels/Side.qml\" }\n    \
         function f() { return Qt.createComponent(\"Pop.qml\") }\n}\n",
    );
    assert_eq!(refs.dirs, ["parts"]);
    assert_eq!(refs.files, ["Pop.qml", "panels/Side.qml"]);
    for ty in ["Item", "Swatch", "Button", "Loader", "Qt"] {
        assert!(refs.types.iter().any(|t| t == ty), "{ty}: {:?}", refs.types);
    }
}

/// A kept file that sets `color` on a lowered `Button` would find a `fill`.
#[test]
fn files_that_name_one_another_are_lowered_together() {
    let button = "import QtQuick\nRectangle { color: \"red\" }\n";
    let files = std::collections::BTreeMap::from([
        ("app/Button".to_string(), alone(button)),
        (
            "app/Main".to_string(),
            // Kept: `Flow` is not in Ljos.Ui.
            alone("import QtQuick\nFlow { Button { color: \"blue\" } }\n"),
        ),
        ("other/Lone".to_string(), alone(button)),
    ]);
    let outcomes = group(&files);
    let Outcome::Kept(kept) = &outcomes["app/Button"] else {
        panic!("{:?}", outcomes["app/Button"]);
    };
    assert!(
        kept.reason.contains("`Main.qml`, which is kept"),
        "{}",
        kept.reason
    );
    assert!(matches!(outcomes["app/Main"], Outcome::Kept(_)));
    assert!(matches!(outcomes["other/Lone"], Outcome::Lowered(_)));
}

#[test]
fn a_file_reached_by_address_or_directory_is_of_the_group() {
    let lowers = "import QtQuick\nRectangle { color: \"red\" }\n";
    let kept = "import QtQuick\nImage { }\n";
    for (main, other) in [
        (
            "import QtQuick\nLoader { source: \"../parts/Part.qml\" }\n",
            "parts/Part",
        ),
        (
            "import QtQuick\nimport \"../parts\"\nItem { }\n",
            "parts/Part",
        ),
    ] {
        let files = std::collections::BTreeMap::from([
            ("app/Main".to_string(), alone(main)),
            (other.to_string(), alone(kept)),
            ("app/Swatch".to_string(), alone(lowers)),
        ]);
        let outcomes = group(&files);
        assert!(matches!(outcomes["app/Main"], Outcome::Kept(_)), "{main}");
        // Named by nothing: alone.
        assert!(matches!(outcomes["app/Swatch"], Outcome::Lowered(_)));
    }
}

/// `import MyControls` keeps the importer; the module's `MyButton`, lowered
/// alone, would have a `fill` where the importer sets `color`.
#[test]
fn a_module_of_the_project_is_of_the_group_of_the_files_that_import_it() {
    let button = "import QtQuick\nRectangle { }\n";
    let main = "import QtQuick\nimport MyControls\nItem { MyButton { color: \"red\" } }\n";
    for module in ["app/MyControls/MyButton", "imports/MyControls/MyButton"] {
        let files = std::collections::BTreeMap::from([
            ("app/Main".to_string(), alone(main)),
            (module.to_string(), alone(button)),
            ("app/Lone".to_string(), alone(button)),
        ]);
        let outcomes = group(&files);
        assert!(matches!(outcomes["app/Main"], Outcome::Kept(_)));
        assert!(
            matches!(outcomes[module], Outcome::Kept(_)),
            "{module}: {:?}",
            outcomes[module]
        );
        assert!(matches!(outcomes["app/Lone"], Outcome::Lowered(_)));
    }
    // A dotted name is a directory per part.
    let dotted = "import QtQuick\nimport My.Controls\nItem { MyButton { color: \"red\" } }\n";
    let files = std::collections::BTreeMap::from([
        ("app/Main".to_string(), alone(dotted)),
        ("lib/My/Controls/MyButton".to_string(), alone(button)),
        ("lib/Controls/Other".to_string(), alone(button)),
    ]);
    let outcomes = group(&files);
    assert!(matches!(
        outcomes["lib/My/Controls/MyButton"],
        Outcome::Kept(_)
    ));
    assert!(matches!(
        outcomes["lib/Controls/Other"],
        Outcome::Lowered(_)
    ));
}

/// A project with a module `Gauges` of QML files (`Dial`, a `Rectangle`;
/// `Palette`, a singleton `QtObject`; `Text`, `Label` and `Timer`, types of
/// its own of Qt's names), a native module `Sensors`, and a directory `parts` with `Swatch`.
struct Project;

impl Locals for Project {
    fn root(&self, _: &str) -> Option<Root> {
        None
    }

    fn module(&self, uri: &str) -> Option<ProjectModule> {
        match uri {
            "Gauges" => Some(ProjectModule::Files),
            "Sensors" => Some(ProjectModule::Native),
            _ => None,
        }
    }

    fn imported(&self, from: Imported<'_>, name: &str) -> Option<Root> {
        match (from, name) {
            (Imported::Module("Gauges"), "Dial") => Some(root_of("Rectangle", &[])),
            (Imported::Module("Gauges"), "Palette") => Some(root_of("QtObject", &[])),
            (Imported::Module("Gauges"), "Text") => Some(root_of("Rectangle", &[])),
            (Imported::Module("Gauges"), "Label") => Some(root_of("Rectangle", &[])),
            (Imported::Module("Gauges"), "Timer") => Some(root_of("Rectangle", &[])),
            (Imported::Dir("parts"), "Swatch") => Some(root_of("Rectangle", &[])),
            _ => None,
        }
    }
}

fn lowered_in_project(source: &str) -> String {
    match lower(source, &Project) {
        Outcome::Lowered(lowered) => lowered.text,
        other => panic!("not lowered: {other:?}"),
    }
}

#[test]
fn a_module_of_the_project_is_lowered_and_its_types_are_their_roots() {
    let text = lowered_in_project(
        "import QtQuick\nimport Gauges\nItem { Dial { id: d; color: \"red\" } Rectangle { color: d.color } }\n",
    );
    assert_eq!(
        text,
        "pragma Scripts: JavaScript; import Ljos.Ui\nimport Gauges\n\
         Item { Dial { id: d; fill: \"red\" } Rectangle { fill: d.fill } }\n"
    );
}

#[test]
fn a_directory_imported_by_path_has_its_types_by_their_roots() {
    let text = lowered_in_project(
        "import QtQuick\nimport \"parts\"\nItem { Swatch { color: \"red\" } }\n",
    );
    assert!(text.contains("Swatch { fill: \"red\" }"), "{text}");
    // A script imported by path is no directory of types.
    let text = lowered_in_project(
        "import QtQuick\nimport \"lib.js\" as Lib\nItem { Rectangle { color: Lib.tint } }\n",
    );
    assert!(text.contains("fill: Lib.tint"), "{text}");
}

/// A module of the project may bring an `App` of its own: `Qt.platform`
/// stays Qt's, and a member Ljos.Core gives a lowercase name still lowers.
#[test]
fn a_member_of_qt_stays_qt_s_where_a_module_of_the_project_may_name_it() {
    assert_eq!(
        lowered_in_project(
            "import QtQuick\nimport Gauges\nItem { x: Qt.platform.os.length; y: Qt.point(1, 2).y }\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\n\
         import Gauges\nItem { x: Qt.platform.os.length; y: point(1, 2).y }\n"
    );
}

#[test]
fn a_singleton_of_the_project_is_read_as_its_root() {
    // A `QtObject` has no `fill`: its `color` is its own.
    let text =
        lowered_in_project("import QtQuick\nimport Gauges\nRectangle { color: Palette.color }\n");
    assert!(text.contains("fill: Palette.color"), "{text}");
}

#[test]
fn a_type_of_the_project_imported_after_qt_is_the_projects() {
    // `Text` of `Gauges` is a Rectangle: its `color` is a fill.
    let text =
        lowered_in_project("import QtQuick\nimport Gauges\nItem { Text { color: \"red\" } }\n");
    assert!(text.contains("Text { fill: \"red\" }"), "{text}");
    // Imported before Qt's, the name is Qt's `Text`.
    let text =
        lowered_in_project("import Gauges\nimport QtQuick\nItem { Text { color: \"red\" } }\n");
    assert!(text.contains("Text { color: \"red\" }"), "{text}");
}

#[test]
fn a_type_of_the_project_imported_after_qtqml_is_the_projects() {
    // `Timer` of `Gauges` is a Rectangle: its `color` is a fill, in a file
    // of QtQml alone as beside QtQuick.
    for qt in ["import QtQml", "import QtQuick\nimport QtQml"] {
        let text = lowered_in_project(&format!(
            "{qt}\nimport Gauges\nItem {{ Timer {{ color: \"red\" }} }}\n"
        ));
        assert!(text.contains("Timer { fill: \"red\" }"), "{text}");
    }
    // Imported before QtQml, the name is QtQml's `Timer`, whose members
    // keep their names.
    for qt in [
        "import Gauges\nimport QtQml",
        "import QtQuick\nimport Gauges\nimport QtQml",
    ] {
        let text = lowered_in_project(&format!("{qt}\nItem {{ Timer {{ color: \"red\" }} }}\n"));
        assert!(text.contains("Timer { color: \"red\" }"), "{text}");
    }
}

#[test]
fn a_module_the_project_does_not_have_as_files_keeps_the_file() {
    let kept = |source: &str| match lower(source, &Project) {
        Outcome::Kept(kept) => (kept.reason, kept.at),
        other => panic!("lowered: {other:?}"),
    };
    assert_eq!(
        kept("import QtQuick\nimport Sensors\nItem { }\n"),
        (
            "it imports `Sensors`, a module of the project whose types are native: they are not lowered"
                .to_string(),
            15
        )
    );
    assert_eq!(
        kept("import QtQuick\nimport Elsewhere\nItem { }\n").0,
        "it imports `Elsewhere`, which is not lowered yet"
    );
    assert_eq!(
        kept("import QtQuick\nimport Gauges as G\nItem { G.Dial { } }\n").0,
        "it imports `Gauges` under a name"
    );
}

/// The project's loader says which files a module's import brings: they
/// are of the importer's group wherever they are.
#[test]
fn the_files_a_module_import_brings_are_of_the_group() {
    let main = "import QtQuick\nimport Gauges\nItem { Dial { color: \"red\" } }\n";
    let importer = Alone {
        outcome: lower(main, &Project),
        references: references(main),
        uses: vec!["src/qml/Dial".to_string()],
    };
    assert!(matches!(importer.outcome, Outcome::Lowered(_)));
    let files = std::collections::BTreeMap::from([
        ("app/Main".to_string(), importer),
        (
            "src/qml/Dial".to_string(),
            alone("import QtQuick\nImage { }\n"),
        ),
    ]);
    let outcomes = group(&files);
    let Outcome::Kept(kept) = &outcomes["app/Main"] else {
        panic!("{:?}", outcomes["app/Main"]);
    };
    assert!(
        kept.reason.contains("`Dial.qml`, which is kept"),
        "{}",
        kept.reason
    );
}

#[test]
fn a_singleton_beside_the_file_is_read_as_its_root() {
    let tones = "pragma Singleton\nimport QtQml\nQtObject { property color color: \"#30a050\" }\n";
    assert_eq!(
        crate::root(tones).map(|root| root.type_name).as_deref(),
        Some("QtObject")
    );
    let locals = |name: &str| (name == "Tones").then(|| crate::root(tones)).flatten();
    let source = "import QtQuick\nRectangle {\n  Rectangle { color: Tones.color }\n}\n";
    match lower(source, &locals) {
        Outcome::Lowered(lowered) => assert!(lowered.text.contains("fill: Tones.color")),
        other => panic!("not lowered: {other:?}"),
    }
}

#[test]
fn a_type_of_the_project_imported_after_the_controls_is_the_projects() {
    let lowered = |source: &str| match lower(
        source,
        &Styled {
            locals: &Project,
            style: CONTROLS_STYLE,
            natives: None,
        },
    ) {
        Outcome::Lowered(lowered) => lowered.text,
        other => panic!("not lowered: {other:?}"),
    };
    // `Label` of `Gauges` is a Rectangle: its `color` is a fill.
    let text = lowered(
        "import QtQuick\nimport QtQuick.Controls\nimport Gauges\nItem { Label { color: \"red\" } }\n",
    );
    assert!(text.contains("Label { fill: \"red\" }"), "{text}");
    // Imported before the controls, the name is theirs.
    let text = lowered(
        "import QtQuick\nimport Gauges\nimport QtQuick.Controls\nItem { Label { color: \"red\" } }\n",
    );
    assert!(text.contains("Label { color: \"red\" }"), "{text}");
}
