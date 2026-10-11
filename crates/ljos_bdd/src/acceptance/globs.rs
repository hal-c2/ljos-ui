//! `features.globs`: the allowlist of a suite's feature files.
//!
//! One glob per line, relative to the checkout, `#` starts a comment. A
//! line starting with `!` takes files out again. Every glob has to match
//! something: an allowlist that silently stopped matching (the suite moved a
//! directory) would otherwise turn a suite into zero scenarios and a green
//! result.

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
        });
    }
    if lines.iter().all(|l| l.exclude) {
        return Err(GlobError::Empty);
    }
    Ok(lines)
}

/// The suite's feature files, relative to `root` and sorted.
///
/// # Errors
/// See [`GlobError`].
pub fn expand(text: &str, root: &Path) -> Result<Vec<String>, GlobError> {
    let lines = parse(text)?;
    let files = files_below(root)?;
    let mut out = Vec::new();
    for line in &lines {
        let hits: Vec<&String> = files.iter().filter(|f| line.matcher.is_match(f)).collect();
        if hits.is_empty() {
            return Err(GlobError::NoMatch {
                line: line.number,
                glob: line.text.clone(),
                root: root.display().to_string(),
            });
        }
        if !line.exclude {
            out.extend(hits.into_iter().cloned());
        }
    }
    // Excludes apply to everything listed before and after them.
    out.retain(|f| {
        !lines
            .iter()
            .any(|l| l.exclude && l.matcher.is_match(f.as_str()))
    });
    out.sort();
    out.dedup();
    if out.is_empty() {
        return Err(GlobError::Empty);
    }
    Ok(out)
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
        assert_eq!(got.unwrap(), ["features/a.feature"]);
        let got = expand("features/**/*.feature\nother/*.feature\n", dir.path()).unwrap();
        assert_eq!(
            got,
            [
                "features/a.feature",
                "features/deep/b.feature",
                "other/d.feature"
            ]
        );
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
