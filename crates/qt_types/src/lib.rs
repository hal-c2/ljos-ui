//! What Qt's QML types are: the modules, types, properties, signals and enum
//! keys Qt describes for itself.
//!
//! Nobody writes these down by hand. Qt ships a `qmldir` and a `.qmltypes`
//! next to every plugin, and `mise run types` (`crates/qmltypes`) boils them
//! down to `src/types.txt`, which is compiled in and indexed on first use.
//! This says what a name *is* (a type, a property, a signal, an enum key);
//! what it means on a render target is not Qt's to say.
//!
//! The table is derived from a local Qt install, so it is git-ignored and never
//! committed (ARCHITECTURE.md D1). Without one the crate still builds, with an
//! empty table: [`is_available`] is false and every lookup finds nothing.
//! `LJOS_QT_TYPES=/path/to/types.txt` at build time takes a table from
//! elsewhere.
//!
//! The table is lines of words, indented by what they belong to:
//!
//! ```text
//! module QtQuick                        a module, by the URI it is imported as
//!     import QtQml                      its qmldir `import`: QtQml's types are QtQuick's too
//!     import QtQuick.Controls.Basic default     `default import`, see [`ImportKind`]
//!     Item QQuickItem                   an export: QML name, class
//!     type QQuickItem Item              a class, and the QML name it is known by if it has one
//!         prototype QObject             what it inherits
//!         extension QQuickFontValueType [namespace]   a class whose members are its own
//!         attached QQuickKeysAttached   the type of `Keys.` on another object
//!         default data                  the property that takes its children
//!         is singleton uncreatable value namespace parser
//!         enum TransformOrigin TopLeft Top ...        `flags` when the keys combine
//!         prop anchors QQuickAnchors readonly pointer   also `list`, `required`
//!         signal clicked mouse          the arguments by name, `_` for one without
//!         method forceActiveFocus
//! ```
//!
//! A class is the C++ class a type is registered from. A type that is a QML
//! file (Controls' `Button.qml`) is a class named by that file's path under
//! Qt's QML directory. The generator reads only `qmldir` and `.qmltypes`, never
//! the QML file itself, so such a type has no members of its own: its
//! prototype is the type of the same name in a module its qmldir depends on
//! (Basic's `Button` is Templates' `QQuickButton`), when there is one.
//!
//! Enum keys have no numbers: `.qmltypes` has listed only the names since Qt
//! 5.15. Versions are not kept either; the table is one Qt, the newest name
//! wins.

#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

/// Whether a table was compiled in. Without one every lookup finds nothing,
/// which a caller should report rather than treat as "no such type".
pub fn is_available() -> bool {
    cfg!(qt_types_table)
}

/// The text the table was read from, empty without one.
const TEXT: &str = include_str!(concat!(env!("OUT_DIR"), "/types.txt"));

/// The module `uri` is imported as, when it is one of Qt's that the table has.
pub fn module(uri: &str) -> Option<&'static Module> {
    table().modules.get(uri)
}

/// The URI of every module the table has, in order: what a file may import,
/// and what [`module`] finds. Empty without a table.
pub fn modules() -> Vec<&'static str> {
    let mut uris: Vec<&'static str> = table().modules.keys().copied().collect();
    uris.sort_unstable();
    uris
}

/// The types of the module `uri` that Qt has in C++ and not as QML files:
/// what a runtime has to have of it. None when the table has no such module.
pub fn native_types(uri: &str) -> Option<Vec<&'static str>> {
    types_of(uri, false)
}

/// The types of the module `uri` that Qt has as QML files: what is there only
/// where Qt's own module is installed. None when the table has no such module.
pub fn written_types(uri: &str) -> Option<Vec<&'static str>> {
    types_of(uri, true)
}

fn types_of(uri: &str, written: bool) -> Option<Vec<&'static str>> {
    let module = module(uri)?;
    let types = &table().types;
    let mut names: Vec<&'static str> = module
        .exports
        .iter()
        .filter(|(name, index)| {
            name.starts_with(|c: char| c.is_ascii_uppercase())
                && types[**index].qml_file.is_some() == written
        })
        .map(|(name, _)| *name)
        .collect();
    names.sort_unstable();
    Some(names)
}

#[derive(Debug)]
pub struct Module {
    pub uri: &'static str,
    /// The modules its qmldir imports, in the order it does.
    pub imports: Vec<Import>,
    exports: HashMap<&'static str, usize>,
}

#[derive(Debug)]
pub struct Import {
    pub module: &'static str,
    pub kind: ImportKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImportKind {
    /// `import`: importing the module imports this one too. `QtQuick` has
    /// `Timer` because it imports `QtQml`.
    Always,
    /// `default import`: as `import` unless one of the optional imports is
    /// chosen in its place. This is how `QtQuick.Controls` has a style: its
    /// `Button` is `QtQuick.Controls.Basic`'s. Qt itself loads no such
    /// import; the module's plugin does, having a choice to make. One that
    /// has none (`QtQuick.VectorImage` names `QtQuick.Shapes` so) is not
    /// followed: `Shape` is no type until `QtQuick.Shapes` is imported.
    Default,
    /// `optional import`: what may be chosen at run time in place of the
    /// default (`QtQuick.Controls.Material`). Not followed; a file that wants
    /// one imports it by name.
    Optional,
}

impl Module {
    /// The type `name` is in a file that imports this module: one of its own
    /// or of a module that comes with it.
    pub fn type_named(&self, name: &str) -> Option<&'static Type> {
        self.find(name, &mut Vec::new())
    }

    fn find(&self, name: &str, seen: &mut Vec<&'static str>) -> Option<&'static Type> {
        if seen.contains(&self.uri) {
            return None;
        }
        seen.push(self.uri);
        if let Some(index) = self.exports.get(name) {
            return Some(&table().types[*index]);
        }
        let chooses = self
            .imports
            .iter()
            .any(|import| import.kind == ImportKind::Optional);
        self.imports
            .iter()
            .filter(|import| match import.kind {
                ImportKind::Always => true,
                ImportKind::Default => chooses,
                ImportKind::Optional => false,
            })
            .find_map(|import| module(import.module)?.find(name, seen))
    }
}

#[derive(Debug)]
pub struct Type {
    /// The QML name. A type no module exports (`anchors` is a
    /// `QQuickAnchors`) has only its class.
    pub name: &'static str,
    /// The C++ class, or the path of the QML file under Qt's QML directory.
    pub class: &'static str,
    /// The module that describes it; of a type several modules export, one
    /// of them.
    pub module: &'static str,
    /// Set when the type is a QML file: `QtQuick/Controls/Basic/Button.qml`.
    pub qml_file: Option<&'static str>,
    pub is_singleton: bool,
    /// May be written as `Name { }`: exported, and not a singleton, a value,
    /// a namespace or a type Qt registers as uncreatable (`Keys`, `Layout`).
    pub is_creatable: bool,
    /// Copied, not referred to: `color`, `font`, `point`.
    pub is_value: bool,
    /// Qt reads the object's body itself, so anything may be in it:
    /// `ListElement`, `Connections`, `PropertyChanges`.
    pub has_custom_parser: bool,
    /// The type's own members; the lookups below also see inherited ones.
    pub enums: Vec<Enum>,
    pub properties: Vec<Property>,
    pub signals: Vec<Signal>,
    pub methods: Vec<&'static str>,
    default_property: Option<&'static str>,
    prototype: Option<usize>,
    extension: Option<usize>,
    /// The extension is there for its enums only (`Qt.AlignLeft`).
    extension_is_namespace: bool,
    attached: Option<usize>,
}

impl Type {
    pub fn prototype(&self) -> Option<&'static Type> {
        self.prototype.map(|index| &table().types[index])
    }

    /// The property an object written inside this one is put in (`data` for
    /// an `Item`).
    pub fn default_property(&'static self) -> Option<&'static str> {
        self.find(false, &|ty| ty.default_property)
    }

    /// The type of `Name.` on another object, for a type that attaches:
    /// `Layout.fillWidth` is a property of `Layout`'s attached type, which is
    /// `RowLayout`'s too.
    pub fn attached(&'static self) -> Option<&'static Type> {
        self.find(false, &|ty| ty.attached.map(|index| &table().types[index]))
    }

    /// Whether `Name.member` is of what the type attaches. A `PathView`
    /// attaches whatever its path names too: `PathView.iconScale`.
    pub fn attaches(&'static self, member: &str) -> bool {
        self.attached().is_some_and(|attached| {
            attached.property(member).is_some()
                || attached.signal(member).is_some()
                || attached.has_method(member)
                || (attached.class == "QQuickPathViewAttached"
                    && member.starts_with(|c: char| c.is_ascii_lowercase()))
        })
    }

    pub fn property(&'static self, name: &str) -> Option<&'static Property> {
        self.find(false, &|ty| {
            ty.properties.iter().find(|property| property.name == name)
        })
    }

    /// The signal `name`, which `onName` handles. A property's change signal
    /// is one of these.
    pub fn signal(&'static self, name: &str) -> Option<&'static Signal> {
        self.find(false, &|ty| {
            ty.signals.iter().find(|signal| signal.name == name)
        })
    }

    pub fn has_method(&'static self, name: &str) -> bool {
        self.find(false, &|ty| ty.methods.contains(&name).then_some(()))
            .is_some()
    }

    /// The enum key `Type.key` names: `Text.Raised`, `Qt.AlignLeft`.
    pub fn enum_value(&'static self, key: &str) -> Option<EnumValue> {
        self.find(true, &|ty| {
            ty.enums.iter().find_map(|enumeration| {
                let (key, number) = enumeration.keys.iter().find(|(known, _)| *known == key)?;
                Some(EnumValue {
                    enumeration,
                    key,
                    number: *number,
                })
            })
        })
    }

    /// The first thing `get` finds in the type, its extension or what it
    /// inherits, in the order QML looks.
    fn find<T>(&'static self, enums: bool, get: &impl Fn(&'static Type) -> Option<T>) -> Option<T> {
        let extension = || {
            let extension = &table().types[self.extension?];
            (enums || !self.extension_is_namespace).then(|| extension.find(enums, get))?
        };
        get(self)
            .or_else(extension)
            .or_else(|| self.prototype()?.find(enums, get))
    }
}

#[derive(Debug)]
pub struct Property {
    pub name: &'static str,
    /// The class of the value (`QColor`, `double`, `QQuickItem`), or the name
    /// of an enum: of the class that has the property (`TransformOrigin`) or
    /// of another (`Qt::Alignment`). In a QML file's `property alias`,
    /// `alias`.
    pub type_name: &'static str,
    pub is_list: bool,
    /// The property cannot be assigned. Its value can still be written into
    /// when it is an object or a list: `anchors.fill`, `border.color` and
    /// `children` are all read-only.
    pub is_readonly: bool,
    /// The value is an object, not a value type.
    pub is_pointer: bool,
    /// A `required property`: whoever makes the object has to set it.
    pub is_required: bool,
    /// The value is a `Component`: what is written there is a template that
    /// is instantiated later (a `delegate`), not an object that exists.
    pub is_component: bool,
}

impl Property {
    /// The type of the value, which is where `font.bold` and `anchors.fill`
    /// go on. None for an enum.
    pub fn value_type(&self) -> Option<&'static Type> {
        let table = table();
        table
            .classes
            .get(self.type_name)
            .map(|index| &table.types[*index])
    }
}

#[derive(Debug)]
pub struct Signal {
    pub name: &'static str,
    /// What a handler may call the arguments without declaring them. Empty
    /// for an argument Qt gives no name.
    pub parameters: Vec<&'static str>,
}

#[derive(Debug)]
pub struct Enum {
    pub name: &'static str,
    /// The name properties are declared with when it is not the enum's:
    /// `Flags` is the alias of `Flag`.
    pub alias: Option<&'static str>,
    /// The keys are bits to combine.
    pub is_flag: bool,
    /// In the order declared, which is not their value. The number is there
    /// only when Qt's description gives it, and Qt 6's never does.
    pub keys: Vec<(&'static str, Option<i64>)>,
}

#[derive(Clone, Copy, Debug)]
pub struct EnumValue {
    pub enumeration: &'static Enum,
    pub key: &'static str,
    pub number: Option<i64>,
}

/// Whether some type of Qt's has a member called `name`: a name that is
/// most likely that member wherever it is written.
pub fn is_member_name(name: &str) -> bool {
    static NAMES: OnceLock<HashSet<&'static str>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            let mut names = HashSet::new();
            for ty in &table().types {
                names.extend(ty.properties.iter().map(|property| property.name));
                names.extend(ty.signals.iter().map(|signal| signal.name));
                names.extend(ty.methods.iter().copied());
            }
            names
        })
        .contains(name)
}

#[derive(Default)]
struct Table {
    modules: HashMap<&'static str, Module>,
    types: Vec<Type>,
    /// A type's index by its class.
    classes: HashMap<&'static str, usize>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| read(TEXT))
}

fn read(text: &'static str) -> Table {
    let mut table = Table::default();
    // Classes are named before they are described, so what refers to one is
    // looked up once all are read.
    let mut exports = Vec::new();
    let mut links = Vec::new();
    let mut module = "";

    for line in text.lines() {
        let depth = line.len() - line.trim_start_matches('\t').len();
        let mut words = line.split_ascii_whitespace();
        let mut word = || words.next().unwrap_or("");
        match (depth, word()) {
            (0, "module") => {
                module = word();
                let empty = Module {
                    uri: module,
                    imports: Vec::new(),
                    exports: HashMap::new(),
                };
                table.modules.insert(module, empty);
            }
            (0, _) => {}
            (1, "import") => {
                let import = Import {
                    module: word(),
                    kind: match word() {
                        "default" => ImportKind::Default,
                        "optional" => ImportKind::Optional,
                        _ => ImportKind::Always,
                    },
                };
                table
                    .modules
                    .get_mut(module)
                    .expect("a module")
                    .imports
                    .push(import);
            }
            (1, "type") => {
                let (class, name) = (word(), word());
                table.classes.insert(class, table.types.len());
                table.types.push(Type {
                    name: if name.is_empty() { class } else { name },
                    class,
                    module,
                    qml_file: class.ends_with(".qml").then_some(class),
                    is_singleton: false,
                    // What has a QML name is exported; `is` takes the rest back.
                    is_creatable: !name.is_empty(),
                    is_value: false,
                    has_custom_parser: false,
                    enums: Vec::new(),
                    properties: Vec::new(),
                    signals: Vec::new(),
                    methods: Vec::new(),
                    default_property: None,
                    prototype: None,
                    extension: None,
                    extension_is_namespace: false,
                    attached: None,
                });
            }
            (1, name) => exports.push((module, name, word())),
            (_, member) => {
                let index = table.types.len() - 1;
                let ty = &mut table.types[index];
                match member {
                    "prototype" | "attached" => links.push((index, member, word())),
                    "extension" => {
                        links.push((index, member, word()));
                        ty.extension_is_namespace = word() == "namespace";
                    }
                    "default" => ty.default_property = Some(word()),
                    "is" => {
                        for flag in words {
                            match flag {
                                "singleton" => ty.is_singleton = true,
                                "value" => ty.is_value = true,
                                "parser" => ty.has_custom_parser = true,
                                _ => {}
                            }
                            // Only `parser` leaves a type one to instantiate.
                            ty.is_creatable &= flag == "parser";
                        }
                    }
                    "enum" | "flags" => {
                        let name = word();
                        let (name, alias) = match name.split_once('=') {
                            Some((name, alias)) => (name, Some(alias)),
                            None => (name, None),
                        };
                        let keys = words.map(|key| match key.split_once('=') {
                            Some((key, number)) => (key, number.parse().ok()),
                            None => (key, None),
                        });
                        let is_flag = member == "flags";
                        ty.enums.push(Enum {
                            name,
                            alias,
                            is_flag,
                            keys: keys.collect(),
                        });
                    }
                    "prop" => {
                        let (name, type_name) = (word(), word());
                        let flags: Vec<&str> = words.collect();
                        ty.properties.push(Property {
                            name,
                            type_name,
                            is_list: flags.contains(&"list"),
                            is_readonly: flags.contains(&"readonly"),
                            is_pointer: flags.contains(&"pointer"),
                            is_required: flags.contains(&"required"),
                            is_component: type_name == "QQmlComponent",
                        });
                    }
                    "signal" => {
                        let name = word();
                        let parameters = words.map(|name| if name == "_" { "" } else { name });
                        ty.signals.push(Signal {
                            name,
                            parameters: parameters.collect(),
                        });
                    }
                    "method" => ty.methods.push(word()),
                    _ => {}
                }
            }
        }
    }

    for (module, name, class) in exports {
        if let Some(index) = table.classes.get(class) {
            table
                .modules
                .get_mut(module)
                .expect("a module")
                .exports
                .insert(name, *index);
        }
    }
    for (index, link, class) in links {
        let to = table.classes.get(class).copied();
        let ty = &mut table.types[index];
        match link {
            "prototype" => ty.prototype = to,
            "extension" => ty.extension = to,
            _ => ty.attached = to,
        }
    }
    table
}
