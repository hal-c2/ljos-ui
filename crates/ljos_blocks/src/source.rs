//! Blocks written in QML, read from their files, and a project's registry
//! read from its directory.

use std::path::{Path, PathBuf};

use qml_syntax::ast::{QmlMember, QmlTypeName};
use qml_syntax::{Allocator, GetSpan, text};

use crate::{
    ActionEntry, BlockContract, BlockDecl, BlockRegistry, BlocksError, CONTRIBUTION, EventEntry,
    Origin, Param, SlotEntry, StateEntry, read_qmldir,
};

/// The contract of the block `decl` of `module`, from its file's `source`;
/// `file` is what [`Origin::Qml`] says. `Err` says why a file that does
/// not parse has none: a contract read from a damaged file would be
/// missing the members that failed.
pub fn qml_contract(
    module: &str,
    decl: &BlockDecl,
    file: &str,
    source: &str,
) -> Result<BlockContract, String> {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    if let Some(d) = parsed.diagnostics.first() {
        let (line, column) = d.position(source);
        return Err(format!("{file}:{line}:{column}: {}", d.message));
    }
    let mut contract = BlockContract {
        module: module.to_string(),
        name: decl.name.clone(),
        version: decl.version,
        origin: Origin::Qml {
            file: file.to_string(),
        },
        category: decl.category.clone(),
        summary: decl.summary.clone(),
        state: Vec::new(),
        inputs: Vec::new(),
        actions: Vec::new(),
        events: Vec::new(),
        slots: Vec::new(),
    };
    let ty = |t: &QmlTypeName<'_>| text(source, t.span).to_string();
    for member in &parsed.document.root.members {
        match member {
            QmlMember::Property(p) => {
                let name = p.name.name.to_string();
                if let Some(t) = &p.type_name
                    && t.is_list
                    && t.name.last() == CONTRIBUTION
                {
                    contract.slots.push(SlotEntry {
                        name,
                        accepts: CONTRIBUTION.to_string(),
                    });
                    continue;
                }
                let entry = StateEntry {
                    notify: Some(format!("{name}Changed")),
                    name,
                    ty: p.type_name.as_ref().map_or_else(|| "var".to_string(), ty),
                };
                if p.is_readonly {
                    contract.state.push(entry);
                } else {
                    contract.inputs.push(entry);
                }
            }
            QmlMember::Signal(s) => contract.events.push(EventEntry {
                name: s.name.name.to_string(),
                params: s
                    .params
                    .iter()
                    .map(|p| Param {
                        name: p.name.name.to_string(),
                        ty: p.type_name.as_ref().map(ty),
                    })
                    .collect(),
            }),
            QmlMember::Function(f) => {
                let Some(id) = &f.id else { continue };
                let annotation = |a: &qml_syntax::js::TSTypeAnnotation<'_>| {
                    text(source, a.type_annotation.span()).to_string()
                };
                contract.actions.push(ActionEntry {
                    name: id.name.to_string(),
                    params: f
                        .params
                        .items
                        .iter()
                        .map(|p| Param {
                            name: p.pattern.get_identifier_name().map_or_else(
                                || text(source, p.pattern.span()).to_string(),
                                |n| n.to_string(),
                            ),
                            ty: p.type_annotation.as_deref().map(annotation),
                        })
                        .collect(),
                    returns: f
                        .return_type
                        .as_deref()
                        .map(annotation)
                        .filter(|r| r != "void"),
                });
            }
            _ => {}
        }
    }
    Ok(contract)
}

/// Directories no project keeps its QML in.
fn skipped(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "target" | "node_modules" | "build")
}

/// `path` relative to `root`, with `/`.
fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn io(path: &Path, source: std::io::Error) -> BlocksError {
    BlocksError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// The project's `qmldir` and `ljostypes.json` files, each list sorted.
fn project_files(root: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>), BlocksError> {
    let mut dirs = vec![root.to_path_buf()];
    let mut qmldirs = Vec::new();
    let mut dumps = Vec::new();
    while let Some(dir) = dirs.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .map_err(|e| io(&dir, e))?
            .filter_map(Result::ok)
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !skipped(&name) {
                    dirs.push(path);
                }
            } else if !kind.is_file() {
                // A link is not followed out of the project.
            } else if name == "qmldir" {
                qmldirs.push(path);
            } else if name == "ljostypes.json" {
                dumps.push(path);
            }
        }
    }
    qmldirs.sort();
    dumps.sort();
    Ok((qmldirs, dumps))
}

impl crate::DumpedTypes {
    /// What the `ljostypes.json` files of the project at `root` describe,
    /// found as [`BlockRegistry::from_project`] finds them. A file that
    /// cannot be read adds nothing: the registry's problems say why.
    pub fn from_project(root: &Path) -> Result<Self, BlocksError> {
        let root = root.canonicalize().map_err(|e| io(root, e))?;
        let mut types = Self::default();
        for path in project_files(&root)?.1 {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let _ = types.add_dump(&text);
            }
        }
        Ok(types)
    }
}

pub(crate) fn from_project(root: &Path) -> Result<BlockRegistry, BlocksError> {
    // Canonical, so that a file a link or a `..` takes out of the project
    // is seen to be outside it: a tool confined to the project reads
    // nothing else through a block.
    let root = &root.canonicalize().map_err(|e| io(root, e))?;
    let mut registry = BlockRegistry::new();
    let (qmldirs, dumps) = project_files(root)?;
    for path in &dumps {
        let file = relative(root, path);
        let text = std::fs::read_to_string(path).map_err(|e| io(path, e))?;
        if let Err(e) = registry.add_type_dump(&text, &file) {
            registry.problem(&file, None, e.to_string());
        }
    }
    for path in &qmldirs {
        let file = relative(root, path);
        let text = std::fs::read_to_string(path).map_err(|e| io(path, e))?;
        let read = read_qmldir(&text);
        for e in &read.errors {
            registry.problem(&file, Some(e.line), e.message.clone());
        }
        if read.blocks.is_empty() {
            continue;
        }
        let Some(module) = &read.module else {
            registry.problem(
                &file,
                None,
                "blocks of a qmldir without a `module` line belong to no module",
            );
            continue;
        };
        let dir = path.parent().unwrap_or(root);
        for (line, decl) in &read.blocks {
            let qml = dir.join(&decl.file);
            let qml_file = relative(root, &qml);
            let read = qml.canonicalize().and_then(|real| {
                if real.starts_with(root) {
                    std::fs::read_to_string(real).map(Some)
                } else {
                    Ok(None)
                }
            });
            match read {
                Ok(None) => registry.problem(
                    &file,
                    Some(*line),
                    format!("block {}: {qml_file} is outside the project", decl.name),
                ),
                Ok(Some(source)) => match crate::qml_contract(module, decl, &qml_file, &source) {
                    Ok(contract) => registry.insert(contract, &file, Some(*line)),
                    Err(message) => registry.problem(&file, Some(*line), message),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => registry.problem(
                    &file,
                    Some(*line),
                    format!("block {}: {qml_file} does not exist", decl.name),
                ),
                Err(e) => registry.problem(
                    &file,
                    Some(*line),
                    format!("block {}: {qml_file}: {e}", decl.name),
                ),
            }
        }
    }
    Ok(registry)
}
