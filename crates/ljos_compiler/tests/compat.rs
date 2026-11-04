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
