use std::sync::{Arc, Mutex};

/// The engine-neutral value that crosses the Rust/JS boundary.
///
/// This is deliberately small. The Engine's own `Value` maps onto it. JS
/// values with identity that are not host objects (functions, plain objects)
/// cross as a [`JsRef`]; value types such as `point` go into JS as a
/// [`JsValue::Map`] (a fresh plain object) unless the embedder has said how
/// they look ([`JsValue::Typed`]). A colour compares by value in QML and
/// every embedder has it, so it has a variant of its own.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum JsValue {
    #[default]
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsValue>),
    /// Handle of an Engine object. JS sees the cached wrapper for it.
    Object(u64),
    /// A JS value kept alive on the Rust side: a function, a plain object.
    Handle(JsRef),
    /// Into JS only: a new plain object with these fields, in order.
    Map(Vec<(String, JsValue)>),
    /// A colour: red, green, blue and alpha, 16 bits each, not
    /// premultiplied. JS sees one frozen object per distinct colour, so two
    /// equal colours are `===` there as well.
    Color([u16; 4]),
    /// A value of a kind the embedder defined
    /// ([`crate::JsHost::define_value_kind`]): what its fields hold, in the
    /// kind's order. What JS sees of it is the kind's business; a kind that
    /// keeps one object per distinct value makes equal values `===`.
    Typed(ValueKind, Vec<JsValue>),
}

/// Names a kind of value defined with [`crate::JsHost::define_value_kind`].
/// It only means something to the host that gave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueKind(pub(crate) u32);

impl ValueKind {
    /// The kind a host defined after `index` others. An embedder that
    /// defines its kinds in a fixed order can name them as constants and
    /// check them against what the host returns.
    pub const fn nth(index: u32) -> Self {
        ValueKind(index)
    }

    /// The kinds of one host are numbered from zero in the order they were
    /// defined, so an embedder can keep a table beside them.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A strong reference to a JS value held by Rust (a function given to
/// `connect`, the object behind a `var` property).
///
/// It is `Send + Sync` so it can live inside values that cross threads, but
/// it only means something to the host that made it. Dropping the last clone
/// queues the slot for release; the host frees it the next time it runs, so
/// a drop never touches the JS heap directly (it may happen anywhere,
/// including after the host is gone).
#[derive(Clone)]
pub struct JsRef(Arc<RefSlot>);

struct RefSlot {
    key: u64,
    release: Arc<Mutex<Vec<u64>>>,
}

impl Drop for RefSlot {
    fn drop(&mut self) {
        // A poisoned queue only happens after a panic elsewhere; leaking the
        // slot until the host drops is the safe outcome then.
        if let Ok(mut q) = self.release.lock() {
            q.push(self.key);
        }
    }
}

impl JsRef {
    pub(crate) fn new(key: u64, release: Arc<Mutex<Vec<u64>>>) -> Self {
        Self(Arc::new(RefSlot { key, release }))
    }

    pub(crate) fn key(&self) -> u64 {
        self.0.key
    }

    /// Whether both are the same reference (not merely the same JS value:
    /// two refs taken from one function at different times differ; use
    /// `Scope::same_value` for JS identity).
    pub fn ptr_eq(a: &JsRef, b: &JsRef) -> bool {
        Arc::ptr_eq(&a.0, &b.0)
    }

    /// A number unique among the live refs of one host, for hashing and
    /// debugging.
    pub fn id(&self) -> u64 {
        self.0.key
    }
}

impl PartialEq for JsRef {
    fn eq(&self, other: &Self) -> bool {
        JsRef::ptr_eq(self, other)
    }
}

impl std::fmt::Debug for JsRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JsRef({:#x})", self.0.key)
    }
}

/// A value passed into JS.
pub type JsArg = JsValue;
/// A value coming out of JS.
pub type JsOut = JsValue;

impl From<bool> for JsValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<f64> for JsValue {
    fn from(v: f64) -> Self {
        Self::Number(v)
    }
}

impl From<i32> for JsValue {
    fn from(v: i32) -> Self {
        Self::Number(f64::from(v))
    }
}

impl From<&str> for JsValue {
    fn from(v: &str) -> Self {
        Self::String(v.to_owned())
    }
}

impl From<String> for JsValue {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

impl<T: Into<JsValue>> From<Vec<T>> for JsValue {
    fn from(v: Vec<T>) -> Self {
        Self::Array(v.into_iter().map(Into::into).collect())
    }
}
