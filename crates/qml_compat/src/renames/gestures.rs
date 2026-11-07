//! Renames of gestures and handlers: `TapHandler`, `DragHandler`, flicking.

use super::Area;

/// What a pointer handler accepts is what a gesture takes (docs/DIALECT.md
/// §4.7).
const GESTURE: &[(&str, &str)] = &[
    ("acceptedButtons", "buttons"),
    ("acceptedModifiers", "modifiers"),
    ("acceptedDevices", "devices"),
    ("acceptedPointerTypes", "pointerTypes"),
];

pub(super) const AREA: Area = Area {
    types: &[("TapHandler", "Tap"), ("HoverHandler", "Hover")],
    members: &[
        // Ljós's TapHandler is described on its own, not on PointerHandler.
        ("TapHandler", GESTURE),
        ("PointerHandler", GESTURE),
    ],
};
