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

/// The modules of Ljós's API: the items, and what is not an item.
pub const MODULES: &[&str] = &[API, "Ljos.Core"];

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
        let mut by_name: HashMap<&str, Vec<&'static Type>> = HashMap::new();
        for ty in reached.values() {
            by_name.entry(ty.name).or_default().push(ty);
        }
        let names = reached
            .values()
            .map(|ty| {
                let claimed = exported.get(ty.name).is_some_and(|e| e.class == ty.class);
                let alone = by_name[ty.name].len() == 1 && !exported.contains_key(ty.name);
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
        })
    }

    /// The type a file names `name`.
    pub fn named(&self, name: &str) -> Option<&'static Type> {
        self.exported.get(name).copied()
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
        if let Some(ty) = qt_types::type_of_class(name)
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
        // A type the file cannot name is still declared where what it does
        // import reaches it: the `QtObject` a `PointerArea` gives a handler in
        // a file of `Ljos.Ui` alone.
        if let Some(ty) = self.types.iter().find(|ty| self.names[ty.class] == name)
            && let Some(found) = self.object_type(ty)
        {
            return Some(found);
        }
        if let Some(found) = self.enum_type(name, owner) {
            return Some(found);
        }
        Some("unknown".to_string())
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
        if ty.name == "Component" && ty.module == "QtQml" {
            return Some("Component".to_string());
        }
        self.name_of(ty).map(str::to_string)
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
            let enumeration = found
                .enums
                .iter()
                .find(|e| e.name == short || e.alias == Some(short));
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
            "Component".to_string()
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
            for own in [Some(found), found.member_extension()]
                .into_iter()
                .flatten()
            {
                names.extend(own.properties.iter().map(|p| p.name));
                names.extend(own.signals.iter().map(|s| s.name));
                names.extend(own.methods.iter().copied());
            }
            at = found.prototype();
        }
        names
    }

    fn write_type(&self, out: &mut String, ty: &'static Type) {
        let Some(name) = self.name_of(ty) else {
            return;
        };
        let generic = name == "Component" && ty.module == "QtQml";
        let extends = ty
            .prototype()
            .and_then(|p| self.name_of(p))
            .map(|p| format!(" extends {p}"))
            .unwrap_or_default();
        let head = if generic {
            format!("interface Component<T = QtObject>{extends} {{")
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
        let enums: Vec<&Enum> = ty.enums.iter().collect();
        if enums.is_empty() && !ty.is_singleton {
            return;
        }
        let _ = writeln!(out, "declare namespace {name} {{");
        let mut keys = BTreeSet::new();
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
        for enumeration in &enums {
            for (key, _) in &enumeration.keys {
                if RESERVED.contains(key)
                    || !is_word(key)
                    || ty.properties.iter().any(|p| p.name == *key)
                    || !keys.insert(*key)
                {
                    continue;
                }
                let _ = writeln!(out, "    const {key}: {name}.{};", enumeration.name);
            }
        }
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

interface String {
    /** QML's: the lowest `%n` of the text replaced by `value`. */
    arg(value: string | number | boolean): string;
}
";

fn is_word(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
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
        let declarations = Api::new().expect("Ljos.Ui").declarations();
        let start = declarations
            .find("/** QtQml.point */")
            .expect("point is declared");
        let excerpt: String = declarations[start..]
            .lines()
            .take(5)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            excerpt,
            "/** QtQml.point */\ninterface point {\n    readonly x: number;\n    readonly y: number;\n}"
        );
    }

    #[test]
    fn ljos_core_and_the_qt_global_are_declared() {
        let api = Api::new().expect("Ljos.Ui");
        assert_eq!(api.named("Timer").map(|t| t.module), Some("QtQml"));
        let declarations = api.declarations();
        assert!(declarations.contains("\ninterface Timer extends QtObject {"));
        assert!(declarations.contains("\ndeclare namespace Qt {"));
        // A file of `Ljos.Ui` alone has neither.
        assert!(Api::of(API).expect("Ljos.Ui").named("Timer").is_none());
    }

    #[test]
    fn the_declarations_are_written_the_same_each_time() {
        let once = Api::new().expect("Ljos.Ui").declarations();
        let again = Api::new().expect("Ljos.Ui").declarations();
        assert_eq!(once, again);
    }
}
