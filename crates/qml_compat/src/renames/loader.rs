//! Renames of loading: `Loader`, components, dynamic creation.

use super::Area;

pub(super) const AREA: Area = Area {
    types: &[],
    members: &[
        // A `source` naming a file beside this one is `component` too,
        // written by `Pass::loader_source`.
        ("Loader", &[("sourceComponent", "component")]),
    ],
};
