//! The reload lint (ARCHITECTURE.md §12.1): siblings of one type that hold
//! state and have neither an id nor an objectName are told apart by their
//! order only, so adding one before another gives its state to the wrong
//! object. The lint asks for ids.

use std::collections::BTreeMap;

use ljos_ir::{CompiledFile, KeySeg, ObjIdx, ObjectDef, TypeRef};

use crate::{Code, Diagnostic};

/// Types of the runtime whose objects hold what a user or the program gave
/// them: text, a position, a choice, rows, a running clock.
const STATEFUL: &[&str] = &[
    "TextInput",
    "TextEdit",
    "TextField",
    "TextArea",
    "CheckBox",
    "Switch",
    "RadioButton",
    "Slider",
    "RangeSlider",
    "SpinBox",
    "ComboBox",
    "Dial",
    "Tumbler",
    "Flickable",
    "ListView",
    "GridView",
    "PathView",
    "TableView",
    "ScrollView",
    "Loader",
    "Timer",
    "ListModel",
    "StackView",
    "SwipeView",
    "TabBar",
    "Drawer",
];

/// How far a chain of component files is followed to its native type.
const MAX_DEPTH: usize = 16;

/// One warning for each group of two or more stateful siblings of a type
/// that a reload can only tell apart by their order. `lookup` finds another
/// file of the project by its key.
pub fn lint<'a>(
    file: &CompiledFile,
    lookup: &dyn Fn(&str) -> Option<&'a CompiledFile>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for component in &file.components {
        let mut groups: BTreeMap<(ObjIdx, &str), Vec<&ObjectDef>> = BTreeMap::new();
        for def in &component.objects {
            if let (Some(parent), KeySeg::Typed { ty, .. }) = (def.parent, &def.key) {
                groups.entry((parent, ty.as_str())).or_default().push(def);
            }
        }
        for ((_, ty), defs) in groups {
            if defs.len() < 2 || !defs.iter().any(|d| is_stateful(file, d, lookup, 0)) {
                continue;
            }
            out.push(Diagnostic::warning(
                Code::ReloadIdentity,
                format!(
                    "{} {ty} objects here have neither an id nor an objectName, so a reload \
                     matches them by their order only: add, remove or move one and its state \
                     goes to another. Give each an id or an objectName to keep its own state",
                    defs.len()
                ),
                defs[0].span,
            ));
        }
    }
    out.sort_by_key(|d| (d.span.line, d.span.column));
    out
}

fn is_stateful<'a>(
    file: &CompiledFile,
    def: &ObjectDef,
    lookup: &dyn Fn(&str) -> Option<&'a CompiledFile>,
    depth: usize,
) -> bool {
    if !def.properties.is_empty() {
        return true;
    }
    match &def.ty {
        TypeRef::Native { name, .. } => STATEFUL.contains(&name.as_str()),
        TypeRef::Unresolved(_) => false,
        TypeRef::Component { file: key, inline } => {
            if depth >= MAX_DEPTH {
                return false;
            }
            let other = if *key == file.file {
                Some(file)
            } else {
                lookup(key)
            };
            let Some(other) = other else {
                return false;
            };
            let component = match inline {
                None => other.components.first(),
                Some(name) => other
                    .components
                    .iter()
                    .find(|c| c.name == *name && matches!(c.kind, ljos_ir::ComponentKind::Inline)),
            };
            component.is_some_and(|c| is_stateful(other, c.root(), lookup, depth + 1))
        }
    }
}
