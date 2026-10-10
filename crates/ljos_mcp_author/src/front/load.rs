//! Assembling the `qml_front` project a check needs: the files asked for, the
//! files next to them and in the directories they import, and the modules the
//! project's own `qmldir` and `CMakeLists.txt` files describe.
//!
//! A file refers to others by name only (`Button {}` is `Button.qml` beside
//! it), so a check that sees one file would call every such type unknown.
//! Loading is therefore by need, in this order, each step parsing a file at
//! most once.

use std::{
    cell::OnceCell,
    collections::HashSet,
    path::{Path, PathBuf},
};

use qml_front::{
    discover::{self, Module},
    project::{Project, Summary, join},
};

/// A project tree walk stops after this many directories, so a pathological
/// root (a home directory) cannot stall a tool call.
const MAX_DIRS: usize = 20_000;

pub(super) struct Loader<'r> {
    root: &'r Path,
    pub project: Project,
    /// Keys already parsed (or tried), so no file is read twice.
    added: HashSet<String>,
    dirs: HashSet<PathBuf>,
    /// URIs of the modules the project describes itself.
    pub modules: HashSet<String>,
    /// Type names of files that exist but did not parse. A type is not
    /// "unknown" because its file has a syntax error.
    pub broken: HashSet<String>,
    scan: OnceCell<Vec<(PathBuf, Vec<String>)>>,
    /// The capitalised words of the project's C++ files that register QML
    /// types; None when there were too many files to read.
    native: OnceCell<Option<HashSet<String>>>,
}

impl<'r> Loader<'r> {
    pub fn new(root: &'r Path) -> Self {
        Self {
            root,
            project: Project::new(),
            added: HashSet::new(),
            dirs: HashSet::new(),
            modules: HashSet::new(),
            broken: HashSet::new(),
            scan: OnceCell::new(),
            native: OnceCell::new(),
        }
    }

    /// The key a file has in the project: its path from the root without
    /// `.qml`, with forward slashes.
    pub fn key(&self, path: &Path) -> Option<String> {
        let relative = path.strip_prefix(self.root).ok()?;
        let text = relative.to_string_lossy().replace('\\', "/");
        Some(text.strip_suffix(".qml").unwrap_or(&text).to_owned())
    }

    /// Records a file the caller already parsed.
    pub fn insert(&mut self, key: &str, summary: Option<Summary>) {
        if !self.added.insert(key.to_owned()) {
            return;
        }
        match summary {
            Some(summary) => self.project.insert(key, summary),
            None => self.mark_broken(key),
        }
    }

    /// Records a file that did not parse whole. What did parse stays in the
    /// project, so a component built on it still resolves; only a file with
    /// no root type at all is left out, and then it is `broken`.
    pub fn insert_partial(&mut self, key: &str, text: &str) {
        if !self.added.insert(key.to_owned()) {
            return;
        }
        self.keep(key, text);
    }

    fn keep(&mut self, key: &str, text: &str) {
        // The errors are reported for the file asked for, by its own parse.
        let _ = self.project.add(key, text);
        if !self.project.has(key) {
            self.mark_broken(key);
        }
    }

    fn mark_broken(&mut self, key: &str) {
        let stem = key.rsplit('/').next().unwrap_or(key);
        // `Screen.ui` is the type `Screen`.
        self.broken
            .insert(stem.split('.').next().unwrap_or(stem).to_owned());
    }

    fn add_file(&mut self, path: &Path) {
        let Some(key) = self.key(path) else { return };
        if !self.added.insert(key.clone()) {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            self.mark_broken(&key);
            return;
        };
        self.keep(&key, &text);
    }

    /// Every `.qml` file directly in `dir`: they are the types of its
    /// neighbours.
    pub fn add_dir(&mut self, dir: &Path) {
        if !self.dirs.insert(dir.to_owned()) {
            return;
        }
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        let mut files: Vec<PathBuf> = read
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "qml"))
            .collect();
        files.sort();
        for file in files {
            self.add_file(&file);
        }
    }

    /// Follows `import "dir"` into the directories the loaded files import,
    /// until no new one turns up.
    pub fn settle(&mut self) {
        loop {
            let before = self.dirs.len();
            for dir in self.project.directories() {
                let key = join("", &dir);
                // A directory outside the project is not ours to read.
                if key.starts_with("..") || key.starts_with('/') {
                    continue;
                }
                let path = if key.is_empty() {
                    self.root.to_owned()
                } else {
                    self.root.join(&key)
                };
                self.add_dir(&path);
            }
            if self.dirs.len() == before {
                return;
            }
        }
    }

    /// Reads every `qmldir` and `CMakeLists.txt` in the project and makes the
    /// modules they describe known.
    pub fn discover_modules(&mut self) {
        let mut found: Vec<(PathBuf, Module)> = Vec::new();
        for (dir, names) in self.scan() {
            if names.iter().any(|n| n == "qmldir")
                && let Ok(text) = std::fs::read_to_string(dir.join("qmldir"))
                && let Some(module) = discover::qmldir(&text)
            {
                found.push((dir.clone(), module));
            }
            if names.iter().any(|n| n == "CMakeLists.txt")
                && let Ok(text) = std::fs::read_to_string(dir.join("CMakeLists.txt"))
            {
                found.extend(discover::cmake(&text).into_iter().map(|m| (dir.clone(), m)));
            }
        }
        for (dir, module) in found {
            self.module(&dir, module);
        }
    }

    fn module(&mut self, dir: &Path, module: Module) {
        let Some(dir_key) = self.key(dir) else { return };
        self.modules.insert(module.uri.clone());
        for ty in module.types {
            let file_key = join(&dir_key, &ty.path);
            let file_key = file_key.strip_suffix(".qml").unwrap_or(&file_key);
            if file_key.starts_with("..") || file_key.starts_with('/') {
                continue;
            }
            self.add_file(&self.root.join(format!("{file_key}.qml")));
            if self.project.has(file_key) {
                self.project.add_type(&module.uri, &ty.name, file_key);
            }
        }
    }

    /// Loads every `.qml` file of the project. A name that is unresolved in
    /// one component may be what another one provides to it at run time, and
    /// that is only knowable with all of them.
    pub fn add_everything(&mut self) {
        let dirs: Vec<PathBuf> = self.scan().iter().map(|(d, _)| d.clone()).collect();
        for dir in dirs {
            self.add_dir(&dir);
        }
    }

    /// Whether a C++ file of the project may register a QML type called
    /// `name` (`QML_ELEMENT`, `qmlRegisterType`). Such a type is invisible to
    /// a check, which must then not call the name unknown, nor trust a QML
    /// file of the same name to be what is meant.
    pub fn native(&self, name: &str) -> bool {
        self.native
            .get_or_init(|| native_words(self.scan()))
            .as_ref()
            .is_none_or(|words| words.contains(name))
    }

    /// Every directory below the root worth reading, with its file names.
    fn scan(&self) -> &Vec<(PathBuf, Vec<String>)> {
        self.scan.get_or_init(|| scan(self.root))
    }
}

fn skipped(name: &str) -> bool {
    name.starts_with('.')
        || matches!(name, "target" | "node_modules" | "build")
        || name.starts_with("build-")
        || name.starts_with("cmake-build")
}

fn scan(root: &Path) -> Vec<(PathBuf, Vec<String>)> {
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
                Ok(t) if t.is_dir() && !skipped(&name) => stack.push(entry.path()),
                Ok(t) if t.is_file() => names.push(name),
                _ => {}
            }
        }
        out.push((dir, names));
    }
    out.sort();
    out
}

/// Bounds for reading C++: a root that is mostly C++ is not ours to slurp.
const MAX_CPP_FILES: usize = 20_000;
const MAX_CPP_BYTES: u64 = 512 * 1024;

fn native_words(dirs: &[(PathBuf, Vec<String>)]) -> Option<HashSet<String>> {
    let mut words = HashSet::new();
    let mut count = 0;
    for (dir, names) in dirs {
        for name in names {
            let is_cpp = name
                .rsplit_once('.')
                .is_some_and(|(_, ext)| matches!(ext, "h" | "hpp" | "hh" | "cpp" | "cc" | "cxx"));
            if !is_cpp {
                continue;
            }
            count += 1;
            if count > MAX_CPP_FILES {
                return None;
            }
            let path = dir.join(name);
            if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_CPP_BYTES) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !(text.contains("QML_") || text.contains("qmlRegister")) {
                continue;
            }
            words.extend(
                text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .filter(|w| w.starts_with(|c: char| c.is_ascii_uppercase()))
                    .map(str::to_owned),
            );
        }
    }
    Some(words)
}
