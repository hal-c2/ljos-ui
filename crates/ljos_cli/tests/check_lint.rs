//! `ljos check`: the static lints (ARCHITECTURE.md §14). What has no name
//! for an assistive technology, what a translator will never be given, and
//! the design rules the source can tell; how severe each is, and the
//! `ljos-allow` comments that keep a warning with a reason.
//!
//! The rules tell a `Button` from an `Item` by the table of types the build
//! has, which is Qt's where one was generated and Ljós's own where not:
//! these hold of both.

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

/// What `ljos check` says of a project of one file: whether it passed, the
/// lines of its findings and its count.
fn run(source: &str, more: &[&str]) -> (bool, Vec<String>, String) {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Main.qml", source);
    let mut args = vec!["check"];
    args.extend(more);
    args.push(dir.path().to_str().unwrap());
    let out = ljos(&args);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    let lines = err
        .lines()
        .filter(|l| !l.starts_with("ljos: "))
        .map(str::to_string)
        .collect();
    let count = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success(), lines, count)
}

/// [`run`] of a file whose findings are warnings at most.
fn check(source: &str, more: &[&str]) -> (Vec<String>, String) {
    let (passed, lines, count) = run(source, more);
    assert!(passed, "a warning is not a failure: {lines:#?}");
    (lines, count)
}

/// `file:line:column` of each finding of `code`, at `severity`.
fn at<'l>(lines: &'l [String], severity: &str, code: &str) -> Vec<&'l str> {
    lines
        .iter()
        .filter(|line| line.ends_with(&format!("[{code}]")))
        .filter_map(|line| {
            line.split(&format!(": {severity}: "))
                .next()
                .filter(|p| *p != line)
        })
        .collect()
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
fn what_has_no_name_is_an_error_and_fails_the_check() {
    let (passed, lines, count) = run(UNNAMED, &[]);
    assert!(!passed, "{lines:#?}");
    assert_eq!(
        at(&lines, "error", "a11y-unnamed-interactive"),
        ["Main.qml:6:9"],
        "{lines:#?}"
    );
    assert_eq!(
        at(&lines, "error", "a11y-unnamed-icon-button"),
        ["Main.qml:8:5"],
        "{lines:#?}"
    );
    // An image may be decoration nobody has said so of: a warning.
    assert_eq!(
        at(&lines, "warning", "a11y-unnamed-image"),
        ["Main.qml:9:5"],
        "{lines:#?}"
    );
    assert_eq!(
        at(&lines, "warning", "i18n-qstr-non-literal"),
        ["Main.qml:10:18"],
        "{lines:#?}"
    );
    // The pointer area no key reaches, the icon button no tooltip explains.
    assert_eq!(count, "1 files, 2 errors, 4 warnings");
}

#[test]
fn a_design_finding_links_its_guideline() {
    let (_, lines, _) = run(UNNAMED, &[]);
    assert!(
        lines.iter().any(|l| l
            .ends_with("(docs/design/accessibility.md#a11y-unnamed-image) [a11y-unnamed-image]")),
        "{lines:#?}"
    );
}

#[test]
fn lint_warn_makes_the_errors_warnings_for_code_that_is_not_yours() {
    let (lines, count) = check(UNNAMED, &["--lint=warn"]);
    assert_eq!(
        at(&lines, "warning", "a11y-unnamed-icon-button"),
        ["Main.qml:8:5"],
        "{lines:#?}"
    );
    assert_eq!(count, "1 files, 0 errors, 6 warnings");
}

const ALLOWED: &str = r#"import QtQuick
import QtQuick.Controls
Item {
    // ljos-allow(focus-pointer-only): the same action is on the toolbar
    Rectangle {
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Open")
        MouseArea { anchors.fill: parent; onClicked: console.log("pressed") }
    }
}
"#;

#[test]
fn an_allow_with_a_reason_keeps_a_warning_in_the_object_it_opens() {
    let (lines, count) = check(ALLOWED, &[]);
    assert_eq!(lines, Vec::<String>::new());
    assert_eq!(count, "1 files, 0 errors, 0 warnings");
}

#[test]
fn an_allow_without_a_reason_is_an_error() {
    let source = ALLOWED.replace(": the same action is on the toolbar", "");
    let (passed, lines, count) = run(&source, &[]);
    assert!(!passed);
    assert_eq!(
        at(&lines, "error", "lint-allow-without-reason"),
        ["Main.qml:4:5"],
        "{lines:#?}"
    );
    assert_eq!(count, "1 files, 1 errors, 0 warnings");
}

#[test]
fn an_allow_never_keeps_an_error() {
    let source = UNNAMED.replace(
        "    Button {",
        "    // ljos-allow(a11y-unnamed-icon-button): the toolbar says what it does\n    Button {",
    );
    let (passed, lines, _) = run(&source, &[]);
    assert!(!passed);
    assert_eq!(
        at(&lines, "error", "a11y-unnamed-icon-button"),
        ["Main.qml:9:5"],
        "{lines:#?}"
    );
}

const NAMED: &str = r#"import QtQuick
import QtQuick.Controls
Item {
    Rectangle {
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Open")
        Accessible.onPressAction: console.log("pressed")
        MouseArea { anchors.fill: parent; onClicked: console.log("pressed") }
    }
    Button { icon.source: "save.png"; Accessible.name: qsTr("Save"); ToolTip.text: qsTr("Save") }
    Button { icon.source: "save.png"; text: qsTr("Save") }
    Image { source: "logo.png"; Accessible.name: qsTr("The logo") }
    Image { source: "flourish.png"; Accessible.ignored: true }
    Text { text: qsTr("Hello") }
}
"#;

#[test]
fn what_is_named_or_left_out_on_purpose_is_not() {
    let (lines, count) = check(NAMED, &[]);
    assert_eq!(lines, Vec::<String>::new());
    assert_eq!(count, "1 files, 0 errors, 0 warnings");
}

#[test]
fn a_text_that_is_not_translated_is_left_to_the_author() {
    // The authoring server hints at it; a build is not the place.
    let source = "import QtQuick\nText { text: \"Hello\" }\n";
    let (lines, count) = check(source, &[]);
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
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "1 files, 0 errors, 0 warnings"
    );
}

#[test]
fn quiet_keeps_the_count_and_not_the_lines() {
    let (lines, count) = check(UNNAMED, &["--quiet", "--lint=warn"]);
    assert_eq!(lines, Vec::<String>::new());
    assert_eq!(count, "1 files, 0 errors, 6 warnings");
}

#[test]
fn a_file_of_a_tree_of_projects_is_named_under_its_project() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "demos/one/Main.qml", UNNAMED);
    let out = ljos(&["check", "--tree", dir.path().to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.lines()
            .any(|line| line.starts_with("demos/one/Main.qml:8:5: error: ")
                && line.ends_with("[a11y-unnamed-icon-button]")),
        "{err}"
    );
}
