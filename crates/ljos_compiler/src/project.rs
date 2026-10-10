//! The files a compilation sees: their sources, what the front end makes of
//! them, and the runtime's types.

use std::{
    cell::OnceCell,
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

use qml_front::{
    TypeOracle,
    discover::{self, Module},
    project::{self, join},
};

use crate::Error;

/// A project walk stops after this many directories, so a root that is not a
/// project (a home directory) cannot stall a build.
const MAX_DIRS: usize = 20_000;

/// The QML files of one project.
///
/// A file names others by type only (`Button {}` is `Button.qml` beside it,
/// or a type of a module the project describes), so a file is compiled
/// against every file of its project.
#[derive(Debug)]
pub struct Project<O> {
    pub(crate) front: project::Project,
    sources: BTreeMap<String, String>,
    pub(crate) oracle: O,
    /// Names only the context an instance is made in has at run time. Known
    /// once every file is in.
    dynamic: OnceCell<HashSet<String>>,
}

impl<O: TypeOracle> Project<O> {
    pub fn new(oracle: O) -> Self {
        Self {
            front: project::Project::new(),
            sources: BTreeMap::new(),
            oracle,
            dynamic: OnceCell::new(),
        }
    }

    /// Adds the file `key` (its path from the root without `.qml`). A file
    /// with syntax errors is kept for what did parse; only one the parser
    /// gave up on is unknown to the others. What was wrong with it is
    /// [`front`](Self::front)`().diagnostics(key)`.
    pub fn add(&mut self, key: &str, source: impl Into<String>) {
        let source = source.into();
        // The front end keeps what it can; the errors are its to record.
        let _ = self.front.add(key, &source);
        self.sources.insert(key.to_string(), source);
        self.dynamic = OnceCell::new();
    }

    /// Says the module `uri` has the type `name`, which is the file `file`.
    pub fn add_type(&mut self, uri: &str, name: &str, file: &str) {
        self.front.add_type(uri, name, file);
    }

    /// Every `.qml` file under `root`, and the modules its `qmldir` and
    /// `CMakeLists.txt` files describe.
    pub fn load(root: &Path, oracle: O) -> Result<Self, Error> {
        let dirs = scan(root).map_err(|source| Error::Io {
            path: root.to_owned(),
            source,
        })?;
        let mut project = Self::new(oracle);
        for (dir, names) in &dirs {
            for name in names.iter().filter(|name| name.ends_with(".qml")) {
                let path = dir.join(name);
                let (Some(key), Ok(text)) = (key(root, &path), std::fs::read_to_string(&path))
                else {
                    continue;
                };
                project.add(&key, text);
            }
        }
        for (dir, names) in &dirs {
            let mut modules: Vec<Module> = Vec::new();
            if names.iter().any(|name| name == "qmldir")
                && let Ok(text) = std::fs::read_to_string(dir.join("qmldir"))
            {
                modules.extend(discover::qmldir(&text));
            }
            if names.iter().any(|name| name == "CMakeLists.txt")
                && let Ok(text) = std::fs::read_to_string(dir.join("CMakeLists.txt"))
            {
                modules.extend(discover::cmake(&text));
            }
            let Some(dir_key) = key(root, dir) else {
                continue;
            };
            for module in modules {
                for ty in &module.types {
                    let file = join(&dir_key, &ty.path);
                    let file = file.strip_suffix(".qml").unwrap_or(&file);
                    if project.front.has(file) {
                        project.add_type(&module.uri, &ty.name, file);
                    }
                }
            }
        }
        Ok(project)
    }

    /// The files, by key, in order.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.sources.keys().map(String::as_str)
    }

    pub fn source(&self, key: &str) -> Option<&str> {
        self.sources.get(key).map(String::as_str)
    }

    pub fn oracle(&self) -> &O {
        &self.oracle
    }

    /// What the front end makes of the files.
    pub fn front(&self) -> &project::Project {
        &self.front
    }

    pub(crate) fn dynamic_names(&self) -> &HashSet<String> {
        self.dynamic
            .get_or_init(|| self.front.dynamic_names(&self.oracle))
    }
}

/// A file's key: its path from the root without `.qml`, with forward
/// slashes.
fn key(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let text = relative.to_string_lossy().replace('\\', "/");
    Some(text.strip_suffix(".qml").unwrap_or(&text).to_owned())
}

fn skipped(name: &str) -> bool {
    name.starts_with('.')
        || matches!(name, "target" | "node_modules" | "build")
        || name.starts_with("build-")
        || name.starts_with("cmake-build")
}

/// Every directory under the root worth reading, with its file names, in
/// order.
fn scan(root: &Path) -> std::io::Result<Vec<(PathBuf, Vec<String>)>> {
    // The root itself has to be there; what is below is read as it can be.
    std::fs::read_dir(root)?;
    let mut out = Vec::new();
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        if out.len() >= MAX_DIRS {
            break;
        }
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut names = Vec::new();
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            // `file_type` does not follow links, so a link cannot lead the
            // walk out of the project.
            match entry.file_type() {
                Ok(kind) if kind.is_dir() && !skipped(&name) => stack.push(entry.path()),
                Ok(kind) if kind.is_file() => names.push(name),
                _ => {}
            }
        }
        names.sort();
        out.push((dir, names));
    }
    out.sort();
    Ok(out)
}
