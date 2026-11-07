//! The names Ljós gives Qt's types and their members, one file per area of
//! the API.
//!
//! The rule: a lane adds rows only to its own area's file. A new area is a
//! new file plus one line in [`AREAS`]. A Qt type's member renames live in
//! exactly one area, and an old type name is renamed in one area only, so a
//! type has one slice of member renames and one new name (the test
//! `renames::one_area_per_type` holds them to it).

mod controls;
mod core;
mod dialogs;
mod gestures;
mod layout;
mod loader;
mod models;
mod shapes;
mod states;
mod text;
mod ui;
mod window;

/// The renames of one area.
struct Area {
    /// The Qt types Ljós calls by another name: (Qt's name, Ljós's).
    types: &'static [(&'static str, &'static str)],
    /// Per Qt type (and what derives from it), the members Ljós calls by
    /// another name. A property's change signal and its handler follow its
    /// name.
    members: &'static [(&'static str, &'static [(&'static str, &'static str)])],
}

/// Every area, joined here and nowhere else.
const AREAS: &[&Area] = &[
    &ui::AREA,
    &window::AREA,
    &loader::AREA,
    &text::AREA,
    &gestures::AREA,
    &shapes::AREA,
    &layout::AREA,
    &core::AREA,
    &models::AREA,
    &controls::AREA,
    &states::AREA,
    &dialogs::AREA,
];

/// The Qt types Ljós calls by another name. Every other type of the slice is
/// called as Qt calls it.
pub fn type_renames() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    AREAS.iter().flat_map(|area| area.types.iter())
}

/// Per Qt type (and what derives from it), the members Ljós calls by another
/// name. A property's change signal and its handler follow its name.
pub fn member_renames()
-> impl Iterator<Item = &'static (&'static str, &'static [(&'static str, &'static str)])> {
    AREAS.iter().flat_map(|area| area.members.iter())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn one_area_per_type() {
        let mut renamed = HashSet::new();
        for (old, _) in type_renames() {
            assert!(renamed.insert(*old), "`{old}` is renamed in two areas");
        }
        let mut with_members = HashSet::new();
        for (ty, _) in member_renames() {
            assert!(
                with_members.insert(*ty),
                "`{ty}` has member renames in two areas"
            );
        }
    }
}
