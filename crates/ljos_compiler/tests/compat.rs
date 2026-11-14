//! A project's files in Qt's QML lowered to Ljós's API as they load
//! (docs/adr/0004-compat-lowering-before-the-ir.md): a kept file keeps the
//! files it uses, a file that uses a kept one keeps Qt's names on its type,
//! and turning the pass off gives back the files as written.

mod oracle;

use ljos_compiler::{Project, Roots};
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
    // An OpacityAnimator is not of Ljós's API yet: Main is kept, and so is the
    // Button it would set a `color` on.
    project.add(
        "Main",
        "import QtQuick\nRectangle { OpacityAnimator on opacity { } Button { color: \"blue\" } }\n",
    );
    project.add("other/Lone", BUTTON);
    project.set_compat(true);

    let Some(Outcome::Kept(kept)) = project.lowering("Button") else {
        panic!("Button is kept: {:?}", project.lowering("Button"));
    };
    assert!(
        kept.to_string().contains("`Main.qml`, which is kept"),
        "{}",
        kept
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
        "import QtQuick\nItem { OpacityAnimator on opacity { } Button { } }\n",
    );
    assert!(matches!(project.lowering("Button"), Some(Outcome::Kept(_))));
    assert_eq!(project.source("Button"), Some(BUTTON));

    project.add("Main", "import QtQuick\nItem { Button { } }\n");
    assert!(lowered(&project, "Button"));
    assert!(project.source("Button").expect("Button").contains("fill:"));
}

#[test]
fn a_file_removed_leaves_the_files_that_used_it_lowered_again() {
    let mut project = Project::new(Runtime::new());
    project.add("Button", BUTTON);
    project.add(
        "Main",
        "import QtQuick\nItem { Button { color: \"blue\" } }\n",
    );
    project.add(
        "Kept",
        "import QtQuick\nRectangle { OpacityAnimator on opacity { } }\n",
    );
    const OTHER: &str = "import QtQuick\nItem { Kept { color: \"blue\" } }\n";
    project.add("Other", OTHER);
    project.set_compat(true);
    assert!(lowered(&project, "Main"));
    // Other uses Kept, which stays Qt's: Other is lowered, and the `color`
    // it sets on a Kept stays Qt's name.
    assert!(matches!(project.lowering("Kept"), Some(Outcome::Kept(_))));
    assert!(
        lowered(&project, "Other"),
        "{:?}",
        project.lowering("Other")
    );
    let other = project.source("Other").expect("Other");
    assert!(other.contains("color: \"blue\""), "{other}");

    // Without Kept, Other is what it is lowered as alone.
    project.remove("Kept");
    assert_eq!(project.lowering("Kept"), None);
    let mut alone = Project::new(Runtime::new());
    alone.add("Other", OTHER);
    alone.set_compat(true);
    assert_eq!(project.lowering("Other"), alone.lowering("Other"));
    assert_eq!(project.source("Other"), alone.source("Other"));

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
    assert!(main.contains("import Ljos.Controls.Basic"), "{main}");

    // `import QtQuick.Controls` is Fusion now, which Ljos.Controls.Basic is not.
    project.set_controls_style(style("Fusion"));
    let Some(Outcome::Kept(kept)) = project.lowering("Main") else {
        panic!("Main is kept: {:?}", project.lowering("Main"));
    };
    assert!(kept.to_string().contains("Fusion"), "{}", kept);
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
    assert!(kept.to_string().contains("not lowered yet"), "{}", kept);

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
        kept.to_string(),
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

const TABLE: &str = "import QtQuick\nItem {\n    ListModel { id: zones; ListElement { city: \"Oslo\"; shift: 1 } }\n    Repeater { model: zones; Text { text: model.city } }\n}\n";
const OWN: &str = "export const shift = 1;\n";

/// A constant table is lowered with its rows in a module beside the file.
/// Where the project has a script of that path of its own, the file is
/// kept as written, as `ljos migrate` keeps it: its lowered text would
/// import rows the script does not declare.
#[test]
fn a_script_of_the_rows_module_s_path_keeps_the_file_and_is_the_project_s() {
    let mut project = Project::new(Runtime::new());
    project.add("Main", TABLE);
    project.add_script("Main.rows.ts", OWN);
    project.set_compat(true);
    let Some(Outcome::Kept(kept)) = project.lowering("Main") else {
        panic!("Main is kept: {:?}", project.lowering("Main"));
    };
    assert!(kept.to_string().contains("`Main.rows.ts`"), "{}", kept);
    assert_eq!(project.source("Main"), Some(TABLE));
    assert_eq!(project.script("Main.rows.ts"), Some(OWN));

    // Its script gone, the file is lowered with its rows; one added again
    // takes the module's place, and the file is kept again.
    project.remove_script("Main.rows.ts");
    let main = project.source("Main").expect("Main");
    assert!(main.contains("list<Rows.ZonesRow>"), "{main}");
    assert!(
        project
            .script("Main.rows.ts")
            .is_some_and(|rows| rows.contains("ZonesRow"))
    );
    project.add_script("Main.rows.ts", OWN);
    assert!(matches!(project.lowering("Main"), Some(Outcome::Kept(_))));
    assert_eq!(project.source("Main"), Some(TABLE));
    assert_eq!(project.script("Main.rows.ts"), Some(OWN));
    project.set_compat(false);
    assert_eq!(project.script("Main.rows.ts"), Some(OWN));
}

/// A script of the rows module's path that is the module the pass would
/// write, to the byte, is what `ljos migrate` wrote before: the file is
/// lowered again with it, and it stays the project's script as it is.
#[test]
fn a_script_that_is_the_rows_module_lowers_the_file_again() {
    let mut first = Project::new(Runtime::new());
    first.add("Main", TABLE);
    first.set_compat(true);
    let rows = first
        .script("Main.rows.ts")
        .expect("the pass writes the rows module")
        .to_owned();
    let lowered_text = first.source("Main").expect("Main").to_owned();

    let mut project = Project::new(Runtime::new());
    project.add("Main", TABLE);
    project.add_script("Main.rows.ts", rows.clone());
    project.set_compat(true);
    assert!(lowered(&project, "Main"), "{:?}", project.lowering("Main"));
    assert_eq!(project.source("Main"), Some(lowered_text.as_str()));
    assert_eq!(project.script("Main.rows.ts"), Some(rows.as_str()));
    // The pass did not write it, so the pass off leaves it.
    project.set_compat(false);
    assert_eq!(project.source("Main"), Some(TABLE));
    assert_eq!(project.script("Main.rows.ts"), Some(rows.as_str()));
}

#[test]
fn a_file_that_lowered_qt_to_app_is_lowered_again_when_an_app_qml_comes_and_goes() {
    const PAD: &str = "import QtQuick\nItem { x: Qt.platform.os.length }\n";
    let mut project = Project::new(Runtime::new());
    project.add("Pad", PAD);
    project.set_compat(true);
    let source = project.source("Pad").expect("Pad");
    assert!(source.contains("App.platform"), "{source}");

    // `App` is the file's now: `Qt.platform` stays Qt's.
    project.add("App", "import QtQuick\nItem { }\n");
    let source = project.source("Pad").expect("Pad");
    assert!(source.contains("x: Qt.platform"), "{source}");

    project.remove("App");
    let source = project.source("Pad").expect("Pad");
    assert!(source.contains("App.platform"), "{source}");
}

/// A project read without its tree (`load_roots`, what `--compat lower`
/// compiles) does not know which directories a qmldir describes: one may
/// name an `App` that is no file of it, so `Qt.platform` stays Qt's there,
/// as `ljos migrate` keeps it.
#[test]
fn a_directory_with_a_qmldir_leaves_qt_s_members_to_qt_without_the_tree() {
    let mut project = Project::new(Runtime::new());
    project.add("Site/Application", BUTTON);
    project.add_qmldir("Site", "module Site\nApp 1.0 Application.qml\n");
    project.add(
        "Main",
        "import QtQuick\nimport \"Site\"\nItem { x: Qt.platform.os.length }\n",
    );
    project.set_compat(true);

    let source = project.source("Main").expect("Main");
    assert!(!source.contains("App.platform"), "{source}");
}

/// A directory import needs no `module` line: a qmldir without one still
/// names the directory's types.
#[test]
fn a_qmldir_with_no_module_line_leaves_qt_s_members_to_qt() {
    let mut project = Project::new(Runtime::new());
    project.add("Site/Application", BUTTON);
    project.add(
        "Main",
        "import QtQuick\nimport \"Site\"\nItem { x: Qt.platform.os.length }\n",
    );
    project.set_compat(true);
    project.add_qmldir("Site", "App 1.0 Application.qml\n");

    let source = project.source("Main").expect("Main");
    assert!(!source.contains("App.platform"), "{source}");
}

/// A project opened reads its directories as they are reached
/// (`ljos run --compat lower`): a qmldir with no `module` line names its
/// directory's types there too.
#[test]
fn a_project_opened_reads_a_qmldir_with_no_module_line_as_its_directory_s_types() {
    let root = tempfile::tempdir().unwrap();
    let write = |path: &str, text: &str| {
        let path = root.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("Site/Application.qml", BUTTON);
    write("Site/qmldir", "App 1.0 Application.qml\n");
    write(
        "Main.qml",
        "import QtQuick\nimport \"Site\"\nItem { x: Qt.platform.os.length }\n",
    );
    let mut project = Project::open(root.path(), Runtime::new()).expect("readable");
    project.set_compat(true);
    project.reach("Main");

    let source = project.source("Main").expect("Main");
    assert!(!source.contains("App.platform"), "{source}");
}

/// A qmldir told after a file that imports its directory by path was
/// lowered may name the type that file brought in: it is lowered again.
#[test]
fn a_file_is_lowered_again_when_a_qmldir_comes_to_the_directory_it_imports() {
    let mut project = Project::new(Runtime::new());
    project.add("Site/Application", BUTTON);
    project.add(
        "Main",
        "import QtQuick\nimport \"Site\"\nItem { x: Qt.platform.os.length }\n",
    );
    project.set_compat(true);
    let source = project.source("Main").expect("Main");
    assert!(source.contains("App.platform"), "{source}");

    project.add_qmldir("Site", "module Site\nApp 1.0 Application.qml\n");
    let source = project.source("Main").expect("Main");
    assert!(!source.contains("App.platform"), "{source}");
}

const LABEL: &str = "import QtQuick\nimport QtQuick.Controls\nLabel { text: \"Ready\" }\n";
const TILES: &str = "import QtQuick\nItem { Tile { color: \"blue\" } }\n";

/// The files of `root`, read with `project`, lowered through [`Roots`] as
/// `ljos migrate` lowers them: each file's outcome, by its name.
fn lowered_in(
    project: Project<Runtime>,
    files: &[(&str, &str)],
) -> std::collections::BTreeMap<String, Outcome> {
    let root = std::path::PathBuf::from("/work/app");
    let mut roots = Roots::new();
    roots.push(root.clone(), project);
    let mut lowering = qml_compat::Lowering::new();
    for (name, text) in files {
        let key = roots.place(&root.join(format!("{name}.qml")), 0);
        lowering.put(&key, (*text).to_owned());
    }
    lowering.lower(&roots);
    let _ = lowering.settle(&roots);
    files
        .iter()
        .map(|(name, _)| {
            let key = ljos_compiler::roots::file_key(&root.join(format!("{name}.qml")));
            let outcome = lowering.lowering(&key).expect("each file has an outcome");
            ((*name).to_owned(), outcome.clone())
        })
        .collect()
}

/// A root that did not load is read as a project with nothing in it: its
/// files have nothing beside them, and the style it is given.
#[test]
fn a_file_of_a_root_with_nothing_in_it_has_nothing_beside_it() {
    use qml_front::project::ControlsStyle;
    let empty = |name: &str| {
        let mut project = Project::new(Runtime::new());
        project.set_controls_style(Some(ControlsStyle {
            name: name.to_owned(),
            fallback: None,
        }));
        project
    };
    let files = [("Main", LABEL), ("Tiles", TILES)];

    let basic = lowered_in(empty("Basic"), &files);
    assert!(matches!(basic["Main"], Outcome::Lowered(_)), "{basic:?}");
    // Beside it in a project, `Tile` would be a Rectangle; here it is not known.
    let Outcome::Kept(kept) = &basic["Tiles"] else {
        panic!("Tiles is kept: {basic:?}");
    };
    assert!(
        kept.to_string().contains("whose type is not known"),
        "{kept}"
    );

    let fusion = lowered_in(empty("Fusion"), &files);
    let Outcome::Kept(kept) = &fusion["Main"] else {
        panic!("Main is kept: {fusion:?}");
    };
    assert!(kept.to_string().contains("Fusion"), "{kept}");

    // The same root with `Tile` in it: Tiles is lowered with it.
    let mut project = empty("Basic");
    project.add("Tile", BUTTON);
    let loaded = lowered_in(project, &files);
    assert!(matches!(loaded["Tiles"], Outcome::Lowered(_)), "{loaded:?}");
}
