//! `features.globs`: the allowlist of a suite's feature files.
//!
//! One glob per line, relative to the checkout, `#` starts a comment. A
//! line starting with `!` takes files out again. A glob may end in
//! `.feature:<name glob>` to list (or with `!`, take out) only the
//! scenarios of those files whose name matches. Every glob has to match
//! something: an allowlist that silently stopped matching (the suite moved a
//! directory) would otherwise turn a suite into zero scenarios and a green
//! result. A name glob is held to that too, once the files are read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use globset::{Glob, GlobMatcher};

use super::pin::{PinError, files_below};

#[derive(Debug, thiserror::Error)]
pub enum GlobError {
    #[error("features.globs line {line}: {source}")]
    Bad { line: usize, source: globset::Error },
    #[error("features.globs lists no feature files")]
    Empty,
    #[error("features.globs line {line}: `{glob}` matches no file in {root}")]
    NoMatch {
        line: usize,
        glob: String,
        root: String,
    },
    #[error("{0}")]
    Walk(#[from] PinError),
    #[error("features.globs: {0}")]
    Io(String),
}

struct Line {
    number: usize,
    text: String,
    exclude: bool,
    matcher: GlobMatcher,
    /// The scenario-name glob after `.feature:`, if the line has one.
    names: Option<String>,
}

/// A listed feature file and which of its scenarios are listed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listed {
    /// Relative to the checkout.
    pub file: String,
    /// Name globs of the scenarios listed; empty when the whole file is.
    pub only: Vec<String>,
    /// Name globs of scenarios taken out again.
    pub except: Vec<String>,
}

impl Listed {
    /// The whole of `file`.
    #[must_use]
    pub fn file(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            ..Self::default()
        }
    }
}

/// # Errors
/// A glob is malformed, or nothing is listed.
fn parse(text: &str) -> Result<Vec<Line>, GlobError> {
    let mut lines = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let (exclude, glob) = line
            .strip_prefix('!')
            .map_or((false, line), |rest| (true, rest.trim()));
        let (glob, names) = match glob.split_once(".feature:") {
            Some((files, names)) => (&glob[..files.len() + ".feature".len()], Some(names)),
            None => (glob, None),
        };
        if let Some(names) = names {
            Glob::new(names).map_err(|source| GlobError::Bad {
                line: n + 1,
                source,
            })?;
        }
        let matcher = Glob::new(glob)
            .map_err(|source| GlobError::Bad {
                line: n + 1,
                source,
            })?
            .compile_matcher();
        lines.push(Line {
            number: n + 1,
            text: glob.to_owned(),
            exclude,
            matcher,
            names: names.map(str::to_owned),
        });
    }
    if lines.iter().all(|l| l.exclude) {
        return Err(GlobError::Empty);
    }
    Ok(lines)
}

/// The suite's feature files, relative to `root` and sorted, each with
/// the scenario-name globs that narrow it.
///
/// # Errors
/// See [`GlobError`].
pub fn expand(text: &str, root: &Path) -> Result<Vec<Listed>, GlobError> {
    let lines = parse(text)?;
    let files = files_below(root)?;
    let mut out: BTreeMap<String, Listed> = BTreeMap::new();
    // A file listed whole stays whole whatever names other lines list.
    let mut whole: BTreeSet<String> = BTreeSet::new();
    let mut dropped: BTreeSet<String> = BTreeSet::new();
    for line in &lines {
        let hits: Vec<&String> = files.iter().filter(|f| line.matcher.is_match(f)).collect();
        if hits.is_empty() {
            return Err(GlobError::NoMatch {
                line: line.number,
                glob: line.text.clone(),
                root: root.display().to_string(),
            });
        }
        for f in hits {
            match (line.exclude, &line.names) {
                // Excludes apply to everything listed before and after them.
                (true, None) => {
                    dropped.insert(f.clone());
                }
                (true, Some(n)) => out.entry(f.clone()).or_default().except.push(n.clone()),
                (false, None) => {
                    whole.insert(f.clone());
                    out.entry(f.clone()).or_default();
                }
                (false, Some(n)) => out.entry(f.clone()).or_default().only.push(n.clone()),
            }
        }
    }
    let listed: Vec<Listed> = out
        .into_iter()
        .filter(|(f, _)| !dropped.contains(f))
        .filter_map(|(file, mut l)| {
            if whole.contains(&file) {
                l.only.clear();
            } else if l.only.is_empty() {
                // Only taken out of, never listed.
                return None;
            }
            l.file = file;
            Some(l)
        })
        .collect();
    if listed.is_empty() {
        return Err(GlobError::Empty);
    }
    Ok(listed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for f in [
            "features/a.feature",
            "features/deep/b.feature",
            "features/c.txt",
            "other/d.feature",
        ] {
            let p = dir.path().join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "").unwrap();
        }
        dir
    }

    #[test]
    fn globs_list_files_and_excludes_take_them_out() {
        let dir = tree();
        let got = expand(
            "features/**/*.feature # all\n!features/deep/*\n",
            dir.path(),
        );
        assert_eq!(got.unwrap(), [Listed::file("features/a.feature")]);
        let got = expand("features/**/*.feature\nother/*.feature\n", dir.path()).unwrap();
        let files: Vec<&str> = got.iter().map(|l| l.file.as_str()).collect();
        assert_eq!(
            files,
            [
                "features/a.feature",
                "features/deep/b.feature",
                "other/d.feature"
            ]
        );
    }

    #[test]
    fn a_line_can_name_the_scenarios_of_a_file() {
        let dir = tree();
        let got = expand(
            "features/a.feature:Adding*\nfeatures/a.feature:Taking one\n\
             features/deep/*.feature\n!features/deep/b.feature:Slow *\n\
             !other/d.feature:Anything\n",
            dir.path(),
        )
        .unwrap();
        let a = Listed {
            only: vec!["Adding*".into(), "Taking one".into()],
            ..Listed::file("features/a.feature")
        };
        let b = Listed {
            except: vec!["Slow *".into()],
            ..Listed::file("features/deep/b.feature")
        };
        assert_eq!(got, [a, b], "a file only taken out of is not listed");
        let got = expand("features/a.feature:One\nfeatures/a.feature\n", dir.path()).unwrap();
        assert_eq!(got, [Listed::file("features/a.feature")], "whole wins");
    }

    #[test]
    fn a_glob_that_matches_nothing_is_an_error_naming_it() {
        let dir = tree();
        let e = expand("features/*.feature\nmoved/*.feature\n", dir.path()).unwrap_err();
        assert!(matches!(e, GlobError::NoMatch { line: 2, .. }), "{e}");
        assert!(e.to_string().contains("moved/*.feature"));
    }

    #[test]
    fn nothing_listed_and_bad_globs_are_errors() {
        let dir = tree();
        assert!(matches!(
            expand("# nothing\n", dir.path()),
            Err(GlobError::Empty)
        ));
        assert!(matches!(
            expand("!features/*\n", dir.path()),
            Err(GlobError::Empty)
        ));
        assert!(matches!(
            expand("features/[\n", dir.path()),
            Err(GlobError::Bad { line: 1, .. })
        ));
    }
}
