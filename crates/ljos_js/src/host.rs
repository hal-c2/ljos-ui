use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    ops::Deref,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::NonNull,
    rc::{Rc, Weak},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rquickjs::{
    Array, Class, Context, Ctx, Exception, Function, Module, Object, Persistent, Runtime, Type,
    Value,
    function::{Args, Rest, This},
    object::{Accessor, Property},
};
use slotmap::{SlotMap, new_key_type};

use crate::{
    Bytecode, HostError, HostObjects, HostTypeId, JsArg, JsConfig, JsFault, JsOut, JsRef, JsValue,
    PropDef, SignalDef, Slice, SourceMapper, SourceText, TypeDef, ValueKind,
    objects::HostWrapper,
    value::{Pending, WeakRef},
};

/// A Rust function callable from JS as a global (see
/// [`JsHost::set_global_function`]).
pub type HostFn = Rc<dyn for<'js> Fn(&Scope<'js>, &[JsValue]) -> Result<JsValue, HostError>>;

new_key_type! {
    /// A function exported from a loaded module (or otherwise registered).
    pub struct FnId;
}

struct FnEntry {
    func: Persistent<Function<'static>>,
    /// `module::export`, used as the label in budget logs.
    name: Rc<str>,
}

/// What the interrupt handler reads. Separate from [`HostState`] so the
/// handler stored in the runtime does not keep the whole state alive.
#[derive(Default)]
struct InterruptState {
    deadline: Cell<Option<Instant>>,
    fired: Cell<bool>,
}

pub(crate) struct HostState {
    fns: RefCell<SlotMap<FnId, FnEntry>>,
    /// Prototype per registered type, indexed by [`HostTypeId`]; None
    /// where one was let go of, for the next type to take.
    types: RefCell<Vec<Option<Persistent<Object<'static>>>>>,
    /// Engine-owned wrappers. Kept strongly so identity is stable and so the
    /// Engine decides when a wrapper may die ([`JsHost::drop_wrapper`]).
    wrappers: RefCell<HashMap<u64, Persistent<Object<'static>>>>,
    host: RefCell<Option<Rc<dyn HostObjects>>>,
    released: Rc<RefCell<Vec<u64>>>,
    /// The objects scripts own ([`JsHost::give_to_script`]), each with
    /// how many of the wrappers made for it are still alive. One at most
    /// is reachable; one more may be dead and not yet reported.
    owned: RefCell<HashMap<u64, u32>>,
    /// Made with the first wrapper of a script-owned object.
    owned_face: RefCell<Option<OwnedFace>>,
    mapper: RefCell<Option<Rc<SourceMapper>>>,
    text: RefCell<Option<Rc<SourceText>>>,
    entered: Cell<bool>,
    /// JS values held by Rust through [`JsRef`].
    handles: RefCell<HashMap<u64, Held>>,
    /// The reference given out for each held JS object, by the object's
    /// address, so one object has one [`JsRef`] while any is held and
    /// equal references mean the same object. The address cannot be
    /// another object's while the entry in `handles` keeps this one alive.
    by_object: RefCell<HashMap<usize, (u64, WeakRef)>>,
    /// Whether an array crosses to the embedder by reference
    /// ([`HostObjects::arrays_by_reference`]).
    arrays_by_reference: Cell<bool>,
    next_handle: Cell<u64>,
    /// Keys of dropped [`JsRef`]s. A mutex because a `JsRef` may be dropped
    /// on any thread; only this host ever drains it.
    handle_release: Arc<Mutex<Vec<u64>>>,
    /// Made by the first colour that crosses; most hosts never see one
    /// before their first QML file does.
    color: RefCell<Option<ColorFace>>,
    /// Indexed by [`ValueKind`].
    kinds: RefCell<Vec<KindFace>>,
}

/// One JS value held for Rust.
struct Held {
    value: Persistent<Value<'static>>,
    /// Its key in `by_object`, when it is an object.
    object: Option<usize>,
}

/// The JS side of one [`ValueKind`].
struct KindFace {
    make: Persistent<Function<'static>>,
    /// What tells a value of the kind from any other object.
    proto: Persistent<Object<'static>>,
    fields: Vec<String>,
}

/// The property of a kind's prototype that holds the kind's number. It only
/// finds the candidate; the prototype itself is what is trusted.
const KIND_MARK: &str = "__ljos_kind";

/// Finds the wrapper of a script-owned object while a script still holds
/// it, so the object is one object to the script however often it crosses.
/// Held weakly: the collector is what says nobody holds it any more.
struct OwnedFace {
    get: Persistent<Function<'static>>,
    set: Persistent<Function<'static>>,
}

const OWNED_FACE: &str = r"
(function () {
  const live = new Map();
  let sweepAt = 64;
  return {
    get(key) {
      const ref = live.get(key);
      return ref === undefined ? undefined : ref.deref();
    },
    set(key, made) {
      live.set(key, new WeakRef(made));
      if (live.size >= sweepAt) {
        for (const [k, w] of live) if (w.deref() === undefined) live.delete(k);
        sweepAt = Math.max(64, live.size * 2);
      }
    },
  };
})()
";

/// The JS side of [`JsValue::Color`] (see [`crate::color`]).
struct ColorFace {
    make: Persistent<Function<'static>>,
    /// What tells a colour from any other object coming back from JS.
    proto: Persistent<Object<'static>>,
}

impl HostState {
    /// Frees the JS values of dropped [`JsRef`]s. The values are removed from
    /// the table first and dropped after the borrow ends, because freeing an
    /// object can run finalisers.
    fn drain_handles(&self) {
        let keys = match self.handle_release.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => return,
        };
        if keys.is_empty() {
            return;
        }
        let freed: Vec<_> = {
            let mut handles = self.handles.borrow_mut();
            let mut by_object = self.by_object.borrow_mut();
            keys.iter()
                .filter_map(|k| {
                    let held = handles.remove(k)?;
                    // The object may have been given a new reference since
                    // this one was dropped; that entry stays.
                    if let Some(at) = held.object
                        && by_object.get(&at).is_some_and(|(key, _)| key == k)
                    {
                        by_object.remove(&at);
                    }
                    Some(held)
                })
                .collect()
        };
        drop(freed);
    }

    fn host(&self) -> Result<Rc<dyn HostObjects>, rquickjs::Error> {
        self.host.borrow().clone().ok_or_else(|| {
            rquickjs::Error::new_from_js_message("host", "objects", "no HostObjects installed")
        })
    }
}

/// Access to the entered context, passed to [`JsHost::enter`] closures and to
/// [`HostObjects`] callbacks. It is cheap to clone.
#[derive(Clone)]
pub struct Scope<'js> {
    ctx: Ctx<'js>,
    state: Rc<HostState>,
}

impl std::fmt::Debug for Scope<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scope").finish_non_exhaustive()
    }
}

impl<'js> Deref for Scope<'js> {
    type Target = Ctx<'js>;
    fn deref(&self) -> &Ctx<'js> {
        &self.ctx
    }
}

impl<'js> Scope<'js> {
    pub fn ctx(&self) -> &Ctx<'js> {
        &self.ctx
    }

    /// Calls an exported function. Safe to use from inside a [`HostObjects`]
    /// callback (re-entrancy): it runs on the context that is already entered.
    pub fn call(&self, f: FnId, args: &[JsArg]) -> rquickjs::Result<JsOut> {
        let func = {
            let fns = self.state.fns.borrow();
            fns.get(f).map(|e| e.func.clone())
        };
        let Some(func) = func else {
            return Err(Exception::throw_message(&self.ctx, "stale function handle"));
        };
        let func = func.restore(&self.ctx)?;
        let mut a = Args::new(self.ctx.clone(), args.len());
        for arg in args {
            a.push_arg(self.to_value(arg)?)?;
        }
        let out: Value<'js> = func.call_arg(a)?;
        self.script_value(&out)
    }

    /// Calls an exported function with an explicit `this` (signal handlers
    /// run with `this` bound to their scope object).
    pub fn call_with_this(
        &self,
        f: FnId,
        this: &JsValue,
        args: &[JsArg],
    ) -> rquickjs::Result<JsOut> {
        let func = {
            let fns = self.state.fns.borrow();
            fns.get(f).map(|e| e.func.clone())
        };
        let Some(func) = func else {
            return Err(Exception::throw_message(&self.ctx, "stale function handle"));
        };
        let func = func.restore(&self.ctx)?;
        self.invoke(&func, this, args)
    }

    /// Calls a function held through a [`JsRef`] (a handler given to
    /// `connect`, a `Qt.binding` function, a `callLater` target).
    pub fn call_ref(&self, f: &JsRef, this: &JsValue, args: &[JsArg]) -> rquickjs::Result<JsOut> {
        let value = self.restore(f)?;
        let Some(func) = value.into_function() else {
            return Err(Exception::throw_type(&self.ctx, "not a function"));
        };
        self.invoke(&func, this, args)
    }

    /// After a call of the function `export` of `file`'s module threw:
    /// a `not a function` thrown by that function's own code, at a call
    /// `root.a.name(...)` whose `root` is one of its `params`, is worded
    /// as Qt's engine words it, with what `root.a` is: `Property 'name' of
    /// object QQuickItem(0x…) is not a function`. The function's values
    /// are gone once it has thrown, but its parameters are the caller's
    /// `args`; what is called through a local of its own keeps QuickJS's
    /// words. Only a call that threw pays for this. Gives the exception
    /// back, pending again.
    pub fn retell_failed_call(
        &self,
        file: &str,
        export: &str,
        params: &[&str],
        args: &[JsArg],
    ) -> rquickjs::Error {
        let thrown = self.ctx.catch();
        let _ = self.retell(&thrown, file, export, params, args);
        // What the retelling threw is dropped; the call's error stands.
        let _ = self.ctx.catch();
        self.ctx.throw(thrown)
    }

    fn retell(
        &self,
        thrown: &Value<'js>,
        file: &str,
        export: &str,
        params: &[&str],
        args: &[JsArg],
    ) -> Option<()> {
        let e = Exception::from_object(thrown.as_object()?.clone())?;
        if e.message()? != "not a function" {
            return None;
        }
        let stack = e.stack()?;
        // It threw in the function itself, or in the method it applies; in
        // one nested in it a parameter's name may be a local of its own.
        let mut frames = named_frames(&stack);
        let (name, at) = frames.next()?;
        if name != export && (frames.next()? != ("apply", "native") || frames.next()?.0 != export) {
            return None;
        }
        let mut place = at.rsplitn(3, ':');
        let (col, line, module) = (place.next()?, place.next()?, place.next()?);
        let (line, col) = (line.parse().ok()?, col.parse().ok()?);
        let mapper = self.state.mapper.borrow().clone()?;
        if mapper(module, line)?.0 != file {
            return None;
        }
        let text = self.state.text.borrow().clone()?;
        let (path, prop) = called_member(&text(module, line)?, col)?;
        let at = params.iter().position(|p| *p == path[0])?;
        let mut v = self.to_value(args.get(at)?).ok()?;
        for m in &path[1..] {
            v = v.as_object()?.get(m.as_str()).ok()?;
        }
        let shown = v.get::<rquickjs::convert::Coerced<String>>().ok()?.0;
        e.as_object()
            .set(
                "message",
                format!("Property '{prop}' of object {shown} is not a function"),
            )
            .ok()
    }

    fn invoke(
        &self,
        func: &Function<'js>,
        this: &JsValue,
        args: &[JsArg],
    ) -> rquickjs::Result<JsOut> {
        let mut a = Args::new(self.ctx.clone(), args.len());
        a.this(self.to_value(this)?)?;
        for arg in args {
            a.push_arg(self.to_value(arg)?)?;
        }
        let out: Value<'js> = func.call_arg(a)?;
        self.script_value(&out)
    }

    /// The JS value behind a [`JsRef`]. An array made on the Rust side
    /// ([`JsRef::array`]) becomes a JS array here, the first time.
    pub fn restore(&self, r: &JsRef) -> rquickjs::Result<Value<'js>> {
        let Some(key) = r.key() else {
            return self.materialise(r);
        };
        let p = self
            .state
            .handles
            .borrow()
            .get(&key)
            .map(|h| h.value.clone());
        match p {
            Some(p) => p.restore(&self.ctx),
            None => Err(self.stale()),
        }
    }

    fn stale(&self) -> rquickjs::Error {
        Exception::throw_reference(
            &self.ctx,
            "stale JS reference (from another host, or already released)",
        )
    }

    fn materialise(&self, r: &JsRef) -> rquickjs::Result<Value<'js>> {
        let Some(pending) = r.take_pending() else {
            return Err(self.stale());
        };
        let made = (|| match &pending {
            Pending::Array(items) => {
                let arr = Array::new(self.ctx.clone())?;
                for (i, item) in items.iter().enumerate() {
                    arr.set(i, self.to_value(item)?)?;
                }
                Ok(arr.into_value())
            }
            Pending::Object(fields) => {
                let o = Object::new(self.ctx.clone())?;
                for (k, v) in fields {
                    o.set(k.as_str(), self.to_value(v)?)?;
                }
                Ok(o.into_value())
            }
        })();
        let v = match made {
            Ok(v) => v,
            Err(e) => {
                r.restore_pending(pending);
                return Err(e);
            }
        };
        let key = self.next_key();
        let at = object_address(&v);
        self.state.handles.borrow_mut().insert(
            key,
            Held {
                value: Persistent::save(&self.ctx, v.clone()),
                object: at,
            },
        );
        if let Some(at) = at {
            self.state
                .by_object
                .borrow_mut()
                .insert(at, (key, r.downgrade()));
        }
        r.set_live(key, self.state.handle_release.clone());
        Ok(v)
    }

    fn next_key(&self) -> u64 {
        let key = self.state.next_handle.get();
        self.state.next_handle.set(key + 1);
        key
    }

    /// Keeps a JS value alive for Rust. An object that is held already
    /// gives the reference it has.
    pub fn keep(&self, v: Value<'js>) -> JsRef {
        let at = object_address(&v);
        if let Some(at) = at {
            let known = self
                .state
                .by_object
                .borrow()
                .get(&at)
                .and_then(|(_, weak)| weak.upgrade());
            if let Some(r) = known {
                return r;
            }
        }
        let key = self.next_key();
        // A proxy answers as what it stands for; it is not that array.
        let array = !v.is_proxy() && v.is_array();
        self.state.handles.borrow_mut().insert(
            key,
            Held {
                value: Persistent::save(&self.ctx, v),
                object: at,
            },
        );
        let r = JsRef::new(key, self.state.handle_release.clone(), array);
        if let Some(at) = at {
            self.state
                .by_object
                .borrow_mut()
                .insert(at, (key, r.downgrade()));
        }
        r
    }

    /// What the array behind `r` holds now: its entries as
    /// [`Scope::from_value`] gives them, so an array inside it is a copy.
    pub fn array_items(&self, r: &JsRef) -> rquickjs::Result<Vec<JsValue>> {
        if let Some(items) = r.pending_items() {
            return Ok(items);
        }
        let v = self.restore(r)?;
        let Some(arr) = v.as_array() else {
            return Ok(Vec::new());
        };
        let mut out = Vec::with_capacity(arr.len());
        for item in arr.iter::<Value>() {
            out.push(self.read_value_at(&item?, 1)?);
        }
        Ok(out)
    }

    /// How many entries the array behind `r` has now.
    pub fn array_len(&self, r: &JsRef) -> rquickjs::Result<usize> {
        if let Some(len) = r.with_pending(<[JsValue]>::len) {
            return Ok(len);
        }
        let v = self.restore(r)?;
        Ok(v.as_array().map_or(0, Array::len))
    }

    /// One entry of the array behind `r`, as [`Scope::array_items`] gives
    /// it; `undefined` past the end.
    pub fn array_entry(&self, r: &JsRef, index: usize) -> rquickjs::Result<JsValue> {
        if let Some(entry) = r.with_pending(|items| items.get(index).cloned()) {
            return Ok(entry.unwrap_or(JsValue::Undefined));
        }
        let v = self.restore(r)?;
        let Some(arr) = v.as_array() else {
            return Ok(JsValue::Undefined);
        };
        if index >= arr.len() {
            return Ok(JsValue::Undefined);
        }
        let item: Value = arr.get(index)?;
        self.read_value_at(&item, 1)
    }

    pub fn is_function(&self, r: &JsRef) -> bool {
        self.restore(r).is_ok_and(|v| v.is_function())
    }

    /// JS `===` on two references.
    pub fn same_value(&self, a: &JsRef, b: &JsRef) -> bool {
        if JsRef::ptr_eq(a, b) {
            return true;
        }
        match (self.restore(a), self.restore(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }

    /// One field of the object behind `r` (`undefined` when missing).
    pub fn get_field(&self, r: &JsRef, key: &str) -> rquickjs::Result<JsValue> {
        let v = self.restore(r)?;
        match v.as_object() {
            Some(o) => {
                let field: Value<'js> = o.get(key)?;
                self.from_value(&field)
            }
            None => Ok(JsValue::Undefined),
        }
    }

    /// The own enumerable fields of the object behind `r`, in JS order.
    pub fn entries(&self, r: &JsRef) -> rquickjs::Result<Vec<(String, JsValue)>> {
        let v = self.restore(r)?;
        let Some(o) = v.as_object() else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for kv in o.props::<String, Value>() {
            let (k, v) = kv?;
            out.push((k, self.from_value(&v)?));
        }
        Ok(out)
    }

    /// A copy of `v` that shares no array or plain object with it, as a
    /// sequence copies what it is given: arrays and objects made by a
    /// literal (or `Object.create(null)`) are copied all the way down.
    /// Anything else (a host object, a function, an instance of a class,
    /// a proxy) is the value itself.
    pub fn copied(&self, v: &JsValue) -> rquickjs::Result<JsValue> {
        self.copied_at(v, 0)
    }

    fn copied_at(&self, v: &JsValue, depth: u32) -> rquickjs::Result<JsValue> {
        // Deeper than this is a cycle, which is left shared.
        if depth > 32 {
            return Ok(v.clone());
        }
        let each = |items: &[JsValue]| {
            items
                .iter()
                .map(|i| self.copied_at(i, depth + 1))
                .collect::<rquickjs::Result<Vec<_>>>()
        };
        Ok(match v {
            JsValue::Array(items) => JsValue::Array(each(items)?),
            JsValue::Typed(kind, fields) => JsValue::Typed(*kind, each(fields)?),
            JsValue::Handle(r) => {
                if let Some(items) = r.pending_items() {
                    return Ok(JsValue::Array(each(&items)?));
                }
                let value = self.restore(r)?;
                if value.is_proxy() {
                    return Ok(v.clone());
                }
                if value.is_array() {
                    let items = self.array_items(r)?;
                    return Ok(JsValue::Array(each(&items)?));
                }
                let Some(o) = value
                    .as_object()
                    .filter(|_| value.type_of() == Type::Object)
                else {
                    return Ok(v.clone());
                };
                let object_proto: Object<'js> = self
                    .ctx
                    .globals()
                    .get::<_, Object<'js>>("Object")?
                    .get("prototype")?;
                match o.get_prototype() {
                    Some(p) if p != object_proto => return Ok(v.clone()),
                    _ => {}
                }
                let mut fields = Vec::new();
                for kv in o.props::<String, Value>() {
                    let (k, f) = kv?;
                    let f = self.read_value_at(&f, depth + 1)?;
                    fields.push((k, self.copied_at(&f, depth + 1)?));
                }
                JsValue::Handle(JsRef::object(fields))
            }
            other => other.clone(),
        })
    }

    /// `String(value)` for a value from JS, as `console.log` prints it.
    pub fn display(&self, v: &JsValue) -> String {
        let Ok(value) = self.to_value(v) else {
            return String::new();
        };
        value
            .get::<rquickjs::convert::Coerced<String>>()
            .map(|c| c.0)
            .unwrap_or_default()
    }

    /// The wrapper for `obj`, created on first use: the cached
    /// Engine-owned one, or for an object given to scripts
    /// ([`JsHost::give_to_script`]) the one a script still holds.
    pub fn wrapper(&self, obj: u64) -> rquickjs::Result<Object<'js>> {
        if self.state.owned.borrow().contains_key(&obj) {
            return self.owned_wrapper(obj);
        }
        let cached = self.state.wrappers.borrow().get(&obj).cloned();
        if let Some(p) = cached {
            return p.restore(&self.ctx);
        }
        let w = self.instantiate(obj, None)?;
        let obj_handle = w.into_inner();
        self.state
            .wrappers
            .borrow_mut()
            .insert(obj, Persistent::save(&self.ctx, obj_handle.clone()));
        Ok(obj_handle)
    }

    /// A wrapper owned by JS: it is not cached, and when the collector frees
    /// it [`HostObjects::released`] runs (between runs). For objects created
    /// by script, such as `Component.createObject`.
    pub fn new_owned(&self, obj: u64) -> rquickjs::Result<Object<'js>> {
        let q = self.state.released.clone();
        Ok(self.instantiate(obj, Some(q))?.into_inner())
    }

    fn owned_wrapper(&self, obj: u64) -> rquickjs::Result<Object<'js>> {
        let (get, set) = self.owned_face()?;
        // A string: an id need not fit the integers a number is exact for.
        let key = obj.to_string();
        let held: Value<'js> = get.call((key.as_str(),))?;
        if let Some(o) = held.into_object() {
            return Ok(o);
        }
        let o = self.new_owned(obj)?;
        if let Some(n) = self.state.owned.borrow_mut().get_mut(&obj) {
            *n += 1;
        }
        set.call::<_, ()>((key, o.clone()))?;
        Ok(o)
    }

    fn owned_face(&self) -> rquickjs::Result<(Function<'js>, Function<'js>)> {
        let cached = self
            .state
            .owned_face
            .borrow()
            .as_ref()
            .map(|f| (f.get.clone(), f.set.clone()));
        if let Some((get, set)) = cached {
            return Ok((get.restore(&self.ctx)?, set.restore(&self.ctx)?));
        }
        let face: Object<'js> = self.ctx.eval(OWNED_FACE)?;
        let get: Function<'js> = face.get("get")?;
        let set: Function<'js> = face.get("set")?;
        *self.state.owned_face.borrow_mut() = Some(OwnedFace {
            get: Persistent::save(&self.ctx, get.clone()),
            set: Persistent::save(&self.ctx, set.clone()),
        });
        Ok((get, set))
    }

    fn instantiate(
        &self,
        obj: u64,
        release_queue: Option<Rc<RefCell<Vec<u64>>>>,
    ) -> rquickjs::Result<Class<'js, HostWrapper>> {
        let host = self.state.host()?;
        let Some(ty) = host.type_of(obj) else {
            return Err(Exception::throw_message(
                &self.ctx,
                "object no longer exists",
            ));
        };
        let proto = self
            .state
            .types
            .borrow()
            .get(ty.0 as usize)
            .cloned()
            .flatten();
        let Some(proto) = proto else {
            return Err(Exception::throw_message(
                &self.ctx,
                "unregistered host type",
            ));
        };
        let proto = proto.restore(&self.ctx)?;
        Class::instance_proto(
            HostWrapper {
                id: obj,
                release_queue,
            },
            proto,
        )
    }

    pub fn to_value(&self, v: &JsValue) -> rquickjs::Result<Value<'js>> {
        let ctx = &self.ctx;
        Ok(match v {
            JsValue::Undefined => Value::new_undefined(ctx.clone()),
            JsValue::Null => Value::new_null(ctx.clone()),
            JsValue::Bool(b) => Value::new_bool(ctx.clone(), *b),
            JsValue::Number(n) => Value::new_number(ctx.clone(), *n),
            JsValue::String(s) => rquickjs::String::from_str(ctx.clone(), s)?.into_value(),
            JsValue::Array(items) => {
                let arr = Array::new(ctx.clone())?;
                for (i, item) in items.iter().enumerate() {
                    arr.set(i, self.to_value(item)?)?;
                }
                arr.into_value()
            }
            JsValue::Object(id) => self.wrapper(*id)?.into_value(),
            JsValue::Handle(r) => self.restore(r)?,
            JsValue::Map(fields) => {
                let o = Object::new(ctx.clone())?;
                for (k, v) in fields {
                    o.set(k.as_str(), self.to_value(v)?)?;
                }
                o.into_value()
            }
            JsValue::Color([r, g, b, a]) | JsValue::InvalidColor([r, g, b, a]) => {
                let (make, _) = self.color_face()?;
                let valid = matches!(v, JsValue::Color(_));
                make.call((
                    i32::from(*r),
                    i32::from(*g),
                    i32::from(*b),
                    i32::from(*a),
                    valid,
                ))?
            }
            JsValue::Typed(kind, fields) => {
                let make = self
                    .state
                    .kinds
                    .borrow()
                    .get(kind.index())
                    .map(|k| k.make.clone())
                    .ok_or_else(|| Exception::throw_type(ctx, "unknown kind of value"))?;
                let args = fields
                    .iter()
                    .map(|f| self.to_value(f))
                    .collect::<rquickjs::Result<Vec<_>>>()?;
                make.restore(ctx)?.call((Rest(args),))?
            }
        })
    }

    /// The fields of `o` when it is a value of a defined kind.
    fn read_typed(&self, o: &Object<'js>, depth: u32) -> rquickjs::Result<Option<JsValue>> {
        if self.state.kinds.borrow().is_empty() {
            return Ok(None);
        }
        let Some(proto) = o.get_prototype() else {
            return Ok(None);
        };
        let Some(index) = proto.get::<_, Value<'js>>(KIND_MARK)?.as_int() else {
            return Ok(None);
        };
        let known = usize::try_from(index).ok().and_then(|i| {
            let kinds = self.state.kinds.borrow();
            kinds.get(i).map(|k| (k.proto.clone(), k.fields.clone()))
        });
        let Some((kind_proto, names)) = known else {
            return Ok(None);
        };
        if kind_proto.restore(&self.ctx)? != proto {
            return Ok(None);
        }
        let mut fields = Vec::with_capacity(names.len());
        for name in &names {
            let v: Value<'js> = o.get(name.as_str())?;
            // Script can build an object on the prototype by hand; without
            // the fields it is just an object.
            if v.is_undefined() {
                return Ok(None);
            }
            fields.push(self.read_value_at(&v, depth + 1)?);
        }
        Ok(Some(JsValue::Typed(ValueKind(index as u32), fields)))
    }

    fn color_face(&self) -> rquickjs::Result<(Function<'js>, Object<'js>)> {
        let cached = self
            .state
            .color
            .borrow()
            .as_ref()
            .map(|f| (f.make.clone(), f.proto.clone()));
        if let Some((make, proto)) = cached {
            return Ok((make.restore(&self.ctx)?, proto.restore(&self.ctx)?));
        }
        let face: Object<'js> = self.ctx.eval(crate::color::FACE)?;
        let make: Function<'js> = face.get("make")?;
        let proto: Object<'js> = face.get("proto")?;
        *self.state.color.borrow_mut() = Some(ColorFace {
            make: Persistent::save(&self.ctx, make.clone()),
            proto: Persistent::save(&self.ctx, proto.clone()),
        });
        Ok((make, proto))
    }

    /// `o` when it is a colour object.
    fn read_color(&self, o: &Object<'js>) -> rquickjs::Result<Option<JsValue>> {
        let proto = self.state.color.borrow().as_ref().map(|f| f.proto.clone());
        let Some(proto) = proto else {
            return Ok(None);
        };
        if o.get_prototype() != Some(proto.restore(&self.ctx)?) {
            return Ok(None);
        }
        let mut out = [0u16; 4];
        for (slot, key) in out.iter_mut().zip(["r", "g", "b", "a"]) {
            // Script can build an object on the colour prototype by hand;
            // without the channels it is just an object.
            let Some(v) = o.get::<_, Value<'js>>(key)?.as_number() else {
                return Ok(None);
            };
            *slot = (v * 65535.0).round().clamp(0.0, 65535.0) as u16;
        }
        let valid = o.get::<_, Value<'js>>("valid")?.as_bool() != Some(false);
        Ok(Some(if valid {
            JsValue::Color(out)
        } else {
            JsValue::InvalidColor(out)
        }))
    }

    /// A JS value for Rust. An array is copied, entry by entry.
    pub fn from_value(&self, v: &Value<'js>) -> rquickjs::Result<JsValue> {
        self.read_value_at(v, 0)
    }

    /// [`Scope::from_value`], except that an array is not copied: it
    /// crosses as a [`JsValue::Handle`] whose [`JsRef::is_array`] is true,
    /// so Rust holds the script's own array.
    pub fn from_value_kept(&self, v: &Value<'js>) -> rquickjs::Result<JsValue> {
        if !v.is_proxy() && v.is_array() {
            return Ok(JsValue::Handle(self.keep(v.clone())));
        }
        self.read_value_at(v, 0)
    }

    /// What a script gave the embedder: an argument, a value assigned, a
    /// function's result.
    fn script_value(&self, v: &Value<'js>) -> rquickjs::Result<JsValue> {
        if self.state.arrays_by_reference.get() {
            self.from_value_kept(v)
        } else {
            self.from_value(v)
        }
    }

    fn read_value_at(&self, v: &Value<'js>, depth: u32) -> rquickjs::Result<JsValue> {
        // Cyclic arrays would otherwise recurse until the native stack limit.
        if depth > 32 {
            return Err(Exception::throw_type(&self.ctx, "value nested too deeply"));
        }
        // A proxy of an array says it is an array, and reading it entry by
        // entry would run its traps: it crosses as the object it is.
        if v.is_proxy() {
            return Ok(JsValue::Handle(self.keep(v.clone())));
        }
        Ok(match v.type_of() {
            Type::Uninitialized | Type::Undefined => JsValue::Undefined,
            Type::Null => JsValue::Null,
            Type::Bool => JsValue::Bool(v.as_bool().unwrap_or(false)),
            Type::Int | Type::Float => JsValue::Number(v.as_number().unwrap_or(f64::NAN)),
            Type::String => JsValue::String(v.as_string().expect("string").to_string()?),
            Type::Array => {
                let arr = v.as_array().expect("array");
                let mut out = Vec::with_capacity(arr.len());
                for item in arr.iter::<Value>() {
                    out.push(self.read_value_at(&item?, depth + 1)?);
                }
                JsValue::Array(out)
            }
            Type::Object => {
                let o = v.as_object().expect("object");
                match Class::<HostWrapper>::from_object(o) {
                    Some(c) => JsValue::Object(c.borrow().id),
                    None => match self.read_color(o)? {
                        Some(c) => c,
                        None => match self.read_typed(o, depth)? {
                            Some(typed) => typed,
                            None => JsValue::Handle(self.keep(v.clone())),
                        },
                    },
                }
            }
            // Identity matters for these (a handler is disconnected by the
            // same function), so they cross by reference.
            Type::Function
            | Type::Constructor
            | Type::Promise
            | Type::Exception
            | Type::Proxy
            | Type::Symbol => JsValue::Handle(self.keep(v.clone())),
            other => {
                return Err(Exception::throw_type(
                    &self.ctx,
                    &format!("{other:?} cannot cross into the engine"),
                ));
            }
        })
    }
}

/// The address of the object behind `v`, which is what tells one JS object
/// from another on the Rust side.
fn object_address(v: &Value<'_>) -> Option<usize> {
    if !v.is_object() {
        return None;
    }
    // SAFETY: the value is tagged as an object, so its payload is the
    // pointer to it; the pointer is only compared, never followed.
    Some(unsafe { rquickjs::qjs::JS_VALUE_GET_PTR(v.as_raw()) } as usize)
}

/// A loaded module and the functions it exports.
#[derive(Debug, Clone)]
pub struct ModuleHandle {
    pub name: String,
    /// Exports in namespace order (QuickJS sorts them by name).
    pub exports: Vec<(String, FnId)>,
}

impl ModuleHandle {
    pub fn function(&self, name: &str) -> Option<FnId> {
        self.exports
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, f)| *f)
    }

    pub fn function_at(&self, index: usize) -> Option<FnId> {
        self.exports.get(index).map(|(_, f)| *f)
    }
}

/// A snapshot of QuickJS allocator statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryStats {
    pub malloc_size: usize,
    pub malloc_count: usize,
    pub memory_used_size: usize,
    pub object_count: usize,
    pub js_func_count: usize,
}

/// The JS host: one Runtime and Context. `!Send` by construction (rquickjs
/// without `parallel`).
pub struct JsHost {
    // Field order is drop order. The state owns Persistents, which must be
    // released while the runtime is still alive (QuickJS asserts that no
    // objects leak at runtime teardown), so it goes first; `Drop` also clears
    // it explicitly.
    state: Rc<HostState>,
    /// The context pointer, for re-entrant runs (see [`JsHost::enter`]).
    /// Valid as long as `ctx` is.
    raw: NonNull<rquickjs::qjs::JSContext>,
    ctx: Context,
    rt: Runtime,
    interrupt: Rc<InterruptState>,
    cfg: JsConfig,
    last_collect: Cell<usize>,
    /// Loaded bytecode is read in place (QuickJS `ROM_DATA`): module code
    /// points into these buffers, so they must outlive the runtime. Declared
    /// last, so they are freed last.
    bytecode: RefCell<Vec<std::sync::Arc<[u8]>>>,
}

impl std::fmt::Debug for JsHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsHost")
            .field("cfg", &self.cfg)
            .finish_non_exhaustive()
    }
}

impl Drop for JsHost {
    fn drop(&mut self) {
        self.state.fns.borrow_mut().clear();
        self.state.types.borrow_mut().clear();
        self.state.wrappers.borrow_mut().clear();
        self.state.handles.borrow_mut().clear();
        self.state.by_object.borrow_mut().clear();
        self.state.color.borrow_mut().take();
        self.state.owned_face.borrow_mut().take();
        self.state.kinds.borrow_mut().clear();
    }
}

struct EnterGuard<'a>(&'a Cell<bool>);
impl Drop for EnterGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// How much slower a build without optimisations runs scripts (the
/// interpreter's C too). The soft budget is what an optimised build
/// should keep to; a debug build that warned at it would warn about
/// nothing a user gets.
const UNOPTIMISED: u32 = if cfg!(debug_assertions) { 8 } else { 1 };

impl JsHost {
    pub fn new(cfg: JsConfig) -> Result<Self, JsFault> {
        let rt = Runtime::new()?;
        if let Some(limit) = cfg.memory_limit {
            rt.set_memory_limit(limit);
        }
        rt.set_gc_threshold(cfg.gc_threshold);
        rt.set_max_stack_size(cfg.max_stack);

        let interrupt = Rc::new(InterruptState::default());
        {
            let st = interrupt.clone();
            // QuickJS calls this about every 10k opcodes (roughly 266 us), so
            // one clock read per call costs well under 0.5 %.
            rt.set_interrupt_handler(Some(Box::new(move || match st.deadline.get() {
                Some(d) if Instant::now() >= d => {
                    st.fired.set(true);
                    true
                }
                _ => false,
            })));
        }
        rt.set_host_promise_rejection_tracker(Some(Box::new(|_ctx, _promise, reason, handled| {
            if !handled {
                let text = reason
                    .get::<rquickjs::convert::Coerced<String>>()
                    .map(|c| c.0)
                    .unwrap_or_default();
                tracing::warn!(reason = %text, "unhandled promise rejection");
            }
        })));

        let ctx = Context::full(&rt)?;
        let raw = ctx.with(|c| c.as_raw());
        let state = Rc::new(HostState {
            fns: RefCell::default(),
            types: RefCell::default(),
            wrappers: RefCell::default(),
            host: RefCell::default(),
            released: Rc::default(),
            owned: RefCell::default(),
            owned_face: RefCell::default(),
            mapper: RefCell::default(),
            text: RefCell::default(),
            entered: Cell::new(false),
            handles: RefCell::default(),
            by_object: RefCell::default(),
            arrays_by_reference: Cell::new(false),
            next_handle: Cell::new(1),
            handle_release: Arc::default(),
            color: RefCell::default(),
            kinds: RefCell::default(),
        });
        Ok(Self {
            state,
            raw,
            ctx,
            rt,
            interrupt,
            cfg,
            last_collect: Cell::new(0),
            bytecode: RefCell::default(),
        })
    }

    pub fn config(&self) -> &JsConfig {
        &self.cfg
    }

    /// Installs the Engine side of the host-object seam.
    pub fn set_host_objects(&self, host: Rc<dyn HostObjects>) {
        self.state
            .arrays_by_reference
            .set(host.arrays_by_reference());
        *self.state.host.borrow_mut() = Some(host);
    }

    /// Installs the hook that maps JS positions to QML positions in
    /// [`JsFault::JsException`].
    pub fn set_source_mapper(&self, mapper: Option<SourceMapper>) {
        *self.state.mapper.borrow_mut() = mapper.map(Rc::new);
    }

    /// Installs the hook that reads a line of a module's source, so an
    /// exception says what the script was doing where it threw: a method
    /// called through null is not a property read through it.
    pub fn set_source_text(&self, text: Option<SourceText>) {
        *self.state.text.borrow_mut() = text.map(Rc::new);
    }

    /// Runs `f` with the interrupt armed for `slice.hard`, and maps every
    /// failure onto a [`JsFault`]. Exceeding `slice.soft` is logged with
    /// `label`.
    ///
    /// Called from inside a run (a host callback that pulls a lazy binding
    /// whose code is JS), it re-enters the context that is already locked
    /// and keeps the outer deadline: the nested work is part of the outer
    /// slice (ARCHITECTURE.md section 6.1). A failure inside the nested run
    /// is classified and its exception drained, so the outer JS continues
    /// unless the deadline has passed, in which case it is interrupted too.
    pub fn enter<R>(
        &self,
        label: &str,
        slice: Slice,
        f: impl for<'js> FnOnce(&Scope<'js>) -> rquickjs::Result<R>,
    ) -> Result<R, JsFault> {
        self.enter_naming(label, String::new, slice, f)
    }

    /// [`Host::enter`], with `what` naming the code that ran (a QML file
    /// and line) in the warning over `slice.soft`. `what` is only called
    /// then, so a dispatch within its budget pays nothing for the name.
    pub fn enter_naming<R>(
        &self,
        label: &str,
        what: impl FnOnce() -> String,
        slice: Slice,
        f: impl for<'js> FnOnce(&Scope<'js>) -> rquickjs::Result<R>,
    ) -> Result<R, JsFault> {
        if self.state.entered.get() {
            return self.enter_nested(label, f);
        }
        self.state.entered.set(true);
        let guard = EnterGuard(&self.state.entered);
        self.state.drain_handles();
        self.interrupt.fired.set(false);
        let start = Instant::now();
        self.interrupt.deadline.set(Some(start + slice.hard));

        let outcome = catch_unwind(AssertUnwindSafe(|| {
            self.ctx.with(|ctx| {
                let scope = Scope {
                    ctx: ctx.clone(),
                    state: self.state.clone(),
                };
                match f(&scope) {
                    Ok(v) => Ok(v),
                    Err(e) => Err(self.classify(&scope, e, label, start.elapsed())),
                }
            })
        }));

        self.interrupt.deadline.set(None);
        let elapsed = start.elapsed();
        drop(guard);

        let soft = slice.soft * UNOPTIMISED;
        if elapsed > soft {
            let what = what();
            if what.is_empty() {
                tracing::warn!(
                    label,
                    ?elapsed,
                    ?soft,
                    "JS dispatch exceeded its soft budget"
                );
            } else {
                tracing::warn!(
                    label,
                    what,
                    ?elapsed,
                    ?soft,
                    "JS dispatch exceeded its soft budget"
                );
            }
        }
        self.flush_released();
        match outcome {
            Ok(r) => r,
            Err(payload) => Err(JsFault::Panic {
                label: label.to_owned(),
                message: panic_message(payload.as_ref()),
            }),
        }
    }

    fn enter_nested<R>(
        &self,
        label: &str,
        f: impl for<'js> FnOnce(&Scope<'js>) -> rquickjs::Result<R>,
    ) -> Result<R, JsFault> {
        // SAFETY: `entered` is only set while this thread holds the runtime
        // lock (inside `Context::with`, or while a promise job runs), and
        // `raw` is the context of this host, alive as long as `self.ctx`.
        // The `Ctx` does not escape `f`, whose lifetime is fresh.
        let ctx = unsafe { Ctx::from_raw(self.raw) };
        let scope = Scope {
            ctx: ctx.clone(),
            state: self.state.clone(),
        };
        let start = Instant::now();
        f(&scope).map_err(|e| self.classify(&scope, e, label, start.elapsed()))
    }

    fn classify<'js>(
        &self,
        scope: &Scope<'js>,
        err: rquickjs::Error,
        label: &str,
        elapsed: Duration,
    ) -> JsFault {
        let mapper = self.state.mapper.borrow().clone();
        let text = self.state.text.borrow().clone();
        classify_error(
            &scope.ctx,
            Some(scope),
            err,
            label,
            self.interrupt.fired.get(),
            elapsed,
            Sources {
                mapper: mapper.as_deref(),
                text: text.as_deref(),
            },
        )
    }

    /// Calls a function held through a [`JsRef`] under the default slice.
    pub fn call_ref(&self, f: &JsRef, this: &JsValue, args: &[JsArg]) -> Result<JsOut, JsFault> {
        self.enter("call_ref", self.cfg.slice, |scope| {
            scope.call_ref(f, this, args)
        })
    }

    /// Evaluates a classic script (the Engine's global prelude).
    pub fn eval_script(&self, name: &str, source: &str) -> Result<JsOut, JsFault> {
        self.enter(&format!("eval {name}"), self.cfg.load_slice, |scope| {
            let v: Value = scope.ctx.eval(source)?;
            scope.from_value(&v)
        })
    }

    /// Defines a kind of value that crosses as [`JsValue::Typed`].
    ///
    /// `source` is a JS expression that gives `{ make, proto, fields }`:
    /// `fields` names what a value of the kind holds, `make(...fields)`
    /// gives the object JS sees for those, and `proto` is the prototype of
    /// every such object, by which one coming back is known. The kind
    /// decides everything else: what the object prints as, its methods, and
    /// whether equal values are one object (and so `===`).
    pub fn define_value_kind(&self, source: &str) -> Result<ValueKind, JsFault> {
        self.enter("define_value_kind", self.cfg.load_slice, |scope| {
            let face: Object = scope.ctx.eval(source)?;
            let make: Function = face.get("make")?;
            let proto: Object = face.get("proto")?;
            let fields: Vec<String> = face.get("fields")?;
            let index = scope.state.kinds.borrow().len() as u32;
            proto.prop(KIND_MARK, Property::from(index as i32))?;
            scope.state.kinds.borrow_mut().push(KindFace {
                make: Persistent::save(&scope.ctx, make),
                proto: Persistent::save(&scope.ctx, proto),
                fields,
            });
            Ok(ValueKind(index))
        })
    }

    /// Installs `f` as a global function `name`. Arguments and the result
    /// cross as [`JsValue`]s; an error becomes a thrown JS `Error`.
    pub fn set_global_function(&self, name: &str, f: HostFn) -> Result<(), JsFault> {
        self.enter("set_global_function", self.cfg.load_slice, |scope| {
            let func = make_global(&scope.ctx, Rc::downgrade(&scope.state), f.clone(), name)?;
            scope.ctx.globals().set(name, func)
        })
    }

    /// Calls an exported function under the default slice.
    pub fn call(&self, f: FnId, args: &[JsArg]) -> Result<JsOut, JsFault> {
        let label = self
            .state
            .fns
            .borrow()
            .get(f)
            .map(|e| e.name.clone())
            .ok_or_else(|| JsFault::Internal("stale function handle".into()))?;
        self.enter(&label, self.cfg.slice, |scope| scope.call(f, args))
    }

    /// Compiles and evaluates a module from source.
    pub fn load_module(&self, name: &str, source: &str) -> Result<ModuleHandle, JsFault> {
        self.enter(
            &format!("load_module {name}"),
            self.cfg.load_slice,
            |scope| {
                let m = Module::declare(scope.ctx.clone(), name, source)?;
                register_module(scope, m, name)
            },
        )
    }

    /// Evaluates a module from precompiled bytecode (section 3.2: roughly an
    /// order of magnitude faster than compiling).
    pub fn load_module_bytecode(&self, bytecode: &Bytecode) -> Result<ModuleHandle, JsFault> {
        // Keep the buffer alive for the life of the host; see `bytecode`.
        self.bytecode.borrow_mut().push(bytecode.shared());
        let bytes = bytecode.as_bytes();
        self.enter("load_module_bytecode", self.cfg.load_slice, |scope| {
            // SAFETY: `Bytecode` can only be produced by `Bytecode::compile`
            // or the unsafe `from_trusted`, so the bytes are well-formed
            // output of this QuickJS version, and the buffer outlives the
            // runtime (retained above).
            let m = unsafe { Module::load(scope.ctx.clone(), bytes)? };
            let name: String = m.name()?;
            register_module(scope, m, &name)
        })
    }

    /// Drops our handles to a module's functions. QuickJS cannot unload the
    /// module record itself, so repeated hot reloads must use fresh module
    /// names (ARCHITECTURE.md section 12.1).
    pub fn unload_module(&self, module: &ModuleHandle) {
        let mut fns = self.state.fns.borrow_mut();
        for (_, f) in &module.exports {
            fns.remove(*f);
        }
    }

    /// Registers a host type: one prototype with accessor properties whose
    /// getters and setters call [`HostObjects`].
    pub fn register_type(&self, def: &TypeDef) -> Result<HostTypeId, JsFault> {
        let proto = self.enter("register_type", self.cfg.load_slice, |scope| {
            let proto = Object::new(scope.ctx.clone())?;
            let weak = Rc::downgrade(&scope.state);
            for p in &def.props {
                install_prop(&proto, weak.clone(), p)?;
            }
            for m in &def.methods {
                install_method(&proto, weak.clone(), m.id, &m.name)?;
            }
            for sig in &def.signals {
                install_signal(&proto, weak.clone(), sig)?;
            }
            if def.plain {
                return Ok(Persistent::save(&scope.ctx, proto));
            }
            let name = def.name.clone();
            // As Qt prints an object: its class, its address, and its
            // objectName when it has one.
            let to_string = Function::new(
                scope.ctx.clone(),
                move |ctx: Ctx<'_>, This(this): This<Class<'_, HostWrapper>>| -> String {
                    let id = this.borrow().id;
                    let named = match this.get::<_, Value>("objectName") {
                        Ok(v) => v
                            .as_string()
                            .and_then(|s| s.to_string().ok())
                            .filter(|s| !s.is_empty()),
                        // A destroyed object has no name to read.
                        Err(_) => {
                            let _ = ctx.catch();
                            None
                        }
                    };
                    match named {
                        Some(n) => format!("{name}({id:#x}, \"{n}\")"),
                        None => format!("{name}({id:#x})"),
                    }
                },
            )?
            .with_name("toString")?;
            proto.prop(
                "toString",
                Property::from(to_string).writable().configurable(),
            )?;
            Ok(Persistent::save(&scope.ctx, proto))
        })?;
        let mut types = self.state.types.borrow_mut();
        let slot = match types.iter().position(Option::is_none) {
            Some(free) => {
                types[free] = Some(proto);
                free
            }
            None => {
                types.push(Some(proto));
                types.len() - 1
            }
        };
        Ok(HostTypeId(
            u32::try_from(slot).expect("type count fits u32"),
        ))
    }

    /// Lets go of the prototype of `id`, for a type no object will have
    /// again (one a hot reload replaced). Wrappers made with it keep it;
    /// the id may be given to a type registered later.
    pub fn unregister_type(&self, id: HostTypeId) {
        if let Some(slot) = self.state.types.borrow_mut().get_mut(id.0 as usize) {
            *slot = None;
        }
    }

    /// Forgets the cached wrapper of `obj`. JS that still holds it keeps a
    /// dangling wrapper whose accessors fail with "object no longer exists";
    /// the next `wrapper(obj)` makes a fresh one. Call it when the Engine
    /// destroys the object.
    pub fn drop_wrapper(&self, obj: u64) -> bool {
        self.state.owned.borrow_mut().remove(&obj);
        self.state.wrappers.borrow_mut().remove(&obj).is_some()
    }

    /// Gives `obj` to the scripts: from now on its wrapper lives only as
    /// long as a script holds it, the object is still one object to them
    /// while one does, and [`HostObjects::released_many`] says when none
    /// does any more. False, and nothing changes, when `obj` has crossed
    /// already: the wrapper it got then is the Engine's and never goes.
    pub fn give_to_script(&self, obj: u64) -> bool {
        if self.state.wrappers.borrow().contains_key(&obj) {
            return false;
        }
        self.state.owned.borrow_mut().entry(obj).or_insert(0);
        true
    }

    /// Whether a wrapper of the script-owned `obj` is alive: a script
    /// holds it, or the collector has not looked since the last one let
    /// go.
    pub fn script_holds(&self, obj: u64) -> bool {
        self.state.owned.borrow().get(&obj).is_some_and(|n| *n > 0)
    }

    pub fn wrapper_count(&self) -> usize {
        self.state.wrappers.borrow().len()
    }

    /// Delivers finaliser notifications for JS-owned wrappers. Already called
    /// at the end of every run; exposed for the idle lane.
    pub fn flush_released(&self) {
        if !self.state.entered.get() {
            self.state.drain_handles();
        }
        let ids = std::mem::take(&mut *self.state.released.borrow_mut());
        if ids.is_empty() {
            return;
        }
        // A wrapper of a script-owned object may have been made again
        // before the death of the one before it was reported.
        let gone: Vec<u64> = {
            let mut owned = self.state.owned.borrow_mut();
            ids.into_iter()
                .filter(|id| match owned.get_mut(id) {
                    Some(n) => {
                        *n = n.saturating_sub(1);
                        *n == 0
                    }
                    None => true,
                })
                .collect()
        };
        let host = self.state.host.borrow().clone();
        if let Some(host) = host
            && !gone.is_empty()
        {
            host.released_many(&gone);
        }
    }

    /// Drains the promise job queue until it is empty or `until` passes.
    /// Returns the number of jobs executed. A job that throws is logged and
    /// does not stop the others.
    pub fn pump_jobs(&self, until: Instant) -> usize {
        if self.state.entered.replace(true) {
            return 0;
        }
        let guard = EnterGuard(&self.state.entered);
        self.interrupt.fired.set(false);
        self.interrupt.deadline.set(Some(until));
        let mut n = 0;
        while Instant::now() < until {
            match self.rt.execute_pending_job() {
                Ok(true) => n += 1,
                Ok(false) => break,
                Err(e) => {
                    n += 1;
                    let text = e.0.with(|ctx| {
                        let v = ctx.catch();
                        v.get::<rquickjs::convert::Coerced<String>>()
                            .map(|c| c.0)
                            .unwrap_or_else(|_| "<unprintable>".into())
                    });
                    tracing::warn!(error = %text, "promise job threw");
                }
            }
        }
        self.interrupt.deadline.set(None);
        drop(guard);
        self.flush_released();
        n
    }

    pub fn has_pending_jobs(&self) -> bool {
        !self.state.entered.get() && self.rt.is_job_pending()
    }

    /// Runs the cycle collector if the heap grew enough since the last
    /// collection. Meant for the Idle lane; returns whether it ran.
    pub fn collect_if_idle(&self) -> bool {
        let Some(stats) = self.memory_stats() else {
            return false;
        };
        let last = self.last_collect.get();
        if stats.malloc_size < last.saturating_add(self.cfg.idle_collect_min_growth) {
            return false;
        }
        self.rt.run_gc();
        self.last_collect
            .set(self.memory_stats().map_or(0, |s| s.malloc_size));
        self.flush_released();
        true
    }

    /// Unconditional collection (memory pressure). Not available inside a run.
    pub fn collect_now(&self) -> bool {
        if self.state.entered.get() {
            return false;
        }
        self.rt.run_gc();
        self.last_collect
            .set(self.memory_stats().map_or(0, |s| s.malloc_size));
        self.flush_released();
        true
    }

    /// `None` inside a run: rquickjs panics if asked for usage while its lock
    /// is held (03 section 2.2).
    pub fn memory_stats(&self) -> Option<MemoryStats> {
        if self.state.entered.get() {
            return None;
        }
        let u = self.rt.memory_usage();
        Some(MemoryStats {
            malloc_size: usize::try_from(u.malloc_size).unwrap_or(0),
            malloc_count: usize::try_from(u.malloc_count).unwrap_or(0),
            memory_used_size: usize::try_from(u.memory_used_size).unwrap_or(0),
            object_count: usize::try_from(u.obj_count).unwrap_or(0),
            js_func_count: usize::try_from(u.js_func_count).unwrap_or(0),
        })
    }
}

fn register_module<'js>(
    scope: &Scope<'js>,
    m: Module<'js, rquickjs::module::Declared>,
    name: &str,
) -> rquickjs::Result<ModuleHandle> {
    let (m, promise) = m.eval()?;
    // Without top-level await the evaluation promise is already settled once
    // the job queue is empty.
    promise.finish::<()>()?;
    let ns = m.namespace()?;
    let mut exports = Vec::new();
    {
        let mut fns = scope.state.fns.borrow_mut();
        for kv in ns.props::<String, Value>() {
            let (key, value) = kv?;
            if let Some(func) = value.into_function() {
                let id = fns.insert(FnEntry {
                    func: Persistent::save(&scope.ctx, func),
                    name: format!("{name}::{key}").into(),
                });
                exports.push((key, id));
            }
        }
    }
    Ok(ModuleHandle {
        name: name.to_owned(),
        exports,
    })
}

/// Accessors are built from closures, not `Function` values: rquickjs only
/// implements `AsProperty` for `IntoJsFunc` types.
fn install_prop<'js>(
    proto: &Object<'js>,
    state: Weak<HostState>,
    p: &PropDef,
) -> rquickjs::Result<()> {
    let prop = p.id;
    let st = state.clone();
    let getter = move |ctx: Ctx<'js>,
                       This(this): This<Class<'js, HostWrapper>>|
          -> rquickjs::Result<Value<'js>> {
        let (scope, host) = callback_scope(&ctx, &st)?;
        let id = this.borrow().id;
        let out = host.get(&scope, id, prop).map_err(|e| e.into_js(&ctx))?;
        scope.to_value(&out)
    };
    if !p.writable {
        return proto.prop(
            p.name.as_str(),
            Accessor::new_get(getter).configurable().enumerable(),
        );
    }
    let setter = move |ctx: Ctx<'js>,
                       This(this): This<Class<'js, HostWrapper>>,
                       value: Value<'js>|
          -> rquickjs::Result<()> {
        let (scope, host) = callback_scope(&ctx, &state)?;
        let id = this.borrow().id;
        let v = scope.script_value(&value)?;
        host.set(&scope, id, prop, v).map_err(|e| e.into_js(&ctx))
    };
    proto.prop(
        p.name.as_str(),
        Accessor::new_get(getter)
            .set(setter)
            .configurable()
            .enumerable(),
    )
}

/// A signal reads as a function bound to its object: calling it emits, and
/// it carries `connect` and `disconnect`. A fresh function per read keeps the
/// prototype free of per-object state; signals are read rarely enough that
/// the allocation does not matter.
fn install_signal<'js>(
    proto: &Object<'js>,
    state: Weak<HostState>,
    sig: &SignalDef,
) -> rquickjs::Result<()> {
    let signal = sig.id;
    let name = sig.name.clone();
    let getter = move |ctx: Ctx<'js>,
                       This(this): This<Class<'js, HostWrapper>>|
          -> rquickjs::Result<Function<'js>> {
        let id = this.borrow().id;
        let st = state.clone();
        let emit = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>, args: Rest<Value<'js>>| -> rquickjs::Result<()> {
                let (scope, host) = callback_scope(&ctx, &st)?;
                let argv = args
                    .0
                    .iter()
                    .map(|a| scope.script_value(a))
                    .collect::<rquickjs::Result<Vec<_>>>()?;
                host.emit(&scope, id, signal, &argv)
                    .map_err(|e| e.into_js(&ctx))
            },
        )?
        .with_name(name.as_str())?;
        for (verb, connect) in [("connect", true), ("disconnect", false)] {
            let st = state.clone();
            let f = Function::new(
                ctx.clone(),
                move |ctx: Ctx<'js>, args: Rest<Value<'js>>| -> rquickjs::Result<()> {
                    let (scope, host) = callback_scope(&ctx, &st)?;
                    let argv = args
                        .0
                        .iter()
                        .map(|a| scope.from_value(a))
                        .collect::<rquickjs::Result<Vec<_>>>()?;
                    let r = if connect {
                        host.connect(&scope, id, signal, &argv)
                    } else {
                        host.disconnect(&scope, id, signal, &argv)
                    };
                    r.map_err(|e| e.into_js(&ctx))
                },
            )?
            .with_name(verb)?;
            emit.set(verb, f)?;
        }
        Ok(emit)
    };
    proto.prop(
        sig.name.as_str(),
        Accessor::new_get(getter).configurable().enumerable(),
    )
}

fn make_global<'js>(
    ctx: &Ctx<'js>,
    state: Weak<HostState>,
    f: HostFn,
    name: &str,
) -> rquickjs::Result<Function<'js>> {
    Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, args: Rest<Value<'js>>| -> rquickjs::Result<Value<'js>> {
            let Some(state) = state.upgrade() else {
                return Err(Exception::throw_message(&ctx, "JS host was dropped"));
            };
            let scope = Scope {
                ctx: ctx.clone(),
                state,
            };
            let argv = args
                .0
                .iter()
                .map(|a| scope.from_value(a))
                .collect::<rquickjs::Result<Vec<_>>>()?;
            let out = f(&scope, &argv).map_err(|e| e.into_js(&ctx))?;
            scope.to_value(&out)
        },
    )?
    .with_name(name)
}

/// A method reads as a function bound to its object, as QML methods are:
/// `Qt.callLater(obj.method)` and `sig.connect(obj.method)` keep the
/// object. The bound function is made on first read and kept on the
/// wrapper, so reads compare equal (`disconnect(obj.method)` works).
fn install_method<'js>(
    proto: &Object<'js>,
    state: Weak<HostState>,
    method: u32,
    name: &str,
) -> rquickjs::Result<()> {
    let key = name.to_owned();
    let getter = move |ctx: Ctx<'js>,
                       This(this): This<Class<'js, HostWrapper>>|
          -> rquickjs::Result<Value<'js>> {
        let id = this.borrow().id;
        let f = make_method(&ctx, state.clone(), id, method, &key)?;
        if let Some(o) = this.as_object() {
            o.prop(
                key.as_str(),
                Property::from(f.clone()).writable().configurable(),
            )?;
        }
        Ok(f.into_value())
    };
    proto.prop(name, Accessor::new_get(getter).configurable())
}

fn make_method<'js>(
    ctx: &Ctx<'js>,
    state: Weak<HostState>,
    id: u64,
    method: u32,
    name: &str,
) -> rquickjs::Result<Function<'js>> {
    Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, args: Rest<Value<'js>>| -> rquickjs::Result<Value<'js>> {
            let (scope, host) = callback_scope(&ctx, &state)?;
            let argv = args
                .0
                .iter()
                .map(|a| scope.script_value(a))
                .collect::<rquickjs::Result<Vec<_>>>()?;
            let out = host
                .call(&scope, id, method, &argv)
                .map_err(|e| e.into_js(&ctx))?;
            scope.to_value(&out)
        },
    )?
    .with_name(name)
}

fn callback_scope<'js>(
    ctx: &Ctx<'js>,
    state: &Weak<HostState>,
) -> rquickjs::Result<(Scope<'js>, Rc<dyn HostObjects>)> {
    let Some(state) = state.upgrade() else {
        return Err(Exception::throw_message(ctx, "JS host was dropped"));
    };
    let host = state.host()?;
    Ok((
        Scope {
            ctx: ctx.clone(),
            state,
        },
        host,
    ))
}

/// What an exception is described from beside itself.
#[derive(Clone, Copy, Default)]
pub(crate) struct Sources<'a> {
    pub(crate) mapper: Option<&'a SourceMapper>,
    pub(crate) text: Option<&'a SourceText>,
}

/// Turns an rquickjs error into a [`JsFault`], draining the pending exception.
/// With a `scope`, an exception keeps the value thrown.
pub(crate) fn classify_error<'js>(
    ctx: &Ctx<'js>,
    scope: Option<&Scope<'js>>,
    err: rquickjs::Error,
    label: &str,
    interrupted: bool,
    elapsed: Duration,
    sources: Sources<'_>,
) -> JsFault {
    match err {
        _ if interrupted => {
            // Clear the pending exception so the context is clean.
            let _ = ctx.catch();
            JsFault::JsInterrupted {
                label: label.to_owned(),
                after: elapsed,
            }
        }
        rquickjs::Error::Allocation => JsFault::JsOutOfMemory {
            label: label.to_owned(),
        },
        rquickjs::Error::Exception => {
            let thrown = ctx.catch();
            let mut fault = describe_exception(&thrown, label, sources);
            if let (JsFault::JsException { thrown: kept, .. }, Some(scope)) = (&mut fault, scope) {
                *kept = scope.from_value(&thrown).ok();
                // Reading it can only fail by throwing (an array nested too
                // deep): the context is left clean, and the value goes.
                if kept.is_none() {
                    let _ = ctx.catch();
                }
            }
            fault
        }
        other => JsFault::Internal(other.to_string()),
    }
}

fn describe_exception(
    thrown: &Value<'_>,
    label: &str,
    Sources { mapper, text }: Sources<'_>,
) -> JsFault {
    let as_text = |v: &Value<'_>| {
        v.get::<rquickjs::convert::Coerced<String>>()
            .map(|c| c.0)
            .ok()
    };
    let (message, stack, mut file, mut line) = match thrown
        .as_object()
        .and_then(|o| Exception::from_object(o.clone()))
    {
        Some(e) => {
            let o = e.as_object();
            let stack = e.stack().unwrap_or_default();
            let file = o.get::<_, Option<String>>("fileName").ok().flatten();
            let line = o.get::<_, Option<u32>>("lineNumber").ok().flatten();
            let (sf, sl) = first_frame(&stack);
            // An error is reported with the name of its kind, as QML
            // reports one: `Error: boom`, `TypeError: ...`.
            let message = e.message().unwrap_or_default();
            // QuickJS words a read through nothing as QML does, but for
            // the capital: one spelling for whoever matches the log. It
            // words a method called through nothing the same; QML says it
            // could not call the method, which the source tells.
            let message = match message.strip_prefix("cannot read property ") {
                Some(rest) => {
                    let called = text.zip(top_place(&stack)).is_some_and(|(t, (f, l, c))| {
                        let prop = rest.strip_prefix('\'').and_then(|r| r.rsplit_once("' of "));
                        prop.zip(t(f, l))
                            .is_some_and(|((prop, _), line)| calls(&line, c, prop))
                    });
                    if called {
                        format!("Cannot call method {rest}")
                    } else {
                        format!("Cannot read property {rest}")
                    }
                }
                None => message,
            };
            // Writing through nothing, Qt's engine says nothing is not an
            // object, as it says of deleting through it.
            let message = match message
                .strip_prefix("cannot set property '")
                .and_then(|r| r.rsplit_once("' of "))
            {
                Some((_, what @ ("undefined" | "null"))) => {
                    format!("Value is {what} and could not be converted to an object")
                }
                _ => message,
            };
            let message = match o.get::<_, Option<String>>("name").ok().flatten() {
                // Heap exhaustion is told apart by its bare message below.
                Some(name) if !name.is_empty() && name != "InternalError" => {
                    format!("{name}: {message}")
                }
                _ => message,
            };
            (message, stack, file.or(sf), line.or(sl))
        }
        None => (
            as_text(thrown).unwrap_or_else(|| "<non-error value thrown>".into()),
            String::new(),
            None,
            None,
        ),
    };
    // QuickJS reports heap exhaustion as an InternalError with this message.
    if message == "out of memory" {
        return JsFault::JsOutOfMemory {
            label: label.to_owned(),
        };
    }
    if let Some(m) = mapper {
        // The exception is the embedder's where its code was running: the
        // place it was thrown at when the mapper knows that place, else the
        // nearest caller it knows. What a host function or a script of the
        // embedder's own throws is then reported where it was asked for.
        let thrown_at = match (file.as_deref(), line) {
            (Some(f), Some(l)) => m(f, l),
            _ => None,
        };
        if let Some((mf, ml)) = thrown_at.or_else(|| frames(&stack).find_map(|(f, l)| m(f, l))) {
            file = Some(mf);
            line = Some(ml);
        }
    }
    JsFault::JsException {
        message,
        stack,
        file,
        line,
        thrown: None,
    }
}

/// Parses `at fn (file:line:col)` or `at file:line:col` from the first frame.
fn first_frame(stack: &str) -> (Option<String>, Option<u32>) {
    match stack
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("at "))
        .and_then(frame_place)
    {
        Some((f, l)) => (Some(f.to_owned()), Some(l)),
        None => (None, None),
    }
}

/// The file and line of every frame of a stack that has them, the one that
/// threw first. A native function's frame has neither.
fn frames(stack: &str) -> impl Iterator<Item = (&str, u32)> {
    stack
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("at "))
        .filter_map(frame_place)
}

/// Each frame's function and the line it is at: `("f", "m.js:5:45")`,
/// `("apply", "native")`; an anonymous function's name is empty.
fn named_frames(stack: &str) -> impl Iterator<Item = (&str, &str)> {
    stack
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("at "))
        .map(|f| match f.rfind(" (") {
            Some(open) if f.ends_with(')') => (&f[..open], &f[open + 2..f.len() - 1]),
            _ => ("", f),
        })
}

/// The member called at column `col` (1-based) of `line`: `prop` and the
/// names it is read through, `["root", "a"]` of `root.a.prop(`. QuickJS
/// puts the column at the member's name; past a space, at the bracket or
/// at what follows it. None unless the callee is such a chain of names,
/// whole.
pub(crate) fn called_member(line: &str, col: u32) -> Option<(Vec<String>, String)> {
    let s: Vec<char> = line.chars().collect();
    let i = col.checked_sub(1)? as usize;
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let back_ws = |mut i: usize| {
        while i > 0 && s[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    };
    let mut ends = Vec::new();
    // At the name, which a bracket follows.
    let mut end = i;
    while s.get(end).copied().is_some_and(ident) {
        end += 1;
    }
    let mut open = end;
    while s.get(open).is_some_and(|c| c.is_whitespace()) {
        open += 1;
    }
    if end > i && s.get(open) == Some(&'(') {
        ends.push(end);
    }
    // At the bracket, or at what follows it.
    if s.get(i) == Some(&'(') {
        ends.push(back_ws(i));
    } else if i < s.len() {
        let open = back_ws(i);
        if open > 0 && s[open - 1] == '(' {
            ends.push(back_ws(open - 1));
        }
    }
    ends.into_iter().find_map(|end| chain_before(&s, end))
}

/// The chain of names `a.b.prop` that ends at `end` of `s`, whole.
fn chain_before(s: &[char], end: usize) -> Option<(Vec<String>, String)> {
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let back_ws = |mut i: usize| {
        while i > 0 && s[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    };
    // The name read backwards from just before `end`: its start.
    let name_start = |end: usize| {
        let mut i = end;
        while i > 0 && ident(s[i - 1]) {
            i -= 1;
        }
        (i < end && !s[i].is_ascii_digit()).then_some(i)
    };
    let start = name_start(end)?;
    let prop: String = s[start..end].iter().collect();
    let mut path = Vec::new();
    let mut at = start;
    loop {
        let dot = back_ws(at);
        if dot == 0 || s[dot - 1] != '.' {
            break;
        }
        let end = back_ws(dot - 1);
        let start = name_start(end)?;
        path.push(s[start..end].iter().collect::<String>());
        at = start;
    }
    // The chain is whole: nothing it is read off, `?.`, or a call before it.
    let before = back_ws(at);
    if path.is_empty() || (before > 0 && matches!(s[before - 1], '.' | ')' | ']' | '?')) {
        return None;
    }
    path.reverse();
    Some((path, prop))
}

/// The file, line and column of the frame that threw.
fn top_place(stack: &str) -> Option<(&str, u32, u32)> {
    let frame = stack
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("at "))?;
    let frame = frame.trim_start_matches("at ");
    let inner = match (frame.rfind('('), frame.ends_with(')')) {
        (Some(open), true) => &frame[open + 1..frame.len() - 1],
        _ => frame,
    };
    let mut parts = inner.rsplitn(3, ':');
    let (col, line, file) = (parts.next()?, parts.next()?, parts.next()?);
    Some((file, line.parse().ok()?, col.parse().ok()?))
}

/// Whether the member chain that starts at column `col` (1-based) of
/// `line` calls what it reads as `prop`: `a.prop(`, `a.b.prop (`,
/// `a["prop"](`, `a[i](`. QuickJS puts a failed read at the start of the
/// chain it reads from. The first access to `prop` decides; a chain that
/// goes on past the line, or text this does not follow, is a read.
pub(crate) fn calls(line: &str, col: u32, prop: &str) -> bool {
    let s: Vec<char> = line.chars().skip(col.saturating_sub(1) as usize).collect();
    let mut i = 0;
    let ws = |i: &mut usize| {
        while s.get(*i).is_some_and(|c| c.is_whitespace()) {
            *i += 1;
        }
    };
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    // The closing bracket of the group that opens at `i`, strings skipped.
    let close = |mut i: usize| -> Option<usize> {
        let mut depth = 0usize;
        let mut quote = None;
        while let Some(&c) = s.get(i) {
            match quote {
                Some(_) if c == '\\' => i += 1,
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None => match c {
                    '\'' | '"' | '`' => quote = Some(c),
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => {
                        depth = depth.checked_sub(1)?;
                        if depth == 0 {
                            return Some(i);
                        }
                    }
                    _ => {}
                },
            }
            i += 1;
        }
        None
    };
    match s.first() {
        Some(&c) if ident(c) => {
            while s.get(i).copied().is_some_and(ident) {
                i += 1;
            }
        }
        Some('(' | '[') => match close(0) {
            Some(end) => i = end + 1,
            None => return false,
        },
        _ => return false,
    }
    loop {
        ws(&mut i);
        let named = match s.get(i) {
            Some('.') => {
                i += 1;
                ws(&mut i);
                let start = i;
                while s.get(i).copied().is_some_and(ident) {
                    i += 1;
                }
                s[start..i].iter().copied().eq(prop.chars())
            }
            Some('[') => {
                let Some(end) = close(i) else {
                    return false;
                };
                let inner: String = s[i + 1..end].iter().collect();
                let inner = inner.trim();
                i = end + 1;
                // A key the text gives must be `prop`; one it computes may be.
                match inner.chars().next() {
                    Some(q @ ('\'' | '"')) => {
                        inner.len() >= 2 && inner.ends_with(q) && inner[1..inner.len() - 1] == *prop
                    }
                    Some(c) if c.is_ascii_digit() => inner == prop,
                    _ => true,
                }
            }
            Some('(') => {
                let Some(end) = close(i) else {
                    return false;
                };
                i = end + 1;
                continue;
            }
            _ => return false,
        };
        if named {
            ws(&mut i);
            return s.get(i) == Some(&'(');
        }
    }
}

fn frame_place(frame: &str) -> Option<(&str, u32)> {
    let frame = frame.trim_start_matches("at ");
    let inner = match (frame.rfind('('), frame.ends_with(')')) {
        (Some(open), true) => &frame[open + 1..frame.len() - 1],
        _ => frame,
    };
    let mut parts = inner.rsplitn(3, ':');
    let (_col, line, file) = (parts.next(), parts.next(), parts.next());
    Some((file?, line?.parse().ok()?))
}

fn panic_message(p: &(dyn Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic>".into())
}

#[cfg(test)]
mod tests {
    use super::{called_member, calls};

    #[test]
    fn a_call_is_told_from_a_read() {
        let at = |line: &str, prop| calls(line, line.find('n').unwrap() as u32 + 1, prop);
        assert!(at("x = nothing.foo()", "foo"));
        assert!(at("x = nothing  .  foo  ( 1 )", "foo"));
        assert!(at("x = nothing[\"foo\"]()", "foo"));
        assert!(at("x = nothing[k](1)", "foo"));
        assert!(at("x = nothing.a.foo(2)", "foo"));
        assert!(at("x = nothing(1).foo()", "foo"));
        assert!(!at("x = nothing.foo", "foo"));
        assert!(!at("x = nothing.foo.bar()", "foo"));
        assert!(!at("x = nothing[0]", "0"));
        assert!(!at("x = nothing['bar']()", "foo"));
        assert!(!at("x = nothing.foo", "bar"));
    }

    #[test]
    fn the_member_called_is_found_at_its_column() {
        // The column is that of the text `at` begins.
        let at = |line: &str, at: &str| {
            let col = line.find(at).unwrap() as u32 + 1;
            called_member(line, col).map(|(path, prop)| (path.join("."), prop))
        };
        let some = |path: &str, prop: &str| Some((path.to_owned(), prop.to_owned()));
        assert_eq!(
            at("{ $scope.kid.spawn() }", "spawn"),
            some("$scope.kid", "spawn")
        );
        assert_eq!(at("{ root.obj.a.b(1) }", "b("), some("root.obj.a", "b"));
        assert_eq!(at("{ kid . spawn ( 1 ) }", "1"), some("kid", "spawn"));
        assert_eq!(at("{ kid . spawn (x) }", "x"), some("kid", "spawn"));
        assert_eq!(at("{ kid.spawn  (k(1)) }", "k("), some("kid", "spawn"));
        assert_eq!(at("{ a.f(b.g(1)) }", "g"), some("b", "g"));
        assert_eq!(at("{ kid.spawn () }", "("), some("kid", "spawn"));
        assert_eq!(at("{ spawn() }", "spawn"), None);
        assert_eq!(at("{ a?.b() }", "b"), None);
        assert_eq!(at("{ f().a.b() }", "b"), None);
        assert_eq!(at("{ x[0].a.b() }", "b"), None);
        assert_eq!(at("{ a.b.c }", "c"), None);
    }
}
