//! `qmldir`: what a module is called, where its type descriptions are, which
//! modules come with it and which of its types are QML files.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use crate::model::{Import, ImportKind};

#[derive(Debug, Default)]
pub struct Qmldir {
    pub module: String,
    /// `typeinfo`: the `.qmltypes` files, relative to the directory.
    pub typeinfo: Vec<String>,
    pub imports: Vec<Import>,
    /// `depends`: modules the descriptions refer to but do not bring along.
    pub depends: Vec<String>,
    /// `Name 6.0 Name.qml`, by name. Of several versions the newest is kept.
    pub files: BTreeMap<String, FileType>,
}

#[derive(Debug)]
pub struct FileType {
    pub file: String,
    pub is_singleton: bool,
    pub version: (u32, u32),
}

pub fn parse(source: &str) -> Qmldir {
    let mut qmldir = Qmldir::default();
    for line in source.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words[..] {
            ["module", uri] => qmldir.module = uri.to_string(),
            ["typeinfo", file] => qmldir.typeinfo.push(file.to_string()),
            ["depends", uri, ..] => qmldir.depends.push(uri.to_string()),
            ["import", uri, ..] => qmldir.import(uri, ImportKind::Always),
            ["default", "import", uri, ..] => qmldir.import(uri, ImportKind::Default),
            ["optional", "import", uri, ..] => qmldir.import(uri, ImportKind::Optional),
            ["singleton", name, version, file] => qmldir.file(name, version, file, true),
            [name, version, file] => qmldir.file(name, version, file, false),
            _ => {}
        }
    }
    qmldir
}

impl Qmldir {
    fn import(&mut self, uri: &str, kind: ImportKind) {
        self.imports.push(Import {
            module: uri.to_string(),
            kind,
        });
    }

    fn file(&mut self, name: &str, version: &str, file: &str, is_singleton: bool) {
        let is_type = name.starts_with(|c: char| c.is_ascii_uppercase());
        let Some(version) = version_of(version) else {
            return;
        };
        // `qml/+Material/SideBar.qml` is what a file selector puts in the
        // place of `qml/SideBar.qml` at run time; the type is the plain file.
        let is_selected = file.split('/').any(|part| part.starts_with('+'));
        if !is_type || is_selected || !file.ends_with(".qml") {
            return;
        }
        if self
            .files
            .get(name)
            .is_none_or(|known| known.version < version)
        {
            self.files.insert(
                name.to_string(),
                FileType {
                    file: file.to_string(),
                    is_singleton,
                    version,
                },
            );
        }
    }
}

/// `6.11` as numbers, so that it is after `6.9`.
pub fn version_of(text: &str) -> Option<(u32, u32)> {
    let (major, minor) = text.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// The directory of every module under `root`, by URI.
pub fn find(root: &Path) -> io::Result<BTreeMap<String, PathBuf>> {
    fn walk(directory: &Path, found: &mut BTreeMap<String, PathBuf>) -> io::Result<()> {
        let mut entries: Vec<PathBuf> = fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<_, _>>()?;
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, found)?;
            } else if path.file_name().is_some_and(|name| name == "qmldir") {
                let module = parse(&fs::read_to_string(&path)?).module;
                if !module.is_empty() {
                    found
                        .entry(module)
                        .or_insert_with(|| directory.to_path_buf());
                }
            }
        }
        Ok(())
    }
    let mut found = BTreeMap::new();
    walk(root, &mut found)?;
    Ok(found)
}
