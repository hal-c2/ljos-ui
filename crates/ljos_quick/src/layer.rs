//! The layer above a window's content: a second top-level item of a scene,
//! drawn over the root item and asked first by the pointer.
//!
//! Popups live there (doc.qt.io/qt-6/qml-qtquick-controls-overlay.html):
//! they are not children of the item that declared them, so nothing in
//! the content clips or covers them. The layer is not a child of the root
//! item either; an application's root may be a positioner or a layout,
//! and would lay the layer out.
//!
//! This module only knows that a tree can have such a layer. What is in
//! it, and what a press beside it does, is the business of whoever made
//! it ([`LayerHooks`]).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use ljos_engine::{Engine, NodeId, ObjectId, Value};
use ljos_render::kurbo;

use crate::input::PointerEvent;

/// A tree deeper than this is a cycle someone built by hand.
const DEPTH: usize = 4096;

/// What the scene asks the maker of the layers.
#[derive(Clone, Copy, Debug)]
pub struct LayerHooks {
    /// Takes out of the items under the pointer (topmost first) what the
    /// layer keeps the pointer from: everything beneath something modal.
    pub filter: fn(&Engine, ObjectId, &mut Vec<ObjectId>),
    /// A press is about to be delivered. Scene coordinates.
    pub press: fn(&Engine, ObjectId, &PointerEvent),
    /// A release is about to be delivered.
    pub release: fn(&Engine, ObjectId, &PointerEvent),
}

/// A move with a button held, before the scene delivers it: the layer,
/// where the press was and the move, in scene coordinates. Returns an item
/// that takes the pointer from whatever holds it, which hears that its
/// press was cancelled. Something in the layer that is dragged in from
/// beside it (a drawer from the edge of the window) is not under the press,
/// so nothing else would give it the pointer.
pub type LayerTake = fn(&Engine, ObjectId, kurbo::Point, &PointerEvent) -> Option<ObjectId>;

#[derive(Default)]
struct Layers {
    /// The layer of each tree, by the tree's top item.
    by_top: RefCell<HashMap<ObjectId, ObjectId>>,
    by_layer: RefCell<HashMap<ObjectId, ObjectId>>,
    hooks: Cell<Option<LayerHooks>>,
    take: Cell<Option<LayerTake>>,
    /// Bumped when a layer comes or goes, for bindings that found none.
    epoch: Cell<Option<NodeId>>,
}

fn epoch(engine: &Engine, layers: &Layers) -> Option<NodeId> {
    if layers.epoch.get().is_none() {
        let g = engine.graph();
        layers.epoch.set(g.property(g.root(), Value::Int(0)).ok());
    }
    layers.epoch.get()
}

/// Says who answers for the layers of this engine.
pub fn set_layer_hooks(engine: &Engine, hooks: LayerHooks) {
    engine.extension::<Layers>().hooks.set(Some(hooks));
}

/// Says who may take the pointer for the layers of this engine while a
/// button is held ([`LayerTake`]).
pub fn set_layer_take(engine: &Engine, take: LayerTake) {
    engine.extension::<Layers>().take.set(Some(take));
}

/// Makes `layer` the layer above the tree that hangs from `top`. The
/// scene showing that tree picks it up at its next frame.
pub fn set_layer(engine: &Engine, top: ObjectId, layer: ObjectId) {
    let layers = engine.extension::<Layers>();
    if let Some(old) = layers.by_top.borrow_mut().insert(top, layer) {
        layers.by_layer.borrow_mut().remove(&old);
    }
    layers.by_layer.borrow_mut().insert(layer, top);
    if let Some(n) = epoch(engine, &layers) {
        crate::item::bump_node(engine, n);
    }
    engine.request_wake();
}

/// The layer above the tree that hangs from `top`. Inside a binding or an
/// effect this is read again when a layer is made.
pub fn layer_of(engine: &Engine, top: ObjectId) -> Option<ObjectId> {
    let layers = engine.extension::<Layers>();
    if let Some(n) = epoch(engine, &layers) {
        engine.graph().get(n);
    }
    let found = layers.by_top.borrow().get(&top).copied();
    match found {
        Some(l) if engine.is_alive(l) => Some(l),
        Some(l) => {
            layers.by_top.borrow_mut().remove(&top);
            layers.by_layer.borrow_mut().remove(&l);
            None
        }
        None => None,
    }
}

/// The top item of the tree `layer` lies above, if it is a layer.
pub fn layer_host(engine: &Engine, layer: ObjectId) -> Option<ObjectId> {
    let layers = engine.extension::<Layers>();
    let host = layers.by_layer.borrow().get(&layer).copied();
    host.filter(|h| engine.is_alive(*h))
}

/// The item the window of `item` hangs from: the top of its tree, and for
/// an item in a layer the top of the tree under the layer. Tracks the
/// parents when asked inside a binding or an effect.
pub fn window_item(engine: &Engine, item: ObjectId) -> ObjectId {
    let mut top = item;
    for _ in 0..DEPTH {
        match engine.visual_parent(top) {
            Some(p) => top = p,
            None => break,
        }
    }
    layer_host(engine, top).unwrap_or(top)
}

/// The top item of the window `item` is shown in: [`window_item`] when a
/// scene shows that tree. Inside a binding or an effect this is asked
/// again when the item moves and when a scene takes a tree.
pub fn window_of(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    let top = window_item(engine, item);
    crate::focus::tree::window_nodes(engine, top).map(|_| top)
}

/// The direct child of a layer that `item` is in or is: what Tab stays
/// inside (a popup keeps the keyboard while it has it).
pub fn tab_scope(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    let layers = engine.extension::<Layers>();
    if layers.by_layer.borrow().is_empty() {
        return None;
    }
    engine.untrack(|| {
        let mut cur = item;
        for _ in 0..DEPTH {
            let p = engine.visual_parent(cur)?;
            if layers.by_layer.borrow().contains_key(&p) {
                return Some(cur);
            }
            cur = p;
        }
        None
    })
}

/// Drops from `hits` (topmost first) what the layer cuts off from the
/// pointer.
pub(crate) fn trim(engine: &Engine, layer: Option<ObjectId>, hits: &mut Vec<ObjectId>) {
    if let Some(layer) = layer.filter(|l| engine.is_alive(*l))
        && let Some(hooks) = engine.extension::<Layers>().hooks.get()
    {
        (hooks.filter)(engine, layer, hits);
    }
}

pub(crate) fn press(engine: &Engine, layer: Option<ObjectId>, ev: &PointerEvent) {
    if let Some(layer) = layer.filter(|l| engine.is_alive(*l))
        && let Some(hooks) = engine.extension::<Layers>().hooks.get()
    {
        (hooks.press)(engine, layer, ev);
    }
}

pub(crate) fn take(
    engine: &Engine,
    layer: Option<ObjectId>,
    press: kurbo::Point,
    ev: &PointerEvent,
) -> Option<ObjectId> {
    let layer = layer.filter(|l| engine.is_alive(*l))?;
    let take = engine.extension::<Layers>().take.get()?;
    take(engine, layer, press, ev).filter(|o| engine.is_alive(*o))
}

pub(crate) fn release(engine: &Engine, layer: Option<ObjectId>, ev: &PointerEvent) {
    if let Some(layer) = layer.filter(|l| engine.is_alive(*l))
        && let Some(hooks) = engine.extension::<Layers>().hooks.get()
    {
        (hooks.release)(engine, layer, ev);
    }
}
