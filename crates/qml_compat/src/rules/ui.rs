//! Rules of the core visual types: `Item`, `Rectangle`, pointer input.

use super::Area;

pub(super) const AREA: Area = Area {
    types: &[("MouseArea", "PointerArea")],
    members: &[("Rectangle", &[("color", "fill")])],
};
