//! Rules of gestures and handlers: `TapHandler`, `DragHandler`, flicking.

pub(crate) mod gesture;

use super::Area;

pub(super) const AREA: Area = Area {
    types: &[
        ("TapHandler", "Tap"),
        ("HoverHandler", "Hover"),
        ("DragHandler", "Drag"),
        ("PointHandler", "Pointer"),
        ("WheelHandler", "Wheel"),
        // Qt's attached `Drag` carries data to a `DropArea`; Ljós's `Drag` is
        // the gesture that moves an item.
        ("Drag", "DragSource"),
    ],
    // What a pointer handler accepts is what a gesture takes
    // (docs/DIALECT.md §4.7), as each gesture of Ljos.Ui says.
    members: &[],
};
