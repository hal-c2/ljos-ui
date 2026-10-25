//! The fixtures every check of a rule is tested on
//! (`crates/ljos_design/fixtures/<code>/<case>.bad.qml` and its
//! `<case>.good.qml` twin, `fixtures/README.md`). The static lint tests and
//! the runtime audit tests read them through here, so the two agree on what
//! a fixture says it must produce.

use std::path::{Path, PathBuf};

/// The directory the fixtures are in.
pub fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

/// One finding a bad fixture must produce: `<code> at <line>`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Expected {
    pub code: String,
    /// 1-based, the QML line the finding points at.
    pub line: u32,
}

/// One case of a rule: a bad file and the good one written right.
#[derive(Clone, Debug)]
pub struct Case {
    /// The rule's code: the directory's name.
    pub code: String,
    /// The file name without `.bad.qml`/`.good.qml`.
    pub name: String,
    pub bad: PathBuf,
    pub good: PathBuf,
}

/// Every case below [`dir`], by code then name. A bad file with no good twin
/// is still listed (the structure test fails on it).
pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let Ok(rules) = std::fs::read_dir(dir()) else {
        return out;
    };
    for rule in rules.flatten() {
        let path = rule.path();
        if !path.is_dir() {
            continue;
        }
        let code = rule.file_name().to_string_lossy().into_owned();
        let Ok(files) = std::fs::read_dir(&path) else {
            continue;
        };
        for file in files.flatten() {
            let file_name = file.file_name().to_string_lossy().into_owned();
            if let Some(name) = file_name.strip_suffix(".bad.qml") {
                out.push(Case {
                    code: code.clone(),
                    name: name.to_owned(),
                    bad: file.path(),
                    good: path.join(format!("{name}.good.qml")),
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.code, &a.name).cmp(&(&b.code, &b.name)));
    out
}

/// What a fixture's header says is wrong with it, or why the header cannot
/// be read.
pub fn expected(source: &str) -> Result<Vec<Expected>, String> {
    let Some(list) = header(source, "expect") else {
        return Err("no `// expect: <code> at <line>` line".to_owned());
    };
    let mut out = Vec::new();
    for item in list.split(',') {
        let item = item.trim();
        let (code, line) = item
            .split_once(" at ")
            .ok_or_else(|| format!("`{item}` is not `<code> at <line>`"))?;
        let line: u32 = line
            .trim()
            .parse()
            .map_err(|_| format!("`{item}`: `{}` is not a line number", line.trim()))?;
        out.push(Expected {
            code: code.trim().to_owned(),
            line,
        });
    }
    out.sort();
    Ok(out)
}

/// The matrix point a runtime fixture names (`// audit: scheme=dark …`), as
/// key and value pairs in the order written; empty without the line.
pub fn audit(source: &str) -> Result<Vec<(String, String)>, String> {
    let Some(list) = header(source, "audit") else {
        return Ok(Vec::new());
    };
    list.split_whitespace()
        .map(|pair| {
            let (key, value) = pair
                .split_once('=')
                .ok_or_else(|| format!("`{pair}` is not `key=value`"))?;
            if !AUDIT_KEYS.contains(&key) {
                return Err(format!("`{key}` is not one of {AUDIT_KEYS:?}"));
            }
            Ok((key.to_owned(), value.to_owned()))
        })
        .collect()
}

/// What an `// audit:` line may set.
pub const AUDIT_KEYS: &[&str] = &["scheme", "contrast", "font-scale", "size", "reduced-motion"];

/// The text after `// <key>:` in the comment lines that open the file.
fn header<'s>(source: &'s str, key: &str) -> Option<&'s str> {
    let prefix = format!("{key}:");
    source
        .lines()
        .map(str::trim)
        .take_while(|line| line.starts_with("//"))
        .find_map(|line| line.trim_start_matches('/').trim().strip_prefix(&prefix))
        .map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_names_codes_and_lines() {
        let source = "// expect: motion-bounce at 7, color-literal at 3\n// audit: scheme=dark font-scale=2\nimport QtQuick\n";
        assert_eq!(
            expected(source).unwrap(),
            [
                Expected {
                    code: "color-literal".into(),
                    line: 3
                },
                Expected {
                    code: "motion-bounce".into(),
                    line: 7
                },
            ]
        );
        assert_eq!(
            audit(source).unwrap(),
            [
                ("scheme".to_owned(), "dark".to_owned()),
                ("font-scale".to_owned(), "2".to_owned())
            ]
        );
    }

    #[test]
    fn a_bad_header_says_why() {
        assert!(expected("import QtQuick\n").is_err());
        assert!(expected("// expect: motion-bounce on 7\n").is_err());
        assert!(audit("// audit: theme=dark\n").is_err());
        assert_eq!(audit("import QtQuick\n").unwrap(), []);
    }
}
