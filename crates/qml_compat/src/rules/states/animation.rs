//! An animation names the members it moves as strings: `property:
//! "color"`, `properties: "x,color"` (docs/DIALECT.md R7). The animation
//! looks each name up on the object it moves, by the name that object has,
//! so where Ljós renames the member (`color` is a `Rectangle`'s `fill`) the
//! string is renamed with it. What it moves is its `target` and `targets`;
//! without them, the object a `Behavior` or a value source is on, or in a
//! state's `Transition` what the states change.

use std::ops::Range;

use oxc_ast::ast::{ArrayExpressionElement, Expression};
use qml_edit::Change;
use qml_syntax::ast::{QmlBindingValue, QmlMember, QmlObject};

use crate::rules::{File, Rule};
use crate::{Animated, Kept, Rename, Ty, is_renamed_member};

/// Where the pass is, as far as what an animation moves goes.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum Animating {
    /// In no `Behavior`, value source or `Transition`.
    #[default]
    None,
    /// In a `Behavior` or a value source on an object of this type.
    On(Ty),
    /// In a `Transition` of an object's `transitions`: what the states
    /// change.
    Transitions,
    /// In a `Transition` given to something else (a view's `add`), whose
    /// objects the file does not say.
    Elsewhere,
}

/// The rule of what an animation moves by name.
pub(crate) struct AnimationRule;

pub(crate) const ANIMATION: AnimationRule = AnimationRule;

impl Rule for AnimationRule {
    fn owns(&self) -> &'static [&'static str] {
        // What has `property` and `properties`; `NumberAnimation`,
        // `ColorAnimation` and the rest are built on the first.
        &["PropertyAnimation", "PropertyAction"]
    }

    fn object(
        &self,
        object: &QmlObject<'_>,
        _on: Ty,
        _a_root: bool,
        file: &dyn File,
    ) -> Result<Vec<Change>, Kept> {
        let mut changes = Vec::new();
        let named = names(object, file.source())?;
        for (at, name) in named.iter().filter(|(_, n)| is_renamed_member(n)) {
            let no = |cause: Animated| Kept {
                cause: cause.into(),
                at: at.start,
            };
            let unknown = || no(Animated::Unknown { name: name.clone() });
            let moved = match targets(object, file) {
                Some(targets) => targets,
                None => match file.animating() {
                    Animating::On(ty) => vec![ty],
                    Animating::Transitions => file.changed(name),
                    Animating::None | Animating::Elsewhere => Vec::new(),
                },
            };
            if moved.is_empty() {
                return Err(unknown());
            }
            let mut called: Option<String> = None;
            for ty in moved {
                let new = match file.member_of(name, ty) {
                    Rename::To(new) => new,
                    Rename::Keep => name.clone(),
                    Rename::No(cause) => {
                        return Err(Kept {
                            cause,
                            at: at.start,
                        });
                    }
                    Rename::Unknown => return Err(unknown()),
                };
                match &called {
                    Some(was) if *was != new => {
                        let new = if *was == *name { new } else { was.clone() };
                        return Err(no(Animated::Split {
                            name: name.clone(),
                            new,
                        }));
                    }
                    _ => called = Some(new),
                }
            }
            if let Some(new) = called.filter(|new| new != name) {
                changes.push(Change {
                    range: at.clone(),
                    replacement: new,
                });
            }
        }
        Ok(changes)
    }
}

/// Each name `property` and `properties` hold, where it is in the source.
fn names(object: &QmlObject<'_>, source: &str) -> Result<Vec<(Range<usize>, String)>, Kept> {
    let mut out = Vec::new();
    for member in &object.members {
        let QmlMember::Binding(binding) = member else {
            continue;
        };
        let Some(which @ ("property" | "properties")) = binding.name.as_simple() else {
            continue;
        };
        let QmlBindingValue::Expression(Expression::StringLiteral(text)) = &binding.value else {
            return Err(Kept {
                cause: Animated::Made {
                    binding: which.to_owned(),
                }
                .into(),
                at: binding.name.span.start as usize,
            });
        };
        // Between the quotes, as written: a name has no escapes.
        let start = text.span.start as usize + 1;
        let raw = &source[start..text.span.end as usize - 1];
        let mut offset = start;
        for part in raw.split(',') {
            let name = part.trim();
            if !name.is_empty() {
                let lead = part.len() - part.trim_start().len();
                out.push((offset + lead..offset + lead + name.len(), name.to_owned()));
            }
            offset += part.len() + 1;
        }
    }
    Ok(out)
}

/// What the animation's `target` and `targets` say it moves; `None` when
/// it has neither.
fn targets(object: &QmlObject<'_>, file: &dyn File) -> Option<Vec<Ty>> {
    let of = |id: &str| file.id(id).unwrap_or(Ty::Unknown);
    let mut found: Option<Vec<Ty>> = None;
    for member in &object.members {
        let QmlMember::Binding(binding) = member else {
            continue;
        };
        let QmlBindingValue::Expression(value) = &binding.value else {
            continue;
        };
        let these = match (binding.name.as_simple(), value) {
            (Some("target"), Expression::Identifier(id)) => vec![of(&id.name)],
            (Some("targets"), Expression::ArrayExpression(list)) => list
                .elements
                .iter()
                .map(|element| match element {
                    ArrayExpressionElement::Identifier(id) => of(&id.name),
                    _ => Ty::Unknown,
                })
                .collect(),
            (Some("targets"), Expression::Identifier(id)) => vec![of(&id.name)],
            (Some("target" | "targets"), _) => vec![Ty::Unknown],
            _ => continue,
        };
        found.get_or_insert_with(Vec::new).extend(these);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    // The names the rule reads are what Ljos.Ui's animations have, so a
    // file in Ljós's API says them as a lowered one does.
    #[test]
    fn what_it_reads_are_properties_of_the_ljos_animations() {
        let ui = qt_types::module(crate::MODULE).expect("Ljos.Ui is described");
        for ty in ANIMATION.owns() {
            let ty = ui.type_named(ty).expect(ty);
            for name in ["property", "properties", "target", "targets"] {
                assert!(ty.property(name).is_some(), "{}.{name}", ty.name);
            }
        }
    }
}
