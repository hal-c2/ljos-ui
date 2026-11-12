//! The declarations of Ljós's API for TypeScript, written from the table of
//! types the checker reads (docs/DIALECT.md §4.4), so that a member renamed
//! in the table is renamed for TypeScript as well.
//!
//! What a QML type is in TypeScript is decided once, in [`Api::ts_type`]:
//! `bool` is `boolean`, a number of any kind `number`, a string, a URL or a
//! colour `string`, `var` `unknown`, a value type an interface of readonly
//! fields, an object type an interface extending what it inherits, an enum a
//! TypeScript `enum` in a namespace named after its type, and a list
//! `readonly T[]`. A date is not a value of the dialect: a member that holds
//! one is not declared.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

use qt_types::{Enum, ImportKind, Property, Type};

/// The module the dialect is written against.
pub const API: &str = "Ljos.Ui";

/// The modules of Ljós's API: the items, what is not an item, and what
/// arranges items.
pub const MODULES: &[&str] = &[API, "Ljos.Core", "Ljos.Layout"];

/// The modules of Ljós's API `source` imports, by their own names, in the
/// order of [`MODULES`]. An import under an alias counts: the file sees the
/// module, as the checker's `is_dialect` does.
pub fn modules_of(source: &str) -> Vec<&'static str> {
    let allocator = qml_syntax::Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    MODULES
        .iter()
        .copied()
        .filter(|uri| {
            parsed.document.imports.iter().any(|import| {
                matches!(&import.source, qml_syntax::ast::QmlImportSource::Module(name) if name.to_string() == *uri)
            })
        })
        .collect()
}

/// What [`Api::written_type`] makes of a type a file writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Written {
    /// The declarations' type for it.
    Declared(String),
    /// A word the declarations do not have. Written as it is, TypeScript
    /// cannot find it, as the checker cannot find a type nothing the file
    /// imports names; a type of the project's own, which the checker does
    /// find, is the caller's to write `unknown`.
    Unnamed(String),
}

/// Names of QML (and of the C++ classes Qt's table calls them by) that are a
/// TypeScript primitive.
const PRIMITIVES: &[(&str, &str)] = &[
    ("bool", "boolean"),
    ("boolean", "boolean"),
    ("int", "number"),
    ("uint", "number"),
    ("real", "number"),
    ("double", "number"),
    ("float", "number"),
    ("number", "number"),
    ("qreal", "number"),
    ("qint64", "number"),
    ("qlonglong", "number"),
    ("quint64", "number"),
    ("short", "number"),
    ("ushort", "number"),
    ("uchar", "number"),
    ("string", "string"),
    ("QString", "string"),
    ("url", "string"),
    ("QUrl", "string"),
    ("color", "string"),
    ("QColor", "string"),
    ("var", "unknown"),
    ("variant", "unknown"),
    ("unknown", "unknown"),
    ("QVariant", "unknown"),
    ("QJSValue", "unknown"),
    ("QVariantMap", "unknown"),
    ("QVariantList", "unknown"),
    ("QObject", "QtObject"),
    ("void", "void"),
];

/// Names that are a date, which the dialect leaves out.
const DATES: &[&str] = &["date", "QDateTime", "QDate", "QTime"];

/// Words a declaration cannot be named: a key of an enum so named is
/// reached through its enum only.
const RESERVED: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "let",
    "static",
    "yield",
    "await",
    "implements",
    "interface",
    "package",
    "private",
    "protected",
    "public",
];

/// The QML name of a primitive, as a TypeScript type.
pub fn primitive(name: &str) -> Option<&'static str> {
    let short = name.rsplit(['.', ':']).next().unwrap_or(name);
    PRIMITIVES
        .iter()
        .find(|(qml, _)| *qml == name || *qml == short)
        .map(|(_, ts)| *ts)
}

fn is_date(name: &str) -> bool {
    let short = name.rsplit(['.', ':']).next().unwrap_or(name);
    DATES.contains(&short)
}

/// The types of the API and what each is called in TypeScript.
#[derive(Debug)]
pub struct Api {
    /// Every type declared, by its class, with its TypeScript name.
    names: HashMap<&'static str, String>,
    /// The same types, in the order they are written.
    types: Vec<&'static Type>,
    /// The type of the QML name, of the API's module and those it imports.
    exported: HashMap<&'static str, &'static Type>,
    /// `Qt`, whose enums `Qt::Alignment` names.
    qt: Option<&'static Type>,
    /// Whether the modules are or bring `Ljos.Core`, whose globals are then
    /// declared.
    core: bool,
}

impl Api {
    /// The API of Ljós's [`MODULES`]: their types, those of the modules
    /// they import and every type one of them reaches through what it
    /// inherits, holds or attaches. None when the table has no such module.
    pub fn new() -> Option<Self> {
        Self::of_all(MODULES)
    }

    /// The API a file that imports `uri` is written against.
    pub fn of(uri: &str) -> Option<Self> {
        Self::of_all(&[uri])
    }

    /// The API a file that imports every one of `modules` is written
    /// against.
    pub fn of_all(modules: &[&str]) -> Option<Self> {
        for uri in modules {
            qt_types::module(uri)?;
        }
        let (mut exported, mut roots, seen) = exports(modules);
        let core = seen.contains(CORE);
        if core {
            // What `Ljos.Core`'s makers make: QtQml's values, and QtQuick's
            // vectors and `quaternion`.
            let modules = ["QtQml", "QtQuick"].map(qt_types::module);
            roots.extend(
                CORE_VALUES
                    .iter()
                    .filter_map(|name| modules.iter().flatten().find_map(|m| m.type_named(name))),
            );
        }
        // `Qt` is a global of every file, whichever modules it imports.
        if let Some(qt) = qt_types::module("QtQml").and_then(|m| m.type_named("Qt"))
            && !exported.contains_key(qt.name)
        {
            exported.insert(qt.name, qt);
            roots.push(qt);
        }
        let mut reached: BTreeMap<&'static str, &'static Type> = BTreeMap::new();
        let mut todo = roots;
        while let Some(ty) = todo.pop() {
            if reached.insert(ty.class, ty).is_some() || ty.qml_file.is_some() && !ty.is_own {
                continue;
            }
            todo.extend(ty.prototype());
            todo.extend(ty.member_extension());
            todo.extend(ty.attached());
            for property in &ty.properties {
                todo.extend(property.value_type());
            }
        }
        // A primitive is no interface.
        reached.retain(|_, ty| !(ty.is_value && primitive(ty.name).is_some()) && !is_date(ty.name));
        let (elsewhere, ..) = exports(MODULES);
        let mut by_name: HashMap<&str, Vec<&'static Type>> = HashMap::new();
        for ty in reached.values() {
            by_name.entry(ty.name).or_default().push(ty);
        }
        let names = reached
            .values()
            .map(|ty| {
                let claimed = exported.get(ty.name).is_some_and(|e| e.class == ty.class);
                // A type another module of the API exports is named only by
                // a file that imports that module: here it goes by its class,
                // so a file that writes `QtObject` without `Ljos.Core` is told
                // TypeScript cannot find it, as the checker tells it.
                let alone = by_name[ty.name].len() == 1
                    && !exported.contains_key(ty.name)
                    && !elsewhere.contains_key(ty.name);
                let name = if !ty.name.is_empty() && is_word(ty.name) && (claimed || alone) {
                    ty.name.to_string()
                } else {
                    sanitized(ty.class)
                };
                (ty.class, name)
            })
            .collect();
        let qt = exported.get("Qt").copied();
        let mut types: Vec<&'static Type> = reached.into_values().collect();
        let names: HashMap<&'static str, String> = names;
        types.sort_by(|a, b| names[a.class].cmp(&names[b.class]));
        Some(Self {
            names,
            types,
            exported,
            qt,
            core,
        })
    }

    /// The type a file names `name`.
    pub fn named(&self, name: &str) -> Option<&'static Type> {
        self.exported.get(name).copied()
    }

    /// Whether `ty`, or what it inherits, has a member called `name`: the
    /// same members the declarations of `ty` and its ancestors are written
    /// from, so a file's `Omit` of them matches what is declared.
    pub fn has_member(&self, ty: &'static Type, name: &str) -> bool {
        members_of(ty).any(|member| member == name) || self.inherited(ty).contains(name)
    }

    /// What TypeScript calls the type.
    pub fn name_of(&self, ty: &Type) -> Option<&str> {
        self.names.get(ty.class).map(String::as_str)
    }

    /// The TypeScript type of a value of the QML type called `name` (a QML
    /// name or a class), seen from `owner`, whose enums it may name. None
    /// for a date.
    pub fn ts_type(&self, name: &str, owner: Option<&'static Type>) -> Option<String> {
        if is_date(name) {
            return None;
        }
        Some(
            self.resolved(name, owner)
                .unwrap_or_else(|| "unknown".to_string()),
        )
    }

    /// The TypeScript type of a declaration the file writes as `name`: the
    /// declarations' type for it, or [`Written::Unnamed`] for a word they do
    /// not have, or `unknown` for a value type every file may write (`font`).
    /// None for a date.
    pub fn written_type(&self, name: &str) -> Option<Written> {
        if is_date(name) {
            return None;
        }
        if let Some(found) = self.resolved(name, None) {
            return Some(Written::Declared(found));
        }
        let is_value = ["QtQuick", "QtQml"]
            .iter()
            .filter_map(|uri| qt_types::module(uri)?.type_named(name))
            .any(|ty| ty.is_value);
        if is_word(name) && !is_value {
            return Some(Written::Unnamed(name.to_string()));
        }
        Some(Written::Declared("unknown".to_string()))
    }

    fn resolved(&self, name: &str, owner: Option<&'static Type>) -> Option<String> {
        // Only Qt's description names a type by a bare class. A file, and
        // Ljós's own table, name it as QML does, and beside Qt's table a
        // QML name can be another type's class: `QtObject` is the class of
        // the `Qt` singleton there.
        if owner.is_some_and(|owner| !owner.is_own)
            && let Some(ty) = qt_types::type_of_class(name)
            && let Some(found) = self.object_type(ty)
        {
            return Some(found);
        }
        if let Some(ts) = primitive(name) {
            return Some(ts.to_string());
        }
        if let Some(ty) = self.named(name)
            && let Some(found) = self.object_type(ty)
        {
            return Some(found);
        }
        // A type handed to the file that it cannot name is still declared
        // where what it does import reaches it: the `QtObject` a
        // `PointerArea` gives a handler in a file of `Ljos.Ui` alone. A type
        // the file writes (no `owner`) is one it names, or none.
        if owner.is_some()
            && let Some(ty) = self.types.iter().find(|ty| ty.name == name)
            && let Some(found) = self.object_type(ty)
        {
            return Some(found);
        }
        self.enum_type(name, owner)
    }

    fn object_type(&self, ty: &'static Type) -> Option<String> {
        if is_date(ty.name) {
            return None;
        }
        if ty.is_value
            && let Some(ts) = primitive(ty.name)
        {
            return Some(ts.to_string());
        }
        if is_component(ty) {
            return Some(self.component());
        }
        self.name_of(ty).map(str::to_string)
    }

    /// What the declarations call QtQml's `Component`: by its name where
    /// the file imports `Ljos.Core`, by its class where it reaches it only
    /// through what it does import (a `ListView`'s delegate in a file of
    /// `Ljos.Ui` alone).
    fn component(&self) -> String {
        qt_types::module("QtQml")
            .and_then(|m| m.type_named("Component"))
            .and_then(|ty| self.name_of(ty))
            .unwrap_or("unknown")
            .to_string()
    }

    /// The enum `name` names, as `Owner.Enum`: one of `owner`'s or of what it
    /// inherits, or `Qt::Name`. Flags are numbers: what `|` makes of keys.
    fn enum_type(&self, name: &str, owner: Option<&'static Type>) -> Option<String> {
        let (scope, short) = match name.rsplit_once("::") {
            Some((scope, short)) => (Some(scope), short),
            None => (None, name),
        };
        let mut ty = match scope {
            Some("Qt") => self.qt,
            Some(scope) => self.named(scope).or(owner),
            None => owner,
        };
        while let Some(found) = ty {
            let enumeration = enums_of(found).find(|e| e.name == short || e.alias == Some(short));
            if let Some(enumeration) = enumeration {
                if enumeration.is_flag || enumeration.alias == Some(short) {
                    return Some("number".to_string());
                }
                return Some(format!("{}.{}", self.name_of(found)?, enumeration.name));
            }
            ty = found.prototype();
        }
        None
    }

    /// The parameters of `signal`, as TypeScript writes them.
    pub fn parameters(&self, signal: &qt_types::Signal, owner: &'static Type) -> String {
        signal
            .parameters
            .iter()
            .enumerate()
            .map(|(i, param)| {
                let ty = signal
                    .parameter_types
                    .get(i)
                    .and_then(|ty| self.ts_type(ty, Some(owner)))
                    .unwrap_or_else(|| "unknown".to_string());
                format!("{}: {ty}", param_name(param, i))
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn property_type(&self, property: &Property, owner: &'static Type) -> Option<String> {
        let ty = match property.value_type() {
            Some(value) => self
                .object_type(value)
                .unwrap_or_else(|| "unknown".to_string()),
            None if is_date(property.type_name) => return None,
            None => self.ts_type(property.type_name, Some(owner))?,
        };
        let ty = if property.is_component {
            self.component()
        } else {
            ty
        };
        Some(if property.is_list {
            format!("readonly {}[]", wrapped(&ty))
        } else {
            ty
        })
    }

    /// The declarations, in one file TypeScript reads as a script: the API's
    /// types are global, as a QML file's are.
    pub fn declarations(&self) -> String {
        let mut out = String::new();
        out.push_str(HEADER);
        out.push_str(PRELUDE);
        if self.core {
            out.push_str(CORE_PRELUDE);
        }
        for ty in &self.types {
            self.write_type(&mut out, ty);
        }
        out
    }

    /// The members each declared ancestor of `ty` has, which `ty` cannot
    /// declare again with another type.
    fn inherited(&self, ty: &'static Type) -> HashSet<&'static str> {
        let mut names = HashSet::new();
        let mut at = ty.prototype();
        while let Some(found) = at {
            names.extend(members_of(found));
            at = found.prototype();
        }
        names
    }

    /// The properties `ty` inherits that hold something else on it than on
    /// its base, with what they hold on it: an enum `ty` declares again
    /// (`TextEdit.HAlignment`, which justifies, for the
    /// `horizontalAlignment` of the base both editors share). An interface
    /// cannot declare an inherited member with another type, so `ty`
    /// extends its base without them.
    fn held_again(&self, ty: &'static Type) -> Vec<(&'static Property, String)> {
        let Some(base) = ty.prototype() else {
            return Vec::new();
        };
        let own: HashSet<&str> = members_of(ty).collect();
        let mut seen = HashSet::new();
        let mut again = Vec::new();
        let mut at = Some(base);
        while let Some(found) = at {
            let parts = [Some(found), found.member_extension()];
            for property in parts.into_iter().flatten().flat_map(|t| &t.properties) {
                if own.contains(property.name) || !seen.insert(property.name) {
                    continue;
                }
                // Only the enum is read from the type that has the member;
                // what else it holds is named alike on both.
                let held = self.enum_type(property.type_name, Some(ty));
                if held.is_none() || held == self.enum_type(property.type_name, Some(base)) {
                    continue;
                }
                if let Some(here) = self.property_type(property, ty) {
                    again.push((property, here));
                }
            }
            at = found.prototype();
        }
        again
    }

    /// The keys of the enums `ty` inherits and does not declare again,
    /// that `ty`'s namespace does not have of its own, by the nearest type
    /// that declares each enum, named as the declarations name it: what a
    /// file reads through `ty`'s name (`TextInput.AlignRight`, of the base
    /// both editors share). Empty for a type no file names (`QQmlLocale`,
    /// reached as what `Locale` inherits), which is read through no name.
    fn inherited_keys(
        &self,
        ty: &'static Type,
        own: &[(&'static str, &'static Enum)],
    ) -> Vec<(&str, Vec<&str>)> {
        if self
            .exported
            .get(ty.name)
            .is_none_or(|e| e.class != ty.class)
        {
            return Vec::new();
        }
        let ancestors: Vec<Ancestor<'_>> =
            std::iter::successors(ty.prototype(), |found| found.prototype())
                .filter_map(|found| {
                    Some(Ancestor {
                        name: self.name_of(found)?,
                        enums: enums_of(found).map(|e| e.name).collect(),
                        keys: own_keys(found)
                            .into_iter()
                            .map(|(key, enumeration)| (key, enumeration.name))
                            .collect(),
                    })
                })
                .collect();
        let namespace = Namespace {
            enums: enums_of(ty).map(|e| e.name).collect(),
            keys: own.iter().map(|(key, _)| *key).collect(),
            // What else the namespace declares: a singleton's members.
            members: if ty.is_singleton {
                let properties = ty.properties.iter().map(|p| p.name);
                properties.chain(ty.methods.iter().copied()).collect()
            } else {
                HashSet::new()
            },
            properties: ty.properties.iter().map(|p| p.name).collect(),
        };
        held_keys(namespace, &ancestors)
    }

    fn write_type(&self, out: &mut String, ty: &'static Type) {
        let Some(name) = self.name_of(ty) else {
            return;
        };
        let generic = is_component(ty);
        let base = ty.prototype().and_then(|p| self.name_of(p));
        let again = self.held_again(ty);
        let extends = match base {
            Some(p) if !again.is_empty() => {
                let names: Vec<String> = again
                    .iter()
                    .map(|(property, _)| format!("\"{}\"", property.name))
                    .collect();
                format!(" extends Omit<{p}, {}>", names.join(" | "))
            }
            Some(p) => format!(" extends {p}"),
            None => String::new(),
        };
        let head = if generic {
            let made = base.unwrap_or("unknown");
            format!("interface {name}<T = {made}>{extends} {{")
        } else {
            format!("interface {name}{extends} {{")
        };
        let _ = writeln!(out, "\n/** {} */\n{head}", ty.class);
        let inherited = self.inherited(ty);
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let own: Vec<&'static Type> = [Some(ty), ty.member_extension()]
            .into_iter()
            .flatten()
            .collect();
        for part in &own {
            for property in &part.properties {
                if inherited.contains(property.name) || !seen.insert(property.name) {
                    continue;
                }
                let Some(ts) = self.property_type(property, ty) else {
                    continue;
                };
                let readonly = if property.is_readonly || ty.is_value {
                    "readonly "
                } else {
                    ""
                };
                let _ = writeln!(out, "    {readonly}{}: {ts};", member(property.name));
            }
        }
        for (property, ts) in &again {
            let readonly = if property.is_readonly {
                "readonly "
            } else {
                ""
            };
            let _ = writeln!(out, "    {readonly}{}: {ts};", member(property.name));
        }
        if generic {
            out.push_str("    createObject(parent: Item, props?: Partial<T>): T;\n");
            seen.insert("createObject");
        }
        for part in &own {
            for signal in &part.signals {
                if inherited.contains(signal.name) || !seen.insert(signal.name) {
                    continue;
                }
                let params = self.parameters(signal, ty);
                let _ = writeln!(out, "    {}({params}): void;", member(signal.name));
            }
        }
        for part in &own {
            for method in &part.methods {
                if inherited.contains(method) || !seen.insert(method) {
                    continue;
                }
                // The table has no method's signature.
                let _ = writeln!(
                    out,
                    "    {}(...args: readonly unknown[]): unknown;",
                    member(method)
                );
            }
        }
        out.push_str("}\n");
        self.write_namespace(out, ty, name);
    }

    /// The type's enums, and what a singleton is: a namespace named as the
    /// interface is, which TypeScript merges with it.
    fn write_namespace(&self, out: &mut String, ty: &'static Type, name: &str) {
        let enums: Vec<&Enum> = enums_of(ty).collect();
        let own = own_keys(ty);
        let inherited = self.inherited_keys(ty, &own);
        if enums.is_empty() && inherited.is_empty() && !ty.is_singleton {
            return;
        }
        let _ = writeln!(out, "declare namespace {name} {{");
        for enumeration in &enums {
            let _ = write!(out, "    enum {} {{", enumeration.name);
            let mut first = true;
            let mut written = HashSet::new();
            for (key, number) in &enumeration.keys {
                if !written.insert(*key) {
                    continue;
                }
                out.push_str(if first { " " } else { ", " });
                first = false;
                match number {
                    Some(number) => {
                        let _ = write!(out, "{key} = {number}");
                    }
                    None => out.push_str(key),
                }
            }
            out.push_str(" }\n");
        }
        // `Text.AlignLeft`, as QML writes a key, as well as
        // `Text.HAlignment.AlignLeft`.
        for (key, enumeration) in &own {
            // An enum named as its type (`Flow.Flow`) hides the namespace
            // inside it, so it is named alone there.
            if enumeration.name == name {
                let _ = writeln!(out, "    const {key}: {name};");
            } else {
                let _ = writeln!(out, "    const {key}: {name}.{};", enumeration.name);
            }
        }
        // A namespace cannot extend another, so each inherited key is
        // declared again; taken from the namespace that declares it, so
        // that it is named once rather than typed again (every item has
        // `Item`'s keys, `Locale` the hundreds of `QLocale`'s).
        for (owner, held) in &inherited {
            let _ = writeln!(out, "    const {{ {} }}: typeof {owner};", held.join(", "));
        }
        let keys: HashSet<&str> = own
            .iter()
            .map(|(key, _)| *key)
            .chain(inherited.iter().flat_map(|(_, held)| held.iter().copied()))
            .collect();
        if ty.is_singleton {
            for property in &ty.properties {
                if keys.contains(property.name) || RESERVED.contains(&property.name) {
                    continue;
                }
                let Some(ts) = self.property_type(property, ty) else {
                    continue;
                };
                let binding = if property.is_readonly { "const" } else { "let" };
                let _ = writeln!(out, "    {binding} {}: {ts};", property.name);
            }
            for method in &ty.methods {
                if keys.contains(method) || RESERVED.contains(method) {
                    continue;
                }
                let _ = writeln!(
                    out,
                    "    function {method}(...args: readonly unknown[]): unknown;"
                );
            }
        }
        out.push_str("}\n");
    }
}

const HEADER: &str = "\
// Ljós's API for TypeScript: what a file that imports `Ljos.Ui` reads and
// writes. Written by `ljos types --dts` from the table of types; do not edit.
";

/// What QML gives every script, of what the dialect allows (docs/DIALECT.md
/// §4.3), and the names a QML function's parameters are typed with.
const PRELUDE: &str = "
type real = number;
type double = number;
type int = number;
type bool = boolean;
type color = string;
type url = string;
type variant = unknown;

declare const console: {
    log(...data: readonly unknown[]): void;
    warn(...data: readonly unknown[]): void;
    error(...data: readonly unknown[]): void;
};
declare function qsTr(text: string, comment?: string, n?: number): string;
declare function qsTrId(id: string, n?: number): string;
/** A Controller a module exports (docs/DIALECT.md §4.20): `self` is the
 * Controller, its properties and what its bindings give. */
declare function controller<P extends object = {}, B extends object = {}>(spec: {
    readonly properties?: P;
    readonly bindings?: { readonly [K in keyof B]: (host: Item, self: P & Readonly<B>) => B[K] };
    attached?(host: Item, self: P & Readonly<B>): void | (() => void);
    detached?(host: Item, self: P & Readonly<B>): void;
    updated?(host: Item, self: P & Readonly<B>): void;
}): unknown;

interface String {
    /** QML's: the lowest `%n` of the text replaced by `value`. */
    arg(value: string | number | boolean): string;
}
";

const CORE: &str = "Ljos.Core";

/// The value types `Ljos.Core`'s makers make.
const CORE_VALUES: &[&str] = &[
    "point",
    "size",
    "rect",
    "vector2d",
    "vector3d",
    "vector4d",
    "quaternion",
];

/// What `Ljos.Core` gives a file that imports it in place of Qt's `Qt`
/// (docs/DIALECT.md §4.3). TypeScript reads `color` as `string`, so a
/// colour's own members are declared on `String`; the checker holds them
/// to colours.
const CORE_PRELUDE: &str = "
declare function later(f: (...args: readonly unknown[]) => unknown, ...args: readonly unknown[]): void;
declare function point(x: number, y: number): point;
declare function size(width: number, height: number): size;
declare function rect(x: number, y: number, width: number, height: number): rect;
declare function vector2d(x: number, y: number): vector2d;
declare function vector3d(x: number, y: number, z: number): vector3d;
declare function vector4d(x: number, y: number, z: number, w: number): vector4d;
declare function quaternion(scalar: number, x: number, y: number, z: number): quaternion;
declare function rgba(r: number, g: number, b: number, a?: number): color;
declare function hsla(h: number, s: number, l: number, a?: number): color;
declare function hsva(h: number, s: number, v: number, a?: number): color;
/** The address `url` names, from the file that asks. */
declare function url(url: string): url;
declare const App: {
    readonly platform: { readonly os: string; readonly pluginName: string };
    openUrlExternally(url: url): boolean;
    quit(): void;
};

interface String {
    /** A colour's: its channels, from 0 to 1. */
    readonly r: number;
    readonly g: number;
    readonly b: number;
    readonly a: number;
    /** A colour's: the same colour, darker by `factor` (2 by default). */
    darker(factor?: number): color;
    /** A colour's: the same colour, lighter by `factor` (1.5 by default). */
    lighter(factor?: number): color;
    /** A colour's: the same colour with the alpha `alpha`. */
    alpha(alpha: number): color;
    /** A colour's: `tint` over it. */
    tint(tint: color): color;
}
";

/// Whether `ty` is QtQml's `Component`, which the declarations make generic
/// in what it creates.
fn is_component(ty: &Type) -> bool {
    ty.name == "Component" && qt_types::module("QtQml").is_some_and(|qml| qml.has(ty))
}

/// The names of the members a type declares itself: its own and those of
/// the class that extends it with members.
fn members_of(ty: &'static Type) -> impl Iterator<Item = &'static str> {
    [Some(ty), ty.member_extension()]
        .into_iter()
        .flatten()
        .flat_map(|own| {
            let properties = own.properties.iter().map(|p| p.name);
            let signals = own.signals.iter().map(|s| s.name);
            properties.chain(signals).chain(own.methods.iter().copied())
        })
}

/// What a type's namespace declares before the keys it inherits, by name.
struct Namespace<'a> {
    enums: HashSet<&'a str>,
    keys: HashSet<&'a str>,
    /// A singleton's properties and methods, written into the namespace.
    members: HashSet<&'a str>,
    /// The type's properties, which no key is named as.
    properties: HashSet<&'a str>,
}

/// A type `ty` inherits from, nearest first, as the declarations name it.
struct Ancestor<'a> {
    name: &'a str,
    /// Every enum a file reaches through its name.
    enums: Vec<&'a str>,
    /// Its own keys, each with the enum that has it.
    keys: Vec<(&'a str, &'a str)>,
}

/// The keys a namespace takes from each ancestor, by the nearest one that
/// declares their enum. Inside the namespace a name it declares (an enum,
/// a key, a singleton's member) named as an ancestor hides that ancestor,
/// so `typeof` it would not be the ancestor: its keys are left out, and so
/// is a key named as an ancestor.
fn held_keys<'a>(
    mut namespace: Namespace<'a>,
    ancestors: &[Ancestor<'a>],
) -> Vec<(&'a str, Vec<&'a str>)> {
    let mut inherited = Vec::new();
    for ancestor in ancestors {
        let hidden = namespace.enums.contains(ancestor.name)
            || namespace.keys.contains(ancestor.name)
            || namespace.members.contains(ancestor.name);
        let held: Vec<&'a str> = ancestor
            .keys
            .iter()
            .filter(|&&(key, enumeration)| {
                !hidden
                    && !namespace.enums.contains(enumeration)
                    && !namespace.properties.contains(key)
                    && !ancestors.iter().any(|a| a.name == key)
                    && namespace.keys.insert(key)
            })
            .map(|&(key, _)| key)
            .collect();
        namespace.enums.extend(ancestor.enums.iter().copied());
        if !held.is_empty() {
            inherited.push((ancestor.name, held));
        }
    }
    inherited
}

/// The keys `ty`'s namespace declares of its own enums, each with the
/// first enum that has it. A key named as a property of the type is not
/// one, nor is a word TypeScript reserves.
fn own_keys(ty: &'static Type) -> Vec<(&'static str, &'static Enum)> {
    let mut keys = HashSet::new();
    enums_of(ty)
        .flat_map(|enumeration| {
            enumeration
                .keys
                .iter()
                .map(move |(key, _)| (*key, enumeration))
        })
        .filter(|(key, _)| {
            !RESERVED.contains(key)
                && is_word(key)
                && !ty.properties.iter().any(|p| p.name == *key)
                && keys.insert(*key)
        })
        .collect()
}

/// The enums a file reaches through `ty`'s name: its own, then those of
/// its extension that it has no enum of the same name for (`Qt.Key_W`,
/// whose enum beside Qt's table is the namespace's that extends `Qt`).
fn enums_of(ty: &'static Type) -> impl Iterator<Item = &'static Enum> {
    let extension = ty.enum_extension().into_iter().flat_map(|extension| {
        extension
            .enums
            .iter()
            .filter(|e| !ty.enums.iter().any(|own| own.name == e.name))
    });
    ty.enums.iter().chain(extension)
}

fn is_word(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

type Exports<'a> = (
    HashMap<&'static str, &'static Type>,
    Vec<&'static Type>,
    HashSet<&'a str>,
);

/// The names `modules` export, with what they import always, first
/// module's first; the types they name; and the modules reached.
fn exports<'a>(modules: &[&'a str]) -> Exports<'a> {
    // Popped from the end: the first module's names come first.
    let mut uris: Vec<&str> = modules.iter().rev().copied().collect();
    let mut seen = HashSet::new();
    let mut exported: HashMap<&'static str, &'static Type> = HashMap::new();
    let mut roots = Vec::new();
    while let Some(uri) = uris.pop() {
        if !seen.insert(uri) {
            continue;
        }
        let Some(module) = qt_types::module(uri) else {
            continue;
        };
        for name in qt_types::native_types(uri)
            .into_iter()
            .chain(qt_types::written_types(uri))
            .flatten()
        {
            if let Some(ty) = module.type_named(name) {
                // The module's own names come before those it imports.
                exported.entry(name).or_insert(ty);
                roots.push(ty);
            }
        }
        uris.extend(
            module
                .imports
                .iter()
                .filter(|import| import.kind == ImportKind::Always)
                .map(|import| import.module),
        );
    }
    (exported, roots, seen)
}

/// A class as a TypeScript name: `QtQuick.Anchors` is `QtQuick_Anchors`.
fn sanitized(class: &str) -> String {
    class
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// A member's name as a key of an interface.
fn member(name: &str) -> String {
    if is_word(name) {
        name.to_string()
    } else {
        format!("{name:?}")
    }
}

fn param_name(name: &str, index: usize) -> String {
    if is_word(name) && !RESERVED.contains(&name) {
        name.to_string()
    } else {
        format!("arg{index}")
    }
}

/// `T` as the element of an array type.
fn wrapped(ty: &str) -> String {
    if ty.contains([' ', '|', '&']) {
        format!("({ty})")
    } else {
        ty.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_is_declared_with_fill_and_without_color() {
        let declarations = Api::new().expect("Ljos.Ui").declarations();
        let start = declarations
            .find("interface Rectangle extends Item {")
            .expect("Rectangle is declared");
        let body = &declarations[start..];
        let body = &body[..body.find("\n}\n").expect("closed")];
        assert!(body.contains("\n    fill: string;\n"), "{body}");
        assert!(!body.contains("\n    color:"), "{body}");
    }

    #[test]
    fn an_excerpt_is_pinned() {
        // The comment is the class of the description the table has:
        // Ljós's `QtQml.point`, or Qt's own class beside Qt's table, which
        // also gives `point` its `toString`.
        let point = qt_types::module("QtQml")
            .and_then(|m| m.type_named("point"))
            .expect("QtQml's point");
        let head = format!("/** {} */", point.class);
        let declarations = Api::new().expect("Ljos.Ui").declarations();
        let start = declarations.find(&head).expect("point is declared");
        let excerpt: String = declarations[start..]
            .lines()
            .take(4)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            excerpt,
            format!("{head}\ninterface point {{\n    readonly x: number;\n    readonly y: number;")
        );
        if point.is_own {
            assert_eq!(
                declarations[start..].lines().nth(4),
                Some("}"),
                "Ljós's point has x and y alone"
            );
        }
    }

    #[test]
    fn ljos_core_and_the_qt_global_are_declared() {
        let api = Api::new().expect("Ljos.Ui");
        assert_eq!(api.named("Timer").map(|t| t.module), Some("QtQml"));
        let declarations = api.declarations();
        assert!(declarations.contains("\ninterface Timer extends QtObject {"));
        assert!(declarations.contains("\ndeclare namespace Qt {"));
        assert!(declarations.contains("\ndeclare function later("));
        assert!(declarations.contains("\ninterface vector3d {"));
        assert!(declarations.contains("\ndeclare const App: {"));
        let ui = Api::of("Ljos.Ui").expect("the table has Ljos.Ui");
        assert!(!ui.declarations().contains("declare function later("));
        // A file of `Ljos.Ui` alone has neither.
        assert!(Api::of(API).expect("Ljos.Ui").named("Timer").is_none());
    }

    #[test]
    fn a_component_ljos_ui_reaches_is_declared_by_its_class() {
        let component = qt_types::module("QtQml")
            .and_then(|m| m.type_named("Component"))
            .expect("QtQml's Component");
        let class = sanitized(component.class);
        assert_ne!(class, "Component");
        // A file of `Ljos.Ui` alone reaches it through a view's delegate.
        let declarations = Api::of(API).expect("Ljos.Ui").declarations();
        assert!(!declarations.contains("interface Component<"));
        assert!(
            declarations.contains(&format!("\ninterface {class}<T = ")),
            "{class} is not declared"
        );
        assert!(
            declarations.contains(&format!("    delegate: {class};\n")),
            "a delegate is not a {class}"
        );
        // Beside `Ljos.Core`, it goes by its name.
        let declarations = Api::new().expect("Ljos.Ui").declarations();
        assert!(declarations.contains("\ninterface Component<T = QtObject>"));
        assert!(declarations.contains("    delegate: Component;\n"));
    }

    #[test]
    fn an_enum_named_as_its_type_is_named_in_its_namespace() {
        let declarations = Api::of("QtQuick").expect("QtQuick").declarations();
        let start = declarations
            .find("declare namespace Flow {")
            .expect("Flow's enums are declared");
        let body = &declarations[start..];
        let body = &body[..body.find("\n}\n").expect("closed")];
        assert!(body.contains("\n    const LeftToRight: Flow;"), "{body}");
    }

    /// The body of the namespace `name` declares.
    fn namespace<'a>(declarations: &'a str, name: &str) -> &'a str {
        let start = declarations
            .find(&format!("\ndeclare namespace {name} {{"))
            .unwrap_or_else(|| panic!("{name} has no namespace"));
        let body = &declarations[start..];
        &body[..body.find("\n}\n").expect("closed")]
    }

    /// What types `key` in a namespace's body, each time it is declared:
    /// the enum of its own, or the namespace it is taken from.
    fn typed_as<'a>(body: &'a str, key: &str) -> Vec<&'a str> {
        body.lines()
            .filter_map(|line| line.trim().strip_prefix("const "))
            .filter_map(|line| {
                if let Some(held) = line.strip_prefix("{ ") {
                    let (held, from) = held.split_once(" }: ")?;
                    held.split(", ").any(|k| k == key).then_some(from)
                } else {
                    line.strip_prefix(key)?.strip_prefix(": ")
                }
            })
            .map(|ty| ty.trim_end_matches(';'))
            .collect()
    }

    #[test]
    fn a_key_of_an_inherited_enum_is_declared_on_the_type() {
        let declarations = Api::of(API).expect("Ljos.Ui").declarations();
        // Item's, which every item inherits, is taken from Item.
        let rectangle = namespace(&declarations, "Rectangle");
        assert_eq!(
            typed_as(rectangle, "TopLeft"),
            ["typeof Item"],
            "{rectangle}"
        );
        assert!(!rectangle.contains("enum TransformOrigin"), "{rectangle}");
        // A key of an enum the type declares again is its own, once.
        let edit = namespace(&declarations, "TextEdit");
        assert_eq!(
            typed_as(edit, "AlignLeft"),
            ["TextEdit.HAlignment"],
            "{edit}"
        );
        assert_eq!(
            typed_as(edit, "AlignJustify"),
            ["TextEdit.HAlignment"],
            "{edit}"
        );
        assert_eq!(typed_as(edit, "Wrap").len(), 1, "{edit}");
        let input = namespace(&declarations, "TextInput");
        assert_eq!(typed_as(input, "AlignRight").len(), 1, "{input}");
        assert!(typed_as(input, "AlignJustify").is_empty(), "{input}");
    }

    /// `Owner`, which declares `Origin { TopLeft }`, beside a namespace of
    /// a type that inherits it and declares nothing else.
    fn beside_owner<'a>(
        members: &[&'a str],
        keys: &[&'a str],
    ) -> (Namespace<'a>, [Ancestor<'a>; 1]) {
        let namespace = Namespace {
            enums: HashSet::new(),
            keys: keys.iter().copied().collect(),
            members: members.iter().copied().collect(),
            properties: HashSet::new(),
        };
        let owner = Ancestor {
            name: "Owner",
            enums: vec!["Origin"],
            keys: vec![("TopLeft", "Origin")],
        };
        (namespace, [owner])
    }

    #[test]
    fn a_name_the_namespace_declares_as_an_ancestor_hides_its_keys() {
        let (namespace, ancestors) = beside_owner(&[], &[]);
        assert_eq!(
            held_keys(namespace, &ancestors),
            [("Owner", vec!["TopLeft"])]
        );
        // `typeof Owner` inside the namespace would be the singleton's
        // `function Owner()` or `let Owner`, which has no `TopLeft`.
        let (namespace, ancestors) = beside_owner(&["Owner"], &[]);
        assert!(held_keys(namespace, &ancestors).is_empty());
        let (namespace, ancestors) = beside_owner(&[], &["Owner"]);
        assert!(held_keys(namespace, &ancestors).is_empty());
    }

    #[test]
    fn the_keys_of_qt_are_declared_on_qt() {
        // Beside Qt's table they are the namespace's that extends `Qt`.
        let declarations = Api::of(API).expect("Ljos.Ui").declarations();
        let start = declarations
            .find("\ndeclare namespace Qt {")
            .expect("Qt's enums are declared");
        let body = &declarations[start..];
        let body = &body[..body.find("\n}\n").expect("closed")];
        assert!(body.contains("\n    const Key_W: "), "no Qt.Key_W");
        assert!(
            body.contains("\n    const ControlModifier: "),
            "no Qt.ControlModifier"
        );
    }

    #[test]
    fn a_name_a_file_writes_is_a_qml_name_not_a_class() {
        // Beside Qt's table, `QtObject` is the class of the `Qt` singleton;
        // a file of `Ljos.Ui` alone cannot name QtQml's `QtObject` either way.
        let api = Api::of(API).expect("Ljos.Ui");
        assert!(matches!(
            api.written_type("QtObject"),
            Some(Written::Unnamed(word)) if word == "QtObject"
        ));
        // What `PointerArea` hands a handler is QtQml's `QtObject`.
        let area = api.named("PointerArea").expect("PointerArea");
        let clicked = area
            .signals
            .iter()
            .find(|s| s.name == "clicked")
            .expect("clicked");
        let qt = api
            .qt
            .and_then(|qt| api.name_of(qt))
            .expect("Qt is declared");
        assert_ne!(api.parameters(clicked, area), format!("mouse: {qt}"));
        let object = qt_types::module("QtQml")
            .and_then(|m| m.type_named("QtObject"))
            .and_then(|ty| api.name_of(ty))
            .expect("QtObject is declared");
        assert_eq!(api.parameters(clicked, area), format!("mouse: {object}"));
    }

    #[test]
    fn the_declarations_are_written_the_same_each_time() {
        let once = Api::new().expect("Ljos.Ui").declarations();
        let again = Api::new().expect("Ljos.Ui").declarations();
        assert_eq!(once, again);
    }
}
