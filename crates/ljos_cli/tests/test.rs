//! `ljos test` from the outside: what it prints, what it exits with, and
//! the two suites of `ljos_test/tests/qml` that also run on Qt's runner.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs `ljos test` in `dir`: `grabImage(...).save(name)` writes beside
/// the current directory, and the places in the results are relative to
/// it.
fn ljos_test(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ljos"))
        .arg("test")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("ljos runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn evidence(part: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ljos_test/tests/qml")
        .join(part)
        .canonicalize()
        .expect("the evidence suite is in the repository")
}

fn totals(text: &str) -> &str {
    text.lines()
        .find(|line| line.starts_with("Totals: "))
        .and_then(|line| line.split(", 0 blacklisted").next())
        .unwrap_or_else(|| panic!("no totals in:\n{text}"))
}

const PASSING: &str = r#"import QtQuick
import QtTest

TestCase {
    name: "Sums"

    function test_add() {
        compare(1 + 1, 2)
    }

    function test_rows_data() {
        return [{ tag: "small", n: 1 }, { tag: "large", n: 100 }]
    }

    function test_rows(row) {
        verify(row.n > 0)
    }
}
"#;

const FAILING: &str = r#"import QtQuick
import QtTest

TestCase {
    name: "Broken"

    function test_a_wrong() {
        compare(1 + 1, 3)
    }

    function test_b_throws() {
        throw new Error("gave up")
    }

    function test_c_right() {
        verify(true)
    }
}
"#;

#[test]
fn a_passing_file_is_listed_and_exits_with_zero() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tst_sums.qml"), PASSING).unwrap();
    let out = ljos_test(dir.path(), &[]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}");
    for line in [
        "PASS   : Sums::initTestCase()",
        "PASS   : Sums::test_add()",
        "PASS   : Sums::test_rows(small)",
        "PASS   : Sums::test_rows(large)",
        "PASS   : Sums::cleanupTestCase()",
    ] {
        assert!(text.lines().any(|l| l == line), "no `{line}` in:\n{text}");
    }
    assert_eq!(totals(&text), "Totals: 5 passed, 0 failed, 0 skipped");
}

#[test]
fn a_failure_names_its_place_and_does_not_stop_the_next_test() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tst_broken.qml"), FAILING).unwrap();
    let out = ljos_test(dir.path(), &["tst_broken.qml"]);
    let text = stdout(&out);
    // The exit status is the number of failed tests, as with Qt's runner.
    assert_eq!(out.status.code(), Some(2), "{text}");
    assert!(
        text.contains(
            "FAIL!  : Broken::test_a_wrong() Compared values are not the same\n   \
             Actual   (): 2\n   Expected (): 3\n   Loc: ["
        ),
        "{text}"
    );
    assert!(text.contains("tst_broken.qml(8)]"), "{text}");
    assert!(
        text.contains("FAIL!  : Broken::test_b_throws() Uncaught exception: gave up"),
        "{text}"
    );
    assert!(text.contains("PASS   : Broken::test_c_right()"), "{text}");
    assert_eq!(totals(&text), "Totals: 3 passed, 2 failed, 0 skipped");
}

#[test]
fn a_filter_runs_only_what_it_names() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tst_sums.qml"), PASSING).unwrap();
    std::fs::write(dir.path().join("tst_broken.qml"), FAILING).unwrap();
    let out = ljos_test(dir.path(), &["--filter", "Sums::test_rows:large"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("PASS   : Sums::test_rows(large)"), "{text}");
    assert!(!text.contains("test_rows(small)"), "{text}");
    assert!(!text.contains("test_add"), "{text}");
    assert!(!text.contains("Broken::test_"), "{text}");
}

#[test]
fn the_results_are_also_written_as_json() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tst_broken.qml"), FAILING).unwrap();
    let out = ljos_test(dir.path(), &["--json", "results.json"]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    let json = std::fs::read_to_string(dir.path().join("results.json")).unwrap();
    let report: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(report["totals"]["passed"], 3);
    assert_eq!(report["totals"]["failed"], 2);
    assert_eq!(report["totals"]["skipped"], 0);
    let wrong = report["tests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["function"] == "test_a_wrong")
        .expect("the failed test is in the report");
    assert_eq!(wrong["case"], "Broken");
    assert_eq!(wrong["outcome"], "fail");
    let message = &wrong["messages"][0];
    assert_eq!(message["message"], "Compared values are not the same");
    assert_eq!(message["actual"], "2");
    assert_eq!(message["expected"], "3");
    assert_eq!(message["line"], 8);
    assert!(
        message["file"]
            .as_str()
            .unwrap()
            .ends_with("tst_broken.qml"),
        "{message}"
    );
}

#[test]
fn a_module_is_found_only_through_an_import_directory() {
    let dir = tempfile::tempdir().unwrap();
    let tests = dir.path().join("tests");
    let module = dir.path().join("imports/Shapes");
    std::fs::create_dir_all(&tests).unwrap();
    std::fs::create_dir_all(&module).unwrap();
    std::fs::write(
        module.join("qmldir"),
        "module Shapes\nSquare 1.0 Square.qml\n",
    )
    .unwrap();
    std::fs::write(
        module.join("Square.qml"),
        "import QtQuick\nItem {\n    property int side: 4\n    readonly property int area: side * side\n}\n",
    )
    .unwrap();
    std::fs::write(
        tests.join("tst_shapes.qml"),
        "import QtQuick\nimport QtTest\nimport Shapes\n\nTestCase {\n    name: \"Shapes\"\n\n    \
         Square {\n        id: square\n    }\n\n    function test_area() {\n        \
         compare(square.area, 16)\n    }\n}\n",
    )
    .unwrap();

    let without = ljos_test(dir.path(), &["tests"]);
    let text = stdout(&without);
    assert_eq!(without.status.code(), Some(1), "{text}");
    assert!(
        text.contains("FAIL!  : tst_shapes::compile() module \"Shapes\" is not installed"),
        "{text}"
    );
    assert!(text.contains("tst_shapes.qml(3)]"), "{text}");

    // Both spellings: clap's, and the one Qt's runner takes.
    for flag in ["--import", "-import"] {
        let with = ljos_test(dir.path(), &["tests", flag, "imports"]);
        let text = stdout(&with);
        assert_eq!(with.status.code(), Some(0), "{flag}: {text}");
        assert!(text.contains("PASS   : Shapes::test_area()"), "{text}");
    }
}

#[test]
fn nothing_to_run_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), "import QtQuick\nItem {}\n").unwrap();
    let out = ljos_test(dir.path(), &[]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no tst_*.qml file to run"), "{err}");
}

/// The suite that uses every function of the module passes; the same
/// files pass on Qt's runner with the same totals (93 passed, 1 skipped
/// on Qt 6.11.2).
#[test]
fn the_evidence_suite_passes() {
    let dir = tempfile::tempdir().unwrap();
    let suite = evidence("suite");
    let imports = evidence("imports");
    let out = ljos_test(
        dir.path(),
        &[
            suite.to_str().unwrap(),
            "-import",
            imports.to_str().unwrap(),
        ],
    );
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert_eq!(totals(&text), "Totals: 93 passed, 0 failed, 1 skipped");
    assert!(!text.contains("FAIL!"), "{text}");
    // `grabImage(...).save(name)` writes into the current directory.
    assert!(dir.path().join("grabbed-by-the-suite.png").is_file());
}

/// The deliberately failing file fails the same way as on Qt's runner:
/// the same lines, in the same order (compared by hand with Qt 6.11.2;
/// the pinned ones here are a sample of each kind).
#[test]
fn the_failing_suite_fails_as_it_does_on_qt() {
    let dir = tempfile::tempdir().unwrap();
    let failing = evidence("failing");
    let out = ljos_test(dir.path(), &[failing.to_str().unwrap()]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(17), "{text}");
    assert_eq!(totals(&text), "Totals: 9 passed, 17 failed, 1 skipped");
    for part in [
        "FAIL!  : FailingStart::initTestCase() nothing can run",
        "FAIL!  : Failures::test_compare() Compared values are not the same\n   Actual   (): 2\n   Expected (): 3",
        "FAIL!  : Failures::test_fuzzyCompare() Compared values are not the same with delta(0.25)",
        "FAIL!  : Failures::test_rows(wrong) Compared values are not the same",
        "SKIP   : Failures::test_rows(skipped) no third row yet",
        "FAIL!  : Failures::test_spy_wait() 'wait for signal ping' returned FALSE. ()",
        "FAIL!  : Failures::test_throw() Uncaught exception: thrown by the test",
        "FAIL!  : Failures::test_tryCompare() property n",
        "XPASS  : Failures::test_unexpected_pass() QCOMPARE(, ) returned TRUE unexpectedly.",
        "FAIL!  : Failures::test_verify_message() 'n is too small' returned FALSE. ()",
        "PASS   : Failures::test_passes()",
    ] {
        assert!(text.contains(part), "no `{part}` in:\n{text}");
    }
}
