use std::path::{Path, PathBuf};

use ljos_mcp_core::{ErrorCode, SurfaceError};

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("project directory `{}` is not usable: {source}", path.display())]
    Root {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// The directory an authoring session is confined to (§19.7). Every path an
/// agent supplies goes through [`Project::confine`].
#[derive(Clone, Debug)]
pub struct Project {
    root: PathBuf,
}

impl Project {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let path = root.as_ref();
        let root = path.canonicalize().map_err(|source| ProjectError::Root {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn ledger_path(&self) -> PathBuf {
        self.root.join("ledger").join("qtquick.toml")
    }

    /// The ledger, resolved as a path an agent names is: a `ledger/` or a
    /// `qtquick.toml` that links out of the project is not read.
    pub fn ledger_file(&self) -> Result<PathBuf, SurfaceError> {
        self.confine("ledger/qtquick.toml")
    }

    pub fn docs_dir(&self) -> PathBuf {
        self.root.join("docs")
    }

    pub fn features_dir(&self) -> PathBuf {
        self.root.join("features")
    }

    /// `docs/`, `features/` and `features/README.md` as a walk or a listing
    /// starts from them: resolved as a path an agent names is, so one that
    /// links out of the project is not read.
    pub fn docs_root(&self) -> Result<PathBuf, SurfaceError> {
        self.confine_within(&self.docs_dir(), ".")
    }

    pub fn features_root(&self) -> Result<PathBuf, SurfaceError> {
        self.confine_within(&self.features_dir(), ".")
    }

    pub fn features_readme(&self) -> Result<PathBuf, SurfaceError> {
        self.confine_within(&self.features_dir(), "README.md")
    }

    /// Resolves `requested` (relative to the project, or absolute) to a
    /// canonical path that must lie inside `base` and inside the project.
    /// Canonicalising first means `..` and symlinks cannot escape, and
    /// checking the root too means `base` itself cannot be a link out; a
    /// missing path is `not_found`.
    pub fn confine_within(&self, base: &Path, requested: &str) -> Result<PathBuf, SurfaceError> {
        let outside = || {
            SurfaceError::new(
                ErrorCode::PathDenied,
                format!("`{requested}` is outside the project"),
            )
        };
        let joined = if Path::new(requested).is_absolute() {
            PathBuf::from(requested)
        } else {
            // A path that climbs out is refused for what it says, before
            // the disk is asked: otherwise `not_found` and `path_denied`
            // would tell an agent which files exist outside the project.
            let mut depth = 0usize;
            for part in Path::new(requested).components() {
                match part {
                    std::path::Component::ParentDir => {
                        depth = depth.checked_sub(1).ok_or_else(outside)?;
                    }
                    std::path::Component::Normal(_) => depth += 1,
                    _ => {}
                }
            }
            base.join(requested)
        };
        let canonical = joined.canonicalize().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SurfaceError::not_found(format!("`{requested}` does not exist"))
            } else {
                SurfaceError::new(ErrorCode::PathDenied, format!("`{requested}`: {e}"))
            }
        })?;
        let base = base
            .canonicalize()
            .map_err(|e| SurfaceError::new(ErrorCode::Internal, e.to_string()))?;
        if canonical.starts_with(&self.root) && canonical.starts_with(&base) {
            Ok(canonical)
        } else {
            Err(outside())
        }
    }

    pub fn confine(&self, requested: &str) -> Result<PathBuf, SurfaceError> {
        self.confine_within(&self.root, requested)
    }

    /// Project-relative display form with forward slashes.
    pub fn display(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_escapes_and_accepts_inner_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/a.qml"), "").unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("x.qml"), "").unwrap();

        let p = Project::open(dir.path()).unwrap();
        assert!(p.confine("sub/a.qml").is_ok());
        assert_eq!(
            p.confine("sub/../sub/a.qml").unwrap(),
            p.confine("sub/a.qml").unwrap()
        );
        // Refused whether or not there is such a file out there.
        for climbing in ["../x.qml", "../../etc/passwd", "sub/../../x.qml"] {
            assert_eq!(
                p.confine(climbing).unwrap_err().code,
                ErrorCode::PathDenied,
                "{climbing}"
            );
        }
        let abs = outside.path().join("x.qml");
        assert_eq!(
            p.confine(abs.to_str().unwrap()).unwrap_err().code,
            ErrorCode::PathDenied
        );
        assert_eq!(
            p.confine("missing.qml").unwrap_err().code,
            ErrorCode::NotFound
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_escape() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("x.qml"), "").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        let p = Project::open(dir.path()).unwrap();
        assert_eq!(
            p.confine("link/x.qml").unwrap_err().code,
            ErrorCode::PathDenied
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_base_that_links_out_is_not_inside_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("x.md"), "").unwrap();
        for base in ["docs", "features"] {
            std::os::unix::fs::symlink(outside.path(), dir.path().join(base)).unwrap();
        }
        let p = Project::open(dir.path()).unwrap();
        for base in [p.docs_dir(), p.features_dir()] {
            for requested in [".", "x.md"] {
                assert_eq!(
                    p.confine_within(&base, requested).unwrap_err().code,
                    ErrorCode::PathDenied,
                    "{}: {requested}",
                    base.display()
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn roots_that_link_out_are_not_walked() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("README.md"), "").unwrap();
        let root = tempfile::tempdir().unwrap();
        for base in ["docs", "features"] {
            std::os::unix::fs::symlink(outside.path(), root.path().join(base)).unwrap();
        }
        let p = Project::open(root.path()).unwrap();
        for found in [p.docs_root(), p.features_root(), p.features_readme()] {
            assert_eq!(found.unwrap_err().code, ErrorCode::PathDenied);
        }

        let file = tempfile::tempdir().unwrap();
        std::fs::create_dir(file.path().join("features")).unwrap();
        let readme = file.path().join("features/README.md");
        std::os::unix::fs::symlink(outside.path().join("README.md"), readme).unwrap();
        let p = Project::open(file.path()).unwrap();
        assert_eq!(p.features_readme().unwrap_err().code, ErrorCode::PathDenied);

        let inside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(inside.path().join("docs")).unwrap();
        std::fs::create_dir_all(inside.path().join("features")).unwrap();
        std::fs::write(inside.path().join("features/README.md"), "").unwrap();
        let p = Project::open(inside.path()).unwrap();
        assert_eq!(p.docs_root().unwrap(), p.docs_dir());
        assert_eq!(p.features_root().unwrap(), p.features_dir());
        assert_eq!(
            p.features_readme().unwrap(),
            p.features_dir().join("README.md")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_ledger_that_links_out_is_not_read() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("qtquick.toml"), "").unwrap();

        let file = tempfile::tempdir().unwrap();
        std::fs::create_dir(file.path().join("ledger")).unwrap();
        let link = file.path().join("ledger/qtquick.toml");
        std::os::unix::fs::symlink(outside.path().join("qtquick.toml"), link).unwrap();
        let root = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("ledger")).unwrap();
        for dir in [&file, &root] {
            let p = Project::open(dir.path()).unwrap();
            assert_eq!(p.ledger_file().unwrap_err().code, ErrorCode::PathDenied);
        }

        let inside = tempfile::tempdir().unwrap();
        std::fs::create_dir(inside.path().join("ledger")).unwrap();
        std::fs::write(inside.path().join("ledger/qtquick.toml"), "").unwrap();
        let p = Project::open(inside.path()).unwrap();
        assert_eq!(p.ledger_file().unwrap(), p.ledger_path());
    }
}
