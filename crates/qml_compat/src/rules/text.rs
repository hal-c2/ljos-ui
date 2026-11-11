//! Rules of text: `Text`, `TextInput`, `TextEdit`, fonts.

use super::Area;

pub(super) const AREA: Area = Area {
    types: &[],
    members: &[(
        "Text",
        &[
            ("horizontalAlignment", "align"),
            ("wrapMode", "wrap"),
            ("textFormat", "format"),
        ],
    )],
};

/// The members of a font Ljós calls by another name, wherever the font is.
/// `bold` is a `weight` (`Pass::font_part`) and `pointSize` a run-time
/// conversion, so neither is a rename.
pub const FONT_RENAMES: &[(&str, &str)] = &[("pixelSize", "size")];
