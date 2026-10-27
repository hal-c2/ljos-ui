//! Explicit type registration ([`module!`](crate::module)) and the type
//! dump that feeds the compiler, the LSP and the API ledger.
//!
//! Registration is explicit, not `inventory` or linker sections (§5.5):
//! the application lists its modules, so what a static iOS binary contains
//! is what the source says.

use rustc_hash::FxHashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::atom::Atom;
use crate::meta::{MetaObject, MetaRef, PropertyAccess, PropertyFlags};
use crate::types::TypeTag;

/// A module version (`2.15`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
}

impl Version {
    pub const fn new(major: u32, minor: u32) -> Self {
        Version { major, minor }
    }

    /// Parses `"2.15"` or `"6"`.
    pub fn parse(s: &str) -> Option<Version> {
        let (major, minor) = s.trim().split_once('.').unwrap_or((s.trim(), "0"));
        Some(Version {
            major: major.parse().ok()?,
            minor: minor.parse().ok()?,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// What [`module!`](crate::module) produces: one module version and the
/// native types it exports.
#[derive(Clone, Debug)]
pub struct ModuleDef {
    pub uri: &'static str,
    pub version: Version,
    pub types: Vec<fn() -> &'static MetaObject>,
}

impl ModuleDef {
    /// `version` as written in `module!` (`2.15`). Panics on a malformed
    /// version, which is a typo in the source.
    pub fn new(uri: &'static str, version: &str, types: Vec<fn() -> &'static MetaObject>) -> Self {
        let version = Version::parse(version)
            .unwrap_or_else(|| panic!("module! {uri}: bad version {version:?}"));
        ModuleDef {
            uri,
            version,
            types,
        }
    }
}

/// Lists the native types of a module version, for
/// [`Engine::register_module`](crate::Engine::register_module).
///
/// ```ignore
/// engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [Item, Rectangle]));
/// ```
#[macro_export]
macro_rules! module {
    ($uri:literal $version:literal => [$($ty:ty),* $(,)?]) => {
        $crate::ModuleDef::new(
            $uri,
            stringify!($version),
            vec![$(<$ty as $crate::QmlType>::static_meta as fn() -> &'static $crate::MetaObject),*],
        )
    };
}

#[derive(Clone, Debug)]
struct Entry {
    meta: Slot,
    /// The module version that introduced the type.
    since: Version,
}

/// What a registered type is known by.
#[derive(Clone, Debug)]
pub(crate) enum Slot {
    Meta(MetaRef),
    /// A type written in QML whose description is made the first time it
    /// is looked up, from the library file of this key: a library
    /// registers many more types than a program uses.
    Qml(Atom),
}

impl Slot {
    fn meta(&self) -> Option<&MetaRef> {
        match self {
            Slot::Meta(m) => Some(m),
            Slot::Qml(_) => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Module {
    versions: Vec<Version>,
    types: FxHashMap<Atom, Entry>,
    order: Vec<Atom>,
}

/// Registered modules and their types.
#[derive(Debug, Default)]
pub struct Registry {
    modules: FxHashMap<Atom, Module>,
    order: Vec<Atom>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers every type of `def` under its module and version. A type
    /// registered again (a later version re-exporting it) keeps its first
    /// version.
    pub fn register_module(&mut self, def: ModuleDef) {
        for ty in def.types {
            self.register_type(def.uri, def.version, MetaRef::Static(ty()));
        }
    }

    /// Registers one type; QML-declared types come through here.
    pub fn register_type(&mut self, uri: &str, version: Version, meta: MetaRef) {
        let name = meta.name.clone();
        self.register_slot(uri, version, name, Slot::Meta(meta));
    }

    /// Registers the type `name`, written in the library file `key`, to be
    /// described when it is first looked up.
    pub(crate) fn register_qml(&mut self, uri: &str, version: Version, name: &str, key: &str) {
        self.register_slot(
            uri,
            version,
            Atom::intern(name),
            Slot::Qml(Atom::intern(key)),
        );
    }

    fn register_slot(&mut self, uri: &str, version: Version, name: Atom, meta: Slot) {
        let uri = Atom::intern(uri);
        let module = self.modules.entry(uri.clone()).or_insert_with(|| {
            self.order.push(uri);
            Module::default()
        });
        if !module.versions.contains(&version) {
            module.versions.push(version);
            module.versions.sort();
        }
        match module.types.get_mut(&name) {
            // Hot reload replaces a QML type's description in place.
            Some(e) => e.meta = meta,
            None => {
                module.order.push(name.clone());
                module.types.insert(
                    name,
                    Entry {
                        meta,
                        since: version,
                    },
                );
            }
        }
    }

    pub fn has_module(&self, uri: &str) -> bool {
        self.modules.contains_key(uri)
    }

    /// The type `name` of module `uri`, at any version. A QML type not yet
    /// described is not found here; the engine's lookups describe it.
    pub fn lookup(&self, uri: &str, name: &str) -> Option<MetaRef> {
        self.slot(uri, name)?.meta().cloned()
    }

    /// The type `name` in whichever module has it, first registered first.
    pub fn lookup_any(&self, name: &str) -> Option<MetaRef> {
        self.slot_any(name)?.meta().cloned()
    }

    /// The type `name` as an `import uri version` sees it: types added in
    /// a later version are hidden.
    pub fn lookup_versioned(&self, uri: &str, version: Version, name: &str) -> Option<MetaRef> {
        self.slot_versioned(uri, version, name)?.meta().cloned()
    }

    pub(crate) fn slot(&self, uri: &str, name: &str) -> Option<Slot> {
        crate::lookups::type_();
        Some(self.modules.get(uri)?.types.get(name)?.meta.clone())
    }

    pub(crate) fn slot_any(&self, name: &str) -> Option<Slot> {
        crate::lookups::type_();
        self.order
            .iter()
            .find_map(|uri| Some(self.modules.get(uri)?.types.get(name)?.meta.clone()))
    }

    pub(crate) fn slot_versioned(&self, uri: &str, version: Version, name: &str) -> Option<Slot> {
        crate::lookups::type_();
        let e = self.modules.get(uri)?.types.get(name)?;
        (e.since <= version).then(|| e.meta.clone())
    }

    /// Every library file whose types are not described yet.
    pub(crate) fn undescribed(&self) -> Vec<Atom> {
        let mut keys: Vec<Atom> = Vec::new();
        for uri in &self.order {
            for name in &self.modules[uri].order {
                if let Slot::Qml(key) = &self.modules[uri].types[name].meta
                    && !keys.contains(key)
                {
                    keys.push(key.clone());
                }
            }
        }
        keys
    }

    /// Gives every type written in `key` its description, or, with `None`
    /// (the file did not load), takes those types away.
    pub(crate) fn describe(&mut self, key: &str, meta: Option<&MetaRef>) {
        for module in self.modules.values_mut() {
            let names: Vec<Atom> = module
                .types
                .iter()
                .filter(|(_, e)| matches!(&e.meta, Slot::Qml(k) if k.as_str() == key))
                .map(|(n, _)| n.clone())
                .collect();
            for name in names {
                match meta {
                    Some(m) => {
                        if let Some(e) = module.types.get_mut(&name) {
                            e.meta = Slot::Meta(m.clone());
                        }
                    }
                    None => {
                        module.types.remove(&name);
                        module.order.retain(|n| *n != name);
                    }
                }
            }
        }
    }

    /// Everything registered, in registration order.
    pub fn dump(&self) -> TypeDump {
        let mut dump = TypeDump {
            format: TypeDump::FORMAT,
            unnamed: Vec::new(),
            modules: self
                .order
                .iter()
                .map(|uri| {
                    let m = &self.modules[uri];
                    ModuleDump {
                        uri: uri.to_string(),
                        versions: m.versions.iter().map(Version::to_string).collect(),
                        types: m
                            .order
                            .iter()
                            .filter_map(|n| {
                                Some(type_entry(m.types[n].meta.meta()?, m.types[n].since))
                            })
                            .collect(),
                    }
                })
                .collect(),
        };
        let metas: Vec<MetaRef> = self
            .order
            .iter()
            .flat_map(|uri| {
                let m = &self.modules[uri];
                m.order
                    .iter()
                    .filter_map(|n| m.types[n].meta.meta().cloned())
            })
            .collect();
        dump.unnamed = reached(&dump, &metas);
        dump
    }
}

/// The group and attached types `dump`'s types reach that no module
/// exports by a name of its own, each once.
fn reached(dump: &TypeDump, metas: &[MetaRef]) -> Vec<TypeEntry> {
    let mut seen: Vec<MetaRef> = Vec::new();
    let mut todo: Vec<MetaRef> = metas.to_vec();
    let mut out = Vec::new();
    while let Some(meta) = todo.pop() {
        let mut next: Vec<MetaRef> = meta
            .own_properties()
            .iter()
            .filter_map(|p| match p.access {
                PropertyAccess::Group { meta, .. } => Some(MetaRef::Static(meta)),
                _ => None,
            })
            .collect();
        next.extend(meta.attached.map(|f| f()));
        for m in next {
            if seen.iter().any(|s| MetaRef::ptr_eq(s, &m)) {
                continue;
            }
            seen.push(m.clone());
            let named = dump
                .modules
                .iter()
                .any(|md| md.types.iter().any(|t| t.name == m.name.as_str()));
            if !named {
                out.push(type_entry(&m, Version::default()));
            }
            todo.push(m);
        }
    }
    out
}

/// A property's type as the dump writes it: its name and what kind of
/// name that is, which the name alone does not say (`TransformOrigin` is
/// an enum, `border` a group, `Item` an object).
fn property_kind(ty: &TypeTag) -> PropertyKind {
    match ty {
        TypeTag::Object(_) => PropertyKind::Object,
        TypeTag::List(_) => PropertyKind::List,
        TypeTag::Enum(_) => PropertyKind::Enum,
        TypeTag::Flags(_) => PropertyKind::Flags,
        TypeTag::Group(_) => PropertyKind::Group,
        _ => PropertyKind::Value,
    }
}

impl PropertyEntry {
    /// The property's type, as the engine knows it; a `var` that a Rust
    /// type reads as data is a `var` here.
    pub fn type_tag(&self) -> TypeTag {
        let name = || Atom::intern(&self.ty);
        match self.kind {
            PropertyKind::Value | PropertyKind::List => TypeTag::parse(&self.ty),
            PropertyKind::Object if self.ty == "QtObject" => TypeTag::Object(None),
            PropertyKind::Object => TypeTag::Object(Some(name())),
            PropertyKind::Enum => TypeTag::Enum(name()),
            PropertyKind::Flags => TypeTag::Flags(name()),
            PropertyKind::Group => TypeTag::Group(name()),
        }
    }
}

fn type_entry(meta: &MetaObject, since: Version) -> TypeEntry {
    let base_enums = meta.base.as_ref().map_or(0, |b| b.enums.len());
    TypeEntry {
        name: meta.name.to_string(),
        since: since.to_string(),
        base: meta.base.as_ref().map(|b| b.name.to_string()),
        base_module: meta
            .base
            .as_ref()
            .and_then(|b| b.module.as_ref().map(Atom::to_string)),
        singleton: false,
        enums: meta
            .enums
            .get(base_enums..)
            .unwrap_or_default()
            .iter()
            .map(|e| EnumEntry {
                name: e.name.to_string(),
                keys: e.keys.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            })
            .collect(),
        creatable: meta.create.is_some(),
        default_property: meta.default_property.as_ref().map(Atom::to_string),
        attached: meta.attached.map(|f| f().name.to_string()),
        properties: meta
            .own_properties()
            .iter()
            .filter(|p| !matches!(p.access, PropertyAccess::Group { .. }))
            .map(|p| PropertyEntry {
                name: p.name.to_string(),
                ty: p
                    .holds
                    .as_ref()
                    .map_or_else(|| p.ty.name(), Atom::to_string),
                kind: property_kind(&p.ty),
                list: p.flags.contains(PropertyFlags::LIST),
                readonly: p.flags.contains(PropertyFlags::READONLY),
                is_final: p.flags.contains(PropertyFlags::FINAL),
                required: p.flags.contains(PropertyFlags::REQUIRED),
                constant: p.flags.contains(PropertyFlags::CONSTANT),
                group: p.flags.contains(PropertyFlags::GROUP),
                notify: p
                    .notify
                    .map(|s| meta.signals[s.0 as usize].name.to_string()),
            })
            .collect(),
        signals: meta
            .own_signals()
            .iter()
            .filter(|s| s.notify_of.is_none())
            .map(|s| SignalEntry {
                name: s.name.to_string(),
                parameters: s
                    .params
                    .iter()
                    .map(|(n, t)| ParamEntry {
                        name: n.to_string(),
                        ty: t.name(),
                    })
                    .collect(),
            })
            .collect(),
        methods: meta
            .own_methods()
            .iter()
            .map(|m| MethodEntry {
                name: m.name.to_string(),
                parameters: m
                    .params
                    .iter()
                    .map(|(n, t)| ParamEntry {
                        name: n.to_string(),
                        ty: t.name(),
                    })
                    .collect(),
                return_type: m.ret.name(),
            })
            .collect(),
    }
}

/// The `ljostypes.json` shape: every registered type with its own members
/// (inherited ones are found through `base`), enough for a compiler to
/// check QML against the types without the program that registers them.
/// Group leaves are omitted; a group's type, and an attached type, that
/// no module exports is in `unnamed`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeDump {
    pub format: u32,
    pub modules: Vec<ModuleDump>,
    /// Types reachable only as a group or an attached type.
    #[serde(default)]
    pub unnamed: Vec<TypeEntry>,
}

impl TypeDump {
    /// 2 added what a compiler needs: property kinds, enums, singletons,
    /// bases' modules and the unnamed types.
    pub const FORMAT: u32 = 2;

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("type dump serialises")
    }

    /// Reads what [`to_json`](Self::to_json) wrote. `Err` for text that is
    /// not a dump, or one of a format this engine does not read.
    pub fn from_json(text: &str) -> Result<TypeDump, TypeDumpError> {
        let dump: TypeDump = serde_json::from_str(text)?;
        if dump.format != Self::FORMAT {
            return Err(TypeDumpError::Format { found: dump.format });
        }
        Ok(dump)
    }
}

/// Why [`TypeDump::from_json`] read no dump.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TypeDumpError {
    /// The text is not a type dump.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// A dump of another format: written by another version, to be
    /// written again.
    #[error(
        "type dump format {found} where {} is read; write it again",
        TypeDump::FORMAT
    )]
    Format { found: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleDump {
    pub uri: String,
    pub versions: Vec<String>,
    pub types: Vec<TypeEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeEntry {
    pub name: String,
    pub since: String,
    pub base: Option<String>,
    /// The module that exports `base`; `None` for a base no module
    /// exports.
    #[serde(default)]
    pub base_module: Option<String>,
    /// Registered as one object: a file reads its members and makes none.
    #[serde(default)]
    pub singleton: bool,
    /// This type's own enums.
    #[serde(default)]
    pub enums: Vec<EnumEntry>,
    pub creatable: bool,
    pub default_property: Option<String>,
    pub attached: Option<String>,
    pub properties: Vec<PropertyEntry>,
    pub signals: Vec<SignalEntry>,
    pub methods: Vec<MethodEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// What `type` names; see [`PropertyEntry::type_tag`].
    #[serde(default)]
    pub kind: PropertyKind,
    /// A list property (`list<T>` or a native `QmlList`).
    #[serde(default)]
    pub list: bool,
    pub readonly: bool,
    #[serde(rename = "final")]
    pub is_final: bool,
    pub required: bool,
    pub constant: bool,
    pub group: bool,
    pub notify: Option<String>,
}

/// What a property's type name names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyKind {
    /// A value type (`int`, `color`, `var`).
    #[default]
    Value,
    Object,
    List,
    Enum,
    Flags,
    Group,
}

/// One enum and its keys' numbers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumEntry {
    pub name: String,
    pub keys: Vec<(String, i64)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalEntry {
    pub name: String,
    pub parameters: Vec<ParamEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodEntry {
    pub name: String,
    pub parameters: Vec<ParamEntry>,
    pub return_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse() {
        assert_eq!(Version::parse("2.15"), Some(Version::new(2, 15)));
        assert_eq!(Version::parse("6"), Some(Version::new(6, 0)));
        assert_eq!(Version::parse("x"), None);
        assert!(Version::new(2, 9) < Version::new(2, 15));
    }

    #[test]
    fn a_dump_of_another_format_says_to_write_it_again() {
        let text = r#"{"format": 1, "modules": []}"#;
        let e = TypeDump::from_json(text).unwrap_err();
        assert!(matches!(e, TypeDumpError::Format { found: 1 }), "{e:?}");
        assert_eq!(
            e.to_string(),
            "type dump format 1 where 2 is read; write it again"
        );
        let e = TypeDump::from_json("{").unwrap_err();
        assert!(matches!(e, TypeDumpError::Json(_)), "{e:?}");
    }
}
