//! `ljos run` as far as it goes without a window: what it takes and what
//! it refuses before it opens one.

use std::process::Command;

fn ljos(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ljos"))
        .args(args)
        .output()
        .expect("ljos runs")
}

#[test]
fn run_offers_to_watch() {
    let out = ljos(&["run", "--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("--watch"), "{help}");
    assert!(help.contains("--size"), "{help}");
}

#[test]
fn a_file_that_is_not_qml_is_refused_before_a_window_opens() {
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "").unwrap();
    for watch in [&[][..], &["--watch"][..]] {
        let mut args = vec!["run"];
        args.extend_from_slice(watch);
        args.push(notes.to_str().unwrap());
        let out = ljos(&args);
        assert!(!out.status.success());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("is not a QML file"), "{err}");
    }
}

#[test]
fn a_project_with_a_mistake_is_refused_without_watch() {
    // With `--watch` it opens and shows the mistake; without, nobody
    // would mend it under the window.
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("Main.qml");
    std::fs::write(&main, "import QtQuick\nItem {\n  width: 20 +\n}\n").unwrap();
    let out = ljos(&["run", main.to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("does not compile"), "{err}");
    assert!(err.contains("Main.qml: "), "{err}");
}
