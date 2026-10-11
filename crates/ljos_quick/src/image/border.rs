//! `BorderImage`: a picture cut into nine parts, whose corners keep their
//! size while the edges and the middle stretch or repeat.

use ljos_engine::{Engine, ObjectId, QmlObject};
use ljos_render::{Insets, TileMode};

use super::{ImageBase, Shape, keys, tile_mode};
use crate::enums::EnumTable;

fn border_image_enums() -> EnumTable {
    use tile_mode as t;
    vec![(
        "TileMode",
        keys(&[
            ("Stretch", t::STRETCH),
            ("Repeat", t::REPEAT),
            ("Round", t::ROUND),
        ]),
    )]
}

/// `BorderImage` (QtQuick).
///
/// Its implicit size is the picture's. `.sci` description files are not
/// read: the borders come from the `border` group.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "BorderImage",
    extends = ImageBase,
    enums = border_image_enums
)]
pub struct BorderImage {
    #[qml(base)]
    pub base: ImageBase,
    #[qml(group)]
    pub border: BorderImageBorder,
    /// [`tile_mode`]: how the top, middle and bottom parts fill sideways.
    #[qml(notify, enumeration = "TileMode")]
    pub horizontal_tile_mode: i32,
    /// [`tile_mode`]: how the left, middle and right parts fill downwards.
    #[qml(notify, enumeration = "TileMode")]
    pub vertical_tile_mode: i32,
}

/// The `border` group of [`BorderImage`]: how many pixels of the picture
/// each edge keeps.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "BorderImageBorder", uncreatable)]
pub struct BorderImageBorder {
    #[qml(notify)]
    pub left: i32,
    #[qml(notify)]
    pub top: i32,
    #[qml(notify)]
    pub right: i32,
    #[qml(notify)]
    pub bottom: i32,
}

fn mode(v: f64) -> TileMode {
    match v as i32 {
        tile_mode::REPEAT => TileMode::Repeat,
        tile_mode::ROUND => TileMode::Round,
        _ => TileMode::Stretch,
    }
}

/// Reads what a BorderImage paints with (tracked).
pub(super) fn shape(e: &Engine, obj: ObjectId, mirror: bool) -> Shape {
    let side = |name| e.read_by_name(obj, name).to_number().max(0.0) as f32;
    let tile = |p: ljos_engine::PropIndex| mode(e.read(obj, p).to_number());
    Shape::Border {
        insets: Insets {
            left: side("border.left"),
            top: side("border.top"),
            right: side("border.right"),
            bottom: side("border.bottom"),
        },
        tile: [
            tile(BorderImage::HORIZONTAL_TILE_MODE.index),
            tile(BorderImage::VERTICAL_TILE_MODE.index),
        ],
        mirror,
    }
}
