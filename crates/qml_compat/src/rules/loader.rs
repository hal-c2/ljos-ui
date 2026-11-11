//! Rules of loading: `Loader`, components, dynamic creation.

pub(crate) mod source;

use super::Area;

pub(super) const AREA: Area = Area {
    types: &[],
    members: &[
        // A `source` naming a file beside this one is `component` too,
        // written by the `Loader` rule (`source.rs`).
        ("Loader", &[("sourceComponent", "component")]),
    ],
};
