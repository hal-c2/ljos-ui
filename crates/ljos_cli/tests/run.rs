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
fn run_offers_the_inspector_and_says_what_each_capability_is_for() {
    let out = ljos(&["run", "--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("--mcp[=<CAPS>]"), "{help}");
    assert!(help.contains("--mcp=inspect,input,mutate,eval"), "{help}");
    assert!(help.contains("never implied"), "{help}");
    assert!(help.contains("--mcp-bind"), "{help}");
    assert!(help.contains("--mcp-socket"), "{help}");
    assert!(help.contains("loopback"), "{help}");
    assert!(help.contains("--mcp-redact"), "{help}");
    assert!(help.contains("--top"), "{help}");
}

#[test]
fn a_capability_for_the_authoring_server_is_refused_before_a_window_opens() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("Main.qml");
    std::fs::write(&main, "import QtQuick\nItem {}\n").unwrap();
    let out = ljos(&["run", main.to_str().unwrap(), "--mcp=inspect,fs"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("authoring server"), "{err}");
    assert!(err.contains("fs"), "{err}");
}

#[test]
fn a_socket_path_that_is_too_long_is_refused_before_a_window_opens() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("Main.qml");
    std::fs::write(&main, "import QtQuick\nItem {}\n").unwrap();
    let long = format!("/tmp/{}", "s".repeat(200));
    let out = ljos(&[
        "run",
        main.to_str().unwrap(),
        "--mcp",
        "--mcp-socket",
        &long,
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains(&long), "{err}");
    assert!(err.contains("205"), "{err}");
    assert!(err.contains("at most"), "{err}");
    assert!(err.contains("--mcp-socket"), "{err}");
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
fn an_entry_with_a_mistake_is_refused_without_watch() {
    // With `--watch` it opens and shows the mistake; without, nobody
    // would mend it under the window. A mistake in another file stops
    // nothing until an object of that file is made.
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("Main.qml");
    std::fs::write(&main, "import QtQuick\nItem {\n  width: 20 +\n}\n").unwrap();
    let out = ljos(&["run", main.to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Main.qml:4:"), "{err}");
}

#[test]
fn an_alias_to_a_directory_without_the_module_is_refused_naming_both() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("Main.qml");
    std::fs::write(&main, "import QtQuick\nItem {}\n").unwrap();
    let other = dir.path().join("Other");
    std::fs::create_dir(&other).unwrap();
    std::fs::write(other.join("qmldir"), "module App.Other\n").unwrap();
    let alias = format!("App.Media={}", other.display());
    let out = ljos(&["run", main.to_str().unwrap(), "--watch", "--alias", &alias]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("App.Media"), "{err}");
    assert!(err.contains(&other.display().to_string()), "{err}");
    assert!(err.contains("App.Other"), "{err}");
}

#[test]
fn run_offers_aliases() {
    let out = ljos(&["run", "--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("--alias <MODULE=DIR>"), "{help}");
}

#[test]
fn a_theme_beside_the_entry_that_cannot_be_read_is_refused_at_its_line() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("Main.qml");
    std::fs::write(&main, "import QtQuick\nItem {}\n").unwrap();
    std::fs::write(
        dir.path().join("theme.json"),
        "{\n  \"Colors\": {\n    \"acent\": \"#9c4dc4\"\n  }\n}\n",
    )
    .unwrap();
    let out = ljos(&["run", main.to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("theme.json:3:"), "{err}");
    assert!(err.contains("Colors has no role \"acent\""), "{err}");
}

#[test]
fn a_named_theme_is_read_in_place_of_the_one_beside() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("Main.qml");
    std::fs::write(&main, "import QtQuick\nItem {}\n").unwrap();
    let named = dir.path().join("dim.json");
    std::fs::write(&named, "{ \"Spacing\": { \"md\": \"wide\" } }\n").unwrap();
    let out = ljos(&[
        "run",
        main.to_str().unwrap(),
        "--theme",
        named.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("dim.json:1:"), "{err}");
    assert!(err.contains("Spacing.md"), "{err}");
    // A named theme that is not there is a mistake, not the module's own.
    let gone = dir.path().join("gone.json");
    let out = ljos(&[
        "run",
        main.to_str().unwrap(),
        "--theme",
        gone.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("gone.json"), "{err}");
}

#[test]
fn run_offers_a_theme() {
    let out = ljos(&["run", "--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("--theme <FILE>"), "{help}");
}
