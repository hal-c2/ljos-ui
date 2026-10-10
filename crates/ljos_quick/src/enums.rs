//! Integer values of the QtQuick enums this crate uses.
//!
//! The registry has no enum registration yet, so enum-typed properties are
//! `int` properties and QML code writes these numbers. The values follow
//! the documented Qt enums so a later enum registration can map
//! `Text.AlignHCenter` and friends onto them without a migration.

/// `Item.transformOrigin` (`Item.TopLeft` … `Item.BottomRight`).
pub mod transform_origin {
    pub const TOP_LEFT: i32 = 0;
    pub const TOP: i32 = 1;
    pub const TOP_RIGHT: i32 = 2;
    pub const LEFT: i32 = 3;
    pub const CENTER: i32 = 4;
    pub const RIGHT: i32 = 5;
    pub const BOTTOM_LEFT: i32 = 6;
    pub const BOTTOM: i32 = 7;
    pub const BOTTOM_RIGHT: i32 = 8;
}

/// `Text.horizontalAlignment` and `Text.verticalAlignment` (`Qt.AlignmentFlag`).
pub mod align {
    pub const LEFT: i32 = 0x1;
    pub const RIGHT: i32 = 0x2;
    pub const H_CENTER: i32 = 0x4;
    pub const JUSTIFY: i32 = 0x8;
    pub const TOP: i32 = 0x20;
    pub const BOTTOM: i32 = 0x40;
    pub const V_CENTER: i32 = 0x80;
}

/// `Text.wrapMode`.
pub mod wrap_mode {
    pub const NO_WRAP: i32 = 0;
    pub const WORD_WRAP: i32 = 1;
    pub const WRAP_ANYWHERE: i32 = 3;
    pub const WRAP: i32 = 4;
}

/// `Text.elide`.
pub mod elide {
    pub const LEFT: i32 = 0;
    pub const RIGHT: i32 = 1;
    pub const MIDDLE: i32 = 2;
    pub const NONE: i32 = 3;
}

/// `Text.lineHeightMode`.
pub mod line_height_mode {
    pub const PROPORTIONAL: i32 = 0;
    pub const FIXED: i32 = 1;
}

/// `Qt.LayoutDirection` (`Row.layoutDirection`).
pub mod layout_direction {
    pub const LEFT_TO_RIGHT: i32 = 0;
    pub const RIGHT_TO_LEFT: i32 = 1;
}

/// `Qt.MouseButton` (`MouseArea.acceptedButtons`, `mouse.button`).
pub mod mouse_button {
    pub const NO_BUTTON: i32 = 0;
    pub const LEFT: i32 = 0x1;
    pub const RIGHT: i32 = 0x2;
    pub const MIDDLE: i32 = 0x4;
    pub const BACK: i32 = 0x8;
    pub const FORWARD: i32 = 0x10;
    pub const ALL: i32 = 0x07ff_ffff;
}

/// `Qt.KeyboardModifier` (`mouse.modifiers`).
pub mod modifier {
    pub const SHIFT: i32 = 0x0200_0000;
    pub const CONTROL: i32 = 0x0400_0000;
    pub const ALT: i32 = 0x0800_0000;
    pub const META: i32 = 0x1000_0000;
}

/// `TapHandler.gesturePolicy`.
pub mod gesture_policy {
    pub const DRAG_THRESHOLD: i32 = 0;
    pub const WITHIN_BOUNDS: i32 = 1;
    pub const RELEASE_WITHIN_BOUNDS: i32 = 2;
    pub const DRAG_WITHIN_BOUNDS: i32 = 3;
}

/// `Loader.status`.
pub mod loader_status {
    pub const NULL: i32 = 0;
    pub const READY: i32 = 1;
    pub const LOADING: i32 = 2;
    pub const ERROR: i32 = 3;
}
