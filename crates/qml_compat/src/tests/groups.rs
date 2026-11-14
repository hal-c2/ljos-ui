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
fn a_kept_file_keeps_the_files_it_uses() {
    let button = "import QtQuick\nRectangle { color: \"red\" }\n";
    let files = std::collections::BTreeMap::from([
        ("app/Button".to_string(), alone(button)),
        (
            "app/Main".to_string(),
            // Kept: `FocusScope` is not in Ljos.Ui.
            alone("import QtQuick\nFocusScope { Button { color: \"blue\" } }\n"),
        ),
        ("other/Lone".to_string(), alone(button)),
        // Uses the kept file, which keeps nothing it does not use.
        (
            "app/Window".to_string(),
            alone("import QtQuick\nItem { Main { } }\n"),
        ),
    ]);
    let outcomes = group(&files);
    let Outcome::Kept(kept) = &outcomes["app/Button"] else {
        panic!("{:?}", outcomes["app/Button"]);
    };
    assert!(
        kept.to_string().contains("`Main.qml`, which is kept"),
        "{}",
        kept
    );
    assert!(matches!(outcomes["app/Main"], Outcome::Kept(_)));
    assert!(matches!(outcomes["other/Lone"], Outcome::Lowered(_)));
    assert!(
        matches!(outcomes["app/Window"], Outcome::Lowered(_)),
        "{:?}",
        outcomes["app/Window"]
    );
}

/// What a kept file uses, it keeps however far: a file a kept one uses
/// keeps the files it uses in turn.
#[test]
fn a_kept_file_keeps_what_the_files_it_uses_use() {
    let files = std::collections::BTreeMap::from([
        (
            "app/Main".to_string(),
            alone("import QtQuick\nFocusScope { Panel { } }\n"),
        ),
        (
            "app/Panel".to_string(),
            alone("import QtQuick\nItem { Swatch { } }\n"),
        ),
        (
            "app/Swatch".to_string(),
            alone("import QtQuick\nRectangle { color: \"red\" }\n"),
        ),
    ]);
    let outcomes = group(&files);
    let Outcome::Kept(kept) = &outcomes["app/Swatch"] else {
        panic!("{:?}", outcomes["app/Swatch"]);
    };
    assert!(
        kept.to_string()
            .starts_with("it is used by `Main.qml`, which is kept"),
        "{kept}"
    );
}

/// A file left as it is, with no import the pass lowers, goes on in Qt's
/// names: what a kept file uses through it, it keeps.
#[test]
fn a_kept_file_keeps_what_it_uses_through_a_file_left_as_it_is() {
    let files = std::collections::BTreeMap::from([
        (
            "app/Main".to_string(),
            alone("import QtQuick\nFocusScope { Bridge { } }\n"),
        ),
        ("app/Bridge".to_string(), alone("Swatch { width: 4 }\n")),
        (
            "app/Swatch".to_string(),
            alone("import QtQuick\nRectangle { color: \"red\" }\n"),
        ),
    ]);
    assert!(matches!(files["app/Bridge"].outcome, Outcome::Unchanged));
    let outcomes = group(&files);
    assert!(matches!(outcomes["app/Bridge"], Outcome::Unchanged));
    let Outcome::Kept(kept) = &outcomes["app/Swatch"] else {
        panic!("{:?}", outcomes["app/Swatch"]);
    };
    assert!(
        kept.to_string()
            .starts_with("it is used by `Main.qml`, which is kept"),
        "{kept}"
    );
}

/// A file lowered against a kept one binds Qt's names on it: lowering that
/// one later would rename them under it. What it binds that Ljós calls as
/// Qt does, and what the file declares, rename nothing.
#[test]
fn a_file_in_ljoss_api_keeps_the_file_it_binds_qts_names_on() {
    let dial = "import QtQuick\nRectangle { property real angle: 0 }\n";
    for (main, held) in [
        (
            "Dial { color: \"red\" }",
            Some("binds `color` on it, which Ljós's `Rectangle` calls `fill`"),
        ),
        (
            "Dial { onColorChanged: {} }",
            Some("binds `onColorChanged` on it, which Ljós's `Rectangle` calls `onFillChanged`"),
        ),
        ("Dial { width: 4; angle: 1 }", None),
        (
            "Dial { Behavior on color { ColorAnimation { } } }",
            Some("binds `color` on it, which Ljós's `Rectangle` calls `fill`"),
        ),
        (
            "Dial { id: dial } PropertyChanges { target: dial; color: \"red\" }",
            Some("binds `color` on it, which Ljós's `Rectangle` calls `fill`"),
        ),
        (
            "Dial { id: dial } PropertyChanges { dial.color: \"red\" }",
            Some("binds `color` on it, which Ljós's `Rectangle` calls `fill`"),
        ),
        (
            "Dial { id: dial } Binding { target: dial; property: \"color\"; value: \"red\" }",
            Some("binds `color` on it, which Ljós's `Rectangle` calls `fill`"),
        ),
        (
            "Dial { id: dial } Binding { target: (dial); property: \"color\"; value: \"red\" }",
            Some("binds `color` on it, which Ljós's `Rectangle` calls `fill`"),
        ),
        (
            "Dial { id: dial } Binding { dial.color: \"red\" }",
            Some("binds `color` on it, which Ljós's `Rectangle` calls `fill`"),
        ),
        (
            "Dial { id: dial } PropertyChanges { dial { color: \"red\" } }",
            Some("binds `color` on it, which Ljós's `Rectangle` calls `fill`"),
        ),
        (
            "Dial { id: dial } Connections { target: dial; function onColorChanged() {} }",
            Some("binds `onColorChanged` on it, which Ljós's `Rectangle` calls `onFillChanged`"),
        ),
        (
            "Dial { Connections { function onColorChanged() {} } }",
            Some("binds `onColorChanged` on it, which Ljós's `Rectangle` calls `onFillChanged`"),
        ),
        // An object a binding holds is the object's child too.
        (
            "Dial { data: [Connections { function onColorChanged() {} }] }",
            Some("binds `onColorChanged` on it, which Ljós's `Rectangle` calls `onFillChanged`"),
        ),
        (
            "Dial { property QtObject c: Connections { function onColorChanged() {} } }",
            Some("binds `onColorChanged` on it, which Ljós's `Rectangle` calls `onFillChanged`"),
        ),
        // An inline component's ids are its own.
        (
            "Dial { id: dial } component Other: Item { Item { id: dial } } \
             Binding { target: dial; property: \"color\"; value: \"red\" }",
            Some("binds `color` on it, which Ljós's `Rectangle` calls `fill`"),
        ),
        (
            "Dial { id: dial } PropertyChanges { target: dial; angle: 2; explicit: true }",
            None,
        ),
        // An inline component has its base's properties: what it does not
        // declare is bound on the dial.
        (
            "component LocalDial: Dial {} LocalDial { color: \"red\" }",
            Some("binds `color` on it, which Ljós's `Rectangle` calls `fill`"),
        ),
        (
            "component LocalDial: Dial {} component Big: LocalDial {} Big { onColorChanged: {} }",
            Some("binds `onColorChanged` on it, which Ljós's `Rectangle` calls `onFillChanged`"),
        ),
        (
            "component LocalDial: Dial { property color color } LocalDial { color: \"red\" }",
            None,
        ),
    ] {
        let main = format!("import Ljos.Ui\nRectangle {{ {main} }}\n");
        let files = std::collections::BTreeMap::from([
            ("app/Main".to_string(), alone(&main)),
            ("app/Dial".to_string(), alone(dial)),
        ]);
        assert!(matches!(files["app/Main"].outcome, Outcome::Unchanged));
        let outcomes = group(&files);
        match (held, &outcomes["app/Dial"]) {
            (Some(why), Outcome::Kept(kept)) => assert_eq!(
                kept.to_string(),
                format!(
                    "`Main.qml` is left as it is and {why}: rename it there as this file is lowered"
                )
            ),
            (None, Outcome::Lowered(_)) => {}
            (_, outcome) => panic!("{main}: {outcome:?}"),
        }
    }
    // Built on the dial, lowered with it, a file renames what the dial
    // does, and kept, it keeps the dial.
    let files = std::collections::BTreeMap::from([
        ("app/Dial".to_string(), alone(dial)),
        ("app/Big".to_string(), alone("import QtQuick\nDial { }\n")),
        (
            "app/Uses".to_string(),
            alone("import Ljos.Ui\nItem { Big { color: \"red\" } }\n"),
        ),
    ]);
    let outcomes = group(&files);
    let Outcome::Kept(kept) = &outcomes["app/Big"] else {
        panic!("{:?}", outcomes["app/Big"]);
    };
    assert!(
        kept.to_string()
            .starts_with("`Uses.qml` is left as it is and binds `color`"),
        "{kept}"
    );
    assert!(matches!(outcomes["app/Dial"], Outcome::Kept(_)));
}

/// A file that imports no Qt module is left as it is too, and binds Qt's
/// names on what it uses as one in Ljós's API does.
#[test]
fn a_file_left_as_it_is_without_imports_keeps_the_file_it_binds_qts_names_on() {
    let files = std::collections::BTreeMap::from([
        (
            "app/Swatch".to_string(),
            alone("import QtQuick\nRectangle { }\n"),
        ),
        (
            "app/Bridge".to_string(),
            alone("Swatch { color: \"red\" }\n"),
        ),
    ]);
    assert!(matches!(files["app/Bridge"].outcome, Outcome::Unchanged));
    let outcomes = group(&files);
    let Outcome::Kept(kept) = &outcomes["app/Swatch"] else {
        panic!("{:?}", outcomes["app/Swatch"]);
    };
    assert_eq!(
        kept.to_string(),
        "`Bridge.qml` is left as it is and binds `color` on it, which Ljós's `Rectangle` \
         calls `fill`: rename it there as this file is lowered"
    );
}

/// An inline component of another file, named qualified by it
/// (`Kit.Dial`), is that file's: what is bound on it keeps the file, read
/// through the components it is built on to the type they end on.
#[test]
fn a_file_binding_on_an_inline_component_of_another_keeps_it() {
    let gauge = "import QtQuick\nRectangle { }\n";
    for (kit, main, held) in [
        (
            "Item { component Dial: Rectangle {} }",
            "Kit.Dial { color: \"red\" }",
            Some("`color`"),
        ),
        (
            "Item { component Dial: Rectangle {} }",
            "Kit.Dial { width: 4 }",
            None,
        ),
        (
            "Item { component Base: Rectangle {} component Dial: Base {} }",
            "Kit.Dial { onColorChanged: {} }",
            Some("`onColorChanged`"),
        ),
        (
            "Item { component Dial: Gauge {} }",
            "Kit.Dial { color: \"red\" }",
            Some("`color`"),
        ),
        (
            "Item { component Dial: Rectangle {} }",
            "Kit.Dial { id: dial } Binding { target: dial; property: \"color\"; value: \"red\" }",
            Some("`color`"),
        ),
    ] {
        let files = std::collections::BTreeMap::from([
            ("app/Gauge".to_string(), alone(gauge)),
            // A file named as the component is not what `Kit.Dial` names.
            ("app/Dial".to_string(), alone(gauge)),
            (
                "app/Kit".to_string(),
                alone(&format!("import QtQuick\n{kit}\n")),
            ),
            (
                "app/Main".to_string(),
                alone(&format!("import Ljos.Ui\nItem {{ {main} }}\n")),
            ),
        ]);
        assert!(
            matches!(files["app/Kit"].outcome, Outcome::Lowered(_)),
            "{kit}: {:?}",
            files["app/Kit"].outcome
        );
        let outcomes = group(&files);
        match (held, &outcomes["app/Kit"]) {
            (Some(name), Outcome::Kept(kept)) => assert!(
                kept.to_string().starts_with(&format!(
                    "`Main.qml` is left as it is and binds {name} on it"
                )),
                "{main}: {kept}"
            ),
            (None, Outcome::Lowered(_)) => {}
            (_, outcome) => panic!("{kit} / {main}: {outcome:?}"),
        }
        assert!(
            matches!(outcomes["app/Dial"], Outcome::Lowered(_)),
            "{main}: {:?}",
            outcomes["app/Dial"]
        );
    }
}

#[test]
fn a_file_reached_by_address_or_directory_is_of_the_group() {
    let lowers = "import QtQuick\nRectangle { color: \"red\" }\n";
    // Kept: `FocusScope` is not in Ljos.Ui.
    for (main, other) in [
        (
            "import QtQuick\nFocusScope { Loader { source: \"../parts/Part.qml\" } }\n",
            "parts/Part",
        ),
        (
            "import QtQuick\nimport \"../parts\"\nFocusScope { }\n",
            "parts/Part",
        ),
    ] {
        let files = std::collections::BTreeMap::from([
            ("app/Main".to_string(), alone(main)),
            (other.to_string(), alone(lowers)),
            ("app/Swatch".to_string(), alone(lowers)),
        ]);
        let outcomes = group(&files);
        assert!(matches!(outcomes[other], Outcome::Kept(_)), "{main}");
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
/// its own of Qt's names, and `App`, of Ljós's), a native module `Sensors`,
/// and a directory `parts` with `Swatch`. It knows every type of each.
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
            (Imported::Module("Gauges"), "App") => Some(root_of("QtObject", &[])),
            (Imported::Dir("parts"), "Swatch") => Some(root_of("Rectangle", &[])),
            _ => None,
        }
    }

    fn imports_type(&self, from: Imported<'_>, name: &str) -> Option<bool> {
        Some(self.imported(from, name).is_some())
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

/// A module of the project that brings an `App` of its own leaves
/// `Qt.platform` nowhere to go, and keeps the file.
#[test]
fn a_member_of_qt_whose_home_a_module_of_the_project_may_name_keeps_the_file() {
    match lower(
        "import QtQuick\nimport Gauges\nItem { x: Qt.platform.os.length; y: Qt.point(1, 2).y }\n",
        &Project,
    ) {
        Outcome::Kept(kept) => assert_eq!(
            kept.to_string(),
            "`Qt.platform` needs `App`, which a type of a directory the file imports could be"
        ),
        other => panic!("lowered: {other:?}"),
    }
}

/// Where the project knows every type of what the file imports, and none
/// is `App`, `Qt.platform` is `App.platform`.
#[test]
fn a_member_of_qt_whose_home_no_type_the_file_imports_is_lowered() {
    let text =
        lowered_in_project("import QtQuick\nimport \"parts\"\nItem { x: Qt.platform.os.length }\n");
    assert!(text.contains("x: App.platform.os.length"), "{text}");
    assert!(text.contains("import Ljos.Core"), "{text}");
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
        Outcome::Kept(kept) => (kept.to_string(), kept.at),
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
    // Kept: `FocusScope` is not in Ljos.Ui.
    let main = "import QtQuick\nimport Gauges\nFocusScope { Dial { } }\n";
    let importer = Alone {
        outcome: lower(main, &Project),
        references: references(main),
        uses: vec!["src/qml/Dial".to_string()],
    };
    assert!(matches!(importer.outcome, Outcome::Kept(_)));
    let files = std::collections::BTreeMap::from([
        ("app/Main".to_string(), importer),
        (
            "src/qml/Dial".to_string(),
            alone("import QtQuick\nRectangle { color: \"red\" }\n"),
        ),
    ]);
    let outcomes = group(&files);
    let Outcome::Kept(kept) = &outcomes["src/qml/Dial"] else {
        panic!("{:?}", outcomes["src/qml/Dial"]);
    };
    assert!(
        kept.to_string()
            .contains("used by `Main.qml`, which is kept"),
        "{}",
        kept
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

/// A type qualified by a namespace the file imports is that module's, by
/// its last name; one qualified by another name is an inline component of
/// the file of that name.
#[test]
fn a_namespace_qualifies_a_modules_type_and_a_file_name_its_component() {
    let found = crate::group::references(
        "import QtQuick as QQ\nimport QtQuick.Controls as C\n\
         QQ.Item { component Dial: QQ.Rectangle {} C.Button { text: \"a\" } Kit.Dial { color: \"red\" } }\n",
    );
    assert_eq!(found.components[0].base, "Rectangle");
    for bound in [("Button", "text"), ("Kit.Dial", "color")] {
        assert!(
            found
                .bound
                .contains(&(bound.0.to_string(), bound.1.to_string())),
            "{:?}",
            found.bound
        );
    }
    assert!(
        !found.bound.iter().any(|(on, _)| on == "Dial"),
        "{:?}",
        found.bound
    );
    for ty in ["Button", "Item", "Kit"] {
        assert!(
            found.types.iter().any(|t| t == ty),
            "{ty}: {:?}",
            found.types
        );
    }
    for ty in ["C", "QQ", "Dial"] {
        assert!(
            !found.types.iter().any(|t| t == ty),
            "{ty}: {:?}",
            found.types
        );
    }
}

/// A file left as it is that is built on a lowered one has its names: what
/// a file binds on it is bound on the lowered one, through any number of
/// such files, unless one declares the name itself.
#[test]
fn a_file_binding_on_a_wrapper_left_as_it_is_keeps_what_it_is_built_on() {
    for (wrappers, main, held) in [
        (
            vec![("Bridge", "Swatch { }")],
            "Bridge { color: \"red\" }",
            true,
        ),
        (
            vec![("Mid", "Swatch { }"), ("Bridge", "Mid { }")],
            "Bridge { onColorChanged: {} }",
            true,
        ),
        (vec![("Bridge", "Swatch { }")], "Bridge { width: 4 }", false),
        (
            vec![("Bridge", "Swatch { property color color }")],
            "Bridge { color: \"red\" }",
            false,
        ),
    ] {
        let mut files = std::collections::BTreeMap::from([
            (
                "app/Swatch".to_string(),
                alone("import QtQuick\nRectangle { }\n"),
            ),
            (
                "app/Main".to_string(),
                alone(&format!("import Ljos.Ui\nItem {{ {main} }}\n")),
            ),
        ]);
        for (name, source) in &wrappers {
            files.insert(format!("app/{name}"), alone(&format!("{source}\n")));
            assert!(
                matches!(files[&format!("app/{name}")].outcome, Outcome::Unchanged),
                "{source}"
            );
        }
        let outcomes = group(&files);
        match (held, &outcomes["app/Swatch"]) {
            (true, Outcome::Kept(kept)) => assert!(
                kept.to_string()
                    .starts_with("`Main.qml` is left as it is and binds `"),
                "{main}: {kept}"
            ),
            (false, Outcome::Lowered(_)) => {}
            (_, outcome) => panic!("{wrappers:?} / {main}: {outcome:?}"),
        }
    }
}

/// Where two files beside one may be its root, as two imported
/// directories have a file of that name, what either renames counts.
#[test]
fn a_root_two_files_may_be_is_read_through_either() {
    let files = std::collections::BTreeMap::from([
        (
            "app/a/Dial".to_string(),
            alone("import QtQuick\nItem { }\n"),
        ),
        (
            "app/b/Dial".to_string(),
            alone("import QtQuick\nRectangle { }\n"),
        ),
        (
            "app/Wrapper".to_string(),
            alone("import QtQuick\nimport \"a\"\nimport \"b\"\nDial { }\n"),
        ),
        (
            "app/Main".to_string(),
            alone("import Ljos.Ui\nItem { Wrapper { color: \"red\" } }\n"),
        ),
    ]);
    assert!(
        matches!(files["app/Wrapper"].outcome, Outcome::Lowered(_)),
        "{:?}",
        files["app/Wrapper"].outcome
    );
    let outcomes = group(&files);
    let Outcome::Kept(kept) = &outcomes["app/Wrapper"] else {
        panic!("{:?}", outcomes["app/Wrapper"]);
    };
    assert!(
        kept.to_string()
            .starts_with("`Main.qml` is left as it is and binds `color` on it"),
        "{kept}"
    );
}
