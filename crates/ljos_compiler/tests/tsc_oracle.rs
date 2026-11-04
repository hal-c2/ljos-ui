//! TypeScript as the checker's oracle (docs/DIALECT.md M4): each file of
//! `tests/tsc_oracle/` says in its first line what both should make of it,
//! and this holds TypeScript and the checker to it.
//!
//! `// expect: accept`: neither reports anything.
//! `// expect: reject-type`: both report an error.
//! `// expect: reject-subset <code>`: TypeScript accepts it, and the checker
//! reports it with `<code>`, a rule of the subset TypeScript does not have.

use std::path::{Path, PathBuf};

use ljos_compiler::{Severity, compile_project, ts};

/// Cases today's checker does not agree with, each because a rule of
/// docs/DIALECT.md §5.1 is not built yet or does not reach that far. The
/// test fails when one of them starts agreeing, so the list shrinks as the
/// checker grows.
const KNOWN_GAPS: &[(&str, &str)] = &[
    (
        "C01ToggleReject",
        "`!` on a number is not reported (ts-condition-boolean covers `!` in §5.1)",
    ),
    (
        "C05ConnectionsReject",
        "a Connections handler's parameter is not typed",
    ),
    (
        "C11FormatReject",
        "a binding's value is not checked against its property",
    ),
    (
        "C12ClampReject",
        "a call's arguments are not checked against the function's parameters",
    ),
    (
        "C17DigitsReject",
        "a returned value is not checked against the function's return type",
    ),
    (
        "C19HoverReject",
        "a member an id does not have is not reported in a binding",
    ),
    ("N02ToggleReject", "ts-no-any is not built"),
    ("N03StatusReject", "ts-switch-exhaustive is not built"),
    ("N05RecordReject", "ts-optional-exact is not built"),
    (
        "N06CallbackReject",
        "a call of what may be undefined is not reported",
    ),
    ("N07AnswerReject", "ts-index-undefined is not built"),
    (
        "N08KeysReject",
        "a local `const` is not typed, so its truthiness is let by",
    ),
    ("N10DayReject", "ts-index-undefined is not built"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expect {
    Accept,
    RejectType,
    RejectSubset(String),
}

struct Case {
    key: String,
    expect: Expect,
}

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tsc_oracle")
}

fn cases() -> Vec<Case> {
    let mut cases: Vec<Case> = std::fs::read_dir(dir())
        .expect("the cases")
        .map(|entry| entry.expect("a case").path())
        .filter(|path| path.extension().is_some_and(|e| e == "qml"))
        .map(|path| {
            let key = path
                .file_stem()
                .expect("a name")
                .to_string_lossy()
                .into_owned();
            let text = std::fs::read_to_string(&path).expect("readable");
            let header = text.lines().next().unwrap_or_default();
            let said = header
                .strip_prefix("// expect: ")
                .unwrap_or_else(|| panic!("{key}.qml: the first line is not `// expect: …`"));
            let expect = match said.split_once(' ') {
                None if said == "accept" => Expect::Accept,
                None if said == "reject-type" => Expect::RejectType,
                Some(("reject-subset", code)) => Expect::RejectSubset(code.to_owned()),
                _ => panic!("{key}.qml: `{said}` is not an expectation"),
            };
            Case { key, expect }
        })
        .collect();
    cases.sort_by(|a, b| a.key.cmp(&b.key));
    cases
}

#[test]
fn the_cases_cover_note_11_and_the_corpus() {
    let cases = cases();
    for n in 1..=10 {
        let note = format!("N{n:02}");
        let of: Vec<&Expect> = cases
            .iter()
            .filter(|c| c.key.starts_with(&note))
            .map(|c| &c.expect)
            .collect();
        assert!(
            of.contains(&&Expect::Accept) && of.iter().any(|e| **e != Expect::Accept),
            "note 11 §3.{n} needs an accepted case and a rejected one"
        );
    }
    let shapes: std::collections::BTreeSet<&str> = cases
        .iter()
        .filter(|c| c.key.starts_with('C'))
        .map(|c| &c.key[..3])
        .collect();
    assert!(
        shapes.len() >= 15,
        "fifteen handler shapes from the corpus, not {}",
        shapes.len()
    );
    for gap in KNOWN_GAPS {
        assert!(
            cases.iter().any(|c| c.key == gap.0),
            "KNOWN_GAPS names {}, which is not a case",
            gap.0
        );
    }
}

#[test]
fn typescript_agrees_with_each_expectation() {
    let outcome = ts::check_dir(&dir(), None).expect("TypeScript ran");
    let ts::Outcome::Checked { files, findings } = outcome else {
        println!("SKIPPED: no tsc");
        return;
    };
    let cases = cases();
    let mut wrong = Vec::new();
    for case in &cases {
        if !files.contains(&case.key) {
            wrong.push(format!("{}: not given to TypeScript", case.key));
            continue;
        }
        let said: Vec<String> = findings
            .iter()
            .filter(|f| f.key == case.key)
            .map(ToString::to_string)
            .collect();
        match (&case.expect, said.is_empty()) {
            (Expect::RejectType, true) => {
                wrong.push(format!("{}: TypeScript accepts it", case.key));
            }
            (Expect::Accept | Expect::RejectSubset(_), false) => {
                wrong.push(format!(
                    "{}: TypeScript says\n  {}",
                    case.key,
                    said.join("\n  ")
                ));
            }
            _ => {}
        }
    }
    assert!(
        wrong.is_empty(),
        "TypeScript disagrees:\n{}",
        wrong.join("\n")
    );
    println!("{} cases, TypeScript agrees with each", cases.len());
}

#[test]
fn the_checker_agrees_or_the_gap_is_known() {
    let results = compile_project(&dir()).expect("the cases compile");
    let cases = cases();
    let mut unknown = Vec::new();
    let mut closed = Vec::new();
    for case in &cases {
        let file = results
            .iter()
            .find(|r| r.key == case.key)
            .unwrap_or_else(|| panic!("{} was not compiled", case.key));
        let diagnostics = match &file.result {
            Ok(compiled) => &compiled.diagnostics,
            Err(diagnostics) => diagnostics,
        };
        let errors: Vec<String> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| format!("{} [{}]", d.message, d.code.as_str()))
            .collect();
        let agrees = match &case.expect {
            Expect::Accept => errors.is_empty(),
            Expect::RejectType => !errors.is_empty(),
            Expect::RejectSubset(code) => errors.iter().any(|e| e.ends_with(&format!("[{code}]"))),
        };
        let known = KNOWN_GAPS.iter().any(|gap| gap.0 == case.key);
        match (agrees, known) {
            (false, false) => unknown.push(format!(
                "{}: expected {:?}, the checker says [{}]",
                case.key,
                case.expect,
                errors.join("; ")
            )),
            (true, true) => closed.push(case.key.clone()),
            _ => {}
        }
    }
    assert!(
        unknown.is_empty() && closed.is_empty(),
        "the checker disagrees where no gap is known:\n{}\nthese now agree; take them out of \
         KNOWN_GAPS:\n{}",
        unknown.join("\n"),
        closed.join("\n")
    );
    println!(
        "{} cases, the checker agrees with {} and {} gaps are known",
        cases.len(),
        cases.len() - KNOWN_GAPS.len(),
        KNOWN_GAPS.len()
    );
}
