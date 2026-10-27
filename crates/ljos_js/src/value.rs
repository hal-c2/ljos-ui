use std::sync::{Arc, Mutex, OnceLock, Weak};

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
    /// The list the property `prop` of the Engine object `obj` holds, as
    /// a script reads it: a list object whose length and entries are the
    /// property's whenever they are read ([`crate::HostObjects::list_len`]).
    /// A new object each time it crosses into JS; what comes back of one.
    LiveList {
        obj: u64,
        prop: u32,
    },
    /// A colour: red, green, blue and alpha, 16 bits each, not
    /// premultiplied. JS sees one frozen object per distinct colour, so two
    /// equal colours are `===` there as well.
    Color([u16; 4]),
    /// An invalid colour (one never given) with these channels: `valid`
    /// is false, and it is a different object from any valid colour.
    InvalidColor([u16; 4]),
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
/// `connect`, the object or array behind a `var` property).
///
/// It is `Send + Sync` so it can live inside values that cross threads, but
/// it only means something to the host that made it. Dropping the last clone
/// queues the slot for release; the host frees it the next time it runs, so
/// a drop never touches the JS heap directly (it may happen anywhere,
/// including after the host is gone).
///
/// The host gives one reference per JS object for as long as one is held,
/// so two references are equal exactly when they are the same JS object.
///
/// An array can also be made on the Rust side ([`JsRef::array`]) before any
/// script has seen it: it becomes a JS array the first time a script gets
/// it, and is that array from then on. An embedder whose values are arrays
/// that no script reads never starts a JS host for them.
#[derive(Clone)]
pub struct JsRef(Arc<RefSlot>);

pub(crate) struct RefSlot {
    /// Where the value is in the host's table, once it is in the JS heap.
    live: OnceLock<Live>,
    /// What a value made on the Rust side holds until a script gets it.
    pending: Mutex<Option<Pending>>,
    array: bool,
    /// What [`JsRef::array_of_kind`] was given.
    kind: Option<Arc<str>>,
}

/// What a [`JsRef`] made on the Rust side becomes in JS.
pub(crate) enum Pending {
    Array(Vec<JsValue>),
    Object(Vec<(String, JsValue)>),
}

struct Live {
    key: u64,
    release: Arc<Mutex<Vec<u64>>>,
}

impl Drop for RefSlot {
    fn drop(&mut self) {
        let Some(live) = self.live.get() else {
            return;
        };
        // A poisoned queue only happens after a panic elsewhere; leaking the
        // slot until the host drops is the safe outcome then.
        if let Ok(mut q) = live.release.lock() {
            q.push(live.key);
        }
    }
}

/// A [`JsRef`] that does not keep the value alive, for the host's table of
/// the references it has given out.
pub(crate) struct WeakRef(Weak<RefSlot>);

impl WeakRef {
    pub(crate) fn upgrade(&self) -> Option<JsRef> {
        self.0.upgrade().map(JsRef)
    }
}

impl JsRef {
    pub(crate) fn new(key: u64, release: Arc<Mutex<Vec<u64>>>, array: bool) -> Self {
        let live = OnceLock::new();
        let _ = live.set(Live { key, release });
        Self(Arc::new(RefSlot {
            live,
            pending: Mutex::new(None),
            array,
            kind: None,
        }))
    }

    /// An array of `items` that no script has seen yet. It has an identity
    /// already: every clone is the same array, in Rust and later in JS.
    pub fn array(items: Vec<JsValue>) -> Self {
        Self(Arc::new(RefSlot {
            live: OnceLock::new(),
            pending: Mutex::new(Some(Pending::Array(items))),
            array: true,
            kind: None,
        }))
    }

    /// [`JsRef::array`], which once it is made in JS is handed with `kind`
    /// to the global function `__ljos_arrayMade(array, kind)`, if a host
    /// defined one, and is what that returns: the host's kind of array, as
    /// a copy of a typed list is one that makes entries of its type.
    pub fn array_of_kind(items: Vec<JsValue>, kind: &str) -> Self {
        Self(Arc::new(RefSlot {
            live: OnceLock::new(),
            pending: Mutex::new(Some(Pending::Array(items))),
            array: true,
            kind: Some(kind.into()),
        }))
    }

    /// A plain object with these fields, in order, that no script has seen
    /// yet: as [`JsRef::array`], every clone is the same object.
    pub fn object(fields: Vec<(String, JsValue)>) -> Self {
        Self(Arc::new(RefSlot {
            live: OnceLock::new(),
            pending: Mutex::new(Some(Pending::Object(fields))),
            array: false,
            kind: None,
        }))
    }

    /// Whether this is a JS array (and not a proxy of one): what a script
    /// changes in place, and what Rust reads with `Scope::array_items`.
    pub fn is_array(&self) -> bool {
        self.0.array
    }

    /// The kind of array this is to the host ([`JsRef::array_of_kind`]).
    pub fn kind(&self) -> Option<&str> {
        self.0.kind.as_deref()
    }

    /// The entries of an array no script has seen yet; `None` once one
    /// has, because the script may have changed it since.
    pub fn pending_items(&self) -> Option<Vec<JsValue>> {
        if self.0.live.get().is_some() {
            return None;
        }
        match self.0.pending.lock().ok()?.as_ref()? {
            Pending::Array(items) => Some(items.clone()),
            Pending::Object(_) => None,
        }
    }

    /// [`JsRef::pending_items`] without the copy: `f` sees the entries of
    /// an array no script has seen yet.
    pub fn with_pending<R>(&self, f: impl FnOnce(&[JsValue]) -> R) -> Option<R> {
        if self.0.live.get().is_some() {
            return None;
        }
        match self.0.pending.lock().ok()?.as_ref()? {
            Pending::Array(items) => Some(f(items)),
            Pending::Object(_) => None,
        }
    }

    pub(crate) fn key(&self) -> Option<u64> {
        self.0.live.get().map(|l| l.key)
    }

    /// Takes what to build the JS value from.
    pub(crate) fn take_pending(&self) -> Option<Pending> {
        self.0.pending.lock().ok()?.take()
    }

    /// Puts back what [`JsRef::take_pending`] took, when the array could
    /// not be built after all.
    pub(crate) fn restore_pending(&self, items: Pending) {
        if let Ok(mut p) = self.0.pending.lock() {
            *p = Some(items);
        }
    }

    pub(crate) fn set_live(&self, key: u64, release: Arc<Mutex<Vec<u64>>>) {
        let _ = self.0.live.set(Live { key, release });
    }

    pub(crate) fn downgrade(&self) -> WeakRef {
        WeakRef(Arc::downgrade(&self.0))
    }

    /// Whether both are the same reference, which is whether they are the
    /// same JS value.
    pub fn ptr_eq(a: &JsRef, b: &JsRef) -> bool {
        Arc::ptr_eq(&a.0, &b.0)
    }

    /// A number unique among the live refs, for hashing and debugging. It
    /// stays what it is when an array made in Rust reaches the JS heap.
    pub fn id(&self) -> u64 {
        Arc::as_ptr(&self.0) as usize as u64
    }
}

impl PartialEq for JsRef {
    fn eq(&self, other: &Self) -> bool {
        JsRef::ptr_eq(self, other)
    }
}

impl std::fmt::Debug for JsRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.key() {
            Some(key) => write!(f, "JsRef({key:#x})"),
            None => f.write_str("JsRef(not in JS yet)"),
        }
    }
}

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
