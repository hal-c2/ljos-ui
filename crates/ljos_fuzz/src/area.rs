//! What a fuzzed area provides. Bindings and JavaScript are the first;
//! geometry and layouts, controls and input, and rendered pictures can come
//! later as areas of their own, sharing the sides, the comparison, the
//! shrinker and the buckets.

use crate::{
    compare::{Diff, DiffKind},
    outcome::Rendered,
    rng::Rng,
};

pub trait Area: Sync {
    /// A generated program in the area's own terms, small enough to clone.
    type Case: Clone + Send + Sync + PartialEq;

    fn name(&self) -> &'static str;
    /// A case Qt loads: the generator only makes programs `qml6` accepts.
    fn generate(&self, rng: &mut Rng) -> Self::Case;
    /// The file both sides run, with its observation functions.
    fn render(&self, case: &Self::Case) -> Rendered;
    /// Smaller cases this one can become, those taking away most first.
    fn reductions(&self, case: &Self::Case) -> Vec<Self::Case>;
    /// The value type and the construct a difference is about: with its
    /// kind, the bucket's signature. Neither may hold whitespace. The
    /// construct names a class of difference, not an expression, and ends
    /// in the class of the values (`load:nan-ljos`) where there are values.
    fn describe(&self, case: &Self::Case, diff: &Diff) -> (String, String);
    /// What the program writes where the difference is, for the person
    /// triaging the bucket: no part of the signature.
    fn detail(&self, _case: &Self::Case, _diff: &Diff) -> String {
        String::new()
    }
    /// The case as a person reads it, without the observation helpers.
    fn program(&self, case: &Self::Case) -> String;
    /// The steps run after the load, as JavaScript.
    fn steps(&self, case: &Self::Case) -> Vec<String>;
    /// How big the case is, in the area's words.
    fn summary(&self, case: &Self::Case) -> String;
    /// A draft `@qt-parity` scenario showing the difference.
    fn scenario(&self, case: &Self::Case, diff: &Diff, signature: &str) -> String;
}

/// The bucket a difference falls in. Stable across runs and seeds: it names
/// what the program does, never a generated name or value.
pub fn signature<A: Area>(area: &A, case: &A::Case, diff: &Diff) -> String {
    let (ty, construct) = area.describe(case, diff);
    format!("{}:{ty}:{construct}", diff.kind.label()).replace(char::is_whitespace, "_")
}

/// The part of a signature that must hold while a case shrinks: the kind of
/// difference and, for a value, how the values differ. Where in the program
/// it shows may change on the way down: an unshrunk case nearly always has
/// a change handler, which makes it `load-handlers`, and the shrunk one is
/// then `load` if the handler was no part of it.
pub fn coarse<A: Area>(area: &A, case: &A::Case, diff: &Diff) -> String {
    let (ty, construct) = area.describe(case, diff);
    match diff.kind {
        DiffKind::Value | DiffKind::Missing => format!("{}:{construct}", diff.kind.label()),
        _ => match construct.rsplit_once(':') {
            Some((_, class)) => format!("{}:{ty}:{class}", diff.kind.label()),
            None => format!("{}:{ty}", diff.kind.label()),
        },
    }
}
