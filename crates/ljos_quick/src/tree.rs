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

/// The `Item.data` insert hook: [`item_data_append`], with `child` right
/// before `anchor` in the lists it goes to, or at their end when `anchor`
/// is not in them.
pub(crate) fn item_data_insert(
    cx: &Cx,
    child: ObjectId,
    anchor: ObjectId,
) -> Result<(), EngineError> {
    let e = cx.engine;
    if is_item(e, child) {
        crate::loader::note_child(e, cx.this, child);
        set_parent_item_before(e, child, cx.this, anchor).map(drop)
    } else {
        e.batch(|| {
            for p in [Item::RESOURCES.index, Item::DATA.index] {
                if !e.list_insert_before(cx.this, p, child, anchor)? {
                    push_unique(e, cx.this, p, child)?;
                }
            }
            Ok(())
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
        outside_of(engine, child, p)?;
    }
    engine.batch(|| {
        engine.set_visual_parent(child, parent)?;
        // A no-op when the listener `register` installs already ran.
        sync_parent(engine, child, parent)
    })
}

/// [`set_parent_item`] for an item that goes right before `anchor` in
/// `parent`'s `children` and `data` instead of at their end (a Repeater's
/// delegate, which sits before the Repeater). At the end when `anchor` is
/// not a child of `parent`.
///
/// `false`, and nothing done, when `child` is a child of `parent` already:
/// moving it among its siblings is the caller's.
pub(crate) fn set_parent_item_before(
    engine: &Engine,
    child: ObjectId,
    parent: ObjectId,
    anchor: ObjectId,
) -> Result<bool, EngineError> {
    let old = engine.with(child, |i: &Item| i.synced_parent)?;
    if old == Some(parent) {
        return Ok(false);
    }
    if !is_item(engine, parent) {
        // No lists to have a place in.
        set_parent_item(engine, child, Some(parent))?;
        return Ok(true);
    }
    outside_of(engine, child, parent)?;
    engine.batch(|| {
        if let Some(o) = old.filter(|o| engine.is_alive(*o) && is_item(engine, *o)) {
            engine.list_remove(o, Item::CHILDREN.index, child)?;
            engine.list_remove(o, Item::DATA.index, child)?;
        }
        for p in [Item::CHILDREN.index, Item::DATA.index] {
            if !engine.list_insert_before(parent, p, child, anchor)? {
                engine.list_push(parent, p, child)?;
            }
        }
        // The lists are in step already, which is what this records: the
        // reparent below then has nothing to append.
        engine.with_mut(child, |i: &mut Item| i.synced_parent = Some(parent))?;
        engine.set_visual_parent(child, Some(parent))?;
        sync_parent(engine, child, Some(parent))
    })?;
    Ok(true)
}

/// Qt refuses a parent inside the item's own subtree.
fn outside_of(engine: &Engine, child: ObjectId, parent: ObjectId) -> Result<(), EngineError> {
    let mut cur = Some(parent);
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
    Ok(())
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
                engine.list_remove(o, Item::CHILDREN.index, child)?;
                engine.list_remove(o, Item::DATA.index, child)?;
            }
            // `synced_parent` says whose lists the item is in, so it is
            // not in these: no look through them, which would make adding
            // a child cost more the more children there are.
            if let Some(p) = parent.filter(|p| is_item(engine, *p)) {
                engine.list_push(p, Item::CHILDREN.index, child)?;
                engine.list_push(p, Item::DATA.index, child)?;
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
        engine.list_remove(parent, Item::RESOURCES.index, child)?;
        engine.list_remove(parent, Item::DATA.index, child)?;
        Ok(())
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

/// Appends a resource unless it is there. A resource has nothing like an
/// item's `synced_parent` to say so, so the list is looked through (it is
/// not copied): an item's resources are few next to its children.
fn push_unique(
    engine: &Engine,
    obj: ObjectId,
    p: PropIndex,
    item: ObjectId,
) -> Result<(), EngineError> {
    let there = match engine.untrack(|| engine.read(obj, p)) {
        Value::List(l) => l.iter().any(|v| v.as_object() == Some(item)),
        _ => false,
    };
    if there {
        return Ok(());
    }
    engine.list_push(obj, p, item)
}
