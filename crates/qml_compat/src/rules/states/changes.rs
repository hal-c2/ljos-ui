//! A `PropertyChanges`'s members are its target's: `target: box; width: 30`
//! sets `box.width`. Ljós's API names the object in the change instead
//! (DIALECT §4.9), so a target that is an id is written into each member:
//! `box.width: 30`.

use oxc_ast::ast::Expression;
use qml_syntax::ast::{QmlBinding, QmlBindingValue, QmlMember, QmlObject};

use crate::rules::{Aim, Aimed, File, Rule, Through};
use crate::{Ty, member_extent};

/// The rule of Qt's `PropertyChanges`.
pub(crate) struct ChangesRule;

pub(crate) const CHANGES: ChangesRule = ChangesRule;

/// The names of a `PropertyChanges`'s own; every other is its target's.
const OWN: &[&str] = &["target", "explicit", "restoreEntryValues"];

impl Rule for ChangesRule {
    fn owns(&self) -> &'static [&'static str] {
        &["PropertyChanges"]
    }

    fn aim(&self, object: &QmlObject<'_>, on: Ty, file: &dyn File) -> Option<Aim> {
        if !matches!(on, Ty::Exact(t) if self.owns().contains(&t.name)) {
            return None;
        }
        let target = target(object);
        Some(Aim {
            at: target.and_then(|id| file.id(id)).unwrap_or(Ty::Unknown),
            aimed: Aimed::All {
                own: OWN,
                what: "the target of a `PropertyChanges`",
            },
            through: target.and_then(|id| through(object, id, file)),
        })
    }
}

/// The id a `target` binding of `object` names, if it is one.
fn target<'a>(object: &'a QmlObject<'_>) -> Option<&'a str> {
    match &target_binding(object)?.value {
        QmlBindingValue::Expression(Expression::Identifier(id)) => Some(id.name.as_str()),
        _ => None,
    }
}

fn target_binding<'a, 'b>(object: &'a QmlObject<'b>) -> Option<&'a QmlBinding<'b>> {
    object.members.iter().find_map(|member| match member {
        QmlMember::Binding(binding) if binding.name.as_simple() == Some("target") => Some(binding),
        _ => None,
    })
}

/// The members written through `id`, the target, with the target taken
/// out; none when `id` is not one of the file's.
///
/// DECISION: a target that is not an id (a property, `parent`, a path)
/// stays a target. The id form has no spelling for it, and Ljos.Ui's
/// `PropertyChanges` still has `target`, so the file is lowered as before
/// rather than kept.
fn through(object: &QmlObject<'_>, id: &str, file: &dyn File) -> Option<Through> {
    file.id(id)?;
    let binding = target_binding(object)?;
    Some(Through {
        id: id.to_owned(),
        taken_out: member_extent(
            file.source(),
            binding.span.start as usize,
            binding.span.end as usize,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // A name the rule leaves to the `PropertyChanges` is written as it is,
    // so it must be one Ljos.Ui's `PropertyChanges` has.
    #[test]
    fn its_own_names_are_properties_of_the_ljos_property_changes() {
        let ui = qt_types::module(crate::MODULE).expect("Ljos.Ui is described");
        let changes = ui.type_named("PropertyChanges").expect("a PropertyChanges");
        for name in OWN {
            assert!(changes.property(name).is_some(), "`{name}`");
        }
    }
}
