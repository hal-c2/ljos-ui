//! What a script reads of an object's signal: an object of a QuickJS class
//! of its own that emits the signal on the object when it is called.
//!
//! As on Qt, each read is a new function with no properties of its own,
//! whose prototype is `Function.prototype`, and `connect` and `disconnect`
//! are on `Function.prototype`, shared by every read. A read is then one
//! small object: a handler that emits a signal of its object in a loop
//! reads the signal every time. Three named functions a read cost half of
//! such an emit.

use rquickjs::{Exception, Object, Value, qjs};

use super::{Scope, SignalDef, raw_accessor, raw_entry, raw_in, raw_out};
use crate::{JsRef, JsValue};

/// The signal an object of the class emits: its opaque.
struct Target {
    obj: u64,
    signal: u32,
}

/// Registers the class in `rt` and gives its id.
///
/// # Safety
/// `rt` is a live runtime.
pub(super) unsafe fn register(rt: *mut qjs::JSRuntime) -> Option<qjs::JSClassID> {
    let mut id: qjs::JSClassID = 0;
    let def = qjs::JSClassDef {
        class_name: c"Function".as_ptr(),
        finalizer: Some(finalize),
        gc_mark: None,
        call: Some(call),
        exotic: std::ptr::null_mut(),
    };
    // SAFETY: as the caller's; QuickJS copies the definition.
    unsafe {
        qjs::JS_NewClassID(rt, &mut id);
        (qjs::JS_NewClass(rt, id, &def) == 0).then_some(id)
    }
}

/// Gives `Function.prototype` of the entered context `connect` and
/// `disconnect`, which act on a signal read and throw on any other `this`.
pub(super) fn install_connect(ctx: &rquickjs::Ctx<'_>) -> rquickjs::Result<()> {
    let raw = ctx.as_raw().as_ptr();
    // SAFETY: the context is alive and entered; the prototype taken is
    // freed by the Value, and each function is taken by its property.
    unsafe {
        let proto = Value::from_raw(ctx.clone(), qjs::JS_GetFunctionProto(raw));
        for (name, magic) in [(c"connect", 1), (c"disconnect", 0)] {
            let f = qjs::JS_NewCFunctionData(raw, Some(connect), 1, magic, 0, std::ptr::null_mut());
            if qjs::JS_IsException(f) {
                return Err(rquickjs::Error::Exception);
            }
            let flags = (qjs::JS_PROP_CONFIGURABLE | qjs::JS_PROP_WRITABLE) as std::ffi::c_int;
            if qjs::JS_DefinePropertyValueStr(raw, proto.as_raw(), name.as_ptr(), f, flags) < 0 {
                return Err(rquickjs::Error::Exception);
            }
        }
    }
    Ok(())
}

/// Gives `proto` the getter of `sig`. The signal's id rides in the
/// getter's data, as a property's does (`install_prop`).
pub(super) fn install(proto: &Object<'_>, sig: &SignalDef) -> rquickjs::Result<()> {
    let ctx = proto.ctx().as_raw().as_ptr();
    // A number holds no reference, so its raw value outlives the Value.
    let mut id = Value::new_number(proto.ctx().clone(), f64::from(sig.id)).as_raw();
    // SAFETY: the context is alive and entered; `JS_NewCFunctionData`
    // copies the data, `JS_DefinePropertyGetSet` takes the getter, and the
    // atom is freed after it.
    unsafe {
        let getter = qjs::JS_NewCFunctionData(ctx, Some(get), 0, 0, 1, &mut id);
        if qjs::JS_IsException(getter) {
            return Err(rquickjs::Error::Exception);
        }
        let atom = qjs::JS_NewAtomLen(ctx, sig.name.as_ptr().cast(), sig.name.len() as _);
        let flags = (qjs::JS_PROP_CONFIGURABLE | qjs::JS_PROP_ENUMERABLE) as std::ffi::c_int;
        let r = qjs::JS_DefinePropertyGetSet(
            ctx,
            proto.as_raw(),
            atom,
            getter,
            qjs::JS_UNDEFINED,
            flags,
        );
        qjs::JS_FreeAtom(ctx, atom);
        if r < 0 {
            return Err(rquickjs::Error::Exception);
        }
    }
    Ok(())
}

/// A read of the signal: a new object of the class for the wrapper `this`.
unsafe extern "C" fn get(
    ctx: *mut qjs::JSContext,
    this: qjs::JSValue,
    _argc: std::ffi::c_int,
    _argv: *mut qjs::JSValue,
    _magic: std::ffi::c_int,
    data: *mut qjs::JSValue,
) -> qjs::JSValue {
    // SAFETY: QuickJS calls it on a live context with the data it was made
    // with; the new object owns the box from here.
    unsafe {
        let signal = super::prop_id(data);
        raw_accessor(ctx, this, |scope, _, obj| {
            let proto = qjs::JS_GetFunctionProto(ctx);
            let o = qjs::JS_NewObjectProtoClass(ctx, proto, scope.state.signal_class);
            qjs::JS_FreeValue(ctx, proto);
            if qjs::JS_IsException(o) {
                return Err(rquickjs::Error::Exception);
            }
            qjs::JS_SetOpaque(o, Box::into_raw(Box::new(Target { obj, signal })).cast());
            Ok(o)
        })
    }
}

/// The signal `v` emits, if it is a signal read.
///
/// # Safety
/// `class` is the class's id in `v`'s runtime.
unsafe fn target(class: qjs::JSClassID, v: qjs::JSValue) -> Option<(u64, u32)> {
    // SAFETY: the opaque of the class is a live `Target`.
    unsafe {
        let p = qjs::JS_GetOpaque(v, class).cast::<Target>();
        (!p.is_null()).then(|| ((*p).obj, (*p).signal))
    }
}

/// A call of a signal read emits it, with the call's arguments.
unsafe extern "C" fn call(
    ctx: *mut qjs::JSContext,
    func: qjs::JSValue,
    _this: qjs::JSValue,
    argc: std::ffi::c_int,
    argv: *mut qjs::JSValue,
    _flags: std::ffi::c_int,
) -> qjs::JSValue {
    // SAFETY: QuickJS calls it on a live context with `argc` arguments,
    // borrowed, so each is duplicated for the Value that frees it.
    unsafe {
        raw_entry(ctx, |scope| {
            let Some((obj, signal)) = target(scope.state.signal_class, func) else {
                return Err(Exception::throw_type(&scope.ctx, "not a signal"));
            };
            let host = scope.state.host()?;
            let args = (0..argc.max(0) as usize)
                .map(|i| {
                    // A number or a bool, what most signals carry, is read
                    // off its tag.
                    let raw = *argv.add(i);
                    if let Some(v) = raw_in(raw) {
                        return Ok(v);
                    }
                    let a = Value::from_raw(scope.ctx.clone(), qjs::JS_DupValue(ctx, raw));
                    scope.script_value(&a)
                })
                .collect::<rquickjs::Result<Vec<_>>>()?;
            let out = host
                .emit(scope, obj, signal, &args)
                .map_err(|e| e.into_js(&scope.ctx));
            scope.back(out)?;
            raw_out(scope, &JsValue::Undefined)
        })
    }
}

/// `connect` (magic 1) and `disconnect` (magic 0) of `Function.prototype`.
unsafe extern "C" fn connect(
    ctx: *mut qjs::JSContext,
    this: qjs::JSValue,
    argc: std::ffi::c_int,
    argv: *mut qjs::JSValue,
    magic: std::ffi::c_int,
    _data: *mut qjs::JSValue,
) -> qjs::JSValue {
    // SAFETY: as `call`'s.
    unsafe {
        raw_entry(ctx, |scope| {
            let Some((obj, signal)) = target(scope.state.signal_class, this) else {
                let verb = if magic == 1 { "connect" } else { "disconnect" };
                return Err(Exception::throw_type(
                    &scope.ctx,
                    &format!("Function.prototype.{verb}: this object is not a signal"),
                ));
            };
            let host = scope.state.host()?;
            let args = (0..argc.max(0) as usize)
                .map(|i| {
                    let a = Value::from_raw(scope.ctx.clone(), qjs::JS_DupValue(ctx, *argv.add(i)));
                    scope.from_value(&a)
                })
                .collect::<rquickjs::Result<Vec<_>>>()?;
            let r = if magic == 1 {
                host.connect(scope, obj, signal, &args)
            } else {
                host.disconnect(scope, obj, signal, &args)
            };
            scope.back(r.map_err(|e| e.into_js(&scope.ctx)))?;
            raw_out(scope, &JsValue::Undefined)
        })
    }
}

impl<'js> Scope<'js> {
    /// Whether two reads denote one signal, although each is a new function.
    pub fn same_signal(&self, a: &JsRef, b: &JsRef) -> bool {
        let (Ok(a), Ok(b)) = (self.restore(a), self.restore(b)) else {
            return false;
        };
        // SAFETY: both values are live in this context's runtime.
        unsafe {
            match (
                target(self.state.signal_class, a.as_raw()),
                target(self.state.signal_class, b.as_raw()),
            ) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            }
        }
    }
}

unsafe extern "C" fn finalize(_rt: *mut qjs::JSRuntime, val: qjs::JSValue) {
    let mut class = 0;
    // SAFETY: QuickJS finalises only objects of the class, whose opaque is
    // the box `get` made.
    unsafe {
        let p = qjs::JS_GetAnyOpaque(val, &mut class).cast::<Target>();
        if !p.is_null() {
            drop(Box::from_raw(p));
        }
    }
}
