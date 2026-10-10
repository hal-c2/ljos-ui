//! Building item trees: `parent`, `children`, `resources` and `data`.
//!
//! SHIM for contract items 1 and 2. Until the engine has list properties
//! and observable visual parents, these functions keep the engine's visual
//! parent, `Item.parent` and the three lists in step, and the lists are
//! what [`crate::QuickScene`] observes.

use ljos_engine::{Engine, EngineError, ObjectId, PropIndex, Value, WriteKind};

use crate::item::{Item, is_item};

/// What QML's default property does with a declared child: an `Item`
/// becomes a visual child, anything else a resource.
pub fn add_data(engine: &Engine, parent: ObjectId, child: ObjectId) -> Result<(), EngineError> {
    if is_item(engine, child) {
        set_parent_item(engine, child, Some(parent))
    } else {
        engine.batch(|| {
            push(engine, parent, Item::RESOURCES.index, child)?;
            push(engine, parent, Item::DATA.index, child)
        })
    }
}

/// Sets `Item.parent`: moves `child` to the end of `parent`'s children.
/// Ownership does not change.
pub fn set_parent_item(
    engine: &Engine,
    child: ObjectId,
    parent: Option<ObjectId>,
) -> Result<(), EngineError> {
    if parent == Some(child) {
        return Err(EngineError::WrongType {
            ty: "Item".into(),
            expected: "a parent other than itself".into(),
        });
    }
    engine.batch(|| {
        let old = engine.visual_parent(child);
        if old != parent {
            engine.set_visual_parent(child, parent)?;
        }
        if let Some(o) = old.filter(|o| engine.is_alive(*o)) {
            remove(engine, o, Item::CHILDREN.index, child)?;
            remove(engine, o, Item::DATA.index, child)?;
        }
        if let Some(p) = parent {
            remove(engine, p, Item::CHILDREN.index, child)?;
            remove(engine, p, Item::DATA.index, child)?;
            push(engine, p, Item::CHILDREN.index, child)?;
            push(engine, p, Item::DATA.index, child)?;
        }
        let cur = engine.untrack(|| engine.read(child, Item::PARENT.index));
        if cur.as_object() != parent {
            engine.write_internal(
                child,
                Item::PARENT.index,
                parent.map_or(Value::Null, Value::Object),
                WriteKind::Imperative,
            )?;
        }
        Ok(())
    })
}

/// The visual children, in declaration order.
pub fn child_items(engine: &Engine, obj: ObjectId) -> Vec<ObjectId> {
    list(engine, obj, Item::CHILDREN.index)
}

/// The non-visual children (`TapHandler`, `Timer` …).
pub fn resources(engine: &Engine, obj: ObjectId) -> Vec<ObjectId> {
    list(engine, obj, Item::RESOURCES.index)
}

/// Reads an object list property; tracked inside bindings and effects.
pub(crate) fn list(engine: &Engine, obj: ObjectId, p: PropIndex) -> Vec<ObjectId> {
    match engine.read(obj, p) {
        Value::List(l) => l.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    }
}

fn push(engine: &Engine, obj: ObjectId, p: PropIndex, item: ObjectId) -> Result<(), EngineError> {
    let mut l = engine.untrack(|| list(engine, obj, p));
    l.push(item);
    set_list(engine, obj, p, &l)
}

fn remove(engine: &Engine, obj: ObjectId, p: PropIndex, item: ObjectId) -> Result<(), EngineError> {
    let mut l = engine.untrack(|| list(engine, obj, p));
    let before = l.len();
    l.retain(|c| *c != item);
    if l.len() == before {
        return Ok(());
    }
    set_list(engine, obj, p, &l)
}

fn set_list(
    engine: &Engine,
    obj: ObjectId,
    p: PropIndex,
    l: &[ObjectId],
) -> Result<(), EngineError> {
    engine.write_internal(
        obj,
        p,
        Value::list(l.iter().copied().map(Value::Object).collect()),
        WriteKind::Imperative,
    )
}
