//! Arrays as a script holds them.
//!
//! A `var` property declared in QML holds the script's own array: reading
//! it twice gives one array, `push` changes it in place, and nothing is
//! told of such a change because the property still holds the same array.
//! So an array in such a property is a [`Value::Js`] whose reference says
//! it is an array ([`ljos_js::JsRef::is_array`]), never a [`Value::List`].
//!
//! Everything native keeps lists: a property of a Rust type, an argument
//! of a native method or signal, a typed property. [`Engine::for_slot`] is
//! the one place that decides, on the way into a property, and
//! [`Engine::plain`] is what Rust code calls on a value it read from a
//! `var` property or got back from a QML function.
//!
//! An array made on the Rust side (a literal in tier 0, a list assigned
//! from native code) is not built in the JS heap until a script asks for
//! it, so a UI that never runs JS still never starts the JS host.

use std::collections::HashSet;
use std::sync::Arc;

use ljos_js::{JsRef, JsValue};

use crate::engine::Engine;
use crate::ids::{ObjectId, PropIndex};
use crate::js::u64_to_obj;
use crate::meta::PropertyAccess;
use crate::types::TypeTag;
use crate::value::{JsHandle, Value};

/// What a property is to an array on its way in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    /// A `var` declared in QML: it holds the script's array.
    Var,
    /// Anything else: it holds a list of the entries.
    Other,
}

impl Engine {
    /// `obj` and `prop` are the property itself, not an alias of it.
    pub(crate) fn slot_of(&self, obj: ObjectId, prop: PropIndex) -> Slot {
        let store = self.inner.store.borrow();
        let var = store
            .objects
            .get(obj)
            .and_then(|h| h.meta.property(prop))
            .is_some_and(|p| {
                matches!(p.ty, TypeTag::Var) && matches!(p.access, PropertyAccess::Dynamic { .. })
            });
        if var { Slot::Var } else { Slot::Other }
    }

    /// Whether the property (itself, not an alias of it) was declared in
    /// QML: `property var xs`, `property list<int> ints`.
    pub(crate) fn is_declared_in_qml(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let (obj, prop) = self.redirect(obj, prop);
        let store = self.inner.store.borrow();
        store
            .objects
            .get(obj)
            .and_then(|h| h.meta.property(prop))
            .is_some_and(|p| matches!(p.access, PropertyAccess::Dynamic { .. }))
    }

    /// The value as native code reads it: an array a script holds is a
    /// list of what it holds now, arrays inside it too. Anything else is
    /// the value itself. Use it on what a `var` property declared in QML
    /// holds and on what a function declared in QML returned.
    pub fn plain(&self, v: &Value) -> Value {
        let Value::Js(h) = v else {
            return v.clone();
        };
        if !h.0.is_array() {
            // The face of a list property is that list (list_faces.rs).
            return self.face_list(&h.0).unwrap_or_else(|| v.clone());
        }
        if let Some(items) = h.0.pending_items() {
            return Value::list(items.into_iter().map(|i| self.unscripted(i)).collect());
        }
        let r = h.0.clone();
        self.run_js("array", |s| {
            let items = s.array_items(&r)?;
            Ok(self.js_to_value(s, JsValue::Array(items), &TypeTag::Var))
        })
        .unwrap_or_else(|| Value::list(Vec::new()))
    }

    /// How long the array a script holds is now. `None`: it could not be
    /// asked.
    pub(crate) fn array_len(&self, r: &JsRef) -> Option<usize> {
        if let Some(len) = r.with_pending(<[JsValue]>::len) {
            return Some(len);
        }
        self.run_js("array", |s| s.array_len(r))
    }

    /// The entry of an array a script holds, `undefined` past its end.
    /// `None` for an array inside it, which a caller that reads by value
    /// would hold a copy of where the script holds the array itself.
    pub(crate) fn array_entry(&self, r: &JsRef, index: usize) -> Option<Value> {
        let pending = r.with_pending(|items| items.get(index).cloned());
        if let Some(entry) = pending {
            return match entry {
                Some(JsValue::Array(_)) => None,
                Some(entry) => Some(self.unscripted(entry)),
                None => Some(Value::Undefined),
            };
        }
        self.run_js("array", |s| {
            Ok(match s.array_entry(r, index)? {
                JsValue::Array(_) => None,
                entry => Some(self.js_to_value(s, entry, &TypeTag::Var)),
            })
        })
        .flatten()
    }

    /// An entry of an array no script has seen, back as an engine value.
    /// Such an array was made by [`Engine::to_js`], so nothing in it needs
    /// the JS host to be read.
    fn unscripted(&self, v: JsValue) -> Value {
        match v {
            JsValue::Undefined => Value::Undefined,
            JsValue::Null => Value::Null,
            JsValue::Bool(b) => Value::Bool(b),
            JsValue::Number(n) => Value::Real(n),
            JsValue::String(s) => Value::String(s.into()),
            JsValue::Array(items) => {
                Value::list(items.into_iter().map(|i| self.unscripted(i)).collect())
            }
            JsValue::Object(id) => Value::Object(u64_to_obj(id)),
            v @ (JsValue::Color(_) | JsValue::InvalidColor(_)) => {
                Value::Color(crate::js::color_from_js(&v))
            }
            JsValue::Typed(kind, fields) => self.typed_from_js(kind, fields),
            JsValue::Handle(r) => self.plain(&Value::Js(JsHandle(r))),
            JsValue::LiveList { obj, prop } => self
                .untrack(|| self.live_list(obj, prop))
                .map_or_else(|| Value::list(Vec::new()), Value::List),
            // `to_js` makes none.
            JsValue::Map(_) => Value::Undefined,
        }
    }

    /// A list as an array a script can hold, with an identity of its own.
    pub(crate) fn script_array(&self, items: &[Value]) -> Value {
        Value::Js(JsHandle(JsRef::array(
            items.iter().map(|i| self.to_js(i)).collect(),
        )))
    }

    /// `v` with copies of the script's arrays and plain objects in it, as
    /// a sequence keeps what it is given ([`ljos_js::Scope::copied`]).
    fn copied(&self, v: Value) -> Value {
        if !crate::js::holds_js(&v) {
            return v;
        }
        let js = self.to_js(&v);
        self.run_js("copy", |s| {
            let made = s.copied(&js)?;
            Ok(self.js_to_value(s, made, &TypeTag::Var))
        })
        .unwrap_or(v)
    }

    /// [`Engine::copied`] for the entries of a list going into the
    /// property, except those it holds already: a list a script changed in
    /// place (`push`) comes back with the entries it had, and they stay
    /// the ones a script may be holding.
    fn copied_entries(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        items: &Arc<Vec<Value>>,
    ) -> Arc<Vec<Value>> {
        if !items.iter().any(crate::js::holds_js) {
            return items.clone();
        }
        // Ids, which stay unique while `now` keeps the entries alive.
        let now = self.peek(obj, prop);
        let held: HashSet<u64> = match &now {
            Value::List(now) => now
                .iter()
                .filter_map(|e| match e {
                    Value::Js(h) => Some(h.0.id()),
                    _ => None,
                })
                .collect(),
            _ => HashSet::new(),
        };
        let kept = |e: &Value| matches!(e, Value::Js(h) if held.contains(&h.0.id()));
        let fresh: Vec<Value> = items.iter().filter(|e| !kept(e)).cloned().collect();
        let Value::List(fresh) = self.copied(Value::list(fresh)) else {
            return items.clone();
        };
        let mut fresh = fresh.iter();
        Arc::new(
            items
                .iter()
                .map(|e| {
                    if kept(e) {
                        e.clone()
                    } else {
                        fresh.next().cloned().unwrap_or(Value::Undefined)
                    }
                })
                .collect(),
        )
    }

    /// What `v` is once it is in the property `prop` of `obj` (the
    /// property itself, not an alias of it): a `var` declared in QML holds
    /// an array, everything else a list.
    pub(crate) fn for_slot(&self, obj: ObjectId, prop: PropIndex, v: Value) -> Value {
        let v = match &v {
            Value::List(items) => {
                let items = self.copied_entries(obj, prop, items);
                match self.slot_of(obj, prop) {
                    Slot::Var => self.script_array(&items),
                    Slot::Other => Value::List(items),
                }
            }
            // A var's array is that array to another var; a list is given
            // a copy of it, entries and all.
            Value::Js(h) if h.0.is_array() => match self.slot_of(obj, prop) {
                // A copy of a typed list is copied again, as qml6 does.
                Slot::Var => match (h.0.kind(), self.plain_if_kind(&v)) {
                    (Some(kind), Some(Value::List(items))) => Value::Js(JsHandle(
                        JsRef::array_of_kind(items.iter().map(|i| self.to_js(i)).collect(), kind),
                    )),
                    _ => v,
                },
                Slot::Other => self.copied(self.plain(&v)),
            },
            // A list property a script handed on: a copy of the entries it
            // has now, and no longer that property's list.
            Value::Js(h) => match self.face_list(&h.0) {
                Some(list) => match (self.slot_of(obj, prop), self.copied(list)) {
                    (Slot::Var, Value::List(items)) => self
                        .face_copy(&h.0, &items)
                        .unwrap_or_else(|| self.script_array(&items)),
                    (_, list) => list,
                },
                _ => v,
            },
            Value::Object(o) => {
                // One that belongs to scripts is the property's too now.
                self.held_by_property(*o);
                v
            }
            _ => v,
        };
        // A `list<T>` declared in QML makes each entry a `T`.
        self.for_list(obj, prop, v)
    }

    /// [`Engine::plain`] of a copy of a typed list ([`JsRef::kind`]), and
    /// `None` of anything else, which is not read.
    fn plain_if_kind(&self, v: &Value) -> Option<Value> {
        match v {
            Value::Js(h) if h.0.kind().is_some() => Some(self.plain(v)),
            _ => None,
        }
    }

    /// Arguments for native code: see [`Engine::plain`].
    pub(crate) fn plain_args<'a>(&self, args: &'a [Value]) -> std::borrow::Cow<'a, [Value]> {
        if args
            .iter()
            .any(|a| matches!(a, Value::Js(h) if h.0.is_array() || self.is_face(&h.0)))
        {
            std::borrow::Cow::Owned(args.iter().map(|a| self.plain(a)).collect())
        } else {
            std::borrow::Cow::Borrowed(args)
        }
    }
}
