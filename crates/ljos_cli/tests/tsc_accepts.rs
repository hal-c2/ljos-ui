//! TypeScript accepts every desugared Ljós file the checker accepts
//! (docs/DIALECT.md §4.21, M7's exit in §13): the files of each scenario
//! under `features/` that have Ljós's syntax in them (`match`, `is`, `|>`,
//! or an `.lts` module) and that `ljos check` passes with no error pass
//! `ljos check --tsc` too, with nothing said by TypeScript.
//!
//! It needs TypeScript: the `tsc` that `LJOS_TSC` names, or `deno`. Without
//! either it says so and passes, as `ljos check --tsc` does.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use gherkin::{GherkinEnv, Scenario, Step};

fn features() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../features")
}

fn feature_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            feature_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "feature") {
            out.push(path);
        }
    }
}

/// The text of a step's doc string, without the label of its language.
fn docstring(step: &Step) -> Option<String> {
    let text = step.docstring.as_deref()?;
    Some(match text.split_once('\n') {
        Some((first, rest)) if matches!(first.trim(), "qml" | "js" | "ts" | "json" | "xml") => {
            rest.to_owned()
        }
        _ => text.to_owned(),
    })
}

/// The files a scenario gives: each `the file`, and `the QML` as
/// `Main.qml`.
fn given(background: &[&Step], scenario: &Scenario) -> Option<Vec<(String, String)>> {
    if !scenario.examples.is_empty()
        || scenario
            .tags
            .iter()
            .any(|t| matches!(t.as_str(), "backlog" | "qt-only" | "wip"))
    {
        return None;
    }
    let mut files = Vec::new();
    for step in background.iter().copied().chain(&scenario.steps) {
        let value = step.value.as_str();
        if let Some(name) = value
            .strip_prefix("the file \"")
            .and_then(|rest| rest.split_once('"'))
            .filter(|(_, rest)| matches!(*rest, ":" | " without its indentation:"))
            .map(|(name, _)| name)
        {
            files.push((name.to_owned(), docstring(step)?));
        } else if value == "the QML:" {
            files.push(("Main.qml".to_owned(), docstring(step)?));
        }
    }
    Some(files)
}

/// `ljos check`, with `args`, in `dir`: whether it passed, and what it said.
fn check(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .arg("check")
        .args(args)
        .arg(".")
        .current_dir(dir)
        .output()
        .expect("ljos runs");
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// Whether any of `files` has Ljós's syntax in it.
fn desugared(files: &[(String, String)]) -> bool {
    files.iter().any(|(name, text)| {
        name.ends_with(".lts")
            || ((name.ends_with(".qml") || name.ends_with(".ts") || name.ends_with(".js"))
                && qml_syntax::desugar(text).text != *text)
    })
}

fn has_typescript() -> bool {
    if std::env::var_os("LJOS_TSC").is_some_and(|v| !v.is_empty()) {
        return true;
    }
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join("deno").is_file()))
}

#[test]
fn typescript_accepts_every_desugared_file_the_checker_accepts() {
    if !has_typescript() {
        eprintln!("SKIPPED: no tsc (set LJOS_TSC, or put deno on the PATH)");
        return;
    }
    let mut paths = Vec::new();
    feature_files(&features(), &mut paths);
    paths.sort();
    let root = tempfile::tempdir().unwrap();
    let mut problems = Vec::new();
    let mut seen = 0;
    for path in &paths {
        let shown = path
            .strip_prefix(features())
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let feature = gherkin::Feature::parse_path(path, GherkinEnv::default())
            .unwrap_or_else(|e| panic!("{shown}: {e}"));
        let outer: Vec<&Step> = feature.background.iter().flat_map(|b| &b.steps).collect();
        let mut scenarios: Vec<(Vec<&Step>, &Scenario)> = feature
            .scenarios
            .iter()
            .map(|s| (outer.clone(), s))
            .collect();
        for rule in &feature.rules {
            let mut steps = outer.clone();
            steps.extend(rule.background.iter().flat_map(|b| &b.steps));
            scenarios.extend(rule.scenarios.iter().map(|s| (steps.clone(), s)));
        }
        for (background, scenario) in scenarios {
            let Some(files) = given(&background, scenario) else {
                continue;
            };
            if !desugared(&files) {
                continue;
            }
            let dir = root
                .path()
                .join(format!("{}-{}", seen, scenario.position.line));
            for (name, text) in &files {
                let file = dir.join(name);
                fs::create_dir_all(file.parent().unwrap()).unwrap();
                fs::write(file, text).unwrap();
            }
            // Only what the checker accepts: a scenario may show it
            // refusing a file.
            let (passed, said) = check(&dir, &[]);
            if !passed || said.contains(": error: ") {
                continue;
            }
            seen += 1;
            let (passed, text) = check(&dir, &["--tsc"]);
            if !passed || text.contains("[TS") {
                problems.push(format!(
                    "features/{shown}:{} ({}):\n{text}",
                    scenario.position.line, scenario.name
                ));
            }
        }
    }
    assert!(
        seen > 0,
        "no scenario gives a file with Ljós's syntax in it"
    );
    eprintln!("{seen} scenarios with Ljós's syntax, checked by TypeScript");
    assert!(
        problems.is_empty(),
        "TypeScript refuses {} of the {seen} scenarios the checker accepts:\n\n{}",
        problems.len(),
        problems.join("\n")
    );
}
