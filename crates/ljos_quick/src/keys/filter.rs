//! Application-wide key filters: Rust that sees every key event a window
//! is given before its shortcuts and its focused item do, and may keep
//! it, as an event filter installed on a Qt application does. What one is
//! for: recording the next key for a "press the new shortcut" field, a
//! keymap kept in Rust (chords a user configured, prefix keys), or a
//! mode in which keys go nowhere.
//!
//! Filters live on the Engine, so every host that delivers through
//! [`crate::QuickScene::key_event`] (or [`super::deliver_key`]) runs
//! them; nothing is asked when none is installed.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{Engine, ObjectId};

use crate::input::KeyEvent;

/// A key filter: called on the UI thread with the window's root (the
/// object its scene shows) and the event, presses, auto-repeats and
/// releases alike. True keeps the event: nothing else sees it.
pub type KeyFilter = dyn Fn(&Engine, ObjectId, &KeyEvent) -> bool;

/// Names an installed filter, to remove it with [`remove_key_filter`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyFilterId(u64);

#[derive(Default)]
struct Filters {
    next: Cell<u64>,
    installed: RefCell<Vec<(KeyFilterId, Rc<KeyFilter>)>>,
}

/// Installs `filter` for every window of `engine`. The filter installed
/// last is asked first, as in Qt; the rest are not asked once one keeps
/// the event.
pub fn add_key_filter(
    engine: &Engine,
    filter: impl Fn(&Engine, ObjectId, &KeyEvent) -> bool + 'static,
) -> KeyFilterId {
    let filters = engine.extension::<Filters>();
    let id = KeyFilterId(filters.next.get());
    filters.next.set(id.0 + 1);
    filters.installed.borrow_mut().push((id, Rc::new(filter)));
    id
}

/// Takes the filter `id` out; false when it was not installed.
pub fn remove_key_filter(engine: &Engine, id: KeyFilterId) -> bool {
    let filters = engine.extension::<Filters>();
    let mut installed = filters.installed.borrow_mut();
    let before = installed.len();
    installed.retain(|(i, _)| *i != id);
    installed.len() != before
}

/// Whether a filter kept `ev`, given to the window `root`.
pub(crate) fn keeps(engine: &Engine, root: ObjectId, ev: &KeyEvent) -> bool {
    let filters = engine.extension::<Filters>();
    // Copied out: a filter may install or remove filters.
    let asked: Vec<Rc<KeyFilter>> = {
        let installed = filters.installed.borrow();
        if installed.is_empty() {
            return false;
        }
        installed.iter().rev().map(|(_, f)| f.clone()).collect()
    };
    asked.iter().any(|f| f(engine, root, ev))
}
