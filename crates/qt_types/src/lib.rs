//! What the QML types are: the modules, types, properties, signals and enum
//! keys a file may name. This says what a name *is* (a type, a property, a
//! signal, an enum key); what it means on a render target is not for a table
//! to say.
//!
//! There are two descriptions, in one format, and nobody writes either by
//! hand:
//!
//! - **Qt's of its own types.** Qt ships a `qmldir` and a `.qmltypes` next to
//!   every plugin, and `mise run types` (`crates/qmltypes`) boils them down to
//!   `src/types.txt`. It is derived from a local Qt install, so it is
//!   git-ignored and never committed (ARCHITECTURE.md D1): a machine has it
//!   only when somebody generated it there.
//! - **Ljós's of its own types**, `src/ljos.txt`, which is committed. It is
//!   what the native types of the runtime and the QML files of its style
//!   declare of themselves, written out by
//!   `crates/ljos_cli/tests/own_types.rs`. Nothing in it comes from Qt.
//!
//! # Which one is read
//!
//! Without Qt's table, Ljós's own is the whole table: QML is read, checked
//! and compiled against what Ljós has. Nothing needs a Qt anywhere.
//! [`has_qt_table`] is false, and the only thing lost is what only Qt's
//! table can say: that a type nobody here has is one Qt has. Such a type is
//! an unknown type like any other.
//!
//! With Qt's table, Qt's description is the whole truth of every module it
//! describes, and Ljós's own table gives only the modules Qt's has not
//! (`Ljos`, `QtTest`, a module of an add-on the local Qt was installed
//! without). The choice is made by module, never by type, for three reasons:
//!
//! 1. Qt's is the complete description. It also lists what Qt has and Ljós
//!    has not yet, which is what `ljos check`, the `api_status` tool and the
//!    lints tell apart; and a file written for Qt is read as Qt reads it,
//!    which is what parity work needs.
//! 2. A type's description names others (what it inherits, what a property
//!    holds). Taking one type from one table and its base from the other
//!    would make chains nobody described.
//! 3. It is what a build with Qt's table has always done, so nothing that
//!    was read one way there is read another way now.
//!
//! So the two never describe one module at once, and a build without Qt's
//! table is held to the other by tests that compare the two where both
//! exist (`crates/ljos_cli/tests/own_types.rs`), not by mixing them.
//!
//! `LJOS_QT_TYPES=/path/to/types.txt` at build time takes Qt's table from
//! elsewhere; `LJOS_QT_TYPES=none` builds without one on a machine that has
//! it.
//!
//! # The format
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
//!         prop anchors QQuickAnchors readonly pointer   also `list`, `required`, `reset`, `script`
//!         signal clicked mouse          the arguments by name, `_` for one without
//!         method forceActiveFocus
//! ```
//!
//! A class is what a type is registered from: in Qt's table the C++ class, in
//! Ljós's own the module and the name (`QtQuick.Item`; no class of Qt's has a
//! dot before its last word). A type that is a QML file (Controls'
//! `Button.qml`) is a class named by that file's path under the directory of
//! QML modules. Qt's generator reads only `qmldir` and `.qmltypes`, never the
//! QML file itself, so such a type of Qt's has no members of its own: its
//! prototype is the type of the same name in a module its qmldir depends on
//! (Basic's `Button` is Templates' `QQuickButton`), when there is one.
//!
//! Enum keys of Qt's table have no numbers: `.qmltypes` has listed only the
//! names since Qt 5.15. Ljós's own types say theirs (`Top=1`). Versions are
//! not kept; the table is one Qt, the newest name wins.
//!
//! # What Ljós's own table adds to the format
//!
//! A native type names what it inherits and what its properties hold as
//! QML does (`Item`, `string`), not by classes, so that file has one thing
//! Qt's table has no use for:
//!
//! ```text
//! module Ljos
//!     type Ljos.ErrorBoundary ErrorBoundary
//!         prototype QtQuick.Item named  `named`: what a file that imports QtQuick calls `Item`
//!         prop failed QtQml.bool named readonly
//! ```
//!
//! A `named` type is the module and the name: it is looked up as a file
//! that imports that module would find it, in whichever table describes the
//! module. This is what lets one text serve both builds: `QtQuick.Item
//! named` is Qt's `QQuickItem` beside Qt's table and Ljós's `QtQuick.Item`
//! without it. Naming a type of another module gives the module nothing: a
//! file that imports `Ljos` has no `Item`.
//!
//! # How the committed table is made, and why that is not a circle
//!
//! The crates that read QML must not link the runtime that has the types
//! (ARCHITECTURE.md principle 5), so the description
//! reaches them as text, and the text is written by a test that links
//! both. In order:
//!
//! 1. This crate compiles `src/ljos.txt` in, as committed.
//! 2. `ljos_controls`' build script compiles the Basic style's QML with the
//!    compiler, which reads it against that text (or against Qt's table,
//!    where there is one).
//! 3. `LJOS_OWN_TYPES=write cargo test -p ljos_cli --test own_types` links
//!    the runtime, asks every registered type what it declares and writes
//!    `src/ljos.txt`; without `write` it fails when the file is not what
//!    would be written.
//!
//! The committed text is what breaks the circle: step 2 never needs step 3
//! to have run on this machine. The one case where it does is a style file
//! that uses a native member added since the text was last written. Then,
//! in a build without Qt's table, the style does not compile until the text
//! is written again, which needs the build: so with `LJOS_OWN_TYPES=write`
//! set the build script only warns and packs no style, the text is written
//! from the native types, and running the same command once more (the
//! style compiles now) adds what the style's files declare. The test says so
//! when it is the case.

#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

/// Whether Qt's own description of its types was compiled in. Without it
/// the table is Ljós's description of its own types alone: every lookup
/// still answers, but a type that is not found may be one Qt has, which a
/// caller that reports it should say ([`WITHOUT_QT_TABLE`]).
pub fn has_qt_table() -> bool {
    cfg!(qt_types_table)
}

/// What a message about a name the table does not have ends with in a build
/// without Qt's table: the name may be Qt's all the same.
pub const WITHOUT_QT_TABLE: &str = "this build knows Ljós's own types only: with a table of Qt's types it could tell whether Qt has it";

/// For a test that holds only of Qt's description of its types (a class
/// name, a type Ljós does not have): whether there is one to hold it of.
/// Without it the test is named as skipped on the terminal, past what the
/// test harness captures, so that a green run does not pass for a run of it.
pub fn test_has_qt_table(test: &str) -> bool {
    if !has_qt_table() {
        use std::io::Write as _;
        // Written to the stream itself: `eprintln!` in a passing test is
        // swallowed by the harness.
        let _ = writeln!(
            std::io::stderr(),
            "SKIPPED {test}: it is about Qt's own description of its types, and this build has \
             no table of them (`mise run types`, and build without LJOS_QT_TYPES=none)"
        );
    }
    has_qt_table()
}

/// Qt's table as it was compiled in, empty without one.
const TEXT: &str = include_str!(concat!(env!("OUT_DIR"), "/types.txt"));

/// The module `uri` is imported as, when the table has it.
pub fn module(uri: &str) -> Option<&'static Module> {
    table().modules.get(uri)
}

/// The URI of every module the table has, in order: what a file may import,
/// and what [`module`] finds.
pub fn modules() -> Vec<&'static str> {
    let mut uris: Vec<&'static str> = table().modules.keys().copied().collect();
    uris.sort_unstable();
    uris
}

/// The types of the module `uri` that are native and not QML files: of Qt's
/// description, what a runtime has to have of it. None when the table has no
/// such module.
pub fn native_types(uri: &str) -> Option<Vec<&'static str>> {
    types_of(uri, false)
}

/// The types of the module `uri` that are QML files: of Qt's description,
/// what is there only where Qt's own module is installed. None when the
/// table has no such module.
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
    /// Described by Ljós's own table, not by Qt's.
    pub is_own: bool,
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
        let table = table();
        let index = table.type_named(self.uri, name, &mut Vec::new())?;
        Some(&table.types[index])
    }
}

#[derive(Debug)]
pub struct Type {
    /// The QML name. A type no module exports (`anchors` is a
    /// `QQuickAnchors`) has only its class.
    pub name: &'static str,
    /// The C++ class, or the path of the QML file under Qt's QML directory.
    /// Of a type Ljós describes, the module and the name: `QtQuick.Item`.
    pub class: &'static str,
    /// Described by Ljós's own table, not by Qt's.
    pub is_own: bool,
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

    /// Whether the type is, or inherits, one of `classes`. The same type has
    /// one class in Qt's table and another in Ljós's own (`QQuickImage`,
    /// `QtQuick.Image`), so whoever asks names both.
    pub fn is_a(&self, classes: &[&str]) -> bool {
        classes.contains(&self.class) || self.prototype().is_some_and(|ty| ty.is_a(classes))
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

    /// Whether a property declared with the enum `name` of the type holds
    /// several of its keys at once: `Alignment`, not `AlignmentFlag` or
    /// `Orientation`. None when the type has no enum of that name.
    pub fn enum_is_flags(&'static self, name: &str) -> Option<bool> {
        self.find(true, &|ty| {
            let own = |enumeration: &&Enum| enumeration.name == name;
            // The name of one key's enum is what the flags are made of.
            let of_flags = |enumeration: &&Enum| enumeration.alias == Some(name);
            ty.enums
                .iter()
                .find(own)
                .map(|enumeration| enumeration.is_flag)
                .or_else(|| ty.enums.iter().find(of_flags).map(|_| false))
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
    /// Assigning `undefined` takes the property back to the state it had
    /// before anything gave it a value. False in a table written before
    /// this was recorded.
    pub has_reset: bool,
    /// The value is a `Component`: what is written there is a template that
    /// is instantiated later (a `delegate`), not an object that exists.
    pub is_component: bool,
    /// What is written there is run when its time comes and is worth nothing
    /// until then: the `script` of a `ScriptAction`.
    pub is_script: bool,
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

/// Whether some module of the table has a type called `name`.
pub fn is_type_name(name: &str) -> bool {
    static NAMES: OnceLock<HashSet<&'static str>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            let modules = table().modules.values();
            modules
                .flat_map(|module| module.exports.keys().copied())
                .collect()
        })
        .contains(name)
}

/// Whether some type of the table has a member called `name`: a name that
/// is most likely that member wherever it is written.
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

impl Table {
    /// The type `name` is in a file that imports the module `uri`, by its
    /// index: one of the module's own or of a module that comes with it.
    fn type_named(&self, uri: &str, name: &str, seen: &mut Vec<&'static str>) -> Option<usize> {
        let module = self.modules.get(uri)?;
        if seen.contains(&module.uri) {
            return None;
        }
        seen.push(module.uri);
        if let Some(index) = module.exports.get(name) {
            return Some(*index);
        }
        let chooses = module
            .imports
            .iter()
            .any(|import| import.kind == ImportKind::Optional);
        module
            .imports
            .iter()
            .filter(|import| match import.kind {
                ImportKind::Always => true,
                ImportKind::Default => chooses,
                ImportKind::Optional => false,
            })
            .find_map(|import| self.type_named(import.module, name, seen))
    }

    /// The type a description means by `Module.Name`: what `Name` is in a
    /// file that imports `Module`.
    fn type_meant(&self, named: &str) -> Option<usize> {
        let (uri, name) = named.rsplit_once('.')?;
        self.type_named(uri, name, &mut Vec::new())
    }
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| read(TEXT, OWN))
}

/// Every module Ljós implements, in the table's format. Committed, and
/// nothing in it comes from Qt: it is what Ljós's native types and the QML
/// files of its style declare, kept so by
/// `crates/ljos_cli/tests/own_types.rs` (see the crate's documentation).
const OWN: &str = include_str!("ljos.txt");

/// Ljós's own description of its types, as committed, whatever Qt's table
/// says of the same modules. [`module`] and the lookups answer from Qt's
/// description where there is one, so a tool that asks what *Ljós* has
/// (`ljos check --port`, `api_status`) reads this text itself.
pub fn own_text() -> &'static str {
    OWN
}

/// The classes of the types the front end knows by what they are, in each
/// table: Qt's class, and the one of Ljós's own description.
pub mod class {
    pub const COMPONENT: [&str; 2] = ["QQmlComponent", "QtQml.Component"];
    pub const CONNECTIONS: [&str; 2] = ["QQmlConnections", "QtQml.Connections"];
    pub const PROPERTY_CHANGES: [&str; 2] = ["QQuickPropertyChanges", "QtQuick.PropertyChanges"];
    pub const ANCHOR_CHANGES: [&str; 2] = ["QQuickAnchorChanges", "QtQuick.AnchorChanges"];
    pub const LIST_ELEMENT: [&str; 2] = ["QQmlListElement", "QtQml.Models.ListElement"];
    pub const STRING: [&str; 2] = ["QString", "QtQml.string"];
    pub const URL: [&str; 2] = ["QUrl", "QtQml.url"];
}

/// Reads Qt's table, then of Ljós's own the modules Qt's does not describe
/// (see "Which one is read" in the crate's documentation).
fn read(qt: &'static str, own: &'static str) -> Table {
    let mut table = Table::default();
    // Classes are named before they are described, so what refers to one is
    // looked up once all are read.
    let mut exports = Vec::new();
    let mut links = Vec::new();
    // What is `named` is a QML name, which only the exports can turn into
    // a class: the properties to look up once those are read.
    let mut named = Vec::new();
    let mut module = "";
    // A module of the own table that Qt's has described already is passed
    // over, with everything in it.
    let mut passed_over = false;

    let lines = |text: &'static str, is_own| text.lines().map(move |line| (line, is_own));
    for (line, is_own) in lines(qt, false).chain(lines(own, true)) {
        let depth = line.len() - line.trim_start_matches('\t').len();
        let mut words = line.split_ascii_whitespace();
        let mut word = || words.next().unwrap_or("");
        match (depth, word()) {
            (0, "module") => {
                module = word();
                passed_over = is_own && table.modules.contains_key(module);
                if passed_over {
                    continue;
                }
                let empty = Module {
                    uri: module,
                    imports: Vec::new(),
                    is_own,
                    exports: HashMap::new(),
                };
                table.modules.insert(module, empty);
            }
            (0, _) => {}
            _ if passed_over => {}
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
                    is_own,
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
                    "prototype" | "attached" => {
                        let to = word();
                        links.push((index, member, to, word() == "named"));
                    }
                    "extension" => {
                        links.push((index, member, word(), false));
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
                        if flags.contains(&"named") {
                            named.push((index, ty.properties.len()));
                        }
                        ty.properties.push(Property {
                            name,
                            type_name,
                            is_list: flags.contains(&"list"),
                            is_readonly: flags.contains(&"readonly"),
                            is_pointer: flags.contains(&"pointer"),
                            is_required: flags.contains(&"required"),
                            has_reset: flags.contains(&"reset"),
                            is_component: class::COMPONENT.contains(&type_name),
                            // Qt's table has no word for it: there it is
                            // the property of that name and type.
                            is_script: flags.contains(&"script")
                                || (type_name == "QQmlScriptString" && name == "script"),
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
    for (index, at) in named {
        let meant = table.type_meant(table.types[index].properties[at].type_name);
        // A name nothing has stays as it is written: a value of no known
        // type, as a class the table does not describe is.
        if let Some(class) = meant.map(|index| table.types[index].class) {
            let property = &mut table.types[index].properties[at];
            property.type_name = class;
            property.is_component = class::COMPONENT.contains(&class);
        }
    }
    for (index, link, class, named) in links {
        let to = if named {
            table.type_meant(class)
        } else {
            table.classes.get(class).copied()
        };
        let ty = &mut table.types[index];
        match link {
            "prototype" => ty.prototype = to,
            "extension" => ty.extension = to,
            _ => ty.attached = to,
        }
    }
    table
}
