//! What the objects hold, for a tool that shows where memory went.
//!
//! Estimates. An object counts its header, the struct its type keeps
//! natively, the slots of its declared properties, and the tables that
//! grew beside them; what those point at (strings, lists, script values)
//! is not followed. Nothing is kept up to date for this: it is one pass
//! over the objects, made when asked.

use std::collections::HashMap;
use std::mem::size_of;

use smallvec::SmallVec;

use crate::atom::Atom;
use crate::engine::Engine;
use crate::ids::ObjectId;
use crate::object::{ObjectExt, ObjectHeader};
use crate::value::Value;

/// The objects of one type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeMemory {
    /// The type's name, as `app_objects` gives it.
    pub name: String,
    pub count: usize,
    /// About what they hold.
    pub bytes: usize,
}

fn vec_bytes<T>(v: &Vec<T>) -> usize {
    v.capacity() * size_of::<T>()
}

fn small_bytes<A: smallvec::Array>(v: &SmallVec<A>) -> usize {
    if v.spilled() {
        v.capacity() * size_of::<A::Item>()
    } else {
        0
    }
}

impl ObjectExt {
    fn bytes(&self) -> usize {
        size_of::<ObjectExt>()
            + vec_bytes(&self.aliases)
            + vec_bytes(&self.methods)
            + vec_bytes(&self.direct)
            + vec_bytes(&self.binding_locs)
            + vec_bytes(&self.contexts)
            + vec_bytes(&self.keep)
            + self.made_on_read.bytes()
    }
}

impl ObjectHeader {
    pub(crate) fn bytes(&self) -> usize {
        size_of::<ObjectHeader>()
            + size_of_val(&*self.native)
            + self.dynamic.len() * size_of::<Value>()
            + vec_bytes(&self.children)
            + vec_bytes(&self.visual_children)
            + small_bytes(&self.attached)
            + self.ext.as_deref().map_or(0, ObjectExt::bytes)
            + self.nodes.as_deref().map_or(0, size_of_val)
            + small_bytes(&self.assigned)
            + self.field_reads.as_deref().map_or(0, |r| r.bytes())
    }
}

impl Engine {
    /// About the bytes `obj` holds; `None` once it is gone.
    pub fn object_bytes(&self, obj: ObjectId) -> Option<usize> {
        Some(self.inner.store.borrow().objects.get(obj)?.bytes())
    }

    /// Every object alive, in the order of their places in the store:
    /// for a tool that looks at each once (an inspector's model list).
    pub fn object_ids(&self) -> Vec<ObjectId> {
        self.inner.store.borrow().objects.keys().collect()
    }

    /// The objects by type, the most bytes first.
    pub fn object_memory(&self) -> Vec<TypeMemory> {
        let store = self.inner.store.borrow();
        let mut by_type: HashMap<Atom, (usize, usize)> = HashMap::new();
        for header in store.objects.values() {
            let entry = by_type.entry(header.meta.name.clone()).or_default();
            entry.0 += 1;
            entry.1 += header.bytes();
        }
        let mut types: Vec<TypeMemory> = by_type
            .into_iter()
            .map(|(name, (count, bytes))| TypeMemory {
                name: name.to_string(),
                count,
                bytes,
            })
            .collect();
        types.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
        types
    }

    /// About the bytes of the reactive graph's own tables
    /// ([`ljos_reactive::Graph::bytes`]).
    pub fn graph_bytes(&self) -> usize {
        self.inner.graph.bytes()
    }

    /// What the script engine holds from the system allocator, as the
    /// allocator counts it: read, not walked, so it may be asked at any
    /// time. `None` before any script ran.
    pub fn script_heap_bytes(&self) -> Option<usize> {
        if !self.js_started() {
            return None;
        }
        Some(self.js().ok()?.host.heap_bytes())
    }
}
