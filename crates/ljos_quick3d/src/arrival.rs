//! What a scene read waits for that comes later: a texture's picture, a
//! model's mesh file. A view reads again only the objects it was told
//! changed, so a read that found nothing yet depends on the object's
//! arrival node, and the arrival notifies it.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use ljos::ljos_engine::NodeId;
use ljos::prelude::*;

#[derive(Default)]
struct Arrivals {
    nodes: RefCell<HashMap<ObjectId, NodeId>>,
    /// How many there were after the last sweep of the gone.
    swept: Cell<usize>,
}

/// Makes the read that is running depend on what arrives for `obj`, a
/// part of `view`'s scene.
pub(crate) fn wait(e: &Engine, view: ObjectId, obj: ObjectId) {
    let reg = e.extension::<Arrivals>();
    let known = reg.nodes.borrow().get(&obj).copied();
    let node = match known.filter(|n| e.graph().is_alive(*n)) {
        Some(n) => n,
        None => {
            // In the object's scope, so that it goes with the object; a
            // plain object has none yet, and the view's (where its reads
            // run) holds it until the sweep.
            let Some(scope) = e.object_scope(obj).or_else(|| e.object_scope(view)) else {
                return;
            };
            let Ok(n) = e.graph().property(scope, Value::Int(0)) else {
                return;
            };
            let mut nodes = reg.nodes.borrow_mut();
            // Swept when the map has doubled: each object costs its entry
            // once, not a walk of all the others.
            if nodes.len() >= reg.swept.get().max(32) * 2 {
                nodes.retain(|o, n| e.is_alive(*o) && e.graph().is_alive(*n));
                reg.swept.set(nodes.len());
            }
            nodes.insert(obj, n);
            n
        }
    };
    let _ = e.graph().get(node);
}

/// Tells the reads that waited on `obj` that something arrived.
pub(crate) fn tell(e: &Engine, obj: ObjectId) {
    let node = e.extension::<Arrivals>().nodes.borrow().get(&obj).copied();
    if let Some(n) = node.filter(|n| e.graph().is_alive(*n)) {
        let _ = e.graph().notify(n);
    }
}
