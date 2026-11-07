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

// `Image` is not in Ljos.Ui yet, so this file is kept, and so is the
// `Button` it names.
const MAIN: &str = "import QtQuick
Item {
    Button { color: \"blue\" }
    Image { }
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
        "would lower app/Swatch.qml (2 edits)\n1 files, 1 lowered, 0 kept\n"
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
        "lowered app/Swatch.qml (2 edits)\n1 files, 1 lowered, 0 kept\n"
    );
    let written = read(dir.path(), "app/Swatch.qml");
    assert_eq!(written.lines().count(), SWATCH.lines().count());
    assert!(
        written.starts_with("pragma Scripts: JavaScript; import Ljos.Ui\n"),
        "{written}"
    );
    assert!(written.contains("    fill: level == 0"), "{written}");
    let (status, out, err) = ljos(dir.path(), &["check", "app"]);
    assert_eq!(status, 0, "{out}{err}");
    // Taking the pragma out is how the file is typed: the checker then says
    // what to change.
    let typed = written.replace("pragma Scripts: JavaScript; ", "");
    write(dir.path(), "app/Swatch.qml", &typed);
    let (status, _, err) = ljos(dir.path(), &["check", "app"]);
    assert_ne!(status, 0);
    assert!(err.contains("[ts-loose-equality]"), "{err}");
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
            &"kept app/Button.qml:1:1: it is used with `Main.qml`, which is kept: `Image` is not in Ljos.Ui yet"
        ),
        "{out}"
    );
    assert!(
        lines.contains(&"kept app/Main.qml:4:5: `Image` is not in Ljos.Ui yet"),
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
        "kept app/Main.qml:2:1: it imports `QtQuick.Controls`, which is the Fusion style where it runs, and Ljos.Controls is Basic\n1 files, 0 lowered, 1 kept\n"
    );
}
