//! Rules of text: `Text`, `TextInput`, `TextEdit`, fonts, translated texts.

pub(crate) mod font;
pub(crate) mod messages;

use super::Area;

pub(super) const AREA: Area = Area {
    types: &[],
    members: &[],
};
