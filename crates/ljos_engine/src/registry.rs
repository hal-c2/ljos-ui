//! Explicit type registration ([`module!`](crate::module)) and the type
//! dump that feeds the compiler, the LSP and the API ledger.
//!
//! Registration is explicit, not `inventory` or linker sections (§5.5):
//! the application lists its modules, so what a static iOS binary contains
//! is what the source says.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::atom::Atom;
use crate::meta::{MetaObject, MetaRef, PropertyAccess, PropertyFlags};

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
    meta: MetaRef,
    /// The module version that introduced the type.
    since: Version,
}

#[derive(Clone, Debug, Default)]
struct Module {
    versions: Vec<Version>,
    types: HashMap<Atom, Entry>,
    order: Vec<Atom>,
}

/// Registered modules and their types.
#[derive(Debug, Default)]
pub struct Registry {
    modules: HashMap<Atom, Module>,
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
        let uri = Atom::intern(uri);
        let module = self.modules.entry(uri.clone()).or_insert_with(|| {
            self.order.push(uri);
            Module::default()
        });
        if !module.versions.contains(&version) {
            module.versions.push(version);
            module.versions.sort();
        }
        let name = meta.name.clone();
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

    /// The type `name` of module `uri`, at any version.
    pub fn lookup(&self, uri: &str, name: &str) -> Option<MetaRef> {
        Some(self.modules.get(uri)?.types.get(name)?.meta.clone())
    }

    /// The type `name` in whichever module has it, first registered first.
    pub fn lookup_any(&self, name: &str) -> Option<MetaRef> {
        self.order
            .iter()
            .find_map(|uri| Some(self.modules.get(uri)?.types.get(name)?.meta.clone()))
    }

    /// The type `name` as an `import uri version` sees it: types added in
    /// a later version are hidden.
    pub fn lookup_versioned(&self, uri: &str, version: Version, name: &str) -> Option<MetaRef> {
        let e = self.modules.get(uri)?.types.get(name)?;
        (e.since <= version).then(|| e.meta.clone())
    }

    /// Everything registered, in registration order.
    pub fn dump(&self) -> TypeDump {
        TypeDump {
            format: TypeDump::FORMAT,
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
                            .map(|n| type_entry(&m.types[n].meta, m.types[n].since))
                            .collect(),
                    }
                })
                .collect(),
        }
    }
}

fn type_entry(meta: &MetaObject, since: Version) -> TypeEntry {
    TypeEntry {
        name: meta.name.to_string(),
        since: since.to_string(),
        base: meta.base.as_ref().map(|b| b.name.to_string()),
        creatable: meta.create.is_some(),
        default_property: meta.default_property.as_ref().map(Atom::to_string),
        attached: meta.attached.map(|f| f().name.to_string()),
        properties: meta
            .own_properties()
            .iter()
            .filter(|p| !matches!(p.access, PropertyAccess::Group { .. }))
            .map(|p| PropertyEntry {
                name: p.name.to_string(),
                ty: p.ty.name(),
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
/// (inherited ones are found through `base`). Group leaves are omitted;
/// the group's type is listed where it is registered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeDump {
    pub format: u32,
    pub modules: Vec<ModuleDump>,
}

impl TypeDump {
    pub const FORMAT: u32 = 1;

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("type dump serialises")
    }
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
    pub readonly: bool,
    #[serde(rename = "final")]
    pub is_final: bool,
    pub required: bool,
    pub constant: bool,
    pub group: bool,
    pub notify: Option<String>,
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
}
