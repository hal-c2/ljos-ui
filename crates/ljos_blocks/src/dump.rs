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
}

#[derive(Deserialize)]
struct Module {
    uri: String,
    types: Vec<Type>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Type {
    name: String,
    since: String,
    #[serde(default)]
    properties: Vec<Property>,
    #[serde(default)]
    signals: Vec<Signal>,
    #[serde(default)]
    methods: Vec<Method>,
    #[serde(default)]
    block: Option<Block>,
}

#[derive(Deserialize)]
struct Block {
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

#[derive(Deserialize)]
struct Property {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    list: bool,
    #[serde(default)]
    readonly: bool,
    #[serde(default)]
    notify: Option<String>,
}

#[derive(Deserialize)]
struct Signal {
    name: String,
    #[serde(default)]
    parameters: Vec<DumpParam>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Method {
    name: String,
    #[serde(default)]
    parameters: Vec<DumpParam>,
    #[serde(default)]
    return_type: Option<String>,
}

#[derive(Deserialize)]
struct DumpParam {
    name: String,
    #[serde(rename = "type")]
    ty: String,
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
fn element(p: &Property) -> Option<&str> {
    if let Some(inner) = p.ty.strip_prefix("list<").and_then(|t| t.strip_suffix('>')) {
        return Some(inner);
    }
    p.list.then_some(p.ty.as_str())
}

fn contract(module: &str, ty: &Type, block: &Block) -> BlockContract {
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
