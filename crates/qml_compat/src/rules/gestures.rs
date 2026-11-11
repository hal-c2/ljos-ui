//! Rules of gestures and handlers: `TapHandler`, `DragHandler`, flicking.

pub(crate) mod gesture;

use super::Area;

/// What a pointer handler accepts is what a gesture takes (docs/DIALECT.md
/// §4.7).
const GESTURE: &[(&str, &str)] = &[
    ("acceptedButtons", "buttons"),
    ("acceptedModifiers", "modifiers"),
    ("acceptedDevices", "devices"),
    ("acceptedPointerTypes", "pointerTypes"),
];

/// A wheel takes no buttons.
const WHEEL: &[(&str, &str)] = &[
    ("acceptedModifiers", "modifiers"),
    ("acceptedDevices", "devices"),
    ("acceptedPointerTypes", "pointerTypes"),
];

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
    members: &[
        // Ljós's TapHandler is described on its own, not on PointerHandler.
        ("TapHandler", GESTURE),
        ("PointerHandler", GESTURE),
        ("WheelHandler", WHEEL),
    ],
};
