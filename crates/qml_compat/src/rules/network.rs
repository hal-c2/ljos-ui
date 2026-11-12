//! Rules of the network: `XMLHttpRequest` written as `fetch`
//! (docs/DIALECT.md §4.19).

pub(crate) mod request;

use super::Area;

pub(super) const AREA: Area = Area {
    types: &[],
    members: &[],
};
