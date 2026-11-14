//! `ljos migrate`: what it says, what it writes, and that what it writes
//! compiles as the files did (docs/DIALECT.md §8).

use std::{path::Path, process::Command};

fn ljos(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("ljos runs");
    (
        out.status.code().expect("ljos exits"),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn write(dir: &Path, name: &str, source: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

// Qt's JavaScript the checker of Ljós's API refuses: `==`, `var`, `null`
// and a number as a condition.
const SWATCH: &str = "import QtQuick
Rectangle {
    property real level: 0
    color: level == 0 ? \"red\" : \"green\"
    function clear() { var was = level; if (level) { level = null } }
}
";

// `FocusScope` is not in Ljos.Ui yet, so this file is kept, and so is the
// `Button` it names.
const MAIN: &str = "import QtQuick
Item {
    Button { color: \"blue\" }
    FocusScope { }
}
";

const BUTTON: &str = "import QtQuick\nRectangle { color: \"red\" }\n";

#[test]
fn a_dry_run_says_what_it_would_do_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "app/Swatch.qml", SWATCH);
    let (status, out, err) = ljos(dir.path(), &["migrate", "app"]);
    assert_eq!(status, 0, "{err}");
    assert_eq!(
        out,
        "would lower app/Swatch.qml (2 edits)\n\
         scripts of app/Swatch.qml stay JavaScript: typed, ljos check finds 4\n\
         1 files, 1 lowered, 0 kept\n"
    );
    assert_eq!(read(dir.path(), "app/Swatch.qml"), SWATCH);
}

#[test]
fn a_migrated_file_compiles_as_the_file_did() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "app/Swatch.qml", SWATCH);
    let (status, out, err) = ljos(dir.path(), &["migrate", "--write", "app"]);
    assert_eq!(status, 0, "{err}");
    assert_eq!(
        out,
        "lowered app/Swatch.qml (2 edits)\n\
         scripts of app/Swatch.qml stay JavaScript: typed, ljos check finds 4\n\
         1 files, 1 lowered, 0 kept\n"
    );
    let written = read(dir.path(), "app/Swatch.qml");
    // The pragma takes a line of its own.
    assert_eq!(written.lines().count(), SWATCH.lines().count() + 1);
    assert!(
        written.starts_with("pragma Scripts: JavaScript\nimport Ljos.Ui\n"),
        "{written}"
    );
    assert!(written.contains("    fill: level == 0"), "{written}");
    let (status, out, err) = ljos(dir.path(), &["check", "app"]);
    assert_eq!(status, 0, "{out}{err}");
    // Taking the pragma out is how the file is typed: the checker then says
    // what to change.
    let typed = written.replace("pragma Scripts: JavaScript\n", "");
    write(dir.path(), "app/Swatch.qml", &typed);
    let (status, _, err) = ljos(dir.path(), &["check", "app"]);
    assert_ne!(status, 0);
    assert!(err.contains("[ts-loose-equality]"), "{err}");
}

/// Scripts that already keep the checker's rules are left typed: the
/// pragma would only turn rules off that the file keeps.
#[test]
fn a_file_whose_scripts_keep_the_rules_is_written_typed() {
    let dir = tempfile::tempdir().unwrap();
    let typed = SWATCH.replace("level == 0", "level === 0").replace(
        "var was = level; if (level) { level = null }",
        "if (level !== 0) { level = 0 }",
    );
    write(dir.path(), "app/Swatch.qml", &typed);
    let (status, out, err) = ljos(dir.path(), &["migrate", "--write", "app"]);
    assert_eq!(status, 0, "{err}");
    assert_eq!(
        out,
        "lowered app/Swatch.qml (2 edits)\n1 files, 1 lowered, 0 kept\n"
    );
    let written = read(dir.path(), "app/Swatch.qml");
    assert!(
        written.starts_with("import Ljos.Ui\nRectangle {\n"),
        "{written}"
    );
    let (status, out, err) = ljos(dir.path(), &["check", "app"]);
    assert_eq!(status, 0, "{out}{err}");
}

/// A file in Ljós's API is nothing to migrate, and a migrated file is one:
/// running the command again changes nothing.
#[test]
fn a_file_in_ljos_api_is_left_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let ljos_api = "import Ljos.Ui\nimport Ljos.Layout\nRectangle { fill: \"red\"; Column { } }\n";
    write(dir.path(), "app/Main.qml", ljos_api);
    let (status, out, err) = ljos(dir.path(), &["migrate", "--write", "app"]);
    assert_eq!(status, 0, "{err}");
    assert_eq!(out, "1 files, 0 lowered, 0 kept\n");
    assert_eq!(read(dir.path(), "app/Main.qml"), ljos_api);
}

/// A module of Ljós's own beside Qt's is in Ljós's API already: its import
/// stays as it is written, and what the file names of it keeps its names.
#[test]
fn a_ljos_module_beside_qt_s_stays_and_the_rest_is_lowered() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "app/Main.qml",
        "import QtQuick\nimport Ljos.Design\nRectangle { color: Colors.canvas }\n",
    );
    let (status, out, err) = ljos(dir.path(), &["migrate", "--write", "app"]);
    assert_eq!(status, 0, "{err}");
    assert_eq!(
        out,
        "lowered app/Main.qml (2 edits)\n1 files, 1 lowered, 0 kept\n"
    );
    assert_eq!(
        read(dir.path(), "app/Main.qml"),
        "import Ljos.Ui\nimport Ljos.Design\nRectangle { fill: Colors.canvas }\n"
    );
    let (status, out, err) = ljos(dir.path(), &["check", "app"]);
    assert_eq!(status, 0, "{out}{err}");
}

/// Beside Qt's modules, a Ljós module the pass writes for them keeps the
/// file as it was: its types would stand for Qt's, whose members are then
/// not renamed, and the lowered file would import it twice.
#[test]
fn a_ljos_module_the_pass_writes_beside_qt_s_keeps_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let ui = "import QtQuick\nimport Ljos.Ui\nRectangle { color: \"red\" }\n";
    let layout = "import QtQuick\nimport Ljos.Layout\nColumn { }\n";
    write(dir.path(), "app/Main.qml", ui);
    write(dir.path(), "app/Stack.qml", layout);
    let (status, out, err) = ljos(dir.path(), &["migrate", "--write", "app"]);
    assert_eq!(status, 0, "{err}");
    assert_eq!(
        out,
        "kept app/Main.qml:2:1: it imports `Ljos.Ui` beside Qt's modules, which lower to it\n\
         kept app/Stack.qml:2:1: it imports `Ljos.Layout` beside Qt's modules, which lower to it\n\
         2 files, 0 lowered, 2 kept\n"
    );
    assert_eq!(read(dir.path(), "app/Main.qml"), ui);
    assert_eq!(read(dir.path(), "app/Stack.qml"), layout);
}

/// What a `Slot` sets that no item has is not lowered away: the lowered
/// file keeps it, and the checker names it, as on any item.
#[test]
fn a_name_a_slot_has_not_is_left_for_check_to_name() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "app/Main.qml",
        "import QtQuick\nimport Ljos.Blocks\nColumn { Slot { name: \"x\"; bogus: 1 } }\n",
    );
    let (status, _, err) = ljos(dir.path(), &["migrate", "--write", "app"]);
    assert_eq!(status, 0, "{err}");
    assert!(read(dir.path(), "app/Main.qml").contains("Slot { name: \"x\"; bogus: 1 }"));
    let (_, out, err) = ljos(dir.path(), &["check", "app"]);
    assert!(
        format!("{out}{err}")
            .contains("warning: `bogus` is not a property of `Slot` [unknown-property]"),
        "{out}{err}"
    );
}

/// `Ljos.Blocks`, what a shell is built of, is a module of Ljós's own as
/// `Ljos.Design` is: a shell written against QtQuick beside it is lowered,
/// and its `Slot`s are carried over as they are written.
#[test]
fn a_shell_of_blocks_beside_qt_s_is_lowered() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "app/Main.qml",
        "import QtQuick\nimport Ljos.Blocks\nColumn { Slot { name: \"x\" } }\n",
    );
    let (status, out, err) = ljos(dir.path(), &["migrate", "--write", "app"]);
    assert_eq!(status, 0, "{err}");
    assert_eq!(
        out,
        "lowered app/Main.qml (1 edits)\n1 files, 1 lowered, 0 kept\n"
    );
    assert_eq!(
        read(dir.path(), "app/Main.qml"),
        "import Ljos.Ui\nimport Ljos.Layout\nimport Ljos.Blocks\nColumn { Slot { name: \"x\" } }\n"
    );
    let (status, out, err) = ljos(dir.path(), &["check", "app"]);
    assert_eq!(status, 0, "{out}{err}");
}

#[test]
fn a_kept_file_keeps_the_files_it_names_and_says_where() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "app/Main.qml", MAIN);
    write(dir.path(), "app/Button.qml", BUTTON);
    write(dir.path(), "other/Lone.qml", BUTTON);
    let (status, out, err) = ljos(dir.path(), &["migrate", "--write", "app", "other"]);
    assert_eq!(status, 0, "{err}");
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines.contains(
            &"kept app/Button.qml:1:1: it is used by `Main.qml`, which is kept: `FocusScope` is not in Ljos.Ui yet"
        ),
        "{out}"
    );
    assert!(
        lines.contains(&"kept app/Main.qml:4:5: `FocusScope` is not in Ljos.Ui yet"),
        "{out}"
    );
    assert!(lines.contains(&"lowered other/Lone.qml (2 edits)"), "{out}");
    assert_eq!(lines.last(), Some(&"3 files, 1 lowered, 2 kept"));
    assert_eq!(read(dir.path(), "app/Main.qml"), MAIN);
    assert_eq!(read(dir.path(), "app/Button.qml"), BUTTON);
}

/// A file named by its bare name is of the project of the directory the
/// command runs in: the files beside it are read for what their roots are.
#[test]
fn a_bare_file_name_is_of_the_project_it_is_run_in() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "Main.qml",
        "import QtQuick\nItem {\n    Button { color: \"blue\" }\n}\n",
    );
    write(dir.path(), "Button.qml", BUTTON);
    let (status, out, err) = ljos(
        dir.path(),
        &["migrate", "--write", "Main.qml", "Button.qml"],
    );
    assert_eq!(status, 0, "{out}{err}");
    assert!(out.ends_with("2 files, 2 lowered, 0 kept\n"), "{out}");
    // `Button`'s root is a Rectangle: its `color` is the API's `fill`.
    assert!(
        read(dir.path(), "Main.qml").contains("Button { fill: \"blue\" }"),
        "{}",
        read(dir.path(), "Main.qml")
    );
}

/// The group is the same whether the files are named by a relative or an
/// absolute path.
#[test]
fn an_absolute_path_groups_as_a_relative_one() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "app/Main.qml", MAIN);
    write(dir.path(), "app/Button.qml", BUTTON);
    let absolute = dir.path().join("app");
    let (status, out, err) = ljos(dir.path(), &["migrate", absolute.to_str().unwrap()]);
    assert_eq!(status, 0, "{err}");
    assert!(out.ends_with("2 files, 0 lowered, 2 kept\n"), "{out}");
}

#[cfg(unix)]
#[test]
fn a_file_it_cannot_read_or_write_is_status_2_and_the_rest_are_still_done() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "app/Swatch.qml", SWATCH);
    write(dir.path(), "locked/Lone.qml", BUTTON);
    let locked = dir.path().join("locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    std::fs::set_permissions(
        locked.join("Lone.qml"),
        std::fs::Permissions::from_mode(0o444),
    )
    .unwrap();
    let (status, out, err) = ljos(
        dir.path(),
        &["migrate", "--write", "app", "locked", "missing.qml"],
    );
    assert_eq!(status, 2, "{out}{err}");
    assert!(err.contains("missing.qml: "), "{err}");
    assert!(err.contains("locked/Lone.qml: "), "{err}");
    assert!(out.contains("lowered app/Swatch.qml (2 edits)"), "{out}");
    assert!(read(dir.path(), "app/Swatch.qml").contains("import Ljos.Ui"));
}

// `import QtQuick.Controls` is the style the program starts with.
const CONTROLS: &str = "import QtQuick\nimport QtQuick.Controls\nLabel { text: \"Ready\" }\n";

/// `ljos migrate` with no `--style`, in an environment that chooses
/// `style`: what a program started there would get.
fn migrate_in_style(dir: &Path, style: &str) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .current_dir(dir)
        .args(["migrate", "app"])
        .env("QT_QUICK_CONTROLS_STYLE", style)
        .env_remove("QT_QUICK_CONTROLS_CONF")
        .output()
        .expect("ljos runs");
    (
        out.status.code().expect("ljos exits"),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn without_a_style_the_controls_are_those_of_the_environment() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "app/Main.qml", CONTROLS);

    let (status, out, err) = migrate_in_style(dir.path(), "Basic");
    assert_eq!(status, 0, "{err}");
    assert_eq!(
        out,
        "would lower app/Main.qml (2 edits)\n1 files, 1 lowered, 0 kept\n"
    );

    let (status, out, err) = migrate_in_style(dir.path(), "Fusion");
    assert_eq!(status, 0, "{err}");
    assert_eq!(
        out,
        "kept app/Main.qml:2:1: it imports `QtQuick.Controls`, which is the Fusion style where it runs, and Ljos.Controls.Basic is Basic\n1 files, 0 lowered, 1 kept\n"
    );
}

/// A file named in a directory that cannot be listed is still read, but
/// its directory is not a project: the file is lowered with nothing beside
/// it, in the style a program started there would get.
#[cfg(unix)]
#[test]
fn a_file_of_a_directory_that_is_not_a_project_has_nothing_beside_it() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "app/Main.qml", CONTROLS);
    // Beside it in a project, `Tile` would be a Rectangle.
    write(
        dir.path(),
        "app/Tiles.qml",
        "import QtQuick\nItem { Tile { color: \"blue\" } }\n",
    );
    write(dir.path(), "app/Tile.qml", BUTTON);
    let app = dir.path().join("app");
    // Its files can be opened by name, but it cannot be listed.
    std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o311)).unwrap();
    // Listable again however the test ends, or the temporary directory
    // cannot be removed.
    struct Listable<'a>(&'a std::path::Path);
    impl Drop for Listable<'_> {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
    let _listable = Listable(&app);
    // A process that reads past permissions (root, CAP_DAC_READ_SEARCH)
    // lists it anyway, so its directory is a project there and the case
    // cannot be made. ljos_compiler's compat tests read such a root
    // without permission bits.
    if std::fs::read_dir(&app).is_ok() {
        eprintln!(
            "skipped: {} can still be listed by this process (root or CAP_DAC_READ_SEARCH), so it loads as a project",
            app.display()
        );
        return;
    }
    let run = |style: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_ljos"))
            .current_dir(dir.path())
            .args(["migrate", "app/Main.qml", "app/Tiles.qml"])
            .env("QT_QUICK_CONTROLS_STYLE", style)
            .env_remove("QT_QUICK_CONTROLS_CONF")
            .output()
            .expect("ljos runs");
        (
            out.status.code().expect("ljos exits"),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let basic = run("Basic");
    let fusion = run("Fusion");

    let (status, out, err) = basic;
    assert_eq!(status, 2, "{out}{err}");
    assert!(err.contains("app: "), "{err}");
    assert_eq!(
        out,
        "would lower app/Main.qml (2 edits)\n\
         kept app/Tiles.qml:2:15: `color` is set on the object, whose type is not known\n\
         2 files, 1 lowered, 1 kept\n"
    );
    let (status, out, err) = fusion;
    assert_eq!(status, 2, "{out}{err}");
    assert_eq!(
        out,
        "kept app/Main.qml:2:1: it imports `QtQuick.Controls`, which is the Fusion style where it runs, and Ljos.Controls.Basic is Basic\n\
         kept app/Tiles.qml:2:15: `color` is set on the object, whose type is not known\n\
         2 files, 0 lowered, 2 kept\n"
    );
}

/// A module file named alone is of the project that imports its module: the
/// directory its `qmldir` is found from, not its own. So a file there that
/// names it and was not named keeps it, as when the importer alone is named.
#[test]
fn a_module_file_named_alone_is_read_with_the_files_that_import_it() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "app/Gauges/qmldir",
        "module Gauges\nDial 1.0 Dial.qml\n",
    );
    write(dir.path(), "app/Gauges/Dial.qml", BUTTON);
    let main = "import QtQuick\nimport Gauges\nItem { Dial { color: \"blue\" } }\n";
    write(dir.path(), "app/Main.qml", main);
    let (status, out, err) = ljos(dir.path(), &["migrate", "--write", "app/Gauges/Dial.qml"]);
    assert_eq!(status, 0, "{out}{err}");
    assert!(out.starts_with("kept app/Gauges/Dial.qml:1:1: "), "{out}");
    assert!(out.contains("name it too"), "{out}");
    assert!(out.ends_with("1 files, 0 lowered, 1 kept\n"), "{out}");
    assert_eq!(read(dir.path(), "app/Gauges/Dial.qml"), BUTTON);
    assert_eq!(read(dir.path(), "app/Main.qml"), main);

    // A dotted module's import path is as many directories up as it has parts.
    write(
        dir.path(),
        "lib/Ui/Gauges/qmldir",
        "module Ui.Gauges\nDial 1.0 Dial.qml\n",
    );
    write(dir.path(), "lib/Ui/Gauges/Dial.qml", BUTTON);
    let main = "import QtQuick\nimport Ui.Gauges\nItem { Dial { color: \"blue\" } }\n";
    write(dir.path(), "lib/Main.qml", main);
    let (status, out, err) = ljos(
        dir.path(),
        &["migrate", "--write", "lib/Ui/Gauges/Dial.qml"],
    );
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        out.starts_with("kept lib/Ui/Gauges/Dial.qml:1:1: "),
        "{out}"
    );
    assert_eq!(read(dir.path(), "lib/Ui/Gauges/Dial.qml"), BUTTON);
}

/// The files that import a module are found however it is named: a file
/// of a directory imported by its path, a module file named from inside
/// its directory, and the module's directory named. None of these writes
/// the module file while its importer stays Qt's.
#[test]
fn a_module_is_read_with_its_importers_however_it_is_named() {
    let dir = tempfile::tempdir().unwrap();
    // A directory imported by its path, with no qmldir.
    write(dir.path(), "app/parts/Dial.qml", BUTTON);
    let main = "import QtQuick\nimport \"parts\"\nItem { Dial { color: \"blue\" } }\n";
    write(dir.path(), "app/Main.qml", main);
    let (status, out, err) = ljos(dir.path(), &["migrate", "--write", "app/parts/Dial.qml"]);
    assert_eq!(status, 0, "{out}{err}");
    assert!(out.starts_with("kept app/parts/Dial.qml:1:1: "), "{out}");
    assert!(out.contains("name it too"), "{out}");
    assert_eq!(read(dir.path(), "app/parts/Dial.qml"), BUTTON);

    // A module file named by its bare name, from inside its directory.
    write(
        dir.path(),
        "lib/Gauges/qmldir",
        "module Gauges\nDial 1.0 Dial.qml\n",
    );
    write(dir.path(), "lib/Gauges/Dial.qml", BUTTON);
    let main = "import QtQuick\nimport Gauges\nItem { Dial { color: \"blue\" } }\n";
    write(dir.path(), "lib/Main.qml", main);
    let gauges = dir.path().join("lib/Gauges");
    let (status, out, err) = ljos(&gauges, &["migrate", "--write", "Dial.qml"]);
    assert_eq!(status, 0, "{out}{err}");
    assert!(out.starts_with("kept Dial.qml:1:1: "), "{out}");
    assert_eq!(read(dir.path(), "lib/Gauges/Dial.qml"), BUTTON);

    // The module's directory named, from where the importer is, and from
    // inside it.
    for (at, named) in [(dir.path().to_owned(), "lib/Gauges"), (gauges, ".")] {
        let (status, out, err) = ljos(&at, &["migrate", "--write", named]);
        assert_eq!(status, 0, "{out}{err}");
        assert!(out.contains("Dial.qml:1:1: "), "{named}: {out}");
        assert!(
            out.ends_with("1 files, 0 lowered, 1 kept\n"),
            "{named}: {out}"
        );
        assert_eq!(read(dir.path(), "lib/Gauges/Dial.qml"), BUTTON);
    }
    assert_eq!(read(dir.path(), "lib/Main.qml"), main);
}

/// A file below the project root is read with the root's project, so a
/// module on the root's import path is one of its files however deep the
/// file is.
#[test]
fn an_importer_below_the_root_is_read_with_the_root_s_project() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "app/Gauges/qmldir",
        "module Gauges\nDial 1.0 Dial.qml\n",
    );
    write(dir.path(), "app/Gauges/Dial.qml", BUTTON);
    let page = "import QtQuick\nimport Gauges\nItem { Dial { color: \"red\" } }\n";
    write(dir.path(), "app/views/Page.qml", page);
    let (status, out, err) = ljos(dir.path(), &["migrate", "--write", "app/Gauges/Dial.qml"]);
    assert_eq!(status, 0, "{out}{err}");
    assert!(out.starts_with("kept app/Gauges/Dial.qml:1:1: "), "{out}");
    assert!(out.contains("it is used by `Page.qml`"), "{out}");
    assert_eq!(read(dir.path(), "app/Gauges/Dial.qml"), BUTTON);
    assert_eq!(read(dir.path(), "app/views/Page.qml"), page);
}

#[test]
fn an_importer_of_a_module_only_a_build_describes_is_read_with_it() {
    // No qmldir: the module is the build's, and the URI is in the
    // CMakeLists.txt of the named file's directory, or of one above it
    // that lists the file from a subdirectory.
    for (cmake_dir, file) in [("app/Gauges", "Dial.qml"), ("app/Gauges", "parts/Dial.qml")] {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            &format!("{cmake_dir}/CMakeLists.txt"),
            &format!("qt_add_qml_module(gauges URI Gauges QML_FILES {file})\n"),
        );
        let named = format!("{cmake_dir}/{file}");
        write(dir.path(), &named, BUTTON);
        let main = "import QtQuick\nimport Gauges\nItem { Dial { color: \"red\" } }\n";
        write(dir.path(), "app/Main.qml", main);
        let (status, out, err) = ljos(dir.path(), &["migrate", "--write", &named]);
        assert_eq!(status, 0, "{out}{err}");
        assert!(
            out.starts_with(&format!("kept {named}:1:1: ")),
            "{named}: {out}"
        );
        assert_eq!(read(dir.path(), &named), BUTTON, "{named}");
        assert_eq!(read(dir.path(), "app/Main.qml"), main, "{named}");
    }
}

/// A module of the path migrate would write the rows to that is there
/// already with other bytes, readable or not, is never overwritten, and
/// the file that would import it is not written either. One the project
/// reads as a script is the project's, and keeps the file; one it cannot
/// read is refused.
#[test]
fn a_rows_module_there_already_is_not_overwritten() {
    const TABLE: &str = "import QtQuick\nItem {\n    ListModel { id: zones; ListElement { city: \"Oslo\"; shift: 1 } }\n    Repeater { model: zones; Text { text: model.city } }\n}\n";
    let not_text: &[u8] = &[0xff, 0xfe, 0x00, b'x'];
    for there in [b"export const x = 1;\n".as_slice(), not_text] {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "app/Main.qml", TABLE);
        std::fs::write(dir.path().join("app/Main.rows.ts"), there).unwrap();
        let (status, out, err) = ljos(dir.path(), &["migrate", "--write", "app"]);
        if there == not_text {
            assert_eq!(status, 2, "{out}{err}");
            assert!(err.contains("app/Main.rows.ts: is there already"), "{err}");
        } else {
            assert_eq!(status, 0, "{out}{err}");
            assert!(
                out.starts_with(
                    "kept app/Main.qml:1:1: `Main.rows.ts` is a script of the project's"
                ),
                "{out}"
            );
        }
        assert_eq!(
            std::fs::read(dir.path().join("app/Main.rows.ts")).unwrap(),
            there
        );
        assert_eq!(read(dir.path(), "app/Main.qml"), TABLE);
    }
}

/// A file put back to its Qt text beside the rows module migrate wrote for
/// it (`git checkout` of the file leaves the module, untracked) is lowered
/// again as it was the first time, and the module, the one it would
/// write, is left as it is: not written again.
#[test]
fn a_file_put_back_beside_its_rows_module_is_lowered_again() {
    const TABLE: &str = "import QtQuick\nItem {\n    ListModel { id: zones; ListElement { city: \"Oslo\"; shift: 1 } }\n    Repeater { model: zones; Text { text: model.city } }\n}\n";
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "app/Main.qml", TABLE);
    let (status, first, err) = ljos(dir.path(), &["migrate", "--write", "app"]);
    assert_eq!(status, 0, "{first}{err}");
    assert!(first.contains("wrote app/Main.rows.ts"), "{first}");
    let main = read(dir.path(), "app/Main.qml");
    let rows = read(dir.path(), "app/Main.rows.ts");

    write(dir.path(), "app/Main.qml", TABLE);
    let (status, again, err) = ljos(dir.path(), &["migrate", "--write", "app"]);
    assert_eq!(status, 0, "{again}{err}");
    assert_eq!(again, first.replace("wrote app/Main.rows.ts\n", ""));
    assert_eq!(read(dir.path(), "app/Main.qml"), main);
    assert_eq!(read(dir.path(), "app/Main.rows.ts"), rows);
}

// A `.ts` module that uses Ljós's `match`, so migrate renames it to
// `.lts` (`ts-lts-syntax`), and the files that import it.
const SIZE: &str = "export type Size = \"small\" | \"large\"\nexport function gap(size: Size): number {\n    return match (size) { when \"small\": 4; when \"large\": 8; }\n}\n";
const LINE: &str =
    "import { gap } from \"./size.ts\"\nexport const wide: number = gap(\"large\")\n";
const PAD: &str = "import Ljos.Ui\nimport \"size.ts\" as Size\nItem { x: Size.gap(\"small\") }\n";
const MAIN_PAD: &str =
    "import Ljos.Ui\nimport \"lib/size.ts\" as Size\nItem { x: Size.gap(\"small\") }\n";

fn project(dir: &Path) {
    write(dir, "lib/size.ts", SIZE);
    write(dir, "lib/line.ts", LINE);
    write(dir, "lib/Pad.qml", PAD);
    write(dir, "Main.qml", MAIN_PAD);
}

/// A directory named: its module is renamed, and so is the import of a
/// file beside the directory, below the directory the command runs in.
#[test]
fn a_module_renamed_in_a_directory_is_renamed_in_the_files_beside_it_too() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let (status, out, err) = ljos(dir.path(), &["migrate", "--write", "lib"]);
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        out.starts_with(
            "renamed lib/size.ts to size.lts, and the imports that name it in Main.qml, lib/Pad.qml, lib/line.ts; files outside . were not looked in\n"
        ),
        "{out}"
    );
    assert!(!dir.path().join("lib/size.ts").exists());
    assert_eq!(read(dir.path(), "lib/size.lts"), SIZE);
    assert_eq!(
        read(dir.path(), "lib/line.ts"),
        LINE.replace("size.ts", "size.lts")
    );
    assert_eq!(
        read(dir.path(), "lib/Pad.qml"),
        PAD.replace("size.ts", "size.lts")
    );
    assert_eq!(
        read(dir.path(), "Main.qml"),
        MAIN_PAD.replace("size.ts", "size.lts")
    );
}

/// Run where the module's project is not, the importers are looked for
/// below the project alone, and the output says a file outside it was not
/// read.
#[test]
fn a_module_of_another_directory_says_where_its_importers_were_looked_for() {
    let dir = tempfile::tempdir().unwrap();
    project(&dir.path().join("app"));
    write(dir.path(), "tools/.keep", "");
    let lib = dir.path().join("app/lib");
    let module = lib.join("size.ts");
    let (status, out, err) = ljos(
        &dir.path().join("tools"),
        &["migrate", "--write", module.to_str().unwrap()],
    );
    assert_eq!(status, 0, "{out}{err}");
    let lib = lib.display();
    assert!(
        out.starts_with(&format!(
            "renamed {lib}/size.ts to size.lts, and the imports that name it in {lib}/Pad.qml, {lib}/line.ts; files outside {lib} were not looked in\n"
        )),
        "{out}"
    );
    assert_eq!(read(dir.path(), "app/Main.qml"), MAIN_PAD);
}

/// A file below that cannot be read may import the module: the module is
/// not renamed, and the file is named.
#[test]
fn a_file_it_cannot_read_leaves_the_module_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let mut pad = PAD.as_bytes().to_vec();
    pad.extend([0xff, 0xfe, b'\n']);
    std::fs::write(dir.path().join("lib/Pad.qml"), &pad).unwrap();
    let (code, out, err) = ljos(dir.path(), &["migrate", "--write", "lib"]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(err.contains("lib/Pad.qml: "), "{err}");
    assert!(!out.contains("renamed"), "{out}");
    assert_eq!(read(dir.path(), "lib/size.ts"), SIZE);
    assert!(!dir.path().join("lib/size.lts").exists());
    assert_eq!(std::fs::read(dir.path().join("lib/Pad.qml")).unwrap(), pad);
}

/// An importer whose write fails part way, as on a full disk: it and the
/// importers written before it get their text back, and the module its
/// name, so nothing names a file that is not there.
#[cfg(unix)]
#[test]
fn an_importer_it_cannot_write_leaves_every_file_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    // `lib/line.ts` is written last, in the order of the paths. It holds
    // 1024 bytes, the most the limit below lets a file hold, so its new
    // text, a byte longer, is cut off after its first 1024.
    let line = format!("{LINE}{}\n", "/".repeat(1024 - LINE.len() - 1));
    assert_eq!(line.len(), 1024);
    write(dir.path(), "lib/line.ts", &line);
    // bash's `ulimit -f` counts 1024-byte blocks. With SIGXFSZ ignored, a
    // write past the limit fails with EFBIG rather than ending the process.
    let run = Command::new("bash")
        .arg("-c")
        .arg("trap '' XFSZ; ulimit -f 1; exec \"$0\" migrate --write lib")
        .arg(env!("CARGO_BIN_EXE_ljos"))
        .current_dir(dir.path())
        .output()
        .expect("bash runs");
    let (out, err) = (
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    assert_eq!(run.status.code(), Some(2), "{out}{err}");
    assert!(err.contains("lib/line.ts: "), "{err}");
    assert!(!out.contains("renamed"), "{out}");
    assert_eq!(read(dir.path(), "lib/size.ts"), SIZE);
    assert!(!dir.path().join("lib/size.lts").exists());
    assert_eq!(read(dir.path(), "Main.qml"), MAIN_PAD);
    assert_eq!(read(dir.path(), "lib/Pad.qml"), PAD);
    assert_eq!(read(dir.path(), "lib/line.ts"), line);
}
