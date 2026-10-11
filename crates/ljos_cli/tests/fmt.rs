//! `ljos fmt`: what it prints, what it writes and what status it leaves,
//! which is what a script and an editor are written against.

use std::{
    io::Write as _,
    path::Path,
    process::{Command, Stdio},
};

const UNTIDY: &str = "import QtQuick\nItem { width: 10 }\n";
const TIDY: &str = "import QtQuick\n\nItem {\n    width: 10\n}\n";
const BROKEN: &str = "import QtQuick\nItem {\n    width: 10 +\n}\n";

fn write(dir: &Path, name: &str, source: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

/// Runs `ljos fmt` in `dir`: its standard output, its standard error and
/// its status.
fn fmt(dir: &Path, args: &[&str]) -> (String, String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .arg("fmt")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("ljos runs");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().expect("ljos exits"),
    )
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Main.qml", UNTIDY);
    write(dir.path(), "parts/Part.qml", TIDY);
    dir
}

#[test]
fn a_file_is_laid_out_in_place() {
    let dir = project();
    let (out, err, status) = fmt(dir.path(), &["Main.qml"]);
    assert_eq!(out, "formatted Main.qml\n1 files, 1 formatted\n");
    assert_eq!(err, "");
    assert_eq!(status, 0);
    assert_eq!(read(dir.path(), "Main.qml"), TIDY);
}

#[test]
fn a_directory_is_walked_and_a_second_run_changes_nothing() {
    let dir = project();
    let (out, _, status) = fmt(dir.path(), &["."]);
    assert_eq!(out, "formatted Main.qml\n2 files, 1 formatted\n");
    assert_eq!(status, 0);
    let (out, _, status) = fmt(dir.path(), &["."]);
    assert_eq!(out, "2 files, 0 formatted\n");
    assert_eq!(status, 0);
}

#[test]
fn check_names_the_files_and_writes_nothing() {
    let dir = project();
    let (out, _, status) = fmt(dir.path(), &["--check", "."]);
    assert_eq!(
        out,
        "would format Main.qml\n2 files, 1 would be formatted\n"
    );
    assert_eq!(status, 1);
    assert_eq!(read(dir.path(), "Main.qml"), UNTIDY);
    let (out, _, status) = fmt(dir.path(), &["--check", "parts"]);
    assert_eq!(out, "1 files, 0 would be formatted\n");
    assert_eq!(status, 0);
}

#[test]
fn diff_shows_the_change_and_writes_nothing() {
    let dir = project();
    let (out, _, status) = fmt(dir.path(), &["--diff", "Main.qml"]);
    assert_eq!(
        out,
        "--- a/Main.qml\n+++ b/Main.qml\n@@ -1,2 +1,5 @@\n import QtQuick\n-Item { width: 10 }\n+\n+Item {\n+    width: 10\n+}\n1 files, 1 would be formatted\n"
    );
    assert_eq!(status, 0);
    assert_eq!(read(dir.path(), "Main.qml"), UNTIDY);
    // With `--check` the status still says that something is not laid out.
    let (_, _, status) = fmt(dir.path(), &["--diff", "--check", "Main.qml"]);
    assert_eq!(status, 1);
}

#[test]
fn a_file_that_does_not_parse_is_left_alone_and_the_rest_is_done() {
    let dir = project();
    write(dir.path(), "Broken.qml", BROKEN);
    let (out, err, status) = fmt(dir.path(), &["."]);
    assert_eq!(err, "Broken.qml:4:1: error: Unexpected token\n");
    assert_eq!(
        out,
        "formatted Main.qml\n3 files, 1 formatted, 1 not parsed\n"
    );
    assert_eq!(status, 2);
    assert_eq!(read(dir.path(), "Broken.qml"), BROKEN);
    assert_eq!(read(dir.path(), "Main.qml"), TIDY);
}

#[test]
fn a_file_that_is_not_there_is_an_error() {
    let dir = project();
    let (_, err, status) = fmt(dir.path(), &["Missing.qml"]);
    assert!(err.starts_with("Missing.qml: error: "), "{err}");
    assert_eq!(status, 2);
}

#[test]
fn build_output_and_hidden_directories_are_not_walked() {
    let dir = project();
    write(dir.path(), "target/Copy.qml", UNTIDY);
    write(dir.path(), ".cache/Copy.qml", UNTIDY);
    let (out, _, _) = fmt(dir.path(), &["--check", "."]);
    assert_eq!(
        out,
        "would format Main.qml\n2 files, 1 would be formatted\n"
    );
    // A file that is named is formatted wherever it is.
    let (out, _, _) = fmt(dir.path(), &["--check", "target/Copy.qml"]);
    assert_eq!(
        out,
        "would format target/Copy.qml\n1 files, 1 would be formatted\n"
    );
}

/// `ljos fmt -` with `input` on standard input.
fn piped(args: &[&str], input: &str) -> (String, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .arg("fmt")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("ljos runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().expect("ljos exits"),
    )
}

#[test]
fn standard_input_is_laid_out_to_standard_output() {
    let (out, err, status) = piped(&["-"], UNTIDY);
    assert_eq!(out, TIDY);
    assert_eq!(err, "");
    assert_eq!(status, 0);
    let (out, _, status) = piped(&["--check", "-"], UNTIDY);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    let (out, err, status) = piped(&["-"], BROKEN);
    assert_eq!(out, "");
    assert_eq!(err, "<stdin>:4:1: error: Unexpected token\n");
    assert_eq!(status, 2);
}
