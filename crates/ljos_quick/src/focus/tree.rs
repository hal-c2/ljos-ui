//! The focus tree's state: which item is each scope's focus item, which
//! trees are windows, and the path of items with active focus in each.
//!
//! The state hangs off the engine, not a scene: `focus: true` is claimed
//! when an item completes, before any scene has seen it, and a reparent
//! is heard from the engine.
//!
//! `activeFocus` is only ever written along a window's old and new path
//! ([`apply_all`]); nothing walks the item tree when focus moves.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ljos_engine::{Engine, EngineError, LogLevel, NodeId, ObjectId, PropIndex, Value, WriteKind};

use super::FocusReason;
use super::scope::is_focus_scope;
use crate::item::Item;

/// The log category of focus warnings.
const CATEGORY: &str = "ljos.quick.focus";

/// A tree deeper than this is a cycle someone built by hand.
const DEPTH: usize = 4096;

/// More `activeFocus` writes than this for one change means handlers keep
/// handing focus to each other.
const MAX_STEPS: usize = 4096;

/// A focus scope: a `FocusScope` item, or a whole tree, named by the item
/// it hangs from. A scene's tree is the scope Qt gives a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ScopeKey {
    Item(ObjectId),
    Top(ObjectId),
}

/// A tree that is shown in a window: the only place active focus exists.
struct Win {
    root: ObjectId,
    /// Whether the window has the keyboard. A window Ljós was told nothing
    /// about has it, so a scene works before its host says anything.
    active: bool,
    /// The items that read `activeFocus: true`, outermost first.
    chain: Vec<ObjectId>,
    /// `Window.activeFocusItem`, for bindings.
    focus_node: NodeId,
    /// `Window.active`, for bindings.
    active_node: NodeId,
}

#[derive(Default)]
struct State {
    /// Each scope's focus item.
    sub: HashMap<ScopeKey, ObjectId>,
    /// The scope each focus item is the focus item of.
    home: HashMap<ObjectId, ScopeKey>,
    windows: Vec<Win>,
    /// Focus items that claimed their scope while being created. Objects
    /// complete last-made-first, and Qt gives the scope to the first
    /// `focus: true` in the document; until the creation is over an
    /// earlier item still takes the scope from a later one.
    unsettled: HashSet<ObjectId>,
    /// Items claimed a scope and nothing was applied yet.
    pending: bool,
    applying: bool,
    /// The state changed while it was being applied.
    stale: bool,
    /// Why the next item gains active focus.
    reason: FocusReason,
}

/// The focus state of one engine.
#[derive(Default)]
pub(crate) struct FocusTree {
    state: RefCell<State>,
    /// Bumped when a scope's focus item or a window changes: wakes the
    /// effect that follows `enabled` along the paths.
    epoch: Cell<Option<NodeId>>,
    /// Bumped when the set of windows changes, for `Window.` bindings
    /// that found no window.
    windows_epoch: Cell<Option<NodeId>>,
    effect: Cell<Option<NodeId>>,
}

fn tree(engine: &Engine) -> Rc<FocusTree> {
    engine.extension::<FocusTree>()
}

fn node(engine: &Engine, slot: &Cell<Option<NodeId>>) -> Option<NodeId> {
    if slot.get().is_none() {
        let g = engine.graph();
        slot.set(g.property(g.root(), Value::Int(0)).ok());
    }
    slot.get()
}

fn bump(engine: &Engine, slot: &Cell<Option<NodeId>>) {
    if let Some(n) = node(engine, slot) {
        let g = engine.graph();
        let v = g.get_untracked(n).to_number() as i32;
        g.set(n, Value::Int(v.wrapping_add(1)));
    }
}

fn warn(engine: &Engine, what: &str, err: EngineError) {
    engine.log(LogLevel::Warning, CATEGORY, format!("{what}: {err}"));
}

/// Writes `focus` or `activeFocus` the way Qt's setter does: a binding on
/// the property stays.
fn write_flag(engine: &Engine, obj: ObjectId, prop: PropIndex, on: bool) {
    if !engine.is_alive(obj) {
        return;
    }
    if let Err(err) = engine.write_internal(obj, prop, Value::Bool(on), WriteKind::Animation) {
        warn(engine, "cannot write a focus property", err);
    }
}

// ------------------------------------------------------------------ scopes

/// The scope of an item whose parent is `parent`.
fn scope_from(engine: &Engine, parent: Option<ObjectId>, item: ObjectId) -> ScopeKey {
    engine.untrack(|| {
        let mut top = item;
        let mut cur = parent;
        for _ in 0..DEPTH {
            let Some(p) = cur else { break };
            if is_focus_scope(engine, p) {
                return ScopeKey::Item(p);
            }
            top = p;
            cur = engine.visual_parent(p);
        }
        // What is in the layer above a window is in that window.
        ScopeKey::Top(crate::layer::layer_host(engine, top).unwrap_or(top))
    })
}

/// The scope `item` is in.
pub(crate) fn scope_of(engine: &Engine, item: ObjectId) -> ScopeKey {
    let parent = engine.untrack(|| engine.visual_parent(item));
    scope_from(engine, parent, item)
}

/// The item the tree `item` is in hangs from. Tracks the parents when
/// asked inside a binding or an effect.
pub(crate) fn top_of(engine: &Engine, item: ObjectId) -> ObjectId {
    let mut top = item;
    for _ in 0..DEPTH {
        match engine.visual_parent(top) {
            Some(p) => top = p,
            None => break,
        }
    }
    crate::layer::layer_host(engine, top).unwrap_or(top)
}

/// Whether `ancestor` is `item` or above it.
pub(crate) fn is_under(engine: &Engine, item: ObjectId, ancestor: ObjectId) -> bool {
    engine.untrack(|| {
        let mut cur = Some(item);
        for _ in 0..DEPTH {
            match cur {
                Some(o) if o == ancestor => return true,
                Some(o) => cur = engine.visual_parent(o),
                None => return false,
            }
        }
        false
    })
}

/// `item` and what is above it, innermost first.
fn ancestry(engine: &Engine, item: ObjectId) -> Vec<ObjectId> {
    let mut out = vec![item];
    let mut cur = engine.visual_parent(item);
    for _ in 0..DEPTH {
        let Some(p) = cur else { break };
        out.push(p);
        cur = engine.visual_parent(p);
    }
    out
}

/// Whether `a` comes before `b` in the document: depth first, a parent
/// before its children.
fn precedes(engine: &Engine, a: ObjectId, b: ObjectId) -> bool {
    engine.untrack(|| {
        let pa = ancestry(engine, a);
        let pb = ancestry(engine, b);
        // Walk down from the tops while the two paths agree.
        let mut ia = pa.iter().rev();
        let mut ib = pb.iter().rev();
        let mut common = None;
        loop {
            match (ia.next(), ib.next()) {
                (Some(x), Some(y)) if x == y => common = Some(*x),
                // `a` is above `b`.
                (None, Some(_)) => return true,
                (Some(x), Some(y)) => {
                    let Some(c) = common else { return false };
                    let kids = engine.visual_children(c);
                    let at = |o: &ObjectId| kids.iter().position(|k| k == o);
                    return at(x) < at(y);
                }
                _ => return false,
            }
        }
    })
}

// ------------------------------------------------------------------ claims

/// Makes `item` its scope's focus item. The item that was loses `focus`.
fn claim(engine: &Engine, t: &FocusTree, item: ObjectId) {
    let key = scope_of(engine, item);
    let prev = {
        let mut st = t.state.borrow_mut();
        let prev = st.sub.insert(key, item).filter(|p| *p != item);
        if let Some(p) = prev {
            st.home.remove(&p);
            st.unsettled.remove(&p);
        }
        st.home.insert(item, key);
        prev
    };
    if let Some(p) = prev {
        write_flag(engine, p, Item::FOCUS.index, false);
    }
}

/// `item` is no longer its scope's focus item.
fn release(t: &FocusTree, item: ObjectId) {
    let mut st = t.state.borrow_mut();
    st.unsettled.remove(&item);
    if let Some(key) = st.home.remove(&item)
        && st.sub.get(&key) == Some(&item)
    {
        st.sub.remove(&key);
    }
}

/// What every completed item does: hears of writes to `focus`, and, if it
/// was made with `focus: true`, asks for its scope.
pub(crate) fn init(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    engine.watch(obj, Item::FOCUS.index, move |e, v| {
        focus_written(e, obj, v.to_boolean());
    })?;
    if engine
        .untrack(|| engine.read(obj, Item::FOCUS.index))
        .to_boolean()
    {
        initial_claim(engine, obj);
    }
    Ok(())
}

/// `focus` was written. The tree's own writes arrive here too, after the
/// state already says what they say, and do nothing.
fn focus_written(engine: &Engine, obj: ObjectId, want: bool) {
    let t = tree(engine);
    let has = t.state.borrow().home.contains_key(&obj);
    if has == want {
        return;
    }
    if want {
        claim(engine, &t, obj);
    } else {
        release(&t, obj);
    }
    changed(engine, &t);
}

/// An item made with `focus: true` joins its scope. A scope that has a
/// focus item keeps it and the newcomer's `focus` is cleared, as when an
/// item is moved into the scope; among items of one creation the first in
/// the document counts as having joined first.
fn initial_claim(engine: &Engine, obj: ObjectId) {
    let t = tree(engine);
    let key = scope_of(engine, obj);
    let holder = t.state.borrow().sub.get(&key).copied();
    let holder = holder.filter(|h| engine.is_alive(*h));
    let takes = match holder {
        None => true,
        Some(h) if h == obj => return,
        Some(h) => t.state.borrow().unsettled.contains(&h) && precedes(engine, obj, h),
    };
    if !takes {
        write_flag(engine, obj, Item::FOCUS.index, false);
        return;
    }
    claim(engine, &t, obj);
    let mut st = t.state.borrow_mut();
    st.unsettled.insert(obj);
    // Applied at the next frame, not now: an item completing later may
    // still take the scope, and active focus must not visit both.
    if !st.pending {
        st.pending = true;
        drop(st);
        engine.request_wake();
    }
}

/// Applies the claims made while objects were being created. Called once
/// per frame and before a key is delivered; costs a flag test when there
/// is nothing to do.
pub(crate) fn settle(engine: &Engine) {
    let t = tree(engine);
    if !t.state.borrow().pending {
        return;
    }
    changed(engine, &t);
}

/// Whether claims wait for [`settle`].
pub(crate) fn pending(engine: &Engine) -> bool {
    tree(engine).state.borrow().pending
}

/// An item's visual parent changed: the focus item it brings along leaves
/// the scope it was in and joins the one it is in now.
pub(crate) fn reparented(engine: &Engine, obj: ObjectId, parent: Option<ObjectId>) {
    let Ok(old) = engine.with_mut(obj, |i: &mut Item| {
        std::mem::replace(&mut i.focus_parent, parent)
    }) else {
        return;
    };
    let t = tree(engine);
    if t.state.borrow().sub.is_empty() {
        return;
    }
    let moved: Vec<ObjectId> = match old {
        Some(p) if !engine.is_alive(p) => {
            // The old parent is gone and with it the way to its scope:
            // look at every focus item (rare, and there are few).
            let all: Vec<(ObjectId, ScopeKey)> = t
                .state
                .borrow()
                .home
                .iter()
                .map(|(o, k)| (*o, *k))
                .collect();
            all.into_iter()
                .filter(|(o, k)| engine.is_alive(*o) && scope_of(engine, *o) != *k)
                .map(|(o, _)| o)
                .collect()
        }
        _ => {
            // The one scope `obj` and its subtree were in. A scope item's
            // children are in the scope it is, which moved with it.
            let was = scope_from(engine, old, obj);
            let f = t.state.borrow().sub.get(&was).copied();
            f.filter(|f| is_under(engine, *f, obj))
                .into_iter()
                .collect()
        }
    };
    if moved.is_empty() {
        return;
    }
    for f in moved {
        release(&t, f);
        let key = scope_of(engine, f);
        let holder = t.state.borrow().sub.get(&key).copied();
        match holder.filter(|h| engine.is_alive(*h) && *h != f) {
            // As Qt: a scope keeps its focus item when another arrives.
            Some(_) => write_flag(engine, f, Item::FOCUS.index, false),
            None => {
                let mut st = t.state.borrow_mut();
                st.sub.insert(key, f);
                st.home.insert(f, key);
            }
        }
    }
    changed(engine, &t);
}

/// Drops what is known of an object that died.
pub(crate) fn forget(engine: &Engine, obj: ObjectId) {
    let t = tree(engine);
    let known = {
        let mut st = t.state.borrow_mut();
        let mut known = st.unsettled.remove(&obj);
        if let Some(key) = st.home.remove(&obj) {
            known = true;
            if st.sub.get(&key) == Some(&obj) {
                st.sub.remove(&key);
            }
        }
        for key in [ScopeKey::Item(obj), ScopeKey::Top(obj)] {
            if let Some(f) = st.sub.remove(&key) {
                known = true;
                st.home.remove(&f);
            }
        }
        known || st.windows.iter().any(|w| w.chain.contains(&obj))
    };
    if known {
        changed(engine, &t);
    }
}

// ------------------------------------------------------------------ windows

/// Makes the tree under `root` a window: its focus item gets active
/// focus. Idempotent.
pub(crate) fn add_window(engine: &Engine, root: ObjectId) {
    let t = tree(engine);
    if t.state.borrow().windows.iter().any(|w| w.root == root) {
        return;
    }
    let g = engine.graph();
    let (Ok(focus_node), Ok(active_node)) = (
        g.property(g.root(), Value::Null),
        g.property(g.root(), Value::Bool(true)),
    ) else {
        return;
    };
    t.state.borrow_mut().windows.push(Win {
        root,
        active: true,
        chain: Vec::new(),
        focus_node,
        active_node,
    });
    bump(engine, &t.windows_epoch);
    ensure_effect(engine, &t);
    changed(engine, &t);
}

/// The tree under `root` is no longer a window: nothing in it has active
/// focus.
pub(crate) fn remove_window(engine: &Engine, root: ObjectId) {
    let t = tree(engine);
    let win = {
        let mut st = t.state.borrow_mut();
        let at = st.windows.iter().position(|w| w.root == root);
        at.map(|i| st.windows.remove(i))
    };
    let Some(win) = win else { return };
    for o in win.chain.iter().rev() {
        write_flag(engine, *o, Item::ACTIVE_FOCUS.index, false);
    }
    let g = engine.graph();
    let _ = g.dispose(win.focus_node);
    let _ = g.dispose(win.active_node);
    bump(engine, &t.windows_epoch);
    changed(engine, &t);
}

/// Tells the tree whether the window of `root` has the keyboard. Active
/// focus leaves when it does not and comes back, to the same item, when
/// it does again; `focus` stays as it is.
pub(crate) fn set_window_active(engine: &Engine, root: ObjectId, active: bool) {
    let t = tree(engine);
    let node = {
        let mut st = t.state.borrow_mut();
        let Some(w) = st.windows.iter_mut().find(|w| w.root == root) else {
            return;
        };
        if w.active == active {
            return;
        }
        w.active = active;
        let n = w.active_node;
        st.reason = FocusReason::ActiveWindow;
        n
    };
    engine.graph().set(node, Value::Bool(active));
    changed(engine, &t);
}

/// Whether the tree under `root` is a window.
pub(crate) fn is_window(engine: &Engine, root: ObjectId) -> bool {
    let t = tree(engine);
    let st = t.state.borrow();
    st.windows.iter().any(|w| w.root == root)
}

/// Whether the window of `root` has the keyboard. False for a tree that
/// is no window.
pub fn is_window_active(engine: &Engine, root: ObjectId) -> bool {
    let t = tree(engine);
    let st = t.state.borrow();
    st.windows.iter().any(|w| w.root == root && w.active)
}

/// The item with active focus in the window of `root`: the one the keys
/// go to. Not a tracked read; bindings use `Window.activeFocusItem`.
pub fn active_focus_item(engine: &Engine, root: ObjectId) -> Option<ObjectId> {
    let t = tree(engine);
    let st = t.state.borrow();
    let w = st.windows.iter().find(|w| w.root == root)?;
    w.chain.last().copied().filter(|o| engine.is_alive(*o))
}

/// The graph nodes behind `Window.activeFocusItem` and `Window.active`
/// for the window of `root`. Reads the set of windows as a dependency, so
/// a binding that found none asks again when one appears.
pub(crate) fn window_nodes(engine: &Engine, root: ObjectId) -> Option<(NodeId, NodeId)> {
    let t = tree(engine);
    if let Some(n) = node(engine, &t.windows_epoch) {
        engine.graph().get(n);
    }
    let st = t.state.borrow();
    st.windows
        .iter()
        .find(|w| w.root == root)
        .map(|w| (w.focus_node, w.active_node))
}

/// The focus item of the scope `scope` (a focus scope item) keeps for
/// its children.
pub fn scope_focus_item(engine: &Engine, scope: ObjectId) -> Option<ObjectId> {
    let t = tree(engine);
    let st = t.state.borrow();
    st.sub
        .get(&ScopeKey::Item(scope))
        .copied()
        .filter(|o| engine.is_alive(*o))
}

/// Why `item` last gained or lost active focus.
pub fn focus_reason(engine: &Engine, item: ObjectId) -> FocusReason {
    engine
        .with(item, |i: &Item| FocusReason::from_qt(i.focus_reason))
        .unwrap_or_default()
}

/// `forceActiveFocus(reason)`: gives `item` focus in its scope, and every
/// scope around it focus in the one around that, so the item has active
/// focus if its window is active (doc.qt.io/qt-6/qml-qtquick-item.html,
/// `forceActiveFocus`).
pub fn force_active_focus(engine: &Engine, item: ObjectId, reason: FocusReason) {
    if !crate::item::is_item(engine, item) {
        return;
    }
    let t = tree(engine);
    // The item, then the scopes around it, innermost first.
    let mut path = vec![item];
    let mut cur = item;
    for _ in 0..DEPTH {
        match scope_of(engine, cur) {
            ScopeKey::Item(s) => {
                path.push(s);
                cur = s;
            }
            ScopeKey::Top(_) => break,
        }
    }
    t.state.borrow_mut().reason = reason;
    let mut gained = Vec::new();
    for o in path {
        if t.state.borrow().home.contains_key(&o) {
            continue;
        }
        claim(engine, &t, o);
        gained.push(o);
    }
    // The state is whole before any handler of `focusChanged` runs.
    for o in gained {
        write_flag(engine, o, Item::FOCUS.index, true);
    }
    changed(engine, &t);
}

// ------------------------------------------------------------------ applying

/// The items that should have active focus in the tree under `root`,
/// outermost first: the tree's focus item, then, while that is a scope,
/// the scope's. Ends at an item that is not enabled. Tracks `enabled` and
/// the parents it reads when called inside an effect.
fn path(engine: &Engine, t: &FocusTree, root: ObjectId) -> Vec<ObjectId> {
    let mut out = Vec::new();
    if !engine.is_alive(root) {
        return out;
    }
    let mut key = ScopeKey::Top(root);
    // Everything above the scope item was checked on the way to it.
    let mut checked: Option<ObjectId> = None;
    for _ in 0..DEPTH {
        let Some(f) = t.state.borrow().sub.get(&key).copied() else {
            break;
        };
        if !engine.is_alive(f) {
            let mut st = t.state.borrow_mut();
            st.sub.remove(&key);
            st.home.remove(&f);
            break;
        }
        let mut cur = Some(f);
        for _ in 0..DEPTH {
            let Some(o) = cur.filter(|o| Some(*o) != checked) else {
                break;
            };
            if !engine.read(o, Item::ENABLED.index).to_boolean() {
                return out;
            }
            cur = engine.visual_parent(o);
        }
        out.push(f);
        if !is_focus_scope(engine, f) {
            break;
        }
        checked = Some(f);
        key = ScopeKey::Item(f);
    }
    out
}

/// One effect for all windows: it reads `enabled` along every path, so a
/// focus item that is disabled, enabled again, moved or destroyed brings
/// the paths up to date without anything polling.
fn ensure_effect(engine: &Engine, t: &FocusTree) {
    if t.effect.get().is_some() {
        return;
    }
    let made = engine.effect(|e| {
        let t = tree(e);
        if let Some(n) = node(e, &t.epoch) {
            e.graph().get(n);
        }
        let roots: Vec<ObjectId> = t.state.borrow().windows.iter().map(|w| w.root).collect();
        for root in roots {
            path(e, &t, root);
        }
        e.untrack(|| apply_all(e));
    });
    match made {
        Ok(n) => t.effect.set(Some(n)),
        Err(err) => warn(engine, "cannot follow the focus items", err),
    }
}

/// The focus state changed: bring `activeFocus` in step with it.
fn changed(engine: &Engine, t: &FocusTree) {
    {
        let mut st = t.state.borrow_mut();
        if st.applying {
            // The loop in `apply_all` looks at the state again by itself;
            // the effect is told once that loop is over.
            st.stale = true;
            return;
        }
    }
    // Wakes the effect, which reads the new paths. Inside a batch it
    // runs late, so the state is applied here as well.
    bump(engine, &t.epoch);
    engine.untrack(|| apply_all(engine));
}

enum Step {
    Lose(ObjectId),
    Gain(ObjectId),
}

/// Clears `applying` however the loop is left.
struct Applying<'a>(&'a FocusTree);

impl Drop for Applying<'_> {
    fn drop(&mut self) {
        let mut st = self.0.state.borrow_mut();
        st.applying = false;
        st.reason = FocusReason::Other;
    }
}

/// Writes `activeFocus` where the windows' paths changed: first the items
/// that lose it, innermost first, then those that gain it, outermost
/// first, the order Qt documents for a focus scope. One write at a time,
/// looking at the state again after each, because a handler of
/// `activeFocusChanged` may move focus itself.
pub(crate) fn apply_all(engine: &Engine) {
    let t = tree(engine);
    {
        let mut st = t.state.borrow_mut();
        st.pending = false;
        st.unsettled.clear();
        if st.applying {
            st.stale = true;
            return;
        }
        st.windows.retain(|w| engine.is_alive(w.root));
        if st.windows.is_empty() {
            return;
        }
        st.applying = true;
    }
    {
        let _guard = Applying(&t);
        let mut steps = 0;
        while let Some(step) = next_step(engine, &t) {
            steps += 1;
            if steps > MAX_STEPS {
                engine.log(
                    LogLevel::Warning,
                    CATEGORY,
                    "active focus does not settle: handlers keep moving it".to_string(),
                );
                break;
            }
            match step {
                Step::Lose(o) => {
                    // Before the flag, as for a gain: a handler of the
                    // change reads why.
                    let reason = t.state.borrow().reason as i32;
                    let _ = engine.with_mut(o, |i: &mut Item| i.focus_reason = reason);
                    write_flag(engine, o, Item::ACTIVE_FOCUS.index, false);
                }
                Step::Gain(o) => {
                    let reason = t.state.borrow().reason as i32;
                    let _ = engine.with_mut(o, |i: &mut Item| i.focus_reason = reason);
                    write_flag(engine, o, Item::ACTIVE_FOCUS.index, true);
                }
            }
        }
    }
    let stale = std::mem::take(&mut t.state.borrow_mut().stale);
    if stale {
        bump(engine, &t.epoch);
    }
}

/// The next `activeFocus` write, already noted in the window's path.
fn next_step(engine: &Engine, t: &FocusTree) -> Option<Step> {
    let count = t.state.borrow().windows.len();
    for i in 0..count {
        let (root, active, chain, focus_node) = {
            let st = t.state.borrow();
            let w = st.windows.get(i)?;
            (w.root, w.active, w.chain.clone(), w.focus_node)
        };
        let target = if active {
            path(engine, t, root)
        } else {
            Vec::new()
        };
        // `Window.activeFocusItem` says where focus is going before the
        // items hear of it, so their handlers read the new one.
        let g = engine.graph();
        let now = target.last().map_or(Value::Null, |o| Value::Object(*o));
        if !g.get_untracked(focus_node).strict_equals(&now) {
            g.set(focus_node, now);
        }
        let common = chain
            .iter()
            .zip(&target)
            .take_while(|(a, b)| a == b)
            .count();
        let mut st = t.state.borrow_mut();
        let w = st.windows.get_mut(i).filter(|w| w.root == root)?;
        if chain.len() > common {
            let o = w.chain.pop()?;
            return Some(Step::Lose(o));
        }
        if let Some(o) = target.get(chain.len()).copied() {
            w.chain.push(o);
            return Some(Step::Gain(o));
        }
    }
    None
}
