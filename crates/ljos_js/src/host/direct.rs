//! A method declared in a script that a script calls without the host in
//! between ([`Scope::direct_method`]).

use std::rc::Rc;

use rquickjs::{Array, Function, Value, qjs};
use smallvec::SmallVec;

use super::Scope;
use crate::{FnId, JsArg, JsValue};

/// One value a [`DirectCall`] passes before the caller's arguments.
#[derive(Debug, Clone)]
pub enum DirectParam {
    /// The same value on every call.
    Value(JsValue),
    /// The property `name` of the object, read as the call starts.
    Read(JsValue, String),
}

/// What [`Scope::direct_method`] makes a function of: `func`, called with
/// `this` and `params`, then the caller's arguments.
#[derive(Debug, Clone)]
pub struct DirectCall {
    pub func: FnId,
    pub this: JsValue,
    pub params: Vec<DirectParam>,
    /// For a call that threw, as [`Scope::retell_failed_call`] takes them.
    pub file: String,
    pub export: String,
    pub names: Vec<String>,
    pub takes: Vec<String>,
}

/// Slots of a direct method's data before its parameters: the function,
/// `this`, the retelling, how many parameters and how many the caller's
/// arguments are padded to. Each parameter then takes two: the value, or
/// the object a property is read from, and the property's name
/// (`undefined` for a value).
const FUNC: usize = 0;
const THIS: usize = 1;
const RETELL: usize = 2;
const PARAMS: usize = 3;
const TAKES: usize = 4;
const HEAD: usize = 5;

/// A direct method's call: QuickJS calls it with what
/// [`Scope::direct_method`] gave it as its data. A plain C function, not
/// a script one, which would make an `arguments` object and a frame of its
/// own on every call to pass them on.
unsafe extern "C" fn raw_direct(
    ctx: *mut qjs::JSContext,
    _this: qjs::JSValue,
    argc: std::ffi::c_int,
    argv: *mut qjs::JSValue,
    _magic: std::ffi::c_int,
    data: *mut qjs::JSValue,
) -> qjs::JSValue {
    // SAFETY: QuickJS calls it on a live context with the data it was made
    // with, and `argv` padded to `takes` (the function's length).
    unsafe {
        let slot = |i: usize| *data.add(i);
        let n = qjs::JS_VALUE_GET_INT(slot(PARAMS)) as usize;
        let takes = qjs::JS_VALUE_GET_INT(slot(TAKES)) as usize;
        let argc = usize::try_from(argc).unwrap_or(0);
        // Called with fewer than it takes, it passes as many as it takes,
        // as a function declared with them would.
        let given = argc.max(takes);
        // Most calls pass a few: on the stack, not allocated each time.
        let mut args: SmallVec<[qjs::JSValue; 8]> = SmallVec::with_capacity(n + given);
        for i in 0..n {
            let (v, name) = (slot(HEAD + 2 * i), slot(HEAD + 2 * i + 1));
            let got = if qjs::JS_IsUndefined(name) {
                qjs::JS_DupValue(ctx, v)
            } else {
                let atom = qjs::JS_ValueToAtom(ctx, name);
                if atom == qjs::JS_ATOM_NULL {
                    qjs::JS_EXCEPTION
                } else {
                    let got = qjs::JS_GetProperty(ctx, v, atom);
                    qjs::JS_FreeAtom(ctx, atom);
                    got
                }
            };
            if qjs::JS_IsException(got) {
                free_all(ctx, &args);
                return got;
            }
            args.push(got);
        }
        for j in 0..given {
            args.push(qjs::JS_DupValue(ctx, *argv.add(j)));
        }
        let ret = qjs::JS_Call(
            ctx,
            slot(FUNC),
            slot(THIS),
            args.len() as std::ffi::c_int,
            args.as_mut_ptr(),
        );
        if !qjs::JS_IsException(ret) {
            free_all(ctx, &args);
            return ret;
        }
        let thrown = qjs::JS_GetException(ctx);
        // An interrupt is no script's to catch, nor to retell.
        if qjs::JS_IsUncatchableError(thrown) {
            free_all(ctx, &args);
            return qjs::JS_Throw(ctx, thrown);
        }
        // The retelling is told what the caller gave, unpadded; it takes
        // the references the call was given.
        free_all(ctx, &args[n + argc..]);
        args.truncate(n + argc);
        let given = qjs::JS_NewArrayFrom(ctx, args.len() as std::ffi::c_int, args.as_ptr());
        if qjs::JS_IsException(given) {
            qjs::JS_FreeValue(ctx, thrown);
            return given;
        }
        let mut told_with = [thrown, given];
        let told = qjs::JS_Call(
            ctx,
            slot(RETELL),
            qjs::JS_UNDEFINED,
            2,
            told_with.as_mut_ptr(),
        );
        free_all(ctx, &told_with);
        if qjs::JS_IsException(told) {
            return told;
        }
        qjs::JS_Throw(ctx, told)
    }
}

/// # Safety
/// Each of `vs` holds a reference of its own.
unsafe fn free_all(ctx: *mut qjs::JSContext, vs: &[qjs::JSValue]) {
    for v in vs {
        // SAFETY: as the caller's.
        unsafe { qjs::JS_FreeValue(ctx, *v) };
    }
}

impl<'js> Scope<'js> {
    /// A function that calls `call.func` as the host would have called
    /// it for a method, in script alone: the parameters read as the call
    /// starts, what it throws the caller's to catch, worded as
    /// [`Scope::retell_failed_call`] words it, and what it gives back
    /// given back as it is.
    pub fn direct_method(&self, call: DirectCall, name: &str) -> rquickjs::Result<Function<'js>> {
        let func = {
            let fns = self.state.fns.borrow();
            fns.get(call.func).map(|e| e.func.clone())
        };
        let Some(func) = func else {
            return Err(rquickjs::Exception::throw_message(
                &self.ctx,
                "stale function handle",
            ));
        };
        let count = |n: usize| {
            i32::try_from(n)
                .map_err(|_| rquickjs::Exception::throw_range(&self.ctx, "too many parameters"))
        };
        let (params, takes) = (count(call.params.len())?, count(call.takes.len())?);
        let mut data: Vec<Value<'js>> = Vec::with_capacity(HEAD + 2 * call.params.len());
        data.push(func.restore(&self.ctx)?.into_value());
        data.push(self.to_value(&call.this)?);
        data.push(self.retelling(&call)?.into_value());
        data.push(Value::new_int(self.ctx.clone(), params));
        data.push(Value::new_int(self.ctx.clone(), takes));
        for p in &call.params {
            match p {
                DirectParam::Value(v) => {
                    data.push(self.to_value(v)?);
                    data.push(Value::new_undefined(self.ctx.clone()));
                }
                DirectParam::Read(o, name) => {
                    data.push(self.to_value(o)?);
                    data.push(rquickjs::String::from_str(self.ctx.clone(), name)?.into_value());
                }
            }
        }
        let mut raw: Vec<qjs::JSValue> = data.iter().map(Value::as_raw).collect();
        // SAFETY: the context is entered; QuickJS takes references of its
        // own to the data, which `data` still holds while it does.
        let made = unsafe {
            let f = qjs::JS_NewCFunctionData(
                self.ctx.as_raw().as_ptr(),
                Some(raw_direct),
                takes,
                0,
                count(raw.len())?,
                raw.as_mut_ptr(),
            );
            if qjs::JS_IsException(f) {
                return Err(rquickjs::Error::Exception);
            }
            Value::from_raw(self.ctx.clone(), f)
        };
        let made = Function::from_value(made)?;
        made.with_name(name)
    }

    /// What a direct method that threw throws: the same error, worded
    /// anew where it can be.
    fn retelling(&self, call: &DirectCall) -> rquickjs::Result<Function<'js>> {
        let state = Rc::downgrade(&self.state);
        let file = call.file.clone();
        let export = call.export.clone();
        let names = call.names.clone();
        let takes = call.takes.clone();
        Function::new(
            self.ctx.clone(),
            move |ctx: rquickjs::Ctx<'js>, thrown: Value<'js>, given: Array<'js>| {
                let Some(state) = state.upgrade() else {
                    return thrown;
                };
                let scope = Scope {
                    ctx: ctx.clone(),
                    state,
                };
                let args: Vec<JsArg> = given
                    .iter::<Value<'js>>()
                    .filter_map(|v| v.ok().and_then(|v| scope.from_value(&v).ok()))
                    .collect();
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                let takes: Vec<&str> = takes.iter().map(String::as_str).collect();
                let _ = scope.retell(&thrown, &file, &export, &names, &takes, &args);
                // What the retelling threw is dropped; the call's error
                // stands.
                let _ = ctx.catch();
                thrown
            },
        )
    }
}
