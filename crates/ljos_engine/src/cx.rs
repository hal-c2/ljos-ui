//! [`Cx`]: what a native method or hook gets to reach the engine.

use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex, SignalIndex};
use crate::meta::Prop;
use crate::object::QmlType;
use crate::types::QmlValue;
use crate::value::Value;

/// The engine plus the object a native method was called on.
///
/// Native methods either borrow their struct (`&self`/`&mut self`) or take
/// a `Cx`, never both: a method holding `&mut self` would hold the object
/// store's borrow, and anything it did through the engine (a write that
/// runs a handler that reads this object) would re-enter it. With a `Cx`,
/// the method borrows its fields for as long as each [`Cx::with`] call
/// lasts and nothing is borrowed while user code runs.
#[derive(Clone, Copy, Debug)]
pub struct Cx<'e> {
    pub engine: &'e Engine,
    pub this: ObjectId,
}

impl<'e> Cx<'e> {
    pub fn new(engine: &'e Engine, this: ObjectId) -> Self {
        Cx { engine, this }
    }

    /// Borrows this object's `T` part. `f` must not call back into the
    /// engine.
    pub fn with<T: QmlType, R>(&self, f: impl FnOnce(&T) -> R) -> Result<R, EngineError> {
        self.engine.with(self.this, f)
    }

    /// Mutably borrows this object's `T` part. Field changes made here are
    /// not seen by bindings until [`Cx::notify`] (or use [`Cx::set`]).
    pub fn with_mut<T: QmlType, R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R, EngineError> {
        self.engine.with_mut(self.this, f)
    }

    /// Reads a property (tracked inside bindings).
    pub fn get<T: QmlValue>(&self, p: Prop<T>) -> Result<T, EngineError> {
        self.engine.get(self.this, p)
    }

    /// Writes a property from native code: read-only properties are
    /// writable here, as they are from C++ in Qt.
    pub fn set<T: QmlValue>(&self, p: Prop<T>, v: T) -> Result<(), EngineError> {
        self.engine.set_internal(self.this, p, v)
    }

    /// Reads a property by name.
    pub fn read(&self, name: &str) -> Value {
        self.engine.read_by_name(self.this, name)
    }

    /// Reports that a field changed behind the engine's back.
    pub fn notify(&self, prop: PropIndex) -> Result<(), EngineError> {
        self.engine.notify(self.this, prop)
    }

    /// Emits one of this object's signals.
    pub fn emit(&self, signal: SignalIndex, args: &[Value]) -> Result<(), EngineError> {
        self.engine.emit(self.this, signal, args)
    }
}
