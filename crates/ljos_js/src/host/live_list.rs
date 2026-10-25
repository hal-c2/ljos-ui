//! What a script holds of a list an object's property keeps
//! ([`JsValue::LiveList`]): an object of a QuickJS class of its own whose
//! `length` and entries are asked of the host whenever they are read.
//!
//! A copy of the list on every read would cost as many entries as the list
//! has, and a loop over `children[i]` reads the list once per step. The
//! class answers through QuickJS's exotic methods, so the array functions,
//! `for … of` and spreading read it as they read any object with a length.
//! It is not an array (`Array.isArray` is false): an entry cannot be
//! replaced, and its prototype stands between `Array.prototype` and it so
//! that it tells itself apart as a `QML List`.

use std::ffi::c_int;
use std::panic::{AssertUnwindSafe, catch_unwind};

use rquickjs::{Object, Persistent, Value, qjs};

use super::{Scope, raw_entry, raw_out, raw_state};
use crate::JsValue;

/// The list an object of the class reads: its opaque.
struct Target {
    obj: u64,
    prop: u32,
}

/// Registers the class in `rt` and gives its id.
///
/// # Safety
/// `rt` is a live runtime.
pub(super) unsafe fn register(rt: *mut qjs::JSRuntime) -> Option<qjs::JSClassID> {
    let mut id: qjs::JSClassID = 0;
    let def = qjs::JSClassDef {
        class_name: c"QML List".as_ptr(),
        finalizer: Some(finalize),
        gc_mark: None,
        call: None,
        // QuickJS keeps the pointer and never writes through it.
        exotic: (&raw const EXOTIC).cast_mut(),
    };
    // SAFETY: as the caller's; QuickJS copies the definition.
    unsafe {
        qjs::JS_NewClassID(rt, &raw mut id);
        (qjs::JS_NewClass(rt, id, &raw const def) == 0).then_some(id)
    }
}

static EXOTIC: qjs::JSClassExoticMethods = qjs::JSClassExoticMethods {
    get_own_property: Some(own),
    get_own_property_names: Some(names),
    delete_property: None,
    define_own_property: None,
    has_property: None,
    get_property: None,
    set_property: Some(set),
};

/// The prototype every list shares, made the first time one is, and an
/// object of it that is kept: QuickJS gives a new object the shape an
/// object of its prototype has, and makes one only when none is alive,
/// as none would be when every list is let go before the next is read.
/// Sorting is refused before it starts, as on Qt: `Array.prototype.sort`
/// would only fail if an entry had to move.
const PROTO: &str = r#"
(function () {
  const p = Object.create(Array.prototype);
  Object.defineProperty(p, Symbol.toStringTag, { value: "QML List" });
  Object.defineProperty(p, "sort", {
    value: function sort() { throw new TypeError(REPLACE); },
    writable: true, configurable: true,
  });
  return [p, Object.create(p)];
})()
"#;

const REPLACE: &str = "an entry of a list of objects cannot be replaced";

impl<'js> Scope<'js> {
    /// A new list object reading the list `prop` of `obj`.
    pub(super) fn live_list(&self, obj: u64, prop: u32) -> rquickjs::Result<Value<'js>> {
        let cached = self
            .state
            .list_proto
            .borrow()
            .as_ref()
            .map(|(p, _)| p.clone());
        let proto = match cached {
            Some(p) => p.restore(&self.ctx)?,
            None => {
                let made: rquickjs::Array<'js> = self
                    .ctx
                    .eval(PROTO.replace("REPLACE", &format!("{REPLACE:?}")))?;
                let (p, keeper): (Object<'js>, Object<'js>) = (made.get(0)?, made.get(1)?);
                *self.state.list_proto.borrow_mut() = Some((
                    Persistent::save(&self.ctx, p.clone()),
                    Persistent::save(&self.ctx, keeper),
                ));
                p
            }
        };
        let raw = self.ctx.as_raw().as_ptr();
        // SAFETY: the context is entered and the class is its runtime's;
        // the object owns the box from here.
        unsafe {
            let o = qjs::JS_NewObjectProtoClass(raw, proto.as_raw(), self.state.list_class);
            if qjs::JS_IsException(o) {
                return Err(rquickjs::Error::Exception);
            }
            qjs::JS_SetOpaque(o, Box::into_raw(Box::new(Target { obj, prop })).cast());
            Ok(Value::from_raw(self.ctx.clone(), o))
        }
    }

    /// The list `v` reads, if it is a list object.
    pub(super) fn live_list_of(&self, v: &Value<'js>) -> Option<JsValue> {
        // SAFETY: the opaque of the class is a live `Target`.
        unsafe {
            let p = qjs::JS_GetOpaque(v.as_raw(), self.state.list_class).cast::<Target>();
            (!p.is_null()).then(|| JsValue::LiveList {
                obj: (*p).obj,
                prop: (*p).prop,
            })
        }
    }
}

unsafe extern "C" fn finalize(_rt: *mut qjs::JSRuntime, val: qjs::JSValue) {
    let mut class = 0;
    // SAFETY: QuickJS finalises only objects of the class, whose opaque is
    // the box `live_list` made.
    unsafe {
        let p = qjs::JS_GetAnyOpaque(val, &raw mut class).cast::<Target>();
        if !p.is_null() {
            drop(Box::from_raw(p));
        }
    }
}

/// The list `obj` reads; QuickJS calls the exotic methods only for objects
/// of the class.
unsafe fn target(obj: qjs::JSValue) -> Option<(u64, u32)> {
    let mut class = 0;
    // SAFETY: as above.
    unsafe {
        let p = qjs::JS_GetAnyOpaque(obj, &raw mut class).cast::<Target>();
        (!p.is_null()).then(|| ((*p).obj, (*p).prop))
    }
}

/// An atom QuickJS keeps as the integer itself, as it keeps every array
/// index below 2³¹ (a test holds it to that); any other atom is a name.
fn index_of(atom: qjs::JSAtom) -> Option<u32> {
    const TAG_INT: u32 = 1 << 31;
    (atom & TAG_INT != 0).then_some(atom & !TAG_INT)
}

/// The length of the list, asked of the host without entering a scope:
/// a loop reads it once a step. `None` when there is no host to ask, or
/// it panicked.
unsafe fn raw_len(ctx: *mut qjs::JSContext, obj: u64, prop: u32) -> Option<u32> {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: as the caller's.
        let state = unsafe { raw_state(ctx) }?;
        let host = state.host.borrow();
        Some(host.as_ref()?.list_len(obj, prop))
    }))
    .ok()
    .flatten()
}

/// Whether `atom` is `length`.
unsafe fn is_length(ctx: *mut qjs::JSContext, atom: qjs::JSAtom) -> bool {
    // SAFETY: as the caller's.
    unsafe { raw_state(ctx) }.is_some_and(|st| st.length_atom == atom)
}

unsafe extern "C" fn own(
    ctx: *mut qjs::JSContext,
    desc: *mut qjs::JSPropertyDescriptor,
    obj: qjs::JSValue,
    atom: qjs::JSAtom,
) -> c_int {
    // SAFETY: QuickJS calls with a live context and an object of the class.
    unsafe {
        let Some((o, p)) = target(obj) else {
            return 0;
        };
        let length = is_length(ctx, atom);
        let index = index_of(atom);
        if !length && index.is_none() {
            return 0;
        }
        if length && let Some(n) = raw_len(ctx, o, p) {
            if !desc.is_null() {
                (*desc).flags = qjs::JS_PROP_WRITABLE as c_int;
                (*desc).value = qjs::JS_MKVAL(qjs::JS_TAG_INT, n as i32);
                (*desc).getter = qjs::JS_UNDEFINED;
                (*desc).setter = qjs::JS_UNDEFINED;
            }
            return 1;
        }
        let mut found = true;
        let v = raw_entry(ctx, |scope| {
            let host = scope.state.host()?;
            if length {
                return Ok(qjs::JS_MKVAL(qjs::JS_TAG_INT, host.list_len(o, p) as i32));
            }
            match index.and_then(|i| host.list_entry(o, p, i)) {
                Some(v) => raw_out(scope, &v),
                None => {
                    found = false;
                    Ok(qjs::JS_UNDEFINED)
                }
            }
        });
        if qjs::JS_IsException(v) {
            return -1;
        }
        if !found {
            return 0;
        }
        if desc.is_null() {
            qjs::JS_FreeValue(ctx, v);
        } else {
            (*desc).flags = if length {
                qjs::JS_PROP_WRITABLE as c_int
            } else {
                (qjs::JS_PROP_ENUMERABLE | qjs::JS_PROP_WRITABLE) as c_int
            };
            (*desc).value = v;
            (*desc).getter = qjs::JS_UNDEFINED;
            (*desc).setter = qjs::JS_UNDEFINED;
        }
        1
    }
}

unsafe extern "C" fn names(
    ctx: *mut qjs::JSContext,
    tab: *mut *mut qjs::JSPropertyEnum,
    len: *mut u32,
    obj: qjs::JSValue,
) -> c_int {
    // SAFETY: QuickJS calls with a live context, an object of the class
    // and places for the table, which it frees with `js_free`.
    unsafe {
        let Some((o, p)) = target(obj) else {
            *tab = std::ptr::null_mut();
            *len = 0;
            return 0;
        };
        // Without a host there is nothing in the list.
        let n = raw_len(ctx, o, p).unwrap_or(0);
        let count = n as usize + 1;
        let t = qjs::js_malloc(
            ctx,
            (count * size_of::<qjs::JSPropertyEnum>()) as qjs::size_t,
        )
        .cast::<qjs::JSPropertyEnum>();
        if t.is_null() {
            return -1;
        }
        for i in 0..n {
            t.add(i as usize).write(qjs::JSPropertyEnum {
                is_enumerable: true,
                atom: qjs::JS_NewAtomUInt32(ctx, i),
            });
        }
        t.add(n as usize).write(qjs::JSPropertyEnum {
            is_enumerable: false,
            atom: qjs::JS_NewAtom(ctx, c"length".as_ptr()),
        });
        *tab = t;
        *len = count as u32;
        0
    }
}

unsafe extern "C" fn set(
    ctx: *mut qjs::JSContext,
    obj: qjs::JSValue,
    atom: qjs::JSAtom,
    value: qjs::JSValue,
    _receiver: qjs::JSValue,
    flags: c_int,
) -> c_int {
    // SAFETY: QuickJS calls with a live context and an object of the class;
    // `value` is borrowed.
    unsafe {
        let Some((o, p)) = target(obj) else {
            return 0;
        };
        let length = is_length(ctx, atom);
        let index = index_of(atom);
        if !length && index.is_none() {
            // Kept by this list object alone, as on any object.
            return qjs::JS_DefinePropertyValue(
                ctx,
                obj,
                atom,
                qjs::JS_DupValue(ctx, value),
                qjs::JS_PROP_C_W_E as c_int,
            );
        }
        let mut done = 1;
        let r = raw_entry(ctx, |scope| {
            let host = scope.state.host()?;
            let now = host.list_len(o, p);
            if length {
                let mut n = 0.0;
                if qjs::JS_ToFloat64(ctx, &raw mut n, value) < 0 {
                    return Err(rquickjs::Error::Exception);
                }
                // What `push` sets after the entry it put, and a longer
                // length, which a list of objects has nothing to fill with.
                if n >= 0.0 && n < f64::from(now) && n == n.trunc() {
                    let out = host.list_truncate(scope, o, p, n as u32);
                    scope.back(out.map_err(|e| e.into_js(&scope.ctx)))?;
                }
                return Ok(qjs::JS_UNDEFINED);
            }
            let i = index.unwrap_or(u32::MAX);
            if i == now {
                let v = Value::from_raw(scope.ctx.clone(), qjs::JS_DupValue(ctx, value));
                let item = scope.script_value(&v)?;
                let out = host.list_append(scope, o, p, item);
                scope.back(out.map_err(|e| e.into_js(&scope.ctx)))?;
                return Ok(qjs::JS_UNDEFINED);
            }
            // An entry is not replaced: `list[0] = x` leaves the list as
            // it is, and what asks for the change to stick (`sort`) hears
            // that it did not.
            done = 0;
            if i < now && flags & qjs::JS_PROP_THROW as c_int != 0 {
                return Err(rquickjs::Exception::throw_type(&scope.ctx, REPLACE));
            }
            Ok(qjs::JS_UNDEFINED)
        });
        if qjs::JS_IsException(r) {
            return -1;
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quickjs_keeps_an_index_as_the_integer_itself() {
        let rt = rquickjs::Runtime::new().unwrap();
        let ctx = rquickjs::Context::full(&rt).unwrap();
        ctx.with(|c| {
            let raw = c.as_raw().as_ptr();
            for i in [0, 1, 7, 1000, (1 << 31) - 1] {
                // SAFETY: the context is entered.
                let atom = unsafe { qjs::JS_NewAtomUInt32(raw, i) };
                assert_eq!(index_of(atom), Some(i));
            }
            // SAFETY: as above.
            let name = unsafe { qjs::JS_NewAtom(raw, c"length".as_ptr()) };
            assert_eq!(index_of(name), None);
            // SAFETY: as above; the atom is the one made.
            unsafe { qjs::JS_FreeAtom(raw, name) };
        });
    }
}
