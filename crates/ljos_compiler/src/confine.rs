//! Where the files of an application's user may come from: the one
//! directory the application names, and nothing outside it (the `ljos`
//! feature `user-qml`, ARCHITECTURE.md §12.1). Every read of a user's
//! file, and every address the engine is asked to load while the user's
//! files run, is checked here and nowhere else.
//!
//! A user's directory is the application's root: its files are keyed as
//! the application's are, and an address in one resolves against
//! `file:///`, so `../` climbs no higher than that root and an absolute
//! path names a file of it. What is refused is told, never skipped
//! silently: a link that leads out, an address that names a place
//! neither the application nor the directory has, and a network address.

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

use crate::reload::CompileFault;

/// Why a file or an address was not loaded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Refusal {
    /// A link (or a directory on its way) leads out of the directory.
    #[error("{0}: a link that leads out of the user's directory")]
    LinkOut(String),
    /// A path or address that names a place outside the application's
    /// files and its user's directory.
    #[error("{0}: names a place outside the application and its user's directory")]
    Outside(String),
    /// An address on the network, or on another host.
    #[error("{0}: a network address; the user's files load from their directory only")]
    Network(String),
    /// A name that is not text.
    #[error("{0}: a file name that is not text")]
    Unnamed(String),
}

impl Refusal {
    /// The refusal as a fault a host tells, as it tells a file that does
    /// not compile.
    #[must_use]
    pub fn fault(&self) -> CompileFault {
        let file = match self {
            Self::LinkOut(f) | Self::Outside(f) | Self::Network(f) | Self::Unnamed(f) => f.clone(),
        };
        CompileFault {
            file,
            line: 0,
            column: 0,
            message: format!("refused: {self}"),
        }
    }
}

/// Why a user's file was not read.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    Refused(Refusal),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// More than this is not a user's QML but a disk.
const MAX_DIRS: usize = 4096;

/// The directory an application's user's files are in.
#[derive(Debug, Clone)]
pub struct UserDir {
    root: PathBuf,
}

impl UserDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        // Absolute, so that the paths a watcher reports compare to it.
        let root = std::path::absolute(&root).unwrap_or(root);
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The root with its links followed; None while it does not exist.
    fn real_root(&self) -> Option<PathBuf> {
        self.root.canonicalize().ok()
    }

    /// The path from the root (`/`-separated) of `path`, a path under it;
    /// a path elsewhere, or one that steps up, is refused.
    pub fn relative(&self, path: &Path) -> Result<String, Refusal> {
        let shown = || path.display().to_string();
        let rest = path
            .strip_prefix(&self.root)
            .map_err(|_| Refusal::Outside(shown()))?;
        let mut parts = Vec::new();
        for part in rest.components() {
            match part {
                Component::Normal(name) => {
                    parts.push(name.to_str().ok_or_else(|| Refusal::Unnamed(shown()))?);
                }
                Component::CurDir => {}
                _ => return Err(Refusal::Outside(shown())),
            }
        }
        Ok(parts.join("/"))
    }

    /// The text of the file at `path`. A path outside the directory, or
    /// whose links lead out of it, is refused; what is read is the file
    /// the check found, so a link changed between the two reads nothing
    /// outside.
    pub fn read(&self, path: &Path) -> Result<String, ReadError> {
        let relative = self.relative(path).map_err(ReadError::Refused)?;
        let real = path.canonicalize()?;
        let root = self.real_root().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "the directory is gone")
        })?;
        if !real.starts_with(&root) {
            return Err(ReadError::Refused(Refusal::LinkOut(relative)));
        }
        Ok(std::fs::read_to_string(real)?)
    }

    /// Whether the directory `dir` (a path from the root, `""` for the
    /// root) is there, and in the directory, links followed.
    #[must_use]
    pub fn has_dir(&self, dir: &str) -> bool {
        if dir.split('/').any(|part| part == ".." || part == ".") {
            return false;
        }
        let (Some(root), Ok(real)) = (self.real_root(), self.root.join(dir).canonicalize()) else {
            return false;
        };
        real.starts_with(root) && real.is_dir()
    }

    /// Every file under the directory a user may change the application
    /// with (`wanted` chooses by name), by path from the root with its
    /// text; and what was refused. Links inside the directory are read
    /// as the files they name; a link that leads out is refused. A
    /// directory a walk skips ([`crate::skipped_dir`]) is not entered.
    #[must_use]
    pub fn list(&self, wanted: impl Fn(&str) -> bool) -> (BTreeMap<String, String>, Vec<Refusal>) {
        let mut files = BTreeMap::new();
        let mut refused = Vec::new();
        let Some(root) = self.real_root() else {
            return (files, refused);
        };
        let mut stack = vec![self.root.clone()];
        let mut dirs = 0;
        while let Some(dir) = stack.pop() {
            dirs += 1;
            if dirs > MAX_DIRS {
                break;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(relative) = self.relative(&path) else {
                    refused.push(Refusal::Unnamed(path.display().to_string()));
                    continue;
                };
                let name = relative.rsplit('/').next().unwrap_or(&relative);
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    if !crate::skipped_dir(name) {
                        stack.push(path);
                    }
                    continue;
                }
                if name.starts_with('.') || !wanted(name) {
                    continue;
                }
                if kind.is_symlink() {
                    match path.canonicalize() {
                        Ok(real) if !real.starts_with(&root) => {
                            refused.push(Refusal::LinkOut(relative));
                            continue;
                        }
                        // A link to a directory is not entered: it can loop.
                        Ok(real) if !real.is_file() => continue,
                        Ok(_) => {}
                        // A link to nothing is no file.
                        Err(_) => continue,
                    }
                }
                match self.read(&path) {
                    Ok(text) => {
                        files.insert(relative, text);
                    }
                    Err(ReadError::Refused(why)) => refused.push(why),
                    // Being written, or not text: its next save brings it.
                    Err(ReadError::Io(_)) => {}
                }
            }
        }
        (files, refused)
    }
}

/// Whether an address the engine is asked to load may be: a `file:` URL
/// with no host and no step up, a `qrc:` one, or an address with no
/// scheme. Anything else is on the network or another host.
pub fn check_address(url: &str) -> Result<(), Refusal> {
    let refused_outside = || Refusal::Outside(url.to_owned());
    let path = if let Some(rest) = url.strip_prefix("file://") {
        let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        if !host.is_empty() && host != "localhost" {
            return Err(Refusal::Network(url.to_owned()));
        }
        path
    } else if let Some(rest) = url.strip_prefix("qrc:") {
        rest
    } else if has_scheme(url) {
        return Err(Refusal::Network(url.to_owned()));
    } else {
        url
    };
    let path = path.split(['?', '#']).next().unwrap_or(path);
    if path.contains('\\') || path.contains('\0') || path.split('/').any(|part| part == "..") {
        return Err(refused_outside());
    }
    Ok(())
}

/// Whether `url` begins with a scheme (`https:`), as RFC 3986 spells one;
/// a drive letter (`C:`) is a path.
fn has_scheme(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    scheme.len() > 1
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_addresses_without_a_step_up_are_loaded() {
        assert_eq!(check_address("file:///Shell.qml"), Ok(()));
        assert_eq!(check_address("file://localhost/parts/Card.qml"), Ok(()));
        assert_eq!(check_address("qrc:/Main.qml"), Ok(()));
        assert_eq!(check_address("Shell.qml"), Ok(()));
        assert!(matches!(
            check_address("https://example.com/Evil.qml"),
            Err(Refusal::Network(_))
        ));
        assert!(matches!(
            check_address("file://server/share/Evil.qml"),
            Err(Refusal::Network(_))
        ));
        assert!(matches!(
            check_address("file:///../etc/Evil.qml"),
            Err(Refusal::Outside(_))
        ));
    }

    #[test]
    fn a_link_out_of_the_directory_is_refused_and_one_inside_is_read() {
        let outside = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("Evil.qml"), "Item {}").unwrap();
        std::fs::write(root.path().join("Shell.qml"), "Item {}").unwrap();
        std::fs::create_dir(root.path().join("parts")).unwrap();
        std::fs::write(root.path().join("parts/Card.qml"), "Rectangle {}").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                outside.path().join("Evil.qml"),
                root.path().join("Out.qml"),
            )
            .unwrap();
            std::os::unix::fs::symlink(root.path().join("Shell.qml"), root.path().join("In.qml"))
                .unwrap();
        }
        let dir = UserDir::new(root.path());
        let (files, refused) = dir.list(|name| name.ends_with(".qml"));
        assert!(files.contains_key("Shell.qml"));
        assert!(files.contains_key("parts/Card.qml"));
        #[cfg(unix)]
        {
            assert!(files.contains_key("In.qml"));
            assert!(!files.contains_key("Out.qml"));
            assert_eq!(refused, vec![Refusal::LinkOut("Out.qml".to_owned())]);
            assert!(matches!(
                dir.read(&root.path().join("Out.qml")),
                Err(ReadError::Refused(Refusal::LinkOut(_)))
            ));
        }
        assert!(matches!(
            dir.read(&root.path().join("../x.qml")),
            Err(ReadError::Refused(Refusal::Outside(_)))
        ));
        assert!(dir.has_dir("parts"));
        assert!(!dir.has_dir("../"));
    }
}
