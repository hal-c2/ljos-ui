//! Small things every part of the crate uses.

use ljos_engine::{Engine, EngineError, LogLevel, ObjectId, PropIndex, Provenance, QmlType, Value};

/// The category of this crate's log lines.
pub const CATEGORY: &str = "qt.quick.controls";

/// Whether nothing has given the property a value: neither the author nor
/// a binding. A value this crate derived for it does not count.
pub(crate) fn is_unset(engine: &Engine, obj: ObjectId, p: PropIndex) -> bool {
    matches!(
        engine.provenance(obj, p),
        Some(Provenance::Default | Provenance::Animated) | None
    )
}

/// Reads a number (tracked).
pub(crate) fn num(engine: &Engine, obj: ObjectId, p: PropIndex) -> f64 {
    engine.read(obj, p).to_number()
}

/// Reads a boolean (tracked).
pub(crate) fn flag(engine: &Engine, obj: ObjectId, p: PropIndex) -> bool {
    engine.read(obj, p).to_boolean()
}

/// Reads an object reference (tracked); a destroyed object reads as none.
pub(crate) fn object(engine: &Engine, obj: ObjectId, p: PropIndex) -> Option<ObjectId> {
    engine
        .read(obj, p)
        .as_object()
        .filter(|o| engine.is_alive(*o))
}

/// Whether the object's type derives from `T`.
pub(crate) fn is_a<T: QmlType>(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .meta_of(obj)
        .is_some_and(|m| m.inherits(T::static_meta()))
}

pub(crate) fn warn(engine: &Engine, what: impl Into<String>) {
    engine.log(LogLevel::Warning, CATEGORY, what.into());
}

/// Logs what went wrong where a hook cannot return it.
pub(crate) fn check(engine: &Engine, what: &str, r: Result<(), EngineError>) {
    if let Err(err) = r {
        warn(engine, format!("{what}: {err}"));
    }
}

/// Makes `prop` follow `f` unless it was given a value. Assigning the
/// property later replaces the binding, as assigning any bound property
/// does.
pub(crate) fn bind_default(
    engine: &Engine,
    obj: ObjectId,
    prop: PropIndex,
    f: impl FnMut(&Engine) -> Value + 'static,
) -> Result<(), EngineError> {
    if is_unset(engine, obj, prop) {
        engine.bind(obj, prop, f)?;
    }
    Ok(())
}

/// Writes a value this crate computed, unless it is the value already.
pub(crate) fn set_computed(
    engine: &Engine,
    obj: ObjectId,
    prop: PropIndex,
    v: Value,
) -> Result<(), EngineError> {
    if engine.untrack(|| engine.read(obj, prop)).change_equals(&v) {
        return Ok(());
    }
    engine.write_internal(obj, prop, v, ljos_engine::WriteKind::Imperative)
}
