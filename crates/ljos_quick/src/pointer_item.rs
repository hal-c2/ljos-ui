//! Items of other crates that take the pointer themselves.
//!
//! `MouseArea` and `TapHandler` live here and the scene knows them. A
//! control (`QtQuick.Controls`' `Button`) is an item that reacts to the
//! pointer without either, and lives in another crate; that crate names
//! its types and what they do with a press here, and the scene delivers
//! to them in the same pass, after an item's handlers and beside
//! `MouseArea`.
//!
//! Also what such a crate needs to know of an item that only this crate
//! can tell: whether its size is still its implicit size, and whether it
//! is mirrored.

use std::cell::RefCell;

use ljos_engine::{Engine, MetaRef, ObjectId};

use crate::by_type::ByType;
use ljos_render::kurbo;

use crate::input::PointerEvent;
use crate::item::Item;

/// What an item type does with the pointer. Positions are in the item's
/// own coordinates.
#[derive(Clone, Copy, Debug)]
pub struct PointerItem {
    /// A press landed on the item. True: the item takes it, and gets the
    /// moves and the release (or cancel) that follow, wherever they are.
    /// False: the press goes on to the items beneath.
    pub press: fn(&Engine, ObjectId, &PointerEvent, kurbo::Point) -> bool,
    /// The pointer moved while the item holds the press.
    pub moved: fn(&Engine, ObjectId, &PointerEvent, kurbo::Point),
    /// The press the item holds ended.
    pub release: fn(&Engine, ObjectId, &PointerEvent, kurbo::Point),
    /// The press the item holds was taken away.
    pub cancel: fn(&Engine, ObjectId),
    /// The pointer came over the item or left it. An item under the
    /// pointer is hovered together with the registered items it is inside.
    pub hover: fn(&Engine, ObjectId, bool),
}

#[derive(Default)]
struct Registry {
    types: ByType<PointerItem>,
}

/// Makes the scene deliver the pointer to items of type `meta` (and of the
/// types derived from it) through `hooks`. A later registration wins, so a
/// derived type can be given hooks of its own.
pub fn register_pointer_item(engine: &Engine, meta: MetaRef, hooks: PointerItem) {
    engine.extension::<Registry>().types.push(meta, hooks);
}

/// The hooks of `obj`'s type.
pub(crate) fn pointer_item(engine: &Engine, obj: ObjectId) -> Option<PointerItem> {
    let reg = engine.extension::<Registry>();
    if reg.types.is_empty() {
        return None;
    }
    let meta = engine.meta_of(obj)?;
    let hooks = reg.types.find(&meta)?;
    let conditions = engine.extension::<Conditions>();
    let unmet = conditions
        .types
        .borrow()
        .iter()
        .any(|(m, met)| meta.inherits(m) && !met(engine, obj));
    (!unmet).then_some(hooks)
}

/// Whether `obj` takes pointer input of its own: a `MouseArea`, a
/// `Flickable`, an item with a pointer handler (`TapHandler`,
/// `DragHandler`, `PointHandler`, `PinchHandler`, `WheelHandler`), or an
/// item of a type registered here. For a tool that reaches the items an
/// accessibility tree has no node for; hover alone does not count.
pub fn takes_pointer(engine: &Engine, obj: ObjectId) -> bool {
    use crate::item::is_a;
    engine.untrack(|| {
        is_a::<crate::input::MouseArea>(engine, obj)
            || is_a::<crate::flickable::Flickable>(engine, obj)
            || pointer_item(engine, obj).is_some()
            || crate::tree::resources(engine, obj).into_iter().any(|h| {
                is_a::<crate::input::TapHandler>(engine, h)
                    || is_a::<crate::handlers::DragHandler>(engine, h)
                    || is_a::<crate::handlers::PointHandler>(engine, h)
                    || is_a::<crate::handlers::PinchHandler>(engine, h)
                    || is_a::<crate::wheel_handler::WheelHandler>(engine, h)
            })
    })
}

/// Whether an item of a registered type takes the pointer just now.
pub type PointerCondition = fn(&Engine, ObjectId) -> bool;

#[derive(Default)]
struct Conditions {
    types: RefCell<Vec<(MetaRef, PointerCondition)>>,
}

/// Makes items of type `meta` (and of the types derived from it) take the
/// pointer only while `met` says so: the rest of the time the scene treats
/// them as items that were never registered, for presses and for hover. For
/// a type that takes the pointer in one state only (a `Label` whose focus
/// policy has `Qt.ClickFocus`).
pub fn register_pointer_condition(engine: &Engine, meta: MetaRef, met: PointerCondition) {
    engine
        .extension::<Conditions>()
        .types
        .borrow_mut()
        .push((meta, met));
}

/// Brings the hovered registered items in step with the items under the
/// pointer (`hits`, topmost first): the topmost registered one and the
/// registered items it is inside.
pub(crate) fn update_hover(engine: &Engine, hits: &[ObjectId], hovered: &mut Vec<ObjectId>) {
    if hovered.is_empty() && engine.extension::<Registry>().types.is_empty() {
        return;
    }
    let mut now: Vec<(ObjectId, PointerItem)> = Vec::new();
    if let Some(top) = hits
        .iter()
        .copied()
        .find(|o| pointer_item(engine, *o).is_some())
    {
        let mut cur = Some(top);
        // Bounded so a corrupt parent cycle cannot hang.
        for _ in 0..4096 {
            let Some(o) = cur else { break };
            if let Some(h) = pointer_item(engine, o) {
                now.push((o, h));
            }
            cur = engine.untrack(|| engine.visual_parent(o));
        }
    }
    let old = std::mem::take(hovered);
    for o in &old {
        if !now.iter().any(|(n, _)| n == o)
            && engine.is_alive(*o)
            && let Some(h) = pointer_item(engine, *o)
        {
            (h.hover)(engine, *o, false);
        }
    }
    for (o, h) in now.iter().rev() {
        if !old.contains(o) {
            (h.hover)(engine, *o, true);
        }
        hovered.push(*o);
    }
}

/// What an item type does with a pointer that moves over it with no
/// button it holds: the point is in the item's own coordinates. For an
/// item with parts that are hovered on their own (the buttons of a spin
/// box); whether the item itself is hovered is [`PointerItem::hover`].
pub type HoverMove = fn(&Engine, ObjectId, kurbo::Point);

#[derive(Default)]
struct HoverMoves {
    types: ByType<HoverMove>,
}

/// Makes the scene tell hovered items of type `meta` (and of the types
/// derived from it) where the pointer is, each time it moves. The item
/// must also be a [`PointerItem`]: only those are hovered.
pub fn register_hover_move(engine: &Engine, meta: MetaRef, moved: HoverMove) {
    engine.extension::<HoverMoves>().types.push(meta, moved);
}

/// Tells the hovered items that asked where the pointer is. `local` maps
/// the pointer into an item's coordinates.
pub(crate) fn hover_moved(
    engine: &Engine,
    hovered: &[ObjectId],
    local: impl Fn(ObjectId) -> kurbo::Point,
) {
    let reg = engine.extension::<HoverMoves>();
    if hovered.is_empty() || reg.types.is_empty() {
        return;
    }
    for o in hovered {
        let Some(meta) = engine.meta_of(*o) else {
            continue;
        };
        // Copied out: the hook may register or destroy.
        let hook = reg.types.find(&meta);
        if let Some(hook) = hook {
            hook(engine, *o, local(*o));
        }
    }
}

/// Whether the point is inside the item's rectangle.
pub fn contains_point(engine: &Engine, obj: ObjectId, local: kurbo::Point) -> bool {
    let (w, h) = engine.untrack(|| {
        (
            engine.read(obj, Item::WIDTH.index).to_number(),
            engine.read(obj, Item::HEIGHT.index).to_number(),
        )
    });
    local.x >= 0.0 && local.y >= 0.0 && local.x < w && local.y < h
}

/// Whether nothing has given the item a size along one axis: it still
/// follows its implicit size and no pair of anchors stretches it. A
/// container that sizes an item it was handed (a control's background)
/// leaves a size the author gave alone.
pub fn follows_implicit_size(engine: &Engine, obj: ObjectId, horizontal: bool) -> bool {
    crate::item::size_follows_implicit(engine, obj, horizontal)
}

/// A control gives the part it lays out its size: the part is a given size
/// on both axes from now on, so a text in it wraps to that width.
pub fn give_size(engine: &Engine, obj: ObjectId) {
    crate::item::give_size(engine, obj);
}

/// Whether `LayoutMirroring` mirrors the item (tracked).
pub fn is_mirrored(engine: &Engine, obj: ObjectId) -> bool {
    crate::mirroring::mirrored(engine, obj)
}
