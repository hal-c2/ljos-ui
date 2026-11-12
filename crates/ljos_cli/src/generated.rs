//! The generated directory of a project with `.lts` modules
//! (docs/DIALECT.md §4.21): `ljos check`, `ljos types`, `ljos migrate` and
//! `ljos lsp` find the project's root and its `.lts` modules alike, and
//! write `.ljos/` alike. Nothing outside `.ljos/` is written here.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use ljos_compiler::lts::{self, Tsconfig};

/// A project's `.lts` modules and the root its `.ljos/` is under.
pub struct Lts {
    pub root: PathBuf,
    /// Each module's path, absolute, and its key below the root (`/`
    /// between its parts), in order. A module outside the root has no
    /// key and no declaration.
    pub modules: Vec<(PathBuf, Option<String>)>,
}

/// The `.lts` modules below `dirs` (directories, or files whose directory
/// is meant), and the root: `explicit`, else as [`lts::project_root`]
/// finds it. None when there is no `.lts` module.
pub fn find(explicit: Option<&Path>, dirs: &[PathBuf]) -> anyhow::Result<Option<Lts>> {
    let mut found = Vec::new();
    for dir in dirs {
        let dir = if dir.is_dir() {
            dir.clone()
        } else {
            // A bare file name's parent is the empty path, which is here.
            dir.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), Path::to_owned)
        };
        crate::walk::files_below(
            &dir,
            |p| p.extension().is_some_and(|e| e == "lts"),
            &mut found,
        )
        .with_context(|| format!("cannot read {}", dir.display()))?;
    }
    if found.is_empty() {
        return Ok(None);
    }
    let root = lts::project_root(explicit, dirs);
    let root = std::path::absolute(&root).unwrap_or(root);
    let mut modules: Vec<(PathBuf, Option<String>)> = found
        .into_iter()
        .map(|path| {
            let path = std::path::absolute(&path).unwrap_or(path);
            let key = path.strip_prefix(&root).ok().map(|below| {
                below
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            });
            (path, key)
        })
        .collect();
    modules.sort();
    modules.dedup();
    Ok(Some(Lts { root, modules }))
}

impl Lts {
    /// The project `dirs` are of, with no `.lts` module yet: one `ljos
    /// migrate` is about to rename into it.
    pub fn empty(dirs: &[PathBuf]) -> Self {
        let root = lts::project_root(None, dirs);
        let root = std::path::absolute(&root).unwrap_or(root);
        Self {
            root,
            modules: Vec::new(),
        }
    }

    /// Writes `.ljos/` under the root: the base, its `.gitignore`, and the
    /// declaration of each module that has one.
    pub fn write(&self) -> anyhow::Result<lts::Written> {
        let mut declarations = Vec::new();
        for (path, key) in &self.modules {
            let Some(key) = key else { continue };
            let source = std::fs::read_to_string(path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            if let Some(text) = lts::declare(key, &source) {
                declarations.push((key.clone(), text));
            }
        }
        lts::write_generated(&self.root, &declarations)
            .with_context(|| format!("cannot write {}", self.root.join(lts::DIR).display()))
    }

    pub fn tsconfig(&self) -> PathBuf {
        self.root.join("tsconfig.json")
    }

    /// The module a warning names: the first.
    fn first(&self) -> (&Path, String) {
        let (path, key) = &self.modules[0];
        let name = key.clone().unwrap_or_else(|| {
            path.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        });
        (path, name)
    }

    /// What `ljos check` says of the root's `tsconfig.json` (`tsconfig-lts`),
    /// each file named as `shown` names it; None when it extends the base.
    pub fn tsconfig_warning(&self, shown: impl Fn(&Path) -> String) -> Option<String> {
        let path = self.tsconfig();
        let text = std::fs::read_to_string(&path).ok();
        let (module, name) = self.first();
        let tsconfig = shown(&path);
        Some(match lts::tsconfig_state(text.as_deref()) {
            Tsconfig::Fine => return None,
            Tsconfig::Missing => format!(
                "{}:1:1: warning: no `tsconfig.json` extends `.ljos/tsconfig.base.json`, so stock \
                 TypeScript cannot resolve an import of `{name}`; `ljos migrate` writes one \
                 [tsconfig-lts]",
                shown(module)
            ),
            Tsconfig::NoBase => format!(
                "{tsconfig}:1:1: warning: `tsconfig.json` does not extend `{}`, so stock \
                 TypeScript cannot resolve an import of `{name}` [tsconfig-lts]",
                lts::BASE
            ),
            Tsconfig::OwnRootDirs(at) => {
                let (line, column) = line_column(text.as_deref().unwrap_or(""), at);
                format!(
                    "{tsconfig}:{line}:{column}: warning: the `rootDirs` of `tsconfig.json` \
                     replace those of `{}`, so stock TypeScript cannot resolve an import of \
                     `{name}`; `ljos migrate` adds \".\" and \".ljos/types\" to them \
                     [tsconfig-lts]",
                    lts::BASE
                )
            }
        })
    }

    /// Makes the root's `tsconfig.json` extend the base, as `ljos migrate`
    /// does: what it did, one line each, or would do when `write` is false.
    pub fn fix_tsconfig(&self, write: bool) -> anyhow::Result<Vec<String>> {
        let path = self.tsconfig();
        let text = std::fs::read_to_string(&path).ok();
        let Some(fix) = lts::fix_tsconfig(text.as_deref(), "tsconfig.json") else {
            return Ok(Vec::new());
        };
        if write {
            std::fs::write(&path, &fix.text)
                .with_context(|| format!("cannot write {}", path.display()))?;
            Ok(fix.said)
        } else {
            Ok(fix.said.into_iter().map(|line| would(&line)).collect())
        }
    }
}

/// `wrote x` as `would write x`, and so on, for a dry run.
fn would(line: &str) -> String {
    for (did, will) in [
        ("wrote ", "would write "),
        ("extended ", "would extend "),
        ("added ", "would add "),
    ] {
        if let Some(rest) = line.strip_prefix(did) {
            return format!("{will}{rest}");
        }
    }
    line.to_owned()
}

/// `path` as a report names it: below the first of `dirs` it is in, else
/// whole.
pub fn shown(dirs: &[PathBuf], path: &Path) -> String {
    for dir in dirs {
        let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.clone());
        if let Ok(below) = path.strip_prefix(&dir) {
            return below
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
        }
    }
    path.display().to_string()
}

/// The line and column, from 1, of byte `at` of `text`; columns count
/// characters.
fn line_column(text: &str, at: usize) -> (usize, usize) {
    let before = &text[..at.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[start..].chars().count() + 1)
}
