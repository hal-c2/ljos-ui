//! The files a compilation sees: their sources, what the front end makes of
//! them, and the runtime's types.

use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use qml_front::{
    TypeOracle,
    discover::{self, Module},
    project::{self, ControlsStyle, is_script, join},
};

use crate::Error;
use crate::style::controls_style;

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
    /// Where each file read from disk is ([`Self::load_roots`]).
    paths: BTreeMap<String, PathBuf>,
    /// The JavaScript files, by their path from the root (`content/calc.js`).
    scripts: BTreeMap<String, String>,
    pub(crate) oracle: O,
    /// Names only the context an instance is made in has at run time. Known
    /// once every file is in.
    dynamic: OnceCell<HashSet<String>>,
    /// Per file, the functions of its root object another file may call
    /// in place. Worked out when first asked for.
    callable: RefCell<HashMap<String, Arc<Vec<String>>>>,
    /// One of those is being worked out: it is not to ask for another.
    summarising: Cell<bool>,
    /// For a project [`open`](Self::open)ed: what is known of the tree
    /// before its files are read.
    tree: Option<Tree>,
}

/// A project's tree as a program loads it: the modules are known from the
/// start, a directory's files once something reaches them.
#[derive(Debug)]
struct Tree {
    root: PathBuf,
    /// What the `qmldir` and `CMakeLists.txt` files describe, with the key
    /// of the directory each is in.
    modules: Vec<(String, Module)>,
    /// The directories each of those has its types in.
    module_dirs: Vec<BTreeSet<String>>,
    /// The directories read, by key.
    read: HashSet<String>,
    /// The modules told to the front end, by their place in `modules`.
    told: HashSet<usize>,
    /// What [`resources`] finds, from the same reading.
    resources: Vec<(String, PathBuf)>,
}

impl<O: TypeOracle> Project<O> {
    pub fn new(oracle: O) -> Self {
        Self {
            front: project::Project::new(),
            sources: BTreeMap::new(),
            paths: BTreeMap::new(),
            scripts: BTreeMap::new(),
            oracle,
            dynamic: OnceCell::new(),
            callable: RefCell::new(HashMap::new()),
            summarising: Cell::new(false),
            tree: None,
        }
    }

    /// The functions of the root object of the file `key` that have a
    /// tier-1 program which writes nothing: what a program of another file
    /// may call in place, when the file is a singleton.
    ///
    /// A function that itself calls one of another file is left out, so
    /// that what this says of a file depends on that file alone and not on
    /// which file was compiled first.
    pub(crate) fn pure_functions(&self, key: &str) -> Arc<Vec<String>> {
        if self.summarising.get() {
            return Arc::default();
        }
        if let Some(known) = self.callable.borrow().get(key) {
            return known.clone();
        }
        self.summarising.set(true);
        let compiled = crate::compile::compile_file(self, key);
        self.summarising.set(false);
        let mut pure: BTreeMap<&str, bool> = BTreeMap::new();
        if let Ok(compiled) = &compiled
            && let Some(root) = compiled
                .file
                .components
                .first()
                .and_then(|component| component.objects.first())
        {
            // In order: the last of a name is the one the object has.
            for function in &root.functions {
                let program = compiled.file.expr(function.expr).tier1.as_ref();
                pure.insert(&function.name, program.is_some_and(|p| !p.writes()));
            }
        }
        let names: Vec<String> = pure
            .into_iter()
            .filter(|(_, pure)| *pure)
            .map(|(name, _)| name.to_string())
            .collect();
        let names = Arc::new(names);
        self.callable
            .borrow_mut()
            .insert(key.to_string(), names.clone());
        names
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
        self.callable.get_mut().clear();
    }

    /// Adds the JavaScript file at `path` (from the root, with its suffix).
    /// A QML file that imports it by path is compiled with it.
    pub fn add_script(&mut self, path: &str, source: impl Into<String>) {
        self.scripts.insert(path.to_string(), source.into());
        self.callable.get_mut().clear();
    }

    /// The JavaScript files: their paths from the root, and their text.
    pub fn scripts(&self) -> impl Iterator<Item = (&str, &str)> {
        self.scripts.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// The text of the JavaScript file at `path`.
    pub fn script(&self, path: &str) -> Option<&str> {
        self.scripts.get(path).map(String::as_str)
    }

    /// Forgets the JavaScript file at `path`.
    pub fn remove_script(&mut self, path: &str) {
        if self.scripts.remove(path).is_some() {
            self.callable.get_mut().clear();
        }
    }

    /// Forgets the file `key`. What a module was said to have stays said.
    pub fn remove(&mut self, key: &str) {
        self.front.remove(key);
        self.sources.remove(key);
        self.paths.remove(key);
        self.dynamic = OnceCell::new();
        self.callable.get_mut().clear();
    }

    /// Says the module `uri` has the type `name`, which is the file `file`.
    pub fn add_type(&mut self, uri: &str, name: &str, file: &str) {
        self.callable.get_mut().clear();
        self.front.add_type(uri, name, file);
    }

    /// What the `qmldir` file of the directory `dir` (a key, empty for the
    /// root) says: the types of the module it names. One that names no
    /// module says nothing a file needs to be told, as the files of a
    /// directory are its types already. Add the files it names first.
    pub fn add_qmldir(&mut self, dir: &str, text: &str) {
        if let Some(module) = discover::qmldir(text) {
            self.add_module(dir, &module);
        }
    }

    fn add_module(&mut self, dir: &str, module: &Module) {
        for ty in &module.types {
            let file = join(dir, &ty.path);
            let file = file.strip_suffix(".qml").unwrap_or(&file);
            if self.front.has(file) {
                self.add_type(&module.uri, &ty.name, file);
            }
        }
    }

    /// Every `.qml` file under `root`, and the modules its `qmldir` and
    /// `CMakeLists.txt` files describe.
    pub fn load(root: &Path, oracle: O) -> Result<Self, Error> {
        Self::load_roots(&[root], oracle)
    }

    /// Every file under each of `roots`, keyed by its path from its own
    /// root, as [`load`](Self::load) reads one: the import path of an
    /// application whose QML is in more than one tree. A file of one root
    /// may import a module another root's `qmldir` describes. Where two
    /// roots have a file of the same key the earlier root's is kept, as
    /// the first import path that has a module is the one used.
    pub fn load_roots(roots: &[&Path], oracle: O) -> Result<Self, Error> {
        let mut project = Self::new(oracle);
        for root in roots {
            project.load_root(root)?;
        }
        // A program chooses its style as it starts, as Qt's does; a
        // scenario or a tool that adds files itself chooses one or none.
        project.front.set_controls_style(controls_style(
            |name| std::env::var(name).ok(),
            |path| std::fs::read_to_string(path).ok(),
        ));
        Ok(project)
    }

    /// Chooses the style behind `import QtQuick.Controls` for the files
    /// compiled from now on.
    pub fn set_controls_style(&mut self, style: Option<ControlsStyle>) {
        self.front.set_controls_style(style);
    }

    fn load_root(&mut self, root: &Path) -> Result<(), Error> {
        let dirs = scan(root).map_err(|source| Error::Io {
            path: root.to_owned(),
            source,
        })?;
        // What earlier roots gave; those keys are not replaced.
        let earlier: HashSet<String> = self.sources.keys().cloned().collect();
        let earlier_scripts: HashSet<String> = self.scripts.keys().cloned().collect();
        for (dir, names) in &dirs {
            for name in names.iter().filter(|name| name.ends_with(".qml")) {
                let path = dir.join(name);
                let (Some(key), Ok(text)) = (key(root, &path), std::fs::read_to_string(&path))
                else {
                    continue;
                };
                if !earlier.contains(&key) {
                    self.add(&key, text);
                    self.paths.insert(key, path);
                }
            }
            for name in names.iter().filter(|name| is_script(name)) {
                let path = dir.join(name);
                let (Some(key), Ok(text)) = (key(root, &path), std::fs::read_to_string(&path))
                else {
                    continue;
                };
                if !earlier_scripts.contains(&key) {
                    self.add_script(&key, text);
                }
            }
        }
        for (dir, names) in &dirs {
            let Some(dir_key) = key(root, dir) else {
                continue;
            };
            for module in &described(dir, names).0 {
                self.add_module(&dir_key, module);
            }
        }
        Ok(())
    }

    /// The project under `root` as a program loads it: no QML file is
    /// read until something [`reach`](Self::reach)es it, so a file
    /// nothing reaches costs nothing. What the `qmldir` and
    /// `CMakeLists.txt` files describe is known from the start, as Qt
    /// knows the modules on its import path, and so are the JavaScript
    /// files, as a script may import one from anywhere.
    pub fn open(root: &Path, oracle: O) -> Result<Self, Error> {
        let dirs = scan(root).map_err(|source| Error::Io {
            path: root.to_owned(),
            source,
        })?;
        let mut project = Self::new(oracle);
        let mut modules = Vec::new();
        let mut resources = Vec::new();
        for (dir, names) in &dirs {
            let Some(dir_key) = key(root, dir) else {
                continue;
            };
            for name in names.iter().filter(|name| is_script(name)) {
                let path = dir.join(name);
                if let (Some(key), Ok(text)) = (key(root, &path), std::fs::read_to_string(&path)) {
                    project.add_script(&key, text);
                }
            }
            let (found, kept) = described(dir, names);
            resources.extend(kept.into_iter().filter(|(_, path)| path.is_file()));
            modules.extend(found.into_iter().map(|m| (dir_key.clone(), m)));
        }
        let module_dirs = modules
            .iter()
            .map(|(dir, module)| {
                let types = module.types.iter();
                // A file beside what describes the module sees its types.
                types
                    .map(|ty| project::directory(&join(dir, &ty.path)).to_owned())
                    .chain(std::iter::once(dir.clone()))
                    .collect()
            })
            .collect();
        project.tree = Some(Tree {
            root: root.to_owned(),
            modules,
            module_dirs,
            read: HashSet::new(),
            told: HashSet::new(),
            resources,
        });
        project.front.set_controls_style(controls_style(
            |name| std::env::var(name).ok(),
            |path| std::fs::read_to_string(path).ok(),
        ));
        Ok(project)
    }

    /// Reads what the file `key` needs to be compiled, in a project
    /// [`open`](Self::open)ed: the files of its directory, which are its
    /// types, and what they import (directories, the project's modules,
    /// the style the controls are), until nothing new is needed. A module
    /// is read whole, as its files see each other. The file itself is
    /// followed again, as a save may have given it an import.
    ///
    /// The keys of the files read. A project read whole reads nothing.
    pub fn reach(&mut self, key: &str) -> Vec<String> {
        let Some(mut tree) = self.tree.take() else {
            return Vec::new();
        };
        let mut read = Vec::new();
        let mut dirs = vec![project::directory(key).to_owned()];
        let mut follow: Vec<String> = Vec::new();
        if self.front.has(key) {
            follow.push(key.to_owned());
        }
        let mut wanted: BTreeSet<usize> = BTreeSet::new();
        loop {
            if let Some(dir) = dirs.pop() {
                // A directory outside the tree is not the project's.
                if dir.starts_with("..") || dir.starts_with('/') || !tree.read.insert(dir.clone()) {
                    continue;
                }
                let new = self.read_dir(&tree.root, &dir);
                follow.extend(new.iter().cloned());
                read.extend(new);
                for (i, at) in tree.module_dirs.iter().enumerate() {
                    if at.contains(&dir) && wanted.insert(i) {
                        dirs.extend(at.iter().cloned());
                    }
                }
                continue;
            }
            let Some(file) = follow.pop() else {
                break;
            };
            let Some(summary) = self.front.summary(&file) else {
                continue;
            };
            let here = project::directory(&file);
            for import in &summary.imports {
                let uri = match &import.source {
                    project::Source::Path(path) if !is_script(path) => {
                        dirs.push(join(here, path));
                        continue;
                    }
                    project::Source::Path(_) => continue,
                    project::Source::Module(uri) => uri.as_str(),
                };
                // `import QtQuick.Controls` is the style's types first.
                let style = self.front.controls_style().map(|s| s.name.as_str());
                let style = style.filter(|_| uri == "QtQuick.Controls");
                for (i, (_, module)) in tree.modules.iter().enumerate() {
                    if (module.uri == uri || Some(module.uri.as_str()) == style) && wanted.insert(i)
                    {
                        dirs.extend(tree.module_dirs[i].iter().cloned());
                    }
                }
            }
        }
        for i in wanted {
            if tree.told.insert(i) {
                let (dir, module) = &tree.modules[i];
                self.add_module(dir, module);
            }
        }
        self.tree = Some(tree);
        read
    }

    /// The files the builds of an [`open`](Self::open)ed project keep
    /// inside the program, as [`resources`] finds them under its root;
    /// none for another project.
    pub fn resources(&self) -> &[(String, PathBuf)] {
        self.tree.as_ref().map_or(&[], |tree| &tree.resources)
    }

    /// Whether the directory `dir` (a key) of an [`open`](Self::open)ed
    /// project was read. Every directory of one read whole was.
    pub fn has_read(&self, dir: &str) -> bool {
        self.tree
            .as_ref()
            .is_none_or(|tree| tree.read.contains(dir))
    }

    /// The QML files directly in the directory `dir` that are not in the
    /// project yet, added: their keys.
    fn read_dir(&mut self, root: &Path, dir: &str) -> Vec<String> {
        let at = if dir.is_empty() {
            root.to_owned()
        } else {
            root.join(dir)
        };
        let Ok(entries) = std::fs::read_dir(&at) else {
            return Vec::new();
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "qml"))
            .collect();
        paths.sort();
        let mut added = Vec::new();
        for path in paths {
            let Some(key) = key(root, &path) else {
                continue;
            };
            if self.sources.contains_key(&key) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            self.add(&key, text);
            self.paths.insert(key.clone(), path);
            added.push(key);
        }
        added
    }

    /// The files, by key, in order.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.sources.keys().map(String::as_str)
    }

    /// The path the file `key` was read from: none for a file
    /// [`add`](Self::add)ed as text.
    pub fn path(&self, key: &str) -> Option<&Path> {
        self.paths.get(key).map(PathBuf::as_path)
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

/// The files a project's builds keep inside the program (`RESOURCES` of a
/// module, `qt_add_resources`), by the address the program names each by
/// (`qrc:/qt/qml/Thermostat/icons/close.svg`) and where each is on disk. A
/// file the build names that is not there is left out.
pub fn resources(root: &Path) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    for (dir, names) in scan(root)? {
        out.extend(described(&dir, &names).1);
    }
    out.retain(|(_, path)| path.is_file());
    Ok(out)
}

/// The modules the `qmldir` and `CMakeLists.txt` of `dir` describe, and
/// the files its `CMakeLists.txt` keeps inside the program.
fn described(dir: &Path, names: &[String]) -> (Vec<Module>, Vec<(String, PathBuf)>) {
    let mut modules: Vec<Module> = Vec::new();
    let mut resources = Vec::new();
    if names.iter().any(|name| name == "qmldir")
        && let Ok(text) = std::fs::read_to_string(dir.join("qmldir"))
    {
        modules.extend(discover::qmldir(&text));
    }
    if names.iter().any(|name| name == "CMakeLists.txt")
        && let Ok(text) = std::fs::read_to_string(dir.join("CMakeLists.txt"))
    {
        let at = |path: &str| dir.join(path.strip_prefix("./").unwrap_or(path));
        let mut built = discover::cmake(&text);
        // A program reads the qmldir, not the build: where both name a type
        // of the same module, the qmldir's file is the type.
        for module in &mut built {
            let uri = &module.uri;
            let named = |name: &str| {
                modules
                    .iter()
                    .filter(|by| &by.uri == uri)
                    .any(|by| by.types.iter().any(|ty| ty.name == name))
            };
            module.types.retain(|ty| !named(&ty.name));
        }
        for module in &built {
            for file in &module.resources {
                resources.push((module.address(file), at(file)));
            }
        }
        for kept in discover::kept(&text) {
            resources.push((kept.address, at(&kept.path)));
        }
        modules.extend(built);
    }
    (modules, resources)
}

/// A file's key: its path from the root without `.qml`, with forward
/// slashes.
fn key(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let text = relative.to_string_lossy().replace('\\', "/");
    Some(text.strip_suffix(".qml").unwrap_or(&text).to_owned())
}

/// Directories a walk of a project does not enter: what tools keep for
/// themselves (a name that starts with a dot) and build output, which
/// holds copies of the sources. Every walk of a project's files (the
/// compiler's, the watcher's, the command line's, the authoring
/// server's) takes this one, so they all see one set of files.
#[must_use]
pub fn skipped_dir(name: &str) -> bool {
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
                Ok(kind) if kind.is_dir() && !skipped_dir(&name) => stack.push(entry.path()),
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
