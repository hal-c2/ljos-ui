//! The files a compilation sees: their sources, what the front end makes of
//! them, and the runtime's types.

use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use qml_front::{
    TypeOracle,
    discover::{self, Module},
    project::{self, ControlsStyle, join},
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
            for name in names
                .iter()
                .filter(|name| name.ends_with(".js") || name.ends_with(".mjs"))
            {
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
            for module in &modules {
                self.add_module(&dir_key, module);
            }
        }
        Ok(())
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
        if !names.iter().any(|name| name == "CMakeLists.txt") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(dir.join("CMakeLists.txt")) else {
            continue;
        };
        let at = |path: &str| dir.join(path.strip_prefix("./").unwrap_or(path));
        for module in discover::cmake(&text) {
            for file in &module.resources {
                out.push((module.address(file), at(file)));
            }
        }
        for kept in discover::kept(&text) {
            out.push((kept.address, at(&kept.path)));
        }
    }
    out.retain(|(_, path)| path.is_file());
    Ok(out)
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

/// The style Qt's run-time selection gives a program started with these
/// variables: `QT_QUICK_CONTROLS_STYLE`, else `Style` of the `[Controls]`
/// group of the file `QT_QUICK_CONTROLS_CONF` names; the fallback style
/// from `QT_QUICK_CONTROLS_FALLBACK_STYLE` or `FallbackStyle` likewise.
/// A `qtquickcontrols2.conf` beside the files is not read, as `qml` does
/// not read it.
fn controls_style(
    var: impl Fn(&str) -> Option<String>,
    read: impl Fn(&str) -> Option<String>,
) -> Option<ControlsStyle> {
    let conf = var("QT_QUICK_CONTROLS_CONF").and_then(|path| read(&path));
    let given =
        |value: Option<String>| value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    let from_conf = |key: &str| given(conf.as_deref().and_then(|text| ini_value(text, key)));
    let name = given(var("QT_QUICK_CONTROLS_STYLE")).or_else(|| from_conf("Style"))?;
    let fallback =
        given(var("QT_QUICK_CONTROLS_FALLBACK_STYLE")).or_else(|| from_conf("FallbackStyle"));
    Some(ControlsStyle { name, fallback })
}

/// The value of `key` in the `[Controls]` group of an INI text.
fn ini_value(text: &str, key: &str) -> Option<String> {
    let mut in_controls = false;
    for line in text.lines().map(str::trim) {
        if let Some(group) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_controls = group.trim() == "Controls";
        } else if in_controls
            && let Some((k, v)) = line.split_once('=')
            && k.trim().eq_ignore_ascii_case(key)
        {
            return Some(v.trim().trim_matches('"').to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(vars: &[(&str, &str)], conf: &str) -> Option<ControlsStyle> {
        let var = |name: &str| {
            vars.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v).to_owned())
        };
        controls_style(var, |_| Some(conf.to_owned()))
    }

    fn named(name: &str, fallback: Option<&str>) -> Option<ControlsStyle> {
        Some(ControlsStyle {
            name: name.to_owned(),
            fallback: fallback.map(str::to_owned),
        })
    }

    /// The order Qt documents and qml6 shows: the variable, then the file.
    #[test]
    fn the_variable_comes_before_the_configuration_file() {
        let conf = "[Controls]\nStyle=Kit\nFallbackStyle=Material\n\n[Material]\nTheme=Dark\n";
        assert_eq!(style(&[], conf), None);
        let file = ("QT_QUICK_CONTROLS_CONF", "x.conf");
        assert_eq!(style(&[file], conf), named("Kit", Some("Material")));
        assert_eq!(
            style(&[file, ("QT_QUICK_CONTROLS_STYLE", "Basic")], conf),
            named("Basic", Some("Material"))
        );
        assert_eq!(
            style(
                &[
                    ("QT_QUICK_CONTROLS_STYLE", "Kit"),
                    ("QT_QUICK_CONTROLS_FALLBACK_STYLE", "Fusion")
                ],
                ""
            ),
            named("Kit", Some("Fusion"))
        );
        // Another group's `Style` is not the controls'.
        assert_eq!(style(&[file], "[Other]\nStyle=Kit\n"), None);
        assert_eq!(style(&[("QT_QUICK_CONTROLS_STYLE", " ")], ""), None);
    }
}
