use std::{cell::RefCell, rc::Rc};

use rquickjs::{Ctx, Exception, qjs};

use crate::{JsValue, Scope};

/// Identifies a registered host type (a prototype with accessors).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HostTypeId(pub(crate) u32);

/// One accessor property on a host type. `id` is opaque to this crate and is
/// handed back to [`HostObjects`]; the Engine uses its own property index.
#[derive(Debug, Clone)]
pub struct PropDef {
    pub name: String,
    pub id: u32,
    /// Without a setter, assignment throws a `TypeError` in strict code.
    pub writable: bool,
}

/// One method on a host type.
#[derive(Debug, Clone, Default)]
pub struct MethodDef {
    pub name: String,
    pub id: u32,
    /// The arguments it takes by copy, by position (bit 0 the first): an
    /// object or an array in one crosses as what [`Scope::copy_of`] makes
    /// of it, and holds nothing of the script's.
    ///
    /// [`Scope::copy_of`]: crate::Scope::copy_of
    pub copied: u32,
}

/// One signal on a host type. JS reads it as a function: calling it emits,
/// and it has `connect` and `disconnect` methods.
#[derive(Debug, Clone)]
pub struct SignalDef {
    pub name: String,
    pub id: u32,
}

/// The shape of one QML type as JS sees it. One prototype per type keeps the
/// QuickJS shapes stable (section 9.2).
#[derive(Debug, Clone, Default)]
pub struct TypeDef {
    pub name: String,
    pub props: Vec<PropDef>,
    pub methods: Vec<MethodDef>,
    pub signals: Vec<SignalDef>,
    /// Prints as a plain object (`[object Object]`), not as a QObject.
    pub plain: bool,
    /// The type whose prototype's entries this one's is given, listed
    /// before its own: only what differs is given here.
    pub base: Option<HostTypeId>,
}

/// Failure of a host callback.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// Becomes a JS `Error` with this message thrown at the access site.
    #[error("{0}")]
    Message(String),
    /// Becomes a JS `TypeError` with this message.
    #[error("{0}")]
    Type(String),
    /// A JS-side failure (typically `scope.call(..)?`) passed through. The
    /// pending JS exception is kept intact, so the original error and stack
    /// reach the outer `enter`.
    #[error("JS error: {0}")]
    Js(#[from] rquickjs::Error),
}

impl HostError {
    pub fn msg(m: impl Into<String>) -> Self {
        Self::Message(m.into())
    }

    pub fn type_error(m: impl Into<String>) -> Self {
        Self::Type(m.into())
    }

    pub(crate) fn into_js(self, ctx: &Ctx<'_>) -> rquickjs::Error {
        match self {
            Self::Message(m) => Exception::throw_message(ctx, &m),
            Self::Type(m) => Exception::throw_type(ctx, &m),
            Self::Js(e) => e,
        }
    }
}

/// A value [`HostObjects::get_plain`] and [`HostObjects::set_plain`]
/// pass: one QuickJS holds with no reference, or a text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlainValue {
    Number(f64),
    Bool(bool),
    /// A text, which `get_plain` gave to the reader it was handed. Never
    /// written with `set_plain`.
    Text,
}

/// What a script reads for a method of an object
/// ([`HostObjects::method_face`]).
#[derive(Debug)]
pub enum MethodFace<'js> {
    /// A function of the script's own to call instead, usually made with
    /// [`Scope::direct_method`]. The object keeps it.
    Direct(rquickjs::Function<'js>),
    /// A function that calls [`HostObjects::call`]. `keep`: the object
    /// keeps it; otherwise the next read asks again.
    Call { keep: bool },
}

/// What the Engine implements so that JS accessors reach the object store.
///
/// Methods take `&self` because they run while the Engine is mid-dispatch;
/// implementations use interior mutability. They must not call
/// [`crate::JsHost`] methods; to run JS from a callback use the [`Scope`],
/// which re-enters the already-locked context (re-entrancy is supported and
/// tested).
///
/// The host is stored as `Rc<dyn HostObjects>`. If the implementor can reach
/// the `JsHost` (it normally lives inside the Engine that owns it), give it a
/// `Weak` back-reference to avoid a cycle.
pub trait HostObjects {
    /// The type of object `obj`, used to pick a prototype when JS first sees
    /// it. `None` for dead objects.
    fn type_of(&self, obj: u64) -> Option<HostTypeId>;

    fn get<'js>(&self, scope: &Scope<'js>, obj: u64, prop: u32) -> Result<JsValue, HostError>;

    /// [`get`](Self::get) for a property that holds a number, a bool or a
    /// text, when the host can tell without the script's scope; `None`
    /// reads it with [`get`](Self::get). A text is handed to `text` rather
    /// than copied, and read as [`PlainValue::Text`]. It runs no script.
    fn get_plain(&self, _obj: u64, _prop: u32, _text: &mut dyn FnMut(&str)) -> Option<PlainValue> {
        None
    }

    fn set<'js>(
        &self,
        scope: &Scope<'js>,
        obj: u64,
        prop: u32,
        value: JsValue,
    ) -> Result<(), HostError>;

    /// [`set`](Self::set) of a number or a bool, when the host can do it
    /// without the script's scope; `None` writes it with
    /// [`set`](Self::set) instead, as though this had not been called.
    fn set_plain(
        &self,
        _obj: u64,
        _prop: u32,
        _value: PlainValue,
    ) -> Option<Result<(), HostError>> {
        None
    }

    /// How many entries the list the property `prop` of `obj` holds has
    /// now, for a script's [`JsValue::LiveList`] of it; 0 once the object
    /// is gone. A read, which a binding follows.
    fn list_len(&self, _obj: u64, _prop: u32) -> u32 {
        0
    }

    /// Entry `index` of that list; `None` past its end.
    fn list_entry(&self, _obj: u64, _prop: u32, _index: u32) -> Option<JsValue> {
        None
    }

    /// A script put `item` at the end of that list (`push`).
    fn list_append<'js>(
        &self,
        _scope: &Scope<'js>,
        _obj: u64,
        _prop: u32,
        _item: JsValue,
    ) -> Result<(), HostError> {
        Ok(())
    }

    /// A script set that list's length to `len`, which is shorter than
    /// it: the entries after it go.
    fn list_truncate<'js>(
        &self,
        _scope: &Scope<'js>,
        _obj: u64,
        _prop: u32,
        _len: u32,
    ) -> Result<(), HostError> {
        Ok(())
    }

    fn call<'js>(
        &self,
        scope: &Scope<'js>,
        obj: u64,
        method: u32,
        args: Vec<JsValue>,
    ) -> Result<JsValue, HostError>;

    /// What a script reads for `obj.method`, the first time it reads it.
    fn method_face<'js>(&self, _scope: &Scope<'js>, _obj: u64, _method: u32) -> MethodFace<'js> {
        MethodFace::Call { keep: true }
    }

    /// `obj.signal(args)` from JS.
    fn emit<'js>(
        &self,
        _scope: &Scope<'js>,
        _obj: u64,
        _signal: u32,
        _args: &[JsValue],
    ) -> Result<(), HostError> {
        Err(HostError::msg("this host does not support signals"))
    }

    /// `obj.signal.connect(args)`: usually one function, or an object and
    /// one of its methods.
    fn connect<'js>(
        &self,
        _scope: &Scope<'js>,
        _obj: u64,
        _signal: u32,
        _args: &[JsValue],
    ) -> Result<(), HostError> {
        Err(HostError::msg("this host does not support signals"))
    }

    /// `obj.signal.disconnect(args)`, with what was given to `connect`.
    fn disconnect<'js>(
        &self,
        _scope: &Scope<'js>,
        _obj: u64,
        _signal: u32,
        _args: &[JsValue],
    ) -> Result<(), HostError> {
        Err(HostError::msg("this host does not support signals"))
    }

    /// Whether an array a script gives this host (assigned to a property,
    /// passed to a method or a signal, returned from a function Rust
    /// called) crosses as a [`crate::JsValue::Handle`] to the script's own
    /// array instead of a copy of its entries. A host that says yes keeps
    /// the array's identity and sees what the script changes in it later;
    /// it reads the entries with [`Scope::array_items`].
    fn arrays_by_reference(&self) -> bool {
        false
    }

    /// A JS-owned wrapper (see [`Scope::new_owned`]) was garbage collected,
    /// or the wrapper of an object whose Engine-owned wrapper was dropped
    /// ([`crate::JsHost::drop_wrapper`]). Called between runs, never from inside
    /// the collector, so the Engine may destroy the object here.
    fn released(&self, _obj: u64) {}

    /// [`HostObjects::released`] for all the wrappers one collection
    /// freed, for a host that has work to do once for all of them.
    fn released_many(&self, objs: &[u64]) {
        for obj in objs {
            self.released(*obj);
        }
    }
}

/// What a wrapper object holds: QuickJS's class of its own
/// ([`register_wrapper_class`]), with this boxed as its opaque. Its `Drop`
/// is the finaliser: QuickJS frees it from inside the collector where
/// running Rust callbacks (or JS) is not allowed, so it only queues the id.
pub(crate) struct HostWrapper {
    pub(crate) id: u64,
    /// Where its collection is told. The cache keeps an Engine-owned
    /// wrapper alive, so it is collected only once the cache dropped it.
    pub(crate) release_queue: Option<Rc<RefCell<Vec<u64>>>>,
    /// Its type's names, which it answers for (host/answer.rs).
    pub(crate) listing: Option<Rc<crate::host::Listing>>,
}

impl Drop for HostWrapper {
    fn drop(&mut self) {
        if let Some(q) = &self.release_queue {
            // Draining uses `mem::take` and runs no JS while borrowed, so a
            // finaliser can never observe the queue borrowed.
            q.borrow_mut().push(self.id);
        }
    }
}

/// Registers the wrappers' class in `rt` and gives its id.
///
/// A class of its own rather than an rquickjs class: rquickjs gives every
/// Rust class one QuickJS class id and tells them apart by a table, two
/// lookups per access; here `JS_GetOpaque` with the id is the whole check.
///
/// `exotic` answers for the wrappers' properties; QuickJS keeps the
/// pointer and never writes through it.
///
/// # Safety
/// `rt` is a live runtime.
pub(crate) unsafe fn register_wrapper_class(
    rt: *mut qjs::JSRuntime,
    exotic: &'static qjs::JSClassExoticMethods,
) -> Option<qjs::JSClassID> {
    let mut id: qjs::JSClassID = 0;
    let def = qjs::JSClassDef {
        class_name: c"LjosHostObject".as_ptr(),
        finalizer: Some(finalize_wrapper),
        gc_mark: None,
        call: None,
        exotic: std::ptr::from_ref(exotic).cast_mut(),
    };
    // SAFETY: as the caller's; QuickJS copies the definition.
    unsafe {
        qjs::JS_NewClassID(rt, &raw mut id);
        (qjs::JS_NewClass(rt, id, &raw const def) == 0).then_some(id)
    }
}

unsafe extern "C" fn finalize_wrapper(_rt: *mut qjs::JSRuntime, val: qjs::JSValue) {
    let mut class = 0;
    // SAFETY: QuickJS finalises only objects of the class, whose opaque is
    // the box `new_wrapper` made, or null.
    unsafe {
        let p = qjs::JS_GetAnyOpaque(val, &raw mut class).cast::<HostWrapper>();
        if !p.is_null() {
            drop(Box::from_raw(p));
        }
    }
}

/// A new wrapper of `class` over `proto`, holding `w`.
///
/// # Safety
/// `ctx` is live and entered, `class` is [`register_wrapper_class`]'s id
/// in its runtime.
pub(crate) unsafe fn new_wrapper(
    ctx: *mut qjs::JSContext,
    class: qjs::JSClassID,
    proto: qjs::JSValue,
    w: HostWrapper,
) -> Option<qjs::JSValue> {
    // SAFETY: as the caller's; the object owns the box from here.
    unsafe {
        let o = qjs::JS_NewObjectProtoClass(ctx, proto, class);
        if qjs::JS_IsException(o) {
            return None;
        }
        qjs::JS_SetOpaque(o, Box::into_raw(Box::new(w)).cast());
        Some(o)
    }
}

/// The id of the object `v` wraps, if it is a wrapper of `class`.
///
/// # Safety
/// `class` is [`register_wrapper_class`]'s id in `v`'s runtime.
pub(crate) unsafe fn wrapper_id(class: qjs::JSClassID, v: qjs::JSValue) -> Option<u64> {
    // SAFETY: the opaque of the class is a live `HostWrapper`.
    unsafe {
        let p = qjs::JS_GetOpaque(v, class).cast::<HostWrapper>();
        (!p.is_null()).then(|| (*p).id)
    }
}
