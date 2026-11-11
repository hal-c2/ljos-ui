//! Rules of windows: `Window`, `ApplicationWindow`, screens.

use super::Area;

pub(super) const AREA: Area = Area {
    types: &[],
    members: &[
        // A window's `color` is what is behind its items, not a fill: Qt's
        // `Window` is no `Rectangle`, and Ljós's API keeps the name. Ljós's own
        // table has it derive from `Rectangle`, which is how it is built.
        ("Window", &[]),
    ],
};
