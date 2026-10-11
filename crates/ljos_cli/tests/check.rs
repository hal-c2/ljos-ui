//! `ljos check`: what it says about a project, and with `--stats` how much
//! of it runs in the binding VM (ARCHITECTURE.md §5.4).

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

const MAIN: &str = r#"import QtQuick
Item {
    id: root
    property real a: 1
    property real b: a * 2 + 1
    property string label: a > 1 ? "many" : "one"
    property var parts: label.split("n")
    function log() { console.log(a) }
    onAChanged: b = a + 1
    onBChanged: console.log(b)
    Card { width: root.b }
}
"#;

const CARD: &str = r#"import QtQuick
Rectangle {
    property real margin: 4
    height: width / 2 + margin
}
"#;

#[test]
fn stats_gives_coverage_per_file_and_in_total_with_the_reasons() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Main.qml", MAIN);
    write(dir.path(), "Card.qml", CARD);
    let out = ljos(&["check", "--stats", dir.path().to_str().unwrap()]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    let text = String::from_utf8_lossy(&out.stdout);
    let row = |name: &str| -> Vec<String> {
        text.lines()
            .find(|line| line.split_whitespace().next() == Some(name))
            .unwrap_or_else(|| panic!("no row for {name} in\n{text}"))
            .split_whitespace()
            .skip(1)
            .map(str::to_string)
            .collect()
    };
    // Bindings, handlers, functions, all, and the share of all.
    assert_eq!(row("Card"), ["1/1", "0/0", "0/0", "1/1", "100.0%"]);
    assert_eq!(row("Main"), ["3/4", "1/2", "0/1", "4/7", "57.1%"]);
    assert_eq!(row("total"), ["4/5", "1/2", "0/1", "5/8", "62.5%"]);
    assert!(text.contains("all        5 of 8 tier 1 (62.5%)"), "{text}");
    // The most common reason first, with the kinds of expression it is the
    // reason for: all, bindings, handlers, functions.
    let reasons: Vec<Vec<&str>> = text
        .lines()
        .skip_while(|line| *line != "tier 2, by construct")
        .skip(2)
        .take_while(|line| !line.is_empty())
        .map(|line| line.split_whitespace().collect())
        .collect();
    assert_eq!(
        reasons,
        [
            vec!["2", "0", "1", "1", "global", "function", "call"],
            vec!["1", "1", "0", "0", "method", "call"]
        ]
    );
    assert!(
        text.lines()
            .any(|line| line.ends_with("  global function call: console.log")),
        "{text}"
    );
}

#[test]
fn summary_leaves_the_files_out_and_top_bounds_the_reasons() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Main.qml", MAIN);
    write(dir.path(), "Card.qml", CARD);
    let dir = dir.path().to_str().unwrap();
    let out = ljos(&["check", "--stats", "--summary", "--top", "1", dir]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        !text.lines().any(|line| line.starts_with("Main ")),
        "{text}"
    );
    assert!(text.contains("global function call"), "{text}");
    assert!(!text.contains("method call"), "{text}");
}

#[test]
fn a_tree_of_projects_is_checked_project_by_project() {
    let dir = tempfile::tempdir().unwrap();
    // The same file name in two projects: neither sees the other's.
    write(dir.path(), "demos/one/Main.qml", CARD);
    write(dir.path(), "demos/two/Main.qml", MAIN);
    write(dir.path(), "demos/two/Card.qml", CARD);
    let out = ljos(&["check", "--stats", "--tree", dir.path().to_str().unwrap()]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    let text = String::from_utf8_lossy(&out.stdout);
    for name in ["demos/one/Main", "demos/two/Main", "demos/two/Card"] {
        assert!(
            text.lines().any(|line| line.starts_with(name)),
            "no {name} in\n{text}"
        );
    }
    assert!(text.contains("files      3"), "{text}");
}

#[test]
fn without_stats_it_counts_and_a_mistake_fails_it() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Main.qml", CARD);
    let out = ljos(&["check", dir.path().to_str().unwrap()]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "1 files, 0 errors, 0 warnings"
    );

    write(
        dir.path(),
        "Broken.qml",
        "import QtQuick\nItem { width: }\n",
    );
    let out = ljos(&["check", "--stats", dir.path().to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Broken.qml:"), "{err}");
    // The numbers are still given: a file with a mistake compiles in part.
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("files      2"), "{text}");
    assert!(text.contains("all        1 of 1 tier 1"), "{text}");
}
