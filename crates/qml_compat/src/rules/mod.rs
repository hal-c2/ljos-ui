//! What Ljós makes of Qt's types, one file per area of the API: the names
//! it gives the types and their members, and the [`Rule`]s that say the
//! rest.
//!
//! The rule: a lane adds rows only to its own area's file. A new area is a
//! new file here, in `src/tests/` and in `docs/dialect/status/`, plus one
//! line in [`AREAS`] (the test
//! `rules::every_area_has_rules_tests_and_status` holds the three lists
//! equal). A member Ljós calls by another name is renamed by the runtime's
//! face of the type, which Ljos.Ui's description says ([`member_renames`]),
//! and nowhere here. An old type name is renamed in one area only, so a
//! type has one slice of member renames and one new name (the test
//! `rules::one_area_per_type` holds them to it).

mod controls;
mod core;
mod dialogs;
pub(crate) mod gestures;
pub(crate) mod layout;
mod loader;
mod models;
mod natives;
pub(crate) mod network;
mod shapes;
pub(crate) mod states;
mod text;
mod tools;
mod types;
mod ui;
mod window;

pub(crate) use gestures::gesture;
pub use layout::LAYOUT_RENAMES;
pub(crate) use layout::arrange;
pub(crate) use text::font;
pub use text::font::FONT_RENAMES;

use std::ops::Range;
use std::sync::OnceLock;

use qml_edit::Change;
use qml_syntax::ast::QmlObject;
use qt_types::{Module, Type};

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
    /// What an object of one of them, or built on one, is changed by, in
    /// the pass's [`Change`]s, or why the file is kept. `on` is what the
    /// pass knows of its type; `a_root` when what uses or holds it may
    /// size it, which the file does not show.
    fn object(
        &self,
        _object: &QmlObject<'_>,
        _on: Ty,
        _a_root: bool,
        _file: &dyn File,
    ) -> Result<Vec<Change>, Kept> {
        Ok(Vec::new())
    }
    /// What the `index`th member of `parent`, an object of one of them, is
    /// written as instead of what the pass makes of it, if the rule says.
    fn child(
        &self,
        _parent: &QmlObject<'_>,
        _on: Ty,
        _index: usize,
        _file: &dyn File,
    ) -> Option<Child> {
        None
    }
    /// The members a script may name unqualified, in a scope one of them
    /// is in, that [`File::names_unqualified`] is asked about.
    fn unqualified(&self) -> &'static [&'static str] {
        &[]
    }
    /// What the members of an object of one of them are aimed at, when it
    /// is not the object itself.
    fn aim(&self, _object: &QmlObject<'_>, _on: Ty, _file: &dyn File) -> Option<Aim> {
        None
    }
}

/// What an object's members are aimed at, as a rule says.
pub(crate) struct Aim {
    /// What the pass knows of the target.
    pub(crate) at: Ty,
    pub(crate) aimed: Aimed,
    /// The id the members are written through, where Ljós names the
    /// target in each member rather than once.
    pub(crate) through: Option<Through>,
}

/// Which of an object's members are its target's.
#[derive(Clone, Copy)]
pub(crate) enum Aimed {
    /// Every one but those named `own`, and its ids. `what` names the
    /// target where a member keeps the file.
    All {
        own: &'static [&'static str],
        what: &'static str,
    },
    /// Only its handlers: `onClicked: …` and `function onClicked() …`.
    Handlers,
}

/// The id an object's members are written through.
pub(crate) struct Through {
    pub(crate) id: String,
    /// The binding of the target, which goes.
    pub(crate) taken_out: Range<usize>,
}

/// What a rule writes for a child: the pass lowers it as it would, then
/// makes these edits of it.
pub(crate) struct Child {
    /// Text replaced as given; what the pass makes inside these ranges is
    /// dropped.
    pub(crate) replace: Vec<(Range<usize>, String)>,
    /// Bindings copied, as the pass lowers them, into an object of their
    /// own written at `end`.
    pub(crate) copies: Vec<Range<usize>>,
    /// The type of that object.
    pub(crate) copied_into: &'static str,
    /// Where it goes: the end of the child.
    pub(crate) end: usize,
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
    /// The type of Qt's (QtQuick, QtQuick.Layouts or the controls) `name`
    /// names in the file.
    fn qt_type(&self, name: &str) -> Option<&'static Type>;
    /// For a file of the project or an inline component, the names its
    /// root binds, with those of the roots it is built on; `None` for a
    /// type that is neither.
    fn root_bindings(&self, name: &str) -> Option<Vec<String>>;
    /// Qt's QtQuick, as the file imports it.
    fn quick(&self) -> &'static Module;
    /// Ljós's Ljos.Ui, which the lowered file imports.
    fn ui(&self) -> &'static Module;
    /// What the pass knows of the object with the id `name`, if it is an
    /// id of the file.
    fn id(&self, name: &str) -> Option<Ty>;
    /// Whether `name` is an id of the file.
    fn is_id(&self, name: &str) -> bool {
        self.id(name).is_some()
    }
    /// Whether a script of the file reads `name` unqualified, or a
    /// binding's name starts with it.
    fn is_read(&self, name: &str) -> bool;
    /// What an animation here moves when it says no target: the object of
    /// a `Behavior` or value source, or what a state changes.
    fn animating(&self) -> states::animation::Animating;
    /// What a member `name` of an object the pass knows as `on` is in Ljós.
    fn member_of(&self, name: &str, on: Ty) -> Rename;
    /// What the file's states change `name` of: the objects, as the pass
    /// knows them, a `PropertyChanges` sets it on.
    fn changed(&self, name: &str) -> Vec<Ty>;
}

/// Every rule, joined here and nowhere else; a Qt type is owned by one.
const RULES: &[&dyn Rule] = &[
    &font::FONT,
    &loader::source::LOADER,
    &arrange::ARRANGE,
    &gesture::GESTURE,
    &states::changes::CHANGES,
    &states::animation::ANIMATION,
    &core::connections::CONNECTIONS,
];

/// The rules of what the pass knows as `on`: the one that owns it, then
/// those that own what it is built on, nearest first.
pub(crate) fn rules_of(on: Ty) -> impl Iterator<Item = &'static dyn Rule> {
    let (font, mut ty) = match on {
        Ty::Font => (owner("font"), None),
        Ty::Exact(ty) | Ty::Derived(ty) => (None, Some(ty)),
        Ty::Plain | Ty::Unknown | Ty::Native(..) | Ty::Kept => (None, None),
    };
    let built_on = std::iter::from_fn(move || {
        let at = ty?;
        ty = at.prototype();
        Some(at.name)
    });
    font.into_iter().chain(built_on.filter_map(owner))
}

/// The rule that owns `name`.
fn owner(name: &str) -> Option<&'static dyn Rule> {
    RULES
        .iter()
        .copied()
        .find(|rule| rule.owns().contains(&name))
}

/// Whether `inner` lies within `outer`.
pub(crate) fn within(inner: &Range<usize>, outer: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// `range` of `source` with the `changes` that lie in it made.
pub(crate) fn lowered_text(source: &str, range: &Range<usize>, changes: &[Change]) -> String {
    let mut inside: Vec<Change> = changes
        .iter()
        .filter(|c| within(&c.range, range))
        .map(|c| Change {
            range: c.range.start - range.start..c.range.end - range.start,
            replacement: c.replacement.clone(),
        })
        .collect();
    inside.sort_by_key(|c| c.range.start);
    inside.dedup();
    qml_edit::splice(&source[range.clone()], &inside)
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
    /// Per Qt type, the members Ljós calls by another name where no face
    /// of Ljos.Ui says them.
    members: &'static [(&'static str, &'static [(&'static str, &'static str)])],
}

/// Every area, joined here and nowhere else.
/// Ljós's names for Qt's types that a script reads as a global of
/// JavaScript's: such a type is renamed where a type goes and keeps its file
/// where a script names it.
pub(crate) const JAVASCRIPT_NAMED: &[&str] = &["Object"];

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
    &network::AREA,
];

/// What Ljós calls Qt's type `name`, where it calls it otherwise. Whether a
/// file's `name` is Qt's type is the scope's to say (`Scope::renamed_type`).
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

/// Per Qt type, the members Ljós calls by another name: a type's own, never
/// those of what it derives from, since only a face renames and a type
/// built on the faced one is built on the toolkit's. A property's change
/// signal and its handler follow its name.
/// Every one is a property: a member that is called is a method, and not
/// one of these.
///
/// DECISION: the runtime's faces own a rename (`ljos_quick`'s `ljos_ui.rs`),
/// and Ljos.Ui's description says each one with the name the faced type
/// gives it (`prop wrap WrapMode was=wrapMode`). A rename is read from
/// there, under the Qt name of the type the face is of, so this crate links
/// no runtime and nothing here can drift from it.
pub fn member_renames()
-> impl Iterator<Item = (&'static str, &'static [(&'static str, &'static str)])> {
    static FACES: OnceLock<Vec<Face>> = OnceLock::new();
    let faces = FACES.get_or_init(faces);
    AREAS
        .iter()
        .flat_map(|area| area.members.iter().copied())
        .chain(faces.iter().map(|(ty, renames)| (*ty, renames.as_slice())))
}

/// A face's renames, by the Qt name of the type it faces.
type Face = (&'static str, Vec<(&'static str, &'static str)>);

/// The renames of each face of Ljos.Ui that has any.
fn faces() -> Vec<Face> {
    let Some(ui) = qt_types::module(crate::MODULE) else {
        return Vec::new();
    };
    let names = qt_types::native_types(crate::MODULE).unwrap_or_default();
    names
        .into_iter()
        .filter_map(|name| {
            let renames: Vec<_> = ui.type_named(name)?.renamed().collect();
            let qt = type_renames()
                .find(|(_, new)| *new == name)
                .map_or(name, |(old, _)| *old);
            (!renames.is_empty()).then_some((qt, renames))
        })
        .collect()
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
                with_members.insert(ty),
                "`{ty}` has member renames in two areas, or in an area and a face"
            );
        }
    }
}
