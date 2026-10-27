//! The declarative lint fixtures (`tests/fixtures/declarative/<code>/`)
//! through `qml_lint`. A bad file gives exactly the `decl-*` findings its
//! `// expect:` header names; a good one gives none. The good ones are
//! mostly what a rule must not mistake for its pattern, since a false hint
//! costs an author more than a missed one.

use std::path::{Path, PathBuf};

use ljos_design::fixture::{self, Expected};
use ljos_mcp_author::{DECLARATIVE, FrontEnd, QmlFrontEnd};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/declarative")
}

fn findings(file: &Path) -> Vec<Expected> {
    let front = QmlFrontEnd::new(file.parent().expect("a fixture is in a rule's directory"));
    let mut out: Vec<Expected> = front
        .lint(&[file.to_path_buf()])
        .expect("the analysis itself does not fail")
        .into_iter()
        .filter(|d| d.code.starts_with("decl-"))
        .map(|d| Expected {
            code: d.code,
            line: d.line,
        })
        .collect();
    out.sort();
    out
}

fn cases(code: &str, kind: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir().join(code))
        .unwrap_or_else(|_| panic!("no fixtures for {code}"))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(&format!(".{kind}.qml")))
        .collect();
    out.sort();
    out
}

#[test]
fn each_rule_finds_its_bad_fixtures_and_nothing_in_the_good_ones() {
    let mut failures = Vec::new();
    for lint in DECLARATIVE {
        let bad = cases(lint.code, "bad");
        let good = cases(lint.code, "good");
        assert!(
            !bad.is_empty() && !good.is_empty(),
            "{} needs a bad and a good fixture",
            lint.code
        );
        for file in bad {
            let source = std::fs::read_to_string(&file).unwrap();
            let mut expected = fixture::expected(&source).unwrap();
            expected.sort();
            let found = findings(&file);
            if found != expected {
                failures.push(format!(
                    "{}: expected {expected:?}, found {found:?}",
                    file.display()
                ));
            }
        }
        for file in good {
            let found = findings(&file);
            if !found.is_empty() {
                failures.push(format!(
                    "{}: expected nothing, found {found:?}",
                    file.display()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
