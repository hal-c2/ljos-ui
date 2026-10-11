//! Members written in one object that are another object's: what a
//! `PropertyChanges` is given (`width: 10` of its target, `box.width: 10`
//! of the object with that id).
//!
//! The creator does not know such a type. A crate built on the engine
//! says which types keep members ([`Engine::register_foreign_members`]),
//! and is handed each member the type itself does not have instead of the
//! "non-existent property" error. What it does with them, and when, is its
//! own business: the engine only keeps the expression runnable.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use crate::engine::Engine;
use crate::ids::ObjectId;
use crate::meta::MetaObject;
use crate::types::TypeTag;
use crate::value::Value;

/// A member written in an object of a type that keeps members for another
/// object.
#[derive(Debug, Clone)]
pub struct ForeignMember {
    /// The object the written path starts at when it starts with an id
    /// (`box.width`); `None` when the member is of whatever the keeping
    /// object aims at (`width`).
    pub object: Option<ObjectId>,
    /// The property, dotted for a group's (`border.width`).
    pub name: String,
    pub value: ForeignValue,
    /// Where it is written: `url:line:column`.
    pub at: String,
}

/// An expression given to a [`ForeignMember`]: called with the type of the
/// property it is for, it runs as any binding does (tier 0, tier 1 or JS)
/// and reads tracked. `None` when it failed (the fault is logged).
pub type ForeignBinding = Rc<dyn Fn(&Engine, &TypeTag) -> Option<Value>>;

/// What a [`ForeignMember`] was given.
#[derive(Clone)]
pub enum ForeignValue {
    /// A value written out.
    Value(Value),
    Binding(ForeignBinding),
}

impl fmt::Debug for ForeignValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value(v) => f.debug_tuple("Value").field(v).finish(),
            Self::Binding(_) => f.write_str("Binding(..)"),
        }
    }
}

/// Takes the members written in `keeper` that are not its own.
pub type ForeignKeeper = Rc<dyn Fn(&Engine, ObjectId, ForeignMember)>;

#[derive(Default)]
struct Keepers {
    by_type: RefCell<Vec<(&'static MetaObject, ForeignKeeper)>>,
}

impl Engine {
    /// Makes `meta` (and what derives from it) a type that keeps members
    /// for other objects: a member written in such an object that the type
    /// does not have goes to `keep` instead of failing the creation.
    pub fn register_foreign_members(
        &self,
        meta: &'static MetaObject,
        keep: impl Fn(&Engine, ObjectId, ForeignMember) + 'static,
    ) {
        let keepers = self.extension::<Keepers>();
        let mut all = keepers.by_type.borrow_mut();
        all.retain(|(m, _)| !std::ptr::eq(*m, meta));
        all.push((meta, Rc::new(keep)));
    }

    /// The keeper of `obj`'s type, when it has one.
    pub(crate) fn foreign_keeper(&self, obj: ObjectId) -> Option<ForeignKeeper> {
        let keepers = self.extension::<Keepers>();
        let all = keepers.by_type.borrow();
        if all.is_empty() {
            return None;
        }
        let meta = self.meta_of(obj)?;
        all.iter()
            .find(|(m, _)| meta.inherits(m))
            .map(|(_, f)| f.clone())
    }
}
