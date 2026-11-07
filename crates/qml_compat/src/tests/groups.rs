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
