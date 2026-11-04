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
use crate::alias::{Aliases, Mount};
use crate::style::{controls_style, selectors};

/// The key of the file `key` is a variant of: `dir/+Material/App` of
/// `dir/App`. None for a file in no selector's directory. A selector's
/// directory counts just above the file, as only there is it one.
pub fn plain_of(key: &str) -> Option<String> {
    let (dirs, name) = key.rsplit_once('/')?;
    let mut parts: Vec<&str> = dirs.split('/').collect();
    let before = parts.len();
    while parts.last().is_some_and(|part| part.starts_with('+')) {
        parts.pop();
    }
    (parts.len() < before).then(|| {
        parts.push(name);
        parts.join("/")
    })
}

/// The variant of `dir/name` the selectors choose, the earliest first; a
/// directory of a later one may be inside it.
fn select(
    variants: &BTreeMap<String, String>,
    dir: &str,
    name: &str,
    selectors: &[&str],
) -> Option<String> {
    selectors.iter().enumerate().find_map(|(i, selector)| {
        let sub = join(dir, &format!("+{selector}"));
        select(variants, &sub, name, &selectors[i + 1..]).or_else(|| {
            let key = join(&sub, name);
            variants.contains_key(&key).then_some(key)
        })
    })
}

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
    /// The files in `+selector` directories, by key ([`plain_of`]). None is
    /// a file of the project: one is only ever the text of its plain file.
    variants: BTreeMap<String, String>,
    /// Per plain file with a variant in its place, that variant's key.
    chosen: BTreeMap<String, String>,
    /// The text of each plain file a variant is in the place of.
    displaced: BTreeMap<String, String>,
    /// Where each file read from disk is ([`Self::load_roots`]).
    paths: BTreeMap<String, PathBuf>,
    /// The JavaScript files, by their path from the root (`content/calc.js`).
    scripts: BTreeMap<String, String>,
    pub(crate) oracle: O,
    /// The modules told to the front end that are built with C++, with its
    /// files: where a name nothing here has may have been meant to come from.
    native: BTreeMap<String, Vec<String>>,
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
    /// Qt's QML is lowered to Ljós's API as each file comes in
    /// ([`set_compat`](Self::set_compat)).
    compat: bool,
    /// Per file the compat pass was given: the text as written, what the
    /// pass made of it alone, and what it made of it with its group.
    written: BTreeMap<String, String>,
    alone: BTreeMap<String, qml_compat::Alone>,
    lowered: BTreeMap<String, qml_compat::Outcome>,
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
    /// Whether each of `modules` is one an import of its URI sees
    /// ([`counted`]).
    counted: Vec<bool>,
    /// The aliased directories outside the root, read under their keys.
    mounts: Vec<Mount>,
    /// The modules told to the front end, by their place in `modules`.
    told: HashSet<usize>,
    /// What [`resources`] finds, from the same reading.
    resources: Vec<(String, PathBuf)>,
    /// What [`resource_dirs`] finds, from the same reading.
    resource_dirs: Vec<(String, PathBuf)>,
}

impl<O: TypeOracle> Project<O> {
    pub fn new(oracle: O) -> Self {
        Self {
            front: project::Project::new(),
            sources: BTreeMap::new(),
            variants: BTreeMap::new(),
            chosen: BTreeMap::new(),
            displaced: BTreeMap::new(),
            paths: BTreeMap::new(),
            scripts: BTreeMap::new(),
            oracle,
            native: BTreeMap::new(),
            dynamic: OnceCell::new(),
            callable: RefCell::new(HashMap::new()),
            summarising: Cell::new(false),
            tree: None,
            compat: false,
            written: BTreeMap::new(),
            alone: BTreeMap::new(),
            lowered: BTreeMap::new(),
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
        if let Some(plain) = plain_of(key) {
            self.variants.insert(key.to_string(), source);
            self.settle(&plain);
        } else if self.chosen.contains_key(key) {
            self.displaced.insert(key.to_string(), source);
        } else {
            self.put(key, source);
        }
    }

    /// Lowers each file written against Qt's API to Ljós's as it comes in,
    /// those already in too (docs/adr/0004-compat-lowering-before-the-ir.md).
    /// What the front end, the compiler and the IR see of such a file is
    /// its lowered text; [`written`](Self::written) is the file as it is.
    /// Files that name one another are lowered together or not at all
    /// ([`qml_compat::group`]). Set it once the files are in: each file
    /// put while it is on decides its group again.
    pub fn set_compat(&mut self, lower: bool) {
        if self.compat == lower {
            return;
        }
        self.compat = lower;
        if !lower {
            for (key, written) in std::mem::take(&mut self.written) {
                self.put_text(&key, written);
            }
            self.alone.clear();
            self.lowered.clear();
            return;
        }
        let keys: Vec<String> = self.sources.keys().cloned().collect();
        for key in &keys {
            let text = self.sources[key].clone();
            self.written.insert(key.clone(), text);
        }
        for key in &keys {
            self.lower_alone(key);
        }
        self.settle_compat();
    }

    /// What the compat pass made of the file `key`, with the files it is
    /// lowered together with: none when it was not given the file
    /// ([`set_compat`](Self::set_compat)).
    pub fn lowering(&self, key: &str) -> Option<&qml_compat::Outcome> {
        self.lowered.get(key)
    }

    /// The file `key` as it is written, which [`source`](Self::source) is
    /// but for a file the compat pass lowered.
    pub fn written(&self, key: &str) -> Option<&str> {
        match self.written.get(key) {
            Some(written) => Some(written),
            None => self.source(key),
        }
    }

    /// What the pass makes of the written file `key` alone.
    fn lower_alone(&mut self, key: &str) {
        let Some(text) = self.written.get(key) else {
            return;
        };
        let outcome = qml_compat::lower(text, &|name: &str| self.root_beside(key, name));
        let references = qml_compat::references(text);
        self.alone.insert(
            key.to_string(),
            qml_compat::Alone {
                outcome,
                references,
            },
        );
    }

    /// Gives the front end each file as its group is lowered, where that
    /// changed.
    fn settle_compat(&mut self) {
        for (key, outcome) in qml_compat::group(&self.alone) {
            let text = match &outcome {
                qml_compat::Outcome::Lowered(lowered) => Some(lowered.text.clone()),
                qml_compat::Outcome::Unchanged | qml_compat::Outcome::Kept(_) => {
                    self.written.get(&key).cloned()
                }
            };
            if let Some(text) = text
                && self.sources.get(&key) != Some(&text)
            {
                self.put_text(&key, text);
            }
            self.lowered.insert(key, outcome);
        }
    }

    /// The type the root object of the file `name` beside the file `key` is
    /// written as, from its text if it is in, else from the disk: the pass
    /// lowers one file of a directory before it has the others.
    fn root_beside(&self, key: &str, name: &str) -> Option<qml_compat::Root> {
        let dir = key.rsplit_once('/').map(|(dir, _)| dir);
        let sibling = dir.map_or_else(|| name.to_string(), |dir| format!("{dir}/{name}"));
        if let Some(text) = self.written(&sibling) {
            return qml_compat::root(text);
        }
        let path = match (self.paths.get(key), &self.tree) {
            (Some(path), _) => path.parent()?.join(format!("{name}.qml")),
            (None, Some(tree)) => tree.root.join(format!("{sibling}.qml")),
            (None, None) => return None,
        };
        let text = std::fs::read_to_string(path).ok()?;
        qml_compat::root(&text)
    }

    fn put(&mut self, key: &str, source: String) {
        if !self.compat {
            self.put_text(key, source);
            return;
        }
        self.written.insert(key.to_string(), source);
        self.lower_alone(key);
        self.lower_naming(key);
        self.settle_compat();
    }

    /// Lowers again, alone, each file beside `key` that names it: the pass
    /// read the root type of `key` for it, which is now another or none.
    fn lower_naming(&mut self, key: &str) {
        let (dir, name) = key.rsplit_once('/').unwrap_or(("", key));
        let naming: Vec<String> = self
            .alone
            .iter()
            .filter(|(other, alone)| {
                other.as_str() != key
                    && other.rsplit_once('/').map_or("", |(d, _)| d) == dir
                    && alone.references.types.iter().any(|t| t == name)
            })
            .map(|(other, _)| other.clone())
            .collect();
        for other in &naming {
            self.lower_alone(other);
        }
    }

    fn put_text(&mut self, key: &str, source: String) {
        // The front end keeps what it can; the errors are its to record.
        let _ = self.front.add(key, &source);
        self.sources.insert(key.to_string(), source);
        self.dynamic = OnceCell::new();
        self.callable.get_mut().clear();
    }

    /// Puts in the place of the file `plain` what the running selectors
    /// choose: the file itself where they choose no variant.
    fn settle(&mut self, plain: &str) {
        let selectors = selectors(self.front.controls_style());
        let selectors: Vec<&str> = selectors.iter().map(String::as_str).collect();
        let (dir, name) = plain.rsplit_once('/').unwrap_or(("", plain));
        let variant = select(&self.variants, dir, name, &selectors);
        match variant {
            Some(variant) => {
                if !self.chosen.contains_key(plain)
                    && let Some(text) = self.sources.remove(plain)
                {
                    // As written: it is lowered again when it is put back.
                    let text = self.written.get(plain).cloned().unwrap_or(text);
                    self.displaced.insert(plain.to_string(), text);
                }
                let text = self.variants[&variant].clone();
                self.chosen.insert(plain.to_string(), variant);
                self.put(plain, text);
            }
            None if self.chosen.remove(plain).is_some() => match self.displaced.remove(plain) {
                Some(text) => self.put(plain, text),
                None => self.forget(plain),
            },
            None => {}
        }
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

    /// The keys of the JavaScript files that the files and the scripts of
    /// the project import and it does not have. What is added for them may
    /// import others, so this is asked again after they are added.
    pub fn unread_scripts(&self) -> Vec<String> {
        let imported = self.scripts.iter().flat_map(|(key, text)| {
            crate::script::lower("", key, text)
                .imports
                .into_iter()
                .map(|import| import.key)
        });
        let wanted: BTreeSet<String> = self
            .front
            .imported_scripts()
            .into_iter()
            .chain(imported)
            .collect();
        wanted
            .into_iter()
            .filter(|key| !self.scripts.contains_key(key))
            .collect()
    }

    /// Forgets the JavaScript file at `path`.
    pub fn remove_script(&mut self, path: &str) {
        if self.scripts.remove(path).is_some() {
            self.callable.get_mut().clear();
        }
    }

    /// Forgets the file `key`. What a module was said to have stays said.
    pub fn remove(&mut self, key: &str) {
        if let Some(plain) = plain_of(key) {
            self.variants.remove(key);
            self.paths.remove(key);
            self.settle(&plain);
            return;
        }
        if self.chosen.contains_key(key) {
            self.displaced.remove(key);
            self.paths.remove(key);
            return;
        }
        self.forget(key);
    }

    fn forget(&mut self, key: &str) {
        self.front.remove(key);
        self.sources.remove(key);
        self.written.remove(key);
        self.paths.remove(key);
        if self.alone.remove(key).is_some() {
            self.lowered.remove(key);
            self.lower_naming(key);
            self.settle_compat();
        }
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

    /// What to add to the report of a name nothing here has, in a project
    /// with modules whose C++ does not run here.
    pub(crate) fn native_note(&self) -> Option<String> {
        let notes: Vec<String> = self
            .native
            .iter()
            .map(|(uri, files)| {
                format!(
                    "module {uri} is built with C++ that does not run here ({}): \
                     give it a stand-in with --alias {uri}=DIR, or a Rust module",
                    files.join(", ")
                )
            })
            .collect();
        (!notes.is_empty()).then(|| notes.join("; "))
    }

    fn add_module(&mut self, dir: &str, module: &Module) {
        if !module.native.is_empty() {
            self.native
                .insert(module.uri.clone(), module.native.clone());
        }
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
        Self::load_roots_aliased(roots, &Aliases::new(), &[], oracle)
    }

    /// As [`load_roots`](Self::load_roots), with each module of `aliases`
    /// imported from its directory alone. `mounts` are the directories
    /// outside the roots an alias may name; the ones `aliases` names are
    /// read under their keys.
    pub fn load_roots_aliased(
        roots: &[&Path],
        aliases: &Aliases,
        mounts: &[Mount],
        oracle: O,
    ) -> Result<Self, Error> {
        let mut project = Self::new(oracle);
        let mut found = Vec::new();
        for root in roots {
            project.load_root(root, "", &mut found)?;
        }
        for mount in active(mounts, aliases) {
            project.load_root(&mount.dir, &mount.prefix, &mut found)?;
        }
        // Every file is in first: a module of one root may name a file
        // another root has.
        for ((dir, _, module), counted) in found.iter().zip(counted(&found, aliases)) {
            if counted {
                project.add_module(dir, module);
            }
        }
        // A program chooses its style as it starts, as Qt's does; a
        // scenario or a tool that adds files itself chooses one or none.
        project.front.set_controls_style(controls_style(
            |name| std::env::var(name).ok(),
            |path| std::fs::read_to_string(path).ok(),
        ));
        project.settle_all();
        Ok(project)
    }

    /// Chooses the style behind `import QtQuick.Controls` for the files
    /// compiled from now on.
    pub fn set_controls_style(&mut self, style: Option<ControlsStyle>) {
        self.front.set_controls_style(style);
        self.settle_all();
    }

    fn settle_all(&mut self) {
        let plain: BTreeSet<String> = self.variants.keys().filter_map(|k| plain_of(k)).collect();
        for plain in plain {
            self.settle(&plain);
        }
    }

    /// Says the files of the directory `dir` on disk are added under the
    /// key `prefix` (`""` for a root), so that an import naming it, or a
    /// directory below it, by its `file:` URL finds them. A root loaded or
    /// opened is said already.
    pub fn add_place(&mut self, dir: &Path, prefix: &str) {
        // The URL a file's own `Qt.resolvedUrl` makes has the absolute
        // path with `.` and `..` taken out, and a link left as it is;
        // the path with the links followed is the other way to write it.
        let absolute = std::path::absolute(dir).unwrap_or_else(|_| dir.to_owned());
        let mut clean = PathBuf::new();
        for part in absolute.components() {
            match part {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    clean.pop();
                }
                part => clean.push(part),
            }
        }
        self.front.add_place(&clean.to_string_lossy(), prefix);
        if let Ok(real) = clean.canonicalize() {
            self.front.add_place(&real.to_string_lossy(), prefix);
        }
    }

    /// Reads the files under `root`, keyed below `prefix`, and adds the
    /// modules its directories describe to `found`.
    fn load_root(
        &mut self,
        root: &Path,
        prefix: &str,
        found: &mut Vec<(String, PathBuf, Module)>,
    ) -> Result<(), Error> {
        let dirs = scan(root).map_err(|source| Error::Io {
            path: root.to_owned(),
            source,
        })?;
        self.add_place(root, prefix);
        let key = |path: &Path| key_under(root, prefix, path);
        // What earlier roots gave; those keys are not replaced.
        let earlier: HashSet<String> = self
            .sources
            .keys()
            .chain(self.variants.keys())
            .cloned()
            .collect();
        let earlier_scripts: HashSet<String> = self.scripts.keys().cloned().collect();
        for (dir, names) in &dirs {
            for name in names.iter().filter(|name| name.ends_with(".qml")) {
                let path = dir.join(name);
                let (Some(key), Ok(text)) = (key(&path), std::fs::read_to_string(&path)) else {
                    continue;
                };
                if !earlier.contains(&key) {
                    self.add(&key, text);
                    self.paths.insert(key, path);
                }
            }
            for name in names.iter().filter(|name| is_script(name)) {
                let path = dir.join(name);
                let (Some(key), Ok(text)) = (key(&path), std::fs::read_to_string(&path)) else {
                    continue;
                };
                if !earlier_scripts.contains(&key) {
                    self.add_script(&key, text);
                }
            }
        }
        for (dir, names) in &dirs {
            let Some(dir_key) = key(dir) else {
                continue;
            };
            for module in described(dir, names).0 {
                found.push((dir_key.clone(), dir.clone(), module));
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
        Self::open_aliased(root, &Aliases::new(), &[], oracle)
    }

    /// As [`open`](Self::open), with each module of `aliases` imported
    /// from its directory alone. `mounts` are the directories outside the
    /// root an alias may name; the ones `aliases` names are read, as the
    /// root is, under their keys.
    pub fn open_aliased(
        root: &Path,
        aliases: &Aliases,
        mounts: &[Mount],
        oracle: O,
    ) -> Result<Self, Error> {
        let mounts: Vec<Mount> = active(mounts, aliases).cloned().collect();
        let mut project = Self::new(oracle);
        let mut found = Vec::new();
        let mut resources = Vec::new();
        let mut resource_dirs = Vec::new();
        let places = std::iter::once((root, ""))
            .chain(mounts.iter().map(|m| (m.dir.as_path(), m.prefix.as_str())));
        for (base, prefix) in places {
            let dirs = scan(base).map_err(|source| Error::Io {
                path: base.to_owned(),
                source,
            })?;
            project.add_place(base, prefix);
            for (dir, names) in &dirs {
                let Some(dir_key) = key_under(base, prefix, dir) else {
                    continue;
                };
                for name in names.iter().filter(|name| is_script(name)) {
                    let path = dir.join(name);
                    if let (Some(key), Ok(text)) = (
                        key_under(base, prefix, &path),
                        std::fs::read_to_string(&path),
                    ) {
                        project.add_script(&key, text);
                    }
                }
                let (described, kept) = described(dir, names);
                resources.extend(kept.into_iter().filter(|(_, path)| path.is_file()));
                resource_dirs.extend(described.iter().map(|m| (m.address(""), dir.clone())));
                found.extend(
                    described
                        .into_iter()
                        .map(|m| (dir_key.clone(), dir.clone(), m)),
                );
            }
        }
        let counted = counted(&found, aliases);
        let modules: Vec<(String, Module)> = found
            .into_iter()
            .map(|(dir, _, module)| (dir, module))
            .collect();
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
            counted,
            mounts,
            read: HashSet::new(),
            told: HashSet::new(),
            resources,
            resource_dirs,
        });
        project.front.set_controls_style(controls_style(
            |name| std::env::var(name).ok(),
            |path| std::fs::read_to_string(path).ok(),
        ));
        project.settle_all();
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
                let new = self.read_dir(&tree.root, &tree.mounts, &dir);
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
            for import in &summary.imports {
                let uri = match &import.source {
                    project::Source::Path(path) if !is_script(path) => {
                        dirs.push(self.front.import_dir(&file, path));
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
            // A module of the URI that an import does not see is read, as
            // its directory is, but its types are not the URI's.
            if tree.counted[i] && tree.told.insert(i) {
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

    /// Each module of an [`open`](Self::open)ed project by the address its
    /// files are under, with the directory they are in, as
    /// [`resource_dirs`] finds them.
    pub fn resource_dirs(&self) -> &[(String, PathBuf)] {
        self.tree.as_ref().map_or(&[], |tree| &tree.resource_dirs)
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
    fn read_dir(&mut self, root: &Path, mounts: &[Mount], dir: &str) -> Vec<String> {
        let mount = mounts.iter().find(|m| m.path_of(dir).is_some());
        let (base, prefix) = mount.map_or((root, ""), |m| (m.dir.as_path(), m.prefix.as_str()));
        let at = match mount {
            Some(m) => m.path_of(dir).unwrap_or_else(|| m.dir.clone()),
            None if dir.is_empty() => root.to_owned(),
            None => root.join(dir),
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
            let Some(key) = key_under(base, prefix, &path) else {
                continue;
            };
            if self.sources.contains_key(&key) || self.variants.contains_key(&key) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            self.add(&key, text);
            self.paths.insert(key.clone(), path);
            added.push(key);
        }
        // What is in a selector's directory is the place of a file here, so
        // it comes with them; `added` names the plain file, which is what
        // anything follows.
        let mut selected: Vec<PathBuf> = std::fs::read_dir(&at)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| e.file_name().to_string_lossy().starts_with('+'))
            .map(|e| e.path())
            .collect();
        selected.sort();
        for sub in selected {
            let Some(sub_key) = key_under(base, prefix, &sub) else {
                continue;
            };
            let new = self.read_dir(root, mounts, &sub_key);
            added.extend(new.iter().filter_map(|key| plain_of(key)));
        }
        added
    }

    /// The files in `+selector` directories: their keys and their text.
    pub fn variants(&self) -> impl Iterator<Item = (&str, &str)> {
        self.variants.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// The files, by key, in order.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.sources.keys().map(String::as_str)
    }

    /// The path the file `key` was read from: none for a file
    /// [`add`](Self::add)ed as text.
    pub fn path(&self, key: &str) -> Option<&Path> {
        let key = self.chosen.get(key).map_or(key, String::as_str);
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

/// Each module under `root` by the address a program names its files under
/// (`qrc:/qt/qml/<uri as a path>`, or the build's `RESOURCE_PREFIX`), with
/// the directory that describes it: a file there is at that address whether
/// or not the build lists it.
pub fn resource_dirs(root: &Path) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    for (dir, names) in scan(root)? {
        out.extend(
            described(&dir, &names)
                .0
                .iter()
                .map(|m| (m.address(""), dir.clone())),
        );
    }
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

/// The modules the `qmldir` and `CMakeLists.txt` of `dir` describe.
pub(crate) fn described_in(dir: &Path) -> Vec<Module> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .map(|read| {
            read.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    described(dir, &names).0
}

/// The mounts of `mounts` an alias of `aliases` names.
fn active<'a>(mounts: &'a [Mount], aliases: &'a Aliases) -> impl Iterator<Item = &'a Mount> {
    mounts
        .iter()
        .filter(|m| aliases.iter().any(|(_, dir)| dir == m.dir))
}

/// Which of the modules `found` describes (each with the key and the path
/// of its directory, in the order the roots are read) an import of its URI
/// sees.
///
/// An aliased URI is the module of its directory alone. Otherwise, where
/// a directory is at the URI's own path from a root (`App/Media` for
/// `App.Media`), that is where an import path finds the module, and
/// another directory that names the same URI (a stand-in kept beside the
/// real one) is not seen. A directory there that describes no QML type
/// (a module of native code, whose build names only its sources) has
/// nothing for an import to find, and hides nothing. With no such
/// directory every description counts.
fn counted(found: &[(String, PathBuf, Module)], aliases: &Aliases) -> Vec<bool> {
    let home = |uri: &str| uri.replace('.', "/");
    let at_home: HashSet<&str> = found
        .iter()
        .filter(|(dir, _, m)| *dir == home(&m.uri) && !m.types.is_empty())
        .map(|(_, _, m)| m.uri.as_str())
        .collect();
    let mut real: HashMap<&Path, Option<PathBuf>> = HashMap::new();
    found
        .iter()
        .map(|(dir_key, dir, m)| match aliases.get(&m.uri) {
            Some(target) => {
                real.entry(dir.as_path())
                    .or_insert_with(|| std::fs::canonicalize(dir).ok())
                    .as_deref()
                    == Some(target)
            }
            None if at_home.contains(m.uri.as_str()) => *dir_key == home(&m.uri),
            None => true,
        })
        .collect()
}

/// A file's key: its path from `root` without `.qml`, with forward
/// slashes, below `prefix` (`""` for a root of the import path).
fn key_under(root: &Path, prefix: &str, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let text = relative.to_string_lossy().replace('\\', "/");
    let text = text.strip_suffix(".qml").unwrap_or(&text);
    Some(match (prefix, text) {
        ("", _) => text.to_owned(),
        (_, "") => prefix.to_owned(),
        _ => format!("{prefix}/{text}"),
    })
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
