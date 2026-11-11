//! Rules of models and views: `ListModel`, `Repeater`, `ListView`.

use super::Area;

pub(super) const AREA: Area = Area {
    types: &[],
    members: &[("ListView", &[("add", "enter"), ("remove", "exit")])],
};
