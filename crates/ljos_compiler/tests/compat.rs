//! A project's files in Qt's QML lowered to Ljós's API as they load
//! (docs/adr/0004-compat-lowering-before-the-ir.md): files that name one
//! another are lowered together or kept together, and turning the pass off
//! gives back the files as written.

mod oracle;

use ljos_compiler::Project;
use oracle::Runtime;
use qml_compat::Outcome;

const BUTTON: &str = "import QtQuick\nRectangle { color: \"red\" }\n";

fn lowered(project: &Project<Runtime>, key: &str) -> bool {
    matches!(project.lowering(key), Some(Outcome::Lowered(_)))
}

#[test]
fn a_file_lowered_whole_is_compiled_as_lowered_and_written_as_it_was() {
    let mut project = Project::new(Runtime::new());
    project.add("Button", BUTTON);
    project.add(
        "Main",
        "import QtQuick\nItem { Button { color: \"blue\" } }\n",
    );
    project.set_compat(true);

    assert!(
        lowered(&project, "Button"),
        "{:?}",
        project.lowering("Button")
    );
    assert!(lowered(&project, "Main"), "{:?}", project.lowering("Main"));
    let button = project.source("Button").expect("Button");
    assert!(button.contains("import Ljos.Ui"), "{button}");
    assert!(button.contains("fill: \"red\""), "{button}");
    // Main sets the colour of a Button whose root is lowered: it says fill too.
    let main = project.source("Main").expect("Main");
    assert!(main.contains("fill: \"blue\""), "{main}");
    assert_eq!(project.written("Button"), Some(BUTTON));

    project.set_compat(false);
    assert_eq!(project.source("Button"), Some(BUTTON));
    assert_eq!(project.lowering("Button"), None);
}

#[test]
fn a_file_kept_keeps_the_files_it_names() {
    let mut project = Project::new(Runtime::new());
    project.add("Button", BUTTON);
    // A Behavior is not of Ljós's API yet: Main is kept, and so is the
    // Button it would set a `color` on.
    project.add(
        "Main",
        "import QtQuick\nRectangle { Behavior on color { } Button { color: \"blue\" } }\n",
    );
    project.add("other/Lone", BUTTON);
    project.set_compat(true);

    let Some(Outcome::Kept(kept)) = project.lowering("Button") else {
        panic!("Button is kept: {:?}", project.lowering("Button"));
    };
    assert!(
        kept.reason.contains("`Main.qml`, which is kept"),
        "{}",
        kept.reason
    );
    assert!(matches!(project.lowering("Main"), Some(Outcome::Kept(_))));
    assert_eq!(project.source("Button"), Some(BUTTON));
    // A file of another directory that names neither is lowered alone.
    assert!(lowered(&project, "other/Lone"));
}

#[test]
fn a_file_changed_while_lowering_is_lowered_with_its_group_again() {
    let mut project = Project::new(Runtime::new());
    project.add("Button", BUTTON);
    project.add("Main", "import QtQuick\nItem { Button { } }\n");
    project.set_compat(true);
    assert!(lowered(&project, "Button"));

    project.add(
        "Main",
        "import QtQuick\nItem { Behavior on x { } Button { } }\n",
    );
    assert!(matches!(project.lowering("Button"), Some(Outcome::Kept(_))));
    assert_eq!(project.source("Button"), Some(BUTTON));

    project.add("Main", "import QtQuick\nItem { Button { } }\n");
    assert!(lowered(&project, "Button"));
    assert!(project.source("Button").expect("Button").contains("fill:"));
}

#[test]
fn a_file_removed_leaves_its_group_and_the_files_that_named_it_lowered_again() {
    let mut project = Project::new(Runtime::new());
    project.add("Button", BUTTON);
    project.add(
        "Main",
        "import QtQuick\nItem { Button { color: \"blue\" } }\n",
    );
    project.add(
        "Kept",
        "import QtQuick\nRectangle { Behavior on color { } }\n",
    );
    project.add("Other", "import QtQuick\nItem { Kept { } }\n");
    project.set_compat(true);
    assert!(lowered(&project, "Main"));
    assert!(matches!(project.lowering("Other"), Some(Outcome::Kept(_))));

    // Without its Kept, Other is lowered.
    project.remove("Kept");
    assert_eq!(project.lowering("Kept"), None);
    assert!(
        lowered(&project, "Other"),
        "{:?}",
        project.lowering("Other")
    );

    // Main's `fill` was a Button's: without Button it is what Main was
    // written as again, as it would be had Button never been in.
    project.remove("Button");
    let mut alone = Project::new(Runtime::new());
    alone.add(
        "Main",
        "import QtQuick\nItem { Button { color: \"blue\" } }\n",
    );
    alone.set_compat(true);
    assert_eq!(project.lowering("Main"), alone.lowering("Main"));
    assert_eq!(project.source("Main"), alone.source("Main"));
}

#[test]
fn a_selectors_variant_is_lowered_in_the_place_of_its_file_and_gives_it_back() {
    const VARIANT: &str = "import QtQuick\nRectangle { color: \"green\" }\n";
    let mut project = Project::new(Runtime::new());
    project.set_controls_style(Some(qml_front::project::ControlsStyle {
        name: "Material".to_owned(),
        fallback: None,
    }));
    project.add("Button", BUTTON);
    project.add("Main", "import QtQuick\nItem { Button { } }\n");
    project.set_compat(true);

    project.add("+Material/Button", VARIANT);
    assert_eq!(project.written("Button"), Some(VARIANT));
    let source = project.source("Button").expect("Button");
    assert!(source.contains("fill: \"green\""), "{source}");
    assert!(lowered(&project, "Button"));

    // The file it was in the place of comes back as written, lowered again.
    project.remove("+Material/Button");
    assert_eq!(project.written("Button"), Some(BUTTON));
    let source = project.source("Button").expect("Button");
    assert!(source.contains("fill: \"red\""), "{source}");

    // Turned off, the pass gives back the file as written.
    project.add("+Material/Button", VARIANT);
    project.set_compat(false);
    assert_eq!(project.source("Button"), Some(VARIANT));
    project.remove("+Material/Button");
    assert_eq!(project.source("Button"), Some(BUTTON));
}

#[test]
fn a_style_chosen_while_lowering_lowers_the_controls_again() {
    use qml_front::project::ControlsStyle;
    let style = |name: &str| {
        Some(ControlsStyle {
            name: name.to_owned(),
            fallback: None,
        })
    };
    const MAIN: &str = "import QtQuick\nimport QtQuick.Controls\nLabel { text: \"Ready\" }\n";
    let mut project = Project::new(Runtime::new());
    project.add("Main", MAIN);
    project.set_controls_style(style("Basic"));
    project.set_compat(true);
    assert!(lowered(&project, "Main"), "{:?}", project.lowering("Main"));
    let main = project.source("Main").expect("Main");
    assert!(main.contains("import Ljos.Controls"), "{main}");

    // `import QtQuick.Controls` is Fusion now, which Ljos.Controls is not.
    project.set_controls_style(style("Fusion"));
    let Some(Outcome::Kept(kept)) = project.lowering("Main") else {
        panic!("Main is kept: {:?}", project.lowering("Main"));
    };
    assert!(kept.reason.contains("Fusion"), "{}", kept.reason);
    assert_eq!(project.source("Main"), Some(MAIN));
}

const GAUGES: &str = "module Gauges\nDial 1.0 Dial.qml\nsingleton Tones 1.0 Tones.qml\n";
const DIAL: &str = "import QtQuick\nRectangle { Rectangle { color: Tones.color } }\n";
const TONES: &str =
    "pragma Singleton\nimport QtQml\nQtObject { property color color: \"green\" }\n";
const GAUGE_MAIN: &str = "import QtQuick\nimport Gauges\nItem { Dial { color: Tones.color } }\n";

#[test]
fn a_module_of_the_project_is_lowered_with_the_files_that_import_it() {
    let mut project = Project::new(Runtime::new());
    project.add("Gauges/Dial", DIAL);
    project.add("Gauges/Tones", TONES);
    project.add("Main", GAUGE_MAIN);
    project.add_qmldir("Gauges", GAUGES);
    project.set_compat(true);

    assert!(lowered(&project, "Main"), "{:?}", project.lowering("Main"));
    assert!(lowered(&project, "Gauges/Dial"));
    // A Dial's root is a Rectangle: its colour is a fill.
    let main = project.source("Main").expect("Main");
    assert!(main.contains("Dial { fill: Tones.color }"), "{main}");
}

#[test]
fn a_module_told_after_its_importer_was_lowered_lowers_it_again() {
    // A project opened reads a module as a file reaches it, after the
    // file that imports it was lowered.
    let mut project = Project::new(Runtime::new());
    project.set_compat(true);
    project.add("Main", GAUGE_MAIN);
    let Some(Outcome::Kept(kept)) = project.lowering("Main") else {
        panic!("Main is kept: {:?}", project.lowering("Main"));
    };
    assert!(kept.reason.contains("not lowered yet"), "{}", kept.reason);

    project.add("Gauges/Dial", DIAL);
    project.add("Gauges/Tones", TONES);
    project.add_qmldir("Gauges", GAUGES);
    assert!(lowered(&project, "Main"), "{:?}", project.lowering("Main"));
    assert!(lowered(&project, "Gauges/Dial"));
}

#[test]
fn a_module_with_native_types_only_keeps_the_files_that_import_it() {
    let mut project = Project::new(Runtime::new());
    project.add("Main", "import QtQuick\nimport Sensors\nItem { }\n");
    project.add_qmldir("Sensors", "module Sensors\nplugin sensorsplugin\n");
    project.set_compat(true);

    let Some(Outcome::Kept(kept)) = project.lowering("Main") else {
        panic!("Main is kept: {:?}", project.lowering("Main"));
    };
    assert_eq!(
        kept.reason,
        "it imports `Sensors`, a module of the project whose types are native: they are not lowered"
    );
}

#[test]
fn a_directory_imported_by_path_has_its_types_by_their_roots() {
    let mut project = Project::new(Runtime::new());
    project.add("parts/Swatch", BUTTON);
    project.add(
        "Main",
        "import QtQuick\nimport \"parts\"\nItem { Swatch { color: \"blue\" } }\n",
    );
    project.set_compat(true);

    assert!(lowered(&project, "Main"), "{:?}", project.lowering("Main"));
    let main = project.source("Main").expect("Main");
    assert!(main.contains("Swatch { fill: \"blue\" }"), "{main}");
}
