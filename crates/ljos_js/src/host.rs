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
    PropDef, SignalDef, Slice, SourceMapper, TypeDef, objects::HostWrapper,
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
    /// Prototype per registered type, indexed by [`HostTypeId`].
    types: RefCell<Vec<Persistent<Object<'static>>>>,
    /// Engine-owned wrappers. Kept strongly so identity is stable and so the
    /// Engine decides when a wrapper may die ([`JsHost::drop_wrapper`]).
    wrappers: RefCell<HashMap<u64, Persistent<Object<'static>>>>,
    host: RefCell<Option<Rc<dyn HostObjects>>>,
    released: Rc<RefCell<Vec<u64>>>,
    mapper: RefCell<Option<Rc<SourceMapper>>>,
    entered: Cell<bool>,
    /// JS values held by Rust through [`JsRef`].
    handles: RefCell<HashMap<u64, Persistent<Value<'static>>>>,
    next_handle: Cell<u64>,
    /// Keys of dropped [`JsRef`]s. A mutex because a `JsRef` may be dropped
    /// on any thread; only this host ever drains it.
    handle_release: Arc<Mutex<Vec<u64>>>,
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
            keys.iter().filter_map(|k| handles.remove(k)).collect()
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
        self.from_value(&out)
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
        self.from_value(&out)
    }

    /// The JS value behind a [`JsRef`].
    pub fn restore(&self, r: &JsRef) -> rquickjs::Result<Value<'js>> {
        let p = self.state.handles.borrow().get(&r.key()).cloned();
        match p {
            Some(p) => p.restore(&self.ctx),
            None => Err(Exception::throw_reference(
                &self.ctx,
                "stale JS reference (from another host, or already released)",
            )),
        }
    }

    /// Keeps a JS value alive for Rust.
    pub fn keep(&self, v: Value<'js>) -> JsRef {
        let key = self.state.next_handle.get();
        self.state.next_handle.set(key + 1);
        self.state
            .handles
            .borrow_mut()
            .insert(key, Persistent::save(&self.ctx, v));
        JsRef::new(key, self.state.handle_release.clone())
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

    /// The cached Engine-owned wrapper for `obj`, created on first use.
    pub fn wrapper(&self, obj: u64) -> rquickjs::Result<Object<'js>> {
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
        let proto = self.state.types.borrow().get(ty.0 as usize).cloned();
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
        })
    }

    pub fn from_value(&self, v: &Value<'js>) -> rquickjs::Result<JsValue> {
        self.read_value_at(v, 0)
    }

    fn read_value_at(&self, v: &Value<'js>, depth: u32) -> rquickjs::Result<JsValue> {
        // Cyclic arrays would otherwise recurse until the native stack limit.
        if depth > 32 {
            return Err(Exception::throw_type(&self.ctx, "value nested too deeply"));
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
                    None => JsValue::Handle(self.keep(v.clone())),
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
    }
}

struct EnterGuard<'a>(&'a Cell<bool>);
impl Drop for EnterGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

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
            mapper: RefCell::default(),
            entered: Cell::new(false),
            handles: RefCell::default(),
            next_handle: Cell::new(1),
            handle_release: Arc::default(),
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
        *self.state.host.borrow_mut() = Some(host);
    }

    /// Installs the hook that maps JS positions to QML positions in
    /// [`JsFault::JsException`].
    pub fn set_source_mapper(&self, mapper: Option<SourceMapper>) {
        *self.state.mapper.borrow_mut() = mapper.map(Rc::new);
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
                    Err(e) => Err(self.classify(&ctx, e, label, start.elapsed())),
                }
            })
        }));

        self.interrupt.deadline.set(None);
        let elapsed = start.elapsed();
        drop(guard);

        if elapsed > slice.soft {
            tracing::warn!(label, ?elapsed, soft = ?slice.soft, "JS dispatch exceeded its soft budget");
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
        f(&scope).map_err(|e| self.classify(&ctx, e, label, start.elapsed()))
    }

    fn classify<'js>(
        &self,
        ctx: &Ctx<'js>,
        err: rquickjs::Error,
        label: &str,
        elapsed: Duration,
    ) -> JsFault {
        let mapper = self.state.mapper.borrow().clone();
        classify_error(
            ctx,
            err,
            label,
            self.interrupt.fired.get(),
            elapsed,
            mapper.as_deref(),
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
            let name = def.name.clone();
            let to_string = Function::new(
                scope.ctx.clone(),
                move |This(this): This<Class<'_, HostWrapper>>| -> String {
                    format!("{name}({:#x})", this.borrow().id)
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
        types.push(proto);
        Ok(HostTypeId(
            u32::try_from(types.len() - 1).expect("type count fits u32"),
        ))
    }

    /// Forgets the cached wrapper of `obj`. JS that still holds it keeps a
    /// dangling wrapper whose accessors fail with "object no longer exists";
    /// the next `wrapper(obj)` makes a fresh one. Call it when the Engine
    /// destroys the object.
    pub fn drop_wrapper(&self, obj: u64) -> bool {
        self.state.wrappers.borrow_mut().remove(&obj).is_some()
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
        let host = self.state.host.borrow().clone();
        if let Some(host) = host {
            for id in ids {
                host.released(id);
            }
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
        let v = scope.from_value(&value)?;
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
                    .map(|a| scope.from_value(a))
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
                .map(|a| scope.from_value(a))
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

/// Turns an rquickjs error into a [`JsFault`], draining the pending exception.
pub(crate) fn classify_error<'js>(
    ctx: &Ctx<'js>,
    err: rquickjs::Error,
    label: &str,
    interrupted: bool,
    elapsed: Duration,
    mapper: Option<&SourceMapper>,
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
            describe_exception(&thrown, label, mapper)
        }
        other => JsFault::Internal(other.to_string()),
    }
}

fn describe_exception(thrown: &Value<'_>, label: &str, mapper: Option<&SourceMapper>) -> JsFault {
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
            (
                e.message().unwrap_or_default(),
                stack,
                file.or(sf),
                line.or(sl),
            )
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
    if let (Some(m), Some(f), Some(l)) = (mapper, file.as_deref(), line)
        && let Some((mf, ml)) = m(f, l)
    {
        file = Some(mf);
        line = Some(ml);
    }
    JsFault::JsException {
        message,
        stack,
        file,
        line,
    }
}

/// Parses `at fn (file:line:col)` or `at file:line:col` from the first frame.
fn first_frame(stack: &str) -> (Option<String>, Option<u32>) {
    let Some(frame) = stack.lines().map(str::trim).find(|l| l.starts_with("at ")) else {
        return (None, None);
    };
    let frame = frame.trim_start_matches("at ");
    let inner = match (frame.rfind('('), frame.ends_with(')')) {
        (Some(open), true) => &frame[open + 1..frame.len() - 1],
        _ => frame,
    };
    let mut parts = inner.rsplitn(3, ':');
    let (_col, line, file) = (parts.next(), parts.next(), parts.next());
    match (file, line.and_then(|l| l.parse().ok())) {
        (Some(f), Some(l)) => (Some(f.to_owned()), Some(l)),
        _ => (None, None),
    }
}

fn panic_message(p: &(dyn Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic>".into())
}
