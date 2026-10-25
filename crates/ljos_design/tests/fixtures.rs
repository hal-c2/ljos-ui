//! The fixtures are laid out as `fixtures/README.md` says: a directory per
//! rule code, each bad file saying what it must produce, each with a good
//! twin. Whether the checks produce it is for the lint and audit tests.

use ljos_design::{fixture, rule};

#[test]
fn every_fixture_directory_is_a_rule_and_every_bad_file_has_a_header_and_a_twin() {
    let dir = fixture::dir();
    let mut problems = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("the fixtures directory exists") {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.path().is_dir() {
            if name != "README.md" {
                problems.push(format!(
                    "{name}: only rule directories and README.md belong here"
                ));
            }
            continue;
        }
        if rule(&name).is_none() {
            problems.push(format!("{name}/: not a code in ljos_design::RULES"));
        }
        for file in std::fs::read_dir(entry.path()).unwrap() {
            let file = file.unwrap().file_name().to_string_lossy().into_owned();
            if !(file.ends_with(".bad.qml") || file.ends_with(".good.qml")) {
                problems.push(format!("{name}/{file}: neither .bad.qml nor .good.qml"));
            }
            if let Some(case) = file.strip_suffix(".good.qml")
                && !entry.path().join(format!("{case}.bad.qml")).exists()
            {
                problems.push(format!("{name}/{file}: no bad twin"));
            }
        }
    }
    for case in fixture::cases() {
        let at = format!("{}/{}", case.code, case.name);
        if !case.good.exists() {
            problems.push(format!("{at}.bad.qml: no good twin"));
        }
        let source = std::fs::read_to_string(&case.bad).unwrap();
        let lines = source.lines().count() as u32;
        match fixture::expected(&source) {
            Err(why) => problems.push(format!("{at}.bad.qml: {why}")),
            Ok(expected) => {
                if !expected.iter().any(|e| e.code == case.code) {
                    problems.push(format!("{at}.bad.qml: expects nothing of {}", case.code));
                }
                for e in &expected {
                    if rule(&e.code).is_none() {
                        problems.push(format!("{at}.bad.qml: `{}` is no rule", e.code));
                    }
                    if e.line == 0 || e.line > lines {
                        problems.push(format!("{at}.bad.qml: line {} is not in it", e.line));
                    }
                }
            }
        }
        for path in [&case.bad, &case.good] {
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            if let Err(why) = fixture::audit(&source) {
                problems.push(format!("{}: {why}", path.display()));
            }
            for line in source
                .lines()
                .filter(|l| l.trim_start().starts_with("import "))
            {
                let module = line.split_whitespace().nth(1).unwrap_or_default();
                if !["QtQuick", "QtQuick.Controls", "QtQuick.Layouts"].contains(&module) {
                    problems.push(format!("{}: imports {module}", path.display()));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
