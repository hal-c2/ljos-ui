//! How a font names its families ([`FamilyList`]).

use std::sync::Arc;

use crate::Atom;

/// How a font names its families: what `font.families` lists
/// (`QFont::families`) and what its text is laid out with.
///
/// Every name is one family whatever it holds, commas and quotes and all,
/// whether `font.family` named it alone or `font.families` listed it, as
/// in Qt: `setFamily("A, B")` lists the one name "A, B". What is the same
/// font either way has one form: a list of one name is that name as a
/// family, and an empty family is the list of one empty name, as QFont has
/// `setFamily("")` equal `setFamilies({""})`, which is not
/// `setFamilies({})`, the list of none. Two fonts compare by it, in Rust
/// and in JS alike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyList {
    first: Atom,
    /// `Some`: a list, and the names after `first`. `None`: `first` is
    /// the one family, and an empty one names none.
    rest: Option<Arc<[Atom]>>,
}

impl FamilyList {
    /// `font.family` (`QFont::setFamily`): one family, whatever it holds.
    pub fn family(text: Atom) -> Self {
        if text.is_empty() {
            return Self::listed(text, Arc::from([]));
        }
        Self {
            first: text,
            rest: None,
        }
    }

    /// `font.families` (`QFont::setFamilies`): each name one family.
    pub fn list(names: &[Atom]) -> Self {
        match names {
            [] => Self {
                first: Atom::EMPTY,
                rest: None,
            },
            [first, rest @ ..] => Self::listed(first.clone(), rest.into()),
        }
    }

    /// From the two parts a font keeps, put in their one form. A font's
    /// family may have been written alone after it was given a list.
    pub(crate) fn from_parts(first: Atom, rest: Option<Arc<[Atom]>>) -> Self {
        match rest {
            None => Self { first, rest: None },
            Some(rest) => Self::listed(first, rest),
        }
    }

    fn listed(first: Atom, rest: Arc<[Atom]>) -> Self {
        // A name alone is the same as that name for a family, kept as one
        // so the two are one value, with no list to hold. Only an empty
        // name needs the list: alone it names none.
        if rest.is_empty() && !first.is_empty() {
            return Self { first, rest: None };
        }
        Self {
            first,
            rest: Some(rest),
        }
    }

    /// The first family: `font.family`.
    pub fn first(&self) -> &Atom {
        &self.first
    }

    /// The names after the first when the font lists more than one family
    /// or one empty name; `None` when the first is the one family.
    pub fn rest(&self) -> Option<&Arc<[Atom]>> {
        self.rest.as_ref()
    }

    /// The families in the order they are tried (`QFont::families`).
    pub fn names(&self) -> impl Iterator<Item = &Atom> {
        names_of(&self.first, self.rest.as_ref())
    }

    pub(crate) fn into_parts(self) -> (Atom, Option<Arc<[Atom]>>) {
        (self.first, self.rest)
    }

    /// The names after the first as they cross to JS beside it: a JSON
    /// array when the font lists more than its first, or one empty name,
    /// and nothing when the first is the one family. JSON keeps an empty
    /// name, or one with a line break in it, as it was.
    pub(crate) fn rest_text(&self) -> String {
        match &self.rest {
            Some(rest) => {
                let names: Vec<&str> = rest.iter().map(|a| a.as_str()).collect();
                serde_json::to_string(&names).unwrap_or_default()
            }
            None => String::new(),
        }
    }

    /// What [`FamilyList::rest_text`] made, back, with the first family
    /// that crossed beside it.
    pub(crate) fn from_js(first: Atom, rest_text: &str) -> Self {
        let rest = (!rest_text.is_empty())
            .then(|| serde_json::from_str::<Vec<String>>(rest_text).ok())
            .flatten()
            .map(|names| names.iter().map(|n| Atom::from(n.as_str())).collect());
        Self::from_parts(first, rest)
    }
}

/// [`FamilyList::names`] of the two parts. A list keeps an empty first
/// family, as QFont does; an empty family alone names none.
pub(crate) fn names_of<'a>(
    first: &'a Atom,
    rest: Option<&'a Arc<[Atom]>>,
) -> impl Iterator<Item = &'a Atom> {
    (!first.is_empty() || rest.is_some())
        .then_some(first)
        .into_iter()
        .chain(rest.map_or(&[][..], |r| &**r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(l: &FamilyList) -> Vec<&str> {
        l.names().map(|a| a.as_str()).collect()
    }

    #[test]
    fn a_list_of_one_name_is_that_family_whatever_it_holds() {
        let a = |s: &str| Atom::from(s);
        let none = FamilyList::list(&[]);
        assert_eq!(names(&none), Vec::<&str>::new());
        assert_eq!(none, FamilyList::from_js(Atom::EMPTY, &none.rest_text()));
        for one in ["A", "A, B", "\"A\"", " A", "A,"] {
            let l = FamilyList::list(&[a(one)]);
            assert_eq!(l, FamilyList::family(a(one)), "{one:?}");
            assert_eq!(names(&l), [one], "{one:?}");
            assert_eq!(l.rest(), None, "{one:?}");
            assert_eq!(l, FamilyList::from_js(a(one), &l.rest_text()));
        }
        let many = FamilyList::list(&[a("A, B"), a(""), a("C")]);
        assert_eq!(names(&many), ["A, B", "", "C"]);
        assert_eq!(many, FamilyList::from_js(a("A, B"), &many.rest_text()));
    }

    #[test]
    fn one_empty_name_is_a_list_of_one_not_of_none() {
        let a = |s: &str| Atom::from(s);
        let blank = FamilyList::list(&[a("")]);
        assert_eq!(FamilyList::family(a("")), blank);
        assert_ne!(blank, FamilyList::list(&[]));
        assert_eq!(names(&blank), [""]);
        assert_eq!(blank.rest_text(), "[]");
        assert_eq!(blank, FamilyList::from_js(Atom::EMPTY, &blank.rest_text()));
        assert_eq!(
            FamilyList::from_parts(a("A"), Some(Arc::from([]))),
            FamilyList::family(a("A"))
        );
    }
}
