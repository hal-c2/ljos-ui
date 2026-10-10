//! `LayoutMirroring` (QtQuick): swaps left and right for anchors,
//! positioners and layouts, never for an `x` that was set directly.
//!
//! An item is mirrored when its own `LayoutMirroring.enabled` says so, or,
//! if it was never given one, when the nearest ancestor that passes
//! mirroring on (`childrenInherit`) is mirrored. [`mirrored`] answers by
//! walking up the tree; anchors, positioners and layouts ask it inside
//! their effects, so they follow every change.
//!
//! Most applications never mirror anything. Until the first
//! `LayoutMirroring` object exists the answer is `false` without a walk,
//! and one graph node (the epoch) tells every asker when that changes.
//!
//! Behaviour follows doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html and
//! is pinned by `features/layouts/layout-mirroring.feature`.

use std::cell::Cell;

use ljos_engine::{Engine, LogLevel, MetaRef, NodeId, ObjectId, QmlObject, QmlType, Value};

use crate::item::{is_a, is_unset};
use crate::scene::SCENE_CATEGORY;

/// `LayoutMirroring` (QtQuick): only ever used attached.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "LayoutMirroring",
    attached = LayoutMirroringAttached,
    uncreatable
)]
pub struct LayoutMirroring {}

/// What `LayoutMirroring.` attaches to an item.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "LayoutMirroringAttached")]
pub struct LayoutMirroringAttached {
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub children_inherit: bool,
}

fn attacher() -> MetaRef {
    MetaRef::Static(LayoutMirroring::static_meta())
}

/// Counts the `LayoutMirroring` objects ever made. Per engine.
#[derive(Default)]
struct Epoch(Cell<Option<NodeId>>);

fn epoch(engine: &Engine) -> Option<NodeId> {
    let slot = engine.extension::<Epoch>();
    if slot.0.get().is_none() {
        let g = engine.graph();
        slot.0.set(g.property(g.root(), Value::Int(0)).ok());
    }
    slot.0.get()
}

/// Tells everything that asked [`mirrored`] to ask again when an item gets
/// its `LayoutMirroring` object.
pub(crate) fn register(engine: &Engine) {
    engine.on_attached_created(|e, _, att| {
        if is_a::<LayoutMirroringAttached>(e, att)
            && let Some(node) = epoch(e)
        {
            let g = e.graph();
            let n = g.get_untracked(node).to_number() as i32;
            g.set(node, Value::Int(n.wrapping_add(1)));
        }
        true
    });
}

/// Whether left and right are swapped for `obj`. A tracked read: an effect
/// or binding that asks runs again when the answer may have changed.
pub(crate) fn mirrored(engine: &Engine, obj: ObjectId) -> bool {
    let Some(node) = epoch(engine) else {
        return false;
    };
    if engine.graph().get(node).to_number() == 0.0 {
        return false;
    }
    let att = attacher();
    let mut cur = obj;
    let mut own = true;
    // A tree deeper than this is a cycle someone built by hand.
    for _ in 0..4096 {
        if let Some(a) = engine.existing_attached_object(cur, &att) {
            let enabled = LayoutMirroringAttached::ENABLED.index;
            let inherit = LayoutMirroringAttached::CHILDREN_INHERIT.index;
            // Both are read even when one decides, so that a later change
            // of either is seen.
            let on = engine.read(a, enabled).to_boolean();
            let passes = engine.read(a, inherit).to_boolean();
            let explicit = !is_unset(engine, a, enabled);
            // An item's own setting wins. An ancestor counts only if it
            // passes mirroring on; one that does not, or that has no
            // setting of its own, lets what comes from further up through.
            if explicit && (own || passes) {
                return on;
            }
        }
        own = false;
        match engine.visual_parent(cur) {
            Some(p) if engine.is_alive(p) => cur = p,
            _ => return false,
        }
    }
    engine.log(
        LogLevel::Warning,
        SCENE_CATEGORY,
        "LayoutMirroring: the item tree does not end".to_string(),
    );
    false
}
