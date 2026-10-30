//! `Contribution`: something the application adds to the Slots of a
//! name.
//!
//! A contribution with a `slot` is in the registry from completion until
//! it is destroyed, for whatever reason: its file was unloaded, a
//! boundary around it stopped its content for a fault, it was destroyed
//! by hand. One with no `slot` is shown only where a Slot lists it in
//! `contributions` (a block's own slot property).

use std::sync::atomic::{AtomicU64, Ordering};

use ljos_engine::{
    Atom, Cx, Engine, EngineError, ObjectId, QmlObject, QtObject, SignalIndex, Value, qml_impl,
};

use super::registry::Registry;

/// `Contribution.mode`.
pub mod contribution_mode {
    pub const APPEND: i32 = 0;
    pub const REPLACE: i32 = 1;
    pub const PREPEND: i32 = 2;
}

fn contribution_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    use contribution_mode as m;
    vec![(
        "Mode",
        crate::enums::keys(&[
            ("Append", m::APPEND),
            ("Replace", m::REPLACE),
            ("Prepend", m::PREPEND),
        ]),
    )]
}

/// Counts contributions as they are made: contributions of one `order`
/// show in the order they were written or made, which completion order
/// does not follow.
static MADE: AtomicU64 = AtomicU64::new(0);

/// `Contribution` (Ljos.Blocks).
#[derive(Debug, QmlObject)]
#[qml(
    module = "Ljos.Blocks",
    name = "Contribution",
    extends = QtObject,
    default = "delegate",
    enums = contribution_enums
)]
pub struct Contribution {
    #[qml(base)]
    pub object: QtObject,
    /// The name of the Slots it shows in.
    #[qml(notify)]
    pub slot: Atom,
    /// [`contribution_mode`].
    #[qml(notify, enumeration = "Mode")]
    pub mode: i32,
    /// What each Slot makes one item of.
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    /// Lower first; contributions of one order in the order they were made.
    #[qml(notify)]
    pub order: i32,
    #[qml(skip)]
    pub(super) made: u64,
}

impl Default for Contribution {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            slot: Atom::default(),
            mode: contribution_mode::APPEND,
            delegate: None,
            order: 0,
            made: MADE.fetch_add(1, Ordering::Relaxed),
        }
    }
}

pub(super) fn slot_of(e: &Engine, c: ObjectId) -> String {
    text(&e.read(c, Contribution::SLOT.index))
}

pub(super) fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.as_str().to_owned(),
        _ => String::new(),
    }
}

#[qml_impl]
impl Contribution {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let registry = Registry::of(e);
        let slot = e.untrack(|| slot_of(e, me));
        if !slot.is_empty() {
            registry.add(e, &slot, me);
        }
        let now = std::rc::Rc::new(std::cell::RefCell::new(slot));
        let moved = now.clone();
        e.watch(me, Contribution::SLOT.index, move |e, v| {
            let next = text(v);
            let old = moved.replace(next.clone());
            let registry = Registry::of(e);
            if !old.is_empty() {
                registry.remove(e, &old, me);
            }
            if !next.is_empty() {
                registry.add(e, &next, me);
            }
        })?;
        e.connect(me, SignalIndex::DESTROYED, move |e, _| {
            let registry = Registry::of(e);
            let name = now.borrow().clone();
            if name.is_empty() {
                // Listed only: what lists it hears through the empty name.
                registry.touch(e, "");
            } else {
                registry.remove(e, &name, me);
            }
        })?;
        Ok(())
    }
}
