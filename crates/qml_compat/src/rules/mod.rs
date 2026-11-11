//! What Ljós makes of Qt's types, one file per area of the API: the names
//! it gives the types and their members, and the [`Rule`]s that say the
//! rest.
//!
//! The rule: a lane adds rows only to its own area's file. A new area is a
//! new file here, in `src/tests/` and in `docs/dialect/status/`, plus one
//! line in [`AREAS`] (the test
//! `rules::every_area_has_rules_tests_and_status` holds the three lists
//! equal). A Qt type's member renames live in
//! exactly one area, and an old type name is renamed in one area only, so a
//! type has one slice of member renames and one new name (the test
//! `rules::one_area_per_type` holds them to it).

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

pub(crate) use text::font;
pub use text::font::FONT_RENAMES;

use qml_edit::Change;
use qml_syntax::ast::QmlObject;

use crate::{Kept, Rename, Ty};

/// What Ljós makes of the Qt types one rule owns. The pass asks the rule of
/// a type, and knows none of their names itself.
pub(crate) trait Rule: Sync {
    /// The Qt types, or value types (`font`), the rule owns, by name. A
    /// type built on one of them is the rule's too.
    fn owns(&self) -> &'static [&'static str];
    /// What a member `name` of one of them is in Ljós, where the pass has
    /// only the name: read in a script, or bound by a name the rule does
    /// not write itself. `None` leaves it to the area's renames.
    fn member(&self, _name: &str) -> Option<Rename> {
        None
    }
    /// What an object of one of them is changed by, in the pass's
    /// [`Change`]s, or why the file is kept.
    fn object(&self, _object: &QmlObject<'_>, _file: &dyn File) -> Result<Vec<Change>, Kept> {
        Ok(Vec::new())
    }
    /// The members a script may name unqualified, in a scope one of them
    /// is in, that [`File::names_unqualified`] is asked about.
    fn unqualified(&self) -> &'static [&'static str] {
        &[]
    }
}

/// What a rule may ask of the file the pass is lowering.
pub(crate) trait File {
    /// The file's text.
    fn source(&self) -> &str;
    /// Whether `name` is a file of the project beside this one.
    fn is_beside(&self, name: &str) -> bool;
    /// Whether an object aimed at the id `id` (`target: loader`) sets or
    /// handles `member` there.
    fn aimed_at(&self, id: &str, member: &str) -> bool;
    /// Whether a script of `object`'s own, or anywhere in its scope when it
    /// is the root of the file or of an inline component, names one of
    /// `names` unqualified; each is one of a rule's
    /// [`unqualified`](Rule::unqualified).
    fn names_unqualified(&self, object: &QmlObject<'_>, names: &[&str]) -> bool;
}

/// Every rule, joined here and nowhere else; a Qt type has one.
const RULES: &[&dyn Rule] = &[&font::FONT, &loader::source::LOADER];

/// The rule of what the pass knows as `on`, if one owns it or what it is
/// built on.
pub(crate) fn rule_of(on: Ty) -> Option<&'static dyn Rule> {
    let mut ty = match on {
        Ty::Font => return owner("font"),
        Ty::Exact(ty) | Ty::Derived(ty) => Some(ty),
        Ty::Plain | Ty::Unknown | Ty::Native(..) => return None,
    };
    while let Some(at) = ty {
        if let Some(rule) = owner(at.name) {
            return Some(rule);
        }
        ty = at.prototype();
    }
    None
}

/// The rule that owns `name`.
fn owner(name: &str) -> Option<&'static dyn Rule> {
    RULES
        .iter()
        .copied()
        .find(|rule| rule.owns().contains(&name))
}

/// Every name a rule asks about unqualified.
pub(crate) fn unqualified() -> impl Iterator<Item = &'static str> {
    RULES
        .iter()
        .flat_map(|rule| rule.unqualified().iter().copied())
}

/// The renames of one area.
struct Area {
    /// The Qt types Ljós calls by another name: (Qt's name, Ljós's).
    types: &'static [(&'static str, &'static str)],
    /// Per Qt type (and what derives from it), the members Ljós calls by
    /// another name. A property's change signal and its handler follow its
    /// name. Every one is a property: a member that is called is a method,
    /// and not one of these.
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

/// What Ljós calls Qt's type `name`, where it calls it otherwise. Whether a
/// file's `name` is Qt's type is the pass's to say (`Pass::renamed_type`).
pub(crate) fn new_name(name: &str) -> Option<&'static str> {
    type_renames()
        .find(|(old, _)| *old == name)
        .map(|(_, new)| *new)
}

/// The Qt types Ljós calls by another name. Every other type of the slice is
/// called as Qt calls it.
pub fn type_renames() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    AREAS.iter().flat_map(|area| area.types.iter())
}

/// Per Qt type (and what derives from it), the members Ljós calls by another
/// name. A property's change signal and its handler follow its name.
/// Every one is a property: a member that is called is a method, and not
/// one of these.
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
    fn every_area_has_rules_tests_and_status() {
        let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let renames = areas_in(&crate_dir.join("src/rules"), "rs", &["mod"]);
        // `groups` tests how a project's files lower together, which is
        // the pass's own, not an area's.
        let tests = areas_in(&crate_dir.join("src/tests"), "rs", &["mod", "groups"]);
        let status = areas_in(&crate_dir.join("../../docs/dialect/status"), "md", &[]);
        assert_eq!(renames, tests, "src/rules and src/tests name other areas");
        assert_eq!(
            renames, status,
            "src/rules and docs/dialect/status name other areas"
        );
        assert_eq!(renames.len(), AREAS.len(), "a rules file is not in AREAS");
    }

    #[test]
    fn one_area_per_type() {
        let mut renamed = HashSet::new();
        for (old, _) in type_renames() {
            assert!(renamed.insert(*old), "`{old}` is renamed in two areas");
        }
        let mut ruled = HashSet::new();
        for name in RULES.iter().flat_map(|rule| rule.owns()) {
            assert!(ruled.insert(*name), "`{name}` is owned by two rules");
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
