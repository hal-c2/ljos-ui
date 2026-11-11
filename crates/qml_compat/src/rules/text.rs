//! Rules of text: `Text`, `TextInput`, `TextEdit`, fonts.

pub(crate) mod font;

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
