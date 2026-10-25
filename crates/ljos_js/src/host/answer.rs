//! A wrapper answers a read or a write of its type's properties itself.
//!
//! The prototype of a type keeps an accessor per property, and QuickJS
//! reaches one by calling a function: the call, its frame and its argument
//! cost more than the read the host then does. The wrappers' class is
//! exotic instead: QuickJS asks it for any name the wrapper does not hold
//! itself, and it looks the name up in its type's slots
//! ([`Listing::slots`](super::Listing)) and reads or writes the property
//! with no call. A name it has no slot for is looked up along the
//! prototypes as QuickJS would, with the wrapper as the receiver, so
//! methods, signals and everything of `Object.prototype` read as before.
//! The prototype keeps its accessors for what asks the prototype itself.

use std::ffi::c_int;
use std::mem::MaybeUninit;

use rquickjs::{Ctx, Exception, qjs};

use super::{Slot, prop_get, prop_set};
use crate::objects::HostWrapper;

pub(super) static EXOTIC: qjs::JSClassExoticMethods = qjs::JSClassExoticMethods {
    get_own_property: None,
    get_own_property_names: None,
    delete_property: None,
    define_own_property: None,
    has_property: None,
    get_property: Some(get),
    set_property: Some(set),
};

/// The slot `obj`'s type has for `atom`, and the id of the object the
/// receiver wraps when it is `obj` itself, as it is unless `obj` is the
/// prototype of another object.
///
/// # Safety
/// `obj` is a wrapper: QuickJS calls the class's methods only with one.
#[inline]
unsafe fn slot(
    obj: qjs::JSValue,
    atom: qjs::JSAtom,
    receiver: qjs::JSValue,
) -> Option<(Slot, Option<u64>)> {
    let mut class = 0;
    // SAFETY: the opaque of the class is a live `HostWrapper`, or null.
    unsafe {
        let w = qjs::JS_GetAnyOpaque(obj, &raw mut class).cast::<HostWrapper>();
        if w.is_null() {
            return None;
        }
        let slot = (*w).listing.as_ref()?.slots.get(&atom).copied()?;
        let own = qjs::JS_VALUE_GET_TAG(receiver) == qjs::JS_TAG_OBJECT
            && qjs::JS_VALUE_GET_PTR(obj) == qjs::JS_VALUE_GET_PTR(receiver);
        Some((slot, own.then_some((*w).id)))
    }
}

/// Whether a write of a name `obj`'s type has not is an error: `obj` is
/// the receiver, not the prototype of another object, and its type is a
/// QML object's.
///
/// # Safety
/// As [`slot`].
unsafe fn closed(obj: qjs::JSValue, receiver: qjs::JSValue) -> bool {
    let mut class = 0;
    // SAFETY: as in `slot`.
    unsafe {
        let w = qjs::JS_GetAnyOpaque(obj, &raw mut class).cast::<HostWrapper>();
        !w.is_null()
            && (*w).listing.as_ref().is_some_and(|l| l.closed)
            && qjs::JS_VALUE_GET_TAG(receiver) == qjs::JS_TAG_OBJECT
            && qjs::JS_VALUE_GET_PTR(obj) == qjs::JS_VALUE_GET_PTR(receiver)
    }
}

unsafe extern "C" fn get(
    ctx: *mut qjs::JSContext,
    obj: qjs::JSValue,
    atom: qjs::JSAtom,
    receiver: qjs::JSValue,
) -> qjs::JSValue {
    // SAFETY: QuickJS calls with a live context and a wrapper.
    unsafe {
        match slot(obj, atom, receiver) {
            Some((s, id)) => prop_get(ctx, receiver, id, s.prop),
            None => proto_get(ctx, obj, atom, receiver),
        }
    }
}

unsafe extern "C" fn set(
    ctx: *mut qjs::JSContext,
    obj: qjs::JSValue,
    atom: qjs::JSAtom,
    value: qjs::JSValue,
    receiver: qjs::JSValue,
    flags: c_int,
) -> c_int {
    // SAFETY: QuickJS calls with a live context and a wrapper; `value` is
    // borrowed, the caller frees it.
    unsafe {
        match slot(obj, atom, receiver) {
            Some((
                Slot {
                    writable: true,
                    prop,
                },
                id,
            )) => {
                if qjs::JS_IsException(prop_set(ctx, receiver, id, prop, value)) {
                    -1
                } else {
                    1
                }
            }
            Some(_) => refuse(ctx, flags, "no setter for property"),
            None => proto_set(
                ctx,
                obj,
                atom,
                value,
                receiver,
                flags,
                closed(obj, receiver),
            ),
        }
    }
}

/// A write that cannot be done: a TypeError where the write throws, as
/// every write of strict code does. Ljós runs scripts strict, and QuickJS
/// does not say whether the code writing is.
unsafe fn refuse(ctx: *mut qjs::JSContext, flags: c_int, message: &str) -> c_int {
    if flags & (qjs::JS_PROP_THROW | qjs::JS_PROP_THROW_STRICT) as c_int == 0 {
        return 0;
    }
    // SAFETY: a live context.
    let ctx = unsafe { Ctx::from_raw(std::ptr::NonNull::new_unchecked(ctx)) };
    let _ = Exception::throw_type(&ctx, message);
    -1
}

/// qml6's `Error` for a write of a property the object has not, which it
/// throws whether the code writing is strict or not.
unsafe fn missing(ctx: *mut qjs::JSContext, atom: qjs::JSAtom) -> c_int {
    // SAFETY: a live context; the name's value is owned by the `Value`.
    unsafe {
        let ctx = Ctx::from_raw(std::ptr::NonNull::new_unchecked(ctx));
        let name = rquickjs::Value::from_raw(
            ctx.clone(),
            qjs::JS_AtomToString(ctx.as_raw().as_ptr(), atom),
        );
        let text = name.get::<String>().unwrap_or_default();
        let _ = Exception::throw_message(
            &ctx,
            &format!("Cannot assign to non-existent property \"{text}\""),
        );
    }
    -1
}

/// The entry `atom` of the first of `obj`'s prototypes that has one, or
/// none: what QuickJS reads past an object with no own entry.
///
/// # Safety
/// A live context; the descriptor's values are the caller's to free.
unsafe fn find(
    ctx: *mut qjs::JSContext,
    obj: qjs::JSValue,
    atom: qjs::JSAtom,
) -> Result<Option<qjs::JSPropertyDescriptor>, ()> {
    // SAFETY: as the caller's; every prototype taken is freed.
    unsafe {
        let mut p = qjs::JS_GetPrototype(ctx, obj);
        loop {
            if qjs::JS_IsException(p) {
                return Err(());
            }
            if qjs::JS_VALUE_GET_TAG(p) != qjs::JS_TAG_OBJECT {
                return Ok(None);
            }
            let mut d = MaybeUninit::<qjs::JSPropertyDescriptor>::uninit();
            let got = qjs::JS_GetOwnProperty(ctx, d.as_mut_ptr(), p, atom);
            if got != 0 {
                qjs::JS_FreeValue(ctx, p);
                return if got < 0 {
                    Err(())
                } else {
                    Ok(Some(d.assume_init()))
                };
            }
            let next = qjs::JS_GetPrototype(ctx, p);
            qjs::JS_FreeValue(ctx, p);
            p = next;
        }
    }
}

fn is_accessor(d: &qjs::JSPropertyDescriptor) -> bool {
    d.flags & qjs::JS_PROP_GETSET as c_int != 0
}

unsafe fn proto_get(
    ctx: *mut qjs::JSContext,
    obj: qjs::JSValue,
    atom: qjs::JSAtom,
    receiver: qjs::JSValue,
) -> qjs::JSValue {
    // SAFETY: a live context; the descriptor's values are owned, and those
    // not given back are freed.
    unsafe {
        match find(ctx, obj, atom) {
            Err(()) => qjs::JS_EXCEPTION,
            Ok(None) => qjs::JS_UNDEFINED,
            Ok(Some(d)) if is_accessor(&d) => {
                qjs::JS_FreeValue(ctx, d.value);
                qjs::JS_FreeValue(ctx, d.setter);
                if qjs::JS_VALUE_GET_TAG(d.getter) != qjs::JS_TAG_OBJECT {
                    return qjs::JS_UNDEFINED;
                }
                let out = qjs::JS_Call(ctx, d.getter, receiver, 0, std::ptr::null_mut());
                qjs::JS_FreeValue(ctx, d.getter);
                out
            }
            Ok(Some(d)) => {
                qjs::JS_FreeValue(ctx, d.getter);
                qjs::JS_FreeValue(ctx, d.setter);
                d.value
            }
        }
    }
}

unsafe fn proto_set(
    ctx: *mut qjs::JSContext,
    obj: qjs::JSValue,
    atom: qjs::JSAtom,
    mut value: qjs::JSValue,
    receiver: qjs::JSValue,
    flags: c_int,
    closed: bool,
) -> c_int {
    // SAFETY: as in `proto_get`; `value` is borrowed.
    unsafe {
        match find(ctx, obj, atom) {
            Err(()) => return -1,
            Ok(Some(d)) if is_accessor(&d) => {
                qjs::JS_FreeValue(ctx, d.value);
                qjs::JS_FreeValue(ctx, d.getter);
                if qjs::JS_VALUE_GET_TAG(d.setter) != qjs::JS_TAG_OBJECT {
                    return refuse(ctx, flags, "no setter for property");
                }
                let out = qjs::JS_Call(ctx, d.setter, receiver, 1, &raw mut value);
                qjs::JS_FreeValue(ctx, d.setter);
                if qjs::JS_IsException(out) {
                    return -1;
                }
                qjs::JS_FreeValue(ctx, out);
                return 1;
            }
            Ok(Some(d)) => {
                qjs::JS_FreeValue(ctx, d.value);
                qjs::JS_FreeValue(ctx, d.getter);
                qjs::JS_FreeValue(ctx, d.setter);
                if d.flags & qjs::JS_PROP_WRITABLE as c_int == 0 {
                    return refuse(ctx, flags, "property is read-only");
                }
            }
            Ok(None) if closed => return missing(ctx, atom),
            Ok(None) => {}
        }
        // A new entry of the receiver's own, or the one it has when it is
        // not the wrapper but an object made over it.
        if qjs::JS_VALUE_GET_TAG(receiver) != qjs::JS_TAG_OBJECT {
            return refuse(ctx, flags, "not an object");
        }
        let mut d = MaybeUninit::<qjs::JSPropertyDescriptor>::uninit();
        match qjs::JS_GetOwnProperty(ctx, d.as_mut_ptr(), receiver, atom) {
            0 => {
                let all = qjs::JS_PROP_C_W_E | qjs::JS_PROP_THROW;
                qjs::JS_DefinePropertyValue(
                    ctx,
                    receiver,
                    atom,
                    qjs::JS_DupValue(ctx, value),
                    all as c_int,
                )
            }
            got if got < 0 => -1,
            _ => {
                let d = d.assume_init();
                qjs::JS_FreeValue(ctx, d.value);
                qjs::JS_FreeValue(ctx, d.getter);
                qjs::JS_FreeValue(ctx, d.setter);
                if is_accessor(&d) || d.flags & qjs::JS_PROP_WRITABLE as c_int == 0 {
                    return refuse(ctx, flags, "property is read-only");
                }
                let keep = qjs::JS_PROP_HAS_VALUE | qjs::JS_PROP_THROW;
                qjs::JS_DefineProperty(
                    ctx,
                    receiver,
                    atom,
                    value,
                    qjs::JS_UNDEFINED,
                    qjs::JS_UNDEFINED,
                    keep as c_int,
                )
            }
        }
    }
}
