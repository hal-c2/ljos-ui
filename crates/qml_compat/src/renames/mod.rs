//! The names Ljós gives Qt's types and their members, one file per area of
//! the API.
//!
//! The rule: a lane adds rows only to its own area's file. A new area is a
//! new file here, in `src/tests/` and in `docs/dialect/status/`, plus one
//! line in [`AREAS`] (the test
//! `renames::every_area_has_renames_tests_and_status` holds the three lists
//! equal). A Qt type's member renames live in
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
mod natives;
mod shapes;
mod states;
mod text;
mod tools;
mod types;
mod ui;
mod window;

pub use text::FONT_RENAMES;

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
    &natives::AREA,
    &tools::AREA,
    &types::AREA,
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

    /// The file stems of `dir` with `ext`, but for the shared ones.
    fn areas_in(dir: &std::path::Path, ext: &str, shared: &[&str]) -> Vec<String> {
        let mut areas: Vec<String> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .map(|entry| entry.expect("a directory entry").path())
            .filter(|path| path.extension().is_some_and(|e| e == ext))
            .filter_map(|path| Some(path.file_stem()?.to_str()?.to_owned()))
            .filter(|stem| !shared.contains(&stem.as_str()))
            .collect();
        areas.sort();
        areas
    }

    // A lane starting an area finds its renames, tests and status files
    // already there, so no two lanes make the same file in their first PRs.
    #[test]
    fn every_area_has_renames_tests_and_status() {
        let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let renames = areas_in(&crate_dir.join("src/renames"), "rs", &["mod"]);
        // `groups` tests how a project's files lower together, which is
        // the pass's own, not an area's.
        let tests = areas_in(&crate_dir.join("src/tests"), "rs", &["mod", "groups"]);
        let status = areas_in(&crate_dir.join("../../docs/dialect/status"), "md", &[]);
        assert_eq!(renames, tests, "src/renames and src/tests name other areas");
        assert_eq!(
            renames, status,
            "src/renames and docs/dialect/status name other areas"
        );
        assert_eq!(renames.len(), AREAS.len(), "a renames file is not in AREAS");
    }

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
