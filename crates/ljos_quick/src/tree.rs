//! Building item trees: `parent`, `children`, `resources` and `data`.
//!
//! The engine's visual parent ([`Engine::set_visual_parent`]) is the truth.
//! [`sync_parent`] brings `Item.parent` and the `children`/`data` lists of
//! the old and new parent in step with it; [`crate::register`] runs it
//! from the engine's visual-parent listener, so a reparent from anywhere
//! (a QML `parent:` assignment, the object creator, native code) keeps the
//! lists right. `Item.data` appends go through [`item_data_append`].

use ljos_engine::{Cx, Engine, EngineError, ObjectId, PropIndex, Provenance, Value, WriteKind};

use crate::item::{Item, is_item};

/// The `Item.data` append hook: an `Item` becomes a visual child, anything
/// else a resource.
pub(crate) fn item_data_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    let e = cx.engine;
    if is_item(e, child) {
        crate::loader::note_child(e, cx.this, child);
        set_parent_item(e, child, Some(cx.this))
    } else {
        e.batch(|| {
            push_unique(e, cx.this, Item::RESOURCES.index, child)?;
            push_unique(e, cx.this, Item::DATA.index, child)
        })
    }
}

/// What QML's default property does with a declared child: appends it to
/// `parent`'s `data`.
pub fn add_data(engine: &Engine, parent: ObjectId, child: ObjectId) -> Result<(), EngineError> {
    engine.list_append(parent, Item::DATA.index, child)
}

/// Sets the visual parent (`Item.parent`): `child` moves to the end of
/// `parent`'s children. Ownership does not change.
pub fn set_parent_item(
    engine: &Engine,
    child: ObjectId,
    parent: Option<ObjectId>,
) -> Result<(), EngineError> {
    if let Some(p) = parent {
        // Qt refuses a parent inside the item's own subtree.
        let mut cur = Some(p);
        for _ in 0..4096 {
            let Some(c) = cur else { break };
            if c == child {
                return Err(EngineError::WrongType {
                    ty: "Item".into(),
                    expected: "a parent outside the item's own subtree".into(),
                });
            }
            cur = engine.untrack(|| engine.visual_parent(c));
        }
    }
    engine.batch(|| {
        engine.set_visual_parent(child, parent)?;
        // A no-op when the listener `register` installs already ran.
        sync_parent(engine, child, parent)
    })
}

/// Brings `Item.parent` and the lists of `child`'s previous and new visual
/// parents in step with `parent`. Idempotent.
pub fn sync_parent(
    engine: &Engine,
    child: ObjectId,
    parent: Option<ObjectId>,
) -> Result<(), EngineError> {
    if !engine.is_alive(child) || !is_item(engine, child) {
        return Ok(());
    }
    let old = engine.with_mut(child, |i: &mut Item| {
        std::mem::replace(&mut i.synced_parent, parent)
    })?;
    engine.batch(|| {
        if old != parent {
            if let Some(o) = old.filter(|o| engine.is_alive(*o) && is_item(engine, *o)) {
                remove(engine, o, Item::CHILDREN.index, child)?;
                remove(engine, o, Item::DATA.index, child)?;
            }
            if let Some(p) = parent.filter(|p| is_item(engine, *p)) {
                push_unique(engine, p, Item::CHILDREN.index, child)?;
                push_unique(engine, p, Item::DATA.index, child)?;
            }
        }
        let cur = engine.untrack(|| engine.read(child, Item::PARENT.index));
        // A `parent:` binding keeps its value; Ljós applies it at
        // completion (`init_item`).
        let bound = engine.provenance(child, Item::PARENT.index) == Some(Provenance::Bound);
        if cur.as_object() != parent && !bound {
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

/// Takes a non-visual `child` out of `parent`'s `resources` and `data`.
pub(crate) fn forget_resource(
    engine: &Engine,
    parent: ObjectId,
    child: ObjectId,
) -> Result<(), EngineError> {
    engine.batch(|| {
        remove(engine, parent, Item::RESOURCES.index, child)?;
        remove(engine, parent, Item::DATA.index, child)
    })
}

/// The visual children, in order.
pub fn child_items(engine: &Engine, obj: ObjectId) -> Vec<ObjectId> {
    list(engine, obj, Item::CHILDREN.index)
}

/// The non-visual children (`TapHandler`, `Timer` …).
pub fn resources(engine: &Engine, obj: ObjectId) -> Vec<ObjectId> {
    list(engine, obj, Item::RESOURCES.index)
}

/// Reads an object list property; tracked inside bindings and effects.
/// Destroyed objects are left out.
pub(crate) fn list(engine: &Engine, obj: ObjectId, p: PropIndex) -> Vec<ObjectId> {
    match engine.read(obj, p) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| engine.is_alive(*o))
            .collect(),
        _ => Vec::new(),
    }
}

fn raw(engine: &Engine, obj: ObjectId, p: PropIndex) -> Vec<ObjectId> {
    match engine.untrack(|| engine.read(obj, p)) {
        Value::List(l) => l.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    }
}

fn push_unique(
    engine: &Engine,
    obj: ObjectId,
    p: PropIndex,
    item: ObjectId,
) -> Result<(), EngineError> {
    if raw(engine, obj, p).contains(&item) {
        return Ok(());
    }
    engine.list_push(obj, p, item)
}

fn remove(engine: &Engine, obj: ObjectId, p: PropIndex, item: ObjectId) -> Result<(), EngineError> {
    let mut l = raw(engine, obj, p);
    let before = l.len();
    l.retain(|c| *c != item);
    if l.len() == before {
        return Ok(());
    }
    engine.write_internal(
        obj,
        p,
        Value::list(l.into_iter().map(Value::Object).collect()),
        WriteKind::Initial,
    )
}
