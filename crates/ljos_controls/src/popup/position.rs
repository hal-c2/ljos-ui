//! Where a popup is: at `x`, `y` from the corner of its parent or
//! centred in an item, and then, by `placement`, flipped and pushed back
//! inside the window as far as its margins ask
//! (doc.qt.io/qt-6/qml-qtquick-controls-popup.html, "Popup Positioning";
//! pinned by `features/controls/popup-geometry.feature`). This file reads
//! the numbers and writes the answer back.

use std::sync::OnceLock;

use ljos_engine::{Engine, EngineError, ObjectId, PropIndex, QmlType, Value};
use ljos_quick::{Item, is_item, scene_transform};
use ljos_render::kurbo;

use super::placement::{self, Axis};
use super::{Popup, item_of, state_of};
use crate::util::{flag, num, object, set_computed, set_derived};

fn center_in() -> PropIndex {
    static PROP: OnceLock<PropIndex> = OnceLock::new();
    *PROP.get_or_init(|| {
        Popup::static_meta()
            .property_index("anchors.centerIn")
            .expect("a popup has anchors")
    })
}

/// How large an open popup that was given no size of its own is along
/// one axis: as large as it `wanted`, and no larger than the room between
/// its margins when it has one at both ends (read off qml6: a margin at
/// one end only moves the popup, and a closed popup is as large as it
/// wants to be). Everything it reads is a dependency of the caller.
pub(super) fn fitted(engine: &Engine, popup: ObjectId, horizontal: bool, wanted: f64) -> f64 {
    let (near, far, extent) = if horizontal {
        (Popup::LEFT_MARGIN, Popup::RIGHT_MARGIN, Item::WIDTH)
    } else {
        (Popup::TOP_MARGIN, Popup::BOTTOM_MARGIN, Item::HEIGHT)
    };
    let (near, far) = (
        num(engine, popup, near.index),
        num(engine, popup, far.index),
    );
    if near < 0.0 || far < 0.0 || !flag(engine, popup, Popup::VISIBLE.index) {
        return wanted;
    }
    let Some(overlay) = item_of(engine, popup).and_then(|item| engine.visual_parent(item)) else {
        return wanted;
    };
    let bound = num(engine, overlay, extent.index);
    if bound <= 0.0 {
        return wanted;
    }
    wanted.min((bound - near - far).max(0.0))
}

/// Puts an open popup's item where the popup belongs, and makes `x` and
/// `y` read as where that is. Everything it reads is a dependency when
/// it runs in an effect, except `x` and `y` themselves: those it writes.
pub(super) fn place(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    if super::kind_of(engine, popup).is_drawer() {
        // A drawer is placed by its edge, not by `x` and `y`.
        return super::drawer::place(engine, popup);
    }
    let Some(state) = state_of(engine, popup) else {
        return Ok(());
    };
    let Some(item) = item_of(engine, popup) else {
        return Ok(());
    };
    if !flag(engine, popup, Popup::VISIBLE.index) {
        return Ok(());
    }
    let Some(overlay) = engine.visual_parent(item) else {
        return Ok(());
    };
    let Some(parent) = object(engine, popup, Popup::PARENT.index).filter(|p| is_item(engine, *p))
    else {
        return Ok(());
    };
    let (w, h) = (
        num(engine, popup, Popup::WIDTH.index),
        num(engine, popup, Popup::HEIGHT.index),
    );
    let to_scene = scene_transform(engine, parent);
    let centre = object(engine, popup, center_in()).filter(|t| is_item(engine, *t));
    let flips = state.flips.get();
    // Where it was put, and where it would be on the other side of its
    // parent along each axis it flips on, in the overlay.
    let (pos, mirrored) = match centre {
        Some(t) => {
            let middle = scene_transform(engine, t)
                * kurbo::Point::new(
                    num(engine, t, Item::WIDTH.index) / 2.0,
                    num(engine, t, Item::HEIGHT.index) / 2.0,
                );
            let pos = kurbo::Point::new(middle.x - w / 2.0, middle.y - h / 2.0);
            (pos, (None, None))
        }
        None => {
            // A binding on `x` or `y` that reads the size runs when the
            // size changes, but its watch may not see the new value: what
            // is written back below can put the old one there again first.
            // A value that is not the one written back is asked for.
            let now = engine.untrack(|| {
                (
                    num(engine, popup, Popup::X.index),
                    num(engine, popup, Popup::Y.index),
                )
            });
            if let Some((px, py)) = state.placed.get() {
                let (mut x, mut y) = state.requested.get();
                if now.0 != px {
                    x = now.0;
                }
                if now.1 != py {
                    y = now.1;
                }
                state.requested.set((x, y));
            }
            let (x, y) = state.requested.get();
            let pos = to_scene * kurbo::Point::new(x, y);
            // Mirrored around the middle of the parent, one axis at a
            // time: the other one stays as it was put.
            let beside = flips.horizontal.then(|| {
                let other = num(engine, parent, Item::WIDTH.index) - x - w;
                (to_scene * kurbo::Point::new(other, y)).x
            });
            let above = flips.vertical.then(|| {
                let other = num(engine, parent, Item::HEIGHT.index) - y - h;
                (to_scene * kurbo::Point::new(x, other)).y
            });
            (pos, (beside, above))
        }
    };
    let margin = |p: ljos_engine::Prop<f64>| num(engine, popup, p.index);
    let pos = kurbo::Point::new(
        placement::settle(Axis {
            at: pos.x,
            mirrored: mirrored.0,
            size: w,
            bound: num(engine, overlay, Item::WIDTH.index),
            near: margin(Popup::LEFT_MARGIN),
            far: margin(Popup::RIGHT_MARGIN),
        }),
        placement::settle(Axis {
            at: pos.y,
            mirrored: mirrored.1,
            size: h,
            bound: num(engine, overlay, Item::HEIGHT.index),
            near: margin(Popup::TOP_MARGIN),
            far: margin(Popup::BOTTOM_MARGIN),
        }),
    );
    engine.untrack(|| {
        set_computed(engine, item, Item::X.index, Value::Real(pos.x))?;
        set_computed(engine, item, Item::Y.index, Value::Real(pos.y))?;
        let local = to_scene.inverse() * pos;
        state.placing.set(true);
        let wrote = set_derived(engine, popup, Popup::X.index, Value::Real(local.x))
            .and_then(|()| set_derived(engine, popup, Popup::Y.index, Value::Real(local.y)));
        state.placing.set(false);
        state.placed.set(Some((local.x, local.y)));
        wrote
    })
}
