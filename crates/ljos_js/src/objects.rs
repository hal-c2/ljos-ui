use std::{cell::RefCell, rc::Rc};

use rquickjs::{
    Ctx, Exception, JsLifetime,
    class::{JsClass, Readable, Trace, Tracer},
    function::Constructor,
};

use crate::{JsArg, JsOut, Scope};

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
#[derive(Debug, Clone)]
pub struct MethodDef {
    pub name: String,
    pub id: u32,
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

    fn get<'js>(&self, scope: &Scope<'js>, obj: u64, prop: u32) -> Result<JsOut, HostError>;

    fn set<'js>(
        &self,
        scope: &Scope<'js>,
        obj: u64,
        prop: u32,
        value: JsArg,
    ) -> Result<(), HostError>;

    fn call<'js>(
        &self,
        scope: &Scope<'js>,
        obj: u64,
        method: u32,
        args: &[JsArg],
    ) -> Result<JsOut, HostError>;

    /// `obj.signal(args)` from JS.
    fn emit<'js>(
        &self,
        _scope: &Scope<'js>,
        _obj: u64,
        _signal: u32,
        _args: &[JsArg],
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
        _args: &[JsArg],
    ) -> Result<(), HostError> {
        Err(HostError::msg("this host does not support signals"))
    }

    /// `obj.signal.disconnect(args)`, with what was given to `connect`.
    fn disconnect<'js>(
        &self,
        _scope: &Scope<'js>,
        _obj: u64,
        _signal: u32,
        _args: &[JsArg],
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

/// The JS class behind every wrapper. Its `Drop` is the finaliser: QuickJS
/// frees it from inside the collector where running Rust callbacks (or JS) is
/// not allowed, so it only queues the id.
pub(crate) struct HostWrapper {
    pub(crate) id: u64,
    /// Where its collection is told. The cache keeps an Engine-owned
    /// wrapper alive, so it is collected only once the cache dropped it.
    pub(crate) release_queue: Option<Rc<RefCell<Vec<u64>>>>,
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

impl<'js> Trace<'js> for HostWrapper {
    fn trace<'a>(&self, _tracer: Tracer<'a, 'js>) {}
}

// SAFETY: HostWrapper holds no JS values, so lifetime changes are trivially
// sound.
unsafe impl<'js> JsLifetime<'js> for HostWrapper {
    type Changed<'to> = HostWrapper;
}

impl<'js> JsClass<'js> for HostWrapper {
    const NAME: &'static str = "LjosHostObject";
    type Mutable = Readable;

    fn constructor(_ctx: &Ctx<'js>) -> rquickjs::Result<Option<Constructor<'js>>> {
        // Not constructible from script: wrappers come from the Engine.
        Ok(None)
    }
}
