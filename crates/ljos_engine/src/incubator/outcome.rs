//! What becomes of a creation once it is over: the objects of one that
//! failed go, the `Component` a script asked is left as Qt leaves it, and
//! the error is told in Qt's words.
//!
//! Qt changes `Component.status` without saying so (no `statusChanged`),
//! and so does this: [`after`] is the whole rule, a row for each way a
//! script asks a `Component` for an object.

use super::Creation;
use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::qtqml::Component;

/// `Component.status` values.
mod status {
    pub(super) const READY: i32 = 1;
    pub(super) const ERROR: i32 = 3;
}

/// How a script asked a `Component` for an object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Asked {
    /// `createObject`.
    Created,
    /// `incubateObject`, at once or later.
    Incubated,
}

/// What a creation came to, as far as its `Component` is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ended<'a> {
    Made,
    /// A `required property` left unset: Qt's lines for it.
    RequiredNotSet(&'a str),
    /// Any other failure.
    Failed,
}

impl<'a> From<Result<(), &'a EngineError>> for Ended<'a> {
    fn from(r: Result<(), &'a EngineError>) -> Self {
        match r {
            Ok(()) => Self::Made,
            Err(EngineError::RequiredNotSet(why)) => Self::RequiredNotSet(why),
            Err(_) => Self::Failed,
        }
    }
}

/// What a creation does to its `Component`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Change<'a> {
    Keep,
    /// Ready again, with nothing for `errorString()`.
    Ready,
    /// Error, with this for `errorString()`.
    Error(&'a str),
}

/// What `ended` does to a `Component` in `status`, which has a template
/// or not, for an object `asked` for. Measured on qml6 (6.11, 6.12):
///
/// | asked     | made                 | a required property unset | other |
/// |-----------|----------------------|---------------------------|-------|
/// | Created   | Ready, if it was Error and can make | Error             | keep  |
/// | Incubated | keep                 | keep (the Incubator is Error) | keep |
pub(crate) fn after<'a>(
    asked: Asked,
    status: i32,
    has_template: bool,
    ended: Ended<'a>,
) -> Change<'a> {
    match (asked, ended) {
        (Asked::Created, Ended::RequiredNotSet(why)) => Change::Error(why),
        (Asked::Created, Ended::Made) if status == status::ERROR && has_template => Change::Ready,
        (Asked::Created, _) | (Asked::Incubated, _) => Change::Keep,
    }
}

/// Whether a `Component` in `status` makes nothing for an object `asked`
/// for: one a createObject left in Error refuses incubateObject until a
/// createObject succeeds, and createObject asks again whatever it is in.
pub(crate) fn refuses(asked: Asked, status: i32) -> bool {
    asked == Asked::Incubated && status == status::ERROR
}

impl EngineError {
    /// What Qt says of the error, a warning a line: one for each required
    /// property left unset, or the error whole.
    pub(crate) fn said(&self) -> Vec<String> {
        match self {
            Self::RequiredNotSet(m) => m.lines().map(str::to_owned).collect(),
            e => vec![e.to_string()],
        }
    }
}

impl Engine {
    /// Destroys the root of a creation that failed before any of it
    /// completed. While a reload makes the new tree, nothing of it is to
    /// be heard.
    pub(crate) fn discard_creation(&self, root: ObjectId) {
        self.reload_hushed(|| self.destroy_now(root));
    }

    /// Destroys what an allocation that cannot go on has made: all of it
    /// is owned by the first object, which nothing refers to yet.
    pub(super) fn discard_allocation(&self, cr: &Creation) {
        if let Some(&first) = cr.all.first() {
            self.discard_creation(first);
        }
    }

    /// Whether `component` makes nothing for an object `asked` for
    /// ([`refuses`]).
    pub(crate) fn component_refuses(&self, component: ObjectId, asked: Asked) -> bool {
        self.with::<Component, _>(component, |c| refuses(asked, c.status))
            .unwrap_or(false)
    }

    /// Leaves `component` as `ended` leaves it for an object `asked` for
    /// ([`after`]). Not told: Qt says nothing either.
    pub(crate) fn component_note(&self, component: ObjectId, asked: Asked, ended: Ended<'_>) {
        let _ = self.with_mut::<Component, _>(component, |c| {
            match after(asked, c.status, c.template.is_some(), ended) {
                Change::Keep => {}
                Change::Ready => {
                    c.status = status::READY;
                    c.error = Atom::default();
                }
                Change::Error(why) => {
                    c.status = status::ERROR;
                    c.error = Atom::new(&format!("{why}\n"));
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOADING: i32 = 2;

    #[test]
    fn create_object_puts_a_component_in_error_for_a_required_property() {
        for s in [status::READY, status::ERROR] {
            assert_eq!(
                after(Asked::Created, s, true, Ended::RequiredNotSet("w")),
                Change::Error("w")
            );
        }
    }

    #[test]
    fn create_object_that_makes_its_object_puts_the_component_back_in_ready() {
        assert_eq!(
            after(Asked::Created, status::ERROR, true, Ended::Made),
            Change::Ready
        );
        assert_eq!(
            after(Asked::Created, status::READY, true, Ended::Made),
            Change::Keep
        );
        // Nothing to make from: whatever it is in stays.
        assert_eq!(
            after(Asked::Created, status::ERROR, false, Ended::Made),
            Change::Keep
        );
        assert_eq!(
            after(Asked::Created, LOADING, true, Ended::Made),
            Change::Keep
        );
    }

    #[test]
    fn any_other_failure_leaves_the_component_as_it_is() {
        for s in [status::READY, status::ERROR] {
            assert_eq!(after(Asked::Created, s, true, Ended::Failed), Change::Keep);
        }
    }

    #[test]
    fn incubate_object_leaves_the_component_as_it_is() {
        for ended in [Ended::Made, Ended::RequiredNotSet("w"), Ended::Failed] {
            for s in [status::READY, status::ERROR] {
                assert_eq!(after(Asked::Incubated, s, true, ended), Change::Keep);
            }
        }
    }

    #[test]
    fn only_incubate_object_is_refused_by_a_component_in_error() {
        assert!(refuses(Asked::Incubated, status::ERROR));
        assert!(!refuses(Asked::Incubated, status::READY));
        assert!(!refuses(Asked::Created, status::ERROR));
    }

    #[test]
    fn a_required_property_is_said_a_line_each() {
        let e = EngineError::RequiredNotSet("a.qml:3:3: a\na.qml:4:3: b".into());
        assert_eq!(e.said(), ["a.qml:3:3: a", "a.qml:4:3: b"]);
        assert_eq!(EngineError::Load("no\nway".into()).said(), ["no\nway"]);
    }
}
