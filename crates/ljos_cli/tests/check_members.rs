//! `ljos check` and the modules that are Ljós's own (`QtTest`, `Ljos`):
//! their types are known, and a name one of them does not have is reported
//! as it is for a type of Qt's.
//!
//! The types come from the one table the compiler reads; that it says what
//! the native types declare is `own_types.rs`'s to hold.

use std::{path::Path, process::Command};

use qml_front::{QtTypeOracle, TypeOracle};

fn write(dir: &Path, name: &str, source: &str) {
    std::fs::write(dir.join(name), source).unwrap();
}

/// What `ljos check` says of a project of one file: whether it passed, the
/// lines of its diagnostics, and its count. `None` where there is no table
/// of Qt's types, without which no type is known at all.
fn check(name: &str, source: &str, more: &[&str]) -> Option<(bool, Vec<String>, String)> {
    if !QtTypeOracle.has_module("QtQuick") {
        eprintln!("skipped: there is no table of Qt's types (`mise run types`)");
        return None;
    }
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), name, source);
    let mut args = vec!["check"];
    args.extend(more);
    args.push(dir.path().to_str().unwrap());
    let out = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .args(&args)
        .output()
        .expect("ljos runs");
    let lines = String::from_utf8_lossy(&out.stderr)
        .lines()
        .map(str::to_string)
        .collect();
    let count = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Some((out.status.success(), lines, count))
}

const TEST: &str = r#"import QtQuick
import QtTest
TestCase {
    name: "Poke"
    when: windowShown
    Item { id: box; signal poked }
    SignalSpy { id: spy; target: box; signalName: "poked" }
    function test_poke() {
        box.poked()
        compare(spy.count, 1)
        verify(spy.valid)
        spy.clear()
    }
}
"#;

const BOUNDARY: &str = r#"import QtQuick
import Ljos
Item {
    ErrorBoundary {
        id: boundary
        maxRestarts: 2
        within: 1000
        catchExceptions: true
        onFailedChanged: console.log(failed, fault.kind, fault.message, fault.line, restarts)
        fallback: Rectangle {
            Timer { interval: 500; running: true; onTriggered: boundary.retry() }
        }
        Rectangle { }
    }
}
"#;

#[test]
fn a_test_case_and_a_signal_spy_are_types_with_their_members() {
    let Some((passed, lines, count)) = check("tst_poke.qml", TEST, &[]) else {
        return;
    };
    assert!(passed, "{lines:?}");
    assert_eq!(lines, [] as [&str; 0]);
    assert_eq!(count, "1 files, 0 errors, 0 warnings");
}

#[test]
fn an_error_boundary_is_a_type_with_its_members() {
    let Some((passed, lines, count)) = check("Main.qml", BOUNDARY, &[]) else {
        return;
    };
    assert!(passed, "{lines:?}");
    assert_eq!(lines, [] as [&str; 0]);
    assert_eq!(count, "1 files, 0 errors, 0 warnings");
}

#[test]
fn a_misspelt_member_of_a_type_of_qttest_is_a_warning() {
    let source = TEST
        .replace("when: windowShown", "wen: windowShown")
        .replace("signalName:", "signalNam:");
    let Some((passed, lines, count)) = check("tst_poke.qml", &source, &[]) else {
        return;
    };
    assert!(
        passed,
        "a name that is no member is not an error: {lines:?}"
    );
    assert_eq!(
        lines,
        [
            "tst_poke.qml:5:5: warning: `wen` is not a property of `TestCase`; did you mean \
             `when`? [unknown-property]",
            "tst_poke.qml:7:39: warning: `signalNam` is not a property of `SignalSpy`; did you \
             mean `signalName`? [unknown-property]",
        ]
    );
    assert_eq!(count, "1 files, 0 errors, 2 warnings");
}

#[test]
fn a_misspelt_member_of_a_type_of_ljos_is_a_warning() {
    let source = BOUNDARY
        .replace("maxRestarts: 2", "maxRestart: 2")
        .replace("onFailedChanged:", "onFailChanged:");
    let Some((passed, lines, count)) = check("Main.qml", &source, &[]) else {
        return;
    };
    assert!(
        passed,
        "a name that is no member is not an error: {lines:?}"
    );
    assert_eq!(
        lines,
        [
            "Main.qml:6:9: warning: `maxRestart` is not a property of `ErrorBoundary`; did you \
             mean `maxRestarts`? [unknown-property]",
            "Main.qml:9:9: warning: `onFailChanged` handles the signal `failChanged`, which \
             `ErrorBoundary` does not have; did you mean `onFailedChanged`? [unknown-signal]",
        ]
    );
    assert_eq!(count, "1 files, 0 errors, 2 warnings");
}

/// A name that is no member is found from the types, not by a lint: it is
/// what the file will fail on, so leaving the lints out keeps it.
#[test]
fn no_lint_keeps_what_is_no_member() {
    let source = BOUNDARY.replace("maxRestarts: 2", "maxRestart: 2");
    let Some((_, lines, count)) = check("Main.qml", &source, &["--no-lint"]) else {
        return;
    };
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].ends_with("[unknown-property]"), "{lines:?}");
    assert_eq!(count, "1 files, 0 errors, 1 warnings");
}

/// A module's types are those of a file that imports it and of no other.
#[test]
fn an_own_type_is_no_type_of_a_file_that_does_not_import_its_module() {
    for (name, source) in [
        ("tst_poke.qml", TEST.replace("import QtTest\n", "")),
        ("Main.qml", BOUNDARY.replace("import Ljos\n", "")),
    ] {
        let Some((passed, lines, _)) = check(name, &source, &[]) else {
            return;
        };
        assert!(!passed, "{lines:?}");
        assert!(
            lines
                .iter()
                .any(|line| line.contains("is not a type of anything the file imports")),
            "{lines:?}"
        );
    }
}
