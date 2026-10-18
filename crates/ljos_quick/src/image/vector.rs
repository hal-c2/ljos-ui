//! `VectorImage` (`QtQuick.VectorImage`): an SVG file shown as a drawing.
//!
//! Qt turns the file into shape items; Ljós draws it as an `Image` does,
//! made again at the size it is painted (and the display's scale), so it
//! is as sharp. What differs from an `Image` follows Qt 6.11.2 as measured
//! on qml6: the fill modes are numbered otherwise and stretch by default,
//! and a drawing that does not fill the item keeps to its top left corner.
//!
//! The drawing has no shape items to walk, so a script that recolours
//! them finds nothing and the drawing keeps the file's colours.

use ljos_engine::QmlObject;

use super::{EnumTable, Image, align, keys};

/// `VectorImage.fillMode`.
pub mod vector_fill_mode {
    pub const NO_RESIZE: i32 = 0;
    pub const PRESERVE_ASPECT_FIT: i32 = 1;
    pub const PRESERVE_ASPECT_CROP: i32 = 2;
    pub const STRETCH: i32 = 3;
}

fn vector_enums() -> EnumTable {
    use vector_fill_mode as f;
    vec![
        (
            "FillMode",
            keys(&[
                ("NoResize", f::NO_RESIZE),
                ("PreserveAspectFit", f::PRESERVE_ASPECT_FIT),
                ("PreserveAspectCrop", f::PRESERVE_ASPECT_CROP),
                ("Stretch", f::STRETCH),
            ]),
        ),
        (
            "RendererType",
            keys(&[("GeometryRenderer", 0), ("CurveRenderer", 1)]),
        ),
    ]
}

/// `VectorImage` (QtQuick.VectorImage).
///
/// Every renderer draws the same; `animations` is not there.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.VectorImage",
    name = "VectorImage",
    extends = Image,
    enums = vector_enums
)]
pub struct VectorImage {
    #[qml(base)]
    pub image: Image,
    #[qml(notify, enumeration = "RendererType")]
    pub preferred_renderer_type: i32,
    #[qml(notify)]
    pub assume_trusted_source: bool,
    #[qml(notify)]
    pub asynchronous_shapes: bool,
}

impl Default for VectorImage {
    fn default() -> Self {
        Self {
            image: Image {
                fill_mode: vector_fill_mode::STRETCH,
                horizontal_alignment: align::LEFT,
                vertical_alignment: align::TOP,
                ..Image::default()
            },
            preferred_renderer_type: 0,
            assume_trusted_source: false,
            asynchronous_shapes: false,
        }
    }
}
