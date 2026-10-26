//! `Flipable`: an item with a front and a back, of which the one facing
//! the viewer shows.
//!
//! Measured on qml6 (features/items/flipable.feature):
//! - `side` follows the item's own transform only (`rotation`, `scale`
//!   and `transform`), not its parents': it is `Back` when that turns the
//!   item over, so that its x and y directions swap orientation.
//! - `front` and `back` become its children, and their `visible` stays as
//!   it is: the side that faces away is left out of the picture only.
//! - The back carries one transform of its own, from the start, that
//!   mirrors it about its centre so it reads the right way round once
//!   turned. It is worked out again when the item turns over to the back,
//!   along whichever direction the turn reversed, and kept when it turns
//!   to the front again.

use std::cell::Cell;

use ljos_engine::{Cx, Engine, EngineError, ObjectId, QmlObject, QmlType, Value, qml_impl};
use ljos_render::kurbo::Point;

use crate::enums::EnumTable;
use crate::item::{Item, is_a, num, write_computed};
use crate::transform::{LocalTransform, own_map};

pub mod side {
    pub const FRONT: i32 = 0;
    pub const BACK: i32 = 1;
}

fn flipable_enums() -> EnumTable {
    vec![(
        "Side",
        vec![
            ("Front", i64::from(side::FRONT)),
            ("Back", i64::from(side::BACK)),
        ],
    )]
}

#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Flipable",
    extends = Item,
    default = "data",
    enums = flipable_enums
)]
pub struct Flipable {
    #[qml(base)]
    pub item: Item,
    #[qml(notify, change = sides_changed)]
    pub front: Option<ObjectId>,
    #[qml(notify, change = sides_changed)]
    pub back: Option<ObjectId>,
    /// One of [`side`].
    #[qml(notify, readonly, enumeration = "Side", change = sides_changed)]
    pub side: i32,
    /// The side the mirror of the back was last worked out for.
    #[qml(skip)]
    seen: Cell<i32>,
}

#[qml_impl]
impl Flipable {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.effect_in(me, move |e| {
            let front = e.read(me, Flipable::FRONT.index).as_object();
            let back = e.read(me, Flipable::BACK.index).as_object();
            e.untrack(|| adopt(e, me, front, back));
        })?;
        e.effect_in(me, move |e| {
            let now = side_of(e, me);
            let back = e.read(me, Flipable::BACK.index).as_object();
            e.untrack(|| turned(e, me, now, back));
        })?;
        Ok(())
    }

    /// `retransformBack()`: works the mirror of the back out again.
    #[qml(invokable, name = "retransformBack")]
    fn retransform_back(cx: &Cx) {
        let (e, me) = (cx.engine, cx.this);
        let back = e.untrack(|| e.read(me, Flipable::BACK.index).as_object());
        if let Some(back) = back {
            mirror_back(e, me, back);
        }
    }
}

/// Makes the front and the back children of the Flipable, and gives the
/// back the transform that mirrors it.
fn adopt(e: &Engine, me: ObjectId, front: Option<ObjectId>, back: Option<ObjectId>) {
    for part in [front, back].into_iter().flatten() {
        if crate::item::is_item(e, part) && e.visual_parent(part) != Some(me) {
            let _ = crate::tree::set_parent_item(e, part, Some(me));
        }
    }
    if let Some(back) = back.filter(|b| crate::item::is_item(e, *b))
        && local_of(e, back).is_none()
        && let Ok(t) = e.create_native_incomplete(LocalTransform::default(), Some(back))
    {
        let _ = e.list_push(back, Item::TRANSFORM.index, t);
        if e.untrack(|| num(e, me, Flipable::SIDE.index)) as i32 == side::BACK {
            mirror_back(e, me, back);
        }
    }
}

/// The transform the Flipable gave `back`.
fn local_of(e: &Engine, back: ObjectId) -> Option<ObjectId> {
    match e.untrack(|| e.read(back, Item::TRANSFORM.index)) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .find(|t| is_a::<LocalTransform>(e, *t)),
        _ => None,
    }
}

/// Which side the item's own transform shows. Tracked.
fn side_of(e: &Engine, me: ObjectId) -> i32 {
    let (dx, dy) = directions(e, me);
    if dx.x * dy.y - dx.y * dy.x < 0.0 {
        side::BACK
    } else {
        side::FRONT
    }
}

/// Where the item's own transform takes a step right and a step down
/// from its centre.
fn directions(e: &Engine, me: ObjectId) -> (Point, Point) {
    let m = own_map(e, me);
    let w = num(e, me, Item::WIDTH.index);
    let h = num(e, me, Item::HEIGHT.index);
    let c = Point::new(w / 2.0, h / 2.0);
    let at = |p: Point| m.apply(p).unwrap_or(p);
    let o = at(c);
    let r = at(c + (1.0, 0.0));
    let d = at(c + (0.0, 1.0));
    (
        Point::new(r.x - o.x, r.y - o.y),
        Point::new(d.x - o.x, d.y - o.y),
    )
}

fn turned(e: &Engine, me: ObjectId, now: i32, back: Option<ObjectId>) {
    let Ok(before) = e.with(me, |f: &Flipable| f.seen.replace(now)) else {
        return;
    };
    if e.read(me, Flipable::SIDE.index).to_number() as i32 != now {
        let _ = write_computed(e, me, Flipable::SIDE.index, Value::Int(now));
    }
    if now == side::BACK
        && before != side::BACK
        && let Some(back) = back
    {
        mirror_back(e, me, back);
    }
}

/// Mirrors the back along each direction the Flipable's turn reversed.
fn mirror_back(e: &Engine, me: ObjectId, back: ObjectId) {
    let Some(t) = local_of(e, back) else {
        return;
    };
    let (dx, dy) = e.untrack(|| directions(e, me));
    // Up and down only when that alone was reversed; otherwise left and
    // right, as a card is turned over.
    let y = dy.y < 0.0 && dx.x >= 0.0;
    let _ = write_computed(e, t, LocalTransform::MIRROR_X.index, Value::Bool(!y));
    let _ = write_computed(e, t, LocalTransform::MIRROR_Y.index, Value::Bool(y));
}

/// Which side faces away may have changed: the scene reads its children's
/// geometry again ([`faces_away`]).
fn sides_changed(cx: &Cx<'_>, _: ljos_engine::PropIndex) {
    let (e, me) = (cx.engine, cx.this);
    for c in crate::child_items(e, me) {
        crate::scene::reshape(e, c);
    }
}

/// Whether `obj` is the side of a Flipable that faces away. Tracked.
pub(crate) fn faces_away(e: &Engine, obj: ObjectId) -> bool {
    let Some(p) = e.visual_parent(obj) else {
        return false;
    };
    if !e
        .meta_of(p)
        .is_some_and(|m| m.inherits(Flipable::static_meta()))
    {
        return false;
    }
    let side = e.read(p, Flipable::SIDE.index).to_number() as i32;
    if e.read(p, Flipable::FRONT.index).as_object() == Some(obj) {
        side != side::FRONT
    } else if e.read(p, Flipable::BACK.index).as_object() == Some(obj) {
        side != side::BACK
    } else {
        false
    }
}

pub(crate) fn register(engine: &Engine) {
    engine.register_type(
        "QtQuick",
        ljos_engine::Version::new(2, 15),
        ljos_engine::MetaRef::Static(Flipable::static_meta()),
    );
}
