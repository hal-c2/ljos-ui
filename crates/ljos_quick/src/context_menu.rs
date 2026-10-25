//! Asking an item for its context menu.
//!
//! The scene knows when the user asks (a right-click the items under the
//! pointer left alone, the Menu key, Shift+F10) but not what answers:
//! `ContextMenu` and the menus of the editors live in another crate,
//! which says here which items take a request and what they do with it.

use std::cell::RefCell;

use ljos_engine::{Engine, ObjectId};
use ljos_render::kurbo;

use crate::input::{KeyEvent, KeyKind, Modifiers};
use crate::keys::key;

/// How the user asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextMenuReason {
    /// A right-click, at the point given.
    Mouse,
    /// The Menu key or Shift+F10, for the item with active focus.
    Keyboard,
}

/// What a crate does with context menu requests.
#[derive(Clone, Copy, Debug)]
pub struct ContextMenuHook {
    /// Whether `obj` takes a request, which then goes no further.
    pub takes: fn(&Engine, ObjectId) -> bool,
    /// The request, at a point in `obj`'s own coordinates.
    pub request: fn(&Engine, ObjectId, kurbo::Point, ContextMenuReason),
}

#[derive(Default)]
struct Registry {
    hooks: RefCell<Vec<ContextMenuHook>>,
}

/// Makes the scene offer context menu requests to `hook`.
pub fn register_context_menu(engine: &Engine, hook: ContextMenuHook) {
    engine.extension::<Registry>().hooks.borrow_mut().push(hook);
}

/// The first of `candidates` a hook takes requests for, and that hook.
pub(crate) fn taker(
    engine: &Engine,
    candidates: impl IntoIterator<Item = ObjectId>,
) -> Option<(ObjectId, ContextMenuHook)> {
    let hooks = engine.extension::<Registry>().hooks.borrow().clone();
    if hooks.is_empty() {
        return None;
    }
    candidates.into_iter().find_map(|o| {
        hooks
            .iter()
            .find(|h| engine.is_alive(o) && engine.untrack(|| (h.takes)(engine, o)))
            .map(|h| (o, *h))
    })
}

/// Whether `ev` is a key press that asks for a context menu.
pub(crate) fn is_request_key(ev: &KeyEvent) -> bool {
    let mods = ev.modifiers - Modifiers::KEYPAD;
    ev.kind == KeyKind::Press
        && ((ev.key == key::MENU && mods.is_empty())
            || (ev.key == key::F10 && mods == Modifiers::SHIFT))
}
