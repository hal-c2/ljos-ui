//! Key delivery, in Qt's order (ARCHITECTURE §11,
//! doc.qt.io/qt-6/qtquick-input-focus.html):
//!
//! 1. `shortcutOverride` goes to the item with active focus and up its
//!    parents; whoever accepts it keeps the key from the shortcuts.
//! 2. The `Shortcut`s of the window.
//! 3. The item with active focus, then each of its parents, until one
//!    accepts: its `Keys` handlers, its own key handling
//!    ([`register_key_item`]) and its `KeyNavigation`, then Tab if the
//!    item is in the Tab order.
//! 4. Tab, if nobody wanted it: to the first or the last item of the Tab
//!    order.
//!
//! A release takes road 3 alone. Nothing here runs unless a key arrives.

use std::cell::RefCell;

use ljos_engine::{Engine, MetaRef, ObjectId, Value};

use super::attached::{
    self, KeyNavigationAttached, KeysAttached, key_signal, keys_attacher, navigation_attacher,
    navigation_prop, priority,
};
use super::{event, key, shortcut};
use crate::focus::{FocusReason, tab, tree as focus};
use crate::input::{KeyEvent, KeyKind, Modifiers};
use crate::item::{Item, is_item};
use crate::scene::QuickScene;

/// A tree deeper than this, or a chain of `forwardTo` or `KeyNavigation`
/// longer, is a cycle someone built.
const DEPTH: usize = 4096;
/// `Keys.forwardTo` naming an item that forwards back would never end.
const FORWARD_DEPTH: usize = 8;

/// What an item type does with keys itself, between its `Keys` handlers
/// with `Keys.BeforeItem` and those with `Keys.AfterItem`. Each returns
/// whether the item took the event.
#[derive(Clone, Copy, Debug)]
pub struct KeyItem {
    /// Asked before the shortcuts: true keeps the key from them (a text
    /// field wants Backspace even if a Shortcut is on it).
    pub shortcut_override: fn(&Engine, ObjectId, &KeyEvent) -> bool,
    pub press: fn(&Engine, ObjectId, &KeyEvent) -> bool,
    pub release: fn(&Engine, ObjectId, &KeyEvent) -> bool,
}

#[derive(Default)]
struct Registry {
    types: RefCell<Vec<(MetaRef, KeyItem)>>,
}

/// Makes key delivery ask items of type `meta` (and of the types derived
/// from it) through `hooks`. A later registration wins, so a derived type
/// can be given hooks of its own.
pub fn register_key_item(engine: &Engine, meta: MetaRef, hooks: KeyItem) {
    engine
        .extension::<Registry>()
        .types
        .borrow_mut()
        .push((meta, hooks));
}

fn key_item(engine: &Engine, obj: ObjectId) -> Option<KeyItem> {
    let reg = engine.extension::<Registry>();
    let types = reg.types.borrow();
    if types.is_empty() {
        return None;
    }
    let meta = engine.meta_of(obj)?;
    types
        .iter()
        .rev()
        .find(|(m, _)| meta.inherits(m))
        .map(|(_, h)| *h)
}

/// Which way Tab goes for this key: forward for Tab, back for Backtab
/// and for Tab with Shift. `None` for any other key, and with Ctrl, Alt
/// or Meta held, which make it another command (the next tab of a tab
/// bar, the window switcher).
fn tab_direction(ev: &KeyEvent) -> Option<bool> {
    let other = Modifiers::CONTROL | Modifiers::ALT | Modifiers::META;
    if ev.modifiers.intersects(other) {
        return None;
    }
    match ev.key {
        key::TAB => Some(!ev.modifiers.contains(Modifiers::SHIFT)),
        key::BACKTAB => Some(false),
        _ => None,
    }
}

fn tab_reason(forward: bool) -> FocusReason {
    if forward {
        FocusReason::Tab
    } else {
        FocusReason::Backtab
    }
}

/// Whether focus can go to the item: shown and enabled, with all its
/// parents.
fn takes_focus(engine: &Engine, item: ObjectId) -> bool {
    is_item(engine, item)
        && engine.read(item, Item::VISIBLE.index).to_boolean()
        && engine.read(item, Item::ENABLED.index).to_boolean()
}

fn keys_of(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    engine
        .existing_attached_object(item, &keys_attacher())
        .filter(|a| engine.read(*a, KeysAttached::ENABLED.index).to_boolean())
}

fn navigation_of(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    engine.existing_attached_object(item, &navigation_attacher())
}

/// Whether the attached object acts at this point of an item's turn.
fn at(engine: &Engine, att: Option<ObjectId>, prop: ljos_engine::PropIndex, after: bool) -> bool {
    att.is_some_and(|a| (engine.read(a, prop).to_number() as i32 == priority::AFTER_ITEM) == after)
}

fn emit(engine: &Engine, att: ObjectId, name: &str, ev: &KeyEvent, accepted: bool) -> bool {
    match engine.signal_index(att, name) {
        Ok(signal) => event::emit(engine, att, signal, ev, accepted),
        Err(_) => false,
    }
}

/// The items `Keys.forwardTo` names.
fn forward_targets(engine: &Engine, att: ObjectId) -> Vec<ObjectId> {
    match engine.read(att, KeysAttached::FORWARD_TO.index) {
        Value::List(list) => list
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| engine.is_alive(*o) && is_item(engine, *o))
            .collect(),
        _ => Vec::new(),
    }
}

/// An item's `Keys` handlers for a press.
fn keys_press(engine: &Engine, root: ObjectId, att: ObjectId, ev: &KeyEvent, depth: usize) -> bool {
    if depth < FORWARD_DEPTH {
        for t in forward_targets(engine, att) {
            if engine.is_alive(t) && item_press(engine, root, t, ev, depth + 1) {
                return true;
            }
        }
    }
    if !engine.is_alive(att) {
        return false;
    }
    // A handler for the one key has taken the press by being there; it
    // passes the press on by saying so.
    if let Some(name) = key_signal(ev.key)
        && let Ok(signal) = engine.signal_index(att, name)
        && engine.signal_connected(att, signal)
        && event::emit(engine, att, signal, ev, true)
    {
        return true;
    }
    engine.is_alive(att) && emit(engine, att, "pressed", ev, false)
}

fn keys_release(
    engine: &Engine,
    root: ObjectId,
    att: ObjectId,
    ev: &KeyEvent,
    depth: usize,
) -> bool {
    if depth < FORWARD_DEPTH {
        for t in forward_targets(engine, att) {
            if engine.is_alive(t) && item_release(engine, root, t, ev, depth + 1) {
                return true;
            }
        }
    }
    engine.is_alive(att) && emit(engine, att, "released", ev, false)
}

/// The item `KeyNavigation` sends focus to for the key: the one named,
/// or, when that one cannot take focus, the one it names for the same
/// key, and so on.
fn navigation_target(engine: &Engine, att: ObjectId, ev: &KeyEvent) -> Option<ObjectId> {
    let prop = navigation_prop(ev.key)?;
    let mut cur = att;
    for _ in 0..DEPTH {
        let target = engine
            .read(cur, prop)
            .as_object()
            .filter(|t| engine.is_alive(*t))?;
        if takes_focus(engine, target) {
            return Some(target);
        }
        cur = navigation_of(engine, target)?;
    }
    None
}

fn navigation_press(engine: &Engine, att: ObjectId, ev: &KeyEvent) -> bool {
    let Some(target) = navigation_target(engine, att, ev) else {
        return false;
    };
    let reason = match ev.key {
        key::TAB => FocusReason::Tab,
        key::BACKTAB => FocusReason::Backtab,
        _ => FocusReason::Other,
    };
    focus::force_active_focus(engine, target, reason);
    true
}

/// One item's turn at a press. `depth` counts `forwardTo` hops; an item
/// keys are forwarded to does not move focus with Tab.
fn item_press(
    engine: &Engine,
    root: ObjectId,
    item: ObjectId,
    ev: &KeyEvent,
    depth: usize,
) -> bool {
    for after in [false, true] {
        if after
            && let Some(hooks) = key_item(engine, item)
            && (hooks.press)(engine, item, ev)
        {
            return true;
        }
        if !engine.is_alive(item) {
            return false;
        }
        let keys = keys_of(engine, item);
        if at(engine, keys, KeysAttached::PRIORITY.index, after)
            && let Some(att) = keys
            && keys_press(engine, root, att, ev, depth)
        {
            return true;
        }
        if !engine.is_alive(item) {
            return false;
        }
        let nav = navigation_of(engine, item);
        if at(engine, nav, KeyNavigationAttached::PRIORITY.index, after)
            && let Some(att) = nav
            && navigation_press(engine, att, ev)
        {
            return true;
        }
    }
    if depth == 0
        && let Some(forward) = tab_direction(ev)
        && engine.is_alive(item)
        && engine
            .read(item, Item::ACTIVE_FOCUS_ON_TAB.index)
            .to_boolean()
        && let Some(target) = tab::target(engine, root, Some(item), forward)
    {
        focus::force_active_focus(engine, target, tab_reason(forward));
        return true;
    }
    false
}

fn item_release(
    engine: &Engine,
    root: ObjectId,
    item: ObjectId,
    ev: &KeyEvent,
    depth: usize,
) -> bool {
    for after in [false, true] {
        if after
            && let Some(hooks) = key_item(engine, item)
            && (hooks.release)(engine, item, ev)
        {
            return true;
        }
        if !engine.is_alive(item) {
            return false;
        }
        let keys = keys_of(engine, item);
        if at(engine, keys, KeysAttached::PRIORITY.index, after)
            && let Some(att) = keys
            && keys_release(engine, root, att, ev, depth)
        {
            return true;
        }
        if !engine.is_alive(item) {
            return false;
        }
        // The release of a key that moves focus from here belongs to
        // that move.
        let nav = navigation_of(engine, item);
        if at(engine, nav, KeyNavigationAttached::PRIORITY.index, after)
            && let Some(att) = nav
            && navigation_target(engine, att, ev).is_some()
        {
            return true;
        }
    }
    false
}

/// Asks the focus item and its parents whether the key is theirs and not
/// a shortcut's.
fn shortcut_override(engine: &Engine, from: ObjectId, ev: &KeyEvent) -> bool {
    let mut cur = Some(from);
    for _ in 0..DEPTH {
        let Some(item) = cur.filter(|o| engine.is_alive(*o)) else {
            break;
        };
        if let Some(att) = keys_of(engine, item)
            && emit(engine, att, "shortcutOverride", ev, false)
        {
            return true;
        }
        if engine.is_alive(item)
            && let Some(hooks) = key_item(engine, item)
            && (hooks.shortcut_override)(engine, item, ev)
        {
            return true;
        }
        cur = engine.visual_parent(item);
    }
    false
}

/// Sends the event from `from` up its parents until one accepts.
fn bubble(
    engine: &Engine,
    root: ObjectId,
    from: ObjectId,
    ev: &KeyEvent,
    turn: fn(&Engine, ObjectId, ObjectId, &KeyEvent, usize) -> bool,
) -> bool {
    let mut cur = Some(from);
    for _ in 0..DEPTH {
        let Some(item) = cur.filter(|o| engine.is_alive(*o)) else {
            break;
        };
        // Read first: a handler may move the item.
        let parent = engine.visual_parent(item);
        if turn(engine, root, item, ev, 0) {
            return true;
        }
        cur = parent;
    }
    false
}

/// Delivers a key event to the window whose content is `root`. Returns
/// whether anything took it; the host passes on what nothing took.
///
/// A window that does not have the keyboard gets no keys. The key
/// filters ([`super::add_key_filter`]) see the event first.
pub fn deliver_key(engine: &Engine, root: ObjectId, ev: &KeyEvent) -> bool {
    // Focus asked for while the tree was being made counts from now.
    focus::settle(engine);
    if !focus::is_window_active(engine, root) {
        return false;
    }
    if super::filter::keeps(engine, root, ev) {
        return true;
    }
    let item = focus::active_focus_item(engine, root);
    if ev.kind == KeyKind::Release {
        return item.is_some_and(|f| bubble(engine, root, f, ev, item_release));
    }
    // Once a sequence of several chords is under way the next press is
    // the shortcuts' whatever it is.
    let claimed = !shortcut::sequence_pending(engine, root)
        && item.is_some_and(|f| shortcut_override(engine, f, ev));
    if !claimed && shortcut::press(engine, root, ev) {
        return true;
    }
    // A handler above may have moved focus.
    let item = focus::active_focus_item(engine, root);
    if item.is_some_and(|f| bubble(engine, root, f, ev, item_press)) {
        return true;
    }
    if let Some(forward) = tab_direction(ev)
        && let Some(target) = tab::target(engine, root, None, forward)
    {
        focus::force_active_focus(engine, target, tab_reason(forward));
        return true;
    }
    false
}

/// Where Tab (`forward`) or Shift+Tab would take focus from `from`,
/// without pressing it: the `KeyNavigation` of the item and its parents,
/// then the Tab order. With no `from`, where the first Tab goes. `None`
/// when Tab stops nowhere.
///
/// `Keys` handlers and a control's own key handling are not asked, since
/// asking is running them: this is where focus goes when nothing keeps
/// the key. For what looks without touching, like an audit.
pub fn tab_target(
    engine: &Engine,
    root: ObjectId,
    from: Option<ObjectId>,
    forward: bool,
) -> Option<ObjectId> {
    let ev = KeyEvent::press(if forward { key::TAB } else { key::BACKTAB });
    let mut cur = from;
    for _ in 0..DEPTH {
        let Some(item) = cur.filter(|o| engine.is_alive(*o)) else {
            break;
        };
        if let Some(att) = navigation_of(engine, item)
            && let Some(target) = navigation_target(engine, att, &ev)
        {
            return Some(target);
        }
        if engine
            .read(item, Item::ACTIVE_FOCUS_ON_TAB.index)
            .to_boolean()
            && let Some(target) = tab::target(engine, root, Some(item), forward)
        {
            return Some(target);
        }
        cur = engine.visual_parent(item);
    }
    tab::target(engine, root, None, forward)
}

impl QuickScene {
    /// Delivers a key event (ARCHITECTURE §11): the application's key
    /// filters first ([`crate::keys::add_key_filter`]), then shortcuts,
    /// then the item with active focus and its parents. Returns whether anything
    /// accepted it, so the host knows which keys are still its own.
    pub fn key_event(&mut self, engine: &Engine, ev: &KeyEvent) -> bool {
        self.root()
            .is_some_and(|root| deliver_key(engine, root, ev))
    }

    /// Tells the scene whether its window has the keyboard. Active focus
    /// follows: it leaves every item when the window is not active and
    /// returns to the same item when it is again. A scene starts active.
    pub fn set_window_active(&mut self, engine: &Engine, active: bool) {
        let Some(root) = self.root() else {
            return;
        };
        focus::settle(engine);
        if !active {
            shortcut::reset(engine, root);
        }
        focus::set_window_active(engine, root, active);
    }

    /// Whether the scene's window has the keyboard.
    pub fn is_window_active(&self, engine: &Engine) -> bool {
        self.root()
            .is_some_and(|root| focus::is_window_active(engine, root))
    }

    /// The item with active focus: where keys go.
    pub fn active_focus_item(&self, engine: &Engine) -> Option<ObjectId> {
        focus::active_focus_item(engine, self.root()?)
    }
}

pub(crate) fn register(engine: &Engine) {
    attached::register(engine);
}
