//! `Drawer` (QtQuick.Templates): a popup that slides in from an edge of
//! its window.
//!
//! A drawer is a popup whose place is worked out from its `edge` and its
//! `position`, from 0 (out of sight past the edge) to 1 (all of it in
//! the window), rather than from `x` and `y`. Its `enter` and `exit`
//! transitions animate `position`. One without them is opened and closed
//! where its position is, as in Qt: out of sight unless something moves
//! it. The shade behind it is as opaque as the drawer is far in.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-drawer.html. The
//! defaults, the places along each edge and the order of the signals were
//! read off a running `qml6` (Qt 6.11.2); `features/controls/drawer.feature`
//! pins them.

use ljos_engine::{Engine, EngineError, ObjectId, QmlObject, Value};
use ljos_quick::Item;

use super::{Popup, close_policy, item_of, state_of};
use crate::util::{flag, num, set_computed, set_derived};

/// `Qt.Edge`.
pub(crate) mod edge {
    pub const TOP: i32 = 1;
    pub const LEFT: i32 = 2;
    pub const RIGHT: i32 = 4;
    pub const BOTTOM: i32 = 8;
}

/// How near the edge a press has to be to drag a drawer in (read off
/// qml6).
const DRAG_MARGIN: f64 = 8.0;

/// `Drawer`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "Drawer", extends = Popup)]
pub struct Drawer {
    #[qml(base)]
    pub popup: Popup,
    /// The edge of the window the drawer comes in from: a `Qt.Edge`.
    #[qml(notify, enumeration = "Qt.Edge")]
    pub edge: i32,
    /// How far in the drawer is, from 0 to 1.
    #[qml(notify)]
    pub position: f64,
    #[qml(notify, reset)]
    pub drag_margin: f64,
    #[qml(notify)]
    pub interactive: bool,
}

impl Default for Drawer {
    fn default() -> Self {
        // A drawer is modal, takes the keyboard, and goes when Escape is
        // pressed or the pointer is let go beside it.
        let popup = Popup {
            modal: true,
            focus: true,
            close_policy: close_policy::ON_ESCAPE | close_policy::ON_RELEASE_OUTSIDE,
            ..Popup::default()
        };
        Self {
            popup,
            edge: edge::LEFT,
            position: 0.0,
            drag_margin: DRAG_MARGIN,
            interactive: true,
        }
    }
}

/// Whether `popup` is a drawer.
pub(super) fn is_drawer(engine: &Engine, popup: ObjectId) -> bool {
    crate::util::is_a::<Drawer>(engine, popup)
}

/// Where a drawer is, from the corner of the window: past `edge` by as
/// much of its size as is not in yet.
fn corner(edge: i32, position: f64, size: (f64, f64), bound: (f64, f64)) -> (f64, f64) {
    let (w, h) = size;
    let (bw, bh) = bound;
    match edge {
        edge::RIGHT => (bw - position * w, 0.0),
        edge::TOP => (0.0, (position - 1.0) * h),
        edge::BOTTOM => (0.0, bh - position * h),
        _ => ((position - 1.0) * w, 0.0),
    }
}

/// Puts an open drawer's item where its edge and its position say, and
/// makes `x` and `y` read as where that is. Everything it reads is a
/// dependency when it runs in an effect.
pub(super) fn place(engine: &Engine, drawer: ObjectId) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, drawer) else {
        return Ok(());
    };
    let Some(item) = item_of(engine, drawer) else {
        return Ok(());
    };
    if !flag(engine, drawer, Popup::VISIBLE.index) {
        return Ok(());
    }
    let Some(overlay) = engine.visual_parent(item) else {
        return Ok(());
    };
    let (x, y) = corner(
        num(engine, drawer, Drawer::EDGE.index) as i32,
        num(engine, drawer, Drawer::POSITION.index),
        (
            num(engine, drawer, Popup::WIDTH.index),
            num(engine, drawer, Popup::HEIGHT.index),
        ),
        (
            num(engine, overlay, Item::WIDTH.index),
            num(engine, overlay, Item::HEIGHT.index),
        ),
    );
    engine.untrack(|| {
        set_computed(engine, item, Item::X.index, Value::Real(x))?;
        set_computed(engine, item, Item::Y.index, Value::Real(y))?;
        // The overlay is where a drawer is placed from: its `x` and `y`
        // are from the corner of the window.
        state.placing.set(true);
        let wrote = set_derived(engine, drawer, Popup::X.index, Value::Real(x))
            .and_then(|()| set_derived(engine, drawer, Popup::Y.index, Value::Real(y)));
        state.placing.set(false);
        wrote
    })
}

/// How opaque the shade behind a drawer is: as far as the drawer is in.
pub(super) fn shade_opacity(engine: &Engine, drawer: ObjectId) -> Value {
    Value::Real(num(engine, drawer, Drawer::POSITION.index).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::{corner, edge};

    #[test]
    fn a_drawer_is_past_its_edge_by_what_is_not_in() {
        let (size, bound) = ((120.0, 80.0), (400.0, 300.0));
        assert_eq!(corner(edge::LEFT, 0.0, size, bound), (-120.0, 0.0));
        assert_eq!(corner(edge::LEFT, 0.5, size, bound), (-60.0, 0.0));
        assert_eq!(corner(edge::RIGHT, 1.0, size, bound), (280.0, 0.0));
        assert_eq!(corner(edge::TOP, 1.0, size, bound), (0.0, 0.0));
        assert_eq!(corner(edge::TOP, 0.0, size, bound), (0.0, -80.0));
        assert_eq!(corner(edge::BOTTOM, 1.0, size, bound), (0.0, 220.0));
    }
}
