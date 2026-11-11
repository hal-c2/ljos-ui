//! Rules of layout: anchors, positioners, `RowLayout` and `ColumnLayout`.

pub(crate) mod arrange;

use super::Area;

/// The types of QtQuick.Layouts Ljos.Layout calls by another name. A
/// `GridLayout` object is renamed where it is arranged ([`arrange`]), with
/// what the pass checks of it; a name of one in a script is renamed here.
pub const LAYOUT_RENAMES: &[(&str, &str)] = &[("GridLayout", "Grid"), ("StackLayout", "Pages")];

/// What Ljos.Layout calls the type of QtQuick.Layouts `name`, where it
/// calls it otherwise.
pub(crate) fn new_name(name: &str) -> Option<&'static str> {
    LAYOUT_RENAMES
        .iter()
        .find(|(old, _)| *old == name)
        .map(|(_, new)| *new)
}

pub(super) const AREA: Area = Area {
    types: &[],
    members: &[],
};
