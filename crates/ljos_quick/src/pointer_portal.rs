//! Items that pass the pointer on to items they show elsewhere: a 3D view
//! shows items on its models, and a press on a model there is a press on
//! the item at that point of it.
//!
//! The crate that owns such a type says where under a point of the item
//! another item is shown, and at what point of it. A press nothing above
//! the portal takes goes to the items there, topmost first, as a press on
//! the window goes to the items under it, and the moves and the release
//! that follow are mapped the same way while one of them holds it.

use ljos_engine::{Engine, MetaRef, ObjectId};
use ljos_render::kurbo;

use crate::by_type::ByType;
use crate::item::{Item, num, scene_transform};

/// Where `portal` shows an item under `at`, a point in its own
/// coordinates: that item and the point of it, in its own coordinates.
pub type PointerPortal = fn(&Engine, ObjectId, kurbo::Point) -> Option<(ObjectId, kurbo::Point)>;

#[derive(Default)]
struct Registry {
    types: ByType<PointerPortal>,
}

/// Makes items of type `meta`, and of the types derived from it, pass the
/// pointer on through `find`. A later registration wins.
pub fn register_pointer_portal(engine: &Engine, meta: MetaRef, find: PointerPortal) {
    engine.extension::<Registry>().types.push(meta, find);
}

pub(crate) fn portal(engine: &Engine, obj: ObjectId) -> Option<PointerPortal> {
    let reg = engine.extension::<Registry>();
    if reg.types.is_empty() {
        return None;
    }
    reg.types.find(&engine.meta_of(obj)?)
}

/// Where `find` puts the window point `pos` over `portal`, in the
/// coordinates of the scene `item` lies in; `None` when it shows another
/// item there, or none.
pub(crate) fn map(
    engine: &Engine,
    find: PointerPortal,
    portal: ObjectId,
    item: Option<ObjectId>,
    pos: kurbo::Point,
) -> Option<(ObjectId, kurbo::Point)> {
    let local = crate::scene::local_point(engine, portal, pos);
    let (shown, at) = engine.untrack(|| find(engine, portal, local))?;
    if item.is_some_and(|i| i != shown) || !engine.is_alive(shown) {
        return None;
    }
    Some((
        shown,
        engine.untrack(|| scene_transform(engine, shown)) * at,
    ))
}

/// The items of `top`'s tree under `pos`, in the coordinates of the scene
/// it lies in, topmost first. Found from the items themselves: a tree
/// shown only on a model is not in the window's picture.
pub(crate) fn hits_in(engine: &Engine, top: ObjectId, pos: kurbo::Point) -> Vec<ObjectId> {
    let mut out = Vec::new();
    engine.untrack(|| walk(engine, top, pos, &mut out, 0));
    out
}

fn walk(engine: &Engine, obj: ObjectId, pos: kurbo::Point, out: &mut Vec<ObjectId>, depth: u32) {
    // Bounded so a corrupt parent cycle cannot recurse forever.
    if depth > 256 || !engine.read(obj, Item::VISIBLE.index).to_boolean() {
        return;
    }
    let local = crate::scene::local_point(engine, obj, pos);
    let inside = crate::scene::contains_local(engine, obj, local);
    if !inside && engine.read(obj, Item::CLIP.index).to_boolean() {
        return;
    }
    let mut children = crate::tree::child_items(engine, obj);
    // Stable: of equal z the later is drawn over the earlier.
    children
        .sort_by(|a, b| num(engine, *a, Item::Z.index).total_cmp(&num(engine, *b, Item::Z.index)));
    for c in children.into_iter().rev() {
        walk(engine, c, pos, out, depth + 1);
    }
    if inside {
        out.push(obj);
    }
}
