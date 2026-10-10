//! The QtQuick enums this crate uses.
//!
//! Each type registers its enums (`Text.AlignHCenter`, `Item.Center`)
//! through the `*_enums` functions here. Enum-typed properties are stored
//! as `i32`, the number QML and JS read; the modules give the values names
//! for Rust code. The values follow the documented Qt enums.

/// What `#[qml(enums = …)]` takes: every enum's name and its keys.
pub(crate) type EnumTable = Vec<(&'static str, Vec<(&'static str, i64)>)>;

fn keys(list: &[(&'static str, i32)]) -> Vec<(&'static str, i64)> {
    list.iter().map(|(k, v)| (*k, i64::from(*v))).collect()
}

/// `Item`'s enums, which every visual type inherits (`Rectangle.Center`).
pub(crate) fn item_enums() -> EnumTable {
    use transform_origin as o;
    vec![(
        "TransformOrigin",
        keys(&[
            ("TopLeft", o::TOP_LEFT),
            ("Top", o::TOP),
            ("TopRight", o::TOP_RIGHT),
            ("Left", o::LEFT),
            ("Center", o::CENTER),
            ("Right", o::RIGHT),
            ("BottomLeft", o::BOTTOM_LEFT),
            ("Bottom", o::BOTTOM),
            ("BottomRight", o::BOTTOM_RIGHT),
        ]),
    )]
}

pub(crate) fn text_enums() -> EnumTable {
    vec![
        (
            "HAlignment",
            keys(&[
                ("AlignLeft", align::LEFT),
                ("AlignRight", align::RIGHT),
                ("AlignHCenter", align::H_CENTER),
                ("AlignJustify", align::JUSTIFY),
            ]),
        ),
        (
            "VAlignment",
            keys(&[
                ("AlignTop", align::TOP),
                ("AlignBottom", align::BOTTOM),
                ("AlignVCenter", align::V_CENTER),
            ]),
        ),
        (
            "TextElideMode",
            keys(&[
                ("ElideLeft", elide::LEFT),
                ("ElideRight", elide::RIGHT),
                ("ElideMiddle", elide::MIDDLE),
                ("ElideNone", elide::NONE),
            ]),
        ),
        (
            "WrapMode",
            keys(&[
                ("NoWrap", wrap_mode::NO_WRAP),
                ("WordWrap", wrap_mode::WORD_WRAP),
                ("WrapAnywhere", wrap_mode::WRAP_ANYWHERE),
                ("Wrap", wrap_mode::WRAP),
            ]),
        ),
        (
            "LineHeightMode",
            keys(&[
                ("ProportionalHeight", line_height_mode::PROPORTIONAL),
                ("FixedHeight", line_height_mode::FIXED),
            ]),
        ),
    ]
}

pub(crate) fn tap_handler_enums() -> EnumTable {
    use gesture_policy as g;
    vec![(
        "GesturePolicy",
        keys(&[
            ("DragThreshold", g::DRAG_THRESHOLD),
            ("WithinBounds", g::WITHIN_BOUNDS),
            ("ReleaseWithinBounds", g::RELEASE_WITHIN_BOUNDS),
            ("DragWithinBounds", g::DRAG_WITHIN_BOUNDS),
        ]),
    )]
}

pub(crate) fn loader_enums() -> EnumTable {
    use loader_status as l;
    vec![(
        "Status",
        keys(&[
            ("Null", l::NULL),
            ("Ready", l::READY),
            ("Loading", l::LOADING),
            ("Error", l::ERROR),
        ]),
    )]
}

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
