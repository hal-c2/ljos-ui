//! Where an animation's steps of one number property go, looked up once.
//!
//! An animation writes each of its properties every frame. The generic
//! write looks up the property, checks what it is, finds the native struct
//! that holds its field and goes through the setter, all to learn what is
//! the same every time. A [`NumberSlot`] keeps what that look found; a
//! write through it does only what can change from one frame to the next,
//! and says when the slot no longer fits, so the caller writes the generic
//! way instead.

use std::any::TypeId;

use ljos_reactive::NodeId;

use crate::engine::Engine;
use crate::ids::{ObjectId, PropIndex};
use crate::meta::{PropertyAccess, PropertyFlags, Setter};
use crate::object::QmlObject;
use crate::types::TypeTag;
use crate::value::Value;

/// See the module. Made by [`Engine::number_slot`].
#[derive(Clone, Copy, Debug)]
pub struct NumberSlot {
    obj: ObjectId,
    prop: PropIndex,
    node: NodeId,
    /// An `int` takes `Value::Int`, anything else `Value::Real`.
    int: bool,
    field: Field,
}

#[derive(Clone, Copy, Debug)]
enum Field {
    /// The setter of the native struct `depth` bases down the chain.
    Native {
        depth: u8,
        set: Setter,
    },
    Dynamic(u32),
}

impl Engine {
    /// A slot for an animation's writes of numbers to `prop` of `obj`: a
    /// number property with a node, no alias, no group, no filter on its
    /// writes and nothing still to be made. `None` when it is not one of
    /// those, or the object is gone.
    pub fn number_slot(&self, obj: ObjectId, prop: PropIndex) -> Option<NumberSlot> {
        let store = self.inner.store.borrow();
        let h = store.objects.get(obj)?;
        let p = h.meta.property(prop)?;
        let int = match p.ty {
            TypeTag::Real => false,
            TypeTag::Int => true,
            _ => return None,
        };
        if p.is_alias() || p.flags.contains(PropertyFlags::GROUP) || h.makes_on_read(prop) {
            return None;
        }
        let field = match p.access {
            PropertyAccess::Field {
                owner,
                set,
                write: None,
                ..
            } => Field::Native {
                depth: depth_of(&*h.native, owner)?,
                set,
            },
            PropertyAccess::Dynamic { slot } => Field::Dynamic(slot),
            _ => return None,
        };
        Some(NumberSlot {
            obj,
            prop,
            node: h.node(prop)?,
            int,
            field,
        })
    }

    /// An animation's write of `v` through `slot`, as
    /// [`Engine::write_internal`] with [`WriteKind::Animation`] would make
    /// it. `v` comes back, with nothing written, when the slot does not
    /// fit any more, `v` is not the property's own kind of number or no
    /// batch is open: the caller then writes it the generic way.
    ///
    /// [`WriteKind::Animation`]: ljos_reactive::WriteKind::Animation
    pub fn write_number(&self, slot: &NumberSlot, v: Value) -> Result<(), Value> {
        if !matches!(
            (slot.int, &v),
            (false, Value::Real(_)) | (true, Value::Int(_))
        ) {
            return Err(v);
        }
        let g = &self.inner.graph;
        if !g.batching() {
            return Err(v);
        }
        {
            let store = self.inner.store.borrow();
            let Some(h) = store.objects.get(slot.obj) else {
                return Err(v);
            };
            // The node is made once and stays for the object's life, but a
            // value to be made on read may come later.
            if h.node(slot.prop) != Some(slot.node) || h.makes_on_read(slot.prop) {
                return Err(v);
            }
        }
        // The node is the object's own and alive: the write cannot fail.
        let Ok(Some(cur)) = g.write_animated(slot.node, v) else {
            return Ok(());
        };
        let mut store = self.inner.store.borrow_mut();
        if let Some(h) = store.objects.get_mut(slot.obj) {
            match slot.field {
                Field::Native { depth, set } => {
                    if let Some(o) = nth_base(&mut *h.native, depth) {
                        let _ = set(o, &cur);
                    }
                }
                Field::Dynamic(i) => {
                    if let Some(s) = h.dynamic.get_mut(i as usize) {
                        *s = cur;
                    }
                }
            }
        }
        Ok(())
    }
}

/// How many bases down from `root` its native chain has a `ty`.
fn depth_of(root: &dyn QmlObject, ty: TypeId) -> Option<u8> {
    let mut cur = Some(root);
    let mut depth = 0u8;
    while let Some(o) = cur {
        if o.as_any().type_id() == ty {
            return Some(depth);
        }
        depth = depth.checked_add(1)?;
        cur = o.base();
    }
    None
}

fn nth_base(o: &mut dyn QmlObject, depth: u8) -> Option<&mut dyn QmlObject> {
    if depth == 0 {
        Some(o)
    } else {
        nth_base(o.base_mut()?, depth - 1)
    }
}
