//! Module aliases: an `import App.Media` answered by the module of a
//! directory of the caller's choosing rather than the one the import path
//! finds, so that an application can run against a stand-in and back.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::project::{described_in, skipped_dir};

/// Why an alias was not taken.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AliasError {
    #[error("{module:?} is not a module name")]
    BadName { module: String },
    #[error("{} is not a directory", dir.display())]
    NotDir { dir: PathBuf },
    #[error("{} holds no module {module}{}", dir.display(), found_text(found))]
    NoModule {
        module: String,
        dir: PathBuf,
        /// The modules it does describe.
        found: Vec<String>,
    },
    #[error(
        "{} is outside the project: an alias names a directory inside it, \
         or one the application was started with",
        dir.display()
    )]
    Outside { dir: PathBuf },
    #[error("an alias is written MODULE=DIR, not {0:?}")]
    Syntax(String),
}

fn found_text(found: &[String]) -> String {
    if found.is_empty() {
        String::new()
    } else {
        format!(" (it describes {})", found.join(", "))
    }
}

/// One module and the directory it is imported from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    module: String,
    dir: PathBuf,
}

impl Alias {
    /// `module` imported from `dir`, which has to describe it, in a
    /// `qmldir` or in a `CMakeLists.txt` whose module has that URI. The
    /// directory is kept with its links followed, as the roots of a
    /// session are.
    pub fn new(module: &str, dir: &Path) -> Result<Self, AliasError> {
        let named = !module.is_empty()
            && module.split('.').all(|part| {
                !part.is_empty() && part.chars().all(|c| c.is_alphanumeric() || c == '_')
            });
        if !named {
            return Err(AliasError::BadName {
                module: module.to_owned(),
            });
        }
        let real = std::fs::canonicalize(dir)
            .ok()
            .filter(|d| d.is_dir())
            .ok_or_else(|| AliasError::NotDir {
                dir: dir.to_owned(),
            })?;
        let modules = described_in(&real);
        if !modules.iter().any(|m| m.uri == module) {
            let mut found: Vec<String> = modules.into_iter().map(|m| m.uri).collect();
            found.dedup();
            return Err(AliasError::NoModule {
                module: module.to_owned(),
                dir: dir.to_owned(),
                found,
            });
        }
        Ok(Self {
            module: module.to_owned(),
            dir: real,
        })
    }

    /// `MODULE=DIR`, as a command line gives one.
    pub fn parse(spec: &str) -> Result<Self, AliasError> {
        let (module, dir) = spec
            .split_once('=')
            .filter(|(m, d)| !m.is_empty() && !d.is_empty())
            .ok_or_else(|| AliasError::Syntax(spec.to_owned()))?;
        Self::new(module, Path::new(dir))
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// Every alias in force, by module.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aliases(BTreeMap<String, PathBuf>);

impl Aliases {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes `alias`, in place of any the module had.
    pub fn set(&mut self, alias: Alias) {
        self.0.insert(alias.module, alias.dir);
    }

    /// Drops the module's alias: true when it had one.
    pub fn remove(&mut self, module: &str) -> bool {
        self.0.remove(module).is_some()
    }

    /// The directory `module` is imported from, when it is aliased.
    pub fn get(&self, module: &str) -> Option<&Path> {
        self.0.get(module).map(PathBuf::as_path)
    }

    /// Each module and its directory, by module.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.0.iter().map(|(m, d)| (m.as_str(), d.as_path()))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromIterator<Alias> for Aliases {
    fn from_iter<I: IntoIterator<Item = Alias>>(iter: I) -> Self {
        let mut out = Self::new();
        for alias in iter {
            out.set(alias);
        }
        out
    }
}

/// A directory outside every root, read under a key of its own: the files
/// of an aliased directory that is not on the import path. The prefix
/// starts with a dot, which no walk of a root enters, so no key of a root
/// can be one of its.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub prefix: String,
    pub dir: PathBuf,
}

impl Mount {
    /// The key of `path`, a file or directory under the mount.
    pub(crate) fn key_of(&self, path: &Path) -> Option<String> {
        let rest = path.strip_prefix(&self.dir).ok()?;
        let rest = rest.to_string_lossy().replace('\\', "/");
        Some(if rest.is_empty() {
            self.prefix.clone()
        } else {
            format!("{}/{rest}", self.prefix)
        })
    }

    /// The path of `key`, when it is one of the mount's.
    pub(crate) fn path_of(&self, key: &str) -> Option<PathBuf> {
        if key == self.prefix {
            return Some(self.dir.clone());
        }
        let rest = key.strip_prefix(&self.prefix)?.strip_prefix('/')?;
        Some(self.dir.join(rest))
    }
}

/// The mounts for the aliases of `aliases` whose directories are under
/// none of `roots` (each with its links followed), named after the module
/// each was given for: `.aliases/App.Media`.
pub fn mounts(roots: &[PathBuf], aliases: &Aliases) -> Vec<Mount> {
    aliases
        .iter()
        .filter(|(_, dir)| inside(roots, dir).is_none())
        .map(|(module, dir)| Mount {
            prefix: format!(".aliases/{module}"),
            dir: dir.to_owned(),
        })
        .collect()
}

/// The root `dir` (with its links followed) is the project's directory
/// under, when no directory between the two is one a walk of the root
/// skips: the root's walk reads it, so its files have keys there.
pub fn inside<'a>(roots: &'a [PathBuf], dir: &Path) -> Option<&'a Path> {
    roots.iter().map(PathBuf::as_path).find(|root| {
        let real = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        dir.strip_prefix(&real).is_ok_and(|rest| {
            rest.components()
                .all(|c| !skipped_dir(&c.as_os_str().to_string_lossy()))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(dir: &Path, text: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("qmldir"), text).unwrap();
    }

    #[test]
    fn a_directory_is_taken_only_with_the_module() {
        let tmp = tempfile::tempdir().unwrap();
        let media = tmp.path().join("mocks/Media");
        module(&media, "module App.Media\nPlayer 1.0 Player.qml\n");
        let other = tmp.path().join("Other");
        module(&other, "module App.Other\n");

        let alias = Alias::new("App.Media", &media).unwrap();
        assert_eq!(alias.dir(), std::fs::canonicalize(&media).unwrap());
        assert_eq!(
            Alias::new("App.Media", &other),
            Err(AliasError::NoModule {
                module: "App.Media".into(),
                dir: other.clone(),
                found: vec!["App.Other".into()],
            })
        );
        assert!(matches!(
            Alias::new("App.Media", &tmp.path().join("gone")),
            Err(AliasError::NotDir { .. })
        ));
        assert!(matches!(
            Alias::new("App..Media", &media),
            Err(AliasError::BadName { .. })
        ));
    }

    #[test]
    fn a_command_line_alias_is_module_equals_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let media = tmp.path().join("Media");
        module(&media, "module App.Media\n");
        let spec = format!("App.Media={}", media.display());
        assert_eq!(Alias::parse(&spec).unwrap().module(), "App.Media");
        assert!(matches!(
            Alias::parse("App.Media"),
            Err(AliasError::Syntax(_))
        ));
        assert!(matches!(Alias::parse("=x"), Err(AliasError::Syntax(_))));
    }

    #[test]
    fn a_directory_outside_the_roots_is_mounted_under_a_dotted_key() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("app");
        let inner = root.join("mocks/Media");
        module(&inner, "module App.Media\n");
        let outer = tmp.path().join("elsewhere/Net");
        module(&outer, "module App.Net\n");
        let hidden = root.join("build/Store");
        module(&hidden, "module App.Store\n");
        let aliases: Aliases = [
            Alias::new("App.Media", &inner).unwrap(),
            Alias::new("App.Net", &outer).unwrap(),
            Alias::new("App.Store", &hidden).unwrap(),
        ]
        .into_iter()
        .collect();
        let found = mounts(&[root], &aliases);
        let prefixes: Vec<&str> = found.iter().map(|m| m.prefix.as_str()).collect();
        // A directory a walk of the root skips is not read with it.
        assert_eq!(prefixes, [".aliases/App.Net", ".aliases/App.Store"]);
        let net = &found[0];
        assert_eq!(
            net.key_of(&net.dir.join("Client.qml")).as_deref(),
            Some(".aliases/App.Net/Client.qml")
        );
        assert_eq!(
            net.path_of(".aliases/App.Net/Client.qml"),
            Some(net.dir.join("Client.qml"))
        );
        assert_eq!(net.path_of(".aliases/App.Network/Client.qml"), None);
    }
}
