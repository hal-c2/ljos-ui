//! Items of other crates that paint content of their own.
//!
//! The scene knows how `Rectangle`, `Text` or `Image` paint. An item
//! type from another crate (a `VideoOutput`) names here how to read what
//! it shows. The scene calls `read` inside the item's content effect, so
//! what it reads is tracked and a change repaints the item, and paints the
//! result into the item's render node with the same display list as the
//! built-in items: other crates never see the renderer.
//!
//! Text is laid out in `paint` with the text engine the scene lends it,
//! the one every `Text` is laid out with, and drawn as glyph runs.
//!
//! What an item shows may change where no property sees it (a frame
//! decoded, a grid of cells written by a program): [`request_repaint`]
//! repaints it then, through a node of the reactive graph that the read
//! depends on and that QML cannot see.

use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::rc::Rc;

use ljos_engine::{Engine, MetaRef, ObjectId, Value};

use crate::by_type::ByType;
use ljos_reactive::NodeId;
use ljos_render::{DisplayListBuilder, kurbo};
use ljos_text::TextEngine;
use rustc_hash::FxHashMap;

/// What an item shows, read from its properties.
pub trait ItemPaint: Debug {
    /// Paints into an item of `size`, in its own coordinates. `text` is the
    /// scene's text engine, lent for the call: the scene holds the shared
    /// one ([`crate::text_engine`]) while it paints, so borrowing that
    /// here fails. Layouts made with it are cached as a `Text`'s are.
    fn paint(&self, size: kurbo::Size, text: &mut TextEngine, out: &mut DisplayListBuilder);
}

/// Reads what `obj` shows. Tracked: whatever it reads repaints the item
/// when it changes.
pub type ReadPaint = fn(&Engine, ObjectId) -> Rc<dyn ItemPaint>;

#[derive(Default)]
struct Registry {
    types: ByType<ReadPaint>,
}

/// Makes the scene paint items of type `meta` (and of the types derived
/// from it) with what `read` returns. A later registration wins.
pub fn register_paint_item(engine: &Engine, meta: MetaRef, read: ReadPaint) {
    engine.extension::<Registry>().types.push(meta, read);
}

/// The reader of `obj`'s type, if another crate registered one.
pub(crate) fn paint_item(engine: &Engine, obj: ObjectId) -> Option<ReadPaint> {
    engine.extension::<Registry>().types.of(engine, obj)
}

/// Each painted item's repaint node, made the first time the item is read.
#[derive(Default)]
struct Repaints {
    nodes: RefCell<FxHashMap<ObjectId, NodeId>>,
    /// How many there were after the last sweep of the gone.
    swept: Cell<usize>,
}

/// Makes the read of `obj` that is running depend on its repaint node.
pub(crate) fn track_repaint(engine: &Engine, obj: ObjectId) {
    let reg = engine.extension::<Repaints>();
    let known = reg.nodes.borrow().get(&obj).copied();
    let node = match known.filter(|n| engine.graph().is_alive(*n)) {
        Some(n) => n,
        None => {
            // In the item's scope, so that it goes with the item.
            let Some(scope) = engine.object_scope(obj) else {
                return;
            };
            let Ok(n) = engine.graph().property(scope, Value::Int(0)) else {
                return;
            };
            let mut nodes = reg.nodes.borrow_mut();
            // Swept when the map has doubled: each item costs its entry
            // once, not a walk of all the others.
            if nodes.len() >= reg.swept.get().max(32) * 2 {
                nodes.retain(|o, n| engine.is_alive(*o) && engine.graph().is_alive(*n));
                reg.swept.set(nodes.len());
            }
            nodes.insert(obj, n);
            n
        }
    };
    let _ = engine.graph().get(node);
}

/// Paints `obj` again in the next frame, with what its type's read
/// returns then. Nothing happens for an item that has not been painted:
/// its first paint reads afresh anyway.
pub fn request_repaint(engine: &Engine, obj: ObjectId) {
    let node = engine
        .extension::<Repaints>()
        .nodes
        .borrow()
        .get(&obj)
        .copied();
    if let Some(n) = node.filter(|n| engine.graph().is_alive(*n)) {
        let _ = engine.graph().notify(n);
    }
}
