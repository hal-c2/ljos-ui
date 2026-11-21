//! What the objects hold, for a tool that shows where memory went.
//!
//! Estimates. An object counts its header, the struct its type keeps
//! natively, the slots of its declared properties, and the tables that
//! grew beside them; what those point at (strings, lists, script values)
//! is not followed. Nothing is kept up to date for this: it is one pass
//! over the objects, made when asked.

use std::collections::HashMap;
use std::mem::size_of;
use std::sync::Arc;

use smallvec::SmallVec;

use crate::atom::Atom;
use crate::engine::Engine;
use crate::ids::ObjectId;
use crate::object::{ObjectExt, ObjectHeader};
use crate::value::Value;

/// The compiled form of the files loaded, the application's and its
/// libraries'.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompiledMemory {
    pub files: usize,
    /// About what they hold ([`crate::loader`]'s `LoadedFile::bytes`).
    pub bytes: usize,
}

/// What a crate built on the engine holds outside the objects, as a
/// memory tool shows it beside the engine's own owners
/// ([`Engine::add_memory_owner`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerMemory {
    /// A stable key (`media`).
    pub key: &'static str,
    /// What a person reads (`decoded media`).
    pub label: &'static str,
    pub bytes: usize,
    pub count: usize,
    /// What `count` counts (`frames`).
    pub of: &'static str,
}

/// The owners crates told of: each is asked when a tool asks, never
/// kept up to date.
#[derive(Default)]
pub(crate) struct MemoryOwners(std::cell::RefCell<Vec<Box<dyn Fn() -> OwnerMemory>>>);

/// The script values the objects of one type hold in their properties,
/// and about what those reach in the script heap
/// ([`Engine::script_heap_holders`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptHolder {
    /// The type's name, as [`TypeMemory::name`] gives it.
    pub name: String,
    /// Its objects that hold a script value.
    pub objects: usize,
    /// About the bytes reached, each value counted for the first type that
    /// reaches it.
    pub bytes: usize,
}

/// What holds the script heap, by the types whose properties hold it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptHolders {
    /// The most bytes first.
    pub holders: Vec<ScriptHolder>,
    /// The script values walked.
    pub values: usize,
    /// The walk reached its budget, or a read failed; the rest was not
    /// counted.
    pub truncated: bool,
}

/// Whether `v` holds a script value, pushing to `out` the ones it reaches
/// through lists and maps no earlier call went through. Gone through with a
/// stack of its own, so a nesting deeper than the thread's stack is no
/// danger, and at most `left` list and map entries in all; `None` when
/// that ran out. `seen` remembers, by where it lives, each list or map gone
/// through and whether it holds a script value, across every call: a list
/// many objects share is charged to `left` and copied out once, and each of
/// them is still told it holds one. Its values are counted for the first
/// object to reach it either way (`held_by` counts each once). A value
/// that holds the same list twice at each level is gone through once too,
/// not exponentially often.
fn push_js<'v>(
    v: &'v Value,
    seen: &mut HashMap<usize, bool>,
    left: &mut usize,
    out: &mut Vec<&'v ljos_js::JsRef>,
) -> Option<bool> {
    /// A list or map being gone through.
    struct Open<'v> {
        at: usize,
        rest: Box<dyn Iterator<Item = &'v Value> + 'v>,
        holds: bool,
    }
    // Takes `n` entries from what is left, or nothing when they are more.
    let take = |left: &mut usize, n: usize| {
        let enough = n <= *left;
        *left = if enough { *left - n } else { 0 };
        enough
    };
    let mut open: Vec<Open<'v>> = Vec::new();
    let mut next = Some(v);
    let mut holds = false;
    loop {
        if let Some(v) = next.take() {
            let done = match v {
                Value::Js(h) => {
                    out.push(&h.0);
                    Some(true)
                }
                Value::List(items) => {
                    let at = Arc::as_ptr(items) as usize;
                    match seen.get(&at) {
                        Some(&h) => Some(h),
                        None if take(left, items.len()) => {
                            open.push(Open {
                                at,
                                rest: Box::new(items.iter()),
                                holds: false,
                            });
                            None
                        }
                        None => return None,
                    }
                }
                Value::Map(fields) => {
                    let at = Arc::as_ptr(fields) as usize;
                    match seen.get(&at) {
                        Some(&h) => Some(h),
                        None if take(left, fields.len()) => {
                            open.push(Open {
                                at,
                                rest: Box::new(fields.iter().map(|(_, v)| v)),
                                holds: false,
                            });
                            None
                        }
                        None => return None,
                    }
                }
                _ => Some(false),
            };
            if let Some(h) = done {
                match open.last_mut() {
                    Some(up) => up.holds |= h,
                    None => holds = h,
                }
            }
        }
        let Some(top) = open.last_mut() else {
            return Some(holds);
        };
        if let Some(v) = top.rest.next() {
            next = Some(v);
            continue;
        }
        let Some(done) = open.pop() else {
            return Some(holds);
        };
        seen.insert(done.at, done.holds);
        match open.last_mut() {
            Some(up) => up.holds |= done.holds,
            None => holds = done.holds,
        }
    }
}

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

    /// Tells a memory tool of what a crate built on the engine holds that
    /// no object counts (a decoder's frames, a device's buffers), so it is
    /// shown under its own owner rather than among what nobody told of.
    /// `measure` runs when a tool asks, on the Engine thread.
    pub fn add_memory_owner(&self, measure: impl Fn() -> OwnerMemory + 'static) {
        self.extension::<MemoryOwners>()
            .0
            .borrow_mut()
            .push(Box::new(measure));
    }

    /// What the owners [`Engine::add_memory_owner`] told of hold now.
    pub fn memory_owners(&self) -> Vec<OwnerMemory> {
        let owners = self.extension::<MemoryOwners>();
        owners.0.borrow().iter().map(|m| m()).collect()
    }

    /// The files loaded and about the bytes their compiled form holds.
    pub fn compiled_memory(&self) -> CompiledMemory {
        let loader = self.inner.loader.borrow();
        loader.files.values().chain(loader.qml_types.loaded()).fold(
            CompiledMemory::default(),
            |m, f| CompiledMemory {
                files: m.files + 1,
                bytes: m.bytes + f.bytes(),
            },
        )
    }

    /// What holds the script heap: the script values each type's objects
    /// keep in their properties, walked for what they reach
    /// ([`ljos_js::Scope::held_by`]) and counted for the first type to
    /// reach them. What only a running function's closure or a script's
    /// globals hold is not reached from any object, so the holders may add
    /// up to less than [`Engine::script_heap_bytes`]. Walks up to `budget`
    /// values and property reads, and as many list and map entries in the
    /// objects' properties, when asked, never on its own. `None` before any script ran.
    pub fn script_heap_holders(&self, budget: usize) -> Option<ScriptHolders> {
        if !self.js_started() {
            return None;
        }
        let mut groups: Vec<(Atom, usize, Vec<ljos_js::JsRef>)> = Vec::new();
        let mut cut = false;
        {
            let store = self.inner.store.borrow();
            let mut at: HashMap<Atom, usize> = HashMap::new();
            let mut refs = Vec::new();
            let mut seen = HashMap::new();
            let mut left = budget;
            for header in store.objects.values() {
                refs.clear();
                // A list two objects share makes both holders; what it
                // reaches is gone through once, for the first (`seen`).
                let mut holds = false;
                let mut whole = true;
                for v in header.dynamic.iter() {
                    match push_js(v, &mut seen, &mut left, &mut refs) {
                        Some(h) => holds |= h,
                        None => {
                            whole = false;
                            break;
                        }
                    }
                }
                if holds || !refs.is_empty() {
                    let i = *at.entry(header.meta.name.clone()).or_insert_with(|| {
                        groups.push((header.meta.name.clone(), 0, Vec::new()));
                        groups.len() - 1
                    });
                    groups[i].1 += 1;
                    groups[i].2.extend(refs.iter().map(|r| (*r).clone()));
                }
                if !whole {
                    cut = true;
                    break;
                }
            }
        }
        // Entering and leaving the script context may reach the store (a
        // fault report, the jobs it runs after), so its borrow ends first.
        let held = self.run_js("heap holders", |s| {
            Ok(s.held_by(groups.iter().map(|g| g.2.iter()), budget))
        })?;
        let mut holders: Vec<ScriptHolder> = groups
            .into_iter()
            .zip(held.bytes)
            .map(|((name, objects, _), bytes)| ScriptHolder {
                name: name.to_string(),
                objects,
                bytes,
            })
            .collect();
        holders.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
        Some(ScriptHolders {
            holders,
            values: held.values,
            truncated: held.truncated || cut,
        })
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

    /// Puts a new limit on the script heap, `None` for none, in place of
    /// the 96 MiB the engine starts with (`ljos_js::JsConfig`); what is
    /// held already stays. Starts the script engine. False inside a
    /// script, and when the script engine could not start.
    pub fn set_script_heap_limit(&self, limit: Option<usize>) -> bool {
        self.js().is_ok_and(|js| js.host.set_memory_limit(limit))
    }
}
