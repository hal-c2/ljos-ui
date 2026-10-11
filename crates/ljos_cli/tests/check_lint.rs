//! `ljos check`: the static lints (ARCHITECTURE.md §14). What has no name
//! for an assistive technology, and what a translator will never be given.
//!
//! The rules need Qt's table of types to tell a `Button` from an `Item`
//! with. A checkout without one (`mise run types`) is told so by `ljos
//! check`, and these tests then hold it to that and to nothing more.

use std::{path::Path, process::Command};

fn ljos(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ljos"))
        .args(args)
        .output()
        .expect("ljos runs")
}

fn write(dir: &Path, name: &str, source: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
}

/// What `ljos check` says of a project of one file: the lines of its
/// warnings and its count, or `None` where there is no table of types.
fn check(source: &str, more: &[&str]) -> Option<(Vec<String>, String)> {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Main.qml", source);
    let mut args = vec!["check"];
    args.extend(more);
    args.push(dir.path().to_str().unwrap());
    let out = ljos(&args);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "a lint is not an error: {err}");
    if err.contains("the static lints were skipped") {
        eprintln!("skipped: no Qt type table (run `mise run types`)");
        return None;
    }
    let lines = err.lines().map(str::to_string).collect();
    let count = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Some((lines, count))
}

const UNNAMED: &str = r#"import QtQuick
import QtQuick.Controls
Item {
    property string greeting: "hello"
    Rectangle {
        MouseArea { anchors.fill: parent; onClicked: console.log("pressed") }
    }
    Button { icon.source: "save.png" }
    Image { source: "logo.png" }
    Text { text: qsTr(greeting) }
}
"#;

#[test]
fn what_has_no_name_and_what_cannot_be_translated_is_a_warning() {
    let Some((lines, count)) = check(UNNAMED, &[]) else {
        return;
    };
    let at = |code: &str| -> Vec<&str> {
        lines
            .iter()
            .filter(|line| line.ends_with(&format!("[{code}]")))
            .map(|line| line.split(": warning: ").next().unwrap())
            .collect()
    };
    assert_eq!(
        at("a11y-unnamed-interactive"),
        ["Main.qml:6:9"],
        "{lines:#?}"
    );
    assert_eq!(
        at("a11y-unnamed-icon-button"),
        ["Main.qml:8:5"],
        "{lines:#?}"
    );
    assert_eq!(at("a11y-unnamed-image"), ["Main.qml:9:5"], "{lines:#?}");
    assert_eq!(
        at("i18n-qstr-non-literal"),
        ["Main.qml:10:18"],
        "{lines:#?}"
    );
    assert_eq!(count, "1 files, 0 errors, 4 warnings");
}

const NAMED: &str = r#"import QtQuick
import QtQuick.Controls
Item {
    Rectangle {
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Open")
        MouseArea { anchors.fill: parent; onClicked: console.log("pressed") }
    }
    Button { icon.source: "save.png"; Accessible.name: qsTr("Save") }
    Button { icon.source: "save.png"; text: qsTr("Save") }
    Image { source: "logo.png"; Accessible.name: qsTr("The logo") }
    Image { source: "flourish.png"; Accessible.ignored: true }
    Text { text: qsTr("Hello") }
}
"#;

#[test]
fn what_is_named_or_left_out_on_purpose_is_not() {
    let Some((lines, count)) = check(NAMED, &[]) else {
        return;
    };
    assert_eq!(lines, Vec::<String>::new());
    assert_eq!(count, "1 files, 0 errors, 0 warnings");
}

#[test]
fn a_text_that_is_not_translated_is_left_to_the_author() {
    // The authoring server hints at it; a build is not the place.
    let source = "import QtQuick\nText { text: \"Hello\" }\n";
    let Some((lines, count)) = check(source, &[]) else {
        return;
    };
    assert_eq!(lines, Vec::<String>::new());
    assert_eq!(count, "1 files, 0 errors, 0 warnings");
}

#[test]
fn no_lint_leaves_them_out() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Main.qml", UNNAMED);
    let out = ljos(&["check", "--no-lint", dir.path().to_str().unwrap()]);
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("a11y-"), "{err}");
    assert!(!err.contains("lints were skipped"), "{err}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "1 files, 0 errors, 0 warnings"
    );
}

#[test]
fn quiet_keeps_the_count_and_not_the_lines() {
    let Some((lines, count)) = check(UNNAMED, &["--quiet"]) else {
        return;
    };
    assert_eq!(lines, Vec::<String>::new());
    assert_eq!(count, "1 files, 0 errors, 4 warnings");
}

#[test]
fn a_file_of_a_tree_of_projects_is_named_under_its_project() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "demos/one/Main.qml", UNNAMED);
    let out = ljos(&["check", "--tree", dir.path().to_str().unwrap()]);
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    if err.contains("the static lints were skipped") {
        eprintln!("skipped: no Qt type table (run `mise run types`)");
        return;
    }
    assert!(
        err.lines()
            .any(|line| line.starts_with("demos/one/Main.qml:9:5: warning: ")
                && line.ends_with("[a11y-unnamed-image]")),
        "{err}"
    );
}
