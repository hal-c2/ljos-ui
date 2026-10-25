//! Component contexts: the scopes that map QML `id`s to objects.

use rustc_hash::FxHashMap;

use std::rc::Rc;

use crate::atom::Atom;
use crate::ids::{ContextId, ObjectId};
use crate::incubator::Instance;
use crate::value::Value;

/// One context. Each component instance gets one, whose parent is the
/// context it was created in; an `id` resolves in the nearest context that
/// defines it.
#[derive(Debug, Default)]
pub struct Context {
    pub parent: Option<ContextId>,
    pub ids: FxHashMap<Atom, ObjectId>,
    /// The object whose properties are in scope unqualified (the
    /// component's root, or a context object set by the host).
    pub context_object: Option<ObjectId>,
    /// Names the host puts in scope (`setContextProperty`).
    pub props: FxHashMap<Atom, Value>,
    /// Grows with every context made, so siblings can be told apart by
    /// age (`Component.onDestruction` runs the youngest first).
    pub serial: u64,
    /// The instance the context is made for: its ids are found by their
    /// slots ([`ljos_ir::ComponentDef::ids`]) instead of being written
    /// into `ids` one by one as each instance is made.
    pub(crate) slots: Option<IdSlots>,
}

/// See [`Context::slots`].
pub(crate) struct IdSlots(pub(crate) Rc<Instance>);

impl std::fmt::Debug for IdSlots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IdSlots")
    }
}

impl Context {
    /// The object `name` is an id of in this context.
    pub fn id(&self, name: &str) -> Option<ObjectId> {
        if let Some(o) = self.ids.get(name) {
            return Some(*o);
        }
        let inst = &self.slots.as_ref()?.0;
        let (_, at) = inst
            .file
            .ir
            .component(inst.comp)
            .ids
            .iter()
            .find(|(id, _)| id == name)?;
        inst.objects.get(at.index()).copied()
    }

    /// Writes the ids the slots stand for into `ids`, for what changes
    /// them one by one (a reload in place).
    pub(crate) fn note_ids(&mut self) {
        let Some(IdSlots(inst)) = self.slots.take() else {
            return;
        };
        for (id, at) in &inst.file.ir.component(inst.comp).ids {
            if let Some(&o) = inst.objects.get(at.index()) {
                self.ids.entry(Atom::intern(id)).or_insert(o);
            }
        }
    }
}
