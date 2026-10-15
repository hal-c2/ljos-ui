//! `features:lint`: checks on the ledger that cucumber itself does not make.
//!
//! The step patterns are read from `steps.rs` so that there is one list of
//! phrasings, the one the macros register.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use cucumber::feature::Ext as _;
use regex::Regex;

/// Words the writing rules forbid in scenarios (they name the implementation).
const FORBIDDEN: &[&str] = &["egui", "kittest", "widget", "draw call", "crate"];
static QT_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Qt 6\.\d+").expect("static regex"));
const MAX_QML_LINES: usize = 25;

#[derive(Debug, Default)]
pub struct Report {
    pub errors: Vec<String>,
    pub files: usize,
    pub scenarios: usize,
    pub verified: usize,
    pub distinct_steps: usize,
    /// Steps without a definition, in backlog scenarios only.
    pub planned_steps: usize,
}

impl Report {
    #[must_use]
    pub fn steps_per_100_scenarios(&self) -> f64 {
        if self.scenarios == 0 {
            0.0
        } else {
            self.distinct_steps as f64 * 100.0 / self.scenarios as f64
        }
    }
}

/// Step patterns registered by the macros in `steps_source`.
///
/// # Panics
/// When a registered pattern is not a valid regex (that would also fail at run time).
#[must_use]
pub fn step_patterns(steps_source: &str) -> Vec<Regex> {
    // rustfmt puts a long pattern on a line of its own.
    let raw = Regex::new(r##"#\[(?:given|when|then)\(\s*regex = r#"(.+?)"#,?\s*\)\]"##)
        .expect("static regex");
    let bare = Regex::new(r#"#\[(?:given|when|then)\(\s*regex = r"(.+?)",?\s*\)\]"#)
        .expect("static regex");
    let plain = Regex::new(r#"#\[(?:given|when|then)\("(.+?)"\)\]"#).expect("static regex");
    let mut out: Vec<Regex> = raw
        .captures_iter(steps_source)
        .chain(bare.captures_iter(steps_source))
        .map(|c| Regex::new(&c[1]).unwrap_or_else(|e| panic!("bad step pattern {:?}: {e}", &c[1])))
        .collect();
    out.extend(
        plain
            .captures_iter(steps_source)
            .map(|c| Regex::new(&format!("^{}$", regex::escape(&c[1]))).expect("escaped")),
    );
    out
}

fn feature_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            feature_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "feature") {
            out.push(p);
        }
    }
}

fn shape(step: &str) -> String {
    let q = Regex::new(r#""[^"]*""#).expect("static regex");
    let n = Regex::new(r"-?\d+(\.\d+)?").expect("static regex");
    n.replace_all(&q.replace_all(step, "\"…\""), "N")
        .into_owned()
}

/// Lint every `.feature` under `features_dir`.
#[must_use]
pub fn lint(features_dir: &Path, steps_source: &str) -> Report {
    let patterns = step_patterns(steps_source);
    let mut report = Report::default();
    let mut files = Vec::new();
    feature_files(features_dir, &mut files);
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();

    for path in files {
        let rel = path
            .strip_prefix(features_dir)
            .unwrap_or(&path)
            .display()
            .to_string();
        let mut err = |m: String| report.errors.push(format!("{rel}: {m}"));
        let text = fs::read_to_string(&path).unwrap_or_default();
        report.files += 1;

        let header: String = text
            .lines()
            .take_while(|l| l.starts_with('#') || l.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if !header.contains("# Sources:") {
            err("missing a leading `# Sources:` block".into());
        }
        let parity = text
            .lines()
            .any(|l| l.split_whitespace().any(|w| w == "@qt-parity"));
        if parity && !QT_VERSION.is_match(&header) {
            err("a @qt-parity file must name the Qt version it was verified on (`# Verified on Qt 6.x.y`)".into());
        }

        let mut in_doc = false;
        let mut doc_lines = 0;
        for (i, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("\"\"\"") {
                if in_doc && doc_lines > MAX_QML_LINES {
                    err(format!(
                        "line {}: doc string has {doc_lines} lines (max {MAX_QML_LINES})",
                        i + 1
                    ));
                }
                in_doc = !in_doc;
                doc_lines = 0;
            } else if in_doc {
                doc_lines += 1;
            }
            let lower = line.to_lowercase();
            let is_text = !line.trim_start().starts_with('#') && !in_doc;
            if is_text {
                for w in FORBIDDEN {
                    if lower.contains(w) {
                        err(format!(
                            "line {}: mentions {w:?}; scenarios speak QML, not implementation",
                            i + 1
                        ));
                    }
                }
            }
        }

        let feature = match gherkin::Feature::parse_path(&path, gherkin::GherkinEnv::default()) {
            Ok(f) => f,
            Err(e) => {
                err(format!("does not parse: {e}"));
                continue;
            }
        };
        let file_tags = feature.tags.clone();
        let feature = match feature.expand_examples() {
            Ok(f) => f,
            Err(e) => {
                err(format!("examples do not expand: {e}"));
                continue;
            }
        };
        let background: Vec<_> = feature
            .background
            .iter()
            .flat_map(|b| b.steps.iter())
            .collect();
        let scenarios = feature
            .scenarios
            .iter()
            .chain(feature.rules.iter().flat_map(|r| r.scenarios.iter()));
        for s in scenarios {
            report.scenarios += 1;
            let tags: BTreeSet<&str> = file_tags
                .iter()
                .chain(&s.tags)
                .map(String::as_str)
                .collect();
            if tags
                .iter()
                .any(|t| matches!(*t, "qt-parity" | "qt-diverges" | "ljos-only" | "qt-pixels"))
            {
                report.verified += 1;
            }
            for step in background.iter().copied().chain(&s.steps) {
                *shapes.entry(shape(&step.value)).or_default() += 1;
                let hits = patterns
                    .iter()
                    .filter(|re| re.is_match(&step.value))
                    .count();
                let backlog = tags
                    .iter()
                    .any(|t| *t == "backlog" || t.starts_with("backlog-"));
                match hits {
                    // Backlog work may use phrasings that are not built yet; they are listed as planned in the README.
                    0 if backlog => report.planned_steps += 1,
                    0 => err(format!(
                        "scenario {:?}: no step definition for {:?}",
                        s.name, step.value
                    )),
                    1 => {}
                    n => err(format!(
                        "scenario {:?}: {n} step definitions match {:?}",
                        s.name, step.value
                    )),
                }
            }
        }
    }
    report.distinct_steps = shapes.len();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEPS: &str = r##"
        #[given("the QML:")]
        #[when(regex = r#"^"([^"]+)" is loaded$"#)]
        #[then(regex = r#"^the object "([^"]+)" (exists|does not exist)$"#)]
    "##;

    fn write(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).unwrap();
    }

    // Removed on drop, so test runs leave nothing behind in the temp dir.
    fn tmp(tag: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("ljos-lint-{tag}-"))
            .tempdir()
            .unwrap()
    }

    #[test]
    fn reads_patterns_from_macro_attributes() {
        assert_eq!(step_patterns(STEPS).len(), 3);
        assert_eq!(step_patterns(r#"#[when(regex = r"^(\d+) ms$")]"#).len(), 1);
        let wrapped = "#[given(\n    regex = r#\"^a (\\d+) long one$\"#\n)]";
        assert_eq!(step_patterns(wrapped).len(), 1);
    }

    #[test]
    fn a_clean_file_passes() {
        let d = tmp("clean");
        write(
            d.path(),
            "a.feature",
            "# Sources:\n#   https://doc.qt.io/\n\n@ljos-only\nFeature: F\n  Scenario: s\n    Given the QML:\n      \"\"\"qml\n      Item {}\n      \"\"\"\n    When \"A.qml\" is loaded\n    Then the object \"x\" exists\n",
        );
        let r = lint(d.path(), STEPS);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!((r.scenarios, r.verified), (1, 1));
    }

    #[test]
    fn reports_each_kind_of_problem() {
        let d = tmp("bad");
        write(
            d.path(),
            "b.feature",
            "@qt-parity\nFeature: F\n  Scenario: s\n    When the egui frame is drawn\n",
        );
        let r = lint(d.path(), STEPS).errors.join("\n");
        for needle in ["`# Sources:`", "Qt version", "no step definition", "egui"] {
            assert!(r.contains(needle), "missing {needle:?} in:\n{r}");
        }
    }

    #[test]
    fn long_doc_strings_are_flagged() {
        let d = tmp("long");
        let body = "Item {}\n".repeat(26);
        write(
            d.path(),
            "c.feature",
            &format!(
                "# Sources:\n#   x\nFeature: F\n  Scenario: s\n    Given the QML:\n      \"\"\"qml\n{body}      \"\"\"\n"
            ),
        );
        assert!(
            lint(d.path(), STEPS)
                .errors
                .iter()
                .any(|e| e.contains("26 lines"))
        );
    }
}
