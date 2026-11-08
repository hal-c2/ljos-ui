//! The members of Qt's `Qt` global that `Ljos.Core` gives under its own
//! names (docs/DIALECT.md §4.3): `Qt.point(…)` is `point(…)`,
//! `Qt.callLater(…)` is `later(…)`, `Qt.platform` is `App.platform`.
//!
//! `Qt` stays a global of a lowered file, so a member left as written still
//! runs. A member is lowered only where its new name can be nothing else:
//! the new name is a free name of the lowered file, found after everything
//! the file has in sight, so anything in sight of that name would take it.
//! DECISION: the pass cannot see every name in sight, so it lowers a member
//! only when the name it becomes is no other word of the file (outside a
//! property's type), no member of a type the file's objects are, no name a
//! file beside it declares on the root it is built on, and the file has no
//! object whose type the pass does not know and no delegate, whose model's
//! roles would come first. A capitalised name (`App`) may also be a type of a
//! directory the file imports by path, which the pass cannot see, so under
//! such an import it is not free. Otherwise the member stays Qt's, and the
//! file is lowered all the same. A project lowers a file again when a file of
//! a name the pass gives comes beside it or goes ([`introduced`]).

use std::cell::RefCell;
use std::ops::Range;

use qt_types::Type;

/// How a member of `Qt` is written in Ljós's API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Becomes {
    /// `Qt.point` is `point`: `Qt.` and the member are replaced by the name.
    Function(&'static str),
    /// `Qt.platform` is `App.platform`: `Qt` is replaced by `App`.
    OnApp,
}

const APP: &str = "App";

/// The makers `Ljos.Core` calls as `Qt` does.
const SAME: &[&str] = &[
    "point",
    "size",
    "rect",
    "vector2d",
    "vector3d",
    "vector4d",
    "quaternion",
    "rgba",
    "hsla",
    "hsva",
];

/// The type name lowering `Qt.member` brings into the file, if any: a file
/// of that name beside it would take it, so the project lowers the file
/// again when one comes or goes.
pub(crate) fn introduced(member: &str) -> Option<&'static str> {
    match becomes(member)? {
        Becomes::OnApp => Some(APP),
        Becomes::Function(_) => None,
    }
}

fn becomes(member: &str) -> Option<Becomes> {
    if let Some(name) = SAME.iter().find(|name| **name == member) {
        return Some(Becomes::Function(name));
    }
    Some(match member {
        "callLater" => Becomes::Function("later"),
        "resolvedUrl" => Becomes::Function("url"),
        "platform" | "openUrlExternally" | "quit" => Becomes::OnApp,
        _ => return None,
    })
}

/// One `Qt.member` of the file, to be lowered if nothing else is its name.
#[derive(Debug)]
struct Use {
    /// What is replaced.
    range: Range<usize>,
    replacement: &'static str,
    /// The name it becomes, which must be free.
    name: &'static str,
}

/// What the pass gathers, over the file, to say which members it lowers.
#[derive(Debug, Default)]
pub(crate) struct Members {
    uses: Vec<Use>,
    /// The types of the file's objects, whose members are in sight.
    types: Vec<&'static Type>,
    /// What the roots of the files beside it, that the file's objects are
    /// built on, declare.
    declared: RefCell<Vec<String>>,
    /// Where the types of the properties the file declares are written:
    /// `point` of `property point at` is no name in sight.
    typed: Vec<Range<usize>>,
    /// An object whose type the pass does not know, or a delegate.
    unsure: bool,
    /// The file imports a directory by path, or a module of the project,
    /// not under a name: a type it may bring takes a name the pass gives.
    pathed: bool,
}

impl Members {
    /// `Qt.member`, at `qt` (the span of `Qt`) and `member` (that of the
    /// member's name).
    pub(crate) fn qt(&mut self, qt: Range<usize>, member: &str, at: Range<usize>) {
        let Some(becomes) = becomes(member) else {
            return;
        };
        self.uses.push(match becomes {
            Becomes::Function(name) => Use {
                range: qt.start..at.end,
                replacement: name,
                name,
            },
            Becomes::OnApp => Use {
                range: qt,
                replacement: APP,
                name: APP,
            },
        });
    }

    pub(crate) fn object_type(&mut self, ty: Option<&'static Type>) {
        match ty {
            Some(ty) => self.types.push(ty),
            None => self.unsure = true,
        }
    }

    /// `import "qml"` or `import Gauges`: what the directory or the
    /// project's module has is in sight.
    pub(crate) fn imports_by_path(&mut self) {
        self.pathed = true;
    }

    pub(crate) fn delegate(&mut self) {
        self.unsure = true;
    }

    pub(crate) fn declared_beside(&self, names: &[String]) {
        self.declared.borrow_mut().extend(names.iter().cloned());
    }

    pub(crate) fn typed(&mut self, range: Range<usize>) {
        self.typed.push(range);
    }

    /// The changes that lower the members whose names are free, and whether
    /// there are any (the file then imports `Ljos.Core`).
    pub(crate) fn settle(
        self,
        source: &str,
        beside: impl Fn(&str) -> bool,
    ) -> Vec<(Range<usize>, &'static str)> {
        if self.unsure {
            return Vec::new();
        }
        let declared = self.declared.borrow();
        let free = |name: &str| {
            !(self.pathed && name.starts_with(|c: char| c.is_ascii_uppercase()))
                && !beside(name)
                && !declared.iter().any(|d| d == name)
                && !self.types.iter().any(|ty| {
                    ty.property(name).is_some() || ty.has_method(name) || ty.signal(name).is_some()
                })
                && words(source, name).all(|at| {
                    self.uses.iter().any(|u| u.range.contains(&at))
                        || self.typed.iter().any(|t| t.contains(&at))
                })
        };
        self.uses
            .iter()
            .filter(|u| free(u.name))
            .map(|u| (u.range.clone(), u.replacement))
            .collect()
    }
}

/// Where `name` is a whole word of `source`, as byte offsets.
fn words<'a>(source: &'a str, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    source.match_indices(name).filter_map(move |(at, _)| {
        let before = source[..at].chars().next_back();
        let after = source[at + name.len()..].chars().next();
        (!before.is_some_and(is_word) && !after.is_some_and(is_word)).then_some(at)
    })
}
