//! Where a popup is: at `x`, `y` from the corner of its parent or
//! centred in an item, and then pushed back inside the window as far as
//! its margins ask (doc.qt.io/qt-6/qml-qtquick-controls-popup.html,
//! "Popup Positioning"; pinned by `features/controls/popup-geometry.feature`).

use std::sync::OnceLock;

use ljos_engine::{Engine, EngineError, ObjectId, PropIndex, QmlType, Value};
use ljos_quick::{Item, is_item, scene_transform};
use ljos_render::kurbo;

use super::{Popup, item_of, put, state_of};
use crate::util::{flag, num, object, set_computed};

fn center_in() -> PropIndex {
    static PROP: OnceLock<PropIndex> = OnceLock::new();
    *PROP.get_or_init(|| {
        Popup::static_meta()
            .property_index("anchors.centerIn")
            .expect("a popup has anchors")
    })
}

/// Moves `pos` so that `size` stays `near` from the start and `far` from
/// the end of `bound`; a negative margin asks for nothing. What does not
/// fit keeps to the start.
fn clamp(mut pos: f64, size: f64, bound: f64, near: f64, far: f64) -> f64 {
    if far >= 0.0 && pos + size > bound - far {
        pos = bound - far - size;
    }
    if near >= 0.0 && pos < near {
        pos = near;
    }
    pos
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
    if super::drawer::is_drawer(engine, popup) {
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
    let mut pos = match centre {
        Some(t) => {
            let middle = scene_transform(engine, t)
                * kurbo::Point::new(
                    num(engine, t, Item::WIDTH.index) / 2.0,
                    num(engine, t, Item::HEIGHT.index) / 2.0,
                );
            kurbo::Point::new(middle.x - w / 2.0, middle.y - h / 2.0)
        }
        None => {
            let (x, y) = state.requested.get();
            to_scene * kurbo::Point::new(x, y)
        }
    };
    let (bw, bh) = (
        num(engine, overlay, Item::WIDTH.index),
        num(engine, overlay, Item::HEIGHT.index),
    );
    let margin = |p: ljos_engine::Prop<f64>| num(engine, popup, p.index);
    if bw > 0.0 {
        pos.x = clamp(
            pos.x,
            w,
            bw,
            margin(Popup::LEFT_MARGIN),
            margin(Popup::RIGHT_MARGIN),
        );
    }
    if bh > 0.0 {
        pos.y = clamp(
            pos.y,
            h,
            bh,
            margin(Popup::TOP_MARGIN),
            margin(Popup::BOTTOM_MARGIN),
        );
    }
    engine.untrack(|| {
        set_computed(engine, item, Item::X.index, Value::Real(pos.x))?;
        set_computed(engine, item, Item::Y.index, Value::Real(pos.y))?;
        let local = to_scene.inverse() * pos;
        state.placing.set(true);
        let wrote = put(engine, popup, Popup::X.index, Value::Real(local.x))
            .and_then(|()| put(engine, popup, Popup::Y.index, Value::Real(local.y)));
        state.placing.set(false);
        wrote
    })
}

#[cfg(test)]
mod tests {
    use super::clamp;

    #[test]
    fn no_margin_leaves_the_popup_where_it_is() {
        assert_eq!(clamp(450.0, 100.0, 400.0, -1.0, -1.0), 450.0);
        assert_eq!(clamp(-50.0, 100.0, 400.0, -1.0, -1.0), -50.0);
    }

    #[test]
    fn a_margin_pushes_the_popup_back_in() {
        assert_eq!(clamp(350.0, 100.0, 400.0, 0.0, 0.0), 300.0);
        assert_eq!(clamp(350.0, 100.0, 400.0, 20.0, 20.0), 280.0);
        assert_eq!(clamp(-30.0, 100.0, 400.0, 5.0, 5.0), 5.0);
        // One side only.
        assert_eq!(clamp(350.0, 100.0, 400.0, -1.0, 0.0), 300.0);
        assert_eq!(clamp(-30.0, 100.0, 400.0, -1.0, 0.0), -30.0);
    }

    #[test]
    fn what_does_not_fit_keeps_to_the_start() {
        assert_eq!(clamp(10.0, 500.0, 400.0, 0.0, 0.0), 0.0);
    }
}
