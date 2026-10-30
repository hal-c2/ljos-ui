//! The functional blocks an application offers (ADR 0003), as data a tool
//! reads without running the application.
//!
//! A block is a registered type marked as one: `#[qml(block)]` on a Rust
//! type, which the type dump (`ljostypes.json`) carries as `block`, or a
//! `# block` comment in a module's `qmldir` for a type written in QML
//! ([`BlockDecl`]). Its [contract](BlockContract) is the type's own
//! members:
//!
//! | Part | Rust (from the dump) | QML (`source` feature) |
//! |---|---|---|
//! | `state` | readonly properties | `readonly property` |
//! | `inputs` | writable properties | other `property` |
//! | `actions` | invokable methods | `function` |
//! | `events` | signals (not change signals) | `signal` |
//! | `slots` | `list<Contribution>` properties | `property list<Contribution>` |
//!
//! Inherited members are not part of a block's contract: a block built on
//! `Item` offers what it declares, not `Item`'s geometry.
//!
//! A slot is recognised by its element type's name alone, `Contribution`,
//! whatever module that type comes from: the name is what the ADR fixes,
//! and `Ljos.Blocks` (ljos_quick) defines the type, with the `Slot` that
//! shows a list of them.
//!
//! This crate depends on no runtime crate. It reads the type dump through
//! its own mirror of the fields it needs, which ignores the rest.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

mod dump;
mod qmldir;
#[cfg(feature = "source")]
mod source;

pub use qmldir::{BlockDecl, DeclError, QmldirBlocks, read_qmldir};
#[cfg(feature = "source")]
pub use source::qml_contract;

/// The element type of a list property that makes it a slot.
pub const CONTRIBUTION: &str = "Contribution";

/// A module version (`1.0`), written as text in JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
}

impl Version {
    /// `"1.0"`, or `"1"` for `1.0`.
    pub fn parse(text: &str) -> Option<Version> {
        let (major, minor) = text.split_once('.').unwrap_or((text, "0"));
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

impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Version::parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("not a version: {text:?}")))
    }
}

/// What a block is written in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Origin {
    /// A native type, `#[qml(block)]`.
    Rust,
    /// A QML file; `file` is relative to the project.
    Qml { file: String },
}

/// One block and its public surface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockContract {
    pub module: String,
    pub name: String,
    pub version: Version,
    pub origin: Origin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// What it publishes: readonly properties.
    pub state: Vec<StateEntry>,
    /// What a skin may set: writable properties.
    pub inputs: Vec<StateEntry>,
    pub actions: Vec<ActionEntry>,
    pub events: Vec<EventEntry>,
    pub slots: Vec<SlotEntry>,
}

/// A property of a block's state or inputs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// The signal that says it changed; `None` for a constant.
    pub notify: Option<String>,
}

/// A parameter of an action or an event. `ty` is `None` where QML does
/// not annotate it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Option<String>,
}

/// A request a block accepts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionEntry {
    pub name: String,
    pub params: Vec<Param>,
    /// `None` for no value (`void`) or none annotated.
    pub returns: Option<String>,
}

/// Something a block reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEntry {
    pub name: String,
    pub params: Vec<Param>,
}

/// A named place others contribute into.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotEntry {
    pub name: String,
    /// The list's element type: [`CONTRIBUTION`].
    pub accepts: String,
}

/// Something wrong with one declaration, kept beside the blocks that did
/// read so that one bad line does not hide the rest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// The file that says it, relative to the project.
    pub file: String,
    /// 1-based, where the file has lines that matter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub message: String,
}

/// Why no registry could be read at all.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BlocksError {
    /// The text is not a type dump.
    #[error("not a type dump: {0}")]
    Json(#[from] serde_json::Error),
    /// A type dump of a format this crate does not read.
    #[error("type dump format {found} where {} is read", dump::FORMAT)]
    Format { found: u32 },
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Every block, by module and name, and what could not be read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockRegistry {
    blocks: BTreeMap<(String, String), BlockContract>,
    problems: Vec<Problem>,
}

/// What [`BlockRegistry::to_json`] writes.
#[derive(Serialize, Deserialize)]
struct Written {
    format: u32,
    blocks: Vec<BlockContract>,
    errors: Vec<Problem>,
}

impl BlockRegistry {
    /// The `format` of [`to_json`](Self::to_json).
    pub const FORMAT: u32 = 1;

    pub fn new() -> Self {
        Self::default()
    }

    /// The blocks of a type dump (`ljostypes.json`): its types marked
    /// `block`.
    pub fn from_type_dump(json: &str) -> Result<Self, BlocksError> {
        let mut registry = Self::new();
        registry.add_type_dump(json, "ljostypes.json")?;
        Ok(registry)
    }

    /// Adds the blocks of another type dump; `file` names it in problems.
    pub fn add_type_dump(&mut self, json: &str, file: &str) -> Result<(), BlocksError> {
        for contract in dump::blocks(json)? {
            self.insert(contract, file, None);
        }
        Ok(())
    }

    /// Adds `contract`. A second block of the same module and name is not
    /// added: it is a problem of `file`.
    pub fn insert(&mut self, contract: BlockContract, file: &str, line: Option<u32>) {
        let key = (contract.module.clone(), contract.name.clone());
        if self.blocks.contains_key(&key) {
            self.problem(
                file,
                line,
                format!("block {}.{} is declared twice", key.0, key.1),
            );
            return;
        }
        self.blocks.insert(key, contract);
    }

    /// Records something that could not be read.
    pub fn problem(&mut self, file: &str, line: Option<u32>, message: impl Into<String>) {
        self.problems.push(Problem {
            file: file.to_string(),
            line,
            message: message.into(),
        });
    }

    pub fn get(&self, module: &str, name: &str) -> Option<&BlockContract> {
        self.blocks.get(&(module.to_string(), name.to_string()))
    }

    /// Every block, ordered by module, then name.
    pub fn blocks(&self) -> impl Iterator<Item = &BlockContract> {
        self.blocks.values()
    }

    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Only the blocks of `module`, with every problem.
    pub fn of_module(&self, module: &str) -> Self {
        BlockRegistry {
            blocks: self
                .blocks
                .iter()
                .filter(|((m, _), _)| m == module)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            problems: self.problems.clone(),
        }
    }

    /// `{"format": 1, "blocks": [...], "errors": [...]}`, blocks in
    /// [`blocks`](Self::blocks)' order.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.written()).expect("a registry serialises")
    }

    /// What [`to_json`](Self::to_json) writes, as a value.
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self.written()).expect("a registry serialises")
    }

    fn written(&self) -> Written {
        Written {
            format: Self::FORMAT,
            blocks: self.blocks.values().cloned().collect(),
            errors: self.problems.clone(),
        }
    }

    /// Reads what [`to_json`](Self::to_json) wrote.
    pub fn from_json(text: &str) -> Result<Self, BlocksError> {
        let written: Written = serde_json::from_str(text)?;
        if written.format != Self::FORMAT {
            return Err(BlocksError::Format {
                found: written.format,
            });
        }
        Ok(BlockRegistry {
            blocks: written
                .blocks
                .into_iter()
                .map(|b| ((b.module.clone(), b.name.clone()), b))
                .collect(),
            problems: written.errors,
        })
    }

    /// The registry of the project at `dir`: every `qmldir` under it with
    /// `# block` comments, their QML files read for their contracts, and
    /// every `ljostypes.json` under it. Build output, VCS and hidden
    /// directories are not walked. Paths in the registry are relative to
    /// `dir`, with `/`.
    #[cfg(feature = "source")]
    pub fn from_project(dir: &std::path::Path) -> Result<Self, BlocksError> {
        source::from_project(dir)
    }

    /// Adds the block `decl` of `module`, whose QML `text` is in `file`
    /// (relative to the project). A file that does not parse is a problem
    /// of `file`.
    #[cfg(feature = "source")]
    pub fn add_qml_block(&mut self, module: &str, decl: &BlockDecl, file: &str, text: &str) {
        match qml_contract(module, decl, file, text) {
            Ok(contract) => self.insert(contract, file, None),
            Err(message) => self.problem(file, None, message),
        }
    }
}
