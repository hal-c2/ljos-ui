//! Which contributions are alive for each slot name, kept by the engine.
//!
//! A `Slot` reads a name's version, tracked, and so runs again exactly
//! when a contribution of that name comes, goes or moves: nothing polls
//! and nothing walks the tree.

use std::cell::RefCell;
use std::collections::HashMap;

use ljos_engine::{Engine, NodeId, ObjectId, Value};

#[derive(Default)]
pub(super) struct Registry {
    names: RefCell<HashMap<String, Name>>,
}

#[derive(Default)]
struct Name {
    members: Vec<ObjectId>,
    /// Made when a Slot first reads the name, so a name nobody shows
    /// costs no node.
    version: Option<NodeId>,
}

impl Registry {
    pub(super) fn of(e: &Engine) -> std::rc::Rc<Self> {
        e.extension::<Self>()
    }

    pub(super) fn add(&self, e: &Engine, name: &str, obj: ObjectId) {
        let bump = {
            let mut names = self.names.borrow_mut();
            let entry = names.entry(name.to_owned()).or_default();
            if entry.members.contains(&obj) {
                return;
            }
            entry.members.push(obj);
            entry.version
        };
        bump_node(e, bump);
    }

    pub(super) fn remove(&self, e: &Engine, name: &str, obj: ObjectId) {
        let bump = {
            let mut names = self.names.borrow_mut();
            let Some(entry) = names.get_mut(name) else {
                return;
            };
            let before = entry.members.len();
            entry.members.retain(|o| *o != obj);
            let removed = entry.members.len() != before;
            let version = entry.version;
            if entry.members.is_empty() && version.is_none() {
                names.remove(name);
            }
            version.filter(|_| removed)
        };
        bump_node(e, bump);
    }

    /// Tells the Slots of `name` that one of its contributions is gone,
    /// for one that was only listed in a Slot and never registered.
    pub(super) fn touch(&self, e: &Engine, name: &str) {
        let bump = self.names.borrow().get(name).and_then(|n| n.version);
        bump_node(e, bump);
    }

    /// The contributions of `name`, read so that the running effect runs
    /// again when they change.
    pub(super) fn tracked(&self, e: &Engine, name: &str) -> Vec<ObjectId> {
        let node = {
            let mut names = self.names.borrow_mut();
            let entry = names.entry(name.to_owned()).or_default();
            match entry.version {
                Some(n) => Some(n),
                None => {
                    let n = e.graph().property(e.graph().root(), Value::Int(0)).ok();
                    entry.version = n;
                    n
                }
            }
        };
        if let Some(n) = node {
            let _ = e.graph().try_get(n);
        }
        self.names
            .borrow()
            .get(name)
            .map(|n| n.members.clone())
            .unwrap_or_default()
    }
}

fn bump_node(e: &Engine, node: Option<NodeId>) {
    let Some(n) = node else { return };
    // Only a change is followed, so it goes round rather than over.
    let next = (e.graph().get_untracked(n).to_number() as i32).wrapping_add(1);
    e.graph().set(n, Value::Int(next));
}
