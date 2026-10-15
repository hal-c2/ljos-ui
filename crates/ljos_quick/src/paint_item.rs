//! Items of other crates that paint content of their own.
//!
//! The scene knows how `Rectangle`, `Text` or `Image` paint. An item
//! type from another crate (a `VideoOutput`) names here how to read what
//! it shows. The scene calls `read` inside the item's content effect, so
//! what it reads is tracked and a change repaints the item, and paints the
//! result into the item's render node with the same display list as the
//! built-in items: other crates never see the renderer.

use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;

use ljos_engine::{Engine, MetaRef, ObjectId};
use ljos_render::{DisplayListBuilder, kurbo};

/// What an item shows, read from its properties.
pub trait ItemPaint: Debug {
    /// Paints into an item of `size`, in its own coordinates.
    fn paint(&self, size: kurbo::Size, out: &mut DisplayListBuilder);
}

/// Reads what `obj` shows. Tracked: whatever it reads repaints the item
/// when it changes.
pub type ReadPaint = fn(&Engine, ObjectId) -> Rc<dyn ItemPaint>;

#[derive(Default)]
struct Registry {
    types: RefCell<Vec<(MetaRef, ReadPaint)>>,
}

/// Makes the scene paint items of type `meta` (and of the types derived
/// from it) with what `read` returns. A later registration wins.
pub fn register_paint_item(engine: &Engine, meta: MetaRef, read: ReadPaint) {
    engine
        .extension::<Registry>()
        .types
        .borrow_mut()
        .push((meta, read));
}

/// The reader of `obj`'s type, if another crate registered one.
pub(crate) fn paint_item(engine: &Engine, obj: ObjectId) -> Option<ReadPaint> {
    let reg = engine.extension::<Registry>();
    let types = reg.types.borrow();
    if types.is_empty() {
        return None;
    }
    let meta = engine.meta_of(obj)?;
    types
        .iter()
        .rev()
        .find(|(m, _)| meta.inherits(m))
        .map(|(_, r)| *r)
}
