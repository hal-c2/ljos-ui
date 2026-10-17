//! Generational ids for engine-owned things.

slotmap::new_key_type! {
    /// An object in the engine's store. Generational: an id outlives its
    /// object safely and then reads as `null`.
    pub struct ObjectId;

    /// A component context: the scope that maps `id`s to objects.
    pub struct ContextId;

    /// One signal connection.
    pub struct ConnectionId;
}

/// Index of a property in a type's flattened property table (inherited
/// properties first). Stable for a given `MetaObject`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PropIndex(pub u32);

/// Index of a signal in a type's flattened signal table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SignalIndex(pub u32);

impl SignalIndex {
    /// Every object's `destroyed`, as `QObject::destroyed`: a type that
    /// declares no `destroyed` of its own answers this index for the
    /// name. Its handlers run once, from [`Engine::destroy_now`], with the
    /// object as the argument and the object still alive; it is never
    /// emitted by hand.
    ///
    /// [`Engine::destroy_now`]: crate::Engine::destroy_now
    pub const DESTROYED: SignalIndex = SignalIndex(u32::MAX);
}

/// Index of a method in a type's flattened method table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MethodIndex(pub u32);

/// The engine-side `PropKey` the reactive graph does not have: one property
/// of one object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PropKey {
    pub object: ObjectId,
    pub prop: PropIndex,
}
