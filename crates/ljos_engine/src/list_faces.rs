//! A `list<T>` property declared in QML, as a script holds it.
//!
//! What a script reads from such a property is the property's list, not an
//! array of its own: `numbers.push(3)` and `numbers[0] = 1` change the
//! property and tell of the change, and a variable given the list reads
//! what the property holds later. That is the opposite of an array in a
//! `var` (script_arrays.rs), which is the script's own and tells nothing.
//!
//! The engine keeps a list as it keeps any value, so the script is given a
//! face: one JS object per property, made the first time a script reads
//! it, that asks the engine for the list before every read and gives it
//! the whole list back after every change. The face means the property
//! only while a script passes it around: stored in a `var`, sent with a
//! signal or assigned to another list, it is the entries it had.
//!
//! A list of a native type is not given a face: what reads it is given a
//! new array each time, as before. That array is a copy of one the engine
//! keeps for the list while it is unchanged, so that a loop over
//! `children[i]` costs a copy of an array, not a wrapper for every entry.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use ljos_js::rquickjs;
use ljos_js::{HostError, HostFn, JsRef, JsValue, Scope};
use ljos_reactive::WriteKind;

use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::js::{JsState, obj_to_u64, u64_to_obj};
use crate::meta::PropertyAccess;
use crate::types::TypeTag;
use crate::value::Value;

/// The faces given out, by property and by the reference a script holds.
#[derive(Default)]
pub(crate) struct ListFaces {
    by_prop: RefCell<HashMap<(ObjectId, PropIndex), Face>>,
    /// [`JsRef::id`] of a face to its property. The table above holds the
    /// reference, so the id stays that face's.
    by_ref: RefCell<HashMap<u64, (ObjectId, PropIndex)>>,
    /// The array last made of a native list of objects, by property, with
    /// the list it was made of. Held, not only compared, as in [`Face`].
    natives: RefCell<HashMap<(ObjectId, PropIndex), (Value, JsRef)>>,
    /// How many arrays have been made into `natives`.
    native_builds: Cell<u64>,
}

struct Face {
    face: JsRef,
    /// The list the face's array was last filled from. Held, not only
    /// compared, so that another list cannot come to be at its address.
    seen: Option<Value>,
}

impl ListFaces {
    pub(crate) fn forget(&self, objects: &[ObjectId]) {
        if !self.natives.borrow().is_empty() {
            // An array that holds a gone object is made again when read.
            self.natives.borrow_mut().retain(|(o, _), (list, _)| {
                !objects.contains(o)
                    && !matches!(list, Value::List(items)
                        if items.iter().any(|i| i.as_object().is_some_and(|i| objects.contains(&i))))
            });
        }
        if self.by_prop.borrow().is_empty() {
            return;
        }
        self.by_prop
            .borrow_mut()
            .retain(|(o, _), _| !objects.contains(o));
        self.by_ref
            .borrow_mut()
            .retain(|_, (o, _)| !objects.contains(o));
    }

    fn of(&self, r: &JsRef) -> Option<(ObjectId, PropIndex)> {
        self.by_ref.borrow().get(&r.id()).copied()
    }
}

/// What an entry of a list declared in QML is made into.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Entry {
    Int,
    Real,
    String,
    Bool,
    Url,
    /// A type whose values are refused or taken as a property of it does.
    Typed,
    /// `list<var>`, and lists of objects: the entry as it comes.
    Any,
}

fn entry_kind(elem: &Atom) -> (Entry, TypeTag) {
    let ty = TypeTag::parse(elem);
    let kind = match ty {
        TypeTag::Int => Entry::Int,
        TypeTag::Real => Entry::Real,
        TypeTag::String => Entry::String,
        TypeTag::Bool => Entry::Bool,
        TypeTag::Url => Entry::Url,
        TypeTag::Color
        | TypeTag::Point
        | TypeTag::Size
        | TypeTag::Rect
        | TypeTag::Font
        | TypeTag::Vector2d
        | TypeTag::Vector3d
        | TypeTag::Vector4d
        | TypeTag::Quaternion => Entry::Typed,
        _ => Entry::Any,
    };
    (kind, ty)
}

/// Where an entry comes from: Qt makes it of the list's type by one set
/// of rules for an array assigned whole and by another for a value a
/// script puts into the list (`push`, `list[0] = v`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// An entry of an array assigned to the property: JavaScript's own
    /// conversions, so `2.7` is `2` in a list of `int` and `null` is
    /// `"null"` in a list of `string`.
    Array,
    /// A value put into the list, or assigned as the whole of it: `2.7`
    /// is `3`, and what does not convert is the type's zero.
    Put,
}

/// An entry as Qt makes it for a list of the type. Nothing is refused, as
/// a property of the type would refuse it.
fn entry(e: &Engine, kind: Entry, ty: &TypeTag, from: Origin, v: &Value) -> Value {
    match (kind, from) {
        (Entry::Any, _) => v.clone(),
        (Entry::Int, Origin::Array) => Value::Int(match v {
            Value::Int(i) => *i,
            v => crate::number::to_int32(v.to_number()),
        }),
        (Entry::Int, Origin::Put) => Value::Int(match v {
            Value::Int(i) => *i,
            Value::Enum(e) => e.value,
            Value::Bool(b) => i32::from(*b),
            // To the nearest, a half away from zero, then as JavaScript
            // makes any number 32 bits.
            Value::Real(r) if r.is_finite() => crate::number::to_int32(r.round()),
            // A whole number in decimal, or nothing.
            Value::String(s) => s.trim().parse().unwrap_or(0),
            _ => 0,
        }),
        (Entry::Real, Origin::Array) => Value::Real(v.to_number()),
        (Entry::Real, Origin::Put) => Value::Real(match v {
            Value::Int(i) => f64::from(*i),
            Value::Enum(e) => f64::from(e.value),
            Value::Real(r) => *r,
            Value::Bool(b) => f64::from(u8::from(*b)),
            Value::String(s) => s.trim().parse().unwrap_or(f64::NAN),
            // An entry never written, and `null`.
            Value::Undefined | Value::Null => 0.0,
            _ => f64::NAN,
        }),
        (Entry::String, Origin::Array) => match v {
            Value::String(_) => v.clone(),
            v => Value::String(e.to_js_string(v)),
        },
        (Entry::String, Origin::Put) => match v {
            Value::String(_) => v.clone(),
            Value::Url(s) => Value::String(s.clone()),
            Value::Int(_) | Value::Real(_) | Value::Enum(_) | Value::Bool(_) => {
                Value::String(v.to_js_string())
            }
            _ => Value::String(Atom::EMPTY),
        },
        (Entry::Bool, _) => Value::Bool(v.to_boolean()),
        (Entry::Url, _) => match v {
            Value::Url(_) => v.clone(),
            Value::String(s) => Value::Url(s.clone()),
            _ => Value::Url(Atom::EMPTY),
        },
        (Entry::Typed, _) => ty.coerce(v).unwrap_or_else(|_| ty.default_value()),
    }
}

/// The entries one value assigned to the property is: a string is its
/// characters (their codes in a list of `int`), anything else one entry.
/// `None`: what the property's own type makes of it (nothing of
/// `undefined`, a list of an object).
fn entries_of_one(e: &Engine, kind: Entry, ty: &TypeTag, v: &Value) -> Option<Vec<Value>> {
    let var = matches!(ty, TypeTag::Var);
    if kind == Entry::Any && !var {
        return None;
    }
    match v {
        Value::Undefined | Value::List(_) => None,
        Value::String(s) if !matches!(kind, Entry::Url | Entry::Typed) => Some(
            s.encode_utf16()
                .map(|unit| match kind {
                    Entry::Int => Value::Int(i32::from(unit)),
                    Entry::Real => Value::Real(0.0),
                    Entry::Bool => Value::Bool(true),
                    _ => Value::String(Atom::from(String::from_utf16_lossy(&[unit]))),
                })
                .collect(),
        ),
        one => Some(vec![entry(e, kind, ty, Origin::Put, one)]),
    }
}

impl Engine {
    /// The property a `list<T>` declared in QML is, behind any alias of
    /// it; `None` for anything else.
    pub(crate) fn script_list(
        &self,
        obj: ObjectId,
        prop: PropIndex,
    ) -> Option<(ObjectId, PropIndex)> {
        let (obj, prop) = self.redirect(obj, prop);
        self.list_entries(obj, prop).map(|_| (obj, prop))
    }

    /// The entry type of the `list<T>` declared in QML that `prop` (the
    /// property itself) is.
    fn list_entries(&self, obj: ObjectId, prop: PropIndex) -> Option<Atom> {
        let store = self.inner.store.borrow();
        let p = store.objects.get(obj)?.meta.property(prop)?;
        match (&p.ty, &p.access) {
            (TypeTag::List(Some(elem)), PropertyAccess::Dynamic { .. }) => Some(elem.clone()),
            _ => None,
        }
    }

    /// What is on its way into the property `prop` of `obj` (the property
    /// itself), when that is a `list<T>` declared in QML: each entry is
    /// made of the list's type, a single value is a list of it, and a
    /// list of the entries the property holds already is the list it
    /// holds, so that it is no change. A `list<var>` is a new list every
    /// time, as an array in a `var` is.
    pub(crate) fn for_list(&self, obj: ObjectId, prop: PropIndex, v: Value) -> Value {
        let Some(elem) = self.list_entries(obj, prop) else {
            return v;
        };
        let (kind, ty) = entry_kind(&elem);
        // Not a read: this runs inside the property's own binding too.
        let now = &self.peek(obj, prop);
        if let (Value::List(a), Value::List(b)) = (&v, now)
            && Arc::ptr_eq(a, b)
        {
            return v;
        }
        let made: Vec<Value> = match &v {
            Value::List(items) => items
                .iter()
                .map(|i| entry(self, kind, &ty, Origin::Array, i))
                .collect(),
            one => match entries_of_one(self, kind, &ty, one) {
                Some(made) => made,
                None => return v,
            },
        };
        if !matches!(ty, TypeTag::Var)
            && let Value::List(held) = now
            && held.len() == made.len()
            && held.iter().zip(&made).all(|(a, b)| a.change_equals(b))
        {
            return now.clone();
        }
        Value::list(made)
    }

    /// Whether `r` is the face of a list property.
    pub(crate) fn is_face(&self, r: &JsRef) -> bool {
        !r.is_array()
            && self
                .inner
                .js
                .borrow()
                .as_ref()
                .is_some_and(|js| js.lists.of(r).is_some())
    }

    /// The list a face stands for, as it is now; `None` when `r` is not a
    /// face.
    pub(crate) fn face_list(&self, r: &JsRef) -> Option<Value> {
        if r.is_array() {
            return None;
        }
        let js = self.inner.js.borrow().clone()?;
        let (obj, prop) = js.lists.of(r)?;
        Some(match self.untrack(|| self.try_read(obj, prop)) {
            Ok(v @ Value::List(_)) => v,
            _ => Value::List(Arc::default()),
        })
    }

    /// The face of the list property `prop` of `obj` (the property itself,
    /// see [`Engine::script_list`]), made the first time.
    pub(crate) fn list_face(
        &self,
        scope: &Scope<'_>,
        js: &JsState,
        obj: ObjectId,
        prop: PropIndex,
    ) -> rquickjs::Result<JsValue> {
        if let Some(f) = js.lists.by_prop.borrow().get(&(obj, prop)) {
            return Ok(JsValue::Handle(f.face.clone()));
        }
        let make: rquickjs::Function = scope.globals().get("__ljos_list")?;
        let wrapper = scope.wrapper(obj_to_u64(obj))?;
        let v: rquickjs::Value = make.call((wrapper, prop.0))?;
        let face = scope.keep(v);
        js.lists.by_ref.borrow_mut().insert(face.id(), (obj, prop));
        js.lists.by_prop.borrow_mut().insert(
            (obj, prop),
            Face {
                face: face.clone(),
                seen: None,
            },
        );
        Ok(JsValue::Handle(face))
    }

    /// A new array of `list`, which `prop` of `obj` holds, when that is a
    /// native list of objects; `None` for any other value. The array is a
    /// copy of one kept until the property holds another list: an entry of
    /// a list of objects is a wrapper, which costs more to find than to
    /// copy, and a copy is the script's own as a new array is.
    pub(crate) fn native_list(
        &self,
        scope: &Scope<'_>,
        js: &JsState,
        obj: ObjectId,
        prop: PropIndex,
        list: &Value,
    ) -> Option<rquickjs::Result<JsValue>> {
        let Value::List(items) = list else {
            return None;
        };
        // An entry that is not an object may be made a JS object of its
        // own, which a copy would share.
        if items.is_empty()
            || !items
                .iter()
                .all(|i| matches!(i, Value::Object(_) | Value::Null))
        {
            return None;
        }
        let kept = js
            .lists
            .natives
            .borrow()
            .get(&(obj, prop))
            .and_then(|(seen, r)| match seen {
                Value::List(a) if Arc::ptr_eq(a, items) => Some(r.clone()),
                _ => None,
            });
        Some((|| {
            let array = match kept {
                Some(r) => r,
                None => {
                    let made = scope.to_value(&self.to_js(list))?;
                    let r = scope.keep(made);
                    js.lists
                        .natives
                        .borrow_mut()
                        .insert((obj, prop), (list.clone(), r.clone()));
                    js.lists.native_builds.set(js.lists.native_builds.get() + 1);
                    r
                }
            };
            Ok(JsValue::CopyOf(array))
        })())
    }

    /// How many arrays scripts' reads of native lists of objects have
    /// made; a read of a list that has not changed since makes none.
    #[doc(hidden)]
    pub fn native_list_arrays(&self) -> u64 {
        self.inner
            .js
            .borrow()
            .as_ref()
            .map_or(0, |js| js.lists.native_builds.get())
    }

    /// Gives the list back to the property after a script changed it. A
    /// binding on the property stays, as it does on Qt: the list is the
    /// binding's again when what it reads changes.
    fn store_list(&self, obj: ObjectId, prop: PropIndex, v: Value) -> Result<(), EngineError> {
        let bound = {
            let store = self.inner.store.borrow();
            store
                .objects
                .get(obj)
                .and_then(|h| h.node(prop))
                .is_some_and(|n| self.inner.graph.is_bound(n))
        };
        let how = if bound {
            WriteKind::Animation
        } else {
            WriteKind::Imperative
        };
        self.write(obj, prop, v, how)
    }

    pub(crate) fn install_list_globals(&self, js: &JsState) -> Result<(), EngineError> {
        let err = |e: ljos_js::JsFault| EngineError::Js(e.to_string());
        let target = |args: &[JsValue]| match args {
            [JsValue::Object(o), JsValue::Number(p), ..] => {
                Some((u64_to_obj(*o), PropIndex(*p as u32)))
            }
            _ => None,
        };
        let weak = Rc::downgrade(&self.inner);
        // What the property holds, when that is not what the face was
        // last filled from; `undefined` when it is.
        let w = weak.clone();
        let sync: HostFn = Rc::new(move |_scope, args| {
            let (Some(inner), Some((obj, prop))) = (w.upgrade(), target(args)) else {
                return Ok(JsValue::Undefined);
            };
            let engine = Engine::from_inner(inner);
            // Tracked: a binding that reads the list through a face it was
            // handed follows the property all the same.
            let Ok(now @ Value::List(_)) = engine.try_read(obj, prop) else {
                // The object is gone: the face keeps what it had.
                return Ok(JsValue::Undefined);
            };
            let Some(js) = engine.inner.js.borrow().clone() else {
                return Ok(JsValue::Undefined);
            };
            let mut faces = js.lists.by_prop.borrow_mut();
            let Some(face) = faces.get_mut(&(obj, prop)) else {
                return Ok(JsValue::Undefined);
            };
            if let (Some(Value::List(a)), Value::List(b)) = (&face.seen, &now)
                && Arc::ptr_eq(a, b)
            {
                return Ok(JsValue::Undefined);
            }
            face.seen = Some(now.clone());
            drop(faces);
            Ok(engine.to_js(&now))
        });
        // The face's array after a change: the property is given it, and
        // the face what the property made of it.
        let w = weak;
        let store: HostFn = Rc::new(move |scope, args| {
            let (Some(inner), Some((obj, prop))) = (w.upgrade(), target(args)) else {
                return Ok(JsValue::Undefined);
            };
            let engine = Engine::from_inner(inner);
            let Some(elem) = engine.list_entries(obj, prop) else {
                return Ok(JsValue::Undefined);
            };
            let (kind, of) = entry_kind(&elem);
            let ty = TypeTag::List(Some(elem));
            let given = args.get(2).cloned().unwrap_or(JsValue::Array(Vec::new()));
            // What the script put in is made of the list's type here, by
            // the rules for that; what was in the list is of it already.
            let v = match engine.js_to_value(scope, given, &ty) {
                Value::List(items) => Value::list(
                    items
                        .iter()
                        .map(|i| entry(&engine, kind, &of, Origin::Put, i))
                        .collect(),
                ),
                other => other,
            };
            let done = engine.store_list(obj, prop, v);
            let now = engine.untrack(|| engine.try_read(obj, prop)).ok();
            let js = engine.inner.js.borrow().clone();
            if let Some(js) = js
                && let Some(face) = js.lists.by_prop.borrow_mut().get_mut(&(obj, prop))
            {
                face.seen.clone_from(&now);
            }
            done.map_err(|e| HostError::msg(e.to_string()))?;
            Ok(now.map_or(JsValue::Undefined, |v| engine.to_js(&v)))
        });
        js.host
            .set_global_function("__ljos_listSync", sync)
            .map_err(err)?;
        js.host
            .set_global_function("__ljos_listStore", store)
            .map_err(err)?;
        js.host.eval_script("lists", LISTS).map_err(err)?;
        Ok(())
    }
}

/// The face itself. Its array is filled from the property before anything
/// is read from it, and a function of `Array.prototype` runs on that array
/// rather than through the face, so that `push` is one change and `join`
/// asks the engine once.
const LISTS: &str = r##"
(function () {
  "use strict";
  const g = globalThis;
  const sync = g.__ljos_listSync, store = g.__ljos_listStore;
  const proto = Array.prototype;
  const changing = new Set([
    "push", "pop", "shift", "unshift", "splice", "sort", "reverse", "fill", "copyWithin",
  ]);
  const isIndex = (key) => typeof key === "string" && /^(0|[1-9]\d*)$/.test(key);
  g.__ljos_list = function (object, prop) {
    const t = [];
    const fill = (now) => {
      if (now === undefined) return;
      t.length = 0;
      for (let i = 0; i < now.length; i++) t[i] = now[i];
    };
    const read = () => fill(sync(object, prop));
    const write = () => fill(store(object, prop, t));
    const methods = new Map();
    const method = (name, f) => {
      let m = methods.get(name);
      if (m === undefined) {
        m = changing.has(name)
          ? function (...args) {
              read();
              let out;
              try { out = f.apply(t, args); } finally { write(); }
              return out;
            }
          : function (...args) { read(); return f.apply(t, args); };
        methods.set(name, m);
      }
      return m;
    };
    return new Proxy(t, {
      get(_, key) {
        read();
        const v = t[key];
        return typeof v === "function" && typeof key === "string" && key !== "constructor"
          && proto[key] === v ? method(key, v) : v;
      },
      set(_, key, value) {
        read();
        t[key] = value;
        if (key === "length" || isIndex(key)) write();
        return true;
      },
      has(_, key) { read(); return key in t; },
      // An entry taken away is the type's zero: a list has no holes.
      deleteProperty(_, key) {
        read();
        if (isIndex(key) && key in t) { t[key] = undefined; write(); }
        return true;
      },
      ownKeys() { read(); return Reflect.ownKeys(t); },
      getOwnPropertyDescriptor(_, key) { read(); return Reflect.getOwnPropertyDescriptor(t, key); },
      defineProperty() { return false; },
    });
  };
})();
"##;
