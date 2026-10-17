//! A property whose value is made when something reads it, not when it
//! changes.
//!
//! Some values cost as much as what they are made from: the HTML of a
//! rich text is written out from the whole document. Writing it on every
//! key would make typing O(n) in the length of the text even when nothing
//! reads it. The owner says instead how the value is made
//! ([`Engine::make_on_read`]); the first read makes it. Readers that were
//! there when the value went stale are the owner's to tell: it makes the
//! value at once when anything else observes the property
//! ([`Engine::observers_of`]).

use ljos_reactive::NodeId;

use crate::engine::{Engine, store_in};
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::value::Value;

/// See [`Engine::make_on_read`].
pub(crate) type Maker = Box<dyn FnOnce(&Engine) -> Value>;

impl Engine {
    /// Makes the value of `prop` when it is next read, by `make`, instead
    /// of now. The value it makes is given without a change notification:
    /// whoever observes the property must have been told already (the
    /// caller makes the value at once when [`Engine::observers_of`] names
    /// anyone it does not know). A write or a binding to the property
    /// before that read drops `make`, and so does a later `make_on_read`.
    pub fn make_on_read(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        make: impl FnOnce(&Engine) -> Value + 'static,
    ) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let mut store = self.inner.store.borrow_mut();
        let h = store
            .objects
            .get_mut(obj)
            .ok_or_else(|| EngineError::dead(obj))?;
        let makers = &mut h.ext_mut().made_on_read;
        makers.retain(|(p, _)| *p != prop);
        makers.push((prop, Box::new(make)));
        Ok(())
    }

    /// Whether the value of `prop` is still to be made by a read.
    pub fn is_made_on_read(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let (obj, prop) = self.redirect(obj, prop);
        let store = self.inner.store.borrow();
        store
            .objects
            .get(obj)
            .and_then(|h| h.ext.as_ref())
            .is_some_and(|x| x.made_on_read.iter().any(|(p, _)| *p == prop))
    }

    /// What observes `prop` (bindings, handlers of its change signal,
    /// subscribers); empty when nothing ever did.
    pub fn observers_of(&self, obj: ObjectId, prop: PropIndex) -> Vec<NodeId> {
        let (obj, prop) = self.redirect(obj, prop);
        let (node, unnoded) = {
            let store = self.inner.store.borrow();
            let Some(h) = store.objects.get(obj) else {
                return Vec::new();
            };
            // Read without a node: who read it in their last run.
            let g = &self.inner.graph;
            let unnoded: Vec<NodeId> = h
                .field_reads
                .iter()
                .flat_map(|r| r.readers(prop))
                .filter(|r| g.is_current(*r))
                .map(|r| r.node)
                .collect();
            (h.node(prop), unnoded)
        };
        let mut all = node
            .map(|n| self.inner.graph.observers(n))
            .unwrap_or_default();
        for n in unnoded {
            if !all.contains(&n) {
                all.push(n);
            }
        }
        all
    }

    /// Takes the maker of `prop` away, if it has one.
    fn take_maker(&self, obj: ObjectId, prop: PropIndex) -> Option<Maker> {
        let mut store = self.inner.store.borrow_mut();
        let x = store.objects.get_mut(obj)?.ext.as_mut()?;
        if x.made_on_read.is_empty() {
            return None;
        }
        let at = x.made_on_read.iter().position(|(p, _)| *p == prop)?;
        Some(x.made_on_read.swap_remove(at).1)
    }

    /// A write or binding is about to give `prop` its value: what was to be
    /// made is not wanted.
    pub(crate) fn drop_maker(&self, obj: ObjectId, prop: PropIndex) {
        // Dropped outside the borrow: dropping a closure may run anything.
        let maker = self.take_maker(obj, prop);
        drop(maker);
    }

    /// A read of `prop` is about to happen: its value is made first if it
    /// is still to be.
    pub(crate) fn make_if_due(&self, obj: ObjectId, prop: PropIndex) {
        let Some(make) = self.take_maker(obj, prop) else {
            return;
        };
        // Untracked: what the maker reads is not what the reader reads.
        let v = self.untrack(|| make(self));
        let v = self.for_slot(obj, prop, v);
        let node = {
            let mut store = self.inner.store.borrow_mut();
            let Some(h) = store.objects.get_mut(obj) else {
                return;
            };
            let _ = store_in(h, prop, &v);
            h.node(prop)
        };
        if let Some(n) = node {
            let quiet = self.inner.graph.replace_quietly(n, v.clone());
            if !matches!(quiet, Ok(true)) {
                // Something is over the plain value (an animation, an
                // override): written the usual way.
                let _ = self.write_derived(obj, prop, v);
            }
        }
    }
}
