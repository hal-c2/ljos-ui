//! Several projects as one to the compat pass: files of more than one tree
//! lowered in one [`qml_compat::Lowering`], so that a file of one tree
//! that imports a directory of another is lowered with the files it names
//! there (`ljos migrate`).
//!
//! The lowering keys each file by its absolute path ([`file_key`]), as a
//! file of one tree names a file of another by its path. Each project
//! keys its files from its own root, and is asked in its keys.

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

use qml_front::TypeOracle;

use crate::Project;

/// The projects of several roots, with the root each file is read with.
pub struct Roots<O> {
    roots: Vec<(PathBuf, Project<O>)>,
    /// Per file, by its key, its root and its key in that root's project.
    of: BTreeMap<String, (usize, String)>,
}

impl<O> std::fmt::Debug for Roots<O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Roots")
            .field(
                "roots",
                &self.roots.iter().map(|(root, _)| root).collect::<Vec<_>>(),
            )
            .field("files", &self.of.len())
            .finish()
    }
}

impl<O> Default for Roots<O> {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            of: BTreeMap::new(),
        }
    }
}

impl<O: TypeOracle> Roots<O> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `project`, whose files are under `root` (an absolute path), as
    /// the next root: the first is root 0.
    pub fn push(&mut self, root: PathBuf, project: Project<O>) {
        self.roots.push((root, project));
    }

    /// Says the file `file` is read with the project of root `at`, and
    /// gives its key.
    pub fn place(&mut self, file: &Path, at: usize) -> String {
        let key = file_key(file);
        let in_project = key_in(&self.roots[at].0, &absolute(file));
        self.of.insert(key.clone(), (at, in_project));
        key
    }

    /// The root the file `key` is read with.
    pub fn root_of(&self, key: &str) -> Option<usize> {
        self.of.get(key).map(|(at, _)| *at)
    }

    /// The project the file `key` is read with, to change, and the file's
    /// key there: `ljos migrate` puts the lowered files in to compile them as
    /// they will be written.
    pub fn project_mut(&mut self, key: &str) -> Option<(&mut Project<O>, &str)> {
        let (at, in_project) = self.of.get(key)?;
        Some((&mut self.roots[*at].1, in_project))
    }

    /// The root file `key` is read with, its project and its key there.
    fn of(&self, key: &str) -> (&Path, &Project<O>, &str) {
        let (at, in_project) = &self.of[key];
        let (root, project) = &self.roots[*at];
        (root, project, in_project)
    }
}

impl<O: TypeOracle> qml_compat::Project for Roots<O> {
    fn locals<'a>(
        &'a self,
        key: &'a str,
        held: &'a dyn qml_compat::Held,
    ) -> Box<dyn qml_compat::Locals + 'a> {
        let (root, project, in_project) = self.of(key);
        Box::new(project.around().locals_of(
            in_project,
            Box::new(InProject {
                held,
                project,
                root,
            }),
        ))
    }

    fn module_files(&self, key: &str, uri: &str) -> Vec<String> {
        let (root, project, _) = self.of(key);
        project
            .front()
            .module_files(uri)
            .map(|used| file_key(&root.join(format!("{used}.qml"))))
            .collect()
    }

    fn script(&self, key: &str, path: &str) -> Option<qml_compat::ProjectScript<'_>> {
        let (_, project, in_project) = self.of(key);
        let path = beside(key, in_project, path);
        let text = project.script(&path)?;
        Some(qml_compat::ProjectScript { path, text })
    }
}

/// The files the lowering holds, as a project names them. One the project
/// has another text of (a variant its selectors chose) is read as the
/// project has it, as a run of the project reads it.
struct InProject<'a, O> {
    held: &'a dyn qml_compat::Held,
    project: &'a Project<O>,
    root: &'a Path,
}

impl<O: TypeOracle> qml_compat::Held for InProject<'_, O> {
    fn text(&self, key: &str) -> Option<&str> {
        let text = self.held.text(&self.key(key))?;
        (self.project.source(key) == Some(text)).then_some(text)
    }

    fn root(&self, key: &str) -> Option<Option<qml_compat::Root>> {
        self.text(key)?;
        self.held.root(&self.key(key))
    }
}

impl<O> InProject<'_, O> {
    fn key(&self, key: &str) -> String {
        file_key(&self.root.join(format!("{key}.qml")))
    }
}

/// `path`, a path beside the file `key`, from the root of the project in
/// which the file is `in_project`.
fn beside(key: &str, in_project: &str, path: &str) -> String {
    let dir = key.rsplit_once('/').map_or("", |(dir, _)| dir);
    let name = path
        .strip_prefix(dir)
        .and_then(|name| name.strip_prefix('/'))
        .unwrap_or(path);
    match in_project.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{name}"),
        None => name.to_string(),
    }
}

/// `path` from the root of the file system, with `.` and `..` taken out
/// by its words alone, as the path a person wrote names it.
pub fn absolute(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            part => out.push(part),
        }
    }
    out
}

/// A file's key in [`Roots`]: its absolute path without `.qml`, with `/`
/// between the parts on every system.
pub fn file_key(file: &Path) -> String {
    let path = absolute(file).with_extension("");
    let parts: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let joined = parts.join("/");
    // A root `/` is a part of its own.
    joined
        .strip_prefix("//")
        .map_or(joined.clone(), |rest| format!("/{rest}"))
}

/// The key of `file` in the project under `root`: its path from there
/// without `.qml`, with `/` between the parts.
fn key_in(root: &Path, file: &Path) -> String {
    let path = file.strip_prefix(root).unwrap_or(file).with_extension("");
    let parts: Vec<String> = path
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.join("/")
}
