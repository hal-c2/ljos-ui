//! Extension points for the crates built on the engine.
//!
//! The engine knows nothing about animations, but three things an animation
//! system needs can only be done from inside it:
//!
//! - **Write interceptors.** A `Behavior` sees every value on its way into
//!   a property, from a JS write or from a binding, and decides whether the
//!   property takes it now ([`Engine::set_interceptor`]).
//! - **Value sources.** `NumberAnimation on x { }` makes an object that is
//!   not a value of `x` but something that operates on it. The object
//!   creator hands such an object to whoever registered for them
//!   ([`Engine::on_value_source`]).
//! - **Script properties.** `ScriptAction { script: log += "a" }` is a
//!   statement to run on demand, not a binding to evaluate
//!   ([`Engine::register_script_property`], [`Engine::run_script`]).
//!
//! Besides those, [`Engine::extension`] keeps one value per Rust type for
//! as long as the engine lives, so a crate can keep its per-engine state
//! (an animation driver) without a global.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::engine::{Engine, Tracking};
use crate::ids::{ObjectId, PropIndex};
use crate::meta::MetaObject;
use crate::value::Value;

/// How a value reached an [`Interceptor`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interception {
    /// A write from QML or JS. Nothing is on the stack: the interceptor
    /// may write properties.
    Write,
    /// The property's binding computed a new value. The interceptor runs
    /// in the middle of that computation: it must not write or read the
    /// property (use [`Engine::peek`]), only note what to do later.
    Binding,
}

/// Sees a value on its way into a property. True: the interceptor took it,
/// and the property keeps the value it shows.
pub type Interceptor = Rc<dyn Fn(&Engine, &Value, Interception) -> bool>;

/// Called with (the object declared with `on`, its target, the target's
/// property). True: it is now operating on the property.
type ValueSourceListener = Rc<dyn Fn(&Engine, ObjectId, ObjectId, PropIndex) -> bool>;

/// A script property's statement.
pub(crate) type Script = Rc<dyn Fn(&Engine)>;

#[derive(Default)]
pub(crate) struct Hooks {
    extensions: RefCell<HashMap<TypeId, Rc<dyn Any>>>,
    /// Per object, as an object has at most a few and they go together.
    interceptors: RefCell<HashMap<ObjectId, Vec<(PropIndex, Interceptor)>>>,
    value_sources: RefCell<Vec<ValueSourceListener>>,
    script_props: RefCell<Vec<(&'static MetaObject, PropIndex)>>,
    scripts: RefCell<HashMap<ObjectId, Vec<(PropIndex, Script)>>>,
}

impl Hooks {
    /// Drops what destroyed objects had.
    pub(crate) fn forget(&self, objects: &[ObjectId]) {
        let mut interceptors = self.interceptors.borrow_mut();
        let mut scripts = self.scripts.borrow_mut();
        if interceptors.is_empty() && scripts.is_empty() {
            return;
        }
        for o in objects {
            interceptors.remove(o);
            scripts.remove(o);
        }
    }
}

impl Engine {
    /// The engine's one `T`, made on first use. For per-engine state of a
    /// crate built on the engine.
    pub fn extension<T: Default + 'static>(&self) -> Rc<T> {
        let hooks = &self.inner.hooks;
        let found = hooks.extensions.borrow().get(&TypeId::of::<T>()).cloned();
        let any = match found {
            Some(a) => a,
            None => {
                // Made outside the borrow: `T::default()` may ask for
                // another extension.
                let made: Rc<dyn Any> = Rc::new(T::default());
                hooks
                    .extensions
                    .borrow_mut()
                    .entry(TypeId::of::<T>())
                    .or_insert(made)
                    .clone()
            }
        };
        any.downcast::<T>()
            .unwrap_or_else(|_| unreachable!("extensions are keyed by their type"))
    }

    /// Puts `f` in front of `prop` of `obj`: it sees every value QML or JS
    /// writes and every new value of the property's binding. `None` takes
    /// the interceptor away. Writes from native code
    /// ([`Engine::write_internal`]) are not intercepted, which is how the
    /// interceptor itself moves the property.
    pub fn set_interceptor(&self, obj: ObjectId, prop: PropIndex, f: Option<Interceptor>) {
        let (obj, prop) = self.redirect(obj, prop);
        let mut all = self.inner.hooks.interceptors.borrow_mut();
        let list = all.entry(obj).or_default();
        list.retain(|(p, _)| *p != prop);
        if let Some(f) = f {
            list.push((prop, f));
        }
        if list.is_empty() {
            all.remove(&obj);
        }
    }

    fn interceptor(&self, obj: ObjectId, prop: PropIndex) -> Option<Interceptor> {
        let all = self.inner.hooks.interceptors.borrow();
        if all.is_empty() {
            return None;
        }
        all.get(&obj)?
            .iter()
            .find(|(p, _)| *p == prop)
            .map(|(_, f)| f.clone())
    }

    /// A write from QML or JS to a property with an interceptor. True: the
    /// interceptor took the value.
    pub(crate) fn intercept_write(&self, obj: ObjectId, prop: PropIndex, v: &Value) -> bool {
        let Some(f) = self.interceptor(obj, prop) else {
            return false;
        };
        // A write ends the property's binding whether or not the value
        // gets there at once.
        if self.is_bound(obj, prop) {
            let shown = self.untrack(|| self.read(obj, prop));
            let _ = self.write_internal(obj, prop, shown, ljos_reactive::WriteKind::Imperative);
        }
        f(self, v, Interception::Write)
    }

    /// The binding of a property with an interceptor computed `v`. When
    /// the interceptor takes it, the value the property goes on showing.
    pub(crate) fn intercept_binding(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        v: &Value,
    ) -> Option<Value> {
        let f = self.interceptor(obj, prop)?;
        let taken = {
            let _t = Tracking::pause(&self.inner.tracking);
            self.inner
                .graph
                .untrack(|| f(self, v, Interception::Binding))
        };
        taken.then(|| self.peek(obj, prop))
    }

    /// Does `f` once the binding that is being evaluated has given its
    /// value: at once when none is. An interceptor called for a binding's
    /// value ([`Interception::Binding`]) may not write the property from
    /// there; what it does here is done before the statement that changed
    /// the binding's input returns.
    pub fn after_binding(&self, f: impl FnOnce(&Engine) + 'static) {
        let weak = Rc::downgrade(&self.inner);
        self.inner.graph.after_evaluation(move || {
            if let Some(inner) = weak.upgrade() {
                let e = Engine::from_inner(inner);
                // An evaluation further out may still be running; what `f`
                // reads is not one of its inputs.
                e.untrack(|| f(&e));
            }
        });
    }

    /// The value `prop` last had, without running its binding. Usable
    /// where a read is not: inside the property's own binding.
    pub fn peek(&self, obj: ObjectId, prop: PropIndex) -> Value {
        let (obj, prop) = self.redirect(obj, prop);
        let node = {
            let store = self.inner.store.borrow();
            store.objects.get(obj).and_then(|h| h.node(prop))
        };
        match node {
            Some(n) => self.inner.graph.peek(n).unwrap_or_default(),
            // No node, so no binding: the read is of the field.
            None => self.untrack(|| self.read(obj, prop)),
        }
    }

    /// Registers for objects declared with `on` (`NumberAnimation on x`).
    /// `f(source, target, property)` returns whether `source` is one of
    /// its kind and now operates on the property.
    pub fn on_value_source(
        &self,
        f: impl Fn(&Engine, ObjectId, ObjectId, PropIndex) -> bool + 'static,
    ) {
        self.inner.hooks.value_sources.borrow_mut().push(Rc::new(f));
    }

    /// Offers an object declared with `on` to the registered listeners.
    pub(crate) fn offer_value_source(
        &self,
        source: ObjectId,
        target: ObjectId,
        prop: PropIndex,
    ) -> bool {
        let listeners = self.inner.hooks.value_sources.borrow().clone();
        listeners.iter().any(|f| f(self, source, target, prop))
    }

    /// Makes `prop` of `meta` (and of what derives from it) a script
    /// property: what QML writes for it is kept as a statement and runs
    /// only through [`Engine::run_script`].
    pub fn register_script_property(&self, meta: &'static MetaObject, prop: PropIndex) {
        let mut props = self.inner.hooks.script_props.borrow_mut();
        if !props
            .iter()
            .any(|(m, p)| std::ptr::eq(*m, meta) && *p == prop)
        {
            props.push((meta, prop));
        }
    }

    pub(crate) fn is_script_property(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let props = self.inner.hooks.script_props.borrow();
        if props.is_empty() {
            return false;
        }
        self.meta_of(obj)
            .is_some_and(|meta| props.iter().any(|(m, p)| *p == prop && meta.inherits(m)))
    }

    pub(crate) fn set_script(&self, obj: ObjectId, prop: PropIndex, f: Script) {
        let mut all = self.inner.hooks.scripts.borrow_mut();
        let list = all.entry(obj).or_default();
        list.retain(|(p, _)| *p != prop);
        list.push((prop, f));
    }

    /// Runs the statement written for a script property. False when none
    /// was.
    pub fn run_script(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let f = {
            let all = self.inner.hooks.scripts.borrow();
            all.get(&obj)
                .and_then(|l| l.iter().find(|(p, _)| *p == prop))
                .map(|(_, f)| f.clone())
        };
        match f {
            Some(f) => {
                self.untrack(|| f(self));
                true
            }
            None => false,
        }
    }

    /// Asks the host for a frame, as a started timer does. For work that
    /// begins outside a frame and shows nothing by itself (an animation
    /// that has yet to move anything).
    pub fn request_wake(&self) {
        self.wake_host();
    }
}
