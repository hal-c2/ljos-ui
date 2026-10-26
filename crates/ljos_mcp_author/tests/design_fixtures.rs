//! The design rule fixtures (`crates/ljos_design/fixtures/`) through
//! `qml_lint`. For a rule the lints implement (`IMPLEMENTED`), each bad
//! file gives every finding its header names, and of its directory's code
//! exactly those; its good twin gives none of that code. A file may show
//! other rules on the way (a fixed surface is also a colour written out),
//! which their own fixtures hold to account.
//!
//! A rule with no static check yet has its fixtures listed as pending, not
//! failed: they may be written ahead of the check. So is a case with an
//! `// audit:` line: it names the scheme or size where the defect shows,
//! which only the running interface has (a translucent tint over a surface
//! the source does not know).

use std::path::Path;

use ljos_design::{
    Check,
    fixture::{self, Expected},
};
use ljos_mcp_author::{FrontEnd, IMPLEMENTED, QmlFrontEnd};

/// The implemented design findings of a file, as `<code> at <line>`.
fn findings(file: &Path) -> Vec<Expected> {
    let front = QmlFrontEnd::new(file.parent().expect("a fixture is in a rule's directory"));
    let mut out: Vec<Expected> = front
        .lint(&[file.to_path_buf()])
        .expect("the analysis itself does not fail")
        .into_iter()
        .filter(|d| IMPLEMENTED.contains(&d.code.as_str()))
        .map(|d| Expected {
            code: d.code,
            line: d.line,
        })
        .collect();
    out.sort();
    out
}

fn brief(list: &[Expected]) -> String {
    let items: Vec<String> = list
        .iter()
        .map(|e| format!("{} at {}", e.code, e.line))
        .collect();
    format!("[{}]", items.join(", "))
}

#[test]
fn every_implemented_rule_finds_its_bad_fixtures_and_not_its_good_ones() {
    let mut failures = Vec::new();
    let mut pending = Vec::new();
    let mut ran = 0;
    for case in fixture::cases() {
        let rule = ljos_design::rule(&case.code).expect("the structure test checks codes");
        if rule.check == Check::Runtime {
            continue;
        }
        if !IMPLEMENTED.contains(&case.code.as_str()) {
            pending.push(format!("{}/{}", case.code, case.name));
            continue;
        }
        let source = std::fs::read_to_string(&case.bad).unwrap();
        if !fixture::audit(&source)
            .expect("the structure test checks headers")
            .is_empty()
        {
            pending.push(format!("{}/{} (audit)", case.code, case.name));
            continue;
        }
        ran += 1;
        let expected: Vec<Expected> = fixture::expected(&source)
            .expect("the structure test checks headers")
            .into_iter()
            .filter(|e| IMPLEMENTED.contains(&e.code.as_str()))
            .collect();
        let found = findings(&case.bad);
        let mine = |list: &[Expected]| -> Vec<Expected> {
            list.iter()
                .filter(|e| e.code == case.code)
                .cloned()
                .collect()
        };
        let missed = expected.iter().any(|e| !found.contains(e));
        if missed || mine(&found) != mine(&expected) {
            failures.push(format!(
                "{}/{}.bad.qml: expected {}, found {}",
                case.code,
                case.name,
                brief(&expected),
                brief(&found)
            ));
        }
        let found = mine(&findings(&case.good));
        if !found.is_empty() {
            failures.push(format!(
                "{}/{}.good.qml: expected nothing, found {}",
                case.code,
                case.name,
                brief(&found)
            ));
        }
    }
    if !pending.is_empty() {
        eprintln!(
            "{} fixture cases are pending (no static check yet): {}",
            pending.len(),
            pending.join(", ")
        );
    }
    assert!(ran > 0, "no implemented rule has a fixture");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_implemented_static_rule_has_a_fixture() {
    let cases = fixture::cases();
    let missing: Vec<&&str> = IMPLEMENTED
        .iter()
        .filter(|code| !cases.iter().any(|c| c.code == **code))
        .collect();
    assert!(missing.is_empty(), "no fixture for {missing:?}");
}
