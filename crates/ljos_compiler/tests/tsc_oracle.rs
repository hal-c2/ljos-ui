//! TypeScript as the checker's oracle (docs/DIALECT.md M4): each file of
//! `tests/tsc_oracle/` says in its first line what both should make of it,
//! and this holds TypeScript and the checker to it.
//!
//! `// expect: accept`: neither reports anything.
//! `// expect: reject-type <line> <TScode>`: TypeScript reports `<TScode>`
//! at that line of the QML file, the error the case is about, and the
//! checker reports an error.
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
        "C19HoverReject",
        "a member an id does not have is not reported in a binding",
    ),
    (
        "N06CallbackReject",
        "a call of what may be undefined is not reported",
    ),
    ("N10DayReject", "ts-index-undefined is not built"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expect {
    Accept,
    RejectType { line: u32, code: String },
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
                Some(("reject-type", at)) => {
                    let (line, code) = at.split_once(' ').unwrap_or_else(|| {
                        panic!("{key}.qml: `reject-type` takes `<line> <TScode>`")
                    });
                    Expect::RejectType {
                        line: line
                            .parse()
                            .unwrap_or_else(|_| panic!("{key}.qml: `{line}` is not a line number")),
                        code: code.to_owned(),
                    }
                }
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
        let of: Vec<&ts::Finding> = findings.iter().filter(|f| f.key == case.key).collect();
        let said = || {
            of.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n  ")
        };
        match &case.expect {
            // Any error would do for "rejected"; the case is about one, so
            // TypeScript must find that one, where the QML file has it.
            Expect::RejectType { line, code } => {
                if !of.iter().any(|f| f.line == *line && f.code == *code) {
                    wrong.push(format!(
                        "{}: expected {code} at line {line}, TypeScript says\n  {}",
                        case.key,
                        said()
                    ));
                }
            }
            Expect::Accept | Expect::RejectSubset(_) => {
                if !of.is_empty() {
                    wrong.push(format!("{}: TypeScript says\n  {}", case.key, said()));
                }
            }
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
            Expect::RejectType { .. } => !errors.is_empty(),
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
