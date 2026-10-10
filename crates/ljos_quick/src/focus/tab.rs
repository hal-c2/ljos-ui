//! The Tab order: the items of a window with `activeFocusOnTab`, in
//! document order (a parent before its children), without the hidden and
//! the disabled ones. Tab goes to the next, Shift+Tab to the previous,
//! and both wrap (doc.qt.io/qt-6/qml-qtquick-item.html,
//! `activeFocusOnTab`).
//!
//! The order is one list per window, as in Qt, not one per focus scope:
//! Tab leaves a scope when its items are used up. It is made when Tab is
//! first pressed and kept until something it was made from changes (a
//! child list, `visible`, `enabled`, `activeFocusOnTab`); the change only
//! marks the list stale, and the next Tab makes it again.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ljos_engine::{Engine, NodeId, ObjectId};

use super::tree;
use crate::item::Item;

/// A tree deeper than this is a cycle someone built by hand.
const DEPTH: usize = 4096;

#[derive(Default)]
struct Cache {
    /// `None`: stale.
    order: RefCell<Option<Rc<Vec<ObjectId>>>>,
    /// The effect that made `order` and waits for it to go stale.
    effect: Cell<Option<NodeId>>,
    /// How often the order was made, for tests.
    builds: Cell<u64>,
}

#[derive(Default)]
struct Caches {
    by_root: RefCell<HashMap<ObjectId, Rc<Cache>>>,
}

/// Adds the items under `item` (and `item`) that Tab stops at, in
/// document order. `also` is listed whether Tab stops at it or not.
fn collect(
    engine: &Engine,
    item: ObjectId,
    also: Option<ObjectId>,
    depth: usize,
    out: &mut Vec<ObjectId>,
) {
    if depth > DEPTH || !crate::item::is_item(engine, item) {
        return;
    }
    // Both are read so that a change of either is seen.
    let visible = engine.read(item, Item::VISIBLE.index).to_boolean();
    let enabled = engine.read(item, Item::ENABLED.index).to_boolean();
    if !(visible && enabled) {
        if also == Some(item) {
            out.push(item);
        }
        return;
    }
    if also == Some(item)
        || engine
            .read(item, Item::ACTIVE_FOCUS_ON_TAB.index)
            .to_boolean()
    {
        out.push(item);
    }
    for c in crate::tree::child_items(engine, item) {
        collect(engine, c, also, depth + 1, out);
    }
}

/// The Tab order of the window of `root`.
fn order(engine: &Engine, root: ObjectId) -> Rc<Vec<ObjectId>> {
    if !tree::is_window(engine, root) {
        // No window, nobody presses Tab: not worth keeping.
        let mut out = Vec::new();
        engine.untrack(|| collect(engine, root, None, 0, &mut out));
        return Rc::new(out);
    }
    let caches = engine.extension::<Caches>();
    let cache = {
        let mut by_root = caches.by_root.borrow_mut();
        by_root.retain(|r, c| {
            let alive = engine.is_alive(*r);
            if !alive && let Some(n) = c.effect.take() {
                let _ = engine.graph().dispose(n);
            }
            alive
        });
        Rc::clone(by_root.entry(root).or_default())
    };
    if let Some(order) = cache.order.borrow().clone() {
        return order;
    }
    if let Some(old) = cache.effect.take() {
        let _ = engine.graph().dispose(old);
    }
    let slot = Rc::clone(&cache);
    let first = Cell::new(true);
    // Runs now and makes the list. When anything it read changes it runs
    // once more, marks the list stale and, having read nothing that time,
    // never runs again.
    let made = engine.effect(move |e| {
        if first.replace(false) {
            let mut out = Vec::new();
            collect(e, root, None, 0, &mut out);
            slot.builds.set(slot.builds.get() + 1);
            *slot.order.borrow_mut() = Some(Rc::new(out));
        } else {
            *slot.order.borrow_mut() = None;
        }
    });
    cache.effect.set(made.ok());
    cache.order.borrow().clone().unwrap_or_default()
}

/// How often the Tab order of the window of `root` was made.
#[doc(hidden)]
pub fn tab_order_builds(engine: &Engine, root: ObjectId) -> u64 {
    let caches = engine.extension::<Caches>();
    let by_root = caches.by_root.borrow();
    by_root.get(&root).map_or(0, |c| c.builds.get())
}

fn step(order: &[ObjectId], at: usize, forward: bool) -> ObjectId {
    let n = order.len();
    order[if forward {
        (at + 1) % n
    } else {
        (at + n - 1) % n
    }]
}

/// Where Tab (`forward`) or Shift+Tab goes from `from` in the tree under
/// `root`. With no `from`, the first or the last item of the order. `None`
/// when Tab stops nowhere.
pub(crate) fn target(
    engine: &Engine,
    root: ObjectId,
    from: Option<ObjectId>,
    forward: bool,
) -> Option<ObjectId> {
    let order = order(engine, root);
    let ends = |order: &[ObjectId]| {
        if forward {
            order.first().copied()
        } else {
            order.last().copied()
        }
    };
    let Some(from) = from else {
        return ends(&order);
    };
    if let Some(at) = order.iter().position(|o| *o == from) {
        return Some(step(&order, at, forward));
    }
    if order.is_empty() {
        return None;
    }
    // Tab does not stop at `from`: find its place in the document.
    let mut with = Vec::new();
    engine.untrack(|| collect(engine, root, Some(from), 0, &mut with));
    match with.iter().position(|o| *o == from) {
        Some(at) => Some(step(&with, at, forward)),
        // It is inside something hidden or disabled.
        None => ends(&order),
    }
}

/// `nextItemInFocusChain(forward)`: the item Tab (or Shift+Tab) would go
/// to from `item`; `item` itself when Tab stops nowhere.
pub fn next_item_in_focus_chain(engine: &Engine, item: ObjectId, forward: bool) -> ObjectId {
    let root = engine.untrack(|| tree::top_of(engine, item));
    target(engine, root, Some(item), forward).unwrap_or(item)
}
