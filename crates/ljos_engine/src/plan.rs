//! The component plan (docs/adr/0002): what the objects and expressions of
//! a loaded file resolve to against the native types, linked the first
//! time an instance needs it and kept beside the [`LoadedFile`], so every
//! later instance writes and binds by index and a reload drops it with
//! the file.
//!
//! A [`Slot`] keeps the type it was linked against: an object whose type
//! is another (a reload that made the type anew, or one function shared
//! by expressions of objects of other types) links it again rather than
//! taking an index that means something else there.

use std::cell::{Cell, OnceCell, RefCell};

use ljos_ir::{CompiledFile, Members};

use crate::engine::Engine;
use crate::ids::{ObjectId, PropIndex};
use crate::meta::{MetaObject, MetaRef, PropertyFlags};

/// One name of one place, linked to a property index.
#[derive(Default)]
pub(crate) struct Slot(RefCell<Option<(MetaRef, PropIndex)>>);

impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.borrow().is_some() {
            "Slot(linked)"
        } else {
            "Slot"
        })
    }
}

impl Clone for Slot {
    // A copy links on its own: what it was linked to may not hold where
    // the copy is used.
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Slot {
    /// The index `find` gives on `meta`, found once per type.
    pub(crate) fn link(
        &self,
        meta: &MetaRef,
        find: impl FnOnce(&MetaObject) -> Option<PropIndex>,
    ) -> Option<PropIndex> {
        if let Some(p) = self.hit(meta) {
            return Some(p);
        }
        let p = find(meta)?;
        *self.0.borrow_mut() = Some((meta.clone(), p));
        Some(p)
    }

    fn hit(&self, meta: &MetaObject) -> Option<PropIndex> {
        match &*self.0.borrow() {
            Some((m, p)) if std::ptr::eq::<MetaObject>(&**m, meta) => Some(*p),
            _ => None,
        }
    }
}

/// The names of a [`Members`], linked: one slot for each assignment,
/// object assignment and binding, in their order.
pub(crate) struct MembersPlan {
    /// The object of the component the members are written on.
    pub(crate) obj: u32,
    /// The place of the first binding among all the object's (its own,
    /// then each group's), which [`crate::object::Made::bound`] counts in.
    pub(crate) first: u32,
    pub(crate) assignments: Box<[Slot]>,
    pub(crate) objects: Box<[Slot]>,
    pub(crate) bindings: Box<[Slot]>,
}

impl MembersPlan {
    fn of(m: &Members, obj: usize, first: &mut u32) -> Self {
        let slots = |n: usize| (0..n).map(|_| Slot::default()).collect();
        let at = *first;
        *first += m.bindings.len() as u32;
        Self {
            obj: obj as u32,
            first: at,
            assignments: slots(m.assignments.len()),
            objects: slots(m.objects.len()),
            bindings: slots(m.bindings.len()),
        }
    }
}

/// A group (`anchors.fill`, `font.pixelSize`) of an object.
pub(crate) struct GroupPlan {
    /// The property the group's one-step path names on the object.
    pub(crate) head: Slot,
    pub(crate) members: MembersPlan,
}

/// One object of a component.
pub(crate) struct ObjectPlan {
    pub(crate) members: MembersPlan,
    pub(crate) grouped: Box<[GroupPlan]>,
    /// The default property of the type the object is written as, which
    /// its children go to.
    pub(crate) children_default: Slot,
    /// The most graph nodes an object made from this declaration held in
    /// its scope once complete: the next one's scope is made that size.
    pub(crate) nodes: Cell<u32>,
}

/// One parameter of an expression.
#[derive(Default)]
pub(crate) struct ArgPlan {
    /// The property of [`ljos_ir::Arg::Property`].
    pub(crate) prop: Slot,
    /// The type of [`ljos_ir::Arg::Attached`].
    pub(crate) meta: OnceCell<Option<MetaRef>>,
}

/// A file's plans. Components and expressions link on first use.
pub(crate) struct Plans {
    components: Box<[OnceCell<Box<[ObjectPlan]>>]>,
    exprs: Box<[OnceCell<Box<[ArgPlan]>>]>,
}

impl Plans {
    pub(crate) fn new(ir: &CompiledFile) -> Self {
        Self {
            components: ir.components.iter().map(|_| OnceCell::new()).collect(),
            exprs: ir.exprs.iter().map(|_| OnceCell::new()).collect(),
        }
    }

    /// The plan of object `obj` of component `comp`.
    pub(crate) fn object(&self, ir: &CompiledFile, comp: u32, obj: usize) -> Option<&ObjectPlan> {
        let objects = self.components.get(comp as usize)?.get_or_init(|| {
            ir.components[comp as usize]
                .objects
                .iter()
                .enumerate()
                .map(|(k, od)| {
                    let mut first = 0;
                    ObjectPlan {
                        members: MembersPlan::of(&od.members, k, &mut first),
                        grouped: od
                            .grouped
                            .iter()
                            .map(|g| GroupPlan {
                                head: Slot::default(),
                                members: MembersPlan::of(&g.members, k, &mut first),
                            })
                            .collect(),
                        children_default: Slot::default(),
                        nodes: Cell::new(0),
                    }
                })
                .collect()
        });
        objects.get(obj)
    }

    /// The parameters of expression `expr`.
    pub(crate) fn args(&self, ir: &CompiledFile, expr: usize) -> &[ArgPlan] {
        match self.exprs.get(expr) {
            Some(c) => c.get_or_init(|| {
                (0..ir.exprs[expr].params.len())
                    .map(|_| ArgPlan::default())
                    .collect()
            }),
            None => &[],
        }
    }

    /// About the bytes the plans take, for the file's size.
    pub(crate) fn bytes(&self) -> usize {
        self.components.len() * size_of::<OnceCell<Box<[ObjectPlan]>>>()
            + self.exprs.len() * size_of::<OnceCell<Box<[ArgPlan]>>>()
    }
}

impl Engine {
    /// `slot` linked on the type `obj` has; `find` looks the index up the
    /// first time. `None` for a dead object or what `find` finds nothing
    /// for, which is the caller's to say why.
    pub(crate) fn linked(
        &self,
        obj: ObjectId,
        slot: &Slot,
        find: impl FnOnce(&MetaObject) -> Option<PropIndex>,
    ) -> Option<PropIndex> {
        let meta = {
            let store = self.inner.store.borrow();
            let meta = &store.objects.get(obj)?.meta;
            if let Some(p) = slot.hit(meta) {
                return Some(p);
            }
            meta.clone()
        };
        // Not under the store's borrow: finding may load a file.
        slot.link(&meta, find)
    }

    /// The property `name` of `obj`, through `slot`.
    pub(crate) fn linked_property(
        &self,
        obj: ObjectId,
        slot: &Slot,
        name: &str,
    ) -> Option<PropIndex> {
        self.linked(obj, slot, |m| m.property_index(name))
    }

    /// The flags of property `prop` of `obj`; empty for a dead object.
    pub(crate) fn property_flags(&self, obj: ObjectId, prop: PropIndex) -> PropertyFlags {
        let store = self.inner.store.borrow();
        store
            .objects
            .get(obj)
            .and_then(|h| h.meta.property(prop))
            .map_or(PropertyFlags::empty(), |p| p.flags)
    }

    /// Whether `path` is one step to a group of `obj`'s own, through
    /// `head`: a group (`anchors`) and not an alias of one elsewhere.
    pub(crate) fn own_group(&self, obj: ObjectId, path: &[String], head: &Slot) -> bool {
        let [name] = path else { return false };
        self.linked_property(obj, head, name).is_some_and(|p| {
            let f = self.property_flags(obj, p);
            f.contains(PropertyFlags::GROUP) && !f.contains(PropertyFlags::ALIAS)
        })
    }
}
