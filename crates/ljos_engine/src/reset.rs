//! Taking a property back with `undefined`.
//!
//! Some properties have a state before anything gave them a value that is
//! more than a value: a width follows the implicit width, a side margin
//! follows the margins. Qt calls going back to it the property's reset, and
//! in QML assigning `undefined` does it, from a script or as what a binding
//! yields. A type says which of its properties have one, and how
//! ([`Reset`]); every other property refuses `undefined` as a value of the
//! wrong type.

use crate::engine::{Engine, read_access, store_in};
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::meta::{PropertyFlags, Reset};
use crate::types::ConvertError;
use crate::value::Value;

impl Engine {
    /// How the property is taken back, if it can be. An alias is taken back
    /// as what it names is.
    pub fn reset_of(&self, obj: ObjectId, prop: PropIndex) -> Option<Reset> {
        let (obj, prop) = self.redirect(obj, prop);
        let store = self.inner.store.borrow();
        store.objects.get(obj)?.meta.property(prop)?.reset
    }

    /// Takes the property back to the state it had before anything gave it
    /// a value: what `obj.prop = undefined` does. A binding on it goes. A
    /// property without a reset refuses, with the error `undefined` gets as
    /// a value of its type.
    pub fn reset(&self, obj: ObjectId, prop: PropIndex) -> Result<(), EngineError> {
        self.reset_impl(obj, prop, false)
    }

    /// [`Engine::reset`]; `internal` is the creator or native code, which
    /// may write a read-only property.
    pub(crate) fn reset_impl(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        internal: bool,
    ) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let held = !internal
            && self
                .node_if_any(obj, prop)
                .is_some_and(|n| self.inner.graph.is_held_against_writes(n));
        let reset = {
            let store = self.inner.store.borrow();
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            let p = h
                .meta
                .property(prop)
                .ok_or_else(|| EngineError::UnknownProperty {
                    ty: h.meta.name.to_string(),
                    name: format!("#{}", prop.0),
                })?;
            let constant = p.flags.contains(PropertyFlags::CONSTANT);
            if !internal && (p.is_readonly() || constant) {
                return Err(EngineError::ReadOnly(p.name.to_string()));
            }
            p.reset
                .ok_or_else(|| ConvertError::new("[undefined]", p.ty.qt_name()))?
        };
        let reset_done = match reset {
            Reset::Default => self.reset_to_default(obj, prop),
            // Nobody wrote the value it follows, so no interceptor
            // animates it either.
            Reset::Follow(value) | Reset::FollowFromCreation(value) => {
                self.bind_computed(obj, prop, move |e| value(e, obj, prop))
            }
            Reset::Custom { apply, .. } | Reset::CustomFromCreation { apply, .. } => {
                apply(self, obj, prop)
            }
            Reset::Absent { value, .. } => {
                let v = self.untrack(|| value(self, obj, prop));
                self.reset_to(obj, prop, v)
            }
        };
        reset_done?;
        if held {
            self.tell_undone(obj, prop)?;
        }
        Ok(())
    }

    /// Gives the [`Reset::Absent`], [`Reset::FollowFromCreation`] and
    /// [`Reset::CustomFromCreation`] properties of an object that was just
    /// created their first values. Nothing reads or watches them yet, so
    /// nobody is told: the object has had them from the start.
    pub(crate) fn give_absent(&self, obj: ObjectId) {
        let meta = {
            let store = self.inner.store.borrow();
            match store.objects.get(obj) {
                Some(h) if !h.meta.absent().is_empty() => h.meta.clone(),
                _ => return,
            }
        };
        for &(prop, value) in meta.absent() {
            let v = self.untrack(|| value(self, obj, prop));
            if let Some(h) = self.inner.store.borrow_mut().objects.get_mut(obj) {
                let _ = store_in(h, prop, &v);
            }
        }
    }

    /// Whether writing `v` takes the property back instead: `undefined`
    /// does for every property with a reset, and so does a value that
    /// stands for none ([`Reset::Absent`]).
    pub(crate) fn takes_back(&self, obj: ObjectId, prop: PropIndex, v: &Value) -> bool {
        // Nearly every write is of a number, a string that is not empty or
        // an object; none of those can stand for nothing.
        let may = match v {
            Value::Undefined | Value::Null => true,
            Value::String(s) | Value::Url(s) => s.is_empty(),
            _ => false,
        };
        may && self.reset_of(obj, prop).is_some_and(|r| r.takes_back(v))
    }

    /// What the property holds while it is reset. Reads are followed when
    /// this runs in a binding.
    pub(crate) fn reset_value(&self, obj: ObjectId, prop: PropIndex, reset: Reset) -> Value {
        match reset {
            Reset::Default => self.default_value(obj, prop),
            Reset::Follow(value)
            | Reset::FollowFromCreation(value)
            | Reset::Custom { value, .. }
            | Reset::CustomFromCreation { value, .. }
            | Reset::Absent { value, .. } => value(self, obj, prop),
        }
    }

    /// Puts the value a new object of the type has back, as a value
    /// nothing gave; a binding goes. What [`Reset::Default`] does, for a
    /// [`Reset::Custom`] that does this and something more.
    pub fn reset_to_default(&self, obj: ObjectId, prop: PropIndex) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let v = self.default_value(obj, prop);
        self.reset_to(obj, prop, v)
    }

    /// The value a new object of the type has in the property.
    pub fn default_value(&self, obj: ObjectId, prop: PropIndex) -> Value {
        let store = self.inner.store.borrow();
        let Some(h) = store.objects.get(obj) else {
            return Value::Undefined;
        };
        let Some(p) = h.meta.property(prop) else {
            return Value::Undefined;
        };
        // The type's constructor knows its defaults; nothing else does. A
        // reset is rare enough that a throwaway object costs nothing.
        match h.meta.create {
            Some(create) => {
                let fresh = create();
                let dynamic = vec![Value::Undefined; h.dynamic.len()];
                match read_access(&*fresh, &dynamic, &p.access) {
                    Value::Undefined => p.ty.default_value(),
                    v => v,
                }
            }
            None => p.ty.default_value(),
        }
    }

    /// Puts `v` in place as a value nothing gave; a binding goes. For a
    /// [`Reset::Custom`].
    pub fn reset_to(&self, obj: ObjectId, prop: PropIndex, v: Value) -> Result<(), EngineError> {
        let node = {
            let mut store = self.inner.store.borrow_mut();
            let h = store
                .objects
                .get_mut(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            match h.node(prop) {
                Some(n) => n,
                None if h.read_without_node(prop) => {
                    drop(store);
                    self.ensure_node(obj, prop)?
                }
                None => {
                    // Nobody observes it: the field is the only copy.
                    store_in(h, prop, &v)?;
                    h.clear_assigned(prop);
                    let change = crate::engine::change_hook(h, prop);
                    drop(store);
                    self.tell_change(obj, prop, change);
                    return Ok(());
                }
            }
        };
        let g = &self.inner.graph;
        // Batched so effects run after the field cache is updated.
        g.batch(|| {
            g.reset(node, v)?;
            if let Some(cur) = g.untrack(|| g.try_get(node)) {
                self.store_value(obj, prop, &cur);
            }
            Ok(())
        })
    }
}
