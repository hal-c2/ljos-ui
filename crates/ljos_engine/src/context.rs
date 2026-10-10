//! Component contexts: the scopes that map QML `id`s to objects.

use std::collections::HashMap;

use crate::atom::Atom;
use crate::ids::{ContextId, ObjectId};
use crate::value::Value;

/// One context. Each component instance gets one, whose parent is the
/// context it was created in; an `id` resolves in the nearest context that
/// defines it.
#[derive(Debug, Default)]
pub struct Context {
    pub parent: Option<ContextId>,
    pub ids: HashMap<Atom, ObjectId>,
    /// The object whose properties are in scope unqualified (the
    /// component's root, or a context object set by the host).
    pub context_object: Option<ObjectId>,
    /// Names the host puts in scope (`setContextProperty`).
    pub props: HashMap<Atom, Value>,
    /// Grows with every context made, so siblings can be told apart by
    /// age (`Component.onDestruction` runs the youngest first).
    pub serial: u64,
}
