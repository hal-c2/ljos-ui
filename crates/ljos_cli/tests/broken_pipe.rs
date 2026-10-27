//! A command whose report is piped into a program that stops reading
//! early (`ljos check | head -1`) ends quietly, with the status a shell
//! gives a program a closed pipe ended, not with a panic.

use std::{path::Path, process::Command};

/// What a shell reports for a process that SIGPIPE ended.
const BROKEN_PIPE: i32 = 128 + 13;

/// Runs `ljos args` in `dir` with stdout a pipe nobody reads from: its
/// reading end is closed before the command starts, so the first write
/// fails, whenever it comes.
fn into_closed_pipe(dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let out = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .args(args)
        .current_dir(dir)
        .stdout(writer)
        .output()
        .expect("ljos runs");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_report_into_a_closed_pipe_ends_quietly() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Main.qml"),
        "import QtQuick\nItem {   width: 10 }\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("tst_one.qml"),
        "import QtQuick\nimport QtTest\nTestCase {\n    name: \"One\"\n    function test_one() { verify(true) }\n}\n",
    )
    .unwrap();
    for args in [
        &["check", "."][..],
        &["fmt", "--check", "Main.qml"],
        &["shaders", "--check", "."],
        &["test", "tst_one.qml"],
    ] {
        let (status, stderr) = into_closed_pipe(dir.path(), args);
        assert!(!stderr.contains("panicked"), "{args:?}: {stderr}");
        assert_eq!(status, Some(BROKEN_PIPE), "{args:?}: {stderr}");
    }
}
