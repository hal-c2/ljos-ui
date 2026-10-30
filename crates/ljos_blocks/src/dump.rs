//! The blocks of a type dump, read through a mirror of only the fields a
//! contract needs. Fields this mirror does not name are ignored, so the
//! dump may grow without this crate changing.

use serde::Deserialize;

use crate::{
    ActionEntry, BlockContract, BlocksError, CONTRIBUTION, EventEntry, Origin, Param, SlotEntry,
    StateEntry, Version,
};

/// The type dump format read.
pub(crate) const FORMAT: u32 = 2;

#[derive(Deserialize)]
struct Dump {
    format: u32,
    modules: Vec<Module>,
    #[serde(default)]
    unnamed: Vec<DumpedType>,
}

#[derive(Deserialize)]
struct Module {
    uri: String,
    types: Vec<DumpedType>,
}

/// One type as a type dump describes it: its own members, as the dump
/// keeps them, with what it inherits through `base`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpedType {
    pub name: String,
    pub since: String,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub base_module: Option<String>,
    #[serde(default)]
    pub default_property: Option<String>,
    #[serde(default)]
    pub attached: Option<String>,
    #[serde(default)]
    pub properties: Vec<DumpedProperty>,
    #[serde(default)]
    pub signals: Vec<DumpedSignal>,
    #[serde(default)]
    pub methods: Vec<DumpedMethod>,
    #[serde(default)]
    pub enums: Vec<DumpedEnum>,
    #[serde(default)]
    block: Option<Block>,
}

impl DumpedType {
    /// The signals of the type itself: those it declares, and then the
    /// change signals its properties imply, which the dump does not list.
    pub fn own_signals(&self) -> Vec<DumpedSignal> {
        let implied = self.properties.iter().filter_map(|p| {
            let name = p.notify.as_ref()?;
            let declared = self.signals.iter().any(|s| &s.name == name);
            (!declared).then(|| DumpedSignal {
                name: name.clone(),
                parameters: Vec::new(),
            })
        });
        let mut signals = self.signals.clone();
        signals.extend(implied);
        signals
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct DumpedEnum {
    pub name: String,
    #[serde(default)]
    pub keys: Vec<(String, i64)>,
}

#[derive(Clone, Debug, Deserialize)]
struct Block {
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DumpedProperty {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub list: bool,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub notify: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DumpedSignal {
    pub name: String,
    #[serde(default)]
    pub parameters: Vec<DumpParam>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpedMethod {
    pub name: String,
    #[serde(default)]
    pub parameters: Vec<DumpParam>,
    #[serde(default)]
    pub return_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DumpParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

/// The types of the type dumps of a project, or of one dump.
#[derive(Clone, Debug, Default)]
pub struct DumpedTypes {
    modules: Vec<(String, Vec<DumpedType>)>,
    unnamed: Vec<DumpedType>,
}

impl DumpedTypes {
    /// Adds what one dump describes.
    pub fn add_dump(&mut self, json: &str) -> Result<(), BlocksError> {
        let dump: Dump = serde_json::from_str(json)?;
        if dump.format != FORMAT {
            return Err(BlocksError::Format { found: dump.format });
        }
        self.modules
            .extend(dump.modules.into_iter().map(|m| (m.uri, m.types)));
        self.unnamed.extend(dump.unnamed);
        Ok(())
    }

    /// The type `name` of the module `uri`.
    pub fn type_named(&self, uri: &str, name: &str) -> Option<&DumpedType> {
        self.modules
            .iter()
            .filter(|(m, _)| m == uri)
            .find_map(|(_, types)| types.iter().find(|t| t.name == name))
    }

    /// The names of the module `uri`'s types, sorted; `None` for a module
    /// no dump describes.
    pub fn module_types(&self, uri: &str) -> Option<Vec<String>> {
        let mut found = None;
        for (_, types) in self.modules.iter().filter(|(m, _)| m == uri) {
            let names: &mut Vec<String> = found.get_or_insert_default();
            names.extend(types.iter().map(|t| t.name.clone()));
        }
        if let Some(names) = &mut found {
            names.sort();
            names.dedup();
        }
        found
    }

    /// The base of `ty`, when a dump describes it.
    pub fn base_of(&self, ty: &DumpedType) -> Option<&DumpedType> {
        let base = ty.base.as_deref()?;
        self.modules
            .iter()
            .filter(|(uri, _)| ty.base_module.as_ref().is_none_or(|m| m == uri))
            .find_map(|(_, types)| types.iter().find(|t| t.name == base))
            .or_else(|| self.unnamed.iter().find(|t| t.name == base))
    }
}

pub(crate) fn blocks(json: &str) -> Result<Vec<BlockContract>, BlocksError> {
    let dump: Dump = serde_json::from_str(json)?;
    if dump.format != FORMAT {
        return Err(BlocksError::Format { found: dump.format });
    }
    let mut out = Vec::new();
    for module in dump.modules {
        for ty in module.types {
            if let Some(block) = &ty.block {
                out.push(contract(&module.uri, &ty, block));
            }
        }
    }
    Ok(out)
}

/// The element type of a list property: the dump writes `list<T>`, or `T`
/// with `list` set for a native list that says what it holds.
fn element(p: &DumpedProperty) -> Option<&str> {
    if let Some(inner) = p.ty.strip_prefix("list<").and_then(|t| t.strip_suffix('>')) {
        return Some(inner);
    }
    p.list.then_some(p.ty.as_str())
}

fn contract(module: &str, ty: &DumpedType, block: &Block) -> BlockContract {
    let mut state = Vec::new();
    let mut inputs = Vec::new();
    let mut slots = Vec::new();
    for p in &ty.properties {
        if element(p) == Some(CONTRIBUTION) {
            slots.push(SlotEntry {
                name: p.name.clone(),
                accepts: CONTRIBUTION.to_string(),
            });
            continue;
        }
        let entry = StateEntry {
            name: p.name.clone(),
            ty: p.ty.clone(),
            notify: p.notify.clone(),
        };
        if p.readonly {
            state.push(entry);
        } else {
            inputs.push(entry);
        }
    }
    let params = |ps: &[DumpParam]| {
        ps.iter()
            .map(|p| Param {
                name: p.name.clone(),
                ty: Some(p.ty.clone()),
            })
            .collect()
    };
    BlockContract {
        module: module.to_string(),
        name: ty.name.clone(),
        version: Version::parse(&ty.since).unwrap_or_default(),
        origin: Origin::Rust,
        category: block.category.clone(),
        summary: block.summary.clone(),
        state,
        inputs,
        actions: ty
            .methods
            .iter()
            .map(|m| ActionEntry {
                name: m.name.clone(),
                params: params(&m.parameters),
                returns: m
                    .return_type
                    .clone()
                    .filter(|r| !r.is_empty() && r != "void"),
            })
            .collect(),
        events: ty
            .signals
            .iter()
            .map(|s| EventEntry {
                name: s.name.clone(),
                params: params(&s.parameters),
            })
            .collect(),
        slots,
    }
}
