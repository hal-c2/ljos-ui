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

use ljos_reactive::{NodeId, NodeKind, NodeState};

use crate::engine::{Engine, store_in};
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::meta::PropertyAccess;
use crate::types::TypeTag;
use crate::value::Value;

/// See [`Engine::make_on_read`], [`Engine::make_on_read_by`] and
/// [`Engine::bind_on_read`].
pub(crate) enum Maker {
    Value(Box<dyn FnOnce(&Engine) -> Value>),
    By(MakeOnRead),
    Bind(BindOnRead),
    Kept(KeepOnRead),
}

/// What the keeper of a property is told; see [`Engine::keep_on_read`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
    /// The property is about to be read: its value is to be brought up to
    /// date now ([`Engine::give_unobserved`]).
    Read,
    /// Something has come to observe the property: a change handler, a
    /// watch, or a binding that read it.
    Observed,
    /// A write or a binding gives the property its value: it is no longer
    /// kept.
    Given,
}

/// Keeps `prop` of `obj`; see [`Engine::keep_on_read`].
pub type KeepOnRead = fn(&Engine, ObjectId, PropIndex, Kept);

/// Makes the value of `prop` of `obj`; see [`Engine::make_on_read_by`].
pub type MakeOnRead = fn(&Engine, ObjectId, PropIndex) -> Value;

/// Binds `prop` of `obj`; see [`Engine::bind_on_read`].
pub type BindOnRead = fn(&Engine, ObjectId, PropIndex) -> Result<(), EngineError>;

/// The makers of an object's properties. Every read of a property asks
/// whether it has one, and nearly always it has not: a bit per property
/// (modulo 64) says no without looking through them.
#[derive(Default)]
pub(crate) struct Makers {
    mask: u64,
    list: Vec<(PropIndex, Maker)>,
}

fn bit(prop: PropIndex) -> u64 {
    1 << (prop.0 % 64)
}

impl Makers {
    /// Whether `prop` has a maker.
    pub(crate) fn has(&self, prop: PropIndex) -> bool {
        self.mask & bit(prop) != 0 && self.list.iter().any(|(p, _)| *p == prop)
    }

    /// Whether `prop` is still to be bound ([`Engine::bind_on_read`]).
    pub(crate) fn binds(&self, prop: PropIndex) -> bool {
        self.mask & bit(prop) != 0
            && self
                .list
                .iter()
                .any(|(p, m)| *p == prop && matches!(m, Maker::Bind(_)))
    }

    /// Whether `prop` is kept by someone else ([`Engine::keep_on_read`]).
    pub(crate) fn keeps(&self, prop: PropIndex) -> Option<KeepOnRead> {
        if self.mask & bit(prop) == 0 {
            return None;
        }
        self.list.iter().find_map(|(p, m)| match m {
            Maker::Kept(keep) if *p == prop => Some(*keep),
            _ => None,
        })
    }

    /// What the list holds, for [`Engine::object_memory`].
    pub(crate) fn bytes(&self) -> usize {
        self.list.capacity() * std::mem::size_of::<(PropIndex, Maker)>()
    }

    /// Gives `prop` the maker `m`, in place of the one it had.
    fn set(&mut self, prop: PropIndex, m: Maker) {
        drop(self.take(prop));
        // A type that binds one default on a read binds several (a Text
        // binds six), so they go in one allocation.
        if self.list.capacity() == 0 {
            self.list.reserve_exact(8);
        }
        self.list.push((prop, m));
        self.mask |= bit(prop);
    }

    /// Takes the maker of `prop` away, if it has one.
    fn take(&mut self, prop: PropIndex) -> Option<Maker> {
        if self.mask & bit(prop) == 0 {
            return None;
        }
        let at = self.list.iter().position(|(p, _)| *p == prop)?;
        let (_, m) = self.list.swap_remove(at);
        self.mask = self.list.iter().fold(0, |mask, (p, _)| mask | bit(*p));
        Some(m)
    }
}

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
        h.ext_mut()
            .made_on_read
            .set(prop, Maker::Value(Box::new(make)));
        Ok(())
    }

    /// [`Engine::make_on_read`] by a function, which costs no allocation:
    /// an item that makes several values on a read from one piece of work
    /// (a text's sizes from its layout) keeps that work itself and hands
    /// each property its part. A `make` that gives the property itself
    /// again makes its value on every read, until a write or a binding: a
    /// clock nobody observes says where it is when it is asked.
    pub fn make_on_read_by(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        make: MakeOnRead,
    ) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let mut store = self.inner.store.borrow_mut();
        let h = store
            .objects
            .get_mut(obj)
            .ok_or_else(|| EngineError::dead(obj))?;
        h.ext_mut().made_on_read.set(prop, Maker::By(make));
        Ok(())
    }

    /// Binds `prop` by `bind` when it is first read or observed, instead
    /// of now: a default that follows other properties costs nothing on
    /// the many objects where nothing asks for it. Until then the property
    /// holds the value it has, and readers of what `bind` would read are
    /// not told about it; whoever paints from it reads the same way
    /// ([`Engine::read_unless_bound_on_read`]). A write or a binding to the
    /// property before that drops `bind`. Something that already observes
    /// the property gets the binding at once.
    pub fn bind_on_read(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        bind: BindOnRead,
    ) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let observed = {
            let mut store = self.inner.store.borrow_mut();
            let h = store
                .objects
                .get_mut(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            if h.node(prop).is_some() || h.read_without_node(prop) {
                true
            } else {
                h.ext_mut().made_on_read.set(prop, Maker::Bind(bind));
                false
            }
        };
        if observed {
            bind(self, obj, prop)?;
        }
        Ok(())
    }

    /// Says that someone else keeps `prop` and brings its value up to date
    /// only when it is asked, not each time it changes: an animation of a
    /// property nothing observes does no work between reads. Before every
    /// read `keep` is told [`Kept::Read`] and gives the value
    /// ([`Engine::give_unobserved`]); it stays the keeper. A change handler
    /// or a watch that comes to observe the property, or a binding that
    /// reads it, tells it [`Kept::Observed`]: from then on
    /// the keeper writes the property as it changes, and takes itself away
    /// ([`Engine::stop_keeping`]). A write or a binding to the property
    /// takes it away and tells it [`Kept::Given`].
    pub fn keep_on_read(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        keep: KeepOnRead,
    ) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let mut store = self.inner.store.borrow_mut();
        let h = store
            .objects
            .get_mut(obj)
            .ok_or_else(|| EngineError::dead(obj))?;
        h.ext_mut().made_on_read.set(prop, Maker::Kept(keep));
        drop(store);
        // Another keeper out for a read is replaced: it is not put back,
        // and it keeps the property no more.
        for (o, p, k, gone) in self.inner.kept_in_hand.borrow_mut().iter_mut() {
            if (*o, *p) == (obj, prop) && !std::ptr::fn_addr_eq(*k, keep) {
                *gone = true;
            }
        }
        Ok(())
    }

    /// Takes the keeper of `prop` away ([`Engine::keep_on_read`]) without
    /// telling it: what holds the property is what was last given.
    pub fn stop_keeping(&self, obj: ObjectId, prop: PropIndex) {
        let (obj, prop) = self.redirect(obj, prop);
        let kept = self.keeper(obj, prop).is_some();
        if kept {
            drop(self.take_maker(obj, prop));
            return;
        }
        // Out for a read: it is not to be put back.
        for (o, p, _, gone) in self.inner.kept_in_hand.borrow_mut().iter_mut() {
            if (*o, *p) == (obj, prop) {
                *gone = true;
            }
        }
    }

    /// Whether `keep` is the keeper of `prop` ([`Engine::keep_on_read`]),
    /// out for a read or not. A keeper that was told [`Kept::Given`] is not,
    /// and another may keep the property since: an animation started on it.
    /// Nor is one another took the place of while it was out for a read.
    pub fn is_kept_by(&self, obj: ObjectId, prop: PropIndex, keep: KeepOnRead) -> bool {
        let (obj, prop) = self.redirect(obj, prop);
        if let Some(k) = self.keeper(obj, prop) {
            return std::ptr::fn_addr_eq(k, keep);
        }
        self.inner
            .kept_in_hand
            .borrow()
            .iter()
            .any(|&(o, p, k, gone)| (o, p) == (obj, prop) && !gone && std::ptr::fn_addr_eq(k, keep))
    }

    /// [`Engine::stop_keeping`] if `keep` is the keeper of `prop`, and
    /// nothing if another keeps it ([`Engine::is_kept_by`]).
    pub fn stop_keeping_by(&self, obj: ObjectId, prop: PropIndex, keep: KeepOnRead) {
        if self.is_kept_by(obj, prop, keep) {
            self.stop_keeping(obj, prop);
        }
    }

    /// The keeper of `prop`, if someone else keeps it.
    fn keeper(&self, obj: ObjectId, prop: PropIndex) -> Option<KeepOnRead> {
        let store = self.inner.store.borrow();
        store
            .objects
            .get(obj)?
            .ext
            .as_ref()?
            .made_on_read
            .keeps(prop)
    }

    /// A change handler or a watch is about to observe `prop`: its keeper,
    /// if it has one, is told.
    pub(crate) fn tell_watched(&self, obj: ObjectId, prop: PropIndex) {
        let (obj, prop) = self.redirect(obj, prop);
        if let Some(keep) = self.keeper(obj, prop) {
            self.untrack(|| keep(self, obj, prop, Kept::Observed));
        }
    }

    /// Gives `prop` the value `v` of an animation's step, without a
    /// change notification and without running what is queued: what
    /// [`Kept::Read`] asks of a keeper, in the middle of whoever reads.
    /// False, and nothing given, while anything observes or holds the
    /// property, a journal is kept, or `v` is no number for a number
    /// property: it is then written the usual way.
    pub fn give_unobserved(&self, obj: ObjectId, prop: PropIndex, v: Value) -> bool {
        if self.is_journaled() {
            return false;
        }
        let (obj, prop) = self.redirect(obj, prop);
        let node = {
            let store = self.inner.store.borrow();
            let Some(h) = store.objects.get(obj) else {
                return false;
            };
            let Some(p) = h.meta.property(prop) else {
                return false;
            };
            let plain = matches!(
                (&p.ty, &v),
                (TypeTag::Real, Value::Real(_)) | (TypeTag::Int, Value::Int(_))
            ) && matches!(p.access, PropertyAccess::Dynamic { .. })
                && p.given.is_none()
                && p.change.is_none();
            if !plain || h.read_without_node(prop) {
                return false;
            }
            h.node(prop)
        };
        // A node, as an animation's write would make: what reads the
        // property later finds it animated, not assigned.
        let Ok(n) = node.map_or_else(|| self.ensure_node(obj, prop), Ok) else {
            return false;
        };
        if !matches!(
            self.inner.graph.give_animated_quietly(n, v.clone()),
            Ok(true)
        ) {
            return false;
        }
        let mut store = self.inner.store.borrow_mut();
        store
            .objects
            .get_mut(obj)
            .is_some_and(|h| store_in(h, prop, &v).is_ok())
    }

    /// Reads `prop` (tracked), or gives `None` while it is still to be
    /// bound by [`Engine::bind_on_read`]: the reader is then told when
    /// something is written to it, and works out the value itself.
    pub fn read_unless_bound_on_read(&self, obj: ObjectId, prop: PropIndex) -> Option<Value> {
        let (obj, prop) = self.redirect(obj, prop);
        let due = {
            let store = self.inner.store.borrow();
            store
                .objects
                .get(obj)
                .and_then(|h| h.ext.as_ref())
                .is_some_and(|x| x.made_on_read.binds(prop))
        };
        if due {
            self.keep_field_read(obj, prop);
            None
        } else {
            Some(self.read(obj, prop))
        }
    }

    /// Whether the value of `prop` is still to be made by a read.
    pub fn is_made_on_read(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let (obj, prop) = self.redirect(obj, prop);
        let store = self.inner.store.borrow();
        store
            .objects
            .get(obj)
            .and_then(|h| h.ext.as_ref())
            .is_some_and(|x| x.made_on_read.has(prop))
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

    /// Whether anything observes `prop`: [`Engine::observers_of`] is not
    /// empty, without gathering them. Asked for each colour a palette
    /// hands down.
    pub fn is_observed(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let (obj, prop) = self.redirect(obj, prop);
        let store = self.inner.store.borrow();
        let Some(h) = store.objects.get(obj) else {
            return false;
        };
        let g = &self.inner.graph;
        h.node(prop).is_some_and(|n| g.has_observers(n))
            || h.field_reads
                .iter()
                .flat_map(|r| r.readers(prop))
                .any(|r| g.is_current(r))
    }

    /// The bindings that follow `prop` when only bindings read it, each of
    /// them read by nothing but bindings of the same: what would hold its
    /// value, could be worked out again on a read instead
    /// ([`Engine::work_out_again`]), and so need not hear of each change.
    /// Empty when nothing observes `prop`. `None` when anything on the way
    /// shows what it reads: an effect, a handler or a watch, an eager
    /// binding, a property whose change its type is told of, or a reader
    /// that is not a property's binding; also past a long or wide chain.
    pub fn lazy_readers(
        &self,
        obj: ObjectId,
        prop: PropIndex,
    ) -> Option<Vec<(ObjectId, PropIndex)>> {
        // What a hidden thing's bindings come to is a few; more is not
        // worth the walk on every frame it is asked.
        const MOST: usize = 64;
        let g = &self.inner.graph;
        let mut chain: Vec<(ObjectId, PropIndex)> = Vec::new();
        let mut next = vec![self.redirect(obj, prop)];
        while let Some((o, p)) = next.pop() {
            for r in self.observers_of(o, p) {
                if g.kind(r) != Some(NodeKind::Property) || g.is_eager(r) || g.is_noted(r) {
                    return None;
                }
                let key = self.inner.store.borrow().node_keys.get(r).copied()?;
                let at = (key.object, key.prop);
                if chain.contains(&at) {
                    continue;
                }
                if chain.len() == MOST {
                    return None;
                }
                chain.push(at);
                next.push(at);
            }
        }
        Some(chain)
    }

    /// Has the binding of `prop` run again when it is next read, though
    /// nothing it read was written: for what it reads that is made on read
    /// ([`Engine::make_on_read_by`]) and has gone on since. False, and
    /// nothing done, for a property with no binding, one whose binding is
    /// running, or one a run still going has read (it would come back to
    /// that run as a loop).
    pub fn work_out_again(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let (obj, prop) = self.redirect(obj, prop);
        let g = &self.inner.graph;
        let Some(n) = self.node_if_any(obj, prop) else {
            return false;
        };
        if !g.is_bound(n) || g.state(n) == Some(NodeState::Computing) || g.is_read_by_a_run(n) {
            return false;
        }
        g.refresh(n).is_ok()
    }

    /// Takes the maker of `prop` away, if it has one.
    pub(crate) fn take_maker(&self, obj: ObjectId, prop: PropIndex) -> Option<Maker> {
        let mut store = self.inner.store.borrow_mut();
        let x = store.objects.get_mut(obj)?.ext.as_mut()?;
        x.made_on_read.take(prop)
    }

    /// Gives `prop` the maker `m` again, unless it was given another.
    pub(crate) fn give_maker_back(&self, obj: ObjectId, prop: PropIndex, m: Maker) {
        let unused = {
            let mut store = self.inner.store.borrow_mut();
            match store.objects.get_mut(obj) {
                Some(h) if !h.ext.as_ref().is_some_and(|x| x.made_on_read.has(prop)) => {
                    h.ext_mut().made_on_read.set(prop, m);
                    None
                }
                _ => Some(m),
            }
        };
        // Dropped outside the borrow: dropping a closure may run anything.
        drop(unused);
    }

    /// A write or binding is about to give `prop` its value: what was to be
    /// made is not wanted.
    pub(crate) fn drop_maker(&self, obj: ObjectId, prop: PropIndex) {
        // Dropped outside the borrow: dropping a closure may run anything.
        let maker = self.take_maker(obj, prop);
        let unbound = matches!(maker, Some(Maker::Bind(_)));
        if let Some(Maker::Kept(keep)) = maker {
            self.untrack(|| keep(self, obj, prop, Kept::Given));
            return;
        }
        drop(maker);
        // A reader that worked out what the binding would give
        // ([`Engine::read_unless_bound_on_read`]) is told it no longer
        // stands, also when the value written is the one the field held.
        let read = unbound
            && self
                .inner
                .store
                .borrow()
                .objects
                .get(obj)
                .is_some_and(|h| h.read_without_node(prop));
        if read {
            let _ = self.notify(obj, prop);
        }
    }

    /// A read of `prop` is about to happen: its value is made first if it
    /// is still to be.
    pub(crate) fn make_if_due(&self, obj: ObjectId, prop: PropIndex) {
        let v = match self.take_maker(obj, prop) {
            None => return,
            // Untracked: what the maker reads is not what the reader reads.
            Some(Maker::Value(make)) => self.untrack(|| make(self)),
            Some(Maker::By(make)) => self.untrack(|| make(self, obj, prop)),
            Some(Maker::Kept(keep)) => {
                // A tracked read is a binding's (or a painter's): it
                // observes the property from now on.
                let follows = self.inner.tracking.get() > 0;
                // Untracked, as a maker is. It stays the keeper unless it
                // let go meanwhile (a write to another property it keeps
                // wakes it), back before it is told, so that it may then
                // let go.
                self.inner
                    .kept_in_hand
                    .borrow_mut()
                    .push((obj, prop, keep, false));
                self.untrack(|| keep(self, obj, prop, Kept::Read));
                let gone = self
                    .inner
                    .kept_in_hand
                    .borrow_mut()
                    .pop()
                    .is_some_and(|(_, _, _, gone)| gone);
                if gone {
                    // Handed over as it was read: the one that keeps it now
                    // is the one to tell.
                    if follows && let Some(now) = self.keeper(obj, prop) {
                        self.untrack(|| now(self, obj, prop, Kept::Observed));
                    }
                    return;
                }
                self.give_maker_back(obj, prop, Maker::Kept(keep));
                if follows {
                    self.untrack(|| keep(self, obj, prop, Kept::Observed));
                }
                return;
            }
            Some(Maker::Bind(bind)) => {
                // Untracked: the binding's reads are not the reader's.
                if let Err(err) = self.untrack(|| bind(self, obj, prop)) {
                    self.log(crate::LogLevel::Warning, "qml", err.to_string());
                }
                return;
            }
        };
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
