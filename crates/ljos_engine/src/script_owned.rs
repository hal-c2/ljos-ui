//! Objects that belong to the script holding them (ARCHITECTURE.md §9:
//! "a JS-owned object is destroyed by its finaliser").
//!
//! Such an object has no owner in the object tree. Its wrapper is not
//! kept by the engine, so the JS collector frees it when no script holds
//! it any more, and the object goes with it.
//!
//! A script can also put the object in a property (`property var
//! incubator`). There it is an id, which keeps no wrapper alive, so the
//! engine holds the wrapper for the property: [`Engine::for_slot`] says
//! when one is written, and the properties are looked through now and
//! then for the ones that have let go since. Looking through them is the
//! price of not counting every write to every property; it is paid only
//! by an application that has such objects, and only when it stores or
//! drops one.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use ljos_js::JsRef;
use ljos_runtime::Fault;

use crate::engine::Engine;
use crate::fault::FaultReport;
use crate::ids::ObjectId;
use crate::js::{obj_to_u64, u64_to_obj};
use crate::object::ObjectFlags;
use crate::value::Value;

/// How many objects properties hold before the properties are first
/// looked through for those that let go.
const FIRST_REVIEW: usize = 16;

#[derive(Default)]
struct ScriptOwned {
    /// The script-owned objects a property holds, each with its wrapper:
    /// the object is the same object to scripts for as long.
    held: RefCell<HashMap<ObjectId, JsRef>>,
    /// `held` is looked through when it has grown to this.
    review_at: Cell<usize>,
}

impl Engine {
    /// Gives `obj` to the scripts: it lives as long as one holds it or a
    /// property does. Only before `obj` first crosses to JS; false, and
    /// nothing changes, after that or without a JS host.
    pub fn give_to_script(&self, obj: ObjectId) -> bool {
        let Ok(js) = self.js() else { return false };
        if !js.host.give_to_script(obj_to_u64(obj)) {
            return false;
        }
        let mut store = self.inner.store.borrow_mut();
        let Some(h) = store.objects.get_mut(obj) else {
            return false;
        };
        h.flags |= ObjectFlags::JS_OWNED;
        true
    }

    /// `obj`, given to the scripts by [`Engine::give_to_script`], was
    /// handed to them (a signal's argument): it lives on while a script
    /// keeps it, as a value a handler is given does, and goes at the end
    /// of the turn when none took it up, since then none ever will.
    pub fn leave_to_scripts(&self, obj: ObjectId) {
        let held = self.is_script_owned(obj)
            && self
                .js()
                .is_ok_and(|js| js.host.script_holds(obj_to_u64(obj)));
        if !held {
            self.destroy_at_turn_end(obj);
        }
    }

    fn is_script_owned(&self, obj: ObjectId) -> bool {
        self.flags(obj).is_some_and(|f| {
            f.contains(ObjectFlags::JS_OWNED) && !f.contains(ObjectFlags::PENDING_DELETE)
        })
    }

    /// `obj` is about to be the value of a property: a script-owned
    /// object is held for it from now on.
    pub(crate) fn held_by_property(&self, obj: ObjectId) {
        if !self.is_script_owned(obj) {
            return;
        }
        let owned = self.extension::<ScriptOwned>();
        if owned.held.borrow().contains_key(&obj) {
            return;
        }
        let n = owned.held.borrow().len();
        if n >= owned.review_at.get().max(FIRST_REVIEW) {
            self.review_held(&owned);
            // Twice what is left, so a property that is given one object
            // after another costs a look only now and then.
            let left = owned.held.borrow().len();
            owned.review_at.set((left * 2).max(FIRST_REVIEW));
        }
        self.hold(&owned, obj);
    }

    fn hold(&self, owned: &ScriptOwned, obj: ObjectId) {
        let face = self.run_js("hold", |s| {
            Ok(s.keep(s.wrapper(obj_to_u64(obj))?.into_value()))
        });
        if let Some(face) = face {
            owned.held.borrow_mut().insert(obj, face);
        }
    }

    /// Lets go of the held objects no property holds any more. Their
    /// wrappers go at the next collection, unless a script holds them.
    fn review_held(&self, owned: &ScriptOwned) {
        let ids: HashSet<ObjectId> = owned.held.borrow().keys().copied().collect();
        if ids.is_empty() {
            return;
        }
        let still = self.held_in_properties(&ids);
        // Dropped outside the borrow: a reference let go may be the last.
        let dropped: Vec<JsRef> = {
            let mut held = owned.held.borrow_mut();
            let gone: Vec<ObjectId> = held
                .keys()
                .filter(|o| !still.contains(o))
                .copied()
                .collect();
            gone.iter().filter_map(|o| held.remove(o)).collect()
        };
        drop(dropped);
    }

    /// The collector freed the wrappers of `released`: destroys those
    /// that nothing holds any more.
    pub(crate) fn scripts_let_go(&self, released: &[u64]) {
        let Ok(js) = self.js() else { return };
        let candidates: HashSet<ObjectId> = released
            .iter()
            .map(|o| u64_to_obj(*o))
            .filter(|o| self.is_script_owned(*o))
            // A script took it up again: its new wrapper will be heard of.
            .filter(|o| !js.host.script_holds(obj_to_u64(*o)))
            .collect();
        if candidates.is_empty() {
            return;
        }
        // A property can hold one without `held_by_property` having
        // heard of it (inside a list): that one stays.
        let held = self.held_in_properties(&candidates);
        let owned = self.extension::<ScriptOwned>();
        for o in candidates {
            if held.contains(&o) {
                self.hold(&owned, o);
            } else {
                self.destroy(o);
            }
        }
    }

    /// Which of `of` a QML-declared property of a living object holds.
    fn held_in_properties(&self, of: &HashSet<ObjectId>) -> HashSet<ObjectId> {
        fn note(v: &Value, of: &HashSet<ObjectId>, held: &mut HashSet<ObjectId>) {
            match v {
                Value::Object(o) if of.contains(o) => {
                    held.insert(*o);
                }
                Value::List(items) => {
                    for v in items.iter() {
                        note(v, of, held);
                    }
                }
                _ => {}
            }
        }
        let store = self.inner.store.borrow();
        let mut held = HashSet::new();
        for h in store.objects.values() {
            if h.flags.contains(ObjectFlags::PENDING_DELETE) {
                continue;
            }
            for v in h.dynamic.iter() {
                note(v, of, &mut held);
            }
        }
        held.retain(|o| store.objects.contains_key(*o));
        held
    }

    /// Runs the JS collector now and destroys the script-owned objects
    /// no script and no property holds any more. For memory pressure and
    /// for tests; false inside a script or before any script ran.
    pub fn collect_garbage(&self) -> bool {
        if !self.js_started() {
            return false;
        }
        let Ok(js) = self.js() else { return false };
        self.review_held(&self.extension::<ScriptOwned>());
        // What Rust let go of since the last script ran is still held
        // until this.
        js.host.flush_released();
        if !js.host.collect_now() {
            return false;
        }
        self.collected();
        true
    }

    /// A collection ran: it covers every time the heap ran out before it,
    /// for each gate on [`HeapRanOut`]. Every path that collects comes
    /// through [`Engine::collect_garbage`], so it is called there only.
    fn collected(&self) {
        let full = self.extension::<HeapRanOut>();
        full.collected_through.set(full.times.get());
        full.collected_after.set(self.faults_reported());
        full.collections.set(full.collections.get() + 1);
    }

    /// The script heap had no room for what a run allocated: the next
    /// [`Engine::collect_garbage_for_full_heap`] collects.
    pub(crate) fn note_heap_ran_out(&self) {
        let full = self.extension::<HeapRanOut>();
        full.times.set(full.times.get() + 1);
    }

    /// [`Engine::collect_garbage`] for what ran out of script heap, unless
    /// a collection since the heap last ran out already ran. Items that
    /// run out together, a column of boundaries on one full heap, recover
    /// through one collection rather than a full pause each. True when it
    /// collected.
    pub fn collect_garbage_for_full_heap(&self) -> bool {
        let full = self.extension::<HeapRanOut>();
        full.times.get() > full.collected_through.get() && self.collect_garbage()
    }

    /// [`Engine::collect_garbage_for_full_heap`] for one fault: collects
    /// unless a collection already came after `report`, even if the heap
    /// ran out again since. What ran out since is reported too, and its
    /// own report collects; gating on the count instead would have a
    /// column of items that reload at once, each running out again,
    /// collect once per item. False for a fault that is not a full heap.
    pub fn collect_garbage_for_fault(&self, report: &FaultReport) -> bool {
        let full = self.extension::<HeapRanOut>();
        matches!(report.fault, Fault::JsOutOfMemory)
            && report.seq > full.collected_after.get()
            && self.collect_garbage()
    }

    /// Where collections are now, for what ran out of script heap and
    /// tries again later: [`Engine::collect_garbage_unless_since`] with it
    /// collects unless one ran in between. Taken when the retry is set, so
    /// retries set together share the first one's collection, and a retry
    /// that ran out again after it sets a new mark, which collects again.
    pub(crate) fn full_heap_mark(&self) -> u64 {
        self.extension::<HeapRanOut>().collections.get()
    }

    /// [`Engine::collect_garbage`] unless a collection ran since `mark`,
    /// from [`Engine::full_heap_mark`]. True when it collected.
    pub(crate) fn collect_garbage_unless_since(&self, mark: u64) -> bool {
        self.extension::<HeapRanOut>().collections.get() == mark && self.collect_garbage()
    }
}

/// How many times the script heap ran out, and how many of those the last
/// collection came after; how many faults had been reported by then; and
/// how many collections there have been.
#[derive(Default)]
struct HeapRanOut {
    times: Cell<u64>,
    collected_through: Cell<u64>,
    collected_after: Cell<u64>,
    collections: Cell<u64>,
}

/// What the script engine holds, as [`Engine::script_memory`] tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptMemory {
    /// Bytes allocated for scripts.
    pub bytes: usize,
    /// Script objects alive, functions included.
    pub objects: usize,
    /// Full collections run since the script engine started.
    pub collections: u64,
}

impl Engine {
    /// [`Engine::collect_garbage`] if the scripts allocated enough since
    /// the collector last ran, for a host about to rest. False inside a
    /// script, before any ran, and when the heap did not grow. It covers
    /// what ran out of heap before it, as any collection does.
    pub fn collect_if_idle(&self) -> bool {
        self.wants_idle_collect() && self.collect_garbage()
    }

    /// Whether [`Engine::collect_if_idle`] would collect now; a host that
    /// is about to rest asks this to decide whether to wake for it.
    pub fn wants_idle_collect(&self) -> bool {
        self.js_started() && self.js().is_ok_and(|js| js.host.wants_idle_collect())
    }

    /// What the script engine holds now, for a tool that watches memory;
    /// None before any script ran and while one runs.
    pub fn script_memory(&self) -> Option<ScriptMemory> {
        if !self.js_started() {
            return None;
        }
        let host = &self.js().ok()?.host;
        let stats = host.memory_stats()?;
        Some(ScriptMemory {
            bytes: stats.malloc_size,
            objects: stats.object_count,
            collections: host.collections(),
        })
    }
}
