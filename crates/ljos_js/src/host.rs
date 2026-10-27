use std::{
    any::Any,
    cell::{Cell, RefCell},
    ops::Deref,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::NonNull,
    rc::{Rc, Weak},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rquickjs::{
    Array, Context, Ctx, Exception, Function, Module, Object, Persistent, Runtime, Type, Value,
    function::{Args, Rest, This},
    object::{Accessor, Property},
};
use rustc_hash::FxHashMap;
use slotmap::{SlotMap, new_key_type};

mod answer;
mod direct;
mod live_list;
mod signal_fn;
pub use direct::{DirectCall, DirectParam};

use crate::{
    Bytecode, HostError, HostObjects, HostTypeId, JsConfig, JsFault, JsRef, JsValue, MethodDef,
    MethodFace, PlainValue, PropDef, SignalDef, Slice, SourceMapper, SourceText, TypeDef,
    ValueKind,
    heap::{Counted, HeapCount},
    objects::{HostWrapper, new_wrapper, register_wrapper_class, wrapper_id},
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
///
/// Each run has a deadline of its own: a run nested in a dispatch (the
/// `Component.onCompleted` of an object a handler makes) gets a whole
/// slice, and the time it took is given back to the runs it is in. Native
/// work a script asks for ([`JsHost::native`]) is given back to all of
/// them. Two limits are never given time back for nested runs: `cap`, on
/// the dispatch's JavaScript as a whole, and `ceiling`, on its wall time.
#[derive(Default)]
struct InterruptState {
    /// The earliest of the innermost run's deadline, `cap` and `ceiling`:
    /// what the handler compares the clock with.
    due: Cell<Option<Instant>>,
    /// The innermost run's deadline.
    deadline: Cell<Option<Instant>>,
    /// The deadlines of the runs it is nested in, outermost first.
    outer: RefCell<Vec<Instant>>,
    cap: Cell<Option<Instant>>,
    ceiling: Cell<Option<Instant>>,
    /// The time given back so far in this dispatch, for a run or native
    /// call around others to give back only what they did not.
    given: Cell<Duration>,
    /// The native work in this dispatch, for the warning of an interrupt.
    native: Cell<Duration>,
    fired: Cell<bool>,
    /// The nested run an interrupt stopped, named for the dispatch's fault.
    stopped_in: RefCell<Option<String>>,
    clock: RefCell<Option<JsClock>>,
}

/// What the time is, for the limits on a run of JavaScript; a test sets
/// one in place of the wall clock ([`JsHost::set_clock`]). It is told what
/// the time is read for.
pub type JsClock = Rc<dyn Fn(Timing) -> Instant>;

/// What a [`JsClock`] is read for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timing {
    /// The interrupt handler, about every 10k opcodes.
    Poll,
    /// A run of JavaScript begins.
    Enter,
    /// A run of JavaScript ends.
    Leave,
    /// Native work a script asked for begins ([`JsHost::native`]).
    NativeBegin,
    /// That native work ends.
    NativeEnd,
}

/// How many slices one dispatch's JavaScript may take with its nested runs.
const CAP: u32 = 4;
/// How many slices one dispatch may take on the wall clock, its native
/// work included.
const CEILING: u32 = 50;

impl InterruptState {
    fn now(&self, at: Timing) -> Instant {
        match &*self.clock.borrow() {
            Some(clock) => clock(at),
            None => Instant::now(),
        }
    }

    fn settle(&self) {
        let due = [self.deadline.get(), self.cap.get(), self.ceiling.get()]
            .into_iter()
            .flatten()
            .min();
        self.due.set(due);
    }

    /// Arms a dispatch that began at `start`; `None` disarms.
    fn arm(&self, start: Instant, slice: Option<Slice>) {
        self.outer.borrow_mut().clear();
        self.given.set(Duration::ZERO);
        self.native.set(Duration::ZERO);
        self.deadline.set(slice.map(|s| start + s.hard));
        self.cap.set(slice.map(|s| start + s.hard * CAP));
        self.ceiling.set(slice.map(|s| start + s.hard * CEILING));
        self.settle();
    }

    /// Moves the deadlines on by what a run or a native call that took
    /// `took`, from when `given` was `before`, has not given back already.
    fn give_back(&self, took: Duration, before: Duration, native: bool) {
        let extra = took.saturating_sub(self.given.get().saturating_sub(before));
        self.given.set(before + took);
        let on = |d: Option<Instant>| d.map(|d| d + extra);
        self.deadline.set(on(self.deadline.get()));
        for d in self.outer.borrow_mut().iter_mut() {
            *d += extra;
        }
        if native {
            self.native.set(self.native.get() + extra);
            self.cap.set(on(self.cap.get()));
        }
        self.settle();
    }
}

pub(crate) struct HostState {
    fns: RefCell<SlotMap<FnId, FnEntry>>,
    /// Prototype per registered type, indexed by [`HostTypeId`]; None
    /// where one was let go of, for the next type to take.
    types: RefCell<Vec<Option<Persistent<Object<'static>>>>>,
    /// The names each prototype of [`HostState::types`] has, in the order
    /// they were given it, for a type made from it to list them in.
    listings: RefCell<Vec<Option<Rc<Listing>>>>,
    /// Engine-owned wrappers. Kept strongly so identity is stable and so the
    /// Engine decides when a wrapper may die ([`JsHost::drop_wrapper`]).
    wrappers: RefCell<FxHashMap<u64, Kept>>,
    host: RefCell<Option<Rc<dyn HostObjects>>>,
    released: Rc<RefCell<Vec<u64>>>,
    /// The objects scripts own ([`JsHost::give_to_script`]), each with
    /// how many of the wrappers made for it are still alive. One at most
    /// is reachable; one more may be dead and not yet reported.
    owned: RefCell<FxHashMap<u64, u32>>,
    /// Made with the first wrapper of a script-owned object.
    owned_face: RefCell<Option<OwnedFace>>,
    mapper: RefCell<Option<Rc<SourceMapper>>>,
    text: RefCell<Option<Rc<SourceText>>>,
    entered: Cell<bool>,
    /// A run nested in this dispatch was interrupted: the dispatch is over,
    /// and the script that called back is stopped as it gets control back.
    stopped: Cell<bool>,
    /// JS values held by Rust through [`JsRef`].
    handles: RefCell<FxHashMap<u64, Held>>,
    /// The reference given out for each held JS object, by the object's
    /// address, so one object has one [`JsRef`] while any is held and
    /// equal references mean the same object. The address cannot be
    /// another object's while the entry in `handles` keeps this one alive.
    by_object: RefCell<FxHashMap<usize, (u64, WeakRef)>>,
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
    /// A panic in a raw accessor ([`raw_accessor`]), which cannot unwind
    /// through QuickJS: kept until the dispatch is back in Rust.
    panicked: RefCell<Option<Box<dyn Any + Send>>>,
    /// The QuickJS class of every wrapper (objects::register_wrapper_class).
    wrapper_class: rquickjs::qjs::JSClassID,
    /// The class of [`JsValue::LiveList`] (live_list.rs), its prototype
    /// and an object that keeps the prototype's shape, made with the first
    /// list, and the atom of `length` it answers.
    list_class: rquickjs::qjs::JSClassID,
    list_proto: RefCell<Option<(Persistent<Object<'static>>, Persistent<Object<'static>>)>>,
    length_atom: rquickjs::qjs::JSAtom,
    /// The class of what a script reads of a signal (signal_fn.rs), and
    /// whether `Function.prototype` has `connect` and `disconnect` yet:
    /// they are given with the first host type, so a host without objects
    /// is left as QuickJS makes it.
    signal_class: rquickjs::qjs::JSClassID,
    connect_installed: Cell<bool>,
    /// The functions of [`JsHost::set_global_function`], by the `magic`
    /// QuickJS calls them with.
    globals: RefCell<Vec<HostFn>>,
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
    /// The prototype of every value of the kind, held so that its address
    /// stays its own.
    #[allow(dead_code, reason = "held for `proto_at`")]
    proto: Persistent<Object<'static>>,
    /// The address of `proto`: what tells a value of the kind from any
    /// other object, without a lookup in JS.
    proto_at: usize,
    /// Shared, so that reading a value does not copy the names.
    fields: Rc<[String]>,
    /// `Object.isFrozen`, as it was when the kind was defined.
    is_frozen: Persistent<Function<'static>>,
    /// The values of the kind that crossed last, newest first, with their
    /// fields. Only frozen ones: their fields cannot change, so the same
    /// fields are the same object, which a kind that keeps one object per
    /// value would give anyway. A value read and written back, such as a
    /// font whose member a script sets, then costs neither a `make` nor a
    /// read of each field.
    recent: RefCell<Vec<(Persistent<Object<'static>>, Vec<JsValue>)>>,
}

/// How many values of a kind [`KindFace::recent`] keeps.
const RECENT_VALUES: usize = 4;

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
    /// The address of `proto`, as [`KindFace::proto_at`].
    proto_at: usize,
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

    /// What a host callback gives back to the script that called it. Once a
    /// run nested in the dispatch has been interrupted, the script is
    /// interrupted too, uncatchably: QuickJS asks whether to interrupt only
    /// every so many steps, and that can keep landing in the nested runs
    /// while the script around them never stops.
    fn back<T>(&self, r: rquickjs::Result<T>) -> rquickjs::Result<T> {
        if !self.state.stopped.get() {
            return r;
        }
        if matches!(r, Err(rquickjs::Error::Exception)) {
            let _ = self.ctx.catch();
        }
        let e = Exception::from_message(self.ctx.clone(), "interrupted")?;
        // SAFETY: both are alive for the call; the flag only marks the
        // error object, as QuickJS marks the one its own interrupt throws.
        unsafe {
            rquickjs::qjs::JS_SetUncatchableError(
                self.ctx.as_raw().as_ptr(),
                e.as_value().as_raw(),
            );
        }
        Err(self.ctx.throw(e.into_value()))
    }

    /// Calls an exported function. Safe to use from inside a [`HostObjects`]
    /// callback (re-entrancy): it runs on the context that is already entered.
    pub fn call(&self, f: FnId, args: &[JsValue]) -> rquickjs::Result<JsValue> {
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
        args: &[JsValue],
    ) -> rquickjs::Result<JsValue> {
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
    pub fn call_ref(
        &self,
        f: &JsRef,
        this: &JsValue,
        args: &[JsValue],
    ) -> rquickjs::Result<JsValue> {
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
    /// object QQuickItem(0x…) is not a function`. A call of one of the
    /// names in `takes`, the function's own parameters, which `args` hold
    /// after `params`, says what it was given: `7 is not a function`. The
    /// function's values are gone once it has thrown, but its parameters
    /// are the caller's `args`; what is called through a local of its own,
    /// or a parameter it gives another value, keeps QuickJS's words. Only
    /// a call that threw pays for this. Gives the exception back, pending
    /// again.
    pub fn retell_failed_call(
        &self,
        file: &str,
        export: &str,
        params: &[&str],
        takes: &[&str],
        args: &[JsValue],
    ) -> rquickjs::Error {
        let thrown = self.ctx.catch();
        let _ = self.retell(&thrown, file, export, params, takes, args);
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
        takes: &[&str],
        args: &[JsValue],
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
        let lines = |l| text(module, l);
        // What a name of the function's own holds: one of `params`, one of
        // `takes` it has not given another value, or a local it declared
        // as a path of those and has not given another value since.
        let value_of = |path: &[String]| -> Option<Value<'js>> {
            let (root, rest) = path.split_first()?;
            let mut v = if let Some(at) = params.iter().position(|p| *p == root.as_str()) {
                self.to_value(args.get(at)?).ok()?
            } else {
                let at = takes.iter().position(|p| *p == root.as_str())?;
                if rebound_before(&lines, export, line, col, root)? {
                    return None;
                }
                // A parameter the caller left out holds undefined.
                match args.get(params.len() + at) {
                    Some(a) => self.to_value(a).ok()?,
                    None => Value::new_undefined(self.ctx.clone()),
                }
            };
            for m in rest {
                v = v.as_object()?.get(m.as_str()).ok()?;
            }
            Some(v)
        };
        let through_local = |name: &str| -> Option<Value<'js>> {
            let chain = local_alias(&lines, export, line, col, name)?;
            // A local of a local is not followed.
            if !params.contains(&chain[0].as_str()) && !takes.contains(&chain[0].as_str()) {
                return None;
            }
            value_of(&chain)
        };
        let shown = |v: Value<'js>| {
            v.get::<rquickjs::convert::Coerced<String>>()
                .ok()
                .map(|c| c.0)
        };
        if path.is_empty() {
            let v = value_of(std::slice::from_ref(&prop)).or_else(|| through_local(&prop))?;
            return e
                .as_object()
                .set("message", format!("{} is not a function", shown(v)?))
                .ok();
        }
        let mut v = value_of(&path[..1]).or_else(|| through_local(&path[0]))?;
        for m in &path[1..] {
            v = v.as_object()?.get(m.as_str()).ok()?;
        }
        e.as_object()
            .set(
                "message",
                format!(
                    "Property '{prop}' of object {} is not a function",
                    shown(v)?
                ),
            )
            .ok()
    }

    fn invoke(
        &self,
        func: &Function<'js>,
        this: &JsValue,
        args: &[JsValue],
    ) -> rquickjs::Result<JsValue> {
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
                if let Some(kind) = r.kind()
                    && let Ok(made) = self
                        .ctx
                        .globals()
                        .get::<_, rquickjs::Function<'js>>("__ljos_arrayMade")
                {
                    return made.call::<_, Value<'js>>((arr, kind));
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
        let Some((o, len)) = array_like(&v)? else {
            return Ok(Vec::new());
        };
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            out.push(self.read_value_at(&o.get::<_, Value>(i as u32)?, 1)?);
        }
        Ok(out)
    }

    /// How many entries the array behind `r` has now.
    pub fn array_len(&self, r: &JsRef) -> rquickjs::Result<usize> {
        if let Some(len) = r.with_pending(<[JsValue]>::len) {
            return Ok(len);
        }
        let v = self.restore(r)?;
        Ok(array_like(&v)?.map_or(0, |(_, len)| len))
    }

    /// One entry of the array behind `r`, as [`Scope::array_items`] gives
    /// it; `undefined` past the end.
    pub fn array_entry(&self, r: &JsRef, index: usize) -> rquickjs::Result<JsValue> {
        if let Some(entry) = r.with_pending(|items| items.get(index).cloned()) {
            return Ok(entry.unwrap_or(JsValue::Undefined));
        }
        let v = self.restore(r)?;
        let Some((o, len)) = array_like(&v)? else {
            return Ok(JsValue::Undefined);
        };
        if index >= len {
            return Ok(JsValue::Undefined);
        }
        let item: Value = o.get(index as u32)?;
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
        let props = o.props::<String, Value>();
        let mut out = Vec::with_capacity(props.size_hint().0);
        for kv in props {
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
        self.copied_at(v, &mut Vec::new())
    }

    /// `within` holds the addresses of the arrays and objects being copied
    /// round `v`: one of them met again is a cycle, and from there the copy
    /// shares it rather than copying it again.
    fn copied_at(&self, v: &JsValue, within: &mut Vec<usize>) -> rquickjs::Result<JsValue> {
        // Rust-side entries have no cycles, but a copy this deep would sooner
        // run out of stack than end.
        if within.len() > 64 {
            return Ok(v.clone());
        }
        let each = |items: &[JsValue], within: &mut Vec<usize>| {
            items
                .iter()
                .map(|i| self.copied_at(i, within))
                .collect::<rquickjs::Result<Vec<_>>>()
        };
        Ok(match v {
            JsValue::Array(items) => JsValue::Array(each(items, within)?),
            JsValue::Typed(kind, fields) => JsValue::Typed(*kind, each(fields, within)?),
            JsValue::Handle(r) => {
                if let Some(items) = r.pending_items() {
                    return Ok(JsValue::Array(each(&items, within)?));
                }
                let value = self.restore(r)?;
                if value.is_proxy() {
                    return Ok(v.clone());
                }
                let Some(address) = object_address(&value) else {
                    return Ok(v.clone());
                };
                if within.contains(&address) {
                    return Ok(v.clone());
                }
                if value.is_array() {
                    let items = self.array_items(r)?;
                    within.push(address);
                    let out = each(&items, within);
                    within.pop();
                    return Ok(JsValue::Array(out?));
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
                within.push(address);
                let fields = self.copied_fields(o, within);
                within.pop();
                JsValue::Handle(JsRef::object(fields?))
            }
            other => other.clone(),
        })
    }

    fn copied_fields(
        &self,
        o: &Object<'js>,
        within: &mut Vec<usize>,
    ) -> rquickjs::Result<Vec<(String, JsValue)>> {
        let mut fields = Vec::new();
        for kv in o.props::<String, Value>() {
            let (k, f) = kv?;
            // A field is read as it would cross on its own: an object in it
            // is a handle, which the copy then follows with `within`.
            let f = self.from_value(&f)?;
            fields.push((k, self.copied_at(&f, within)?));
        }
        Ok(fields)
    }

    /// `Number(value)`: an object is asked its `valueOf`, so a Date is its
    /// time and an object with no number in it is NaN.
    pub fn number(&self, v: &JsValue) -> f64 {
        self.to_value(v)
            .and_then(|v| v.get::<rquickjs::convert::Coerced<f64>>())
            .map_or(f64::NAN, |c| c.0)
    }

    /// `String(value)` for a Date, None for any other value.
    pub fn date_text(&self, r: &JsRef) -> Option<String> {
        if !self.is_date(r) {
            return None;
        }
        let v = self.restore(r).ok()?;
        v.get::<rquickjs::convert::Coerced<String>>()
            .ok()
            .map(|c| c.0)
    }

    /// Whether `r` is a Date.
    pub fn is_date(&self, r: &JsRef) -> bool {
        let (Ok(v), Ok(date)) = (
            self.restore(r),
            self.ctx.globals().get::<_, Value<'js>>("Date"),
        ) else {
            return false;
        };
        v.as_object().is_some_and(|o| o.is_instance_of(&date))
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
        let cached = self
            .state
            .wrappers
            .borrow()
            .get(&obj)
            .map(|k| k.held.clone());
        if let Some(p) = cached {
            return p.restore(&self.ctx);
        }
        // Its collection is told too: only once the Engine dropped it
        // ([`JsHost::drop_wrapper`]) and no script held it any more.
        let obj_handle = self.instantiate(obj, Some(self.state.released.clone()))?;
        let kept = Kept {
            raw: obj_handle.as_raw(),
            held: Persistent::save(&self.ctx, obj_handle.clone()),
        };
        self.state.wrappers.borrow_mut().insert(obj, kept);
        Ok(obj_handle)
    }

    /// What [`to_value`](Self::to_value) makes of `v`, as a raw value that
    /// holds its own reference. A text or an object that has its wrapper
    /// is made without a `Value`, which would take the context and give
    /// it back.
    fn to_raw(&self, v: &JsValue) -> rquickjs::Result<rquickjs::qjs::JSValue> {
        use rquickjs::qjs;
        let ctx = self.ctx.as_raw().as_ptr();
        match v {
            JsValue::String(s) => {
                // SAFETY: a live context, and the bytes of a `str`.
                unsafe {
                    let raw = qjs::JS_NewStringLen(ctx, s.as_ptr().cast(), s.len() as _);
                    if qjs::JS_IsException(raw) {
                        return Err(rquickjs::Error::Exception);
                    }
                    Ok(raw)
                }
            }
            JsValue::Object(id) if !self.state.owned.borrow().contains_key(id) => {
                let raw = self.state.wrappers.borrow().get(id).map(|k| k.raw);
                match raw {
                    // SAFETY: the map holds a reference to it.
                    Some(raw) => Ok(unsafe { qjs::JS_DupValue(ctx, raw) }),
                    None => Ok(owned_raw(self.wrapper(*id)?.into_value())),
                }
            }
            _ => Ok(owned_raw(self.to_value(v)?)),
        }
    }

    /// A wrapper owned by JS: it is not cached, and when the collector frees
    /// it [`HostObjects::released`] runs (between runs). For objects created
    /// by script, such as `Component.createObject`.
    pub fn new_owned(&self, obj: u64) -> rquickjs::Result<Object<'js>> {
        let q = self.state.released.clone();
        self.instantiate(obj, Some(q))
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
    ) -> rquickjs::Result<Object<'js>> {
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
        let listing = self
            .state
            .listings
            .borrow()
            .get(ty.0 as usize)
            .cloned()
            .flatten();
        let w = HostWrapper {
            id: obj,
            release_queue,
            listing,
        };
        let raw = self.ctx.as_raw().as_ptr();
        // SAFETY: the context is entered and the class is its runtime's.
        let Some(o) = (unsafe { new_wrapper(raw, self.state.wrapper_class, proto.as_raw(), w) })
        else {
            return Err(rquickjs::Error::Exception);
        };
        // SAFETY: a new object, owned by the Value from here.
        let v = unsafe { Value::from_raw(self.ctx.clone(), o) };
        v.into_object()
            .ok_or_else(|| Exception::throw_type(&self.ctx, "not an object"))
    }

    pub fn to_value(&self, v: &JsValue) -> rquickjs::Result<Value<'js>> {
        let ctx = &self.ctx;
        Ok(match v {
            JsValue::Undefined => Value::new_undefined(ctx.clone()),
            JsValue::Null => Value::new_null(ctx.clone()),
            JsValue::Bool(b) => Value::new_bool(ctx.clone(), *b),
            // `new_number` makes -0 the int 0, which has no sign.
            JsValue::Number(n) if *n == 0.0 && n.is_sign_negative() => {
                Value::new_float(ctx.clone(), *n)
            }
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
            JsValue::LiveList { obj, prop } => self.live_list(*obj, *prop)?,
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
                if let Some(o) = self.recent_object(kind.index(), fields)? {
                    return Ok(o.into_value());
                }
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
                let made: Value<'js> = make.restore(ctx)?.call((Rest(args),))?;
                if let Some(o) = made.as_object() {
                    self.keep_typed(kind.index(), o, fields)?;
                }
                made
            }
        })
    }

    /// The fields of `o`, whose prototype is at `proto_at`, when it is a
    /// value of a defined kind.
    fn read_typed(
        &self,
        o: &Object<'js>,
        proto_at: usize,
        depth: u32,
    ) -> rquickjs::Result<Option<JsValue>> {
        let known = {
            let kinds = self.state.kinds.borrow();
            kinds
                .iter()
                .rposition(|k| k.proto_at == proto_at)
                .map(|i| (i, kinds[i].fields.clone()))
        };
        let Some((index, names)) = known else {
            return Ok(None);
        };
        if let Some(fields) = self.recent_fields(index, o)? {
            return Ok(Some(JsValue::Typed(ValueKind(index as u32), fields)));
        }
        let mut fields = Vec::with_capacity(names.len());
        for name in names.iter() {
            let v: Value<'js> = o.get(name.as_str())?;
            // Script can build an object on the prototype by hand; without
            // the fields it is just an object.
            if v.is_undefined() {
                return Ok(None);
            }
            fields.push(self.read_value_at(&v, depth + 1)?);
        }
        self.keep_typed(index, o, &fields)?;
        Ok(Some(JsValue::Typed(ValueKind(index as u32), fields)))
    }

    /// The object of a value of kind `kind` with `fields` that crossed
    /// lately ([`KindFace::recent`]).
    fn recent_object(
        &self,
        kind: usize,
        fields: &[JsValue],
    ) -> rquickjs::Result<Option<Object<'js>>> {
        let kinds = self.state.kinds.borrow();
        let Some(face) = kinds.get(kind) else {
            return Ok(None);
        };
        let mut recent = face.recent.borrow_mut();
        // A number by its bits: `-0` is not `0` to a kind that keeps it.
        let same = |a: &JsValue, b: &JsValue| match (a, b) {
            (JsValue::Number(x), JsValue::Number(y)) => x.to_bits() == y.to_bits(),
            _ => a == b,
        };
        let Some(at) = recent.iter().position(|(_, f)| {
            f.len() == fields.len() && f.iter().zip(fields).all(|(a, b)| same(a, b))
        }) else {
            return Ok(None);
        };
        recent[..=at].rotate_right(1);
        recent[0].0.clone().restore(&self.ctx).map(Some)
    }

    /// The fields of `o`, a value of kind `kind`, when it crossed lately.
    fn recent_fields(
        &self,
        kind: usize,
        o: &Object<'js>,
    ) -> rquickjs::Result<Option<Vec<JsValue>>> {
        let kinds = self.state.kinds.borrow();
        let Some(face) = kinds.get(kind) else {
            return Ok(None);
        };
        let mut recent = face.recent.borrow_mut();
        let mut found = None;
        for (i, (kept, _)) in recent.iter().enumerate() {
            if kept.clone().restore(&self.ctx)? == *o {
                found = Some(i);
                break;
            }
        }
        let Some(at) = found else {
            return Ok(None);
        };
        recent[..=at].rotate_right(1);
        Ok(Some(recent[0].1.clone()))
    }

    /// Keeps `o` as a value of kind `kind` that crossed lately, if it is
    /// frozen.
    fn keep_typed(&self, kind: usize, o: &Object<'js>, fields: &[JsValue]) -> rquickjs::Result<()> {
        let is_frozen = {
            let kinds = self.state.kinds.borrow();
            let Some(face) = kinds.get(kind) else {
                return Ok(());
            };
            face.is_frozen.clone()
        };
        let frozen: bool = is_frozen.restore(&self.ctx)?.call((o.clone(),))?;
        if !frozen {
            return Ok(());
        }
        let kinds = self.state.kinds.borrow();
        let Some(face) = kinds.get(kind) else {
            return Ok(());
        };
        let mut recent = face.recent.borrow_mut();
        if recent.len() == RECENT_VALUES {
            recent.pop();
        }
        recent.insert(0, (Persistent::save(&self.ctx, o.clone()), fields.to_vec()));
        Ok(())
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
            proto_at: object_address(proto.as_value()).expect("an object"),
        });
        Ok((make, proto))
    }

    /// `o`, whose prototype is at `proto_at`, when it is a colour object.
    fn read_color(&self, o: &Object<'js>, proto_at: usize) -> rquickjs::Result<Option<JsValue>> {
        if self.state.color.borrow().as_ref().map(|f| f.proto_at) != Some(proto_at) {
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

    /// `o` when its prototype makes it a colour or a value of a defined
    /// kind. The prototype is compared by address: an object made by a
    /// literal is told from those without asking JS anything.
    fn read_by_prototype(&self, o: &Object<'js>, depth: u32) -> rquickjs::Result<Option<JsValue>> {
        let Some(proto_at) = o.get_prototype().and_then(|p| object_address(p.as_value())) else {
            return Ok(None);
        };
        if let Some(c) = self.read_color(o, proto_at)? {
            return Ok(Some(c));
        }
        self.read_typed(o, proto_at, depth)
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

    /// What a script gave in an argument a method takes by copy
    /// ([`MethodDef::copied`]), as Qt turns a script's value into a
    /// variant: an ordinary object is a [`JsValue::Map`] of its own
    /// enumerable fields and an array a [`JsValue::Array`], each read the
    /// same way, all the way down; a Date is a new Date of the same time,
    /// and a function is `undefined`. Anything else, and what is nested
    /// deeper than a cycle would go, crosses as any argument does.
    pub fn copy_of(&self, v: &Value<'js>) -> rquickjs::Result<JsValue> {
        self.copy_at(v, 0)
    }

    fn copy_at(&self, v: &Value<'js>, depth: u32) -> rquickjs::Result<JsValue> {
        use rquickjs::qjs;
        if !v.is_object() || depth > COPY_DEPTH {
            return self.script_value(v);
        }
        let raw = v.as_raw();
        let ctx = self.ctx.as_raw().as_ptr();
        // The class says what the object is in one look; asking rquickjs
        // for its type asks the engine half a dozen things.
        // SAFETY: `raw` is the live object `v` holds; the class only reads
        // its header.
        match unsafe { qjs::JS_GetClassID(raw) } {
            JS_CLASS_OBJECT => {}
            JS_CLASS_ARRAY => {
                let arr = v.as_array().expect("an array");
                let mut out = Vec::with_capacity(arr.len());
                for item in arr.iter::<Value>() {
                    out.push(self.copy_at(&item?, depth + 1)?);
                }
                return Ok(JsValue::Array(out));
            }
            _ if v.is_proxy() => return self.script_value(v),
            // SAFETY: as above.
            _ if unsafe { qjs::JS_IsFunction(ctx, raw) } => return Ok(JsValue::Undefined),
            // SAFETY: as above.
            _ if unsafe { qjs::JS_IsDate(raw) } => {
                let time = v
                    .get::<rquickjs::convert::Coerced<f64>>()
                    .map_or(f64::NAN, |c| c.0);
                // SAFETY: the context is entered; the new Date is owned by
                // the Value made of it.
                let copy = unsafe { Value::from_raw(self.ctx.clone(), qjs::JS_NewDate(ctx, time)) };
                return Ok(JsValue::Handle(self.keep(copy)));
            }
            _ => return self.script_value(v),
        }
        // A literal's prototype is `Object.prototype`, which no colour or
        // defined kind has: only another prototype needs looking up.
        // SAFETY: the context is entered and `raw` is an ordinary object;
        // both prototypes are freed once compared.
        let plain = unsafe {
            let proto = qjs::JS_GetPrototype(ctx, raw);
            let object = qjs::JS_GetClassProto(ctx, JS_CLASS_OBJECT);
            let same = qjs::JS_VALUE_GET_TAG(proto) == qjs::JS_TAG_OBJECT
                && qjs::JS_VALUE_GET_PTR(proto) == qjs::JS_VALUE_GET_PTR(object);
            qjs::JS_FreeValue(ctx, proto);
            qjs::JS_FreeValue(ctx, object);
            same
        };
        if !plain
            && let Some(read) = self.read_by_prototype(v.as_object().expect("object"), depth)?
        {
            return Ok(read);
        }
        // Its own enumerable string keys, read straight off the engine:
        // each key is made a string once, and each field is one read (a
        // getter runs once), in the order a script sees them.
        let flags = (qjs::JS_GPN_STRING_MASK | qjs::JS_GPN_ENUM_ONLY) as std::ffi::c_int;
        let mut names = OwnNames {
            ctx,
            tab: std::ptr::null_mut(),
            len: 0,
        };
        // SAFETY: the context is entered and `raw` is live; `names` frees
        // what is filled in.
        if unsafe { qjs::JS_GetOwnPropertyNames(ctx, &mut names.tab, &mut names.len, raw, flags) }
            < 0
        {
            return Err(rquickjs::Error::Exception);
        }
        let mut fields = Vec::with_capacity(names.len as usize);
        for i in 0..names.len as usize {
            // SAFETY: `i` is within the table the engine filled in.
            let atom = unsafe { (*names.tab.add(i)).atom };
            let mut n = 0;
            // SAFETY: as above; the C string is freed once copied.
            let key = unsafe {
                let p = qjs::JS_AtomToCStringLen(ctx, &mut n, atom);
                if p.is_null() {
                    return Err(rquickjs::Error::Exception);
                }
                let bytes = std::slice::from_raw_parts(p.cast(), n as usize);
                // A lone surrogate in a key is the one thing that is not
                // UTF-8; the check alone costs less than the lossy read.
                let key = match std::str::from_utf8(bytes) {
                    Ok(key) => key.to_owned(),
                    Err(_) => String::from_utf8_lossy(bytes).into_owned(),
                };
                qjs::JS_FreeCString(ctx, p);
                key
            };
            // SAFETY: as above; the field's value is owned by `f`.
            let f = unsafe {
                let got = qjs::JS_GetProperty(ctx, raw, atom);
                if qjs::JS_IsException(got) {
                    return Err(rquickjs::Error::Exception);
                }
                Value::from_raw(self.ctx.clone(), got)
            };
            fields.push((key, self.copy_at(&f, depth + 1)?));
        }
        Ok(JsValue::Map(fields))
    }

    fn read_value_at(&self, v: &Value<'js>, depth: u32) -> rquickjs::Result<JsValue> {
        // Cyclic arrays would otherwise recurse until the native stack limit.
        if depth > 32 {
            return Err(Exception::throw_type(&self.ctx, "value nested too deeply"));
        }
        // Told by the tag alone, before `type_of` asks an object what it is.
        if let Some(n) = v.as_number() {
            return Ok(JsValue::Number(n));
        }
        if let Some(b) = v.as_bool() {
            return Ok(JsValue::Bool(b));
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
                // SAFETY: the class is this runtime's.
                match unsafe { wrapper_id(self.state.wrapper_class, o.as_raw()) } {
                    Some(id) => JsValue::Object(id),
                    None => match self.live_list_of(v) {
                        Some(list) => list,
                        None => match self.read_by_prototype(o, depth)? {
                            Some(read) => read,
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

/// How deep [`Scope::copy_of`] copies: past it, a cycle is the script's
/// own object again.
const COPY_DEPTH: u32 = 32;

/// The own keys `JS_GetOwnPropertyNames` gave, freed with it.
struct OwnNames {
    ctx: *mut rquickjs::qjs::JSContext,
    tab: *mut rquickjs::qjs::JSPropertyEnum,
    len: u32,
}

impl Drop for OwnNames {
    fn drop(&mut self) {
        if !self.tab.is_null() {
            // SAFETY: the table and its atoms came from this context.
            unsafe { rquickjs::qjs::JS_FreePropertyEnum(self.ctx, self.tab, self.len) };
        }
    }
}

/// QuickJS's class of an ordinary object (`JS_CLASS_OBJECT`, which its
/// header keeps first).
const JS_CLASS_OBJECT: rquickjs::qjs::JSClassID = 1;

/// QuickJS's class of an array, next after [`JS_CLASS_OBJECT`].
const JS_CLASS_ARRAY: rquickjs::qjs::JSClassID = 2;

/// The array behind `v`, an array's reference, and its length. A proxy
/// standing for one (a host's kind of array: see [`JsRef::array_of_kind`])
/// is read through its traps, as a script reads it: QuickJS does not say a
/// proxy is an array here.
fn array_like<'js>(v: &Value<'js>) -> rquickjs::Result<Option<(Object<'js>, usize)>> {
    if let Some(arr) = v.as_array() {
        let len = arr.len();
        return Ok(Some((arr.clone().into_object(), len)));
    }
    match v.as_object() {
        Some(o) if v.is_proxy() => {
            let len: Value = o.get("length")?;
            Ok(len.as_number().map(|len| (o.clone(), len as usize)))
        }
        _ => Ok(None),
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
    heap: HeapCount,
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
        // SAFETY: the opaque is the `Weak` JsHost::new put there, or null.
        unsafe {
            let p = rquickjs::qjs::JS_GetContextOpaque(self.raw.as_ptr());
            rquickjs::qjs::JS_SetContextOpaque(self.raw.as_ptr(), std::ptr::null_mut());
            if !p.is_null() {
                drop(Weak::from_raw(p.cast::<HostState>().cast_const()));
            }
        }
        self.state.fns.borrow_mut().clear();
        self.state.types.borrow_mut().clear();
        self.state.listings.borrow_mut().clear();
        self.state.wrappers.borrow_mut().clear();
        self.state.handles.borrow_mut().clear();
        self.state.by_object.borrow_mut().clear();
        self.state.color.borrow_mut().take();
        self.state.owned_face.borrow_mut().take();
        self.state.kinds.borrow_mut().clear();
        self.state.globals.borrow_mut().clear();
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
        let heap = HeapCount::default();
        let rt = Runtime::new_with_alloc(Counted::new(heap.clone()))?;
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
            rt.set_interrupt_handler(Some(Box::new(move || match st.due.get() {
                Some(d) if st.now(Timing::Poll) >= d => {
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
        // SAFETY: the runtime is alive.
        let wrapper_class = unsafe {
            register_wrapper_class(rquickjs::qjs::JS_GetRuntime(raw.as_ptr()), &answer::EXOTIC)
        }
        .ok_or(rquickjs::Error::Allocation)?;
        // SAFETY: as above.
        let list_class = unsafe { live_list::register(rquickjs::qjs::JS_GetRuntime(raw.as_ptr())) }
            .ok_or(rquickjs::Error::Allocation)?;
        // SAFETY: as above.
        let signal_class =
            unsafe { signal_fn::register(rquickjs::qjs::JS_GetRuntime(raw.as_ptr())) }
                .ok_or(rquickjs::Error::Allocation)?;
        // One of the atoms QuickJS is made with, so it is never freed.
        // SAFETY: the context is alive.
        let length_atom = unsafe { rquickjs::qjs::JS_NewAtom(raw.as_ptr(), c"length".as_ptr()) };
        let state = Rc::new(HostState {
            fns: RefCell::default(),
            types: RefCell::default(),
            listings: RefCell::default(),
            wrappers: RefCell::default(),
            host: RefCell::default(),
            released: Rc::default(),
            owned: RefCell::default(),
            owned_face: RefCell::default(),
            mapper: RefCell::default(),
            text: RefCell::default(),
            entered: Cell::new(false),
            stopped: Cell::new(false),
            handles: RefCell::default(),
            by_object: RefCell::default(),
            arrays_by_reference: Cell::new(false),
            next_handle: Cell::new(1),
            handle_release: Arc::default(),
            color: RefCell::default(),
            kinds: RefCell::default(),
            panicked: RefCell::default(),
            globals: RefCell::default(),
            wrapper_class,
            list_class,
            list_proto: RefCell::default(),
            length_atom,
            signal_class,
            connect_installed: Cell::new(false),
        });
        // SAFETY: the context is alive; the raw accessors read it back as a
        // `Weak` (raw_state), and `Drop` takes it out before the context goes.
        unsafe {
            rquickjs::qjs::JS_SetContextOpaque(
                raw.as_ptr(),
                Weak::into_raw(Rc::downgrade(&state)).cast_mut().cast(),
            );
        }
        Ok(Self {
            state,
            raw,
            ctx,
            rt,
            interrupt,
            cfg,
            last_collect: Cell::new(0),
            heap,
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
    /// whose code is JS, or a new object's `Component.onCompleted`), it
    /// re-enters the context that is already locked with a slice of its
    /// own, and the time it took is given back to the runs around it; the
    /// dispatch as a whole is held to a few slices (ARCHITECTURE.md
    /// section 6.1). A failure inside the nested run is classified and its
    /// exception drained, so the outer JS continues, unless it was
    /// interrupted: then the dispatch is too.
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
            return self.enter_nested(label, what, slice, f);
        }
        self.state.entered.set(true);
        let guard = EnterGuard(&self.state.entered);
        self.state.drain_handles();
        self.interrupt.fired.set(false);
        self.interrupt.stopped_in.take();
        self.state.stopped.set(false);
        let start = self.interrupt.now(Timing::Enter);
        self.interrupt.arm(start, Some(slice));

        let outcome = catch_unwind(AssertUnwindSafe(|| {
            self.ctx.with(|ctx| {
                let scope = Scope {
                    ctx: ctx.clone(),
                    state: self.state.clone(),
                };
                let r = f(&scope);
                if let Some(panic) = self.state.panicked.borrow_mut().take() {
                    std::panic::resume_unwind(panic);
                }
                match r {
                    Ok(v) => Ok(v),
                    Err(e) => {
                        let after = self.interrupt.now(Timing::Leave) - start;
                        Err(self.classify(&scope, e, label, after))
                    }
                }
            })
        }));

        let elapsed = self.interrupt.now(Timing::Leave) - start;
        let native = self.interrupt.native.get();
        self.interrupt.arm(start, None);
        self.state.stopped.set(false);
        drop(guard);

        let soft = slice.soft * UNOPTIMISED;
        let over = elapsed.saturating_sub(native) > soft;
        let stopped = matches!(outcome, Ok(Err(JsFault::JsInterrupted { .. })));
        let what = if over || stopped {
            what()
        } else {
            String::new()
        };
        let outcome = match outcome {
            Ok(Err(JsFault::JsInterrupted { label, after, .. })) => {
                let mut said = if what.is_empty() {
                    String::new()
                } else {
                    format!(" at {what}")
                };
                if let Some(inner) = self.interrupt.stopped_in.take() {
                    said.push_str(&format!(", in {inner}"));
                }
                if !native.is_zero() {
                    said.push_str(&format!(", {native:?} of it native work given back"));
                }
                Ok(Err(JsFault::JsInterrupted {
                    label,
                    after,
                    what: said,
                }))
            }
            other => other,
        };
        if over {
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
        what: impl FnOnce() -> String,
        slice: Slice,
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
        let st = &self.interrupt;
        let start = st.now(Timing::Enter);
        let before = st.given.get();
        if let Some(d) = st.deadline.get() {
            st.outer.borrow_mut().push(d);
        }
        st.deadline.set(Some(start + slice.hard));
        st.settle();
        let r = f(&scope);
        let took = st.now(Timing::Leave) - start;
        st.deadline.set(st.outer.borrow_mut().pop());
        st.give_back(took, before, false);
        let fault = match r {
            Ok(v) => return Ok(v),
            Err(e) => self.classify(&scope, e, label, took),
        };
        if matches!(fault, JsFault::JsInterrupted { .. }) {
            self.state.stopped.set(true);
            let mut inner = st.stopped_in.borrow_mut();
            if inner.is_none() {
                let what = what();
                *inner = Some(if what.is_empty() {
                    format!("`{label}`")
                } else {
                    format!("`{label}` at {what}")
                });
            }
        }
        Err(fault)
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
    pub fn call_ref(
        &self,
        f: &JsRef,
        this: &JsValue,
        args: &[JsValue],
    ) -> Result<JsValue, JsFault> {
        self.enter("call_ref", self.cfg.slice, |scope| {
            scope.call_ref(f, this, args)
        })
    }

    /// Evaluates a classic script (the Engine's global prelude).
    pub fn eval_script(&self, name: &str, source: &str) -> Result<JsValue, JsFault> {
        let ahead = crate::ahead::script(name, source);
        self.enter(&format!("eval {name}"), self.cfg.load_slice, |scope| {
            let v: Value = match &ahead {
                Some(script) => script.eval(&scope.ctx)?,
                None => scope.ctx.eval(source)?,
            };
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
            let fields: Rc<[String]> = fields.into();
            let object: Object = scope.ctx.globals().get("Object")?;
            let is_frozen: Function = object.get("isFrozen")?;
            let index = scope.state.kinds.borrow().len() as u32;
            let proto_at = object_address(proto.as_value()).expect("an object");
            scope.state.kinds.borrow_mut().push(KindFace {
                make: Persistent::save(&scope.ctx, make),
                proto: Persistent::save(&scope.ctx, proto),
                proto_at,
                fields,
                is_frozen: Persistent::save(&scope.ctx, is_frozen),
                recent: RefCell::default(),
            });
            Ok(ValueKind(index))
        })
    }

    /// Installs `f` as a global function `name`. Arguments and the result
    /// cross as [`JsValue`]s; an error becomes a thrown JS `Error`.
    pub fn set_global_function(&self, name: &str, f: HostFn) -> Result<(), JsFault> {
        self.enter("set_global_function", self.cfg.load_slice, |scope| {
            let func = make_global(scope, f.clone(), name)?;
            scope.ctx.globals().set(name, func)
        })
    }

    /// Calls an exported function under the default slice.
    pub fn call(&self, f: FnId, args: &[JsValue]) -> Result<JsValue, JsFault> {
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
        let base = def.base.map(|b| {
            let at = b.0 as usize;
            let proto = self.state.types.borrow().get(at).cloned().flatten();
            let listing = self.state.listings.borrow().get(at).cloned().flatten();
            proto.zip(listing)
        });
        let mut listing = Listing {
            closed: !def.plain,
            ..Listing::default()
        };
        let proto = self.enter("register_type", self.cfg.load_slice, |scope| {
            let proto = Object::new(scope.ctx.clone())?;
            let weak = Rc::downgrade(&scope.state);
            let ctx = &scope.ctx;
            if !scope.state.connect_installed.replace(true) {
                signal_fn::install_connect(ctx)?;
            }
            // A base's entries are its type's too. They are listed first,
            // as Qt lists what a base has before what a type adds; the
            // accessors are the base's, not made again.
            let base = match base {
                None => None,
                Some(None) => {
                    return Err(rquickjs::Exception::throw_message(ctx, "stale type handle"));
                }
                Some(Some((p, l))) => Some((p.restore(ctx)?, l)),
            };
            let mut own_props: Vec<Option<&PropDef>> = def.props.iter().map(Some).collect();
            let mut own_signals: Vec<Option<&SignalDef>> = def.signals.iter().map(Some).collect();
            let mut own_methods: Vec<Option<&MethodDef>> = def.methods.iter().map(Some).collect();
            fn take<'a, T>(
                list: &mut [Option<&'a T>],
                name: &str,
                of: impl Fn(&T) -> &str,
            ) -> Option<&'a T> {
                list.iter_mut()
                    .find(|e| e.is_some_and(|e| of(e) == name))?
                    .take()
            }
            // The base's slots are for the entries copied from it: the
            // same atoms, kept by this prototype too.
            let own = |p: &PropDef, listing: &mut Listing| -> rquickjs::Result<()> {
                let atom = install_prop(&proto, p)?;
                let slot = Slot {
                    prop: p.id,
                    writable: p.writable,
                };
                listing.slots.insert(atom, slot);
                Ok(())
            };
            if let Some((from, names)) = &base {
                listing.slots = names.slots.clone();
                for name in &names.props {
                    match take(&mut own_props, name, |p| p.name.as_str()) {
                        Some(p) => own(p, &mut listing)?,
                        None => copy_entry(ctx, from, &proto, name)?,
                    }
                    listing.props.push(name.clone());
                }
            }
            for p in own_props.into_iter().flatten() {
                own(p, &mut listing)?;
                listing.props.push(p.name.clone());
            }
            if let Some((from, names)) = &base {
                for name in &names.members {
                    if let Some(sig) = take(&mut own_signals, name, |s| s.name.as_str()) {
                        signal_fn::install(&proto, sig)?;
                    } else if let Some(m) = take(&mut own_methods, name, |m| m.name.as_str()) {
                        install_method(&proto, weak.clone(), m)?;
                    } else {
                        copy_entry(ctx, from, &proto, name)?;
                    }
                    listing.members.push(name.clone());
                }
            }
            for sig in own_signals.into_iter().flatten() {
                signal_fn::install(&proto, sig)?;
                listing.members.push(sig.name.clone());
            }
            for m in own_methods.into_iter().flatten() {
                install_method(&proto, weak.clone(), m)?;
                listing.members.push(m.name.clone());
            }
            // A signal or method of a property's name replaced it on the
            // prototype, as a QML method named `state` hides Item's: the
            // wrapper must not answer for it.
            let raw = ctx.as_raw().as_ptr();
            for name in &listing.members {
                // SAFETY: a live context; the atom is freed after the look.
                unsafe {
                    let atom =
                        rquickjs::qjs::JS_NewAtomLen(raw, name.as_ptr().cast(), name.len() as _);
                    listing.slots.remove(&atom);
                    rquickjs::qjs::JS_FreeAtom(raw, atom);
                }
            }
            if !def.plain && !def.name.is_empty() {
                install_to_string(ctx, &proto, def.name.clone())?;
            }
            if !def.plain {
                install_to_json(ctx, &proto)?;
            }
            Ok(Persistent::save(ctx, proto))
        })?;
        let listing = Some(Rc::new(listing));
        let mut listings = self.state.listings.borrow_mut();
        let mut types = self.state.types.borrow_mut();
        let slot = match types.iter().position(Option::is_none) {
            Some(free) => {
                types[free] = Some(proto);
                listings[free] = listing;
                free
            }
            None => {
                types.push(Some(proto));
                listings.resize(types.len() - 1, None);
                listings.push(listing);
                types.len() - 1
            }
        };
        Ok(HostTypeId(
            u32::try_from(slot).expect("type count fits u32"),
        ))
    }

    /// Gives the type `id`, registered with no name, the name an object of
    /// it prints with.
    pub fn name_type(&self, id: HostTypeId, name: &str) -> Result<(), JsFault> {
        let proto = self
            .state
            .types
            .borrow()
            .get(id.0 as usize)
            .cloned()
            .flatten();
        let Some(proto) = proto else {
            return Ok(());
        };
        let name = name.to_owned();
        self.enter("name_type", self.cfg.load_slice, |scope| {
            install_to_string(&scope.ctx, &proto.restore(&scope.ctx)?, name)
        })
    }

    /// Lets go of the prototype of `id`, for a type no object will have
    /// again (one a hot reload replaced). Wrappers made with it keep it;
    /// the id may be given to a type registered later.
    pub fn unregister_type(&self, id: HostTypeId) {
        if let Some(slot) = self.state.types.borrow_mut().get_mut(id.0 as usize) {
            *slot = None;
        }
        if let Some(slot) = self.state.listings.borrow_mut().get_mut(id.0 as usize) {
            *slot = None;
        }
    }

    /// Forgets the cached wrapper of `obj`. JS that still holds it keeps a
    /// dangling wrapper whose accessors fail with "object no longer exists";
    /// the next `wrapper(obj)` makes a fresh one. Call it when the Engine
    /// destroys the object. [`HostObjects::released`] says when the last
    /// script let go of it.
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

    /// Whether a script may hold `obj`: it has a wrapper, cached or
    /// script-owned.
    pub fn has_wrapper(&self, obj: u64) -> bool {
        self.state.wrappers.borrow().contains_key(&obj)
            || self.state.owned.borrow().contains_key(&obj)
    }

    pub fn wrapper_count(&self) -> usize {
        self.state.wrappers.borrow().len()
    }

    /// How many times a script on this thread has called a property's
    /// accessor function rather than had its wrapper answer
    /// (host/answer.rs): a read through the prototype itself, or through
    /// an object made over a wrapper.
    pub fn accessor_calls() -> u64 {
        ACCESSOR_CALLS.get()
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
        self.state.stopped.set(false);
        // A job's deadline is the pump's, and so is the ceiling: nested runs
        // and native work give time back, but never past `until`.
        let now = self.interrupt.now(Timing::Enter);
        self.interrupt.arm(now, None);
        self.interrupt.deadline.set(Some(until));
        self.interrupt.ceiling.set(Some(until));
        self.interrupt.settle();
        let mut n = 0;
        while self.interrupt.now(Timing::Poll) < until {
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
        self.interrupt.arm(now, None);
        self.state.stopped.set(false);
        drop(guard);
        self.flush_released();
        n
    }

    /// Runs native work a script asked for (compiling a component, making
    /// objects): its time is not the script's, and is given back to every
    /// run of the dispatch. JavaScript it runs itself (a new object's
    /// `Component.onCompleted`) is a nested run, with a slice of its own.
    /// Outside a dispatch it only runs `f`.
    pub fn native<R>(&self, f: impl FnOnce() -> R) -> R {
        if !self.state.entered.get() {
            return f();
        }
        let st = &self.interrupt;
        let start = st.now(Timing::NativeBegin);
        let before = st.given.get();
        let r = f();
        let took = st.now(Timing::NativeEnd) - start;
        st.give_back(took, before, true);
        r
    }

    /// Sets the clock the limits on a run read; `None` is the wall clock.
    pub fn set_clock(&self, clock: Option<JsClock>) {
        *self.interrupt.clock.borrow_mut() = clock;
    }

    /// Whether a run nested in the current dispatch was interrupted, which
    /// ends the dispatch: what that run did not give is not its own failure.
    pub fn interrupted(&self) -> bool {
        self.state.stopped.get()
    }

    pub fn has_pending_jobs(&self) -> bool {
        !self.state.entered.get() && self.rt.is_job_pending()
    }

    /// Runs the cycle collector if the heap grew enough since the last
    /// collection. Meant for the Idle lane; returns whether it ran.
    pub fn collect_if_idle(&self) -> bool {
        if !self.wants_idle_collect() {
            return false;
        }
        self.rt.run_gc();
        self.last_collect.set(self.heap_bytes());
        self.flush_released();
        true
    }

    /// Whether [`JsHost::collect_if_idle`] would collect now: outside a
    /// run, the heap grew by the configured growth since the collector
    /// last ran. Walks nothing, so a host may ask it each time it is
    /// about to rest, and wake later to collect only when it is true.
    pub fn wants_idle_collect(&self) -> bool {
        !self.state.entered.get()
            && self.heap_bytes()
                >= self
                    .last_collect
                    .get()
                    .saturating_add(self.cfg.idle_collect_min_growth)
    }

    /// What the runtime holds from the system allocator: blocks QuickJS
    /// carves small allocations from, and large ones whole. Unlike
    /// [`JsHost::memory_stats`] this walks nothing and may be asked inside
    /// a run.
    pub fn heap_bytes(&self) -> usize {
        self.heap.bytes()
    }

    /// Unconditional collection (memory pressure). Not available inside a run.
    pub fn collect_now(&self) -> bool {
        if self.state.entered.get() {
            return false;
        }
        self.rt.run_gc();
        self.last_collect.set(self.heap_bytes());
        self.flush_released();
        true
    }

    /// Whether JS is running: a native function called from a script, or
    /// what that calls.
    pub fn in_run(&self) -> bool {
        self.state.entered.get()
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
/// Gives `proto` the accessors of `p`, and the atom of its name, which
/// the prototype keeps.
fn install_prop(proto: &Object<'_>, p: &PropDef) -> rquickjs::Result<rquickjs::qjs::JSAtom> {
    use rquickjs::qjs;
    let ctx = proto.ctx().as_raw().as_ptr();
    // The id rides in the function's data, not its `magic`: QuickJS keeps
    // `magic` in 16 bits, and ids such as the parent's are far above that.
    // A number holds no reference, so its raw value outlives the Value.
    let mut id = Value::new_number(proto.ctx().clone(), f64::from(p.id)).as_raw();
    // SAFETY: the context is alive and entered; `JS_NewCFunctionData`
    // copies the data, `JS_DefinePropertyGetSet` takes both functions, and
    // the atom is freed after it.
    unsafe {
        let getter = qjs::JS_NewCFunctionData(ctx, Some(raw_get), 0, 0, 1, &raw mut id);
        let setter = if p.writable {
            qjs::JS_NewCFunctionData(ctx, Some(raw_set), 1, 0, 1, &raw mut id)
        } else {
            qjs::JS_UNDEFINED
        };
        let atom = qjs::JS_NewAtomLen(ctx, p.name.as_ptr().cast(), p.name.len() as _);
        let flags = (qjs::JS_PROP_CONFIGURABLE | qjs::JS_PROP_ENUMERABLE) as std::ffi::c_int;
        let r = qjs::JS_DefinePropertyGetSet(ctx, proto.as_raw(), atom, getter, setter, flags);
        qjs::JS_FreeAtom(ctx, atom);
        if r < 0 {
            return Err(rquickjs::Error::Exception);
        }
        Ok(atom)
    }
}

thread_local! {
    static ACCESSOR_CALLS: Cell<u64> = const { Cell::new(0) };
}

/// A property read: QuickJS calls it with the property's id as its data.
/// Plain C functions, not rquickjs closures: a read is the most frequent
/// thing a script does with an object, and the closure's argument
/// conversion, class check and panic guard cost more than the read.
unsafe extern "C" fn raw_get(
    ctx: *mut rquickjs::qjs::JSContext,
    this: rquickjs::qjs::JSValue,
    _argc: std::ffi::c_int,
    _argv: *mut rquickjs::qjs::JSValue,
    _magic: std::ffi::c_int,
    data: *mut rquickjs::qjs::JSValue,
) -> rquickjs::qjs::JSValue {
    ACCESSOR_CALLS.set(ACCESSOR_CALLS.get() + 1);
    // SAFETY: QuickJS calls it on a live context with the data it was made
    // with.
    unsafe { prop_get(ctx, this, None, prop_id(data)) }
}

/// Reads the property `prop` of the wrapper `this` ([`raw_get`], and
/// host/answer.rs), whose id is `known` when the caller has it.
///
/// # Safety
/// A live context; `known` is the id of `this`.
#[inline]
unsafe fn prop_get(
    ctx: *mut rquickjs::qjs::JSContext,
    this: rquickjs::qjs::JSValue,
    known: Option<u64>,
    prop: u32,
) -> rquickjs::qjs::JSValue {
    // SAFETY: as the caller's.
    unsafe {
        // A text goes to QuickJS straight from the host's, with no copy
        // of it on the way, and with no second read of it the long way.
        let mut text = rquickjs::qjs::JS_UNDEFINED;
        let mut made = |s: &str| {
            text = rquickjs::qjs::JS_NewStringLen(ctx, s.as_ptr().cast(), s.len() as _);
        };
        match raw_plain(ctx, this, known, |_, host, id| {
            host.get_plain(id, prop, &mut made)
        }) {
            Some(PlainValue::Number(n)) => return raw_number(n),
            Some(PlainValue::Bool(b)) => return raw_bool(b),
            // An exception when there was no room for it.
            Some(PlainValue::Text) => return text,
            None => {}
        }
        raw_accessor(ctx, this, |scope, host, id| {
            let out = host.get(scope, id, prop).map_err(|e| e.into_js(&scope.ctx));
            raw_out(scope, &scope.back(out)?)
        })
    }
}

unsafe extern "C" fn raw_set(
    ctx: *mut rquickjs::qjs::JSContext,
    this: rquickjs::qjs::JSValue,
    argc: std::ffi::c_int,
    argv: *mut rquickjs::qjs::JSValue,
    _magic: std::ffi::c_int,
    data: *mut rquickjs::qjs::JSValue,
) -> rquickjs::qjs::JSValue {
    use rquickjs::qjs;
    ACCESSOR_CALLS.set(ACCESSOR_CALLS.get() + 1);
    // SAFETY: QuickJS calls it on a live context with `argc` arguments and
    // the data it was made with.
    unsafe {
        let value = if argc > 0 { *argv } else { qjs::JS_UNDEFINED };
        prop_set(ctx, this, None, prop_id(data), value)
    }
}

/// Writes `value`, borrowed, to the property `prop` of the wrapper `this`
/// ([`raw_set`], and host/answer.rs): undefined, or an exception. `known`
/// is as [`prop_get`]'s.
///
/// # Safety
/// A live context; `known` is the id of `this`.
#[inline]
unsafe fn prop_set(
    ctx: *mut rquickjs::qjs::JSContext,
    this: rquickjs::qjs::JSValue,
    known: Option<u64>,
    prop: u32,
    value: rquickjs::qjs::JSValue,
) -> rquickjs::qjs::JSValue {
    use rquickjs::qjs;
    // SAFETY: as the caller's.
    unsafe {
        let plain = match raw_in(value) {
            Some(JsValue::Number(n)) => Some(PlainValue::Number(n)),
            Some(JsValue::Bool(b)) => Some(PlainValue::Bool(b)),
            _ => None,
        };
        // What the write set off may have been stopped: the scope says so.
        let done = plain.and_then(|v| {
            raw_plain(ctx, this, known, |state, host, id| {
                Some(host.set_plain(id, prop, v)?.map(|()| state.stopped.get()))
            })
        });
        match done {
            Some(Ok(false)) => return qjs::JS_UNDEFINED,
            Some(Ok(true)) => {
                return raw_accessor(ctx, this, |scope, _, _| {
                    scope.back(Ok(()))?;
                    Ok(qjs::JS_UNDEFINED)
                });
            }
            Some(Err(e)) => {
                let mut e = Some(e);
                return raw_accessor(ctx, this, |scope, _, _| {
                    let e = e.take().expect("called once");
                    scope.back(Err(e.into_js(&scope.ctx)))
                });
            }
            _ => {}
        }
        raw_accessor(ctx, this, |scope, host, id| {
            let v = match raw_in(value) {
                Some(v) => v,
                None => {
                    let value = Value::from_raw(scope.ctx.clone(), qjs::JS_DupValue(ctx, value));
                    scope.script_value(&value)?
                }
            };
            scope.back(
                host.set(scope, id, prop, v)
                    .map_err(|e| e.into_js(&scope.ctx)),
            )?;
            Ok(qjs::JS_UNDEFINED)
        })
    }
}

/// Runs `f` for the wrapper `this` is, whose id is `known` when the
/// caller has it, and its host, without the scope [`raw_accessor`] makes:
/// `None` when there is none, or when `f` panicked, which the long way
/// then meets again and reports.
unsafe fn raw_plain<T>(
    raw: *mut rquickjs::qjs::JSContext,
    this: rquickjs::qjs::JSValue,
    known: Option<u64>,
    f: impl FnOnce(&HostState, &dyn HostObjects, u64) -> Option<T>,
) -> Option<T> {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: QuickJS calls with a live context.
        let state = unsafe { raw_state(raw) }?;
        let id = match known {
            Some(id) => id,
            // SAFETY: the class is the runtime's.
            None => unsafe { wrapper_id(state.wrapper_class, this) }?,
        };
        let host = state.host.borrow();
        f(&state, &**host.as_ref()?, id)
    }))
    .ok()
    .flatten()
}

#[allow(clippy::float_cmp)]
fn raw_number(n: f64) -> rquickjs::qjs::JSValue {
    use rquickjs::qjs;
    // As `Value::new_number` makes it, but for -0, which an int cannot be.
    if n == f64::from(n as i32) && !(n == 0.0 && n.is_sign_negative()) {
        qjs::JS_MKVAL(qjs::JS_TAG_INT, n as i32)
    } else {
        qjs::JS_NewFloat64(n)
    }
}

fn raw_bool(b: bool) -> rquickjs::qjs::JSValue {
    if b {
        rquickjs::qjs::JS_TRUE
    } else {
        rquickjs::qjs::JS_FALSE
    }
}

/// A value with no reference, made by its tag rather than through a
/// Value, which would hold the context and be freed.
fn raw_out(scope: &Scope<'_>, v: &JsValue) -> rquickjs::Result<rquickjs::qjs::JSValue> {
    use rquickjs::qjs;
    Ok(match *v {
        JsValue::Undefined => qjs::JS_UNDEFINED,
        JsValue::Null => qjs::JS_NULL,
        JsValue::Bool(b) => raw_bool(b),
        JsValue::Number(n) => raw_number(n),
        _ => scope.to_raw(v)?,
    })
}

/// What [`raw_out`] makes, the other way: a number or a bool, by its tag.
fn raw_in(v: rquickjs::qjs::JSValue) -> Option<JsValue> {
    use rquickjs::qjs;
    // SAFETY: reading a tag and the number or bool it says is there.
    unsafe {
        match qjs::JS_VALUE_GET_NORM_TAG(v) {
            qjs::JS_TAG_INT => Some(JsValue::Number(f64::from(qjs::JS_VALUE_GET_INT(v)))),
            qjs::JS_TAG_FLOAT64 => Some(JsValue::Number(qjs::JS_VALUE_GET_FLOAT64(v))),
            qjs::JS_TAG_BOOL => Some(JsValue::Bool(qjs::JS_VALUE_GET_BOOL(v))),
            _ => None,
        }
    }
}

/// An Engine-owned wrapper ([`HostState::wrappers`]), and its raw value,
/// which the `Persistent` keeps alive.
struct Kept {
    held: Persistent<Object<'static>>,
    raw: rquickjs::qjs::JSValue,
}

/// `v`'s raw value, with the reference `v` held.
fn owned_raw(v: Value<'_>) -> rquickjs::qjs::JSValue {
    let raw = v.as_raw();
    // SAFETY: the reference dropped with `v` is the one taken here.
    unsafe { rquickjs::qjs::JS_DupValue(v.ctx().as_raw().as_ptr(), raw) }
}

/// The property id an accessor was made with ([`install_prop`]).
///
/// # Safety
/// `data` is the accessor's own data, one number.
unsafe fn prop_id(data: *mut rquickjs::qjs::JSValue) -> u32 {
    use rquickjs::qjs;
    // Read by its tag, as a Value would hold a reference to the context.
    unsafe {
        let v = *data;
        if qjs::JS_VALUE_GET_NORM_TAG(v) == qjs::JS_TAG_INT {
            qjs::JS_VALUE_GET_INT(v) as u32
        } else {
            qjs::JS_VALUE_GET_FLOAT64(v) as u32
        }
    }
}

/// The id of the object `this` wraps, or a TypeError.
fn this_id(ctx: &Ctx<'_>, this: &Value<'_>) -> rquickjs::Result<u64> {
    let raw = ctx.as_raw().as_ptr();
    // SAFETY: the context is live; the class is its runtime's.
    unsafe { raw_state(raw).and_then(|st| wrapper_id(st.wrapper_class, this.as_raw())) }
        .ok_or_else(|| Exception::throw_type(ctx, "not an object of this type"))
}

/// The host a raw accessor runs for, from the context's opaque (set by
/// [`JsHost::new`]).
unsafe fn raw_state(ctx: *mut rquickjs::qjs::JSContext) -> Option<Rc<HostState>> {
    // SAFETY: the opaque is the `Weak` JsHost::new put there, or null; it
    // is borrowed, not taken.
    unsafe {
        let p = rquickjs::qjs::JS_GetContextOpaque(ctx).cast::<HostState>();
        if p.is_null() {
            return None;
        }
        std::mem::ManuallyDrop::new(Weak::from_raw(p.cast_const())).upgrade()
    }
}

/// Runs `f` for the wrapper `this` is ([`raw_entry`]).
unsafe fn raw_accessor<'js>(
    raw: *mut rquickjs::qjs::JSContext,
    this: rquickjs::qjs::JSValue,
    f: impl FnOnce(&Scope<'js>, &dyn HostObjects, u64) -> rquickjs::Result<rquickjs::qjs::JSValue>,
) -> rquickjs::qjs::JSValue {
    // SAFETY: as the caller's.
    unsafe {
        raw_entry(raw, |scope| {
            let Some(id) = wrapper_id(scope.state.wrapper_class, this) else {
                return Err(Exception::throw_type(
                    &scope.ctx,
                    "not an object of this type",
                ));
            };
            let host = scope.state.host()?;
            f(scope, &*host, id)
        })
    }
}

/// Runs `f` in the host the context is for, and gives QuickJS what it
/// returned or threw. A panic cannot unwind through QuickJS: it is kept
/// for the dispatch to resume, and the script is stopped.
unsafe fn raw_entry<'js>(
    raw: *mut rquickjs::qjs::JSContext,
    f: impl FnOnce(&Scope<'js>) -> rquickjs::Result<rquickjs::qjs::JSValue>,
) -> rquickjs::qjs::JSValue {
    use rquickjs::qjs;
    // SAFETY: QuickJS calls with a live context.
    let run = || unsafe {
        let ctx = Ctx::from_raw(NonNull::new_unchecked(raw));
        let Some(state) = raw_state(raw) else {
            return Err(Exception::throw_message(&ctx, "JS host was dropped"));
        };
        let scope = Scope { ctx, state };
        f(&scope)
    };
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(Ok(v)) => v,
        Ok(Err(rquickjs::Error::Exception)) => qjs::JS_EXCEPTION,
        Ok(Err(e)) => {
            // SAFETY: as above.
            let ctx = unsafe { Ctx::from_raw(NonNull::new_unchecked(raw)) };
            // As rquickjs throws a conversion's error.
            let _ = Exception::throw_type(&ctx, &e.to_string());
            qjs::JS_EXCEPTION
        }
        Err(panic) => {
            // SAFETY: as above.
            let ctx = unsafe { Ctx::from_raw(NonNull::new_unchecked(raw)) };
            if let Some(state) = unsafe { raw_state(raw) } {
                state.panicked.borrow_mut().get_or_insert(panic);
            }
            if let Ok(e) = Exception::from_message(ctx.clone(), "panicked") {
                // SAFETY: the error object is alive; the flag only marks it.
                unsafe { qjs::JS_SetUncatchableError(raw, e.as_value().as_raw()) };
                let _ = ctx.throw(e.into_value());
            }
            qjs::JS_EXCEPTION
        }
    }
}

fn make_global<'js>(scope: &Scope<'js>, f: HostFn, name: &str) -> rquickjs::Result<Function<'js>> {
    let magic = {
        let mut globals = scope.state.globals.borrow_mut();
        globals.push(f);
        globals.len() - 1
    };
    // QuickJS keeps `magic` in 16 bits.
    let Ok(magic) = i16::try_from(magic).map(std::ffi::c_int::from) else {
        return Err(Exception::throw_range(
            &scope.ctx,
            "too many global functions",
        ));
    };
    // SAFETY: the context is alive and entered; the new function is owned
    // by the Value made of it.
    let func = unsafe {
        let raw = scope.ctx.as_raw().as_ptr();
        let v = rquickjs::qjs::JS_NewCFunctionData(
            raw,
            Some(raw_global),
            0,
            magic,
            0,
            std::ptr::null_mut(),
        );
        if rquickjs::qjs::JS_IsException(v) {
            return Err(rquickjs::Error::Exception);
        }
        Value::from_raw(scope.ctx.clone(), v)
    };
    func.into_function()
        .ok_or_else(|| Exception::throw_type(&scope.ctx, "not a function"))?
        .with_name(name)
}

/// A global host function: plain C, as the property accessors are
/// ([`raw_get`]).
unsafe extern "C" fn raw_global(
    ctx: *mut rquickjs::qjs::JSContext,
    _this: rquickjs::qjs::JSValue,
    argc: std::ffi::c_int,
    argv: *mut rquickjs::qjs::JSValue,
    magic: std::ffi::c_int,
    _data: *mut rquickjs::qjs::JSValue,
) -> rquickjs::qjs::JSValue {
    use rquickjs::qjs;
    // SAFETY: QuickJS calls it on a live context with `argc` arguments,
    // borrowed, so each is duplicated for the Value that frees it.
    unsafe {
        raw_entry(ctx, |scope| {
            let f = scope.state.globals.borrow().get(magic as usize).cloned();
            let Some(f) = f else {
                return Err(Exception::throw_message(&scope.ctx, "JS host was dropped"));
            };
            let args = (0..argc.max(0) as usize)
                .map(|i| {
                    let a = Value::from_raw(scope.ctx.clone(), qjs::JS_DupValue(ctx, *argv.add(i)));
                    scope.from_value(&a)
                })
                .collect::<rquickjs::Result<Vec<_>>>()?;
            let out = f(scope, &args).map_err(|e| e.into_js(&scope.ctx));
            raw_out(scope, &scope.back(out)?)
        })
    }
}

/// A method reads as a function bound to its object, as QML methods are:
/// `Qt.callLater(obj.method)` and `sig.connect(obj.method)` keep the
/// object. The bound function is made on first read and kept on the
/// wrapper, so reads compare equal (`disconnect(obj.method)` works).
fn install_method<'js>(
    proto: &Object<'js>,
    state: Weak<HostState>,
    def: &MethodDef,
) -> rquickjs::Result<()> {
    let (method, copied) = (def.id, def.copied);
    let key = def.name.clone();
    let getter =
        move |ctx: Ctx<'js>, This(this): This<Value<'js>>| -> rquickjs::Result<Value<'js>> {
            let id = this_id(&ctx, &this)?;
            let (scope, host) = callback_scope(&ctx, &state)?;
            // What the getter gives is the object's from now on, unless the
            // host says it may give another later.
            let (f, keep) = match host.method_face(&scope, id, method) {
                MethodFace::Direct(f) => (f, true),
                MethodFace::Call { keep } => (make_method(&ctx, id, method, copied, &key)?, keep),
            };
            if keep && let Some(o) = this.as_object() {
                o.prop(
                    key.as_str(),
                    Property::from(f.clone()).writable().configurable(),
                )?;
            }
            Ok(f.into_value())
        };
    proto.prop(def.name.as_str(), Accessor::new_get(getter).configurable())
}

/// A method bound to the object `id`: plain C, as a global host function
/// is ([`raw_global`]), with the object, the method and the arguments it
/// takes by copy ([`MethodDef::copied`]) in its data.
fn make_method<'js>(
    ctx: &Ctx<'js>,
    id: u64,
    method: u32,
    copied: u32,
    name: &str,
) -> rquickjs::Result<Function<'js>> {
    use rquickjs::qjs;
    // Numbers hold no reference; the id goes in two halves, as a double
    // keeps only 53 bits.
    let mut data = [
        Value::new_int(ctx.clone(), id as u32 as i32).as_raw(),
        Value::new_int(ctx.clone(), (id >> 32) as u32 as i32).as_raw(),
        Value::new_int(ctx.clone(), method as i32).as_raw(),
        Value::new_int(ctx.clone(), copied as i32).as_raw(),
    ];
    // SAFETY: the context is alive and entered; `JS_NewCFunctionData`
    // copies the data, and the new function is owned by the Value.
    let func = unsafe {
        let raw = ctx.as_raw().as_ptr();
        let v = qjs::JS_NewCFunctionData(raw, Some(raw_method), 0, 0, 4, data.as_mut_ptr());
        if qjs::JS_IsException(v) {
            return Err(rquickjs::Error::Exception);
        }
        Value::from_raw(ctx.clone(), v)
    };
    func.into_function()
        .ok_or_else(|| Exception::throw_type(ctx, "not a function"))?
        .with_name(name)
}

/// A call of a method [`make_method`] made.
unsafe extern "C" fn raw_method(
    ctx: *mut rquickjs::qjs::JSContext,
    _this: rquickjs::qjs::JSValue,
    argc: std::ffi::c_int,
    argv: *mut rquickjs::qjs::JSValue,
    _magic: std::ffi::c_int,
    data: *mut rquickjs::qjs::JSValue,
) -> rquickjs::qjs::JSValue {
    use rquickjs::qjs;
    // SAFETY: QuickJS calls it on a live context with `argc` arguments,
    // borrowed, so each is duplicated for the Value that frees it, and
    // with the four numbers `make_method` gave it.
    unsafe {
        let low = u64::from(qjs::JS_VALUE_GET_INT(*data) as u32);
        let high = u64::from(qjs::JS_VALUE_GET_INT(*data.add(1)) as u32);
        let method = qjs::JS_VALUE_GET_INT(*data.add(2)) as u32;
        let copied = qjs::JS_VALUE_GET_INT(*data.add(3)) as u32;
        raw_entry(ctx, |scope| {
            let host = scope.state.host()?;
            let args = (0..argc.max(0) as usize)
                .map(|i| {
                    // A number or a bool is the same copied or not, read
                    // from its tag: most arguments are, and are read alone.
                    if let Some(plain) = raw_in(*argv.add(i)) {
                        return Ok(plain);
                    }
                    let a = Value::from_raw(scope.ctx.clone(), qjs::JS_DupValue(ctx, *argv.add(i)));
                    if i < 32 && copied >> i & 1 == 1 {
                        scope.copy_of(&a)
                    } else {
                        scope.script_value(&a)
                    }
                })
                .collect::<rquickjs::Result<Vec<_>>>()?;
            let out = host
                .call(scope, high << 32 | low, method, args)
                .map_err(|e| e.into_js(&scope.ctx));
            raw_out(scope, &scope.back(out)?)
        })
    }
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
                what: String::new(),
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
/// whole; a name called by itself has no path.
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
    // Where the column could be either, a member called is the likelier.
    let found: Vec<_> = ends
        .into_iter()
        .filter_map(|end| chain_before(&s, end))
        .collect();
    let i = found
        .iter()
        .position(|(path, _)| !path.is_empty())
        .unwrap_or(0);
    found.into_iter().nth(i)
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
    // With no path it is a name called by itself.
    let before = back_ws(at);
    if before > 0 && matches!(s[before - 1], '.' | ')' | ']' | '?') {
        return None;
    }
    path.reverse();
    Some((path, prop))
}

/// Whether `name` may have been given another value between the start of
/// the function `export` and column `col` of `line`: a write, a `++`, or a
/// declaration of the name, in any block. None when the start is not
/// found. Over-cautious by design: what it cannot tell apart keeps the
/// engine's own words.
fn rebound_before(
    text: &dyn Fn(u32) -> Option<String>,
    export: &str,
    line: u32,
    col: u32,
    name: &str,
) -> Option<bool> {
    let head = format!("function {export}(");
    let mut at = line;
    loop {
        let mut l: Vec<char> = text(at)?.chars().collect();
        if at == line {
            l.truncate(col.saturating_sub(1) as usize);
        }
        let start = find_chars(&l, &head);
        // The parameter list declares the name; what follows it is looked at.
        let from = start.map_or(0, |i| {
            let open = i + head.chars().count();
            l[open..]
                .iter()
                .position(|c| *c == ')')
                .map_or(l.len(), |p| open + p + 1)
        });
        if rebinds(&l[from..], name) {
            return Some(true);
        }
        if start.is_some() {
            return Some(false);
        }
        at = at.checked_sub(1).filter(|a| *a > 0 && line - a < 10_000)?;
    }
}

/// The path `local` was declared to hold, `["root", "a"]` of `var local =
/// root.a`, when the function `export` declares it so once before column
/// `col` of `line` and gives it no other value up to there. Over-cautious
/// like [`rebound_before`]: a declaration it cannot read is none.
fn local_alias(
    text: &dyn Fn(u32) -> Option<String>,
    export: &str,
    line: u32,
    col: u32,
    local: &str,
) -> Option<Vec<String>> {
    let head = format!("function {export}(");
    // The function's text from its parameter list's end up to the call.
    let mut body: Vec<char> = Vec::new();
    let mut at = line;
    loop {
        let mut l: Vec<char> = text(at)?.chars().collect();
        if at == line {
            l.truncate(col.saturating_sub(1) as usize);
        }
        if let Some(i) = find_chars(&l, &head) {
            let open = i + head.chars().count();
            let from = l[open..]
                .iter()
                .position(|c| *c == ')')
                .map_or(l.len(), |p| open + p + 1);
            l.drain(..from);
            l.push('\n');
            l.append(&mut body);
            body = l;
            break;
        }
        l.push('\n');
        l.append(&mut body);
        body = l;
        at = at.checked_sub(1).filter(|a| *a > 0 && line - a < 10_000)?;
    }
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let skip_ws = |mut i: usize| {
        while body.get(i).is_some_and(|c| c.is_whitespace()) {
            i += 1;
        }
        i
    };
    let word_at = |i: usize| {
        let mut end = i;
        while body.get(end).copied().is_some_and(ident) {
            end += 1;
        }
        (end > i && !body[i].is_ascii_digit())
            .then(|| (body[i..end].iter().collect::<String>(), end))
    };
    let n: Vec<char> = local.chars().collect();
    // `var|let|const local = `, the first declaration of the name.
    let declared = (0..body.len().checked_sub(n.len())? + 1).find_map(|i| {
        if body[i..i + n.len()] != n[..]
            || (i > 0 && (ident(body[i - 1]) || body[i - 1] == '.'))
            || body.get(i + n.len()).copied().is_some_and(ident)
        {
            return None;
        }
        let mut b = i;
        while b > 0 && body[b - 1].is_whitespace() {
            b -= 1;
        }
        let mut w = b;
        while w > 0 && ident(body[w - 1]) {
            w -= 1;
        }
        let keyword: String = body[w..b].iter().collect();
        matches!(keyword.as_str(), "var" | "let" | "const").then_some((w, i + n.len()))
    })?;
    let (decl, mut i) = declared;
    i = skip_ws(i);
    if body.get(i) != Some(&'=') || body.get(i + 1) == Some(&'=') {
        return None;
    }
    let mut chain = Vec::new();
    i = skip_ws(i + 1);
    loop {
        let (name, end) = word_at(i)?;
        chain.push(name);
        let next = skip_ws(end);
        if body.get(next) == Some(&'.') {
            i = skip_ws(next + 1);
            continue;
        }
        // The path is the whole value: nothing is called, read or added on.
        if !matches!(body.get(next), None | Some(';' | ',' | '}'))
            && !body[end..next].contains(&'\n')
        {
            return None;
        }
        i = end;
        break;
    }
    if rebinds(&body[..decl], local) || rebinds(&body[i..], local) {
        return None;
    }
    Some(chain)
}

/// Where `needle` starts in `s`, in characters.
fn find_chars(s: &[char], needle: &str) -> Option<usize> {
    let n: Vec<char> = needle.chars().collect();
    (n.len() <= s.len())
        .then(|| (0..=s.len() - n.len()).find(|i| s[*i..*i + n.len()] == n[..]))
        .flatten()
}

/// `name`, in `s`, written to or declared: after `var`, `let`, `const`,
/// `function`, `catch (` or `for (`, next to `++` or `--`, or before an
/// assignment.
fn rebinds(s: &[char], name: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let back_ws = |mut i: usize| {
        while i > 0 && s[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    };
    // The word that ends at `end`.
    let word_before = |end: usize| {
        let mut i = end;
        while i > 0 && ident(s[i - 1]) {
            i -= 1;
        }
        s[i..end].iter().collect::<String>()
    };
    let n: Vec<char> = name.chars().collect();
    if n.is_empty() || s.len() < n.len() {
        return false;
    }
    (0..=s.len() - n.len()).any(|i| {
        let end = i + n.len();
        if s[i..end] != n[..]
            || (i > 0 && (ident(s[i - 1]) || s[i - 1] == '.'))
            || s.get(end).copied().is_some_and(ident)
        {
            return false;
        }
        let b = back_ws(i);
        if matches!(
            word_before(b).as_str(),
            "var" | "let" | "const" | "function"
        ) {
            return true;
        }
        if b > 0
            && s[b - 1] == '('
            && matches!(word_before(back_ws(b - 1)).as_str(), "catch" | "for")
        {
            return true;
        }
        if b >= 2 && matches!(s[b - 2..b], ['+', '+'] | ['-', '-']) {
            return true;
        }
        let mut a = end;
        while a < s.len() && s[a].is_whitespace() {
            a += 1;
        }
        let rest: String = s[a..s.len().min(a + 4)].iter().collect();
        const OPS: [&str; 16] = [
            "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "**=", "<<=", ">>=", "??=",
            "||=", "&&=",
        ];
        (rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>"))
            || OPS.iter().any(|op| rest.starts_with(op))
    })
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

/// The names a type's prototype was given: its properties, then its
/// signals and methods, each a base's first.
#[derive(Default)]
pub(crate) struct Listing {
    props: Vec<String>,
    members: Vec<String>,
    /// The property each name a wrapper answers itself is, by its atom,
    /// which the prototype's entry keeps (host/answer.rs).
    pub(crate) slots: FxHashMap<rquickjs::qjs::JSAtom, Slot>,
    /// A write of a name the type has not is thrown an `Error`, as a QML
    /// object holds only the properties its type gives it.
    pub(crate) closed: bool,
}

/// A property of a type, as its wrappers answer for it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Slot {
    prop: u32,
    writable: bool,
}

/// Gives `to` the entry `name` of `from` as it is: the same accessor
/// functions, or the same value. Nothing when `from` has none.
fn copy_entry<'js>(
    ctx: &Ctx<'js>,
    from: &Object<'js>,
    to: &Object<'js>,
    name: &str,
) -> rquickjs::Result<()> {
    use rquickjs::qjs;
    let c = ctx.as_raw().as_ptr();
    // SAFETY: the context is alive for the call and both objects are its.
    // The descriptor's values are owned references: the define takes
    // over those it is given, and the others are freed here.
    unsafe {
        let atom = qjs::JS_NewAtomLen(c, name.as_ptr().cast(), name.len() as _);
        let mut d = std::mem::MaybeUninit::<qjs::JSPropertyDescriptor>::uninit();
        let got = qjs::JS_GetOwnProperty(c, d.as_mut_ptr(), from.as_raw(), atom);
        if got <= 0 {
            qjs::JS_FreeAtom(c, atom);
            return if got < 0 {
                Err(rquickjs::Error::Exception)
            } else {
                Ok(())
            };
        }
        let d = d.assume_init();
        let kept = (qjs::JS_PROP_CONFIGURABLE | qjs::JS_PROP_ENUMERABLE) as i32;
        let done = if d.flags & qjs::JS_PROP_GETSET as i32 != 0 {
            qjs::JS_FreeValue(c, d.value);
            qjs::JS_DefinePropertyGetSet(c, to.as_raw(), atom, d.getter, d.setter, d.flags & kept)
        } else {
            qjs::JS_FreeValue(c, d.getter);
            qjs::JS_FreeValue(c, d.setter);
            let kept = kept | qjs::JS_PROP_WRITABLE as i32;
            qjs::JS_DefinePropertyValue(c, to.as_raw(), atom, d.value, d.flags & kept)
        };
        qjs::JS_FreeAtom(c, atom);
        if done < 0 {
            return Err(rquickjs::Error::Exception);
        }
    }
    Ok(())
}

/// As Qt prints an object: its class, its address, and its objectName when
/// it has one.
fn install_to_string<'js>(
    ctx: &Ctx<'js>,
    proto: &Object<'js>,
    name: String,
) -> rquickjs::Result<()> {
    let to_string = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, This(this): This<Object<'_>>| -> rquickjs::Result<String> {
            let id = this_id(&ctx, this.as_value())?;
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
            Ok(match named {
                Some(n) => format!("{name}({id:#x}, \"{n}\")"),
                None => format!("{name}({id:#x})"),
            })
        },
    )?
    .with_name("toString")?;
    proto.prop(
        "toString",
        Property::from(to_string).writable().configurable(),
    )?;
    Ok(())
}

/// Gives `proto` a `toJSON` for `JSON.stringify` of an object of its type.
/// On Qt an object is written with its properties. The prelude's
/// `__ljos_qobjectJSON` writes them when there is one; without it an
/// object whose parent or children lead back to it throws for the circular
/// structure, and one that does not is written as it is.
fn install_to_json<'js>(ctx: &Ctx<'js>, proto: &Object<'js>) -> rquickjs::Result<()> {
    let to_json = Function::new(
        ctx.clone(),
        |ctx: Ctx<'js>, This(this): This<Object<'js>>| -> rquickjs::Result<Value<'js>> {
            if let Ok(write) = ctx.globals().get::<_, Function<'js>>("__ljos_qobjectJSON") {
                return write.call((this,));
            }
            if in_a_cycle(&this) {
                return Err(rquickjs::Exception::throw_type(
                    &ctx,
                    "Cannot convert circular structure to JSON",
                ));
            }
            Ok(this.into_value())
        },
    )?
    .with_name("toJSON")?;
    proto.prop("toJSON", Property::from(to_json).writable().configurable())?;
    Ok(())
}

/// Whether `o` has a parent, whose children hold it, or holds an object
/// whose parent it is. A read that throws (a destroyed object) leads
/// nowhere.
fn in_a_cycle<'js>(o: &Object<'js>) -> bool {
    fn read<'js>(from: &Object<'js>, key: &str) -> Option<Value<'js>> {
        match from.get::<_, Value<'js>>(key) {
            Ok(v) => Some(v),
            Err(_) => {
                let _ = from.ctx().catch();
                None
            }
        }
    }
    if read(o, "parent").is_some_and(|p| p.is_object()) {
        return true;
    }
    let Some(kids) = read(o, "data").and_then(Value::into_object) else {
        return false;
    };
    let n = read(&kids, "length")
        .and_then(|n| n.as_number())
        .unwrap_or(0.0);
    (0..n as u32).any(|i| {
        let kid = match kids.get::<_, Value>(i) {
            Ok(v) => v.into_object(),
            Err(_) => {
                let _ = kids.ctx().catch();
                None
            }
        };
        kid.and_then(|k| read(&k, "parent"))
            .is_some_and(|p| p.as_object() == Some(o))
    })
}

#[cfg(test)]
mod tests {
    use super::{called_member, calls, local_alias};

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
        assert_eq!(at("{ spawn() }", "spawn"), some("", "spawn"));
        assert_eq!(at("{ h (1) }", "1"), some("", "h"));
        assert_eq!(at("{ a?.b() }", "b"), None);
        assert_eq!(at("{ f().a.b() }", "b"), None);
        assert_eq!(at("{ x[0].a.b() }", "b"), None);
        assert_eq!(at("{ a.b.c }", "c"), None);
    }

    #[test]
    fn a_local_is_seen_through_to_the_path_it_was_given() {
        let at = |src: &str, local: &str| {
            let lines: Vec<&str> = src.lines().collect();
            let line = lines.len() as u32;
            let col = lines[lines.len() - 1].find("CALL").unwrap() as u32 + 1;
            let text = |n: u32| lines.get(n as usize - 1).map(|l| (*l).to_owned());
            local_alias(&text, "f", line, col, local).map(|p| p.join("."))
        };
        assert_eq!(
            at("function f(a) { var k = a.kid; CALL", "k"),
            Some("a.kid".into())
        );
        assert_eq!(
            at("function f() {\n  const o = root . obj\n  CALL", "o"),
            Some("root.obj".into())
        );
        assert_eq!(
            at("function f() { let k = kid, j = 1; CALL", "k"),
            Some("kid".into())
        );
        assert_eq!(at("function f() { var k = g(); CALL", "k"), None);
        assert_eq!(at("function f() { var k == kid; CALL", "k"), None);
        assert_eq!(at("function f() { var k = kid; k = other; CALL", "k"), None);
        assert_eq!(at("function f() { var kk = kid; CALL", "k"), None);
    }
}
