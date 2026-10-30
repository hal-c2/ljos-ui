//! `Slot`: a place in a shell that shows what is contributed to it.
//!
//! A Slot makes one item of each contribution's delegate and puts them in
//! its own parent right before itself, as a Repeater does, so a
//! positioner or a layout around the Slot lays them out. With nothing
//! contributed it shows an item of its own `delegate`.
//!
//! The item made of a contribution is owned by the contribution, not the
//! Slot: a fault in it is the fault of whatever boundary the contribution
//! was made under (a plugin's Loader), and when that boundary discards
//! its content the item goes with the contribution.
//!
//! What the Slot shows, in order: contributions sorted by `order`, then
//! by when they were made, laid over the Slot's own delegate. An Append
//! goes at the end; a Prepend before the delegate and after the Prepends
//! before it; a Replace takes the place of everything so far.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, LogLevel, ObjectId, QmlList, QmlObject, SignalIndex, Value,
    qml_impl,
};

use super::contribution::{Contribution, contribution_mode, slot_of, text};
use super::registry::Registry;
use crate::item::{Item, is_item};
use crate::repeater::place_before;
use crate::tree;

/// `Slot` (Ljos.Blocks).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Ljos.Blocks", name = "Slot", extends = Item, default = "delegate")]
pub struct Slot {
    #[qml(base)]
    pub item_base: Item,
    /// Contributions whose `slot` is this name show here.
    #[qml(notify)]
    pub name: Atom,
    /// Contributions shown here whatever their `slot`: a block passes on
    /// what was given to its own `list<Contribution>` property.
    #[qml(notify, list, holds = "Contribution")]
    pub contributions: QmlList,
    /// What shows while no contribution replaces it.
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    /// How many items the Slot shows.
    #[qml(notify, readonly)]
    pub count: i32,
}

/// Where an item came from: a contribution, or the Slot's own delegate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    from: Option<ObjectId>,
    component: ObjectId,
}

#[derive(Debug)]
struct Shown {
    key: Key,
    item: ObjectId,
}

#[qml_impl]
impl Slot {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let shown: Rc<RefCell<Vec<Shown>>> = Rc::default();
        let parent = std::cell::Cell::new(None);
        let state = shown.clone();
        e.effect_in(me, move |e| {
            let wanted = wanted(e, me);
            // Tracked: the items live in the parent.
            let now = e.visual_parent(me);
            let moved = parent.replace(now) != now;
            e.untrack(|| show(e, me, now, moved, &state, wanted));
        })?;
        // What contributions own outlives the Slot unless it is let go.
        e.connect(me, SignalIndex::DESTROYED, move |e, _| {
            for s in shown.borrow_mut().drain(..) {
                if s.key.from.is_some() {
                    discard(e, s.item);
                }
            }
        })?;
        Ok(())
    }
}

/// What the Slot should show, in order. Read tracked: a change to any of
/// it runs the Slot's effect again.
fn wanted(e: &Engine, me: ObjectId) -> Vec<Key> {
    let registry = Registry::of(e);
    let name = text(&e.read(me, Slot::NAME.index));
    let own = e
        .read(me, Slot::DELEGATE.index)
        .as_object()
        .filter(|d| e.is_alive(*d));
    let mut all: Vec<ObjectId> = Vec::new();
    if let Value::List(given) = e.read(me, Slot::CONTRIBUTIONS.index) {
        for c in given.iter().filter_map(Value::as_object) {
            if !e.is_alive(c) || e.with(c, |_: &Contribution| ()).is_err() {
                continue;
            }
            // Its going is told under its own name.
            registry.tracked(e, &slot_of(e, c));
            if !all.contains(&c) {
                all.push(c);
            }
        }
    }
    if !name.is_empty() {
        for c in registry.tracked(e, &name) {
            if e.is_alive(c) && !all.contains(&c) {
                all.push(c);
            }
        }
    }
    let mut sorted: Vec<(i32, u64, i32, Key)> = all
        .into_iter()
        .filter_map(|c| {
            let component = e
                .read(c, Contribution::DELEGATE.index)
                .as_object()
                .filter(|d| e.is_alive(*d))?;
            let order = e.read(c, Contribution::ORDER.index).to_number() as i32;
            let mode = e.read(c, Contribution::MODE.index).to_number() as i32;
            let made = e.with(c, |c: &Contribution| c.made).ok()?;
            let key = Key {
                from: Some(c),
                component,
            };
            Some((order, made, mode, key))
        })
        .collect();
    sorted.sort_by_key(|(order, made, _, _)| (*order, *made));
    let mut out: Vec<Key> = own
        .map(|component| Key {
            from: None,
            component,
        })
        .into_iter()
        .collect();
    let mut front = 0;
    for (_, _, mode, key) in sorted {
        match mode {
            contribution_mode::REPLACE => {
                out.clear();
                out.push(key);
                front = 0;
            }
            contribution_mode::PREPEND => {
                out.insert(front, key);
                front += 1;
            }
            _ => out.push(key),
        }
    }
    out
}

/// Makes what is wanted and not shown, lets go of what is shown and not
/// wanted, and puts the items in order before the Slot.
fn show(
    e: &Engine,
    me: ObjectId,
    parent: Option<ObjectId>,
    moved: bool,
    shown: &RefCell<Vec<Shown>>,
    wanted: Vec<Key>,
) {
    let mut old = std::mem::take(&mut *shown.borrow_mut());
    old.retain(|s| e.is_alive(s.item));
    let mut next = Vec::with_capacity(wanted.len());
    let mut fresh = false;
    for key in wanted {
        if let Some(at) = old.iter().position(|s| s.key == key) {
            next.push(old.remove(at));
            continue;
        }
        let owner = key.from.unwrap_or(me);
        let made = e.component_create_owned(key.component, owner, |e, item| {
            if let Some(parent) = parent
                && is_item(e, item)
            {
                let _ = tree::set_parent_item_before(e, item, parent, me);
            }
        });
        match made {
            Ok(item) => {
                if !is_item(e, item) {
                    warn(e, "QML Slot: a delegate must be of Item type".into());
                }
                fresh = true;
                next.push(Shown { key, item });
            }
            Err(err) => warn(e, format!("QML Slot: cannot make a delegate: {err}")),
        }
    }
    let gone = !old.is_empty();
    for s in old {
        discard(e, s.item);
    }
    let items: Vec<ObjectId> = next
        .iter()
        .map(|s| s.item)
        .filter(|i| is_item(e, *i))
        .collect();
    if let Some(parent) = parent
        && (fresh || gone || moved)
    {
        let r = e.batch(|| {
            for item in &items {
                tree::set_parent_item_before(e, *item, parent, me)?;
            }
            place_before(e, parent, &items, me)
        });
        if let Err(err) = r {
            warn(e, format!("QML Slot: cannot place a delegate: {err}"));
        }
    }
    let count = i32::try_from(next.len()).unwrap_or(i32::MAX);
    *shown.borrow_mut() = next;
    if let Err(err) = e.set_internal(me, Slot::COUNT, count) {
        warn(e, format!("QML Slot: {err}"));
    }
}

fn warn(e: &Engine, message: String) {
    e.log(LogLevel::Warning, "qml", message);
}

/// Takes `obj` out of the tree now and destroys it at the next frame: its
/// own handler may be what is running.
fn discard(e: &Engine, obj: ObjectId) {
    if !e.is_alive(obj) {
        return;
    }
    e.announce_destruction(obj);
    if is_item(e, obj) {
        let _ = tree::set_parent_item(e, obj, None);
    }
    e.destroy(obj);
}
