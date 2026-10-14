//! Writes the table of Qt's QML types that `qt_types` reads.
//!
//! Qt describes every type it registers in files next to its plugins:
//! `qmldir` names a module, the modules that come with it and the types that
//! are QML files, and `*.qmltypes` lists the types registered from C++. This
//! reads them for the modules Qt's examples import and writes one small text
//! file, so the front end knows QtQuick without having Qt.
//!
//! Only `qmldir` and `.qmltypes` are read, never Qt's QML source files
//! (CLEAN_ROOM.md). A type that is a QML file therefore has no members of its
//! own here; it takes them from the type it is most likely built on (see
//! [`files`]). The output is derived from the local Qt, so it is git-ignored
//! and never committed (ARCHITECTURE.md D1).
//!
//! ```sh
//! qmltypes [--qt /usr/lib/qt6/qml]... [--out crates/qt_types/src/types.txt]
//! ```
//!
//! `--qt` may be given more than once, for modules of Qt that are kept
//! somewhere else: a module is taken from the first directory that has it.
//! Without it, the directories `QML_IMPORT_PATH` names come before Qt's own.

mod model;
mod qmldir;
mod qmltypes;
mod write;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use model::{Class, ImportKind, Module, Modules, Semantics, export};
use qmldir::Qmldir;

/// The modules of Qt that the examples in `corpus/qtdoc` import, a broad
/// sample of what applications use:
///
/// ```sh
/// grep -rhoE '^import [A-Za-z0-9.]+' corpus/qtdoc/examples --include=*.qml | sort -u
/// ```
///
/// without the ones the examples define themselves, and with the ones the
/// styles of Qt Quick Controls import: a style is QML too, compiled as an
/// example is. QtWebSockets is what stand-ins for examples' C++ import.
/// What these need comes along; one that is not installed is reported and
/// left out. `--module URI` adds another.
const MODULES: &[&str] = &[
    "Qt.labs.assetdownloader",
    "Qt.labs.folderlistmodel",
    "Qt.labs.qmlmodels",
    "Qt.labs.synchronizer",
    "QtCharts",
    "QtCore",
    "QtGraphs",
    "QtLocation",
    "QtMultimedia",
    "QtPositioning",
    "QtQml",
    "QtQml.Models",
    "QtQml.XmlListModel",
    "QtQuick",
    "QtQuick.Controls",
    "QtQuick.Controls.Basic",
    "QtQuick.Controls.Basic.impl",
    "QtQuick.Controls.Fusion",
    "QtQuick.Controls.Fusion.impl",
    "QtQuick.Controls.Imagine",
    "QtQuick.Controls.Imagine.impl",
    "QtQuick.Controls.Material",
    "QtQuick.Controls.Material.impl",
    "QtQuick.Controls.Universal",
    "QtQuick.Controls.Universal.impl",
    "QtQuick.Controls.impl",
    "QtQuick.Dialogs",
    "QtQuick.Effects",
    "QtQuick.Layouts",
    "QtQuick.LocalStorage",
    "QtQuick.Particles",
    "QtQuick.Shapes",
    "QtQuick.Studio.Application",
    "QtQuick.Studio.Components",
    "QtQuick.Studio.DesignEffects",
    "QtQuick.Templates",
    "QtQuick.Timeline",
    "QtQuick.VectorImage",
    "QtQuick.Window",
    "QtQuick3D",
    "QtQuick3D.AssetUtils",
    "QtQuick3D.Effects",
    "QtQuick3D.Helpers",
    "QtQuick3D.Particles3D",
    "QtQuick3D.Physics",
    "QtQuick3D.Xr",
    "QtSensors",
    "QtWebSockets",
];

/// The module every QML file has without importing it.
const BUILTINS: &str = "QML";

fn main() -> ExitCode {
    let mut qt: Vec<PathBuf> = Vec::new();
    let mut out = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../qt_types/src/types.txt"
    ));
    let mut wanted: Vec<String> = MODULES.iter().map(|uri| uri.to_string()).collect();
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match (argument.as_str(), arguments.next()) {
            ("--qt", Some(path)) => qt.push(path.into()),
            ("--out", Some(path)) => out = path.into(),
            ("--module", Some(uri)) => wanted.push(uri),
            _ => {
                eprintln!("usage: qmltypes [--qt DIRECTORY]... [--module URI]... [--out FILE]");
                return ExitCode::FAILURE;
            }
        }
    }

    // As Qt looks for them: where `QML_IMPORT_PATH` says, then Qt's own.
    if qt.is_empty() {
        if let Some(elsewhere) = std::env::var_os("QML_IMPORT_PATH") {
            qt.extend(
                std::env::split_paths(&elsewhere).filter(|path| !path.as_os_str().is_empty()),
            );
        }
        qt.push(PathBuf::from("/usr/lib/qt6/qml"));
    }

    match generate(&qt, wanted) {
        Ok(text) => {
            if let Err(error) = fs::write(&out, &text) {
                eprintln!("{}: {error}", out.display());
                return ExitCode::FAILURE;
            }
            eprintln!("{}: {} bytes", out.display(), text.len());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// A module as its directory has it, before the modules are put together.
struct Source {
    qmldir: Qmldir,
    classes: Vec<Class>,
    /// The types that are QML files: name and path under the Qt directory.
    files: Vec<(String, String)>,
}

fn generate(qt: &[PathBuf], mut wanted: Vec<String>) -> Result<String, String> {
    let mut directories = BTreeMap::new();
    for root in qt {
        let found = qmldir::find(root).map_err(|error| format!("{}: {error}", root.display()))?;
        for (uri, directory) in found {
            directories
                .entry(uri)
                .or_insert((root.as_path(), directory));
        }
    }

    let mut sources = BTreeMap::new();
    let mut missing = BTreeSet::new();
    wanted.push(BUILTINS.to_string());
    while let Some(uri) = wanted.pop() {
        if sources.contains_key(&uri) || missing.contains(&uri) {
            continue;
        }
        let Some((root, directory)) = directories.get(&uri) else {
            missing.insert(uri);
            continue;
        };
        let source = read(root, directory)?;
        wanted.extend(needs(&source));
        sources.insert(uri, source);
    }
    for uri in &missing {
        eprintln!("not installed: {uri}");
    }

    let version = newest(&sources);
    let mut modules = classes(&sources);
    files(&sources, &mut modules);
    tidy(&mut modules);
    Ok(write::write(&modules, version))
}

fn read(qt: &Path, directory: &Path) -> Result<Source, String> {
    let text = |path: &Path| {
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
    };
    let relative = |path: &Path| {
        path.strip_prefix(qt)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    };

    let qmldir = qmldir::parse(&text(&directory.join("qmldir"))?);
    let mut classes = Vec::new();
    for typeinfo in &qmldir.typeinfo {
        let path = directory.join(typeinfo);
        // A parse error is fatal: a module without its types would silently
        // turn every one of them into an unknown component.
        classes.extend(
            qmltypes::read(&text(&path)?)
                .map_err(|error| format!("{}: {error}", path.display()))?,
        );
    }
    let files = qmldir
        .files
        .iter()
        .map(|(name, file)| (name.clone(), relative(&directory.join(&file.file))))
        .collect();
    Ok(Source {
        qmldir,
        classes,
        files,
    })
}

/// The modules a module's descriptions refer to: the ones it imports or
/// depends on.
fn needs(source: &Source) -> Vec<String> {
    let qmldir = &source.qmldir;
    let imports = qmldir
        .imports
        .iter()
        .filter(|import| import.kind != ImportKind::Optional)
        .map(|import| import.module.clone());
    imports.chain(qmldir.depends.iter().cloned()).collect()
}

/// The newest version anything is exported at, which is the Qt the
/// descriptions are from.
fn newest(sources: &BTreeMap<String, Source>) -> (u32, u32) {
    let files = sources
        .values()
        .flat_map(|source| source.qmldir.files.values());
    let classes = sources.values().flat_map(|source| &source.classes);
    files
        .map(|file| file.version)
        .chain(classes.flat_map(|class| class.exports.values().copied()))
        .max()
        .unwrap_or_default()
}

/// The classes of the `.qmltypes` files, each in one module, and what every
/// module exports of them.
///
/// A class registered by several modules is described by each (Templates and
/// the Basic style both have `QQuickOverlay`). The descriptions differ only in
/// what they export, so one is kept and it exports what all of them do. Kept
/// is one whose own module exports it, of those the one that says most.
///
/// Descriptions that differ in more are of classes that share a name and no
/// more: QtCharts and QtGraphs each have a `QAbstractAxis`. Each is kept,
/// named with its module (`QAbstractAxis@QtGraphs`), and what a module says
/// of the name is said of its own.
fn classes(sources: &BTreeMap<String, Source>) -> Modules {
    let mut modules: Modules = sources
        .iter()
        .map(|(uri, source)| {
            (
                uri.clone(),
                Module {
                    imports: source.qmldir.imports.clone(),
                    ..Module::default()
                },
            )
        })
        .collect();

    let same = |one: &Class, other: &Class| {
        one.prototype == other.prototype && one.properties == other.properties
    };
    let mut described: BTreeMap<&str, Vec<Vec<(&str, &Class)>>> = BTreeMap::new();
    for (uri, source) in sources {
        for class in &source.classes {
            let kinds = described.entry(&class.name).or_default();
            match kinds.iter_mut().find(|kind| same(kind[0].1, class)) {
                Some(kind) => kind.push((uri, class)),
                None => kinds.push(vec![(uri, class)]),
            }
        }
    }

    let mut kept: Vec<(&str, Class)> = Vec::new();
    // What a module means by a name that several classes have.
    let mut own: BTreeMap<&str, BTreeMap<&str, String>> = BTreeMap::new();
    for (name, kinds) in &described {
        for descriptions in kinds {
            let rank = |(uri, class): &(&str, &Class)| {
                (
                    class.exports.keys().any(|(module, _)| module == uri),
                    class.members(),
                )
            };
            // `max_by_key` takes the last of equals; the first is wanted.
            let (home, described) = descriptions
                .iter()
                .rev()
                .max_by_key(|description| rank(description))
                .expect("one");
            let mut class = (*described).clone();
            if kinds.len() > 1 {
                class.name = format!("{name}@{home}");
            }
            for (uri, other) in descriptions {
                if kinds.len() > 1 {
                    own.entry(uri).or_default().insert(name, class.name.clone());
                }
                class.aliases.extend(other.aliases.iter().cloned());
                for (export, version) in &other.exports {
                    let known = class.exports.entry(export.clone()).or_default();
                    *known = (*known).max(*version);
                }
            }
            kept.push((home, class));
        }
    }

    for (home, class) in &mut kept {
        let links = [
            &mut class.prototype,
            &mut class.extension,
            &mut class.attached,
            &mut class.element,
        ];
        let types = class
            .properties
            .iter_mut()
            .map(|property| &mut property.type_name);
        for link in links.into_iter().flatten().chain(types) {
            if let Some(class) = own.get(home).and_then(|own| own.get(link.as_str())) {
                link.clone_from(class);
            }
        }
    }
    // Of two classes with one name the newer is the type: `TextInput` was
    // `QQuickPre64TextInput` until 6.4.
    // (module, exported name) -> (version, class)
    type Newest<'k> = BTreeMap<(&'k str, &'k str), ((u32, u32), &'k str)>;
    let mut names = Newest::new();
    for (_, class) in &kept {
        for ((module, exported), version) in &class.exports {
            let known = names
                .entry((module, exported))
                .or_insert((*version, &class.name));
            if *version >= known.0 {
                *known = (*version, &class.name);
            }
        }
    }
    for ((module, exported), (_, class)) in names {
        if let Some(module) = modules.get_mut(module) {
            module
                .exports
                .insert(exported.to_string(), class.to_string());
        }
    }
    for (home, class) in kept {
        modules
            .get_mut(home)
            .expect("read")
            .classes
            .insert(class.name.clone(), class);
    }
    modules
}

/// Adds the types that are QML files. Such a type is a class named by its
/// file.
///
/// Its prototype would be the class of the file's root object, but the file is
/// Qt's source and is not read. What the module's qmldir says is used instead:
/// a QML type is taken to be built on the type of the same name in a module
/// its qmldir depends on or imports (Basic's `Button.qml` on Templates'
/// `Button`, `QQuickButton`), which is how Qt Quick Controls styles are made.
/// Failing that, a file with the name of a class its own module registers
/// (`SideBar.qml` and `DialogsQuickImpl.SideBar`) is built on that class.
/// What the file itself declares on top is not known.
fn files(sources: &BTreeMap<String, Source>, modules: &mut Modules) {
    let mut replaced = BTreeMap::new();
    for (uri, source) in sources {
        let module = modules.get_mut(uri).expect("read");
        for (name, path) in &source.files {
            if let Some(class) = module.exports.insert(name.clone(), path.clone()) {
                replaced.insert(path, class);
            }
        }
    }
    let mut prototypes = BTreeMap::new();
    for (uri, source) in sources {
        for (name, path) in &source.files {
            let prototype = base(modules, uri, &source.qmldir, name)
                .filter(|prototype| prototype != path)
                .or_else(|| replaced.get(path).cloned());
            prototypes.insert(path.clone(), prototype);
        }
    }
    for (uri, source) in sources {
        for (name, path) in &source.files {
            // A style that imports another (Material imports Basic) is built
            // on what that one is built on, not on its QML.
            let mut prototype = prototypes[path].clone();
            for _ in 0..8 {
                match prototype.as_ref().and_then(|file| prototypes.get(file)) {
                    Some(next) => prototype = next.clone(),
                    None => break,
                }
            }
            if prototype.is_none() {
                eprintln!("{path}: no type to build on; its members are unknown");
            }
            let is_singleton = source.qmldir.files[name].is_singleton;
            let class = Class {
                name: path.clone(),
                is_singleton,
                prototype,
                ..Class::default()
            };
            modules
                .get_mut(uri)
                .expect("read")
                .classes
                .insert(path.clone(), class);
        }
    }
}

/// The class a QML type of module `own` called `name` is built on: the type of
/// that name in a module the qmldir depends on or imports, in the order it
/// names them, or `Abstract` and that name.
fn base(modules: &Modules, own: &str, qmldir: &Qmldir, name: &str) -> Option<String> {
    let imports = qmldir
        .imports
        .iter()
        .filter(|import| import.kind != ImportKind::Optional);
    let uris = qmldir
        .depends
        .iter()
        .chain(imports.map(|import| &import.module));
    uris.filter(|uri| *uri != own)
        .find_map(|uri| {
            // Templates export the calendar types as `AbstractMonthGrid`
            // and so on, which their styles' files drop the prefix of.
            export(modules, uri, name).or_else(|| export(modules, uri, &format!("Abstract{name}")))
        })
        .map(str::to_string)
}

/// Makes every reference one the reader can follow: a class by its one name,
/// a list as its element type, and nothing pointing at a class that is not
/// there. Then drops what no QML name leads to.
fn tidy(modules: &mut Modules) {
    let classes: BTreeMap<String, Class> = modules
        .values()
        .flat_map(|module| module.classes.clone())
        .collect();
    let aliases: BTreeMap<&str, &str> = classes
        .values()
        .flat_map(|class| {
            class
                .aliases
                .iter()
                .map(|alias| (alias.as_str(), class.name.as_str()))
        })
        .collect();
    let canonical = |name: &str| aliases.get(name).copied().unwrap_or(name).to_string();
    let known = |name: &Option<String>| {
        name.as_deref()
            .map(canonical)
            .filter(|name| classes.contains_key(name))
    };

    for class in modules
        .values_mut()
        .flat_map(|module| module.classes.values_mut())
    {
        for (what, link) in [
            ("prototype", &mut class.prototype),
            ("attached type", &mut class.attached),
        ] {
            let found = known(link);
            if let (Some(name), None) = (&link, &found) {
                eprintln!("{}: {what} {name} is not described", class.name);
            }
            *link = found;
        }
        // Mostly JavaScript prototypes, which are not read.
        class.extension = known(&class.extension);
        for property in &mut class.properties {
            property.type_name = canonical(&property.type_name);
            // `QList<QUrl>`, `QStringList`: a list of the element type.
            while let Some(Class {
                semantics: Semantics::Sequence,
                element: Some(element),
                ..
            }) = classes.get(&property.type_name)
            {
                property.type_name = canonical(element);
                property.is_list = true;
            }
        }
    }

    let mut reached = BTreeSet::new();
    let mut next: Vec<&str> = modules
        .values()
        .flat_map(|module| module.exports.values().map(String::as_str))
        .collect();
    while let Some(name) = next.pop() {
        let Some(class) = modules.values().find_map(|module| module.classes.get(name)) else {
            continue;
        };
        if !reached.insert(name.to_string()) {
            continue;
        }
        let links = [&class.prototype, &class.extension, &class.attached];
        next.extend(links.into_iter().flatten().map(String::as_str));
        next.extend(
            class
                .properties
                .iter()
                .map(|property| property.type_name.as_str()),
        );

        // The reader follows prototypes without looking back.
        let mut ancestor = class;
        for _ in 0..64 {
            let Some(prototype) = ancestor
                .prototype
                .as_ref()
                .and_then(|name| classes.get(name))
            else {
                break;
            };
            ancestor = prototype;
        }
        assert!(ancestor.prototype.is_none(), "{name} is its own prototype");
    }
    for module in modules.values_mut() {
        module
            .classes
            .retain(|name, class| reached.contains(name) && class.semantics != Semantics::Sequence);
    }
}
