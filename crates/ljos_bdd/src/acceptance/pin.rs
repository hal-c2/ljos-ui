//! `PIN`: which revision of an external suite these results are about.
//!
//! The file is `key = value` lines; `#` starts a comment. A suite is a
//! git checkout (`commit`) or, for a fixture that lives inside this
//! repository and so has no git history of its own, a directory (`digest`).

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// What a suite's `PIN` says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pin {
    /// Where the suite comes from. Only shown in messages.
    pub repo: Option<String>,
    /// The commit the checkout must be at; at least 7 hex digits.
    pub commit: Option<String>,
    /// `blake3:<hex>` of the checkout's files, instead of a commit.
    pub digest: Option<String>,
    /// Environment variable that holds the checkout's path.
    pub env: Option<String>,
    /// Where the checkout is when nothing else says, relative to the
    /// suite's directory.
    pub path: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PinError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("PIN line {line}: {why}")]
    Syntax { line: usize, why: String },
    #[error("PIN: {0}")]
    Invalid(String),
    #[error("PIN must name exactly one of `commit` and `digest`")]
    NeedsOneRevision,
    #[error(
        "no checkout of the suite: give --path, set ${env} or add `path` to the PIN",
        env = .0.as_deref().unwrap_or("<the PIN's env variable>")
    )]
    NoCheckout(Option<String>),
    #[error("{0} is not a directory")]
    NotADirectory(String),
    #[error("{path} is not a git checkout (git said: {said})")]
    NotGit { path: String, said: String },
    #[error("{path} is at commit {found}, but the PIN says {pinned}{repo}")]
    WrongCommit {
        path: String,
        found: String,
        pinned: String,
        repo: String,
    },
    #[error("{path} has digest {found}, but the PIN says {pinned}")]
    WrongDigest {
        path: String,
        found: String,
        pinned: String,
    },
    #[error(
        "{path} has uncommitted changes to tracked files; commit or stash them, or pass --allow-dirty"
    )]
    Dirty { path: String },
}

impl Pin {
    /// # Errors
    /// Unknown keys, lines without `=`, or a PIN with no revision.
    pub fn parse(text: &str) -> Result<Self, PinError> {
        let mut pin = Self::default();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let syntax = |why: String| PinError::Syntax { line: n + 1, why };
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| syntax("expected `key = value`".into()))?;
            let value = value.trim().to_owned();
            if value.is_empty() {
                return Err(syntax(format!("{:?} has no value", key.trim())));
            }
            let slot = match key.trim() {
                "repo" => &mut pin.repo,
                "commit" => &mut pin.commit,
                "digest" => &mut pin.digest,
                "env" => &mut pin.env,
                "path" => &mut pin.path,
                other => return Err(syntax(format!("unknown key {other:?}"))),
            };
            if slot.replace(value).is_some() {
                return Err(syntax(format!("{:?} is given twice", key.trim())));
            }
        }
        match (&pin.commit, &pin.digest) {
            (Some(c), None) => {
                if c.len() < 7 || !c.chars().all(|ch| ch.is_ascii_hexdigit()) {
                    return Err(PinError::Invalid(format!(
                        "commit {c:?} is not at least 7 hex digits"
                    )));
                }
            }
            (None, Some(d)) => {
                if !d
                    .strip_prefix("blake3:")
                    .is_some_and(|h| h.len() == 64 && h.chars().all(|ch| ch.is_ascii_hexdigit()))
                {
                    return Err(PinError::Invalid(format!(
                        "digest {d:?} is not `blake3:` and 64 hex digits"
                    )));
                }
            }
            _ => return Err(PinError::NeedsOneRevision),
        }
        Ok(pin)
    }

    /// # Errors
    /// The file cannot be read, or [`Self::parse`] fails.
    pub fn read(file: &Path) -> Result<Self, PinError> {
        let text = fs::read_to_string(file).map_err(|source| PinError::Io {
            path: file.display().to_string(),
            source,
        })?;
        Self::parse(&text)
    }

    /// Where the suite's files are. An explicit path wins over the
    /// environment, which wins over the PIN's own default.
    ///
    /// # Errors
    /// None of the three says, or the place is not a directory.
    pub fn checkout(
        &self,
        explicit: Option<&Path>,
        suite_dir: &Path,
        environment: impl Fn(&str) -> Option<String>,
    ) -> Result<PathBuf, PinError> {
        let from_env = || {
            self.env
                .as_deref()
                .and_then(&environment)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let path = explicit
            .map(Path::to_path_buf)
            .or_else(from_env)
            .or_else(|| self.path.as_ref().map(|p| suite_dir.join(p)))
            .ok_or_else(|| PinError::NoCheckout(self.env.clone()))?;
        if !path.is_dir() {
            return Err(PinError::NotADirectory(path.display().to_string()));
        }
        Ok(path)
    }

    /// Checks that `checkout` is the revision the PIN names. Reads only.
    ///
    /// # Errors
    /// It is another revision, or git cannot tell.
    pub fn verify(&self, checkout: &Path, allow_dirty: bool) -> Result<(), PinError> {
        let shown = checkout.display().to_string();
        if let Some(pinned) = &self.commit {
            let found = git(checkout, &["rev-parse", "HEAD"])?;
            if !found.starts_with(&pinned.to_ascii_lowercase()) {
                return Err(PinError::WrongCommit {
                    path: shown,
                    found,
                    pinned: pinned.clone(),
                    repo: self
                        .repo
                        .as_ref()
                        .map_or_else(String::new, |r| format!(" (from {r})")),
                });
            }
            if !allow_dirty
                && !git(checkout, &["status", "--porcelain", "--untracked-files=no"])?.is_empty()
            {
                return Err(PinError::Dirty { path: shown });
            }
        }
        if let Some(pinned) = &self.digest {
            let found = tree_digest(checkout)?;
            if !found.eq_ignore_ascii_case(pinned) {
                return Err(PinError::WrongDigest {
                    path: shown,
                    found,
                    pinned: pinned.clone(),
                });
            }
        }
        Ok(())
    }
}

fn git(checkout: &Path, args: &[&str]) -> Result<String, PinError> {
    let shown = checkout.display().to_string();
    let output = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(args)
        .output()
        .map_err(|e| PinError::NotGit {
            path: shown.clone(),
            said: e.to_string(),
        })?;
    if !output.status.success() {
        return Err(PinError::NotGit {
            path: shown,
            said: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The relative paths (with `/`) of every file below `root`, sorted, except
/// what git keeps in `.git`.
///
/// # Errors
/// A directory cannot be read.
pub fn files_below(root: &Path) -> Result<Vec<String>, PinError> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), PinError> {
        let io = |source| PinError::Io {
            path: dir.display().to_string(),
            source,
        };
        for entry in fs::read_dir(dir).map_err(io)? {
            let entry = entry.map_err(io)?;
            let path = entry.path();
            if entry.file_name() == ".git" {
                continue;
            }
            if entry.file_type().map_err(io)?.is_dir() {
                walk(root, &path, out)?;
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(
                    rel.components()
                        .map(|c| c.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

/// `blake3:<hex>` over every file's path and bytes, in path order. Lengths
/// are part of the input so two trees cannot be made to collide by moving
/// bytes between a name and a body.
///
/// # Errors
/// A file cannot be read.
pub fn tree_digest(root: &Path) -> Result<String, PinError> {
    let mut hasher = blake3::Hasher::new();
    for rel in files_below(root)? {
        let bytes = fs::read(root.join(&rel)).map_err(|source| PinError::Io {
            path: rel.clone(),
            source,
        })?;
        hasher.update(&(rel.len() as u64).to_le_bytes());
        hasher.update(rel.as_bytes());
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "blake3:0000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn a_pin_has_one_revision() {
        let pin =
            Pin::parse("# a suite\nrepo = x # where\ncommit = 0123abc\nenv = SUITE\n").unwrap();
        assert_eq!(pin.commit.as_deref(), Some("0123abc"));
        assert_eq!(pin.repo.as_deref(), Some("x"));
        assert!(matches!(
            Pin::parse("repo = x\n"),
            Err(PinError::NeedsOneRevision)
        ));
        assert!(matches!(
            Pin::parse(&format!("commit = 0123abc\ndigest = {HASH}\n")),
            Err(PinError::NeedsOneRevision)
        ));
    }

    #[test]
    fn a_pin_is_strict_about_what_it_reads() {
        for (text, needle) in [
            ("commit = 0123abc\nbranch = main\n", "unknown key"),
            ("commit\n", "key = value"),
            ("commit =\n", "no value"),
            ("commit = 12\n", "7 hex"),
            ("commit = 0123abc\ncommit = 0123abd\n", "twice"),
            ("digest = blake3:ab\n", "64 hex"),
        ] {
            let e = Pin::parse(text).unwrap_err().to_string();
            assert!(e.contains(needle), "{text:?} -> {e}");
        }
    }

    #[test]
    fn the_checkout_comes_from_the_path_then_the_environment_then_the_pin() {
        let dir = tempfile::tempdir().unwrap();
        for d in ["a", "b", "c"] {
            fs::create_dir(dir.path().join(d)).unwrap();
        }
        let pin = Pin::parse("commit = 0123abc\nenv = SUITE\npath = c\n").unwrap();
        let env_b = |_: &str| Some(dir.path().join("b").display().to_string());
        let none = |_: &str| None;
        let a = dir.path().join("a");
        assert_eq!(pin.checkout(Some(&a), dir.path(), env_b).unwrap(), a);
        assert_eq!(
            pin.checkout(None, dir.path(), env_b).unwrap(),
            dir.path().join("b")
        );
        assert_eq!(
            pin.checkout(None, dir.path(), none).unwrap(),
            dir.path().join("c")
        );
        let bare = Pin::parse("commit = 0123abc\n").unwrap();
        assert!(matches!(
            bare.checkout(None, dir.path(), none),
            Err(PinError::NoCheckout(None))
        ));
        assert!(matches!(
            bare.checkout(Some(&dir.path().join("gone")), dir.path(), none),
            Err(PinError::NotADirectory(_))
        ));
    }

    #[test]
    fn a_digest_follows_names_and_bytes() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/a.feature"), "one").unwrap();
        let first = tree_digest(dir.path()).unwrap();
        assert_eq!(first, tree_digest(dir.path()).unwrap());
        fs::write(dir.path().join("sub/a.feature"), "two").unwrap();
        let second = tree_digest(dir.path()).unwrap();
        assert_ne!(first, second);
        fs::rename(
            dir.path().join("sub/a.feature"),
            dir.path().join("sub/b.feature"),
        )
        .unwrap();
        assert_ne!(second, tree_digest(dir.path()).unwrap());
        let pin = Pin::parse(&format!("digest = {HASH}\n")).unwrap();
        assert!(matches!(
            pin.verify(dir.path(), false),
            Err(PinError::WrongDigest { .. })
        ));
        let right =
            Pin::parse(&format!("digest = {}\n", tree_digest(dir.path()).unwrap())).unwrap();
        right.verify(dir.path(), false).unwrap();
    }
}
