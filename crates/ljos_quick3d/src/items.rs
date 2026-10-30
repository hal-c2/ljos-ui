//! Items inside a 3D scene: an `Item` declared in a `Node` is drawn as a
//! flat picture in the node's plane (doc.qt.io/qt-6/qtquick3d-2d.html).
//!
//! The node's items become the children of an item of no size that the
//! node holds, so that `anchors.centerIn: parent` centres one on the
//! node's origin. That holder is drawn off the screen as one picture of
//! what its children cover ([`item::item_picture`]), shown on a rectangle
//! one scene unit a pixel, its top-left corner at the node's origin where
//! the children's is, and reaching down the node's y axis as it reaches
//! down the screen.

use std::cell::RefCell;
use std::collections::HashMap;

use ljos::item::ljos_3d::{
    self, CullMode, DefaultMaterial, Lighting, Material, MeshSource, NodeKind, Primitive, Sampling,
    Tiling, Transform, UvTransform, math::Vec3,
};
use ljos::item::{self, Item, PictureArea, kurbo};
use ljos::ljos_engine::WriteKind;
use ljos::prelude::*;

use crate::Node;
use crate::node::Object3D;
use crate::texture::ItemPixels;

/// The holder of each node that has items, by the node.
#[derive(Default)]
struct Holders(RefCell<HashMap<ObjectId, ObjectId>>);

/// `Object3D.data`'s append hook: an item goes to the node's holder, as
/// its visual child, and into the list with the rest.
pub(crate) fn data_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    let e = cx.engine;
    e.list_push(cx.this, Object3D::DATA.index, child)?;
    if !e.is::<Item>(child) || !e.is::<Node>(cx.this) {
        return Ok(());
    }
    let holder = holder(e, cx.this)?;
    // Its parent's change hook makes it the holder's visual child.
    e.write(
        child,
        Item::PARENT.index,
        Value::Object(holder),
        WriteKind::Host,
    )
}

/// The item that holds `node`'s items, made the first time one comes.
fn holder(e: &Engine, node: ObjectId) -> Result<ObjectId, EngineError> {
    let holders = e.extension::<Holders>();
    let known = holders.0.borrow().get(&node).copied();
    if let Some(h) = known.filter(|h| e.is_alive(*h)) {
        return Ok(h);
    }
    // Owned by the node, so that it goes with it.
    let h = e.create_native(Item::default(), Some(node))?;
    let mut map = holders.0.borrow_mut();
    map.retain(|n, h| e.is_alive(*n) && e.is_alive(*h));
    map.insert(node, h);
    Ok(h)
}

/// The holder of `node`'s items, if it has any.
pub(crate) fn holder_of(e: &Engine, node: ObjectId) -> Option<ObjectId> {
    let h = e.extension::<Holders>().0.borrow().get(&node).copied();
    h.filter(|h| e.is_alive(*h))
}

/// The rectangle that shows the picture of `holder`'s items, as a child of
/// their node, when there is one to show. Tracked.
pub(crate) fn quad(e: &Engine, view: ObjectId, holder: ObjectId) -> Option<ljos_3d::Node> {
    let picture = item::item_picture(e, view, holder, PictureArea::Children)?;
    let a = picture.area;
    let texture = ljos_3d::Texture {
        image: ljos_3d::TextureImage::new(ItemPixels(picture.image)),
        // Its edge pixels are the items' own, not the far side's.
        sampling: Sampling {
            tiling_u: Tiling::ClampToEdge,
            tiling_v: Tiling::ClampToEdge,
            ..Sampling::default()
        },
        uv: UvTransform::default(),
    };
    let material = Material::Default(DefaultMaterial {
        diffuse_map: Some(texture),
        // "Items are not affected by lights."
        lighting: Lighting::None,
        cull_mode: CullMode::None,
        ..DefaultMaterial::default()
    });
    let model = ljos_3d::Model {
        mesh: MeshSource::Primitive(Primitive::Rectangle),
        material,
    };
    let (w, h) = (a.width() as f32, a.height() as f32);
    let mut n = ljos_3d::Node::new(NodeKind::Model(model)).with_transform(Transform {
        // The rectangle is 100 units a side about its middle; the picture's
        // y grows down, the node's up.
        position: Vec3::new(
            (a.x0 + a.width() / 2.0) as f32,
            -(a.y0 + a.height() / 2.0) as f32,
            0.0,
        ),
        scale: Vec3::new(w / 100.0, h / 100.0, 1.0),
        ..Transform::default()
    });
    // "Items do not cast shadows."
    n.casts_shadows = false;
    n.receives_shadows = false;
    Some(n)
}

/// Where in `holder`'s coordinates the point at `uv` of its rectangle is,
/// as last drawn; `None` before it was.
pub(crate) fn point_at(
    e: &Engine,
    view: ObjectId,
    holder: ObjectId,
    uv: [f32; 2],
) -> Option<kurbo::Point> {
    let a = e
        .untrack(|| item::item_picture(e, view, holder, PictureArea::Children))?
        .area;
    Some(kurbo::Point::new(
        a.x0 + f64::from(uv[0]) * a.width(),
        a.y0 + f64::from(uv[1]) * a.height(),
    ))
}
